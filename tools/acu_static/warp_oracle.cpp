// Execute only the call-free arithmetic excerpt in a private process.
// Input must be the identified mapped ACU image, never a disk PE.
#include <windows.h>
#include <wincrypt.h>
#include <array>
#include <cstdio>
#include <fstream>
#include <vector>
#include <cstring>
#pragma comment(lib,"advapi32.lib")
extern "C" { void* warp_entry = reinterpret_cast<void*>(0x14018979eULL); }
struct Input { float step[4], remaining[4], error[4], dt, time; int mode; };
struct Output { float delta[3]; unsigned complete; };
extern "C" void run_warp(const Input*, Output*);
static LONG report_exception(EXCEPTION_POINTERS* p) {
    fprintf(stderr,"exception %lx at %llx access %llx rsp %llx rbp %llx\n",p->ExceptionRecord->ExceptionCode,p->ContextRecord->Rip,p->ExceptionRecord->ExceptionInformation[1],p->ContextRecord->Rsp,p->ContextRecord->Rbp);
    ExitProcess(8);
}
static bool guarded_run(const Input* x, Output* y) {
    __try { run_warp(x,y); return true; }
    __except(report_exception(GetExceptionInformation())) { return false; }
}
int main(int argc, char** argv) {
    AddVectoredExceptionHandler(1,report_exception);
    if (argc != 2) return 2;
    std::ifstream in(argv[1], std::ios::binary | std::ios::ate);
    if (!in) return 3;
    auto size = in.tellg();
    if (size < 0x5221f30) return 4;
    std::vector<char> image(static_cast<size_t>(size));
    in.seekg(0); in.read(image.data(), image.size());
    HCRYPTPROV provider=0; HCRYPTHASH hash=0; BYTE digest[32]; DWORD length=32;
    if (!CryptAcquireContext(&provider,nullptr,nullptr,PROV_RSA_AES,CRYPT_VERIFYCONTEXT)
        || !CryptCreateHash(provider,CALG_SHA_256,0,0,&hash)
        || !CryptHashData(hash,reinterpret_cast<BYTE*>(image.data()),static_cast<DWORD>(image.size()),0)
        || !CryptGetHashParam(hash,HP_HASHVAL,digest,&length,0)) return 9;
    char identity[65];
    for(unsigned i=0;i<32;++i) sprintf_s(identity+2*i,3,"%02x",digest[i]);
    CryptDestroyHash(hash); CryptReleaseContext(provider,0);
    if (strcmp(identity,"9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520")) return 10;
    // Exact excerpt guards, plus identity checked by the Python driver.
    const unsigned char entry[] = {0x32,0xc9,0x32,0xc0};
    if (memcmp(image.data()+0x18979e,entry,4) || (unsigned char)image[0x189b81] != 0xe9) return 5;
    for (auto region : {std::array<size_t,2>{0x180000,0x10000}, {0x5220000,0x10000}}) {
        auto dest = reinterpret_cast<void*>(0x140000000ULL+region[0]);
        if (VirtualAlloc(dest,region[1],MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE)!=dest) return 6;
        memcpy(dest,image.data()+region[0],region[1]);
        if (region[0]==0x180000) *reinterpret_cast<unsigned char*>(0x140189b81ULL)=0xc3;
        DWORD old;
        if (!VirtualProtect(dest,region[1],PAGE_EXECUTE_READ,&old)) return 7;
    }
    FlushInstructionCache(GetCurrentProcess(),nullptr,0);
    std::array<std::array<float,3>,10> remaining = {{{2,0,0},{0,2,0},{0,0,2},{1,1,1},{1,2,3},{-3,2,-1},{0,0,0},{0.01f,-0.01f,0.01f},{0.011f,0.001f,0},{2,2,1}}};
    std::array<std::array<float,3>,7> steps = {{{.02f,0,0},{0,.02f,0},{0,0,.02f},{.02f,.02f,.02f},{-.03f,.02f,-.01f},{0,0,0},{.01f,-.01f,.011f}}};
    unsigned count=0;
    printf("{\"kind\":\"isolated-native-arithmetic\",\"entry\":\"0x14018979e\",\"exit\":\"0x140189b81\",\"cases\":[");
    for (int mode=0; mode<2; ++mode) for (auto r:remaining) for(auto s:steps) for(int e=0;e<3;++e) {
        Input x{}; Output y{};
        for(int i=0;i<3;++i) { x.step[i]=s[i]; x.remaining[i]=r[i]; x.error[i]=e==0 ? 0.f : (e==1 ? (i+1)*.25f : -(i+1)*.13f); }
        x.dt=.016f; x.time=.8f; x.mode=mode;
        if (!guarded_run(&x,&y)) return 8;
        if(count++) printf(",");
        printf("{\"mode\":%d,\"step\":[%.9g,%.9g,%.9g],\"remaining\":[%.9g,%.9g,%.9g],\"error\":[%.9g,%.9g,%.9g],\"dt\":%.9g,\"time\":%.9g,\"delta\":[%.9g,%.9g,%.9g],\"complete\":%s}",mode,s[0],s[1],s[2],r[0],r[1],r[2],x.error[0],x.error[1],x.error[2],x.dt,x.time,y.delta[0],y.delta[1],y.delta[2],y.complete?"true":"false");
    }
    printf("]}\n");
}
