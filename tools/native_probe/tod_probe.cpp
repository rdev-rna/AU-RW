// acu_tod.dll: switch ACU's time of day (Season) on request, for live sun captures (ACU-RE-0073).
//
// Injected with acu_probe_launcher (`capture_run.py --extra-dll`). Lead: ACUFixes (NameTaken3125,
// commit e20228a0, Cheat_WeatherControls / TimeOfDayManager / Season headers), re-derived here:
// - current World = [[image + 0x5217368] + 0x98] (the focus shim's kWorldHolderRva);
// - its TimeOfDayManager at World + 0x980 (ACUFixes `World::timeOfDayManager`);
// - the current Season through the manager's shared pointer at +0x60: block = [tod + 0x60],
//   season = [block] when the int at block + 12 is negative (the masking 0x141E2F660 itself does);
//   Season ID at season + 0x1C (0x141E2F660 compares its argument with exactly this field);
// - switch: 0x141E2F660(tod, seasonID) (ACUFixes `TimeOfDayManager__SetCurrentSeason_mb`).
//   Paris IDs: 4 dawn, 5 day, 6 sunset, 7 night (ACUFixes `SeasonID`).
// The call runs on the game window's thread: a WH_GETMESSAGE hook on that thread handles a
// private posted message (inside SEH), so it never races the game loop from a foreign thread.
// Requests: write a decimal season ID to acu_tod-<pid>.req beside the DLL; a value of 0 only
// reads. The result goes to acu_tod-<pid>.json. Nothing else is written; not unloaded while ACU runs.
#include <windows.h>

#include <cstdint>
#include <cstdio>
#include <string>

namespace {

constexpr uintptr_t kWorldHolderRva = 0x5217368, kWorldCurrent = 0x98, kWorldTod = 0x980;
constexpr uintptr_t kSetSeasonRva = 0x1E2F660, kTodCurrentSeason = 0x60, kSeasonId = 0x1C;
constexpr UINT kMessage = WM_APP + 0x61;

HMODULE self;
HWND gameWindow;
HHOOK hook;
volatile LONG requestId = -1;  // season ID waiting for the game thread, -1 = none
volatile LONG requests = 0, served = 0;
std::wstring resultPath;

struct Result {
    int status = 0;  // 1 called, 2 read only, -1 no World, -2 no manager, -3 exception
    uint64_t world = 0, tod = 0;
    int before = -1, after = -1, requested = -1;
    DWORD exception = 0;
};

using SetSeason = void(__fastcall*)(void*, unsigned int);

int currentSeason(uintptr_t tod) {
    uintptr_t block = *reinterpret_cast<uintptr_t*>(tod + kTodCurrentSeason);
    if (!block) return -1;
    if (*reinterpret_cast<int32_t*>(block + 12) >= 0) return -1;
    uintptr_t season = *reinterpret_cast<uintptr_t*>(block);
    return season ? *reinterpret_cast<int32_t*>(season + kSeasonId) : -1;
}

int guarded(Result& r, int id) {
    __try {
        uintptr_t image = reinterpret_cast<uintptr_t>(GetModuleHandleW(nullptr));
        uintptr_t holder = *reinterpret_cast<uintptr_t*>(image + kWorldHolderRva);
        r.world = holder ? *reinterpret_cast<uintptr_t*>(holder + kWorldCurrent) : 0;
        if (!r.world) return -1;
        r.tod = *reinterpret_cast<uintptr_t*>(r.world + kWorldTod);
        if (!r.tod) return -2;
        r.before = currentSeason(r.tod);
        if (id > 0) reinterpret_cast<SetSeason>(image + kSetSeasonRva)(reinterpret_cast<void*>(r.tod), id);
        r.after = currentSeason(r.tod);
        return id > 0 ? 1 : 2;
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        r.exception = GetExceptionCode();
        return -3;
    }
}

void write(const Result& r) {
    std::wstring tmp = resultPath + L".tmp";
    FILE* f = nullptr;
    if (_wfopen_s(&f, tmp.c_str(), L"wb") || !f) return;
    fprintf(f,
            "{\"pid\":%lu,\"tick_ms\":%llu,\"served\":%ld,\"status\":%d,\"requested\":%d,\"world\":\"0x%llx\","
            "\"tod\":\"0x%llx\",\"season_before\":%d,\"season_after\":%d,\"exception\":\"0x%lx\"}\n",
            GetCurrentProcessId(), GetTickCount64(), served, r.status, r.requested,
            static_cast<unsigned long long>(r.world), static_cast<unsigned long long>(r.tod), r.before, r.after,
            r.exception);
    fclose(f);
    MoveFileExW(tmp.c_str(), resultPath.c_str(), MOVEFILE_REPLACE_EXISTING);
}

LRESULT CALLBACK getMessage(int code, WPARAM remove, LPARAM lparam) {
    MSG* msg = reinterpret_cast<MSG*>(lparam);
    if (code == HC_ACTION && remove == PM_REMOVE && msg && msg->message == kMessage) {
        LONG id = InterlockedExchange(&requestId, -1);
        if (id >= 0) {
            Result r;
            r.requested = id;
            r.status = guarded(r, id);
            InterlockedIncrement(&served);
            write(r);
        }
        msg->message = WM_NULL;
    }
    return CallNextHookEx(hook, code, remove, lparam);
}

BOOL CALLBACK findWindow(HWND hwnd, LPARAM) {
    DWORD pid = 0;
    GetWindowThreadProcessId(hwnd, &pid);
    if (pid != GetCurrentProcessId() || GetWindow(hwnd, GW_OWNER) || !IsWindowVisible(hwnd)) return TRUE;
    RECT rc;
    if (!GetWindowRect(hwnd, &rc) || rc.right - rc.left < 320) return TRUE;  // skip helper windows
    gameWindow = hwnd;
    return FALSE;
}

std::wstring beside(const wchar_t* name) {
    wchar_t path[MAX_PATH];
    GetModuleFileNameW(self, path, MAX_PATH);
    std::wstring s(path);
    return s.substr(0, s.find_last_of(L"\\/") + 1) + name;
}

DWORD WINAPI run(void*) {
    wchar_t name[64];
    swprintf_s(name, L"acu_tod-%lu.json", GetCurrentProcessId());
    resultPath = beside(name);
    swprintf_s(name, L"acu_tod-%lu.req", GetCurrentProcessId());
    std::wstring requestPath = beside(name);
    for (int i = 0; i < 600 && !gameWindow; i++) {
        EnumWindows(findWindow, 0);
        if (!gameWindow) Sleep(500);
    }
    if (!gameWindow) return 0;
    hook = SetWindowsHookExW(WH_GETMESSAGE, getMessage, self, GetWindowThreadProcessId(gameWindow, nullptr));
    Result ready;
    ready.status = hook ? 0 : -9;
    write(ready);
    FILETIME last{};
    for (;;) {
        Sleep(200);
        WIN32_FILE_ATTRIBUTE_DATA a;
        if (!GetFileAttributesExW(requestPath.c_str(), GetFileExInfoStandard, &a)) continue;
        if (CompareFileTime(&a.ftLastWriteTime, &last) == 0) continue;
        last = a.ftLastWriteTime;
        FILE* f = nullptr;
        int id = -1;
        if (_wfopen_s(&f, requestPath.c_str(), L"rb") || !f) continue;
        if (fscanf_s(f, "%d", &id) != 1) id = -1;
        fclose(f);
        if (id < 0 || id > 255) continue;
        InterlockedExchange(&requestId, id);
        InterlockedIncrement(&requests);
        PostMessageW(gameWindow, kMessage, 0, 0);
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
