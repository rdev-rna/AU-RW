// Background-focus shim for the original ACU process. Injected with acu_probe_launcher --attach.
// Keeps the game running and reading the controller while another window has focus:
//  - subclasses the game's top-level window and drops deactivation messages;
//  - GetForegroundWindow/GetActiveWindow/GetFocus return the game window inside this process;
//  - XInputEnable(FALSE) is ignored (XInputEnable(TRUE) is issued once at install).
// While the game is *really* in the background (original foreground query), mouse and keyboard
// are neutralised so the user's desktop work never reaches the game: GetCursorPos returns the
// last focused position, and DirectInput GetDeviceState/GetDeviceData report no input. XInput
// (the virtual controller) is untouched. With the game really focused, everything passes through,
// unless acu_focus.ini sets [focus] block_user_input=1: then the user's input is neutralised all
// the time, the game's ClipCursor/SetCursorPos/ShowCursor(FALSE) calls are dropped, raw-input,
// mouse and keyboard window messages go to DefWindowProc, any existing clip is released, and
// with return_focus=<hwnd hex> the foreground is handed back to that window at install.
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
#include <cmath>
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
// [focus] block_user_input=1: the user's mouse and keyboard never reach the game, even while the
// game really has the foreground (for example right after launch, or when the user clicks it).
static bool blockUserInput = false;
static bool blockUser() { return blockUserInput || !reallyFocused(); }
static volatile LONG droppedClips = 0, droppedCursorSets = 0, droppedCursorHides = 0, droppedInputMessages = 0;
static bool returnedFocus = false;
static HWND returnFocusTo = nullptr;
static volatile LONG focusReturns = 0;
constexpr UINT kShowCursorMessage = WM_APP + 0x41;  // handled by subclassProc on the game thread
static int(WINAPI* realShowCursor)(BOOL) = nullptr;

// Teleport request (ACU-RE-0046), written by tools/native_probe/teleport.py into the shared
// section Localcu-teleport-<pid>. The debug menu's own "Teleport Character" (0x140EE11B0) needs
// interface 13 of the Human, which retail does not provide (live: null). Instead, on the game
// window's thread inside SEH: copy the requested 4x4 matrix (rotation rows, position row, w = 1)
// over the player entity's world matrix (+0x20), then call the Human's behaviour reset
// (RVA 0x131A530; ACUFixes lead "Human__Dispatch_HSTE_ResetBehavior", re-derived in ACU-RE-0046)
// so the character restarts from the new transform. The Human must carry the vtable at
// RVA 0x2FB2010 and the entity must point back to a world (+0x60); nothing else is called.
// mode 1 (Invisible, ACU-RE-0046): the debug menu's "Invisible :" (0x140EF6DD0) sets byte +208 of the
// player's behaviour component (vtable RVA 0x2F08230, typed by 0x140B9CD00) through 0x141EC93F0,
// which notifies (vtbl+648) first; game systems call the same setter. human = component,
// entity = its owner, matrix[0] = value. mode 2 (God Mode, debug item 0x140EF7510): handle =
// [0x141CCFAC0(entity) + 184]; 0x141212680(&handle, value); matrix[1] = read back through
// 0x140CFBDA0(handle), as the menu displays it.
// mode 3 (world travel, ACU-RE-0049): the portal/fast-travel entry's last step 0x141E42C00 ends with
// 0x141D60C90([0x145219860], &entry->worldHandle (+72), &entry->destinationHandle (+80)). human = the
// entry; entity = the world file ID its world handle must carry at handle+16 (checked first).
// mode 4 (load world, ACU-RE-0049): the same call the travel manager makes after the world switch,
// 0x141CD8A00(worldFileId, 1, 0, 6, destinationFileId, 1) -> 0x141CD8A40 queues command 8 in the
// World holder (handles built from the file IDs by 0x1426EB640 / 0x1426EAF00; the push takes the
// holder's lock; the queue drains on the game's own thread). entity = world ID, human = destination.
// mode 5 (start world, ACU-RE-0049): 0x141D5E520(world, &startHandle) picks the start (a portal in
// the world's list at +312/+322, else its default +304), stores it at +328 and calls 0x141D68080,
// which hands the start's transform to the world's spawner (+1976). The travel manager's update
// 0x141D57AD0 makes this call after a world load. entity = world (must be the current World
// [[0x145217368]+0x98]); the start handle is the engine's null handle (RVA 0x525BB58) = default.
struct TeleportShared { uint32_t magic, request, done; int32_t status; uint64_t human, entity; float matrix[16];
                        uint32_t exception, mode; };
static_assert(sizeof(TeleportShared) == 104, "teleport layout");  // '<IIIiQQ16fII'
constexpr uint32_t kTeleportMagic = 0x54554341;  // "ACUT"
constexpr UINT kTeleportMessage = WM_APP + 0x42;
constexpr uintptr_t kHumanVtableRva = 0x2FB2010, kHumanResetRva = 0x131A530, kEntityMatrix = 0x20, kEntityWorld = 0x60;
constexpr uintptr_t kBehaviourVtableRva = 0x2F08230, kSetInvisibleRva = 0x1EC93F0, kEntityDataRva = 0x1CCFAC0,
                    kSetGodModeRva = 0x1212680, kGodModeStateRva = 0x0CFBDA0,
                    kTravelManagerRva = 0x5219860, kTravelRva = 0x1D60C90, kLoadWorldRva = 0x1CD8A00,
                    kStartWorldRva = 0x1D5E520, kWorldHolderRva = 0x5217368, kNullHandleRva = 0x525BB58;
static volatile TeleportShared* teleport = nullptr;
static volatile LONG teleports = 0;
static uintptr_t imageLow = 0, imageHigh = 0;
static bool readable(uintptr_t p, size_t n) {
    MEMORY_BASIC_INFORMATION m{};
    return p && VirtualQuery((void*)p, &m, sizeof m) && m.State == MEM_COMMIT &&
           !(m.Protect & (PAGE_NOACCESS | PAGE_GUARD)) && p + n <= (uintptr_t)m.BaseAddress + m.RegionSize;
}
// status: 1 moved and reset, -1 not the Human, -2 not an entity, -3 exception
// (all modes: -5 travel in progress, -6 not the current World's player; see playerGuard).
static int runTeleport(uintptr_t human, uintptr_t entity, const float* matrix, uint32_t* exception) {
    if (!readable(human, 8) || *(uintptr_t*)human != imageLow + kHumanVtableRva) return -1;
    if (!readable(entity + kEntityMatrix, 64) || !readable(entity + kEntityWorld, 8) || !*(uintptr_t*)(entity + kEntityWorld))
        return -2;
    __try {
        memcpy((void*)(entity + kEntityMatrix), matrix, 64);
        ((char(__fastcall*)(uintptr_t))(imageLow + kHumanResetRva))(human);
        return 1;
    } __except (*exception = GetExceptionCode(), EXCEPTION_EXECUTE_HANDLER) {
        return -3;
    }
}
// status: 1 done, -1 not the behaviour component / no data, -2 not an entity, -3 exception.
static int runFlag(uint32_t mode, uintptr_t component, uintptr_t entity, bool value, float* readback, uint32_t* exception) {
    if (!readable(entity + kEntityWorld, 8) || !*(uintptr_t*)(entity + kEntityWorld)) return -2;
    __try {
        if (mode == 1) {
            if (!readable(component + 0x10, 8) || *(uintptr_t*)component != imageLow + kBehaviourVtableRva ||
                *(uintptr_t*)(component + 0x10) != entity) return -1;
            ((uintptr_t(__fastcall*)(uintptr_t, char))(imageLow + kSetInvisibleRva))(component, value);
            *readback = *(uint8_t*)(component + 208);
            return 1;
        }
        uintptr_t data = ((uintptr_t(__fastcall*)(uintptr_t))(imageLow + kEntityDataRva))(entity);
        if (!readable(data + 184, 8) || !*(uintptr_t*)(data + 184)) return -1;
        uintptr_t handle = *(uintptr_t*)(data + 184);
        ((uintptr_t(__fastcall*)(uintptr_t*, uint8_t))(imageLow + kSetGodModeRva))(&handle, value);
        *readback = ((char(__fastcall*)(uintptr_t))(imageLow + kGodModeStateRva))(*(uintptr_t*)(data + 184));
        return 1;
    } __except (*exception = GetExceptionCode(), EXCEPTION_EXECUTE_HANDLER) {
        return -3;
    }
}
// status: 1 queued, -1 entry/handles invalid or world ID mismatch, -2 no travel manager, -3 exception.
static int runTravel(uintptr_t entry, uint64_t worldId, uint32_t* exception) {
    if (!readable(entry + 72, 16)) return -1;
    uintptr_t world = *(uintptr_t*)(entry + 72), dest = *(uintptr_t*)(entry + 80);
    if (!readable(world + 16, 8) || !readable(dest + 16, 8) || *(uint64_t*)(world + 16) != worldId) return -1;
    uintptr_t manager = *(uintptr_t*)(imageLow + kTravelManagerRva);
    if (!readable(manager + 648, 8)) return -2;
    __try {
        ((double(__fastcall*)(uintptr_t, uintptr_t, uintptr_t))(imageLow + kTravelRva))(manager, entry + 72, entry + 80);
        return 1;
    } __except (*exception = GetExceptionCode(), EXCEPTION_EXECUTE_HANDLER) {
        return -3;
    }
}
static int runLoadWorld(uint64_t worldId, uint64_t destinationId, uint32_t* exception) {
    if (!worldId || worldId >> 48) return -1;
    __try {
        ((double(__fastcall*)(uint64_t, uint8_t, uint64_t, int, uint64_t, uint8_t))(imageLow + kLoadWorldRva))(
            worldId, 1, 0, 6, destinationId, 1);
        return 1;
    } __except (*exception = GetExceptionCode(), EXCEPTION_EXECUTE_HANDLER) {
        return -3;
    }
}
static int runStartWorld(uintptr_t world, uint32_t* exception) {
    uintptr_t holder = *(uintptr_t*)(imageLow + kWorldHolderRva);
    if (!readable(holder + 0x98, 8) || *(uintptr_t*)(holder + 0x98) != world || !readable(world + 2656, 1)) return -1;
    if (*(uint8_t*)(world + 2656)) return -2;  // the routine itself does nothing then
    __try {
        uintptr_t start = imageLow + kNullHandleRva;
        ((uintptr_t(__fastcall*)(uintptr_t, uintptr_t*))(imageLow + kStartWorldRva))(world, &start);
        return 1;
    } __except (*exception = GetExceptionCode(), EXCEPTION_EXECUTE_HANDLER) {
        return -3;
    }
}
// mode 8 (spawn a static collision box, ACU-RE-0103): the generic runtime entity spawn 0x141FAFDE0
// (allocate 368 B, 0x141CB9AA0 construct, 0x141CE9100 init, 0x141CC00A0 attach component,
// 0x141D40250 World add, entity vtbl+248 activate; caller 0x141FAFE80 then sets the matrix through
// 0x1420F7510) with the component PhysicsWorldComponent::vf16 (0x142613F80) builds: an
// InertComponent (304 B, 0x1425EA110; its constructor sets the RigidBody (+0x50) motion field +0x58
// to 0) whose ObjectShape is set by 0x1425DEEC0. The shape is a BoxShape (128 B, 0x1425C3600) with
// half extents set through 0x1425DDAF0 (as 0x141DA0D30 does; vtbl+168 rebuilds the Havok box) and
// a handle from 0x1426EB640(0x14525BB70, 0, box). Memory: 0x1426D9A10(size, align,
// 0x1426F3760(size, align, [0x1452585D0])). matrix[0..2] = half extents, matrix[3..5] = centre,
// matrix[6] = yaw (radians); the box is world-aligned otherwise. The entity is never freed (one
// extra handle reference is taken). Returns entity in t->entity, component in t->human;
// matrix[8..11] = raw qwords RigidBody +0x98 and +0xA0 after activation (body pointers, if any).
// status: 1 spawned, -1 no World, -7 vtable check failed after construction (nothing added), -3 exception.
constexpr uintptr_t kAllocHeapRva = 0x26F3760, kAllocRva = 0x26D9A10, kDefaultHeapRva = 0x52585D0,
                    kBoxCtorRva = 0x25C3600, kBoxVtableRva = 0x361ECF0, kBoxSetExtentsRva = 0x25DDAF0,
                    kMakeHandleRva = 0x26EB640, kHandleTableRva = 0x525BB70, kInertCtorRva = 0x25EA110,
                    kInertVtableRva = 0x36238F0, kSetShapeRva = 0x25DEEC0, kEntityCtorRva = 0x1CB9AA0,
                    kEntityInitRva = 0x1CE9100, kAttachRva = 0x1CC00A0, kWorldAddRva = 0x1D40250,
                    kSetMatrixRva = 0x20F7510;
static uintptr_t gameAlloc(size_t size, size_t align) {
    uintptr_t heap = ((uintptr_t(__fastcall*)(size_t, size_t, uintptr_t))(imageLow + kAllocHeapRva))(
        size, align, *(uintptr_t*)(imageLow + kDefaultHeapRva));
    return ((uintptr_t(__fastcall*)(size_t, size_t, uintptr_t))(imageLow + kAllocRva))(size, align, heap);
}
static int runSpawnBox(const float* in, uint64_t* entityOut, uint64_t* componentOut, uint64_t* bodies, uint32_t* exception) {
    __try {
        uintptr_t holder = *(uintptr_t*)(imageLow + kWorldHolderRva);
        if (!readable(holder + 0x98, 8)) return -1;
        uintptr_t world = *(uintptr_t*)(holder + 0x98);
        if (!world) return -1;
        alignas(16) float extents[4] = {in[0], in[1], in[2], 0.0f};
        uintptr_t box = gameAlloc(128, 16);
        if (!box) return -1;
        ((uintptr_t(__fastcall*)(uintptr_t))(imageLow + kBoxCtorRva))(box);
        if (*(uintptr_t*)box != imageLow + kBoxVtableRva) return -7;
        ((uintptr_t(__fastcall*)(uintptr_t, float*))(imageLow + kBoxSetExtentsRva))(box, extents);
        uintptr_t handle = ((uintptr_t(__fastcall*)(uintptr_t, uintptr_t, uintptr_t))(imageLow + kMakeHandleRva))(
            imageLow + kHandleTableRva, 0, box);
        uintptr_t component = gameAlloc(304, 16);
        if (!component) return -1;
        ((uintptr_t(__fastcall*)(uintptr_t))(imageLow + kInertCtorRva))(component);
        if (*(uintptr_t*)component != imageLow + kInertVtableRva) return -7;
        ((double(__fastcall*)(uintptr_t, uintptr_t*))(imageLow + kSetShapeRva))(component + 0x50, &handle);
        uintptr_t entity = gameAlloc(368, 16);
        if (!entity) return -1;
        ((uintptr_t(__fastcall*)(uintptr_t))(imageLow + kEntityCtorRva))(entity);
        ((void(__fastcall*)(uintptr_t))(imageLow + kEntityInitRva))(entity);
        float c = cosf(in[6]), s = sinf(in[6]);
        alignas(16) float matrix[16] = {c, s, 0, 0, -s, c, 0, 0, 0, 0, 1, 0, in[3], in[4], in[5], 1};
        memcpy((void*)(entity + kEntityMatrix), matrix, 64);
        ((double(__fastcall*)(uintptr_t, uintptr_t))(imageLow + kAttachRva))(entity, component);
        ((uintptr_t(__fastcall*)(uintptr_t, uintptr_t))(imageLow + kWorldAddRva))(world, entity);
        (*(void(__fastcall**)(uintptr_t, uintptr_t))(*(uintptr_t*)entity + 248))(entity, 0);
        ((char(__fastcall*)(uintptr_t, float*))(imageLow + kSetMatrixRva))(entity, matrix);
        uintptr_t own = *(uintptr_t*)(entity + 304);  // the entity's own handle; keep it alive
        if (readable(own + 12, 4)) InterlockedIncrement((volatile LONG*)(own + 12));
        *entityOut = entity;
        *componentOut = component;
        bodies[0] = *(uint64_t*)(component + 0x50 + 0x98);
        bodies[1] = *(uint64_t*)(component + 0x50 + 0xA0);
        return 1;
    } __except (*exception = GetExceptionCode(), EXCEPTION_EXECUTE_HANDLER) {
        return -3;
    }
}
// mode 9 (move a world entity, ACU-RE-0103): 0x1420F7510(entity, matrix), the call 0x141FAFE80
// makes after a spawn. Without a parent (+0x18) it is Entity vtbl+128, 0x141CE1FF0: it stores the
// matrix at +0x20 and calls every component's vtbl+560 with it (InertComponent: 0x14262CBF0 ->
// 0x1425DE420, which updates the RigidBody's registered body). entity = the entity, matrix = the
// new 4x4 world matrix (rotation rows, position row, w = 1). The entity must point back to the
// current World (+0x60) and must not be the player (use mode 0 for Arno).
// status: 1 moved, -1 not an entity of the current World, -2 the player, -3 exception.
static int runMoveEntity(uintptr_t entity, const float* matrix, uint32_t* exception) {
    __try {
        uintptr_t holder = *(uintptr_t*)(imageLow + kWorldHolderRva);
        if (!readable(holder + 0x98, 8)) return -1;
        uintptr_t world = *(uintptr_t*)(holder + 0x98);
        if (!world || !readable(entity + kEntityWorld, 8) || *(uintptr_t*)(entity + kEntityWorld) != world ||
            !readable(entity + kEntityMatrix, 64) || !readable(world + 0xD8, 8)) return -1;
        if (*(uintptr_t*)(world + 0xD8) == entity) return -2;
        alignas(16) float m[16];
        memcpy(m, matrix, sizeof m);
        ((char(__fastcall*)(uintptr_t, float*))(imageLow + kSetMatrixRva))(entity, m);
        return 1;
    } __except (*exception = GetExceptionCode(), EXCEPTION_EXECUTE_HANDLER) {
        return -3;
    }
}

// mode 10 (clone a world entity, ACU-RE-0103; frame fix after obs. 15): EntityBuilder's instantiation 0x141CD0470 copies its
// Template entity with the object clone, vtbl+24 (Entity::vf3 0x141CC65D0: allocates and inits a new
// Entity, then 0x1426ECC50 clones every component through its own vtbl+24), then sets flags +136
// |= 0x800014, calls 0x141CE94F0(clone, 0) and sets +212 bit 0x40000 (clears 0x80000; the other
// branch of its build-context byte +28). The clone is then placed like mode 8's entity: matrix
// written at +0x20, World add 0x141D40250, activate vtbl+248, set matrix 0x1420F7510.
// entity = the source (an entity of the current World, not the player), matrix = the clone's world
// matrix, human bit 0 = skip the builder flags. Returns the clone in t->entity.
// status: 1 cloned, -1 source not an entity of the current World, -2 the player, -7 clone null or
// not an entity (same vtable as the source), -3 exception.
constexpr uintptr_t kEntityFlagsInitRva = 0x1CE94F0, kOpenMemoryFrameRva = 0x26E6F20, kCloseMemoryFrameRva = 0x26EEC40;
// Fault diagnostics for mode 10: the filter runs on the faulting stack before unwinding, so it can
// record the faulting address, the accessed address (access violations) and the in-image return
// addresses above it. Values are RVAs when inside ACU.exe, else absolute.
static uint64_t cloneFault[8];
static uint64_t imageRelative(uint64_t a) { return a >= imageLow && a < imageHigh ? a - imageLow : a; }
static int recordFault(EXCEPTION_POINTERS* p, uint32_t* code) {
    *code = p->ExceptionRecord->ExceptionCode;
    memset(cloneFault, 0, sizeof cloneFault);
    cloneFault[0] = imageRelative((uint64_t)p->ExceptionRecord->ExceptionAddress);
    if (p->ExceptionRecord->NumberParameters >= 2) cloneFault[1] = p->ExceptionRecord->ExceptionInformation[1];
    void* frames[48] = {};
    USHORT n = RtlCaptureStackBackTrace(0, 48, frames, nullptr);
    int k = 2;
    for (USHORT i = 0; i < n && k < 8; i++) {
        uint64_t a = (uint64_t)frames[i];
        if (a >= imageLow && a < imageHigh) cloneFault[k++] = a - imageLow;
    }
    return EXCEPTION_EXECUTE_HANDLER;
}
static int runCloneEntity(uintptr_t source, const float* matrix, uint64_t options, uint64_t* cloneOut, uint32_t* exception) {
    __try {
        uintptr_t holder = *(uintptr_t*)(imageLow + kWorldHolderRva);
        if (!readable(holder + 0x98, 8)) return -1;
        uintptr_t world = *(uintptr_t*)(holder + 0x98);
        if (!world || !readable(source + kEntityWorld, 8) || *(uintptr_t*)(source + kEntityWorld) != world ||
            !readable(source, 8) || !readable(world + 0xD8, 8)) return -1;
        if (*(uintptr_t*)(world + 0xD8) == source) return -2;
        uintptr_t vtable = *(uintptr_t*)source, clone = 0;
        // The clone allocates through the thread's engine memory context (TLS +104), whose frame
        // index (+2880) is -1 until a frame is opened: 0x1426E6F20(1, 0) opens one and 0x1426EEC40
        // closes it, the pair BreakableCompoundComponent's 0x140DB4490 wraps its clones in.
        ((uint32_t(__fastcall*)(char, char))(imageLow + kOpenMemoryFrameRva))(1, 0);
        __try {
            clone = (*(uintptr_t(__fastcall**)(uintptr_t, uintptr_t, uint32_t))(vtable + 24))(source, 0, 0);
        } __finally {
            ((uint64_t(__fastcall*)(uint64_t, uint64_t, uint64_t, uint64_t))(imageLow + kCloseMemoryFrameRva))(0, 0, 0, 0);
        }
        if (!clone || !readable(clone, 0x170) || *(uintptr_t*)clone != vtable) return -7;
        if (!(options & 1)) {
            *(uint64_t*)(clone + 136) |= 0x800014ull;
            ((void(__fastcall*)(uintptr_t, uintptr_t))(imageLow + kEntityFlagsInitRva))(clone, 0);
            *(uint32_t*)(clone + 212) = (*(uint32_t*)(clone + 212) & ~0x80000u) | 0x40000u;
        }
        alignas(16) float m[16];
        memcpy(m, matrix, sizeof m);
        memcpy((void*)(clone + kEntityMatrix), m, 64);
        ((uintptr_t(__fastcall*)(uintptr_t, uintptr_t))(imageLow + kWorldAddRva))(world, clone);
        (*(void(__fastcall**)(uintptr_t, uintptr_t))(*(uintptr_t*)clone + 248))(clone, 0);
        ((char(__fastcall*)(uintptr_t, float*))(imageLow + kSetMatrixRva))(clone, m);
        uintptr_t own = *(uintptr_t*)(clone + 304);  // keep the clone alive, as mode 8 does
        if (readable(own + 12, 4)) InterlockedIncrement((volatile LONG*)(own + 12));
        *cloneOut = clone;
        return 1;
    } __except (recordFault(GetExceptionInformation(), exception)) {
        return -3;
    }
}

// modes 6/7 (outfit, ACU-RE-0048/0070; lead: the ACUFixes plugin "Gear Visual Customization 365"
// 1.2.5, read statically, no code reused): the matrix bytes carry up to 8 u64 item file IDs, human =
// their count. Each AvatarGear is found as 0x140D2FFD0 does: AGM = [[ [0x1451B4C58]+0x1970 ]], three
// pointer arrays (+0x10 n +0x1A, +0x1C n +0x26, +0x28 n +0x32), keep gear g whose live block [g+0x10]
// has file ID [b+0x10]. mode 6 (visual only, nothing persisted): copy the active loadout
// (0x140D61560(PPM)), put each gear's block in the slot its type selects (0x140D33320: 0->+0,
// 1->+8, 2->+0x10, 4->+0x20, 3->+0x18), and run the visual selection 0x140D25540 on the copy (it
// reads the slots only; the next 0x140D527F0 restores the real loadout). mode 7 (persistent, the
// mission scripts' sequence): 0x140D2B240(AGM, g) per gear, then 0x140D527F0(PPM). entity returns
// the found mask (bits 0-7) and each gear's type in 4-bit fields from bit 8.
// status: n done, -1 no PPM/loadout, -2 no AGM, -7 not every ID found, -8 non-armour gear, -3 exception.
constexpr uintptr_t kPpmRva = 0x51B3C58, kAgmHolderRva = 0x51B4C58, kActiveLoadoutRva = 0x0D61560,
                    kSelectVisualsRva = 0x0D25540, kEquipGearRva = 0x0D2B240, kRefreshVisualsRva = 0x0D527F0;
static uintptr_t outfitLoadout[10];  // static: the selection must never see a dead stack copy
static bool liveBlock(uintptr_t b) { return readable(b, 0x18) && *(int32_t*)(b + 0xC) < 0 && *(uintptr_t*)b; }
static int runOutfit(uint32_t mode, const uint64_t* ids, uint32_t n, uint64_t* found, uint32_t* exception) {
    if (n == 0 || n > 8) return -1;
    __try {
        uintptr_t ppm = *(uintptr_t*)(imageLow + kPpmRva), holder = *(uintptr_t*)(imageLow + kAgmHolderRva);
        if (!readable(ppm + 0x1C8, 8) || !readable(holder + 0x1970, 8)) return -1;
        uintptr_t block = *(uintptr_t*)(holder + 0x1970);
        if (!liveBlock(block)) return -2;
        uintptr_t agm = *(uintptr_t*)block;
        if (!readable(agm, 0x34)) return -2;
        uintptr_t gears[8] = {};
        const uint32_t arrays[3][2] = {{0x10, 0x1A}, {0x1C, 0x26}, {0x28, 0x32}};
        for (const auto& a : arrays) {
            uintptr_t list = 0;
            memcpy(&list, (void*)(agm + a[0]), 8);
            uint16_t count = *(uint16_t*)(agm + a[1]);
            if (!count || !readable(list, 8ull * count)) continue;
            for (uint16_t k = 0; k < count; k++) {
                uintptr_t g = ((uintptr_t*)list)[k];
                if (!readable(g + 8, 0x10)) continue;
                uintptr_t b = *(uintptr_t*)(g + 0x10);
                if (!liveBlock(b)) continue;
                uint64_t id = *(uint64_t*)(b + 0x10);
                for (uint32_t i = 0; i < n; i++)
                    if (id == ids[i] && !gears[i]) gears[i] = g;
            }
        }
        uint64_t mask = 0;
        for (uint32_t i = 0; i < n; i++)
            if (gears[i]) mask |= (1ull << i) | ((uint64_t)(*(uint32_t*)(gears[i] + 8) & 0xF) << (8 + 4 * i));
        *found = mask;
        if ((mask & 0xFF) != (1ull << n) - 1) return -7;
        if (mode == 6) {
            uintptr_t loadout = ((uintptr_t(__fastcall*)(uintptr_t))(imageLow + kActiveLoadoutRva))(ppm);
            if (!readable(loadout, sizeof outfitLoadout)) return -1;
            memcpy(outfitLoadout, (void*)loadout, sizeof outfitLoadout);
            for (uint32_t i = 0; i < n; i++) {
                uint32_t type = *(uint32_t*)(gears[i] + 8);
                if (type > 4) return -8;
                uint32_t slot = type == 3 ? 3 : type;
                outfitLoadout[slot] = *(uintptr_t*)(gears[i] + 0x10);
            }
            ((double(__fastcall*)(uintptr_t*, uintptr_t, uintptr_t))(imageLow + kSelectVisualsRva))(outfitLoadout, 0, 0);
        } else {
            for (uint32_t i = 0; i < n; i++)
                ((double(__fastcall*)(uintptr_t, uintptr_t))(imageLow + kEquipGearRva))(agm, gears[i]);
            ((double(__fastcall*)(uintptr_t))(imageLow + kRefreshVisualsRva))(ppm);
        }
        return (int)n;
    } __except (*exception = GetExceptionCode(), EXCEPTION_EXECUTE_HANDLER) {
        return -3;
    }
}

// Travel guard (ACU-RE-0049 obs. 14/15): during a world change the holder's World is 0 for ~3 s and
// the new World has no player for ~15-20 s more; every arrival makes a new World and player entity,
// so a pointer taken before the trip is stale. -5: the travel manager [0x145219860] state +644 is
// not 0 (a travel is running). -6: the World chain does not name this entity as the current player
// (holder +0x98 -> World +0xD8 == entity and entity +0x60 == World). Read with SEH.
constexpr uintptr_t kTravelStateOffset = 644, kHolderWorld = 0x98, kWorldPlayer = 0xD8;
static int travelState() {
    uintptr_t manager = *(uintptr_t*)(imageLow + kTravelManagerRva);
    if (!readable(manager + kTravelStateOffset, 4)) return -1;
    return *(int32_t*)(manager + kTravelStateOffset);
}
static int playerGuard(uintptr_t entity) {
    __try {
        if (travelState() != 0) return -5;
        uintptr_t holder = *(uintptr_t*)(imageLow + kWorldHolderRva);
        if (!readable(holder + kHolderWorld, 8)) return -6;
        uintptr_t world = *(uintptr_t*)(holder + kHolderWorld);
        if (!world || !readable(world + kWorldPlayer, 8) || *(uintptr_t*)(world + kWorldPlayer) != entity) return -6;
        if (!readable(entity + kEntityWorld, 8) || *(uintptr_t*)(entity + kEntityWorld) != world) return -6;
        return 0;
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        return -6;
    }
}
static int worldGuard() {
    __try {
        return travelState() == 0 ? 0 : -5;
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        return -5;
    }
}

static void serviceTeleport() {
    volatile TeleportShared* t = teleport;
    if (!t || t->magic != kTeleportMagic || t->request == t->done) return;
    float matrix[16];
    for (int i = 0; i < 16; i++) matrix[i] = t->matrix[i];
    uint32_t exception = 0;
    float readback = -1;
    if (t->mode == 6 || t->mode == 7) {
        uint64_t ids[8], found = 0;
        memcpy(ids, matrix, sizeof ids);
        int guard = worldGuard();
        int status = guard ? guard : runOutfit(t->mode, ids, (uint32_t)t->human, &found, &exception);
        t->entity = found;
        t->exception = exception;
        t->status = status;
        InterlockedIncrement(&teleports);
        MemoryBarrier();
        t->done = t->request;
        return;
    }
    if (t->mode == 10) {
        uint64_t clone = 0;
        int guard = worldGuard();
        int status = guard ? guard : runCloneEntity((uintptr_t)t->entity, matrix, t->human, &clone, &exception);
        t->entity = clone;
        if (status == -3) memcpy((void*)&t->matrix[0], cloneFault, sizeof cloneFault);
        t->exception = exception;
        t->status = status;
        InterlockedIncrement(&teleports);
        MemoryBarrier();
        t->done = t->request;
        return;
    }
    if (t->mode == 8) {
        uint64_t entity = 0, component = 0, bodies[2] = {};
        int guard = worldGuard();
        int status = guard ? guard : runSpawnBox(matrix, &entity, &component, bodies, &exception);
        t->entity = entity;
        t->human = component;
        memcpy((void*)&t->matrix[8], bodies, sizeof bodies);
        t->exception = exception;
        t->status = status;
        InterlockedIncrement(&teleports);
        MemoryBarrier();
        t->done = t->request;
        return;
    }
    int guard = t->mode <= 2 ? playerGuard((uintptr_t)t->entity) : (t->mode == 3 || t->mode == 4 || t->mode == 9) ? worldGuard() : 0;
    int status = guard ? guard
               : t->mode == 0 ? runTeleport((uintptr_t)t->human, (uintptr_t)t->entity, matrix, &exception)
               : t->mode == 3 ? runTravel((uintptr_t)t->human, t->entity, &exception)
               : t->mode == 4 ? runLoadWorld(t->entity, t->human, &exception)
               : t->mode == 5 ? runStartWorld((uintptr_t)t->entity, &exception)
               : t->mode == 9 ? runMoveEntity((uintptr_t)t->entity, matrix, &exception)
               : t->mode <= 2 ? runFlag(t->mode, (uintptr_t)t->human, (uintptr_t)t->entity, matrix[0] > 0.5f, &readback, &exception)
               : -4;
    t->matrix[1] = readback;
    t->exception = exception;
    t->status = status;
    InterlockedIncrement(&teleports);
    MemoryBarrier();
    t->done = t->request;
}

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
    if (!blockUser() || !point) {
        BOOL ok = originalGetCursorPos(point);
        if (ok && point) { frozenCursor = *point; InterlockedExchange(&haveFrozenCursor, 1); }
        return ok;
    }
    InterlockedIncrement(&frozenCursorReads);
    if (!haveFrozenCursor) {  // first blocked read: freeze wherever the cursor is now
        if (!originalGetCursorPos(&frozenCursor)) return FALSE;
        InterlockedExchange(&haveFrozenCursor, 1);
    }
    *point = frozenCursor;
    return TRUE;
}

// ClipCursor / SetCursorPos / ShowCursor are 6-byte `jmp [rip+rel]` stubs into win32u; the real
// targets are read from the stubs before patching and called directly when not blocking.
static BOOL(WINAPI* realClipCursor)(const RECT*) = nullptr;
static BOOL(WINAPI* realSetCursorPos)(int, int) = nullptr;
static BOOL WINAPI clipCursorFake(const RECT* rect) {
    if (blockUser() && rect) { InterlockedIncrement(&droppedClips); return TRUE; }
    return realClipCursor ? realClipCursor(rect) : FALSE;
}
static BOOL WINAPI setCursorPosFake(int x, int y) {
    if (blockUser()) { InterlockedIncrement(&droppedCursorSets); frozenCursor = {x, y}; return TRUE; }
    return realSetCursorPos ? realSetCursorPos(x, y) : FALSE;
}
static int WINAPI showCursorFake(BOOL show) {
    // Keep the user's cursor visible over the game window while blocking. Report -1 (hidden): ACU
    // hides in a loop until the count goes negative, and returning 0 spun its thread (2026-11-14,
    // 410 million calls in seconds).
    if (blockUser() && !show) { InterlockedIncrement(&droppedCursorHides); return -1; }
    return realShowCursor ? realShowCursor(show) : 0;
}
static void* stubTarget(const char* name) {
    HMODULE user = GetModuleHandleA("user32.dll");
    const BYTE* stub = user ? (const BYTE*)GetProcAddress(user, name) : nullptr;
    if (!stub || stub[0] != 0xFF || stub[1] != 0x25) return nullptr;
    int32_t rel;
    memcpy(&rel, stub + 2, 4);
    return *(void**)(stub + 6 + rel);
}

// Shared virtual-keyboard state written by the runner (Local\acu-keys-<pid>): '<III256B', 268 bytes.
// keys[DIK scancode] != 0 means held. Held keys are ORed into DirectInput keyboard GetDeviceState
// (256-byte reads), and each change is queued as a buffered GetDeviceData event for the devices
// that made 256-byte reads (the keyboard). The user's real keys stay blanked while blocking.
struct KeyShared { uint32_t magic, enabled, sequence; uint8_t keys[256]; };
static_assert(sizeof(KeyShared) == 268, "key layout");
constexpr uint32_t kKeyMagic = 0x4B554341;  // "ACUK"
static volatile KeyShared* keyboard = nullptr;
static void* keyboardDevices[4] = {};
static uint8_t keyLast[4][256] = {};
static volatile LONG keyInjectedStates = 0, keyInjectedEvents = 0;
static bool keysActive() { return keyboard && keyboard->magic == kKeyMagic && keyboard->enabled; }
static int keyboardSlot(void* device, bool add) {
    for (int i = 0; i < 4; i++) if (keyboardDevices[i] == device) return i;
    if (!add) return -1;
    for (int i = 0; i < 4; i++) if (!keyboardDevices[i]) { keyboardDevices[i] = device; return i; }
    return -1;
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
    if (SUCCEEDED(hr) && data && blockUser()) { memset(data, 0, size); InterlockedIncrement(&blankedDeviceState); }
    if (SUCCEEDED(hr) && data && size == 256) {
        keyboardSlot(device, true);
        if (keysActive()) {
            bool any = false;
            for (int k = 0; k < 256; k++) if (keyboard->keys[k]) { ((BYTE*)data)[k] |= 0x80; any = true; }
            if (any) InterlockedIncrement(&keyInjectedStates);
        }
    }
    return hr;
}
static HRESULT STDMETHODCALLTYPE getDeviceDataFake(void* device, DWORD objectSize, LPDIDEVICEOBJECTDATA items,
                                                   LPDWORD count, DWORD flags) {
    int i = vtableIndex(device);
    if (i < 0) return DIERR_GENERIC;
    DWORD capacity = count ? *count : 0;
    HRESULT hr = vtables[i].data(device, objectSize, items, count, flags);
    if (SUCCEEDED(hr) && count && *count && blockUser()) { *count = 0; InterlockedIncrement(&blankedDeviceData); }
    int slot = keyboardSlot(device, false);
    // Append one event per changed key (never on DIGDD_PEEK, which must not consume anything).
    if (SUCCEEDED(hr) && count && slot >= 0 && keysActive() && !(flags & DIGDD_PEEK) && items &&
        objectSize >= sizeof(DIDEVICEOBJECTDATA)) {
        for (int k = 0; k < 256 && *count < capacity; k++) {
            uint8_t now = keyboard->keys[k] ? 0x80 : 0;
            if (now == keyLast[slot][k]) continue;
            auto* item = (DIDEVICEOBJECTDATA*)((BYTE*)items + (size_t)objectSize * *count);
            memset(item, 0, objectSize);
            item->dwOfs = (DWORD)k;
            item->dwData = now;
            item->dwTimeStamp = GetTickCount();
            item->dwSequence = (DWORD)InterlockedIncrement(&keyInjectedEvents);
            keyLast[slot][k] = now;
            ++*count;
        }
    }
    return hr;
}

static LRESULT CALLBACK subclassProc(HWND hwnd, UINT message, WPARAM wParam, LPARAM lParam) {
    switch (message) {
    case WM_ACTIVATEAPP: if (!wParam) { InterlockedIncrement(&droppedActivateApp); return 0; } break;
    case WM_ACTIVATE: if (LOWORD(wParam) == WA_INACTIVE) { InterlockedIncrement(&droppedActivate); return 0; } break;
    case WM_KILLFOCUS: InterlockedIncrement(&droppedKillFocus); return 0;
    case WM_NCACTIVATE: if (!wParam) { InterlockedIncrement(&droppedNcActivate); return TRUE; } break;
    }
    // ShowCursor's display counter is per thread: undo the game's hiding on its own (window) thread.
    if (message == kShowCursorMessage) {
        for (int i = 0; realShowCursor && i < 64 && realShowCursor(TRUE) < 0; i++) {}
        return 0;
    }
    if (message == kTeleportMessage) { serviceTeleport(); return 0; }
    // Raw input, mouse and keyboard messages go to the default procedure, not the game, while
    // blocking (DefWindowProc also frees WM_INPUT data). XInput is unaffected.
    if (blockUser() && (message == WM_INPUT || (message >= WM_MOUSEFIRST && message <= WM_MOUSELAST) ||
                        (message >= WM_KEYFIRST && message <= WM_KEYLAST))) {
        InterlockedIncrement(&droppedInputMessages);
        return DefWindowProcW(hwnd, message, wParam, lParam);
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
    sprintf_s(buf, ",\"block_user_input\":%s,\"returned_focus\":%s,\"focus_returns\":%ld,\"blocked\":{\"clips\":%ld,"
              "\"cursor_sets\":%ld,\"cursor_hides\":%ld,\"input_messages\":%ld}",
              blockUserInput ? "true" : "false", returnedFocus ? "true" : "false", focusReturns, droppedClips, droppedCursorSets,
              droppedCursorHides, droppedInputMessages);
    s += buf;
    sprintf_s(buf, ",\"frame\":{\"present_hooked\":%s,\"fps_cap\":%u,\"fps\":%ld,\"presents\":%ld}",
              presentPatched ? "true" : "false", fpsCap, measuredFps, presents);
    s += buf;
    sprintf_s(buf, ",\"pad\":{\"mapped\":%s,\"enabled\":%s,\"sequence\":%lu,\"state_reads\":%ld,\"caps_reads\":%ld,\"device_notices\":%ld}",
              pad ? "true" : "false", padActive(0) ? "true" : "false", pad ? (unsigned long)pad->sequence : 0ul,
              padStateReads, padCapsReads, padDeviceNotices);
    s += buf;
    sprintf_s(buf, ",\"teleport\":{\"mapped\":%s,\"serviced\":%ld}", teleport ? "true" : "false", teleports);
    s += buf;
    sprintf_s(buf, ",\"keys\":{\"mapped\":%s,\"states\":%ld,\"events\":%ld,\"devices\":%d}", keyboard ? "true" : "false",
              keyInjectedStates, keyInjectedEvents,
              (keyboardDevices[0] != nullptr) + (keyboardDevices[1] != nullptr) + (keyboardDevices[2] != nullptr) +
              (keyboardDevices[3] != nullptr));
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
    swprintf_s(padName, L"Local\\acu-keys-%lu", GetCurrentProcessId());
    if (HANDLE section = CreateFileMappingW(INVALID_HANDLE_VALUE, nullptr, PAGE_READWRITE, 0, sizeof(KeyShared), padName))
        keyboard = (volatile KeyShared*)MapViewOfFile(section, FILE_MAP_ALL_ACCESS, 0, 0, sizeof(KeyShared));
    swprintf_s(padName, L"Local\\acu-teleport-%lu", GetCurrentProcessId());
    if (HANDLE section = CreateFileMappingW(INVALID_HANDLE_VALUE, nullptr, PAGE_READWRITE, 0, sizeof(TeleportShared), padName))
        teleport = (volatile TeleportShared*)MapViewOfFile(section, FILE_MAP_ALL_ACCESS, 0, 0, sizeof(TeleportShared));
    {
        auto image = (BYTE*)GetModuleHandleW(nullptr);
        auto nt = (IMAGE_NT_HEADERS*)(image + ((IMAGE_DOS_HEADER*)image)->e_lfanew);
        imageLow = (uintptr_t)image;
        imageHigh = imageLow + nt->OptionalHeader.SizeOfImage;
    }
    const std::wstring ini = besideDll(L"acu_focus.ini");
    blockUserInput = GetPrivateProfileIntW(L"focus", L"block_user_input", 0, ini.c_str()) != 0;
    wchar_t returnTo[32] = {};
    GetPrivateProfileStringW(L"focus", L"return_focus", L"", returnTo, 32, ini.c_str());
    realClipCursor = (BOOL(WINAPI*)(const RECT*))stubTarget("ClipCursor");
    realSetCursorPos = (BOOL(WINAPI*)(int, int))stubTarget("SetCursorPos");
    realShowCursor = (int(WINAPI*)(BOOL))stubTarget("ShowCursor");
    patchGetCursorPos();
    if (realClipCursor) patchExport("user32.dll", "ClipCursor", (void*)&clipCursorFake);
    if (realSetCursorPos) patchExport("user32.dll", "SetCursorPos", (void*)&setCursorPosFake);
    if (realShowCursor) patchExport("user32.dll", "ShowCursor", (void*)&showCursorFake);
    if (blockUserInput) {
        if (realClipCursor) realClipCursor(nullptr);  // release a clip the game set before we arrived
        returnFocusTo = (HWND)(uintptr_t)wcstoull(returnTo, nullptr, 16);
    }
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
        if (blockUserInput) PostMessageW(gameWindow, kShowCursorMessage, 0, 0);
    }
    bool padWasActive = false;
    for (unsigned tick = 0; stop && WaitForSingleObject(stop, 100) == WAIT_TIMEOUT; tick++) {
        // Announce pad arrival/removal the way a real device would, so the game re-enumerates.
        // ACU takes the foreground some time after its window appears (not yet at install). For
        // the first 90 s hand it back whenever it does; the foreground process may do that.
        if (returnFocusTo && tick < 900 && realForeground() == gameWindow && IsWindow(returnFocusTo)) {
            if (SetForegroundWindow(returnFocusTo)) { returnedFocus = true; InterlockedIncrement(&focusReturns); }
            if (realClipCursor) realClipCursor(nullptr);
        }
        if (teleport && teleport->magic == kTeleportMagic && teleport->request != teleport->done)
            PostMessageW(gameWindow, kTeleportMessage, 0, 0);
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
