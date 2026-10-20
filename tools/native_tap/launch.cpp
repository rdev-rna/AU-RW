// acu_tap_launch.exe: starts ACU through Ubisoft Connect, waits for a sustained ACU.exe process,
// verifies the executable SHA-256, injects acu_tap.dll (LoadLibrary) and confirms the tap pipe.
// Usage: acu_tap_launch.exe [--attach-only] [--attempts N] [--settle-seconds S]
// Never modifies game files; Ubisoft Connect remains the launching parent.
#include <windows.h>
#include <shellapi.h>
#include <tlhelp32.h>
#include <wincrypt.h>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#pragma comment(lib, "advapi32.lib")
#pragma comment(lib, "shell32.lib")

static const char* kSha = "748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40";

static std::wstring g_stem = L"acu_tap3";   // DLL file stem; the tap's pipe is named <stem>-<pid>
static DWORD find_acu() {
    HANDLE s = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0); PROCESSENTRY32W e{sizeof e}; DWORD pid = 0;
    if (Process32FirstW(s, &e)) do { if (!_wcsicmp(e.szExeFile, L"ACU.exe")) pid = e.th32ProcessID; } while (Process32NextW(s, &e));
    CloseHandle(s); return pid;
}
static std::string sha256_file(const std::wstring& path) {
    HANDLE f = CreateFileW(path.c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr, OPEN_EXISTING, 0, nullptr);
    if (f == INVALID_HANDLE_VALUE) return "";
    HCRYPTPROV p; HCRYPTHASH h; if (!CryptAcquireContextW(&p, nullptr, nullptr, PROV_RSA_AES, CRYPT_VERIFYCONTEXT)) return "";
    CryptCreateHash(p, CALG_SHA_256, 0, 0, &h); std::vector<BYTE> b(1 << 20); DWORD n;
    while (ReadFile(f, b.data(), (DWORD)b.size(), &n, nullptr) && n) CryptHashData(h, b.data(), n, 0);
    BYTE d[32]; DWORD l = 32; CryptGetHashParam(h, HP_HASHVAL, d, &l, 0); CryptDestroyHash(h); CryptReleaseContext(p, 0); CloseHandle(f);
    char o[65]; for (int i = 0; i < 32; i++) sprintf_s(o + 2 * i, 3, "%02x", d[i]); return o;
}
static std::string pipe_cmd(DWORD pid, const char* text) {
    wchar_t name[256]; swprintf_s(name, L"\\\\.\\pipe\\%s-%lu", g_stem.c_str(), pid);
    if (!WaitNamedPipeW(name, 3000)) return "";
    HANDLE h = CreateFileW(name, GENERIC_READ | GENERIC_WRITE, 0, nullptr, OPEN_EXISTING, 0, nullptr); if (h == INVALID_HANDLE_VALUE) return "";
    DWORD n; WriteFile(h, text, (DWORD)strlen(text), &n, nullptr); static char out[70000];
    if (!ReadFile(h, out, sizeof out - 1, &n, nullptr)) n = 0; out[n] = 0; WriteFile(h, "ACK", 3, &n, nullptr); CloseHandle(h); return out;
}
static bool inject(DWORD pid, const std::wstring& dll) {
    HANDLE p = OpenProcess(PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ, FALSE, pid);
    if (!p) { printf("OpenProcess failed %lu\n", GetLastError()); return false; }
    size_t bytes = (dll.size() + 1) * sizeof(wchar_t); void* mem = VirtualAllocEx(p, nullptr, bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    if (!mem || !WriteProcessMemory(p, mem, dll.c_str(), bytes, nullptr)) { printf("write failed %lu\n", GetLastError()); CloseHandle(p); return false; }
    HANDLE t = CreateRemoteThread(p, nullptr, 0, (LPTHREAD_START_ROUTINE)GetProcAddress(GetModuleHandleW(L"kernel32.dll"), "LoadLibraryW"), mem, 0, nullptr);
    if (!t) { printf("CreateRemoteThread failed %lu\n", GetLastError()); CloseHandle(p); return false; }
    WaitForSingleObject(t, 20000); DWORD code = 0; GetExitCodeThread(t, &code); CloseHandle(t); VirtualFreeEx(p, mem, 0, MEM_RELEASE); CloseHandle(p);
    printf("LoadLibrary returned module low32=0x%lx\n", code); return code != 0;
}

int main(int argc, char** argv) {
    bool attach_only = false; int attempts = 4, settle = 25;
    for (int i = 1; i < argc; i++) { if (!strcmp(argv[i], "--attach-only")) attach_only = true; else if (!strcmp(argv[i], "--attempts") && i + 1 < argc) attempts = atoi(argv[++i]); else if (!strcmp(argv[i], "--settle-seconds") && i + 1 < argc) settle = atoi(argv[++i]); }
    wchar_t self[MAX_PATH]; GetModuleFileNameW(nullptr, self, MAX_PATH); std::wstring dir(self); dir.resize(dir.find_last_of(L'\\') + 1);
    for (int i = 1; i + 1 < argc; i++) if (!strcmp(argv[i], "--dll-stem")) { std::string s = argv[i + 1]; g_stem.assign(s.begin(), s.end()); }
    std::wstring dll = dir + g_stem + L".dll"; if (GetFileAttributesW(dll.c_str()) == INVALID_FILE_ATTRIBUTES) { printf("%ls missing next to launcher\n", dll.c_str()); return 2; }
    for (int a = 1; a <= attempts; a++) {
        DWORD pid = find_acu();
        if (!pid && !attach_only) { printf("attempt %d: launching via Ubisoft Connect\n", a); ShellExecuteW(nullptr, L"open", L"uplay://launch/720/0", nullptr, nullptr, SW_SHOWNORMAL); }
        for (int w = 0; w < 90 && !pid; w++) { Sleep(1000); pid = find_acu(); }
        if (!pid) { printf("attempt %d: ACU.exe never appeared\n", a); if (attach_only) return 3; continue; }
        printf("ACU.exe pid %lu; waiting %d s to confirm the process is sustained\n", pid, settle);
        HANDLE p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, FALSE, pid);
        if (!p) { printf("cannot open process %lu\n", GetLastError()); return 4; }
        if (WaitForSingleObject(p, settle * 1000) == WAIT_OBJECT_0) { DWORD c = 0; GetExitCodeProcess(p, &c); printf("attempt %d: game exited early, code %lu\n", a, c); CloseHandle(p); continue; }
        wchar_t img[MAX_PATH]; DWORD n = MAX_PATH; QueryFullProcessImageNameW(p, 0, img, &n); CloseHandle(p);
        std::string sha = sha256_file(img); printf("image %ls sha256 %s\n", img, sha.c_str());
        if (sha != kSha) { printf("REFUSING: executable hash mismatch\n"); return 5; }
        if (!inject(pid, dll)) return 6;
        for (int k = 0; k < 20; k++) { std::string s = pipe_cmd(pid, "STATUS"); if (!s.empty()) { printf("TAP READY pid=%lu %s", pid, s.c_str()); return 0; } Sleep(500); }
        printf("injected but pipe not answering\n"); return 7;
    }
    printf("game did not stay up after %d attempts\n", attempts); return 1;
}
