// Self-test for acu_tap.dll: hooks a local function through the generic thunk and checks that
// register/stack/xmm arguments and return values are preserved, snapshots are captured,
// and the page-protection write tracer reports the writer. Prints PASS/FAIL lines.
#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
typedef uint64_t u64;

extern "C" __declspec(noinline) double target_fn(u64 a, u64 b, u64 c, u64 d, u64 e, u64 f, u64 g, double x, float y) {
    volatile u64 sink = a ^ b ^ c;
    return (double)(a + b * 2 + c * 3 + d * 5 + e * 7 + f * 11 + g * 13 + sink * 0) + x * 0.5 + y;
}
struct Block { u64 p; u64 dummy; };
static volatile u64 g_written[64];
extern "C" __declspec(noinline) void writer_fn(int i, u64 v) { g_written[i] = v; }

static const char* g_name = "acu_tap3.dll"; static char g_stem[64];
static bool fault_caught() { __try { *(volatile int*)8 = 1; } __except (EXCEPTION_EXECUTE_HANDLER) { return true; } return false; }
static std::string cmd(const char* text) {
    wchar_t name[256]; swprintf_s(name, L"\\\\.\\pipe\\%S-%lu", g_stem, GetCurrentProcessId());
    for (int i = 0; i < 200; i++) {
        if (WaitNamedPipeW(name, 100)) {
            HANDLE h = CreateFileW(name, GENERIC_READ | GENERIC_WRITE, 0, nullptr, OPEN_EXISTING, 0, nullptr);
            if (h == INVALID_HANDLE_VALUE) { Sleep(20); continue; }
            DWORD n; WriteFile(h, text, (DWORD)strlen(text), &n, nullptr);
            char out[70000]; ReadFile(h, out, sizeof out - 1, &n, nullptr); out[n] = 0; WriteFile(h, "ACK", 3, &n, nullptr); CloseHandle(h);
            return out;
        }
        Sleep(20);
    }
    return "TIMEOUT";
}
static int fails = 0;
static void check(bool ok, const char* what) { printf("%s %s\n", ok ? "PASS" : "FAIL", what); if (!ok) fails++; }

static volatile bool wh_stop = false; static volatile long long wh_writes = 0;
static DWORD WINAPI w_hammer(void*) { long long n = 0; while (!wh_stop) { writer_fn((int)(n % 64), (u64)n); n++; wh_writes = n; } return 0; }
static volatile bool hammer_stop = false; static volatile long long hammer_calls = 0;
static DWORD WINAPI hammer(void*) { while (!hammer_stop) { target_fn(1, 2, 3, 4, 5, 6, 7, 8.0, 0.25f); hammer_calls++; } return 0; }
int main(int argc, char** argv) {
    setvbuf(stdout, nullptr, _IONBF, 0);
    if (argc > 1) g_name = argv[1];
    strcpy(g_stem, g_name); *strrchr(g_stem, '.') = 0;
    char dll[MAX_PATH]; GetModuleFileNameA(nullptr, dll, MAX_PATH); strcpy(strrchr(dll, '\\') + 1, g_name);
    if (!LoadLibraryA(dll)) { printf("FAIL load %lu\n", GetLastError()); return 2; }
    u64 base = (u64)GetModuleHandleW(nullptr);
    double before = target_fn(1, 2, 3, 4, 5, 6, 7, 8.0, 0.25f);
    unsigned char pro[16]; memcpy(pro, (void*)&target_fn, 16);
    char hexs[40]; for (int i = 0; i < 16; i++) sprintf(hexs + 2 * i, "%02x", pro[i]);
    // hook with a 16-byte guard
    char line[256]; sprintf(line, "HOOK 3 %llx %s selftest", (unsigned long long)((u64)&target_fn - base), hexs);
    std::string r = cmd(line); printf("%s", r.c_str()); check(r.find("\"ok\":true") != std::string::npos, "hook installed");
    // snapshot: 16 bytes at a0 (pointer to Block) on entry, and stack arg slot via rsp
    static Block blk = {0x1122334455667788ull, 0x99aabbccddeeff00ull};
    cmd("SNAP 3 entry 1 16 a0");
    cmd("SNAP 3 entry 2 8 rsp +38");   // 7th arg (g) at entry_rsp_ptr+0x38
    double after = target_fn((u64)&blk, 2, 3, 4, 5, 6, 0x7777777777ull, 8.0, 0.25f);
    double expect = (double)((u64)&blk + 2 * 2 + 3 * 3 + 4 * 5 + 5 * 7 + 6 * 11 + 0x7777777777ull * 13) + 4.0 + 0.25;
    check(after == expect, "return value and all args (regs, 3 stack, 2 xmm) preserved");
    check(before == target_fn(1, 2, 3, 4, 5, 6, 7, 8.0, 0.25f), "same function result under hook");
    std::string st = cmd("STATUS"); check(st.find("\"entries\":2") != std::string::npos && st.find("\"exits\":2") != std::string::npos, "entry/exit counted");
    char path[MAX_PATH]; sprintf(path, "%s.events.%lu", dll, GetCurrentProcessId());
    r = cmd((std::string("DUMP ") + path).c_str()); check(r.find("\"ok\":true") != std::string::npos, "dump written");
    FILE* f = fopen(path, "rb"); bool found = false, found7 = false;
    if (f) { static unsigned char b[1 << 16]; size_t n = fread(b, 1, sizeof b, f); fclose(f);
        for (size_t i = 0; i + 16 <= n; i++) { if (!memcmp(b + i, &blk, 16)) found = true; }
        u64 seven = 0x7777777777ull; for (size_t i = 0; i + 8 <= n; i++) if (!memcmp(b + i, &seven, 8)) found7 = true; }
    check(found, "snapshot copied pointed-to block on the calling thread");
    check(found7, "stack-argument snapshot (7th arg) present");
    DeleteFileA(path);
    // write tracer
    char w[128]; sprintf(w, "WWATCH %llx 16", (unsigned long long)&g_written[4]); r = cmd(w); check(r.find("\"ok\":true") != std::string::npos, "write watch armed");
    for (int i = 0; i < 64; i++) writer_fn(i, 0x1000 + i);
    cmd("WSTOP");
    bool allok = true; for (int i = 0; i < 64; i++) allok &= g_written[i] == (u64)(0x1000 + i);
    check(allok, "memory writes still land under tracing");
    char wp[MAX_PATH]; sprintf(wp, "%s.w.%lu", dll, GetCurrentProcessId()); r = cmd((std::string("WDUMP ") + wp).c_str()); printf("%s", r.c_str());
    f = fopen(wp, "rb"); int hits = 0; bool rip_ok = false;
    if (f) { struct H { u64 qpc, rip, addr, rsp; unsigned tid, len; u64 stack[40]; unsigned char bytes[16]; } h; while (fread(&h, sizeof h, 1, f) == 1) { hits++; if (h.rip >= (u64)&writer_fn && h.rip < (u64)&writer_fn + 64) rip_ok = true; } fclose(f); }
    check(hits >= 2 && hits <= 5 && rip_ok, "watched writes recorded with writer RIP (slack hits allowed)");
    DeleteFileA(wp);
    r = cmd("UNHOOK 3"); check(r.find("\"ok\":true") != std::string::npos, "unhook");
    check(target_fn(1, 2, 3, 4, 5, 6, 7, 8.0, 0.25f) == before, "function restored");
    // hardware-breakpoint probe (no code patching): entry + one-shot return trap
    sprintf(line, "DRHOOK 1 %llx %s drtest exit", (unsigned long long)((u64)&target_fn - base), hexs);
    r = cmd(line); check(r.find("\"ok\":true") != std::string::npos, "DR probe armed");
    cmd("SNAP 1 entry 11 16 a0"); cmd("SNAP 1 exit 12 16 a0");
    double dr_after = target_fn((u64)&blk, 2, 3, 4, 5, 6, 0x7777777777ull, 8.0, 0.25f);
    check(dr_after == expect, "DR probe preserves return value");
    st = cmd("STATUS"); check(st.find("drtest") != std::string::npos && st.find("\"entries\":1,\"exits\":1") != std::string::npos, "DR probe saw entry and exit once");
    sprintf(path, "%s.dr.%lu", dll, GetCurrentProcessId()); r = cmd((std::string("DUMP ") + path).c_str());
    f = fopen(path, "rb"); int blkhits = 0;
    if (f) { static unsigned char b2[1 << 16]; size_t n = fread(b2, 1, sizeof b2, f); fclose(f); for (size_t i = 0; i + 16 <= n; i++) if (!memcmp(b2 + i, &blk, 16)) blkhits++; }
    check(blkhits >= 2, "DR entry+exit snapshots captured on calling thread");
    DeleteFileA(path); cmd("DRUNHOOK 1");
    check(target_fn((u64)&blk, 2, 3, 4, 5, 6, 0x7777777777ull, 8.0, 0.25f) == expect, "DR probe removed cleanly");
    // race stress: arm/disarm repeatedly while another thread calls the function (previously fatal: stale single-step)
    {
        HANDLE ht = CreateThread(nullptr, 0, hammer, nullptr, 0, nullptr);
        for (int i = 0; i < 150; i++) { cmd(line); cmd("DRUNHOOK 1"); }
        hammer_stop = true; WaitForSingleObject(ht, 5000); CloseHandle(ht);
        printf("  hammer calls=%lld\n", (long long)hammer_calls); check(hammer_calls > 50, "survived 150 arm/disarm cycles under concurrent calls");
    }
    // write-trace race stress: writers run continuously while the tracer is armed/stopped repeatedly (previous WSTOP window crashed the game)
    {
        HANDLE h1 = CreateThread(nullptr, 0, w_hammer, nullptr, 0, nullptr), h2 = CreateThread(nullptr, 0, w_hammer, nullptr, 0, nullptr);
        for (int i = 0; i < 100; i++) { char w2[128]; sprintf(w2, "WWATCH %llx 64", (unsigned long long)&g_written[0]); cmd(w2); Sleep(3); cmd("WSTOP"); }
        wh_stop = true; WaitForSingleObject(h1, 5000); WaitForSingleObject(h2, 5000); CloseHandle(h1); CloseHandle(h2);
        DWORD prot = 0; MEMORY_BASIC_INFORMATION mbi; VirtualQuery((void*)&g_written[0], &mbi, sizeof mbi); prot = mbi.Protect;
        printf("  writer iterations=%lld\n",(long long)wh_writes); check(wh_writes > 50 && prot == PAGE_READWRITE, "survived 100 write-trace arm/stop cycles under concurrent writers; page protection restored");
    }
    // quiesce + versioned reload: the old module stays resident, handlers stay safe, a new copy answers on its own pipe
    r = cmd("UNLOAD"); check(r.find("quiesced") != std::string::npos, "UNLOAD quiesces");
    Sleep(800);
    check(fault_caught(), "exceptions still handled after quiesce");
    {
        char dll2[MAX_PATH]; strcpy(dll2, dll); strcpy(strrchr(dll2, '\\') + 1, "acu_tap_reload_test.dll");
        CopyFileA(dll, dll2, FALSE); strcpy(g_stem, "acu_tap_reload_test");
        bool loaded = LoadLibraryA(dll2) != nullptr; check(loaded, "second versioned copy loads");
        r = cmd("STATUS"); check(r.find("\"ok\":true") != std::string::npos, "second copy answers on its own pipe");
        cmd("UNLOAD");
    }
    printf("%s\n", fails ? "SELFTEST FAILED" : "SELFTEST PASSED"); return fails ? 1 : 0;
}
