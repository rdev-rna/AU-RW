// acu_shader_probe.dll: which pixel shaders ACU creates and binds (ACU-RE-0084 live check).
//
// Injected with acu_probe_launcher (`capture_run.py --extra-dll`). Hooks, with MinHook, the
// d3d11.dll implementations of ID3D11Device::CreatePixelShader (vtable slot 15) and
// ID3D11DeviceContext::PSSetShader (slot 9, immediate and deferred contexts), found through a
// throwaway device. Each created shader object is keyed by its DXBC checksum (bytecode bytes
// 4..20), so it can be matched to the blobs in a template payload; every bind is counted.
// Every 2 s the totals go to acu_shader_probe-<pid>.json beside the DLL (temp file + rename).
// Read-only towards the game: no game code, data or files change. Not unloaded while ACU runs.
#include <windows.h>
#include <d3d11.h>
#include <MinHook.h>

#include <atomic>
#include <cstdio>
#include <string>
#include <unordered_map>
#include <vector>

#pragma comment(lib, "d3d11.lib")

namespace {

struct Entry {
    char sum[33];
    UINT size;
    LONG creates;
    std::atomic<long long> binds{0};
};

SRWLOCK lock = SRWLOCK_INIT;
std::unordered_map<void*, Entry*> byObject;
std::unordered_map<std::string, Entry*> bySum;
std::atomic<long long> unknownBinds{0}, nullBinds{0}, created{0};
HMODULE self;
std::string status = "starting";
void* targets[3];

using CreatePS = HRESULT(STDMETHODCALLTYPE*)(ID3D11Device*, const void*, SIZE_T, ID3D11ClassLinkage*,
                                             ID3D11PixelShader**);
using SetPS = void(STDMETHODCALLTYPE*)(ID3D11DeviceContext*, ID3D11PixelShader*, ID3D11ClassInstance* const*,
                                       UINT);
CreatePS realCreate;
SetPS realSetImmediate, realSetDeferred;

void record(void* object, const unsigned char* code, UINT size) {
    char hex[33];
    for (int i = 0; i < 16; i++) sprintf_s(hex + 2 * i, 3, "%02x", code[4 + i]);
    AcquireSRWLockExclusive(&lock);
    Entry*& e = bySum[hex];
    if (!e) {
        e = new Entry();
        memcpy(e->sum, hex, sizeof hex);
        e->size = size;
    }
    e->creates++;
    byObject[object] = e;
    ReleaseSRWLockExclusive(&lock);
    created++;
}

void count(ID3D11PixelShader* shader) {
    if (!shader) {
        nullBinds++;
        return;
    }
    AcquireSRWLockShared(&lock);
    auto it = byObject.find(shader);
    Entry* e = it == byObject.end() ? nullptr : it->second;
    ReleaseSRWLockShared(&lock);
    if (e)
        e->binds++;
    else
        unknownBinds++;
}

HRESULT STDMETHODCALLTYPE fakeCreate(ID3D11Device* device, const void* code, SIZE_T size, ID3D11ClassLinkage* linkage,
                                     ID3D11PixelShader** out) {
    HRESULT hr = realCreate(device, code, size, linkage, out);
    if (SUCCEEDED(hr) && out && *out && code && size >= 32 && memcmp(code, "DXBC", 4) == 0)
        record(*out, static_cast<const unsigned char*>(code), static_cast<UINT>(size));
    return hr;
}

void STDMETHODCALLTYPE fakeSetImmediate(ID3D11DeviceContext* c, ID3D11PixelShader* s, ID3D11ClassInstance* const* i,
                                        UINT n) {
    count(s);
    realSetImmediate(c, s, i, n);
}

void STDMETHODCALLTYPE fakeSetDeferred(ID3D11DeviceContext* c, ID3D11PixelShader* s, ID3D11ClassInstance* const* i,
                                       UINT n) {
    count(s);
    realSetDeferred(c, s, i, n);
}

std::wstring beside(const wchar_t* name) {
    wchar_t path[MAX_PATH];
    GetModuleFileNameW(self, path, MAX_PATH);
    std::wstring s(path);
    return s.substr(0, s.find_last_of(L"\\/") + 1) + name;
}

void install() {
    if (MH_Initialize() != MH_OK) {
        status = "MH_Initialize failed";
        return;
    }
    ID3D11Device* device = nullptr;
    ID3D11DeviceContext* immediate = nullptr;
    ID3D11DeviceContext* deferred = nullptr;
    if (FAILED(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, 0, nullptr, 0, D3D11_SDK_VERSION, &device,
                                 nullptr, &immediate))) {
        status = "D3D11CreateDevice failed";
        return;
    }
    device->CreateDeferredContext(0, &deferred);
    targets[0] = (*reinterpret_cast<void***>(device))[15];
    targets[1] = (*reinterpret_cast<void***>(immediate))[9];
    targets[2] = deferred ? (*reinterpret_cast<void***>(deferred))[9] : nullptr;
    if (deferred) deferred->Release();
    immediate->Release();
    device->Release();
    bool ok = MH_CreateHook(targets[0], &fakeCreate, reinterpret_cast<void**>(&realCreate)) == MH_OK &&
              MH_CreateHook(targets[1], &fakeSetImmediate, reinterpret_cast<void**>(&realSetImmediate)) == MH_OK;
    if (ok && targets[2] && targets[2] != targets[1])
        ok = MH_CreateHook(targets[2], &fakeSetDeferred, reinterpret_cast<void**>(&realSetDeferred)) == MH_OK;
    if (!ok || MH_EnableHook(MH_ALL_HOOKS) != MH_OK) {
        status = "hook failed";
        return;
    }
    status = "hooked";
}

void dump(const std::wstring& path) {
    std::vector<const Entry*> entries;
    AcquireSRWLockShared(&lock);
    entries.reserve(bySum.size());
    for (auto& kv : bySum) entries.push_back(kv.second);
    ReleaseSRWLockShared(&lock);
    std::wstring tmp = path + L".tmp";
    FILE* f = nullptr;
    if (_wfopen_s(&f, tmp.c_str(), L"wb") || !f) return;
    fprintf(f,
            "{\"pid\":%lu,\"tick_ms\":%llu,\"status\":\"%s\",\"targets\":[\"%p\",\"%p\",\"%p\"],"
            "\"created\":%lld,\"unique\":%zu,\"unknown_binds\":%lld,\"null_binds\":%lld,\"shaders\":[",
            GetCurrentProcessId(), GetTickCount64(), status.c_str(), targets[0], targets[1], targets[2],
            created.load(), entries.size(), unknownBinds.load(), nullBinds.load());
    for (size_t i = 0; i < entries.size(); i++)
        fprintf(f, "%s\n{\"sum\":\"%s\",\"size\":%u,\"creates\":%ld,\"binds\":%lld}", i ? "," : "", entries[i]->sum,
                entries[i]->size, entries[i]->creates, entries[i]->binds.load());
    fprintf(f, "]}\n");
    fclose(f);
    MoveFileExW(tmp.c_str(), path.c_str(), MOVEFILE_REPLACE_EXISTING);
}

DWORD WINAPI run(void*) {
    install();
    wchar_t name[64];
    swprintf_s(name, L"acu_shader_probe-%lu.json", GetCurrentProcessId());
    std::wstring path = beside(name);
    for (;;) {
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
