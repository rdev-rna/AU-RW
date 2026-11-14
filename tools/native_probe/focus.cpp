// Background-focus shim for the original ACU process. Injected with acu_probe_launcher --attach.
// Keeps the game running and reading the controller while another window has focus:
//  - subclasses the game's top-level window and drops deactivation messages;
//  - GetForegroundWindow/GetActiveWindow/GetFocus return the game window inside this process;
//  - XInputEnable(FALSE) is ignored (XInputEnable(TRUE) is issued once at install).
// While the game is *really* in the background (original foreground query), mouse and keyboard
// are neutralised so the user's desktop work never reaches the game: GetCursorPos returns the
// last focused position, and DirectInput GetDeviceState/GetDeviceData report no input. XInput
// (the virtual controller) is untouched. With the game really focused, everything passes through.
// In-process virtual controller: when a runner writes an enabled state into the shared section
// Local\acu-pad-<pid>, XInputGetState/GetStateEx/GetCapabilities report a connected pad at user
// index 0 with that state, and the window is sent WM_DEVICECHANGE so the game re-enumerates.
// Nothing outside this process sees the pad; other user indices pass through to the real API.
// Frame limiter: with fps_cap set in acu_focus.ini ([focus] fps_cap=N) beside this DLL,
// IDXGISwapChain::Present (vtable slot 8, found through a throwaway swap chain) waits so the game
// presents at most N frames per second; the status reports the measured rate.
// Everything is reversible: signal the named event Local\acu-focus-stop-<pid> and the original
// window procedure, the saved 5 code bytes of each patched export and the two DirectInput
// device vtable slots are restored. No game code or file changes.
// Status JSON is written beside this DLL as focus-<pid>.json.
#define NOMINMAX
#define DIRECTINPUT_VERSION 0x0800
#include <windows.h>
#include <dinput.h>
#include <xinput.h>
#include <d3d11.h>
#include <cstdio>
#include <cstdint>
#include <cstring>
#include <string>
#pragma comment(lib, "user32.lib")
#pragma comment(lib, "dxguid.lib")
#pragma comment(lib, "winmm.lib")

static HMODULE self;
static HWND gameWindow = nullptr;
static WNDPROC originalProc = nullptr;
static volatile LONG droppedActivateApp = 0, droppedActivate = 0, droppedKillFocus = 0, droppedNcActivate = 0,
                     ignoredXInputDisable = 0, frozenCursorReads = 0, blankedDeviceState = 0, blankedDeviceData = 0;
struct Patch { const char* module; const char* name; void* target; BYTE saved[5]; bool active; };
static Patch patches[16];

// Frame limiter state.
using PresentFn = HRESULT(STDMETHODCALLTYPE*)(IDXGISwapChain*, UINT, UINT);
static void** presentTable = nullptr;
static PresentFn originalPresent = nullptr;
static bool presentPatched = false;
static unsigned fpsCap = 0;
static volatile LONG presents = 0, measuredFps = 0;
static LARGE_INTEGER qpcFrequency;
static int64_t nextFrame = 0;
static HRESULT STDMETHODCALLTYPE presentFake(IDXGISwapChain* chain, UINT sync, UINT flags) {
    if (fpsCap && !(flags & DXGI_PRESENT_TEST)) {
        const int64_t period = qpcFrequency.QuadPart / fpsCap;
        LARGE_INTEGER now;
        QueryPerformanceCounter(&now);
        if (!nextFrame || now.QuadPart - nextFrame > period) nextFrame = now.QuadPart;  // start, or fell behind
        while (now.QuadPart < nextFrame) {
            int64_t ms = (nextFrame - now.QuadPart) * 1000 / qpcFrequency.QuadPart;
            if (ms > 1) Sleep((DWORD)(ms - 1)); else YieldProcessor();
            QueryPerformanceCounter(&now);
        }
        nextFrame += period;
    }
    InterlockedIncrement(&presents);
    return originalPresent(chain, sync, flags);
}
static int patchCount = 0;

// The real foreground query, resolved from the user32 stub before that stub is patched.
static HWND(WINAPI* realForeground)() = nullptr;
static bool reallyFocused() { return !realForeground || realForeground() == gameWindow; }

static HWND WINAPI gameWindowFake() { return gameWindow; }
static void WINAPI xinputEnableFake(BOOL enable) { if (!enable) InterlockedIncrement(&ignoredXInputDisable); }

// Shared virtual-pad state written by the runner: '<IIIHBBhhhh' little-endian, 24 bytes.
struct PadShared { uint32_t magic, enabled, sequence; uint16_t buttons; uint8_t leftTrigger, rightTrigger;
                   int16_t leftX, leftY, rightX, rightY; };
static_assert(sizeof(PadShared) == 24, "pad layout");
constexpr uint32_t kPadMagic = 0x50554341;  // "ACUP"
static volatile PadShared* pad = nullptr;
static volatile LONG padStateReads = 0, padCapsReads = 0, padDeviceNotices = 0;
static bool padActive(DWORD user) { return pad && pad->magic == kPadMagic && pad->enabled && user == 0; }

using XGetStateFn = DWORD(WINAPI*)(DWORD, XINPUT_STATE*);
using XGetCapsFn = DWORD(WINAPI*)(DWORD, DWORD, XINPUT_CAPABILITIES*);
static XGetStateFn originalState[3];
static XGetCapsFn originalCaps[2];
template <int N> static DWORD WINAPI stateFake(DWORD user, XINPUT_STATE* state) {
    if (padActive(user) && state) {
        state->dwPacketNumber = pad->sequence;
        state->Gamepad = {pad->buttons, pad->leftTrigger, pad->rightTrigger, pad->leftX, pad->leftY, pad->rightX, pad->rightY};
        InterlockedIncrement(&padStateReads);
        return ERROR_SUCCESS;
    }
    return originalState[N] ? originalState[N](user, state) : ERROR_DEVICE_NOT_CONNECTED;
}
template <int N> static DWORD WINAPI capsFake(DWORD user, DWORD flags, XINPUT_CAPABILITIES* caps) {
    if (padActive(user) && caps) {
        memset(caps, 0, sizeof *caps);
        caps->Type = XINPUT_DEVTYPE_GAMEPAD;
        caps->SubType = XINPUT_DEVSUBTYPE_GAMEPAD;
        caps->Gamepad = {0xF3FF, 0xFF, 0xFF, -64, -64, -64, -64};
        caps->Vibration = {0xFFFF, 0xFFFF};
        InterlockedIncrement(&padCapsReads);
        return ERROR_SUCCESS;
    }
    return originalCaps[N] ? originalCaps[N](user, flags, caps) : ERROR_DEVICE_NOT_CONNECTED;
}

// GetCursorPos: original reached through a trampoline holding its first 5 bytes (`mov edx,1`).
static BOOL(WINAPI* originalGetCursorPos)(LPPOINT) = nullptr;
static POINT frozenCursor{};
static volatile LONG haveFrozenCursor = 0;
static BOOL WINAPI getCursorPosFake(LPPOINT point) {
    if (reallyFocused() || !point) {
        BOOL ok = originalGetCursorPos(point);
        if (ok && point) { frozenCursor = *point; InterlockedExchange(&haveFrozenCursor, 1); }
        return ok;
    }
    InterlockedIncrement(&frozenCursorReads);
    if (!haveFrozenCursor) return originalGetCursorPos(point);
    *point = frozenCursor;
    return TRUE;
}

// DirectInput device methods 9 (GetDeviceState) and 10 (GetDeviceData); same signature for A and W.
using GetDeviceStateFn = HRESULT(STDMETHODCALLTYPE*)(void*, DWORD, LPVOID);
using GetDeviceDataFn = HRESULT(STDMETHODCALLTYPE*)(void*, DWORD, LPDIDEVICEOBJECTDATA, LPDWORD, DWORD);
struct VtablePatch { void** table; GetDeviceStateFn state; GetDeviceDataFn data; bool active; };
static VtablePatch vtables[2];
static int vtableCount = 0;
static int vtableIndex(void* self_) {
    for (int i = 0; i < vtableCount; i++) if (*(void***)self_ == vtables[i].table) return i;
    return -1;
}
static HRESULT STDMETHODCALLTYPE getDeviceStateFake(void* device, DWORD size, LPVOID data) {
    int i = vtableIndex(device);
    if (i < 0) return DIERR_GENERIC;
    HRESULT hr = vtables[i].state(device, size, data);
    if (SUCCEEDED(hr) && data && !reallyFocused()) { memset(data, 0, size); InterlockedIncrement(&blankedDeviceState); }
    return hr;
}
static HRESULT STDMETHODCALLTYPE getDeviceDataFake(void* device, DWORD objectSize, LPDIDEVICEOBJECTDATA items,
                                                   LPDWORD count, DWORD flags) {
    int i = vtableIndex(device);
    if (i < 0) return DIERR_GENERIC;
    HRESULT hr = vtables[i].data(device, objectSize, items, count, flags);
    if (SUCCEEDED(hr) && count && *count && !reallyFocused()) { *count = 0; InterlockedIncrement(&blankedDeviceData); }
    return hr;
}

static LRESULT CALLBACK subclassProc(HWND hwnd, UINT message, WPARAM wParam, LPARAM lParam) {
    switch (message) {
    case WM_ACTIVATEAPP: if (!wParam) { InterlockedIncrement(&droppedActivateApp); return 0; } break;
    case WM_ACTIVATE: if (LOWORD(wParam) == WA_INACTIVE) { InterlockedIncrement(&droppedActivate); return 0; } break;
    case WM_KILLFOCUS: InterlockedIncrement(&droppedKillFocus); return 0;
    case WM_NCACTIVATE: if (!wParam) { InterlockedIncrement(&droppedNcActivate); return TRUE; } break;
    }
    return CallWindowProcW(originalProc, hwnd, message, wParam, lParam);
}

static bool writeCode(void* target, const void* bytes, size_t n) {
    DWORD old;
    if (!VirtualProtect(target, n, PAGE_EXECUTE_READWRITE, &old)) return false;
    memcpy(target, bytes, n);
    VirtualProtect(target, n, old, &old);
    FlushInstructionCache(GetCurrentProcess(), target, n);
    return true;
}

// Some user32 exports are 6-byte forwarding stubs, so only 5 bytes are overwritten: a rel32
// `jmp` to a trampoline page allocated within +-2 GiB, which holds `jmp qword [rip+0]; dq target`.
static BYTE* allocateNear(void* target) {
    SYSTEM_INFO si;
    GetSystemInfo(&si);
    const uintptr_t step = si.dwAllocationGranularity, origin = (uintptr_t)target & ~(step - 1);
    for (uintptr_t distance = step; distance < 0x7FF00000; distance += step)
        for (uintptr_t candidate : {origin - distance, origin + distance})
            if (BYTE* page = (BYTE*)VirtualAlloc((void*)candidate, si.dwPageSize, MEM_RESERVE | MEM_COMMIT, PAGE_EXECUTE_READWRITE))
                return page;
    return nullptr;
}

// Returns the trampoline page (14 bytes used; the rest is free for an original-call stub).
static BYTE* patchExport(const char* module, const char* name, void* replacement, const BYTE* requiredPrefix = nullptr,
                         LPCSTR proc = nullptr) {
    HMODULE m = GetModuleHandleA(module);
    void* target = m ? (void*)GetProcAddress(m, proc ? proc : name) : nullptr;
    if (!target || patchCount >= 16) return nullptr;
    for (int i = 0; i < patchCount; i++) if (patches[i].target == target) return nullptr;  // forwarded alias
    if (requiredPrefix && memcmp(target, requiredPrefix, 5)) return nullptr;
    BYTE* trampoline = allocateNear(target);
    if (!trampoline) return nullptr;
    BYTE absolute[14] = {0xFF, 0x25, 0, 0, 0, 0};
    memcpy(absolute + 6, &replacement, 8);
    memcpy(trampoline, absolute, sizeof absolute);
    FlushInstructionCache(GetCurrentProcess(), trampoline, sizeof absolute);
    Patch& p = patches[patchCount];
    p = {module, name, target, {}, false};
    memcpy(p.saved, target, sizeof p.saved);
    BYTE jump[5] = {0xE9};
    int32_t rel = (int32_t)((intptr_t)trampoline - ((intptr_t)target + 5));
    memcpy(jump + 1, &rel, 4);
    p.active = writeCode(target, jump, sizeof jump);
    patchCount++;
    return p.active ? trampoline : nullptr;
}

// Resolve the real foreground query: user32's export is `jmp qword [rip+rel]` to win32u.
static void resolveRealForeground() {
    HMODULE user = GetModuleHandleA("user32.dll");
    const BYTE* stub = user ? (const BYTE*)GetProcAddress(user, "GetForegroundWindow") : nullptr;
    if (stub && stub[0] == 0xFF && stub[1] == 0x25) {
        int32_t rel;
        memcpy(&rel, stub + 2, 4);
        realForeground = *(HWND(WINAPI**)())(stub + 6 + rel);
    }
    HMODULE win32u = GetModuleHandleA("win32u.dll");
    if (!realForeground && win32u) realForeground = (HWND(WINAPI*)())GetProcAddress(win32u, "NtUserGetForegroundWindow");
}

// Hook an export and return a callable original. Only prologues whose first instruction is exactly
// 5 position-independent bytes are accepted: the original-call stub replays them, then jumps to +5.
static void* hookWithOriginal(const char* module, const char* label, LPCSTR proc, void* replacement) {
    static const BYTE accepted[][5] = {
        {0xBA, 0x01, 0x00, 0x00, 0x00},  // mov edx, 1            (user32!GetCursorPos)
        {0x48, 0x89, 0x5C, 0x24, 0x08},  // mov [rsp+8], rbx      (XInputGetState and friends)
        {0x48, 0x89, 0x5C, 0x24, 0x20},  // mov [rsp+0x20], rbx   (xinput1_4!XInputGetCapabilities)
    };
    HMODULE m = GetModuleHandleA(module);
    BYTE* target = m ? (BYTE*)GetProcAddress(m, proc) : nullptr;
    if (!target) return nullptr;
    const BYTE* prefix = nullptr;
    for (const auto& a : accepted) if (!memcmp(target, a, 5)) prefix = a;
    BYTE* trampoline = prefix ? patchExport(module, label, replacement, prefix, proc) : nullptr;
    if (!trampoline) return nullptr;
    BYTE* original = trampoline + 32;
    memcpy(original, prefix, 5);
    BYTE back[14] = {0xFF, 0x25, 0, 0, 0, 0};
    void* resume = target + 5;
    memcpy(back + 6, &resume, 8);
    memcpy(original + 5, back, sizeof back);
    FlushInstructionCache(GetCurrentProcess(), original, 19);
    return original;
}

static void patchGetCursorPos() {
    originalGetCursorPos = (BOOL(WINAPI*)(LPPOINT))hookWithOriginal("user32.dll", "GetCursorPos", "GetCursorPos", (void*)&getCursorPosFake);
}

static void patchXInputPad() {
    originalState[0] = (XGetStateFn)hookWithOriginal("xinput1_3.dll", "XInputGetState", "XInputGetState", (void*)&stateFake<0>);
    originalState[1] = (XGetStateFn)hookWithOriginal("xinput1_3.dll", "XInputGetStateEx", (LPCSTR)(uintptr_t)100, (void*)&stateFake<1>);
    originalState[2] = (XGetStateFn)hookWithOriginal("xinput1_4.dll", "XInputGetState", "XInputGetState", (void*)&stateFake<2>);
    originalCaps[0] = (XGetCapsFn)hookWithOriginal("xinput1_3.dll", "XInputGetCapabilities", "XInputGetCapabilities", (void*)&capsFake<0>);
    originalCaps[1] = (XGetCapsFn)hookWithOriginal("xinput1_4.dll", "XInputGetCapabilities", "XInputGetCapabilities", (void*)&capsFake<1>);
}

static void patchVtable(void** table) {
    if (vtableCount >= 2) return;
    for (int i = 0; i < vtableCount; i++) if (vtables[i].table == table) return;
    VtablePatch& v = vtables[vtableCount];
    v = {table, (GetDeviceStateFn)table[9], (GetDeviceDataFn)table[10], false};
    vtableCount++;  // publish before the slots point at the fakes, which look tables up
    void* fakes[2] = {(void*)&getDeviceStateFake, (void*)&getDeviceDataFake};
    DWORD old;
    if (VirtualProtect(table + 9, 2 * sizeof(void*), PAGE_READWRITE, &old)) {
        memcpy(table + 9, fakes, sizeof fakes);
        VirtualProtect(table + 9, 2 * sizeof(void*), old, &old);
        v.active = true;
    }
}

// DirectInput 8 device objects share one vtable per character set; find them with a throwaway
// mouse device from the game's own dinput8.dll.
static void patchDirectInput() {
    HMODULE di = GetModuleHandleA("dinput8.dll");
    auto create = di ? (HRESULT(WINAPI*)(HINSTANCE, DWORD, REFIID, LPVOID*, LPUNKNOWN))GetProcAddress(di, "DirectInput8Create") : nullptr;
    if (!create) return;
    IDirectInput8W* w = nullptr;
    if (SUCCEEDED(create(self, DIRECTINPUT_VERSION, IID_IDirectInput8W, (LPVOID*)&w, nullptr)) && w) {
        IDirectInputDevice8W* device = nullptr;
        if (SUCCEEDED(w->CreateDevice(GUID_SysMouse, &device, nullptr)) && device) { patchVtable(*(void***)device); device->Release(); }
        w->Release();
    }
    IDirectInput8A* a = nullptr;
    if (SUCCEEDED(create(self, DIRECTINPUT_VERSION, IID_IDirectInput8A, (LPVOID*)&a, nullptr)) && a) {
        IDirectInputDevice8A* device = nullptr;
        if (SUCCEEDED(a->CreateDevice(GUID_SysMouse, &device, nullptr)) && device) { patchVtable(*(void***)device); device->Release(); }
        a->Release();
    }
}

static std::wstring besideDll(const wchar_t* name) {
    wchar_t path[MAX_PATH];
    DWORD n = GetModuleFileNameW(self, path, MAX_PATH);
    std::wstring s(path, n);
    return s.substr(0, s.find_last_of(L"\\/") + 1) + name;
}

static void patchPresent() {
    fpsCap = GetPrivateProfileIntW(L"focus", L"fps_cap", 0, besideDll(L"acu_focus.ini").c_str());
    if (fpsCap > 240) fpsCap = 240;
    QueryPerformanceFrequency(&qpcFrequency);
    HMODULE d3d = GetModuleHandleA("d3d11.dll");
    auto create = d3d ? (decltype(&D3D11CreateDeviceAndSwapChain))GetProcAddress(d3d, "D3D11CreateDeviceAndSwapChain") : nullptr;
    if (!create) return;
    HWND dummy = CreateWindowExW(0, L"STATIC", L"acu-focus-dummy", WS_OVERLAPPED, 0, 0, 64, 64, nullptr, nullptr, self, nullptr);
    if (!dummy) return;
    DXGI_SWAP_CHAIN_DESC desc{};
    desc.BufferCount = 1;
    desc.BufferDesc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    desc.BufferDesc.Width = desc.BufferDesc.Height = 64;
    desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    desc.OutputWindow = dummy;
    desc.SampleDesc.Count = 1;
    desc.Windowed = TRUE;
    IDXGISwapChain* chain = nullptr;
    ID3D11Device* device = nullptr;
    ID3D11DeviceContext* context = nullptr;
    if (SUCCEEDED(create(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, 0, nullptr, 0, D3D11_SDK_VERSION, &desc, &chain,
                         &device, nullptr, &context)) && chain) {
        void** table = *(void***)chain;
        DWORD old;
        if (VirtualProtect(table + 8, sizeof(void*), PAGE_READWRITE, &old)) {
            presentTable = table;
            originalPresent = (PresentFn)table[8];
            table[8] = (void*)&presentFake;
            VirtualProtect(table + 8, sizeof(void*), old, &old);
            presentPatched = true;
        }
    }
    if (chain) chain->Release();
    if (context) context->Release();
    if (device) device->Release();
    DestroyWindow(dummy);
}

static void restorePresent() {
    DWORD old;
    if (presentPatched && VirtualProtect(presentTable + 8, sizeof(void*), PAGE_READWRITE, &old)) {
        presentTable[8] = (void*)originalPresent;
        VirtualProtect(presentTable + 8, sizeof(void*), old, &old);
        presentPatched = false;
    }
}

static BOOL CALLBACK findWindow(HWND hwnd, LPARAM) {
    DWORD pid = 0;
    GetWindowThreadProcessId(hwnd, &pid);
    if (pid != GetCurrentProcessId() || GetWindow(hwnd, GW_OWNER)) return TRUE;
    // The engine's top-level window class; it may be minimised (0x0 client area) when found.
    wchar_t cls[64];
    if (!GetClassNameW(hwnd, cls, 64) || wcscmp(cls, L"ScimitarEngineWindowClass")) return TRUE;
    gameWindow = hwnd;
    return FALSE;
}

static std::wstring statusPath() {
    wchar_t path[MAX_PATH];
    DWORD n = GetModuleFileNameW(self, path, MAX_PATH);
    std::wstring s(path, n);
    s = s.substr(0, s.find_last_of(L"\\/") + 1);
    return s + L"focus-" + std::to_wstring(GetCurrentProcessId()) + L".json";
}

static void writeStatus(const char* state, const char* error) {
    std::string s = "{\"pid\":" + std::to_string(GetCurrentProcessId()) + ",\"state\":\"" + state + "\"";
    char buf[512];
    sprintf_s(buf, ",\"window\":\"%016llx\",\"subclassed\":%s,\"tick_ms\":%llu", (unsigned long long)(uintptr_t)gameWindow,
              originalProc ? "true" : "false", (unsigned long long)GetTickCount64());
    s += buf;
    sprintf_s(buf, ",\"dropped\":{\"activateapp\":%ld,\"activate\":%ld,\"killfocus\":%ld,\"ncactivate\":%ld},\"ignored_xinput_disable\":%ld",
              droppedActivateApp, droppedActivate, droppedKillFocus, droppedNcActivate, ignoredXInputDisable);
    s += buf;
    int dinputActive = 0;
    for (int i = 0; i < vtableCount; i++) dinputActive += vtables[i].active;
    sprintf_s(buf, ",\"really_focused\":%s,\"real_foreground_resolved\":%s,\"cursor_hook\":%s,\"dinput_vtables\":%d,"
              "\"neutralised\":{\"cursor_reads\":%ld,\"device_state\":%ld,\"device_data\":%ld}",
              reallyFocused() ? "true" : "false", realForeground ? "true" : "false", originalGetCursorPos ? "true" : "false",
              dinputActive, frozenCursorReads, blankedDeviceState, blankedDeviceData);
    s += buf;
    sprintf_s(buf, ",\"frame\":{\"present_hooked\":%s,\"fps_cap\":%u,\"fps\":%ld,\"presents\":%ld}",
              presentPatched ? "true" : "false", fpsCap, measuredFps, presents);
    s += buf;
    sprintf_s(buf, ",\"pad\":{\"mapped\":%s,\"enabled\":%s,\"sequence\":%lu,\"state_reads\":%ld,\"caps_reads\":%ld,\"device_notices\":%ld}",
              pad ? "true" : "false", padActive(0) ? "true" : "false", pad ? (unsigned long)pad->sequence : 0ul,
              padStateReads, padCapsReads, padDeviceNotices);
    s += buf;
    s += ",\"patches\":[";
    for (int i = 0; i < patchCount; i++) {
        sprintf_s(buf, "%s{\"module\":\"%s\",\"name\":\"%s\",\"address\":\"%016llx\",\"active\":%s}", i ? "," : "",
                  patches[i].module, patches[i].name, (unsigned long long)(uintptr_t)patches[i].target,
                  patches[i].active ? "true" : "false");
        s += buf;
    }
    s += "]";
    if (error) s += std::string(",\"error\":\"") + error + "\"";
    s += "}\n";
    std::wstring path = statusPath(), temp = path + L".tmp";
    HANDLE f = CreateFileW(temp.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_ALWAYS, 0, nullptr);
    if (f == INVALID_HANDLE_VALUE) return;
    DWORD written;
    WriteFile(f, s.data(), (DWORD)s.size(), &written, nullptr);
    CloseHandle(f);
    MoveFileExW(temp.c_str(), path.c_str(), MOVEFILE_REPLACE_EXISTING);
}

static DWORD WINAPI worker(void*) {
    wchar_t eventName[64];
    swprintf_s(eventName, L"Local\\acu-focus-stop-%lu", GetCurrentProcessId());
    HANDLE stop = CreateEventW(nullptr, TRUE, FALSE, eventName);
    // A manual-reset event outlives an earlier shim in this process; clear a stale stop request.
    if (stop) ResetEvent(stop);
    for (int i = 0; i < 600 && !gameWindow; i++) { EnumWindows(findWindow, 0); if (!gameWindow) Sleep(100); }
    if (!gameWindow) { writeStatus("failed", "game window not found"); return 1; }
    // Re-enable XInput through the original export before it is patched.
    for (const char* dll : {"xinput1_3.dll", "xinput1_4.dll"}) {
        HMODULE m = GetModuleHandleA(dll);
        auto enable = m ? (void(WINAPI*)(BOOL))GetProcAddress(m, "XInputEnable") : nullptr;
        if (enable) enable(TRUE);
    }
    resolveRealForeground();
    if (!realForeground) { writeStatus("failed", "real foreground query not resolved"); return 1; }
    wchar_t padName[64];
    swprintf_s(padName, L"Local\\acu-pad-%lu", GetCurrentProcessId());
    if (HANDLE section = CreateFileMappingW(INVALID_HANDLE_VALUE, nullptr, PAGE_READWRITE, 0, sizeof(PadShared), padName))
        pad = (volatile PadShared*)MapViewOfFile(section, FILE_MAP_ALL_ACCESS, 0, 0, sizeof(PadShared));
    patchGetCursorPos();
    patchXInputPad();
    patchDirectInput();
    patchPresent();
    if (fpsCap) timeBeginPeriod(1);
    patchExport("user32.dll", "GetForegroundWindow", (void*)&gameWindowFake);
    patchExport("user32.dll", "GetActiveWindow", (void*)&gameWindowFake);
    patchExport("user32.dll", "GetFocus", (void*)&gameWindowFake);
    patchExport("xinput1_3.dll", "XInputEnable", (void*)&xinputEnableFake);
    patchExport("xinput1_4.dll", "XInputEnable", (void*)&xinputEnableFake);
    originalProc = (WNDPROC)SetWindowLongPtrW(gameWindow, GWLP_WNDPROC, (LONG_PTR)&subclassProc);
    if (!originalProc) { writeStatus("failed", "window subclass failed"); }
    else {
        // Tell the game it is active, in case it already paused before the shim arrived.
        SendMessageTimeoutW(gameWindow, WM_ACTIVATEAPP, TRUE, 0, SMTO_ABORTIFHUNG, 2000, nullptr);
        SendMessageTimeoutW(gameWindow, WM_ACTIVATE, WA_ACTIVE, 0, SMTO_ABORTIFHUNG, 2000, nullptr);
        SendMessageTimeoutW(gameWindow, WM_SETFOCUS, 0, 0, SMTO_ABORTIFHUNG, 2000, nullptr);
    }
    bool padWasActive = false;
    for (unsigned tick = 0; stop && WaitForSingleObject(stop, 100) == WAIT_TIMEOUT; tick++) {
        // Announce pad arrival/removal the way a real device would, so the game re-enumerates.
        bool active = padActive(0);
        if (active != padWasActive) {
            PostMessageW(gameWindow, WM_DEVICECHANGE, 0x0007 /* DBT_DEVNODES_CHANGED */, 0);
            InterlockedIncrement(&padDeviceNotices);
            padWasActive = active;
        }
        if (tick % 10 == 0) {
            static LONG lastPresents = 0;
            LONG now = presents;
            measuredFps = now - lastPresents;
            lastPresents = now;
            writeStatus("installed", nullptr);
        }
    }
    restorePresent();
    if (fpsCap) timeEndPeriod(1);
    // Restore: only put the old procedure back if ours is still the installed one.
    if (originalProc && (WNDPROC)GetWindowLongPtrW(gameWindow, GWLP_WNDPROC) == &subclassProc)
        SetWindowLongPtrW(gameWindow, GWLP_WNDPROC, (LONG_PTR)originalProc);
    for (int i = 0; i < patchCount; i++)
        if (patches[i].active) patches[i].active = !writeCode(patches[i].target, patches[i].saved, sizeof patches[i].saved);
    for (int i = 0; i < vtableCount; i++) {
        VtablePatch& v = vtables[i];
        DWORD old;
        if (v.active && VirtualProtect(v.table + 9, 2 * sizeof(void*), PAGE_READWRITE, &old)) {
            v.table[9] = (void*)v.state;
            v.table[10] = (void*)v.data;
            VirtualProtect(v.table + 9, 2 * sizeof(void*), old, &old);
            v.active = false;
        }
    }
    writeStatus("removed", nullptr);
    // The DLL stays loaded: a window message may still be executing subclassProc on the game thread.
    return 0;
}

BOOL WINAPI DllMain(HINSTANCE h, DWORD reason, void*) {
    if (reason == DLL_PROCESS_ATTACH) {
        self = h;
        DisableThreadLibraryCalls(h);
        HANDLE t = CreateThread(nullptr, 0, worker, nullptr, 0, nullptr);
        if (!t) return FALSE;
        CloseHandle(t);
    }
    return TRUE;
}
