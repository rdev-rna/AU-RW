#define NOMINMAX
#include <windows.h>
#include <tlhelp32.h>
#include <psapi.h>
#include <cstdint>
#include <cstdio>
#include <string>
#include <sstream>
#include <vector>
#include <map>
#include <array>
#include <algorithm>
#pragma comment(lib,"psapi.lib")
static HMODULE self;
static volatile LONG stopFlag=0, gate=0, drops=0;
static uint64_t total=0;
struct Hook { uintptr_t address=0; std::string label; uint64_t hits=0; unsigned every=1; unsigned reg=0; int64_t offset=0; unsigned bytes=0; std::string category; unsigned revision=0; bool redirectRcx=false; uint64_t redirectFrom=0,redirectTo=0; uintptr_t redirectReturn=0; };
static Hook hooks[4];
static uintptr_t ownedSites[4][1024];static volatile LONG ownedCounts[4]{};
struct Event {uint64_t sequence,ticks;DWORD tid;unsigned slot,revision;uintptr_t address,returnAddress;uint64_t rcxAfter;bool rcxRedirected;char label[65],category[65]; CONTEXT context; uint64_t stack[8];DWORD stackBytes;BYTE sample[256];DWORD sampleBytes;uintptr_t sampleAddress;};
static Event events[256];static unsigned nextEvent=0,eventCount=0;
static std::map<DWORD,std::array<DWORD64,6>> originals;
static uint64_t conflicts=0;static unsigned revision=1;static std::map<DWORD,unsigned> applied;static std::map<DWORD,uint64_t> creationTimes;
static uint64_t regValue(const CONTEXT& c,unsigned r){const uint64_t vals[]={c.Rcx,c.Rdx,c.R8,c.R9,c.Rax,c.Rbx,c.Rsi,c.Rdi,c.Rbp,c.Rsp,c.R10,c.R11,c.R12,c.R13,c.R14,c.R15};return r<16?vals[r]:0;}
static LONG CALLBACK handler(EXCEPTION_POINTERS* x){
 if(x->ExceptionRecord->ExceptionCode!=EXCEPTION_SINGLE_STEP)return EXCEPTION_CONTINUE_SEARCH;
 unsigned slot=4;uintptr_t site=(uintptr_t)x->ExceptionRecord->ExceptionAddress;
 for(unsigned i=0;i<4&&slot==4;i++)for(LONG k=0;k<ownedCounts[i];k++)if(ownedSites[i][k]==site&&(x->ContextRecord->Dr6&(1ull<<i))){slot=i;break;}
 if(slot==4)return EXCEPTION_CONTINUE_SEARCH;
 x->ContextRecord->EFlags|=0x10000; x->ContextRecord->Dr6&=~(1ull<<slot);
 if(stopFlag||hooks[slot].address!=site)return EXCEPTION_CONTINUE_EXECUTION;
 if(InterlockedCompareExchange(&gate,1,0)){InterlockedIncrement(&drops);return EXCEPTION_CONTINUE_EXECUTION;}
 Hook& h=hooks[slot];h.hits++;total++;
 uintptr_t callReturn=0;SIZE_T returnBytes=0;ReadProcessMemory(GetCurrentProcess(),(void*)x->ContextRecord->Rsp,&callReturn,sizeof callReturn,&returnBytes);
 bool redirect=h.redirectRcx&&returnBytes==sizeof callReturn&&x->ContextRecord->Rcx==h.redirectFrom&&callReturn==h.redirectReturn;
 uint64_t rcxAfter=redirect?h.redirectTo:x->ContextRecord->Rcx;
 if(h.hits%h.every==0){Event& e=events[nextEvent];e={};e.sequence=total;LARGE_INTEGER q;QueryPerformanceCounter(&q);e.ticks=q.QuadPart;e.tid=GetCurrentThreadId();e.slot=slot;e.revision=h.revision;e.address=h.address;e.returnAddress=callReturn;e.rcxAfter=rcxAfter;e.rcxRedirected=redirect;memcpy(e.label,h.label.data(),h.label.size());memcpy(e.category,h.category.data(),h.category.size());e.context=*x->ContextRecord;
 SIZE_T n=0;ReadProcessMemory(GetCurrentProcess(),(void*)e.context.Rsp,e.stack,sizeof e.stack,&n);e.stackBytes=(DWORD)n;
 if(h.bytes){e.sampleAddress=regValue(e.context,h.reg)+h.offset;n=0;ReadProcessMemory(GetCurrentProcess(),(void*)e.sampleAddress,e.sample,h.bytes,&n);e.sampleBytes=(DWORD)n;}
 nextEvent=(nextEvent+1)%256;if(eventCount<256)eventCount++;else InterlockedIncrement(&drops);}
 if(redirect)x->ContextRecord->Rcx=h.redirectTo;
 InterlockedExchange(&gate,0);return EXCEPTION_CONTINUE_EXECUTION;
}
static void lock(){while(InterlockedCompareExchange(&gate,1,0))Sleep(0);}static void unlock(){InterlockedExchange(&gate,0);}
static std::string narrow(const wchar_t* p){int n=WideCharToMultiByte(CP_UTF8,0,p,-1,nullptr,0,nullptr,nullptr);std::string s(n,0);WideCharToMultiByte(CP_UTF8,0,p,-1,s.data(),n,nullptr,nullptr);s.resize(n-1);return s;}
static std::string hx(uint64_t v){char b[32];sprintf_s(b,"%016llx",(unsigned long long)v);return b;}
static std::string quote(const std::string& s){std::string r="\"";for(unsigned char c:s){if(c=='"'||c=='\\'){r+='\\';r+=c;}else if(c>=32&&c<127)r+=c;else{char b[7];sprintf_s(b,"\\u%04x",c);r+=b;}}return r+'"';}
static std::string hexbytes(const BYTE* p,size_t n){std::string r;char b[3];for(size_t i=0;i<n;i++){sprintf_s(b,"%02x",p[i]);r+=b;}return r;}
static bool number(const std::string& s,uint64_t& n,int base=16){if(s.empty()||s[0]=='-')return false;char* end=nullptr;n=_strtoui64(s.c_str(),&end,base);return end&&!*end;}
static std::string fail(const char* s){return "{\"ok\":false,\"error\":"+quote(s)+"}";}
static bool readable(uintptr_t a,size_t len,std::vector<BYTE>& data){if(!len||len>65536||a+len<a)return false;data.resize(len);SIZE_T n=0;bool ok=ReadProcessMemory(GetCurrentProcess(),(void*)a,data.data(),len,&n)!=0;data.resize(n);return ok&&n==len;}
static void updateThreads(bool restore=false){
 bool active=false;for(auto& h:hooks)active|=h.address!=0;if(!active&&originals.empty())return;
 HANDLE snap=CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD,0);THREADENTRY32 e{};e.dwSize=sizeof e;
 if(Thread32First(snap,&e))do{if(e.th32OwnerProcessID!=GetCurrentProcessId()||e.th32ThreadID==GetCurrentThreadId())continue;
 HANDLE t=OpenThread(THREAD_GET_CONTEXT|THREAD_SET_CONTEXT|THREAD_SUSPEND_RESUME,FALSE,e.th32ThreadID);if(!t)continue;
 FILETIME born,exit,kernel,user;if(GetThreadTimes(t,&born,&exit,&kernel,&user)){uint64_t stamp=((uint64_t)born.dwHighDateTime<<32)|born.dwLowDateTime;if(creationTimes[e.th32ThreadID]!=stamp){originals.erase(e.th32ThreadID);applied.erase(e.th32ThreadID);creationTimes[e.th32ThreadID]=stamp;}}
 // Allocate bookkeeping before suspension: a stopped target may own a heap lock.
 auto appliedIt=applied.try_emplace(e.th32ThreadID,0).first;
 if(!restore&&active&&appliedIt->second==revision){CloseHandle(t);continue;}
 auto it=originals.find(e.th32ThreadID);bool saveOriginal=!restore&&it==originals.end();
 if(restore&&it==originals.end()){CloseHandle(t);continue;}
 if(saveOriginal)it=originals.try_emplace(e.th32ThreadID,std::array<DWORD64,6>{}).first;
 if(SuspendThread(t)==(DWORD)-1){if(saveOriginal)originals.erase(it);CloseHandle(t);continue;}CONTEXT c{};c.ContextFlags=CONTEXT_DEBUG_REGISTERS;
 bool gotContext=GetThreadContext(t,&c)!=FALSE;
 if(gotContext){DWORD64* dr[]={&c.Dr0,&c.Dr1,&c.Dr2,&c.Dr3};
 if(saveOriginal)it->second={c.Dr0,c.Dr1,c.Dr2,c.Dr3,c.Dr6,c.Dr7};
 if(it!=originals.end()){for(unsigned i=0;i<4;i++){auto orig=it->second;
 if(restore||!hooks[i].address){if(*dr[i]==hooks[i].address||*dr[i]==0||((*dr[i]!=orig[i])&&!(orig[5]&(3ull<<(2*i))))){*dr[i]=orig[i];c.Dr7=(c.Dr7&~((3ull<<(2*i))|(15ull<<(16+4*i))))|(orig[5]&((3ull<<(2*i))|(15ull<<(16+4*i))));}}
 else if(orig[5]&(3ull<<(2*i))){conflicts++;}else{*dr[i]=hooks[i].address;c.Dr7=(c.Dr7&~((3ull<<(2*i))|(15ull<<(16+4*i))))|(1ull<<(2*i));}}
 if(SetThreadContext(t,&c))appliedIt->second=revision;}}
 ResumeThread(t);CloseHandle(t);if(saveOriginal&&!gotContext)originals.erase(it);
 }while(Thread32Next(snap,&e));CloseHandle(snap);if(!active||restore){originals.clear();applied.clear();creationTimes.clear();}
}
struct Watch{uintptr_t address;unsigned length;std::vector<BYTE> previous;uint64_t changes=0,samples=0,failed=0;};static std::map<std::string,Watch> watches;
static void sampleWatches(){for(auto& item:watches){auto& w=item.second;std::vector<BYTE> d;if(readable(w.address,w.length,d)){w.samples++;if(w.previous!=d){w.changes++;w.previous=std::move(d);}}else w.failed++;}}
static std::string dispatch(const std::string& line){
 std::istringstream in(line);std::string cmd;in>>cmd;std::ostringstream out;uintptr_t base=(uintptr_t)GetModuleHandleW(nullptr);
 if(cmd=="STATUS"){LARGE_INTEGER f;QueryPerformanceFrequency(&f);out<<"{\"ok\":true,\"protocol\":1,\"pid\":"<<GetCurrentProcessId()<<",\"image_base\":\""<<hx(base)<<"\",\"qpc_frequency\":"<<f.QuadPart<<",\"hits\":"<<total<<",\"dropped_or_overwritten\":"<<drops<<",\"thread_slot_conflicts\":"<<conflicts<<",\"hooks\":[";for(int i=0;i<4;i++){if(i)out<<',';out<<"{\"slot\":"<<i<<",\"address\":\""<<hx(hooks[i].address)<<"\",\"label\":"<<quote(hooks[i].label)<<",\"hits\":"<<hooks[i].hits<<"}";}out<<"],\"capabilities\":[\"inventory\",\"bounded_read\",\"absolute_watches\",\"4_hardware_entry_probes\",\"gp_xmm_stack_register_snapshot\"],\"decoded_bone_or_traversal_layouts\":false}";}
 else if(cmd=="MODULES"){HANDLE s=CreateToolhelp32Snapshot(TH32CS_SNAPMODULE|TH32CS_SNAPMODULE32,GetCurrentProcessId());MODULEENTRY32W e{};e.dwSize=sizeof e;out<<"{\"ok\":true,\"modules\":[";bool first=true;if(Module32FirstW(s,&e))do{if(!first)out<<',';first=false;out<<"{\"name\":"<<quote(narrow(e.szModule))<<",\"path\":"<<quote(narrow(e.szExePath))<<",\"base\":\""<<hx((uintptr_t)e.modBaseAddr)<<"\",\"size\":"<<e.modBaseSize<<"}";}while(Module32NextW(s,&e));CloseHandle(s);out<<"]}";}
 else if(cmd=="THREADS"){HANDLE s=CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD,0);THREADENTRY32 e{};e.dwSize=sizeof e;out<<"{\"ok\":true,\"threads\":[";bool first=true;if(Thread32First(s,&e))do{if(e.th32OwnerProcessID==GetCurrentProcessId()){if(!first)out<<',';first=false;out<<e.th32ThreadID;}}while(Thread32Next(s,&e));CloseHandle(s);out<<"]}";}
 else if(cmd=="REGIONS"){std::string a;in>>a;uint64_t address=0;if(!a.empty()&&!number(a,address))return fail("invalid address");out<<"{\"ok\":true,\"regions\":[";MEMORY_BASIC_INFORMATION m{};unsigned count=0;while(count<256&&VirtualQuery((void*)address,&m,sizeof m)){if(count++)out<<',';out<<"{\"base\":\""<<hx((uintptr_t)m.BaseAddress)<<"\",\"size\":"<<m.RegionSize<<",\"state\":"<<m.State<<",\"protect\":"<<m.Protect<<",\"type\":"<<m.Type<<"}";uint64_t next=(uintptr_t)m.BaseAddress+m.RegionSize;if(next<=address)break;address=next;}out<<"],\"next\":\""<<hx(address)<<"\"}";}
 else if(cmd=="READ"){std::string a,b;in>>a>>b;uint64_t address,n;std::vector<BYTE>d;if(!number(a,address)||!number(b,n,10)||!readable(address,(size_t)n,d))return fail("READ requires readable address and decimal length 1..65536");out<<"{\"ok\":true,\"address\":\""<<hx(address)<<"\",\"hex\":\""<<hexbytes(d.data(),d.size())<<"\"}";}
 else if(cmd=="HOOK"||cmd=="HOOKVA"){unsigned slot,every=1,reg=0,bytes=0;int64_t offset=0;std::string rva,expected,label,category="unknown";in>>slot>>rva>>expected>>label;if(!in||slot>=4||label.size()>64)return fail("HOOK slot rva expected_hex label [every reg_index signed_offset bytes]");if(in>>every){if(!(in>>reg>>offset>>bytes)||!every||reg>=16||bytes>256)return fail("invalid sampling config");in>>category;if(category.size()>64)return fail("category too long");}if(cmd=="HOOKVA")base=0;uint64_t r; if(!number(rva,r)||r>0x7fffffffffff-base||expected.empty()||expected.size()>64||expected.size()%2)return fail("invalid RVA/expected bytes");std::vector<BYTE>d;if(!readable(base+r,expected.size()/2,d)||hexbytes(d.data(),d.size())!=expected)return fail("expected bytes mismatch/unreadable");MEMORY_BASIC_INFORMATION m{};VirtualQuery((void*)(base+r),&m,sizeof m);if(!(m.Protect&(PAGE_EXECUTE|PAGE_EXECUTE_READ|PAGE_EXECUTE_READWRITE|PAGE_EXECUTE_WRITECOPY)))return fail("address is not executable");if(IsDebuggerPresent())return fail("hardware probes unavailable with debugger attached");bool known=false;for(LONG k=0;k<ownedCounts[slot];k++)known|=ownedSites[slot][k]==base+r;if(!known){if(ownedCounts[slot]>=1024)return fail("owned-site limit reached; restart process");ownedSites[slot][ownedCounts[slot]]=base+r;InterlockedIncrement(&ownedCounts[slot]);}lock();hooks[slot]={base+r,label,0,every,reg,offset,bytes,category,++revision};unlock();updateThreads();out<<"{\"ok\":true,\"address\":\""<<hx(base+r)<<"\"}";}
 else if(cmd=="REDIRECT"){unsigned slot;std::string fromText,toText,returnText;uint64_t from,to,returnAddress;in>>slot>>fromText>>toText>>returnText;if(!in||slot>=4||!number(fromText,from)||!number(toText,to)||!number(returnText,returnAddress)||!returnAddress)return fail("REDIRECT slot hex-from hex-to hex-return-address");lock();if(!hooks[slot].address){unlock();return fail("REDIRECT requires an active hook in that slot");}hooks[slot].redirectRcx=true;hooks[slot].redirectFrom=from;hooks[slot].redirectTo=to;hooks[slot].redirectReturn=(uintptr_t)returnAddress;unlock();out<<"{\"ok\":true,\"slot\":"<<slot<<",\"from\":\""<<hx(from)<<"\",\"to\":\""<<hx(to)<<"\",\"return_address\":\""<<hx(returnAddress)<<"\"}";}
 else if(cmd=="UNREDIRECT"){unsigned slot;in>>slot;if(!in||slot>=4)return fail("invalid slot");lock();hooks[slot].redirectRcx=false;hooks[slot].redirectFrom=hooks[slot].redirectTo=0;hooks[slot].redirectReturn=0;unlock();out<<"{\"ok\":true}";}
 else if(cmd=="UNHOOK"){unsigned slot;in>>slot;if(!in||slot>=4)return fail("invalid slot");lock();hooks[slot].address=0;++revision;unlock();updateThreads();out<<"{\"ok\":true}";}
 else if(cmd=="EVENTS"){unsigned n=64;in>>n;n=std::min(n,256u);std::vector<Event> copy;lock();unsigned take=std::min(n,eventCount);for(unsigned k=eventCount-take;k<eventCount;k++)copy.push_back(events[(nextEvent+256-eventCount+k)%256]);eventCount=0;unlock();out<<"{\"ok\":true,\"events\":[";bool first=true;for(const auto& e:copy){if(!first)out<<',';first=false;out<<"{\"sequence\":"<<e.sequence<<",\"qpc\":"<<e.ticks<<",\"tid\":"<<e.tid<<",\"slot\":"<<e.slot<<",\"label\":"<<quote(e.label)<<",\"category\":"<<quote(e.category)<<",\"revision\":"<<e.revision<<",\"rcx_after\":\""<<hx(e.rcxAfter)<<"\",\"rcx_redirected\":"<<(e.rcxRedirected?"true":"false")<<",\"return_address\":\""<<hx(e.returnAddress)<<"\",\"probe_address\":\""<<hx(e.address)<<"\",\"rip\":\""<<hx(e.context.Rip)<<"\",\"registers\":[";for(unsigned i=0;i<16;i++){if(i)out<<',';out<<'"'<<hx(regValue(e.context,i))<<'"';}out<<"],\"xmm0_5_hex\":\""<<hexbytes((const BYTE*)&e.context.Xmm0,6*16)<<"\",\"stack_hex\":\""<<hexbytes((const BYTE*)e.stack,e.stackBytes)<<"\",\"snapshot_address\":\""<<hx(e.sampleAddress)<<"\",\"snapshot_hex\":\""<<hexbytes(e.sample,e.sampleBytes)<<"\"}";}out<<"]}";}
 else if(cmd=="WATCH"){std::string name,a;uint64_t address,n;std::string b;in>>name>>a>>b;if(name.empty()||name.size()>64||!number(a,address)||!number(b,n,10)||!n||n>4096||watches.size()>=32)return fail("WATCH label absolute_address decimal_length (max 32 watches,4096 bytes each)");watches[name]={address,(unsigned)n,{},0,0,0};out<<"{\"ok\":true}";}
 else if(cmd=="WATCHES"){sampleWatches();out<<"{\"ok\":true,\"watches\":[";bool first=true;for(auto& item:watches){if(!first)out<<',';first=false;auto& w=item.second;out<<"{\"label\":"<<quote(item.first)<<",\"address\":\""<<hx(w.address)<<"\",\"changes\":"<<w.changes<<",\"samples\":"<<w.samples<<",\"failed\":"<<w.failed<<",\"hex\":\""<<hexbytes(w.previous.data(),w.previous.size())<<"\"}";}out<<"]}";}
 else if(cmd=="UNWATCH"){std::string name;in>>name;watches.erase(name);out<<"{\"ok\":true}";}
 else if(cmd=="SHUTDOWN"){InterlockedExchange(&stopFlag,1);out<<"{\"ok\":true,\"shutdown\":true}";}
 else return fail("unknown command");return out.str();
}
static bool transfer(HANDLE pipe,bool write,void* data,DWORD count,DWORD& done){OVERLAPPED o{};o.hEvent=CreateEventW(nullptr,TRUE,FALSE,nullptr);BOOL ok=write?WriteFile(pipe,data,count,&done,&o):ReadFile(pipe,data,count,&done,&o);if(!ok&&GetLastError()==ERROR_IO_PENDING){DWORD w=WaitForSingleObject(o.hEvent,5000);if(w==WAIT_OBJECT_0)ok=GetOverlappedResult(pipe,&o,&done,FALSE);else{CancelIoEx(pipe,&o);WaitForSingleObject(o.hEvent,INFINITE);ok=FALSE;}}CloseHandle(o.hEvent);return ok!=FALSE;}
#include "startup_dump.h"
static DWORD WINAPI worker(void*){
 startupDump();
 void* veh=AddVectoredExceptionHandler(1,handler);wchar_t name[128];swprintf_s(name,L"\\\\.\\pipe\\acu-probe-%lu",GetCurrentProcessId());
 HANDLE pipe=CreateNamedPipeW(name,PIPE_ACCESS_DUPLEX|FILE_FLAG_OVERLAPPED,PIPE_TYPE_MESSAGE|PIPE_READMODE_MESSAGE|PIPE_WAIT|PIPE_REJECT_REMOTE_CLIENTS,1,262144,65536,1000,nullptr);
 if(pipe!=INVALID_HANDLE_VALUE)while(!stopFlag){OVERLAPPED o{};o.hEvent=CreateEventW(nullptr,TRUE,FALSE,nullptr);BOOL connected=ConnectNamedPipe(pipe,&o);DWORD err=GetLastError();if(!connected&&err==ERROR_IO_PENDING){while(!stopFlag&&WaitForSingleObject(o.hEvent,50)==WAIT_TIMEOUT){updateThreads();sampleWatches();}if(stopFlag){CancelIoEx(pipe,&o);WaitForSingleObject(o.hEvent,INFINITE);}else{DWORD n;connected=GetOverlappedResult(pipe,&o,&n,FALSE);}}else if(!connected&&err==ERROR_PIPE_CONNECTED)connected=TRUE;CloseHandle(o.hEvent);if(!connected)continue;
 char buf[65536];DWORD n=0;if(transfer(pipe,false,buf,sizeof buf-1,n)&&n){buf[n]=0;std::string response;try{response=dispatch(buf);}catch(...){response=fail("command exception");}response+='\n';DWORD sent=0;if(transfer(pipe,true,response.data(),(DWORD)response.size(),sent)){char ack[8];DWORD ackBytes=0;transfer(pipe,false,ack,sizeof ack,ackBytes);}}DisconnectNamedPipe(pipe);}
 updateThreads(true);if(pipe!=INVALID_HANDLE_VALUE)CloseHandle(pipe);(void)veh;return 0;
}
BOOL WINAPI DllMain(HINSTANCE h,DWORD reason,void*){if(reason==DLL_PROCESS_ATTACH){self=h;DisableThreadLibraryCalls(h);HANDLE t=CreateThread(nullptr,0,worker,nullptr,0,nullptr);if(t)CloseHandle(t);else return FALSE;}return TRUE;}






