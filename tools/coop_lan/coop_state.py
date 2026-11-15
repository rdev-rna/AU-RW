"""coop_state.py pid [pid...]: read-only snapshot of LAN co-op state (ACU-RE-0090)."""
import ctypes as C, ctypes.wintypes as W, struct, sys, json, re
K=C.WinDLL("kernel32",use_last_error=True); K.OpenProcess.restype=W.HANDLE
K.ReadProcessMemory.argtypes=[W.HANDLE,C.c_void_p,C.c_void_p,C.c_size_t,C.POINTER(C.c_size_t)]
def snap(pid):
    h=K.OpenProcess(0x0010|0x0400,False,pid)
    def rd(a,n):
        b=C.create_string_buffer(n);g=C.c_size_t()
        return b.raw if a and K.ReadProcessMemory(h,C.c_void_p(a),b,n,C.byref(g)) else None
    q=lambda a:(struct.unpack('<Q',rd(a,8))[0] if rd(a,8) else 0)
    i32=lambda a:(struct.unpack('<i',rd(a,4))[0] if rd(a,4) else None)
    o={'pid':pid,'flag':i32(0x144E52380)}
    mm=q(0x145137930); sc=mm+1472
    raw=rd(sc+224,32) or b''
    m=re.match(rb'[A-Z_]{3,}',raw)
    if not m:
        p=struct.unpack_from('<Q',raw,0)[0] if len(raw)>=8 else 0
        raw2=rd(p,48) or b''; m=re.match(rb'[A-Z_]{3,}',raw2)
    o['mm+152']=i32(mm+152); o['ctx']=m.group().decode() if m else raw[:32].hex()
    o['start_alone+236']=(rd(sc+236,1) or b'\xff')[0]; o['sc+232']=i32(sc+232)
    for name,g,off in (('game',0x145212F48,0x20),('mission',0x145212EC8,32),('party',0x145212AA0,24)):
        base=q(g); s=base+off
        o[name]={'id':(rd(s+8,8) or b'').hex(),'peer':i32(s+48),'peerptr':hex(q(s+0x90)),'nettime':i32(s+0x98)}
        if name=='mission':
            o[name]['state196']=i32(base+196)
            pl=q(s+144); o[name]['peerlist']=hex(pl); o[name]['counts16_20']=(i32(pl+16),i32(pl+20)) if pl else None
    nsc=q(0x145212ED0)+8
    o['storm']={'nettype+676':i32(nsc+676),'state+688':i32(nsc+688)}
    return o
for p in sys.argv[1:]: print(json.dumps(snap(int(p))))
