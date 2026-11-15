"""lanjoin2.py <joiner_pid> <host_pid> <out.jsonl> [--watch 40]
Experiment (ACU-RE-0090): JoinSession. Takes the joiner's stored LAN discovery record (Storm controller iface+152
map; record+24 = host PeerDescriptor, *(record+640) = host session info, id at +72), checks the info id equals the
host's mission-session id, then calls ACU's JoinSession 0x141C54B30(mission_session, info, record+24, 0) once with
CALLAT on Storm slot 12 -- the same call ACU's 0x141C60210 makes for type 1. Logs sessions and socket results in
both games and hits on Storm Join (slot 18). Writes nothing itself.
"""
import argparse, ctypes as C, ctypes.wintypes as W, json, struct, sys, time
sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parents[1] / "native_probe"))
from client import checked, command  # noqa
ap = argparse.ArgumentParser(); ap.add_argument("joiner", type=int); ap.add_argument("host", type=int); ap.add_argument("out")
ap.add_argument("--session", default="game"); ap.add_argument("--watch", type=float, default=30)
a = ap.parse_args()
K = C.WinDLL("kernel32", use_last_error=True); K.OpenProcess.restype = W.HANDLE; K.VirtualAllocEx.restype = C.c_void_p
K.VirtualAllocEx.argtypes = [W.HANDLE, C.c_void_p, C.c_size_t, W.DWORD, W.DWORD]
K.ReadProcessMemory.argtypes = [W.HANDLE, C.c_void_p, C.c_void_p, C.c_size_t, C.POINTER(C.c_size_t)]
K.WriteProcessMemory.argtypes = [W.HANDLE, C.c_void_p, C.c_void_p, C.c_size_t, C.POINTER(C.c_size_t)]
H = {p: K.OpenProcess(0x0010 | 0x0020 | 0x0008 | 0x0400, False, p) for p in (a.joiner, a.host)}
def rd(p, addr, n):
    b, g = C.create_string_buffer(n), C.c_size_t()
    return b.raw if addr and K.ReadProcessMemory(H[p], C.c_void_p(addr), b, n, C.byref(g)) else None
def wr(p, addr, data):
    g = C.c_size_t(); return bool(K.WriteProcessMemory(H[p], C.c_void_p(addr), data, len(data), C.byref(g)))
q = lambda p, addr: struct.unpack("<Q", rd(p, addr, 8))[0]
out = open(a.out, "a", encoding="utf-8"); t0 = time.monotonic()
def emit(**r):
    r.setdefault("t_s", round(time.monotonic() - t0, 3)); r.setdefault("unix", round(time.time(), 1)); out.write(json.dumps(r) + "\n"); out.flush(); print(json.dumps(r)[:400], flush=True)
SESS = {"game": (0x145212F48, 32), "party": (0x145212AA0, 24), "mission": (0x145212EC8, 32)}
def sessions(p):
    r = {}
    for n, (g, off) in SESS.items():
        s = q(p, g) + off; cap, ln, ptr = struct.unpack("<QQQ", rd(p, s + 8, 24))
        pl = q(p, s + 144); cnt = struct.unpack("<ii", rd(p, pl + 16, 8)) if pl else None
        r[n] = {"id": rd(p, ptr, ln).hex() if ptr and 0 < ln < 64 else None, "peer": struct.unpack("<i", rd(p, s + 48, 4))[0],
                "peerlist": hex(pl), "counts": cnt, "nettime": struct.unpack("<i", rd(p, s + 0x98, 4))[0]}
    return r
J, Hs = a.joiner, a.host
ms = q(J, 0x145212EC8) + 32
hms = q(Hs, 0x145212EC8) + 32
cap, ln, ptr = struct.unpack("<QQQ", rd(Hs, hms + 8, 24)); host_mid = rd(Hs, ptr, ln)
iface = q(J, 0x145212ED0) + 8; node = q(J, iface + 152 + 16)
rec = q(J, node + 72) if node else 0; info = q(J, rec + 640) if rec else 0
icap, iln, iptr = struct.unpack("<QQQ", rd(J, info + 72, 24)) if info else (0, 0, 0)
info_id = rd(J, iptr, iln) if iptr and 0 < iln < 64 else b""
emit(event="before", joiner=sessions(J), host=sessions(Hs), rec=hex(rec), info=hex(info), info_id=info_id.hex(), host_mission=host_mid.hex(),
     rec_vt=hex(q(J, rec)) if rec else None, pd_vt=hex(q(J, rec + 24)) if rec else None, info_vt=hex(q(J, info)) if info else None)
if not rec or q(J, rec) != 0x142FFE588 or q(J, rec + 24) != 0x142FFD960 or info_id != host_mid:
    emit(event="abort", reason="record missing or does not describe the host mission session"); sys.exit(1)
HOOKS = [("storm_slot12_update", 0x1C626C0, "40534883ec2083b9b002000002488bd9"),
         ("storm_join_slot18", 0x1C54E60, ""),
         ("storm_join_impl", 0x1B30900, ""),
         ("after_recvfrom", 0x26D7777, "8bd883f8ff751d57e83b6925033d4627")]
IMG = __import__("os").environ.get("ACU_MAPPED_IMAGE", r"E:/ReverseEngineering/Projects/Unity/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin")
img = open(IMG, "rb").read()
armed = {J: [], Hs: []}
try:
    for slot, (label, rva, exp) in enumerate(HOOKS):
        exp = exp or img[rva:rva + 16].hex()
        checked(J, f"HOOK {slot} {rva:x} {exp} {label} 1 0 0 0 lanjoin"); armed[J].append(slot)
    checked(Hs, f"HOOK 0 26d7777 8bd883f8ff751d57e83b6925033d4627 after_recvfrom 1 0 0 0 lanjoin"); armed[Hs].append(0)
    checked(J, "EVENTS 256"); checked(Hs, "EVENTS 256")
    emit(event="callat", result=checked(J, f"CALLAT 0 141c54b30 {ms:x} {info:x} {rec + 24:x} 0"))
    for _ in range(300):
        st = checked(J, "CALLSTATUS")
        if st["state"] == 3: break
        time.sleep(0.01)
    emit(event="callstatus", status=st)
    if st["state"] != 3: emit(event="cancel", result=checked(J, "CALLCANCEL"))
    checked(J, "UNHOOK 0"); armed[J].remove(0)
    end = time.monotonic() + a.watch
    while time.monotonic() < end:
        time.sleep(1)
        for p, tag in ((J, "joiner"), (Hs, "host")):
            evs = checked(p, "EVENTS 256").get("events", [])
            labs = {}
            for e in evs:
                k = e["label"]
                if k == "after_recvfrom":
                    rax = int(e["registers"][4], 16); k += ":" + (str(rax) if rax < 1 << 63 else "-1")
                labs[k] = labs.get(k, 0) + 1
                if e["label"] in ("storm_join_slot18", "storm_join_impl"):
                    emit(event="hit", who=tag, label=e["label"], tid=e["tid"], ret=e["return_address"], regs=e["registers"][:4])
            emit(event="tick", who=tag, labels=labs)
    emit(event="after", joiner=sessions(J), host=sessions(Hs))
finally:
    try: command(J, "CALLCANCEL")
    except Exception: pass
    for p, slots in armed.items():
        for s in slots:
            try: checked(p, f"UNHOOK {s}")
            except Exception: pass
    emit(event="disarmed"); out.close()
