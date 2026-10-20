// acu_tap: in-process, game-thread observation/modification for Assassin's Creed Unity.
// Inline detours (MinHook) through a generic thunk; snapshots are taken on the calling
// thread so they are atomic with respect to that thread. Never patches files on disk.
#include <windows.h>
#include <atomic>
#include <cstdint>
#include <cstddef>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#include <sstream>
#include <cstdarg>
#include "MinHook.h"

typedef uint64_t u64; typedef uint32_t u32; typedef uint16_t u16; typedef uint8_t u8;

struct Frame { u64 rcx, rdx, r8, r9; u8 xmm[4][16]; u64 entry_rsp_ptr; u64 id; u64 rax; u64 skip; u8 ret_xmm0[16]; };
static_assert(offsetof(Frame, entry_rsp_ptr) == 0x60 && offsetof(Frame, id) == 0x68 && offsetof(Frame, rax) == 0x70 &&
              offsetof(Frame, skip) == 0x78 && offsetof(Frame, ret_xmm0) == 0x80, "thunk layout");

extern "C" void TapThunk();
extern "C" void* TapTrampolines[64];
void* TapTrampolines[64];

// ---- snapshot expression: start + ops ----
struct Op { char kind; int64_t k; };            // '+': add k ; '*': deref 8 bytes
struct SnapSpec { std::atomic<int> live{0}; int kind = 0;   /* 0 = raw bytes, 1 = controller graph walk */ u32 hook = 0; u8 phase = 0; u16 snap_id = 0; int start = 0; std::vector<Op> ops; u32 len = 0; };
static SnapSpec specs[256];
static std::atomic<int> spec_count{0};

static HMODULE self_module = nullptr; static PVOID veh_handle = nullptr; static volatile bool unload_requested = false; static volatile bool stop_rescan = false;
struct HookInfo { std::atomic<int> active{0}; u64 va = 0; std::string label; std::atomic<u64> entries{0}, exits{0}, skipped{0}; std::atomic<int> log{1}; };
static HookInfo hooks[64];

// ---- event buffer (bump allocator, no wrap) ----
static u8* buf = nullptr; static u64 buf_size = 0; static std::atomic<u64> buf_used{0}; static std::atomic<u64> seq{0}; static std::atomic<u64> dropped{0};
static u64 qpc_freq = 0;
#pragma pack(push, 1)
struct EvHdr { u32 magic; u32 size; u64 seq; u64 qpc; u32 tid; u16 hook; u8 phase; u8 nsnap; u64 a[4]; u64 rsp; u64 ret; u64 rax; };
struct SnapHdr { u16 snap_id; u16 ok; u32 len; };
#pragma pack(pop)

static bool safe_copy(void* dst, const void* src, size_t n) {
    __try { memcpy(dst, src, n); return true; } __except (EXCEPTION_EXECUTE_HANDLER) { return false; }
}
static bool plausible(u64 p) { return p > 0x10000 && p < 0x7FFFFFFFFFFFull; }


// ---- in-thread controller graph walk (mirrors the offline reader: list at ctrl+0x6a8, 24-byte entries, playback objects) ----
static const u32 GRAPH_MAX_LEAVES = 32, GRAPH_LEAF_BYTES = 1152, GRAPH_BLOB = 16 + GRAPH_MAX_LEAVES * GRAPH_LEAF_BYTES;
// blob: u32 magic 'GRPH', u32 node_count, u32 leaf_count, u32 flags; leaf: u32 index,u32 ok,f32 weight,u32 slot,u64 state,u64 node,u64 playback,u64 asset,u64 vtable,u64 method58, 24B handle, 1024B playback_raw, pad
static bool capture_graph(u64 ctrl, u8* dst) {
    memset(dst, 0, GRAPH_BLOB); *(u32*)dst = 0x48505247;
    if (!plausible(ctrl)) return false;
    u8 hdr[16]; if (!safe_copy(hdr, (void*)(ctrl + 0x6a8), 16)) return false;
    u64 arr = *(u64*)hdr; u32 cnt = *(u16*)(hdr + 10); *(u32*)(dst + 4) = cnt;
    if (!plausible(arr) || cnt == 0 || cnt > 512) return false;
    u32 leaves = 0; static thread_local u8 raw[512 * 24];
    if (!safe_copy(raw, (void*)arr, cnt * 24)) return false;
    for (u32 i = 0; i < cnt && leaves < GRAPH_MAX_LEAVES; i++) {
        u8* e = raw + i * 24; u64 state = *(u64*)e, node = *(u64*)(e + 8); float w = *(float*)(e + 16); u8 kind = e[22];
        if (kind != 0) continue;
        u8* L = dst + 16 + leaves * GRAPH_LEAF_BYTES; memset(L, 0, GRAPH_LEAF_BYTES); *(u32*)L = i; *(float*)(L + 8) = w; *(u64*)(L + 16) = state; *(u64*)(L + 24) = node;
        u8 nb[104]; u64 vt = 0, method = 0;
        if (plausible(node) && safe_copy(nb, (void*)node, 104)) {
            vt = *(u64*)nb; *(u64*)(L + 64) = vt; u32 slot = *(u16*)(nb + 100); *(u32*)(L + 12) = slot;
            if (vt >= 0x140000000ull && vt < 0x148000000ull && safe_copy(&method, (void*)(vt + 88), 8)) {
                *(u64*)(L + 72) = method;
                u64 pa = 0, pb = 0, asset = 0;
                if (method == 0x1400bcc20ull && slot < 4096 && plausible(state) && safe_copy(&pa, (void*)(state + 608), 8) && plausible(pa) && safe_copy(&pb, (void*)(pa + 8 * slot), 8) && plausible(pb)) {
                    *(u64*)(L + 32) = pb;
                    if (safe_copy(L + 128, (void*)pb, 1024)) {
                        asset = *(u64*)(L + 128 + 40); *(u64*)(L + 40) = asset;
                        if (plausible(asset) && safe_copy(L + 80, (void*)asset, 24)) *(u32*)(L + 4) = 1;
                    }
                }
            }
        }
        leaves++;
    }
    *(u32*)(dst + 8) = leaves; return true;
}

// ---- in-thread modifier list walk at the frame evaluator 0x1400377e0 (expression = pose component) ----
// Mirrors ACU-RE-0028: level = (u16 [comp+232] >> 3) & 7, list = comp + 720 + 12*level (ptr +0, u16 count +10);
// run channels = local pose object (comp+640) +376 block through the +368 descriptor (0x140093080).
static const u32 MODS_MAX = 512, MODS_CHAN = 512, MODS_ENTRY = 544, MODS_BLOB = 32 + MODS_CHAN + MODS_MAX * MODS_ENTRY;
// blob: u32 magic 'MODS', u32 level, u32 list_count, u32 captured, u32 chan_ok, u32 pad, u64 chan_addr, u8 chan[512];
// entry: u64 obj, u32 ok bits (0 obj, 1..4 targets, 5 spheres, 6 base), u32 pad, obj[0xc0],
//        targets 4 x 16 (*(*(obj+0x28)+8k)), spheres 192 (*(obj+0x70)), base 80 (*(obj+8))
static bool deref(u64 a, u64* v) { return plausible(a) && safe_copy(v, (void*)a, 8); }
static bool capture_mods(u64 comp, u8* dst) {
    memset(dst, 0, MODS_BLOB); *(u32*)dst = 0x53444F4D;
    u16 w = 0; if (!plausible(comp) || !safe_copy(&w, (void*)(comp + 232), 2)) return false;
    u32 level = (w >> 3) & 7; *(u32*)(dst + 4) = level;
    u64 L = 0, d = 0, block = 0;
    if (deref(comp + 640, &L) && deref(L + 368, &d) && deref(L + 376, &block)) {
        u16 v2 = 0; u64 chan = 0; bool ok = safe_copy(&v2, (void*)(d + 40), 2);
        if (ok && v2 == 0xFFFF) chan = block;
        else if (ok) {
            u64 t80 = 0, t128 = 0, t8 = 0; u16 i1 = 0, i2 = 0, off = 0;
            ok = deref(d + 80, &t80) && deref(d + 128, &t128) && deref(d + 8, &t8) && plausible(t80) && safe_copy(&i1, (void*)(t80 + 2ull * v2), 2) &&
                 plausible(t128) && safe_copy(&i2, (void*)(t128 + 2ull * i1), 2) && plausible(t8) && safe_copy(&off, (void*)(t8 + 8ull * i2 + 4), 2);
            if (ok) chan = block + off;
        }
        *(u64*)(dst + 24) = chan;
        if (ok && plausible(chan) && safe_copy(dst + 32, (void*)chan, MODS_CHAN)) *(u32*)(dst + 16) = 1;
    }
    if (level >= 6) return true;
    u8 lh[12]; if (!safe_copy(lh, (void*)(comp + 720 + 12ull * level), 12)) return true;
    u64 arr = *(u64*)lh; u32 cnt = *(u16*)(lh + 10); *(u32*)(dst + 8) = cnt;
    u32 n = cnt < MODS_MAX ? cnt : MODS_MAX; if (!plausible(arr)) return true;
    for (u32 i = 0; i < n; i++) {
        u8* E = dst + 32 + MODS_CHAN + i * MODS_ENTRY; u64 obj = 0; if (!deref(arr + 8ull * i, &obj)) continue;
        *(u64*)E = obj; u32 bits = 0;
        if (plausible(obj) && safe_copy(E + 16, (void*)obj, 0xc0)) bits |= 1;
        u64 tarr = 0, t = 0, sp = 0, base = 0;
        if (deref(obj + 0x28, &tarr)) for (int k = 0; k < 4; k++) if (deref(tarr + 8ull * k, &t) && plausible(t) && safe_copy(E + 208 + 16 * k, (void*)t, 16)) bits |= 2u << k;
        if (deref(obj + 0x70, &sp) && plausible(sp) && safe_copy(E + 272, (void*)sp, 192)) bits |= 32;
        if (deref(obj + 8, &base) && plausible(base) && safe_copy(E + 464, (void*)base, 80)) bits |= 64;
        *(u32*)(E + 8) = bits;
    }
    *(u32*)(dst + 12) = n; return true;
}

static bool eval_expr(const SnapSpec& s, Frame* f, u64* out) {
    u64 v = 0;
    switch (s.start) { case 0: v = f->rcx; break; case 1: v = f->rdx; break; case 2: v = f->r8; break; case 3: v = f->r9; break;
                       case 4: v = f->rax; break; case 5: v = f->entry_rsp_ptr; break;
                       case 6: case 7: case 8: case 9: v = (u64)&f->xmm[s.start - 6][0]; break;
                       default: return false; }
    for (const Op& o : s.ops) {
        if (o.kind == '+') v += (u64)o.k;
        else { if (!plausible(v)) return false; u64 t; if (!safe_copy(&t, (void*)v, 8)) return false; v = t; }
    }
    *out = v; return true;
}

struct Pending { const SnapSpec* s; u64 addr; bool ok; };

static void record(u32 id, u8 phase, Frame* f) {
    HookInfo& h = hooks[id];
    if (!h.log.load(std::memory_order_relaxed)) return;
    u64 need = sizeof(EvHdr);
    int n = spec_count.load(std::memory_order_acquire);
    Pending pend[64]; int np = 0;
    for (int i = 0; i < n && np < 64; i++) {
        const SnapSpec& s = specs[i];
        if (!s.live.load(std::memory_order_acquire) || s.hook != id || s.phase != phase) continue;
        u64 addr = 0; bool ok = eval_expr(s, f, &addr) && plausible(addr);
        pend[np++] = {&s, addr, ok}; need += sizeof(SnapHdr) + ((s.len + 7) & ~7u);
    }
    u64 off = buf_used.fetch_add(need);
    if (off + need > buf_size) { dropped++; return; }
    u8* p = buf + off; EvHdr* e = (EvHdr*)p;
    LARGE_INTEGER q; QueryPerformanceCounter(&q);
    e->magic = 0x45544341; e->size = (u32)need; e->seq = seq++; e->qpc = q.QuadPart; e->tid = GetCurrentThreadId(); e->hook = (u16)id; e->phase = phase; e->nsnap = (u8)np;
    e->a[0] = f->rcx; e->a[1] = f->rdx; e->a[2] = f->r8; e->a[3] = f->r9; e->rsp = f->entry_rsp_ptr;
    if (!safe_copy(&e->ret, (void*)f->entry_rsp_ptr, 8)) e->ret = 0;
    e->rax = f->rax;
    u8* w = p + sizeof(EvHdr);
    for (int i = 0; i < np; i++) {
        SnapHdr* sh = (SnapHdr*)w; sh->snap_id = pend[i].s->snap_id; sh->len = pend[i].s->len;
        if (pend[i].s->kind == 1) sh->ok = (pend[i].ok && capture_graph(pend[i].addr, w + sizeof(SnapHdr))) ? 1 : 0;
        else if (pend[i].s->kind == 2) sh->ok = (pend[i].ok && capture_mods(pend[i].addr, w + sizeof(SnapHdr))) ? 1 : 0;
        else sh->ok = (pend[i].ok && safe_copy(w + sizeof(SnapHdr), (void*)pend[i].addr, pend[i].s->len)) ? 1 : 0;
        if (!sh->ok) memset(w + sizeof(SnapHdr), 0, pend[i].s->len);
        w += sizeof(SnapHdr) + ((pend[i].s->len + 7) & ~7u);
    }
}

extern "C" void TapOnEnter(u64 id, Frame* f) {
    memset(f->ret_xmm0, 0, 16); hooks[id].entries++; record((u32)id, 0, f);
}
extern "C" void TapOnExit(u64 id, Frame* f) {
    hooks[id].exits++; record((u32)id, 1, f);
}

// ---- write tracing via page protection ----
struct WWatch { u64 addr, len; };
static WWatch wwatch[8]; static int wwatch_n = 0;
struct WHit { u64 qpc, rip, addr, rsp; u32 tid; u32 len; u64 stack[40]; u8 bytes[16]; };
static WHit* whits = nullptr; static std::atomic<u64> whit_n{0}; static const u64 WHIT_MAX = 400000;
static std::atomic<int> wactive{0}; static std::atomic<int> wstopping{0};   // wstopping: faults only unprotect, never re-arm
static thread_local u64 rearm_page = 0;
static DWORD page_orig_prot[8]; static u64 wpages[8]; static int wpage_n = 0;

static u64 ever_pages[64]; static std::atomic<int> ever_n{0};   // every page ever watched stays claimed: late faults must never fall through to the game
static LONG CALLBACK veh(EXCEPTION_POINTERS* x) {
    EXCEPTION_RECORD* r = x->ExceptionRecord;
    if (r->ExceptionCode == EXCEPTION_SINGLE_STEP && rearm_page) {   // our own trap flag: always honour, whatever the state
        if (!wstopping.load() && wactive.load()) { DWORD old; VirtualProtect((void*)rearm_page, 0x1000, PAGE_READONLY, &old); }
        rearm_page = 0; return EXCEPTION_CONTINUE_EXECUTION;
    }
    if (r->ExceptionCode == EXCEPTION_ACCESS_VIOLATION && r->NumberParameters >= 2 && r->ExceptionInformation[0] == 1) {
        u64 a = r->ExceptionInformation[1], page = a & ~0xFFFull; bool mine = false; int en = ever_n.load();
        for (int i = 0; i < en; i++) if (ever_pages[i] == page) mine = true;
        if (!mine) return EXCEPTION_CONTINUE_SEARCH;
        bool recording = wactive.load() && !wstopping.load();
        if (recording) {
            bool inrange = false; u32 wl = 0;
            for (int i = 0; i < wwatch_n; i++) if (a + 32 > wwatch[i].addr && a < wwatch[i].addr + wwatch[i].len) { inrange = true; wl = (u32)wwatch[i].len; }
            if (inrange) {
                u64 k = whit_n.fetch_add(1);
                if (k < WHIT_MAX) {
                    WHit& h = whits[k]; LARGE_INTEGER q; QueryPerformanceCounter(&q); h.qpc = q.QuadPart; h.rip = x->ContextRecord->Rip; h.addr = a;
                    h.rsp = x->ContextRecord->Rsp; h.tid = GetCurrentThreadId(); h.len = wl;
                    if (!safe_copy(h.stack, (void*)x->ContextRecord->Rsp, sizeof(h.stack))) memset(h.stack, 0, sizeof(h.stack));
                    if (!safe_copy(h.bytes, (void*)(a & ~7ull), 16)) memset(h.bytes, 0, 16);
                }
            }
        }
        DWORD old; VirtualProtect((void*)page, 0x1000, PAGE_READWRITE, &old);
        if (!recording) return EXCEPTION_CONTINUE_EXECUTION;       // stale protection: just release it
        rearm_page = page; x->ContextRecord->EFlags |= 0x100; return EXCEPTION_CONTINUE_EXECUTION;
    }
    return EXCEPTION_CONTINUE_SEARCH;
}

// ---- forensics: log fatal-ish exceptions and who terminates the process ----
static HANDLE forensic_file = INVALID_HANDLE_VALUE; static std::atomic<int> forensic_lines{0};
static void flog(const char* fmt, ...) {
    if (forensic_file == INVALID_HANDLE_VALUE || forensic_lines.fetch_add(1) > 4000) return;
    char b[1024]; va_list ap; va_start(ap, fmt); int n = vsnprintf(b, sizeof b - 1, fmt, ap); va_end(ap); if (n <= 0) return;
    b[n++] = '\n'; DWORD w; WriteFile(forensic_file, b, n, &w, nullptr);
}
static std::string where(u64 a) {
    HMODULE m = nullptr; char buf[300] = "?";
    if (GetModuleHandleExA(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, (LPCSTR)a, &m) && m) {
        char name[MAX_PATH]; GetModuleFileNameA(m, name, MAX_PATH); const char* base = strrchr(name, '\\'); snprintf(buf, sizeof buf, "%s+0x%llx", base ? base + 1 : name, (unsigned long long)(a - (u64)m)); }
    else snprintf(buf, sizeof buf, "0x%llx(unmapped)", (unsigned long long)a);
    return buf;
}
static void log_stack(const char* why) {
    void* fr[24]; USHORT n = RtlCaptureStackBackTrace(0, 24, fr, nullptr); flog("%s tid=%lu frames=%u", why, GetCurrentThreadId(), n);
    for (USHORT i = 0; i < n; i++) flog("  #%u %s", i, where((u64)fr[i]).c_str());
}
static LONG CALLBACK forensic_veh(EXCEPTION_POINTERS* x) {
    DWORD c = x->ExceptionRecord->ExceptionCode;
    if (c == EXCEPTION_BREAKPOINT || c == EXCEPTION_SINGLE_STEP || c == 0x406D1388 || c == 0xE06D7363 || c == 0x40010006 || c == 0x4001000A || c == 0x40020009 || c == 0xE0434352 || c == 0xE0434F4D) return EXCEPTION_CONTINUE_SEARCH;
    if (c == EXCEPTION_ACCESS_VIOLATION && (wactive.load() || wstopping.load()) && x->ExceptionRecord->NumberParameters >= 2 && x->ExceptionRecord->ExceptionInformation[0] == 1) return EXCEPTION_CONTINUE_SEARCH;
    CONTEXT* r = x->ContextRecord;
    flog("EXC code=0x%lx rip=%s addr=0x%llx info0=%llu info1=0x%llx rsp=0x%llx tid=%lu", c, where(r->Rip).c_str(), (unsigned long long)x->ExceptionRecord->ExceptionAddress,
         (unsigned long long)(x->ExceptionRecord->NumberParameters > 0 ? x->ExceptionRecord->ExceptionInformation[0] : 0), (unsigned long long)(x->ExceptionRecord->NumberParameters > 1 ? x->ExceptionRecord->ExceptionInformation[1] : 0), (unsigned long long)r->Rsp, GetCurrentThreadId());
    flog("  rax=%llx rcx=%llx rdx=%llx r8=%llx r9=%llx r10=%llx r11=%llx rbp=%llx", (unsigned long long)r->Rax, (unsigned long long)r->Rcx, (unsigned long long)r->Rdx, (unsigned long long)r->R8, (unsigned long long)r->R9, (unsigned long long)r->R10, (unsigned long long)r->R11, (unsigned long long)r->Rbp);
    return EXCEPTION_CONTINUE_SEARCH;
}
typedef VOID (WINAPI* ExitProcess_t)(UINT); typedef BOOL (WINAPI* TerminateProcess_t)(HANDLE, UINT);
static ExitProcess_t real_ExitProcess; static TerminateProcess_t real_TerminateProcess;
static VOID WINAPI det_ExitProcess(UINT c) { log_stack("ExitProcess"); flog("  exit code %u", c); return real_ExitProcess(c); }
static BOOL WINAPI det_TerminateProcess(HANDLE h, UINT c) { if (h == GetCurrentProcess() || GetProcessId(h) == GetCurrentProcessId()) { log_stack("TerminateProcess(self)"); flog("  exit code %u", c); } return real_TerminateProcess(h, c); }
static void start_forensics() {
    char dir[MAX_PATH]; HMODULE m = self_module; GetModuleFileNameA(m, dir, MAX_PATH); *strrchr(dir, '\\') = 0;
    char p[MAX_PATH]; snprintf(p, sizeof p, "%s\\tap-forensics-%lu.log", dir, GetCurrentProcessId());
    forensic_file = CreateFileA(p, GENERIC_WRITE, FILE_SHARE_READ, nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    AddVectoredExceptionHandler(1, forensic_veh);
    MH_CreateHookApi(L"kernel32.dll", "ExitProcess", (LPVOID)det_ExitProcess, (LPVOID*)&real_ExitProcess); MH_EnableHook(MH_ALL_HOOKS);
    MH_CreateHookApi(L"kernel32.dll", "TerminateProcess", (LPVOID)det_TerminateProcess, (LPVOID*)&real_TerminateProcess); MH_EnableHook(MH_ALL_HOOKS);
    flog("forensics started pid=%lu", GetCurrentProcessId());
}


// ---- hardware-breakpoint probes (no code patching): entry traps + one-shot return traps ----
#include <tlhelp32.h>
static u64 dr_va[3]; static std::atomic<int> dr_exit_wanted[3];
struct PendingExit { int hook; u64 a[4]; };
static thread_local PendingExit pending_exit = {-1, {0, 0, 0, 0}};
static u64 dr_last_va[3];            // every address ever armed per slot: stale traps must still be acknowledged
static std::atomic<u64> dr_filter_a0[3]; static std::atomic<int> dr_filter_arg[3];   // which argument register the filter compares (0..3 = rcx, rdx, r8, r9)
static std::atomic<u64> dr_skip_a0[3];   // when non-zero and a0 matches: do not run the function (emulate an immediate return, rax = 0)
static LONG CALLBACK dr_veh(EXCEPTION_POINTERS* x) {
    if (x->ExceptionRecord->ExceptionCode != EXCEPTION_SINGLE_STEP) return EXCEPTION_CONTINUE_SEARCH;
    CONTEXT* c = x->ContextRecord; u64 dr6 = c->Dr6; bool handled = false;
    for (int s = 0; s < 3; s++) {
        if (!(dr6 & (1ull << s)) || !dr_last_va[s] || c->Rip != dr_last_va[s]) continue;
        handled = true;
        if (!dr_va[s] || dr_va[s] != dr_last_va[s]) { c->Dr7 &= ~(3ull << (s * 2)); continue; }    // stale trap after disarm: acknowledge only
        u64 flt = dr_filter_a0[s].load(); u64 skp = dr_skip_a0[s].load(); int fa = dr_filter_arg[s].load(); u64 fv = fa == 1 ? c->Rdx : fa == 2 ? c->R8 : fa == 3 ? c->R9 : c->Rcx;
        if (flt && fv != flt && !(skp && c->Rcx == skp)) continue;
        Frame f; memset(&f, 0, sizeof f); f.rcx = c->Rcx; f.rdx = c->Rdx; f.r8 = c->R8; f.r9 = c->R9; f.entry_rsp_ptr = c->Rsp; f.id = s;
        for (int i = 0; i < 4; i++) memcpy(f.xmm[i], &c->FltSave.XmmRegisters[i], 16);
        hooks[s].entries++; record((u32)s, 0, &f);
        if (skp && c->Rcx == skp) { u64 ret = 0; if (safe_copy(&ret, (void*)c->Rsp, 8) && plausible(ret)) { c->Rip = ret; c->Rsp += 8; c->Rax = 0; hooks[s].skipped++; } continue; }
        if (dr_exit_wanted[s].load()) {
            u64 ret = 0; if (safe_copy(&ret, (void*)c->Rsp, 8) && plausible(ret)) {
                c->Dr3 = ret; c->Dr7 |= (1ull << 6); c->Dr7 &= ~(0xFull << 28); pending_exit.hook = s; pending_exit.a[0] = f.rcx; pending_exit.a[1] = f.rdx; pending_exit.a[2] = f.r8; pending_exit.a[3] = f.r9; }
        }
    }
    if (dr6 & 8) {
        if (pending_exit.hook >= 0 && c->Rip == c->Dr3) {
            Frame f; memset(&f, 0, sizeof f); f.rcx = pending_exit.a[0]; f.rdx = pending_exit.a[1]; f.r8 = pending_exit.a[2]; f.r9 = pending_exit.a[3];
            f.rax = c->Rax; f.id = pending_exit.hook; f.entry_rsp_ptr = c->Rsp - 8;
            for (int i = 0; i < 4; i++) memcpy(f.xmm[i], &c->FltSave.XmmRegisters[i], 16);
            if (hooks[pending_exit.hook].active.load()) { hooks[pending_exit.hook].exits++; record((u32)pending_exit.hook, 1, &f); }
        }
        c->Dr7 &= ~(1ull << 6); c->Dr3 = 0; pending_exit.hook = -1; handled = true;   // one-shot trap, stale or not
    }
    if (!handled) return EXCEPTION_CONTINUE_SEARCH;
    c->Dr6 = 0; c->EFlags |= 0x10000; return EXCEPTION_CONTINUE_EXECUTION;
}
static u32 dr_seen[2048]; static int dr_seen_n = 0;
static void dr_apply_all(bool all) {   // set Dr0..2/Dr7 on every (new) thread; no heap use while threads are suspended
    DWORD tids[2048]; int n = 0; HANDLE s = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0); THREADENTRY32 e{sizeof e}; DWORD me = GetCurrentThreadId(), pid = GetCurrentProcessId();
    if (s == INVALID_HANDLE_VALUE) return;
    if (Thread32First(s, &e)) do { if (e.th32OwnerProcessID == pid && e.th32ThreadID != me && n < 2048) tids[n++] = e.th32ThreadID; } while (Thread32Next(s, &e));
    CloseHandle(s);
    u64 d7bits = 0; for (int i = 0; i < 3; i++) if (dr_va[i]) d7bits |= 1ull << (i * 2);
    for (int i = 0; i < n; i++) {
        bool seen = false; if (!all) for (int k = 0; k < dr_seen_n; k++) if (dr_seen[k] == tids[i]) { seen = true; break; }
        if (seen) continue; if (dr_seen_n < 2048) dr_seen[dr_seen_n++] = tids[i];
        HANDLE h = OpenThread(THREAD_SUSPEND_RESUME | THREAD_GET_CONTEXT | THREAD_SET_CONTEXT, FALSE, tids[i]); if (!h) continue;
        if (SuspendThread(h) != (DWORD)-1) {
            CONTEXT c; memset(&c, 0, sizeof c); c.ContextFlags = CONTEXT_DEBUG_REGISTERS;
            if (GetThreadContext(h, &c)) { c.Dr0 = dr_va[0]; c.Dr1 = dr_va[1]; c.Dr2 = dr_va[2]; c.Dr7 = (c.Dr7 & ~0x3Full) | d7bits; c.Dr6 = 0; SetThreadContext(h, &c); }
            ResumeThread(h);
        }
        CloseHandle(h);
    }
}

// ---- command server ----
static std::string jstr(const std::string& s) { std::string o = "\""; for (char c : s) { if (c == '"' || c == '\\') o += '\\'; if ((u8)c >= 32) o += c; } return o + "\""; }
static std::string hex(u64 v) { char b[32]; snprintf(b, sizeof b, "0x%llx", (unsigned long long)v); return b; }
static bool parse_hex(const std::string& s, u64* v) { char* e; *v = strtoull(s.c_str(), &e, 16); return *e == 0 && !s.empty(); }
static std::vector<std::string> split(const std::string& s) { std::istringstream i(s); std::vector<std::string> o; std::string t; while (i >> t) o.push_back(t); return o; }
static u8 hexv(char c) { return c <= '9' ? c - '0' : (c | 32) - 'a' + 10; }

static u8* image_base() { return (u8*)GetModuleHandleW(nullptr); }
static void* stub_mem = nullptr;

static std::string handle(const std::string& line) {
    auto t = split(line); if (t.empty()) return "{\"ok\":false,\"error\":\"empty\"}";
    const std::string& c = t[0]; std::ostringstream o;
    if (c == "STATUS") {
        o << "{\"ok\":true,\"pid\":" << GetCurrentProcessId() << ",\"image_base\":\"" << hex((u64)image_base()) << "\",\"qpc_frequency\":" << qpc_freq
          << ",\"buffer_used\":" << buf_used.load() << ",\"buffer_size\":" << buf_size << ",\"dropped\":" << dropped.load() << ",\"events\":" << seq.load()
          << ",\"write_hits\":" << whit_n.load() << ",\"specs\":" << spec_count.load() << ",\"hooks\":[";
        bool first = true;
        for (int i = 0; i < 64; i++) if (hooks[i].active) { if (!first) o << ','; first = false;
            o << "{\"id\":" << i << ",\"va\":\"" << hex(hooks[i].va) << "\",\"label\":" << jstr(hooks[i].label) << ",\"entries\":" << hooks[i].entries.load() << ",\"exits\":" << hooks[i].exits.load() << ",\"skipped\":" << hooks[i].skipped.load() << "}"; }
        o << "]}"; return o.str();
    }
    if (c == "READ" && t.size() == 3) {
        u64 a; if (!parse_hex(t[1], &a)) return "{\"ok\":false,\"error\":\"addr\"}"; size_t n = strtoul(t[2].c_str(), nullptr, 10);
        if (n < 1 || n > 65536) return "{\"ok\":false,\"error\":\"len\"}"; std::vector<u8> v(n);
        if (!plausible(a) || !safe_copy(v.data(), (void*)a, n)) return "{\"ok\":false,\"error\":\"unreadable\"}";
        o << "{\"ok\":true,\"hex\":\""; char b[3]; for (u8 x : v) { snprintf(b, 3, "%02x", x); o << b; } o << "\"}"; return o.str();
    }
    if (c == "HOOK" && t.size() >= 5) {   // HOOK id hexRVA expectedhex label
        int id = atoi(t[1].c_str()); u64 rva; if (id < 0 || id >= 64 || !parse_hex(t[2], &rva)) return "{\"ok\":false,\"error\":\"args\"}";
        if (hooks[id].active) return "{\"ok\":false,\"error\":\"id in use\"}";
        u8* target = image_base() + rva; const std::string& ex = t[3];
        if (ex.size() % 2 || ex.empty() || ex.size() > 64) return "{\"ok\":false,\"error\":\"expected\"}";
        u8 cur[32]; size_t n = ex.size() / 2; if (!safe_copy(cur, target, n)) return "{\"ok\":false,\"error\":\"unreadable target\"}";
        for (size_t i = 0; i < n; i++) if (cur[i] != (hexv(ex[2 * i]) << 4 | hexv(ex[2 * i + 1]))) return "{\"ok\":false,\"error\":\"byte guard mismatch\"}";
        if (!stub_mem) stub_mem = VirtualAlloc(nullptr, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE);
        u8* s = (u8*)stub_mem + id * 32; // mov r11d,id ; mov r10,thunk ; jmp r10
        s[0] = 0x41; s[1] = 0xBB; *(u32*)(s + 2) = id; s[6] = 0x49; s[7] = 0xBA; *(u64*)(s + 8) = (u64)&TapThunk; s[16] = 0x41; s[17] = 0xFF; s[18] = 0xE2;
        void* orig = nullptr; MH_STATUS st = MH_CreateHook(target, s, &orig);
        if (st != MH_OK) { o << "{\"ok\":false,\"error\":\"MH_CreateHook " << MH_StatusToString(st) << "\"}"; return o.str(); }
        TapTrampolines[id] = orig; hooks[id].va = (u64)target; hooks[id].label = t[4]; hooks[id].entries = 0; hooks[id].exits = 0;
        st = MH_EnableHook(target); if (st != MH_OK) { o << "{\"ok\":false,\"error\":\"MH_EnableHook " << MH_StatusToString(st) << "\"}"; return o.str(); }
        hooks[id].active = 1; o << "{\"ok\":true,\"id\":" << id << ",\"va\":\"" << hex((u64)target) << "\"}"; return o.str();
    }
    if (c == "DRHOOK" && t.size() >= 5) {   // DRHOOK slot(0-2) hexRVA expectedhex label [exit]
        int sl = atoi(t[1].c_str()); u64 rva; if (sl < 0 || sl > 2 || !parse_hex(t[2], &rva)) return "{\"ok\":false,\"error\":\"args\"}";
        if (hooks[sl].active) return "{\"ok\":false,\"error\":\"slot in use\"}";
        u8* target = image_base() + rva; const std::string& ex = t[3]; u8 cur[32]; size_t n = ex.size() / 2;
        if (ex.size() % 2 || ex.empty() || ex.size() > 64 || !safe_copy(cur, target, n)) return "{\"ok\":false,\"error\":\"expected/unreadable\"}";
        for (size_t i = 0; i < n; i++) if (cur[i] != (hexv(ex[2 * i]) << 4 | hexv(ex[2 * i + 1]))) return "{\"ok\":false,\"error\":\"byte guard mismatch\"}";
        hooks[sl].va = (u64)target; hooks[sl].label = t[4]; hooks[sl].entries = 0; hooks[sl].exits = 0; dr_exit_wanted[sl] = (t.size() > 5 && t[5] == "exit");
        dr_last_va[sl] = (u64)target; dr_va[sl] = (u64)target; hooks[sl].active = 1; dr_apply_all(true); o << "{\"ok\":true,\"slot\":" << sl << ",\"va\":\"" << hex((u64)target) << "\"}"; return o.str();
    }
    if (c == "DRFILTER" && (t.size() == 3 || t.size() == 4)) {   // DRFILTER slot value [arg 0-3]
        int sl = atoi(t[1].c_str()); u64 v = 0; int arg = t.size() == 4 ? atoi(t[3].c_str()) : 0;
        if (sl < 0 || sl > 2 || arg < 0 || arg > 3 || (t[2] != "0" && !parse_hex(t[2], &v))) return "{\"ok\":false}"; dr_filter_arg[sl] = arg; dr_filter_a0[sl] = v; return "{\"ok\":true}"; }
    if (c == "DRSKIP" && t.size() == 3) { int sl = atoi(t[1].c_str()); u64 v = 0; if (sl < 0 || sl > 2 || (t[2] != "0" && !parse_hex(t[2], &v))) return "{\"ok\":false}"; dr_skip_a0[sl] = v; return "{\"ok\":true}"; }
    if (c == "DRUNHOOK" && t.size() == 2) { int sl = atoi(t[1].c_str()); if (sl < 0 || sl > 2) return "{\"ok\":false}"; dr_va[sl] = 0; dr_filter_a0[sl] = 0; dr_skip_a0[sl] = 0; hooks[sl].active = 0; dr_apply_all(true); return "{\"ok\":true}"; }
    if (c == "UNHOOK" && t.size() == 2) {
        int id = atoi(t[1].c_str()); if (id < 0 || id >= 64 || !hooks[id].active) return "{\"ok\":false,\"error\":\"not active\"}";
        MH_DisableHook((void*)hooks[id].va); Sleep(50); MH_RemoveHook((void*)hooks[id].va); hooks[id].active = 0; return "{\"ok\":true}";
    }
    if (c == "SNAP" && t.size() >= 6) {   // SNAP hook entry|exit snap_id len start [+hex|-hex|*]...
        int n = spec_count.load(); if (n >= 256) return "{\"ok\":false,\"error\":\"spec table full\"}";
        SnapSpec& s = specs[n]; s.hook = atoi(t[1].c_str()); s.phase = t[2] == "exit"; s.snap_id = (u16)atoi(t[3].c_str()); s.len = (u32)strtoul(t[4].c_str(), nullptr, 10);
        if (s.hook >= 64 || s.len < 1 || s.len > 262144) return "{\"ok\":false,\"error\":\"args\"}";
        const std::string& st = t[5]; s.start = st == "a0" ? 0 : st == "a1" ? 1 : st == "a2" ? 2 : st == "a3" ? 3 : st == "rax" ? 4 : st == "rsp" ? 5 : st == "xmm0" ? 6 : st == "xmm1" ? 7 : st == "xmm2" ? 8 : st == "xmm3" ? 9 : -1;
        if (s.start >= 6 && (s.len > 16 || t.size() != 6)) return "{\"ok\":false,\"error\":\"xmm snapshot must be <=16 bytes without ops\"}";
        if (s.start < 0) return "{\"ok\":false,\"error\":\"start\"}";
        s.ops.clear();
        for (size_t i = 6; i < t.size(); i++) {
            if (t[i] == "*") s.ops.push_back({'*', 0});
            else { u64 k; bool neg = t[i][0] == '-'; if ((t[i][0] != '+' && !neg) || !parse_hex(t[i].substr(1), &k)) return "{\"ok\":false,\"error\":\"op\"}"; s.ops.push_back({'+', neg ? -(int64_t)k : (int64_t)k}); }
        }
        s.live.store(1, std::memory_order_release); spec_count.store(n + 1, std::memory_order_release); o << "{\"ok\":true,\"spec\":" << n << "}"; return o.str();
    }
    if (c == "SNAPGRAPH" && t.size() >= 5) {   // SNAPGRAPH hook entry|exit snap_id start [ops...]  (expression must evaluate to the controller pointer; the address itself, not bytes at it)
        int n = spec_count.load(); if (n >= 256) return "{\"ok\":false,\"error\":\"spec table full\"}";
        SnapSpec& s = specs[n]; s.kind = 1; s.hook = atoi(t[1].c_str()); s.phase = t[2] == "exit"; s.snap_id = (u16)atoi(t[3].c_str()); s.len = GRAPH_BLOB;
        const std::string& st = t[4]; s.start = st == "a0" ? 0 : st == "a1" ? 1 : st == "a2" ? 2 : st == "a3" ? 3 : st == "rax" ? 4 : -1;
        if (s.hook >= 64 || s.start < 0) return "{\"ok\":false,\"error\":\"args\"}";
        s.ops.clear();
        for (size_t i = 5; i < t.size(); i++) { if (t[i] == "*") s.ops.push_back({'*', 0}); else { u64 k; bool neg = t[i][0] == '-'; if ((t[i][0] != '+' && !neg) || !parse_hex(t[i].substr(1), &k)) return "{\"ok\":false,\"error\":\"op\"}"; s.ops.push_back({'+', neg ? -(int64_t)k : (int64_t)k}); } }
        s.live.store(1, std::memory_order_release); spec_count.store(n + 1, std::memory_order_release); o << "{\"ok\":true,\"spec\":" << n << ",\"blob_bytes\":" << GRAPH_BLOB << "}"; return o.str();
    }
    if (c == "SNAPMODS" && t.size() >= 5) {   // SNAPMODS hook entry|exit snap_id start [ops...]  (expression = pose component address)
        int n = spec_count.load(); if (n >= 256) return "{\"ok\":false,\"error\":\"spec table full\"}";
        SnapSpec& s = specs[n]; s.kind = 2; s.hook = atoi(t[1].c_str()); s.phase = t[2] == "exit"; s.snap_id = (u16)atoi(t[3].c_str()); s.len = MODS_BLOB;
        const std::string& st = t[4]; s.start = st == "a0" ? 0 : st == "a1" ? 1 : st == "a2" ? 2 : st == "a3" ? 3 : st == "rax" ? 4 : -1;
        if (s.hook >= 64 || s.start < 0) return "{\"ok\":false,\"error\":\"args\"}";
        s.ops.clear();
        for (size_t i = 5; i < t.size(); i++) { if (t[i] == "*") s.ops.push_back({'*', 0}); else { u64 k; bool neg = t[i][0] == '-'; if ((t[i][0] != '+' && !neg) || !parse_hex(t[i].substr(1), &k)) return "{\"ok\":false,\"error\":\"op\"}"; s.ops.push_back({'+', neg ? -(int64_t)k : (int64_t)k}); } }
        s.live.store(1, std::memory_order_release); spec_count.store(n + 1, std::memory_order_release); o << "{\"ok\":true,\"spec\":" << n << ",\"blob_bytes\":" << MODS_BLOB << "}"; return o.str();
    }
    if (c == "CLEARSNAPS") { int n = spec_count.load(); for (int i = 0; i < n; i++) specs[i].live = 0; return "{\"ok\":true}"; }
    if (c == "LOG" && t.size() == 3) { int id = atoi(t[1].c_str()); if (id < 0 || id >= 64) return "{\"ok\":false}"; hooks[id].log = atoi(t[2].c_str()); return "{\"ok\":true}"; }
    if (c == "RESET") { buf_used = 0; seq = 0; dropped = 0; whit_n = 0; return "{\"ok\":true}"; }
    if (c == "DUMP" && t.size() == 2) {   // write event buffer to a new file
        u64 n = buf_used.load(); if (n > buf_size) n = buf_size;
        HANDLE h = CreateFileA(t[1].c_str(), GENERIC_WRITE, 0, nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
        if (h == INVALID_HANDLE_VALUE) return "{\"ok\":false,\"error\":\"create (exists?)\"}";
        u64 done = 0; while (done < n) { DWORD w = 0, chunk = (DWORD)((n - done) > (64u << 20) ? (64u << 20) : (n - done)); if (!WriteFile(h, buf + done, chunk, &w, nullptr) || !w) break; done += w; }
        CloseHandle(h); o << "{\"ok\":" << (done == n ? "true" : "false") << ",\"bytes\":" << done << ",\"events\":" << seq.load() << ",\"dropped\":" << dropped.load() << "}"; return o.str();
    }
    if (c == "WWATCH" && t.size() == 3) {   // trace writes to [addr,addr+len)
        u64 a, l = strtoul(t[2].c_str(), nullptr, 10); if (!parse_hex(t[1], &a) || !plausible(a) || l < 1 || l > 4096 || wwatch_n >= 8 || wpage_n >= 8) return "{\"ok\":false,\"error\":\"args\"}";
        if (!whits) whits = (WHit*)VirtualAlloc(nullptr, WHIT_MAX * sizeof(WHit), MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
        u64 page = a & ~0xFFFull; bool known = false; for (int i = 0; i < wpage_n; i++) known |= wpages[i] == page;
        wwatch[wwatch_n++] = {a, l};
        if (!known) {   // register and activate BEFORE protecting: a writer landing in between must find the page already claimed
            MEMORY_BASIC_INFORMATION mbi; if (!VirtualQuery((void*)page, &mbi, sizeof mbi)) return "{\"ok\":false,\"error\":\"query\"}";
            { bool seen = false; int en = ever_n.load(); for (int i = 0; i < en; i++) seen |= ever_pages[i] == page; if (!seen && en < 64) { ever_pages[en] = page; ever_n.store(en + 1); } }
            page_orig_prot[wpage_n] = mbi.Protect; wpages[wpage_n++] = page; wactive = 1;
            DWORD old; if (!VirtualProtect((void*)page, 0x1000, PAGE_READONLY, &old)) { wpage_n--; return "{\"ok\":false,\"error\":\"protect\"}"; }
        }
        return "{\"ok\":true}";
    }
    if (c == "WSTOP") {   // order matters: faults must stay claimed until every page is unprotected and in-flight single-steps are done
        wstopping = 1;
        for (int pass = 0; pass < 2; pass++) { for (int i = 0; i < wpage_n; i++) { DWORD old; VirtualProtect((void*)wpages[i], 0x1000, page_orig_prot[i], &old); } Sleep(40); }
        wactive = 0; wpage_n = wwatch_n = 0; Sleep(20); wstopping = 0; return "{\"ok\":true}";
    }
    if (c == "WDUMP" && t.size() == 2) {
        u64 n = whit_n.load(); if (n > WHIT_MAX) n = WHIT_MAX;
        HANDLE h = CreateFileA(t[1].c_str(), GENERIC_WRITE, 0, nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
        if (h == INVALID_HANDLE_VALUE) return "{\"ok\":false,\"error\":\"create (exists?)\"}";
        DWORD w; WriteFile(h, whits, (DWORD)(n * sizeof(WHit)), &w, nullptr); CloseHandle(h); o << "{\"ok\":true,\"hits\":" << n << ",\"record_size\":" << sizeof(WHit) << "}"; return o.str();
    }
    if (c == "UNLOAD") {   // quiesce: disarm every probe/hook/tracer and stop worker threads; the module stays resident (inert handlers must outlive in-flight traps)
        if (wactive) { wstopping = 1; for (int pass = 0; pass < 2; pass++) { for (int i = 0; i < wpage_n; i++) { DWORD old; VirtualProtect((void*)wpages[i], 0x1000, page_orig_prot[i], &old); } Sleep(40); } wactive = 0; wpage_n = wwatch_n = 0; Sleep(20); wstopping = 0; }
        for (int i = 0; i < 3; i++) { dr_skip_a0[i] = 0; dr_va[i] = 0; hooks[i].active = 0; } dr_apply_all(true);
        for (int i = 0; i < 64; i++) if (hooks[i].active) { MH_DisableHook((void*)hooks[i].va); hooks[i].active = 0; }
        stop_rescan = true; unload_requested = true; return "{\"ok\":true,\"quiesced\":true}";
    }
    if (c == "POKE" && t.size() == 3) {   // POKE hexVA hexbytes  (live modification; SEH-guarded)
        u64 a; if (!parse_hex(t[1], &a) || !plausible(a) || t[2].size() % 2 || t[2].size() > 8192) return "{\"ok\":false,\"error\":\"args\"}";
        std::vector<u8> v(t[2].size() / 2); for (size_t i = 0; i < v.size(); i++) v[i] = hexv(t[2][2 * i]) << 4 | hexv(t[2][2 * i + 1]);
        return safe_copy((void*)a, v.data(), v.size()) ? "{\"ok\":true}" : "{\"ok\":false,\"error\":\"unwritable\"}";
    }
    return "{\"ok\":false,\"error\":\"unknown command\"}";
}

static DWORD WINAPI rescanner(void*) { while (!stop_rescan) { Sleep(250); bool any = dr_va[0] || dr_va[1] || dr_va[2]; if (any && !stop_rescan) dr_apply_all(false); } return 0; }
static DWORD WINAPI worker(void*) {
    QueryPerformanceFrequency((LARGE_INTEGER*)&qpc_freq);
    buf_size = 1024ull << 20; buf = (u8*)VirtualAlloc(nullptr, buf_size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    { HANDLE rt = CreateThread(nullptr, 0, rescanner, nullptr, 0, nullptr); if (rt) CloseHandle(rt); } MH_Initialize(); veh_handle = AddVectoredExceptionHandler(1, veh); AddVectoredExceptionHandler(1, dr_veh); start_forensics();
    wchar_t modpath[MAX_PATH]; GetModuleFileNameW(self_module, modpath, MAX_PATH);
    wchar_t* base = wcsrchr(modpath, L'\\'); base = base ? base + 1 : modpath; wchar_t* dot = wcsrchr(base, L'.'); if (dot) *dot = 0;
    wchar_t name[256]; swprintf_s(name, L"\\\\.\\pipe\\%s-%lu", base, GetCurrentProcessId());   // pipe name follows the DLL file name (versioned reloads)
    for (;;) {
        HANDLE pipe = CreateNamedPipeW(name, PIPE_ACCESS_DUPLEX, PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS, 1, 1 << 20, 1 << 16, 1000, nullptr);
        if (pipe == INVALID_HANDLE_VALUE) { Sleep(500); continue; }
        if (ConnectNamedPipe(pipe, nullptr) || GetLastError() == ERROR_PIPE_CONNECTED) {
            static char in[65536]; DWORD n = 0;
            if (ReadFile(pipe, in, sizeof in - 1, &n, nullptr) && n) {
                in[n] = 0; std::string line(in); while (!line.empty() && (line.back() == '\n' || line.back() == '\r')) line.pop_back();
                std::string out = handle(line) + "\n"; DWORD w; WriteFile(pipe, out.data(), (DWORD)out.size(), &w, nullptr);
                char ack[8]; DWORD k; ReadFile(pipe, ack, sizeof ack, &k, nullptr);
            }
        }
        FlushFileBuffers(pipe); DisconnectNamedPipe(pipe); CloseHandle(pipe);
        if (unload_requested) { Sleep(500); return 0; }   // thread ends; module and handlers stay resident
    }
}
BOOL WINAPI DllMain(HINSTANCE h, DWORD reason, void*) {
    if (reason == DLL_PROCESS_ATTACH) { self_module = h; DisableThreadLibraryCalls(h); HANDLE t = CreateThread(nullptr, 0, worker, nullptr, 0, nullptr); if (t) CloseHandle(t); else return FALSE; }
    return TRUE;
}
