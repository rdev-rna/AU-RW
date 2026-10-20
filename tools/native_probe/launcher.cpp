#include <windows.h>
#include <bcrypt.h>
#include <tlhelp32.h>
#include <cstdio>
#include <string>
#include <vector>
#include "fixture_hash.h"
#pragma comment(lib, "bcrypt.lib")
static bool pipeTransfer(HANDLE pipe,bool write,void* data,DWORD length,DWORD& done){
    OVERLAPPED op{};op.hEvent=CreateEventW(nullptr,TRUE,FALSE,nullptr);
    BOOL ok=write?WriteFile(pipe,data,length,&done,&op):ReadFile(pipe,data,length,&done,&op);
    if(!ok&&GetLastError()==ERROR_IO_PENDING){if(WaitForSingleObject(op.hEvent,500)==WAIT_OBJECT_0)ok=GetOverlappedResult(pipe,&op,&done,FALSE);else{CancelIoEx(pipe,&op);WaitForSingleObject(op.hEvent,INFINITE);ok=FALSE;}}
    CloseHandle(op.hEvent);return ok!=FALSE;
}
static bool probeReady(DWORD pid){
    wchar_t name[128];swprintf_s(name,L"\\\\.\\pipe\\acu-probe-%lu",pid);
    for(unsigned n=0;n<20;n++){if(WaitNamedPipeW(name,100))break;Sleep(25);}
    HANDLE pipe=CreateFileW(name,GENERIC_READ|GENERIC_WRITE,0,nullptr,OPEN_EXISTING,FILE_FLAG_OVERLAPPED,nullptr);
    if(pipe==INVALID_HANDLE_VALUE)return false;
    char request[]="STATUS\n",response[8192]{};DWORD done=0;
    bool ok=pipeTransfer(pipe,true,request,sizeof request-1,done)&&pipeTransfer(pipe,false,response,sizeof response-1,done);
    if(ok){response[done]=0;ok=strstr(response,"\"ok\":true")&&strstr(response,"\"protocol\":1");char ack[]="ACK";DWORD sent;pipeTransfer(pipe,true,ack,3,sent);}
    CloseHandle(pipe);return ok;
}
static std::string hash(const wchar_t* path) {
    HANDLE f=CreateFileW(path,GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,0,nullptr);
    if(f==INVALID_HANDLE_VALUE) return {};
    BCRYPT_ALG_HANDLE a=nullptr; BCRYPT_HASH_HANDLE h=nullptr;
    std::string out;
    if(BCryptOpenAlgorithmProvider(&a,BCRYPT_SHA256_ALGORITHM,nullptr,0)>=0 && BCryptCreateHash(a,&h,nullptr,0,nullptr,0,0)>=0){
        BYTE data[65536],digest[32]; DWORD n=0; bool ok=true;
        while(ReadFile(f,data,sizeof data,&n,nullptr)&&n) if(BCryptHashData(h,data,n,0)<0) {ok=false;break;}
        if(ok&&BCryptFinishHash(h,digest,32,0)>=0){char b[3];for(auto x:digest){sprintf_s(b,"%02x",x);out+=b;}}
    }
    if(h) BCryptDestroyHash(h); if(a) BCryptCloseAlgorithmProvider(a,0); CloseHandle(f); return out;
}
static bool inject(HANDLE p,const std::wstring& dll,bool bootstrap=true) {
    SIZE_T bytes=(dll.size()+1)*sizeof(wchar_t),w=0;
    void* remote=VirtualAllocEx(p,nullptr,bytes,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE);
    if(!remote){fprintf(stderr,"inject allocation failed win32=%lu\n",GetLastError());return false;}
    if(!WriteProcessMemory(p,remote,dll.c_str(),bytes,&w)||w!=bytes){VirtualFreeEx(p,remote,0,MEM_RELEASE);return false;}
    // Resolve remote kernel32 base + local export RVA; do not assume equal ASLR bases.
    HMODULE local=GetModuleHandleW(L"kernel32.dll");
    auto load=(uintptr_t)GetProcAddress(local,"LoadLibraryW");
    HMODULE owner=nullptr; GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS|GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,(LPCWSTR)load,&owner);
    wchar_t ownerPath[MAX_PATH]; GetModuleFileNameW(owner,ownerPath,MAX_PATH);
    const wchar_t* ownerName=wcsrchr(ownerPath,L'\\'); ownerName=ownerName?ownerName+1:ownerPath;
    HANDLE snap=CreateToolhelp32Snapshot(TH32CS_SNAPMODULE|TH32CS_SNAPMODULE32,GetProcessId(p));
    MODULEENTRY32W entry{};entry.dwSize=sizeof entry;uintptr_t base=0;
    if(Module32FirstW(snap,&entry)) do{if(!_wcsicmp(entry.szModule,ownerName)){base=(uintptr_t)entry.modBaseAddr;break;}}while(Module32NextW(snap,&entry));
    CloseHandle(snap);
    if(!base){
        if(!bootstrap){DWORD code=0;GetExitCodeProcess(p,&code);fprintf(stderr,"inject loader module unavailable after normal startup process_exit=%08lx\n",code);VirtualFreeEx(p,remote,0,MEM_RELEASE);return false;}
        // A CREATE_SUSPENDED process has not run the user-mode loader yet.
        // Start a no-op ntdll thread to initialize its loader while its primary
        // thread remains suspended; then enumerate resolved system modules.
        HMODULE nt=GetModuleHandleW(L"ntdll.dll");uintptr_t remoteNt=0;
        snap=CreateToolhelp32Snapshot(TH32CS_SNAPMODULE|TH32CS_SNAPMODULE32,GetProcessId(p));entry.dwSize=sizeof entry;
        if(Module32FirstW(snap,&entry))do{if(!_wcsicmp(entry.szModule,L"ntdll.dll")){remoteNt=(uintptr_t)entry.modBaseAddr;break;}}while(Module32NextW(snap,&entry));CloseHandle(snap);
        auto alert=GetProcAddress(nt,"NtTestAlert");
        if(!remoteNt&&alert){
            BYTE remoteHeader[4096],remoteCode[32];SIZE_T got=0;
            if(ReadProcessMemory(p,nt,remoteHeader,sizeof remoteHeader,&got)&&got==sizeof remoteHeader&&
               !memcmp(remoteHeader,nt,sizeof remoteHeader)&&ReadProcessMemory(p,alert,remoteCode,sizeof remoteCode,&got)&&
               got==sizeof remoteCode&&!memcmp(remoteCode,alert,sizeof remoteCode))remoteNt=(uintptr_t)nt;
        }
        HANDLE init=remoteNt&&alert?CreateRemoteThread(p,nullptr,0,(LPTHREAD_START_ROUTINE)(remoteNt+(uintptr_t)alert-(uintptr_t)nt),nullptr,0,nullptr):nullptr;
        if(!init)fprintf(stderr,"inject loader bootstrap thread unavailable win32=%lu remote_nt=%llx\n",GetLastError(),(unsigned long long)remoteNt);
        if(init){DWORD waited=WaitForSingleObject(init,15000);CloseHandle(init);if(waited!=WAIT_OBJECT_0){VirtualFreeEx(p,remote,0,MEM_RELEASE);return false;}}
        snap=CreateToolhelp32Snapshot(TH32CS_SNAPMODULE|TH32CS_SNAPMODULE32,GetProcessId(p));entry.dwSize=sizeof entry;
        if(Module32FirstW(snap,&entry))do{if(!_wcsicmp(entry.szModule,ownerName)){base=(uintptr_t)entry.modBaseAddr;break;}}while(Module32NextW(snap,&entry));CloseHandle(snap);
        if(!base){fprintf(stderr,"inject remote loader module not found win32=%lu init=%s\n",GetLastError(),init?"started":"failed");VirtualFreeEx(p,remote,0,MEM_RELEASE);return false;}
    }
    HANDLE t=CreateRemoteThread(p,nullptr,0,(LPTHREAD_START_ROUTINE)(base+load-(uintptr_t)owner),remote,0,nullptr);
    if(!t){fprintf(stderr,"inject remote LoadLibrary thread failed win32=%lu\n",GetLastError());VirtualFreeEx(p,remote,0,MEM_RELEASE);return false;}
    DWORD wait=WaitForSingleObject(t,15000),code=0;GetExitCodeThread(t,&code);CloseHandle(t);
    if(wait==WAIT_OBJECT_0)VirtualFreeEx(p,remote,0,MEM_RELEASE);
    if(wait!=WAIT_OBJECT_0||!code)fprintf(stderr,"inject LoadLibrary wait=%lu exit=%08lx win32=%lu\n",wait,code,GetLastError());
    return wait==WAIT_OBJECT_0 && code!=0;
}
int wmain(int argc,wchar_t** argv){
    if(argc==1){
        wchar_t own[32768],gamePath[32768];GetModuleFileNameW(nullptr,own,32768);
        std::wstring folder=own;folder=folder.substr(0,folder.find_last_of(L"\\/"));
        std::wstring ini=folder+L"\\launch.ini",dll=folder+L"\\acu_probe.dll";
        GetPrivateProfileStringW(L"launch",L"exe",L"",gamePath,32768,ini.c_str());
        if(!gamePath[0]){fwprintf(stderr,L"Missing [launch] exe= in %s\n",ini.c_str());return 2;}
        wchar_t launch[]=L"--launch",flag[]=L"--dll";
        wchar_t* args[]={argv[0],launch,gamePath,flag,dll.data()};return wmain(5,args);
    }
    if(argc!=5 || (wcscmp(argv[1],L"--launch")&&wcscmp(argv[1],L"--attach"))){fwprintf(stderr,L"Usage: acu_probe_launcher --launch <exe> --dll <dll>\n       acu_probe_launcher --attach <pid> --dll <dll>\n");return 2;}
    if(wcscmp(argv[3],L"--dll"))return 2;
    wchar_t dllPath[32768];if(!GetFullPathNameW(argv[4],32768,dllPath,nullptr)||GetFileAttributesW(dllPath)==INVALID_FILE_ATTRIBUTES)return 2;
    bool launch=!wcscmp(argv[1],L"--launch"); PROCESS_INFORMATION pi{};HANDLE p=nullptr;
    std::wstring path;
    if(launch){wchar_t full[32768];if(!GetFullPathNameW(argv[2],32768,full,nullptr))return 2;path=full;}
    else {DWORD pid=wcstoul(argv[2],nullptr,10);p=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION|PROCESS_CREATE_THREAD|PROCESS_VM_OPERATION|PROCESS_VM_WRITE|PROCESS_VM_READ,FALSE,pid);wchar_t full[32768];DWORD n=32768;if(!p||!QueryFullProcessImageNameW(p,0,full,&n)){if(p)CloseHandle(p);return 3;}path.assign(full,n);}
    std::string digest=hash(path.c_str());
    if(digest!="748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40" && digest!=FIXTURE_SHA256){fprintf(stderr,"Rejected executable SHA256: %s\n",digest.c_str());if(p)CloseHandle(p);return 4;}
    if(launch){STARTUPINFOW si{};si.cb=sizeof si;std::wstring command=L"\""+path+L"\"";std::wstring dir=path.substr(0,path.find_last_of(L"\\/"));if(!CreateProcessW(path.c_str(),command.data(),nullptr,nullptr,FALSE,CREATE_SUSPENDED,nullptr,dir.c_str(),&si,&pi)){fprintf(stderr,"CreateProcess failed: %lu\n",GetLastError());return 5;}p=pi.hProcess;}
    bool game=digest!=""&&digest!=FIXTURE_SHA256;
    bool running=false;
    if(launch&&game){
        // The protected ACU image requires primary-thread loader initialization.
        // Let normal startup happen before remote DLL loading; install no hooks.
        ResumeThread(pi.hThread);running=true;
        for(unsigned tries=0;tries<200;tries++){
            DWORD code=0;GetExitCodeProcess(p,&code);if(code!=STILL_ACTIVE){fprintf(stderr,"game exited before injection code=%08lx\n",code);break;}
            HANDLE s=CreateToolhelp32Snapshot(TH32CS_SNAPMODULE|TH32CS_SNAPMODULE32,pi.dwProcessId);MODULEENTRY32W m{};m.dwSize=sizeof m;bool ready=false;
            if(Module32FirstW(s,&m))do{if(!_wcsicmp(m.szModule,L"kernel32.dll")){ready=true;break;}}while(Module32NextW(s,&m));CloseHandle(s);
            if(ready){Sleep(100);break;}Sleep(25);
        }
    }
    bool ok=inject(p,dllPath,!game);
    if(launch){if(ok&&!running)ResumeThread(pi.hThread);else if(!ok&&!running)TerminateProcess(p,6);CloseHandle(pi.hThread);}
    DWORD observation=launch&&game?3000:0;
    if(ok&&observation)WaitForSingleObject(p,observation);
    DWORD processCode=0;GetExitCodeProcess(p,&processCode);
    bool alive=processCode==STILL_ACTIVE;
    bool ready=ok&&alive&&probeReady(GetProcessId(p));
    GetExitCodeProcess(p,&processCode);alive=processCode==STILL_ACTIVE;
    std::string exitValue=alive?"null":std::to_string(processCode);
    printf("{\"injected\":%s,\"pid\":%lu,\"sha256\":\"%s\",\"startup_observation_ms\":%lu,\"process_running\":%s,\"process_exit_code\":%s,\"probe_status_ready\":%s}\n",ok?"true":"false",GetProcessId(p),digest.c_str(),observation,alive?"true":"false",exitValue.c_str(),ready?"true":"false");
    CloseHandle(p);return !ok?6:!alive?7:!ready?8:0;
}

