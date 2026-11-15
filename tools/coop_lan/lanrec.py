"""lanrec.py <joiner_pid> <host_pid>: read-only. Walks the joiner's Storm LAN session map (controller iface+152,
rb-tree: root at +16, node [0]=left [1]=right, key at node+32, record pointer at node+72) and prints each 840-byte
record: PeerDescriptor at rec+24 (its Address at +24+112: sockaddr port/IP), session-info pointer at rec+640 and the
info's id descriptor at +72 ({cap,len,ptr}). Compares with the host's game/party/mission session ids."""
import ctypes as C, ctypes.wintypes as W, struct, sys, json
K=C.WinDLL("kernel32"); K.OpenProcess.restype=W.HANDLE
K.ReadProcessMemory.argtypes=[W.HANDLE,C.c_void_p,C.c_void_p,C.c_size_t,C.POINTER(C.c_size_t)]
J,Hs=int(sys.argv[1]),int(sys.argv[2]); H={p:K.OpenProcess(0x0410,False,p) for p in (J,Hs)}
def rd(p,a,n):
    b,g=C.create_string_buffer(n),C.c_size_t()
    return b.raw if a and K.ReadProcessMemory(H[p],C.c_void_p(a),b,n,C.byref(g)) else None
def q(p,a): r=rd(p,a,8); return struct.unpack("<Q",r)[0] if r else 0
def desc(p,a):
    cap,ln,ptr=struct.unpack("<QQQ",rd(p,a,24)); return rd(p,ptr,ln).hex() if ptr and 0<ln<64 else f"cap={cap} len={ln} ptr={ptr:x}"
SESS={"game":(0x145212F48,32),"party":(0x145212AA0,24),"mission":(0x145212EC8,32)}
for n,(g,o) in SESS.items():
    print("host",n,desc(Hs,q(Hs,g)+o+8),"| joiner",n,desc(J,q(J,g)+o+8))
iface=q(J,0x145212ED0)+8; hdr=iface+152
print("iface",hex(iface),"hdr",[hex(q(J,hdr+8*i)) for i in range(4)])
def walk(n,d=0):
    if not n or d>20: return
    walk(q(J,n),d+1)
    rec=q(J,n+72); info=q(J,rec+640)
    pd=rec+24; ad=pd+112; sa=rd(J,ad+16,8)
    fam,port=struct.unpack_from("<H",sa,0)[0],struct.unpack_from(">H",sa,2)[0]
    print(json.dumps({"node":hex(n),"key":desc(J,n+32),"rec":hex(rec),"rec_vt":hex(q(J,rec)),"ints":struct.unpack("<iii",rd(J,rec+12,12)),
      "pd_vt":hex(q(J,pd)),"addr_vt":hex(q(J,ad)),"addr_kind":struct.unpack("<i",rd(J,ad+8,4))[0],"fam":fam,"port":port,
      "ipv4":".".join(str(b) for b in sa[4:8]),"port144":struct.unpack("<H",rd(J,ad+144,2))[0],
      "info":hex(info),"info_vt":hex(q(J,info)),"info_id":desc(J,info+72) if info else None}))
    walk(q(J,n+8),d+1)
walk(q(J,hdr+16))
