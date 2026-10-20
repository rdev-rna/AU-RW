#include <windows.h>
#include <cstdio>
#include <cstdint>
extern "C" __declspec(dllexport) volatile uint64_t ProbeFixtureCounter = 0;
extern "C" __declspec(dllexport) __declspec(noinline) uint64_t ProbeFixtureTick(uint64_t value) {
    ProbeFixtureCounter += value;
    return ProbeFixtureCounter;
}
int wmain() {
    printf("{\"pid\":%lu,\"tick_rva\":\"%llx\",\"counter_rva\":\"%llx\"}\n", GetCurrentProcessId(),
      (unsigned long long)((uintptr_t)&ProbeFixtureTick-(uintptr_t)GetModuleHandleW(nullptr)),
      (unsigned long long)((uintptr_t)&ProbeFixtureCounter-(uintptr_t)GetModuleHandleW(nullptr)));
    fflush(stdout);
    for (;;) { ProbeFixtureTick(1); Sleep(20); }
}
