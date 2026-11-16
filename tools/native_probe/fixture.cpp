#include <windows.h>
#include <cstdio>
#include <cstdint>
#include <cstddef>
extern "C" __declspec(dllexport) volatile uint64_t ProbeFixtureCounter = 0;
extern "C" __declspec(dllexport) __declspec(noinline) uint64_t ProbeFixtureTick(uint64_t value) {
    ProbeFixtureCounter += value;
    return ProbeFixtureCounter;
}
// CALLAT regression: called by the probe on the fixture's own thread; records the thread and arguments.
extern "C" __declspec(dllexport) volatile uint64_t ProbeFixtureCallSum = 0;
extern "C" __declspec(dllexport) volatile uint32_t ProbeFixtureCallThread = 0;
extern "C" __declspec(dllexport) __declspec(noinline) uint64_t ProbeFixtureAdd(uint64_t a, uint64_t b, uint64_t c, uint64_t d) {
    ProbeFixtureCallThread = GetCurrentThreadId();
    ProbeFixtureCallSum += a + 2 * b + 3 * c + 4 * d;
    return a + 2 * b + 3 * c + 4 * d;
}
// Synthetic pointer array for the GATHER regression: data pointer at +0, u16 count at +10,
// elements of 32 bytes whose bytes are 0x10*(index+1)+offset. No relation to ACU layouts.
struct FixtureArray { uint8_t** data; uint16_t pad; uint16_t count; };
static uint8_t fixtureElements[3][32];
static uint8_t* fixturePointers[3] = {fixtureElements[0], fixtureElements[1], fixtureElements[2]};
static FixtureArray fixtureArray = {fixturePointers, 0, 3};
static FixtureArray* fixtureArrayRef = &fixtureArray;
extern "C" __declspec(dllexport) __declspec(noinline) uint64_t ProbeFixtureSelect(FixtureArray* array, FixtureArray** ref) {
    return array->count + (*ref)->count + ProbeFixtureCounter;
}
int wmain() {
    static_assert(offsetof(FixtureArray, count) == 10, "fixture count offset");
    for (int i = 0; i < 3; i++) for (int k = 0; k < 32; k++) fixtureElements[i][k] = (uint8_t)(0x10 * (i + 1) + k);
    printf("{\"pid\":%lu,\"tick_rva\":\"%llx\",\"counter_rva\":\"%llx\"}\n", GetCurrentProcessId(),
      (unsigned long long)((uintptr_t)&ProbeFixtureTick-(uintptr_t)GetModuleHandleW(nullptr)),
      (unsigned long long)((uintptr_t)&ProbeFixtureCounter-(uintptr_t)GetModuleHandleW(nullptr)));
    fflush(stdout);
    for (;;) { ProbeFixtureTick(1); ProbeFixtureSelect(&fixtureArray, &fixtureArrayRef); Sleep(20); }
}
