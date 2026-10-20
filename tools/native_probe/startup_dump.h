// Included after probe helpers; opt-in bounded native startup snapshot.
#include <bcrypt.h>
#pragma comment(lib,"bcrypt.lib")
static std::string hashFile(const wchar_t* path){
 HANDLE file=CreateFileW(path,GENERIC_READ,FILE_SHARE_READ|FILE_SHARE_WRITE,nullptr,OPEN_EXISTING,0,nullptr);
 if(file==INVALID_HANDLE_VALUE)return "unavailable";
 BCRYPT_ALG_HANDLE alg=nullptr;BCRYPT_HASH_HANDLE hash=nullptr;std::string result="unavailable";
 if(BCryptOpenAlgorithmProvider(&alg,BCRYPT_SHA256_ALGORITHM,nullptr,0)>=0&&BCryptCreateHash(alg,&hash,nullptr,0,nullptr,0,0)>=0){
 BYTE chunk[65536],digest[32];DWORD got=0;bool ok=true;
 for(;;){if(!ReadFile(file,chunk,sizeof chunk,&got,nullptr)){ok=false;break;}if(!got)break;if(BCryptHashData(hash,chunk,got,0)<0){ok=false;break;}}
 if(ok&&BCryptFinishHash(hash,digest,sizeof digest,0)>=0)result=hexbytes(digest,sizeof digest);
 }
 if(hash)BCryptDestroyHash(hash);if(alg)BCryptCloseAlgorithmProvider(alg,0);CloseHandle(file);return result;
}
static void startupDump(){
 wchar_t enabled[16];if(GetEnvironmentVariableW(L"ACU_PROBE_STARTUP_DUMP",enabled,16)!=1||enabled[0]!=L'1')return;
 wchar_t delayText[32]{};GetEnvironmentVariableW(L"ACU_PROBE_STARTUP_DUMP_DELAY_MS",delayText,32);unsigned delay=delayText[0]?wcstoul(delayText,nullptr,10):500;
 if(delay>2000)return;Sleep(delay);
 MODULEINFO info{};if(!GetModuleInformation(GetCurrentProcess(),GetModuleHandleW(nullptr),&info,sizeof info)||!info.SizeOfImage||info.SizeOfImage>512*1024*1024)return;
 wchar_t dllPath[32768],diskPath[32768];GetModuleFileNameW(self,dllPath,32768);GetModuleFileNameW(nullptr,diskPath,32768);
 std::wstring folder=dllPath;folder=folder.substr(0,folder.find_last_of(L"\\/"));
 std::wstring stem=folder+L"\\startup-"+std::to_wstring(GetCurrentProcessId())+L"-"+std::to_wstring(GetTickCount64());
 std::wstring binary=stem+L".mapped.bin",metadata=stem+L".json";
 HANDLE file=CreateFileW(binary.c_str(),GENERIC_WRITE,FILE_SHARE_READ,nullptr,CREATE_NEW,0,nullptr);if(file==INVALID_HANDLE_VALUE)return;
 LARGE_INTEGER size;size.QuadPart=info.SizeOfImage;bool ok=SetFilePointerEx(file,size,nullptr,FILE_BEGIN)&&SetEndOfFile(file);
 uintptr_t base=(uintptr_t)info.lpBaseOfDll,end=base+info.SizeOfImage;std::ostringstream regions,holes;bool firstRegion=true,firstHole=true;
 LARGE_INTEGER startQpc;QueryPerformanceCounter(&startQpc);uint64_t readBytes=0;
 auto hole=[&](uintptr_t a,size_t n,const char* reason){if(!firstHole)holes<<',';firstHole=false;holes<<"{\"address\":\""<<hx(a)<<"\",\"length\":"<<n<<",\"reason\":"<<quote(reason)<<"}";};
 auto write=[&](uintptr_t a,const BYTE* data,DWORD n){LARGE_INTEGER at;at.QuadPart=a-base;DWORD wrote=0;if(!SetFilePointerEx(file,at,nullptr,FILE_BEGIN)||!WriteFile(file,data,n,&wrote,nullptr)||wrote!=n){ok=false;return;}readBytes+=n;};
 BYTE chunk[65536];
 for(uintptr_t address=base;address<end&&ok;){
 MEMORY_BASIC_INFORMATION region{};if(!VirtualQuery((void*)address,&region,sizeof region)){hole(address,end-address,"VirtualQuery failed");break;}
 uintptr_t regionEnd=std::min(end,(uintptr_t)region.BaseAddress+region.RegionSize);if(regionEnd<=address){ok=false;break;}
 if(!firstRegion)regions<<',';firstRegion=false;regions<<"{\"base\":\""<<hx(address)<<"\",\"size\":"<<(regionEnd-address)<<",\"state\":"<<region.State<<",\"protect\":"<<region.Protect<<",\"type\":"<<region.Type<<"}";
 if(region.State!=MEM_COMMIT||(region.Protect&PAGE_GUARD)||!(region.Protect&(2|4|8|32|64|128)))hole(address,regionEnd-address,"unreadable or guard region");
 else for(uintptr_t pos=address;pos<regionEnd&&ok;){DWORD count=(DWORD)std::min((uintptr_t)sizeof chunk,regionEnd-pos);SIZE_T got=0;
 if(ReadProcessMemory(GetCurrentProcess(),(void*)pos,chunk,count,&got)&&got==count)write(pos,chunk,count);
 else for(DWORD offset=0;offset<count&&ok;offset+=4096){DWORD page=std::min(4096ul,count-offset);got=0;if(ReadProcessMemory(GetCurrentProcess(),(void*)(pos+offset),chunk,page,&got)&&got==page)write(pos+offset,chunk,page);else hole(pos+offset,page,"ReadProcessMemory failed");}
 pos+=count;}
 address=regionEnd;
 }
 CloseHandle(file);LARGE_INTEGER finish;QueryPerformanceCounter(&finish);
 std::ostringstream json;json<<"{\"version\":1,\"kind\":\"native-startup-mapped-image\",\"pid\":"<<GetCurrentProcessId()<<",\"base\":\""<<hx(base)<<"\",\"size\":"<<info.SizeOfImage<<",\"disk_path\":"<<quote(narrow(diskPath))<<",\"disk_sha256\":"<<quote(hashFile(diskPath))<<",\"mapped_path\":"<<quote(narrow(binary.c_str()))<<",\"mapped_sha256\":"<<quote(hashFile(binary.c_str()))<<",\"delay_ms\":"<<delay<<",\"qpc_start\":"<<startQpc.QuadPart<<",\"qpc_finish\":"<<finish.QuadPart<<",\"bytes_read\":"<<readBytes<<",\"completed_io\":"<<(ok?"true":"false")<<",\"coherent_snapshot\":false,\"regions\":["<<regions.str()<<"],\"read_failures\":["<<holes.str()<<"]}";
 std::string text=json.str();HANDLE manifest=CreateFileW(metadata.c_str(),GENERIC_WRITE,FILE_SHARE_READ,nullptr,CREATE_NEW,0,nullptr);if(manifest!=INVALID_HANDLE_VALUE){DWORD wrote;WriteFile(manifest,text.data(),(DWORD)text.size(),&wrote,nullptr);CloseHandle(manifest);}
}
