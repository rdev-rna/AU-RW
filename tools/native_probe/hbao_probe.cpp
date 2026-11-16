// acu_hbao_probe.dll: the parameters ACU passes to NVIDIA HBAO+ (max AO), for the AO port.
//
// Injected with acu_probe_launcher (`capture_run.py --extra-dll`). Target: GFSDK_SSAO.win64.dll as
// installed with ACU (SHA-256 3194ccc5…b1f3, base 0x180000000, analysed in IDA on a local copy):
// - GFSDK_SSAO_CreateContext_D3D11 builds GFSDK::SSAO::D3D11::Renderer (vtable RVA 0x9D4B8:
//   slot 2 = RenderAO at RVA 0x10880, slot 3 = PreCreateRTs, slot 4 = Release);
// - RenderAO(this, ID3D11DeviceContext*, InputData* (r8), Parameters* (r9), Output*, mask) emits the
//   "HBAO+" perf event; SetAOParameters 0x18000CA80 reads the Parameters through +0x5C.
// The hook checks the DLL's first function bytes against the analysed copy, then logs the first
// calls' raw Parameters (0x60 bytes) and InputData (0xC0 bytes) and counts every call. Output:
// acu_hbao_probe-<pid>.json beside the DLL every 2 s. Read-only towards the game.
#include <windows.h>
#include <MinHook.h>

#include <atomic>
#include <cstdio>
#include <cstring>
#include <string>

namespace {

constexpr uintptr_t kRenderAoRva = 0x10880;
// First 10 bytes of RenderAO in the analysed copy (push rbx; push rdi; sub rsp, 0x188), checked
// before hooking; the following bytes hold a RIP-relative displacement.
constexpr unsigned char kExpected[10] = {0x40, 0x53, 0x57, 0x48, 0x81, 0xEC, 0x88, 0x01, 0x00, 0x00};
constexpr int kKeep = 4;
constexpr size_t kParams = 0x60, kInput = 0xC0;

HMODULE self;
std::string status = "waiting for GFSDK_SSAO.win64.dll";
std::atomic<long long> calls{0};
SRWLOCK lock = SRWLOCK_INIT;
unsigned char params[kKeep][kParams], input[kKeep][kInput];
unsigned mask[kKeep];
int kept = 0;
unsigned char firstBytes[16];

using RenderAO = int(__fastcall*)(void*, void*, void*, void*, void*, unsigned);
RenderAO realRenderAO;

bool safeCopy(void* dst, const void* src, size_t n) {
    __try {
        memcpy(dst, src, n);
        return true;
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        return false;
    }
}

int __fastcall fakeRenderAO(void* self_, void* context, void* in, void* p, void* out, unsigned m) {
    long long n = calls++;
    if (n < kKeep && p && in) {
        AcquireSRWLockExclusive(&lock);
        if (kept < kKeep && safeCopy(params[kept], p, kParams) && safeCopy(input[kept], in, kInput)) {
            mask[kept] = m;
            kept++;
        }
        ReleaseSRWLockExclusive(&lock);
    }
    return realRenderAO(self_, context, in, p, out, m);
}

std::wstring beside(const wchar_t* name) {
    wchar_t path[MAX_PATH];
    GetModuleFileNameW(self, path, MAX_PATH);
    std::wstring s(path);
    return s.substr(0, s.find_last_of(L"\\/") + 1) + name;
}

void hex(FILE* f, const unsigned char* b, size_t n) {
    fputc('"', f);
    for (size_t i = 0; i < n; i++) fprintf(f, "%02x", b[i]);
    fputc('"', f);
}

void dump(const std::wstring& path) {
    std::wstring tmp = path + L".tmp";
    FILE* f = nullptr;
    if (_wfopen_s(&f, tmp.c_str(), L"wb") || !f) return;
    fprintf(f, "{\"pid\":%lu,\"tick_ms\":%llu,\"status\":\"%s\",\"calls\":%lld,\"first_bytes\":", GetCurrentProcessId(),
            GetTickCount64(), status.c_str(), calls.load());
    hex(f, firstBytes, sizeof firstBytes);
    fprintf(f, ",\"captures\":[");
    AcquireSRWLockShared(&lock);
    for (int i = 0; i < kept; i++) {
        fprintf(f, "%s{\"mask\":%u,\"params\":", i ? "," : "", mask[i]);
        hex(f, params[i], kParams);
        fprintf(f, ",\"input\":");
        hex(f, input[i], kInput);
        fputc('}', f);
    }
    ReleaseSRWLockShared(&lock);
    fprintf(f, "]}\n");
    fclose(f);
    MoveFileExW(tmp.c_str(), path.c_str(), MOVEFILE_REPLACE_EXISTING);
}

DWORD WINAPI run(void*) {
    wchar_t name[64];
    swprintf_s(name, L"acu_hbao_probe-%lu.json", GetCurrentProcessId());
    std::wstring path = beside(name);
    bool hooked = false;
    for (;;) {
        if (!hooked) {
            HMODULE dll = GetModuleHandleW(L"GFSDK_SSAO.win64.dll");
            if (dll) {
                void* target = reinterpret_cast<unsigned char*>(dll) + kRenderAoRva;
                memcpy(firstBytes, target, sizeof firstBytes);
                if (memcmp(firstBytes, kExpected, sizeof kExpected) != 0) {
                    status = "RenderAO bytes differ from the analysed copy; not hooked";
                } else if (MH_Initialize() != MH_OK ||
                           MH_CreateHook(target, &fakeRenderAO, reinterpret_cast<void**>(&realRenderAO)) != MH_OK ||
                           MH_EnableHook(target) != MH_OK) {
                    status = "hook failed";
                } else {
                    status = "hooked";
                }
                hooked = true;
            }
        }
        dump(path);
        Sleep(2000);
    }
}

}  // namespace

BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID) {
    if (reason == DLL_PROCESS_ATTACH) {
        self = instance;
        DisableThreadLibraryCalls(instance);
        HANDLE t = CreateThread(nullptr, 0, run, nullptr, 0, nullptr);
        if (t) CloseHandle(t);
    }
    return TRUE;
}
