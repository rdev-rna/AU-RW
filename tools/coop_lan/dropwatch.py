"""dropwatch.py <joiner_pid> <host_pid> <out.jsonl> [--max 600] [--after 20] [--skip-leave]
Experiment (ACU-RE-0090 run 14): catch why a LAN join drops.
 A. Joiner leaves the host's mission session with ACU's own LeaveSession 0x141C55040(mission_base) (CALLAT on
    Storm slot 12), only if mission base+196 == 3. Then waits for discovery to store the host's record again.
 B. Arms drop hooks with 256-byte stack snapshots (RSP):
      joiner: Leave(23) 0x141B30A40, Create(17) 0x141B2CFF0, router Disconnect(29) 0x141B2D650, and after the
              CALLAT the slot-0 hook becomes Secede(24) 0x141B35EA0;
      host:   Kick(25) 0x141B309C0, Leave(23), router Disconnect(29), Dissolve(26) 0x141B2D6F0.
    Calls JoinSession 0x141C54B30(mission_session, *(rec+640), rec+24, 0) like run 13.
 C. Polls both games' sessions every 0.25 s; logs every change and every hook hit (with code return addresses
    found in the stack snapshot). Stops --after seconds after the joiner's game session leaves the host's id,
    or at --max. Always cancels CALLAT and unhooks."""
import argparse, ctypes as C, ctypes.wintypes as W, json, struct, sys, time
sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parents[1] / "native_probe"))
from client import checked, command  # noqa
ap = argparse.ArgumentParser(); ap.add_argument("joiner", type=int); ap.add_argument("host", type=int); ap.add_argument("out")
ap.add_argument("--max", type=float, default=600); ap.add_argument("--after", type=float, default=20); ap.add_argument("--skip-leave", action="store_true"); ap.add_argument("--watch-only", action="store_true", help="already joined: skip leave, record and join; only arm hooks and watch")
a = ap.parse_args(); J, Hs = a.joiner, a.host
K = C.WinDLL("kernel32"); K.OpenProcess.restype = W.HANDLE
K.ReadProcessMemory.argtypes = [W.HANDLE, C.c_void_p, C.c_void_p, C.c_size_t, C.POINTER(C.c_size_t)]
H = {p: K.OpenProcess(0x0410, False, p) for p in (J, Hs)}
def rd(p, addr, n):
    b, g = C.create_string_buffer(n), C.c_size_t()
    return b.raw if addr and K.ReadProcessMemory(H[p], C.c_void_p(addr), b, n, C.byref(g)) else None
def q(p, addr): r = rd(p, addr, 8); return struct.unpack("<Q", r)[0] if r else 0
def i32(p, addr): r = rd(p, addr, 4); return struct.unpack("<i", r)[0] if r else None
out = open(a.out, "a", encoding="utf-8"); t0 = time.monotonic()
def emit(**r):
    r.setdefault("t_s", round(time.monotonic() - t0, 3)); r.setdefault("unix", round(time.time(), 1))
    out.write(json.dumps(r) + "\n"); out.flush(); print(json.dumps(r)[:500], flush=True)
SESS = {"game": (0x145212F48, 32), "party": (0x145212AA0, 24), "mission": (0x145212EC8, 32)}
def desc(p, addr):
    r = rd(p, addr, 24)
    if not r: return None
    cap, ln, ptr = struct.unpack("<QQQ", r); return rd(p, ptr, ln).hex() if ptr and 0 < ln < 64 else None
def sessions(p):
    o = {}
    for n, (g, off) in SESS.items():
        base = q(p, g); s = base + off; rec = q(p, s + 144)
        o[n] = {"id": desc(p, s + 8), "h": i32(p, s + 48), "st196": i32(p, base + 196) if n == "mission" else None,
                "rec20": i32(p, rec + 20) if rec else None}
    return o
def clock(p): return i32(p, q(p, 0x145212F48) + 32 + 0x98)
def lan_record(p):
    iface = q(p, 0x145212ED0) + 8; node = q(p, iface + 152 + 16)
    if not node: return None
    rec = q(p, node + 72); info = q(p, rec + 640) if rec else 0
    return {"rec": rec, "info": info, "id": desc(p, info + 72) if info else None, "ok": q(p, rec) == 0x142FFE588 and q(p, rec + 24) == 0x142FFD960}
IMG = __import__("os").environ.get("ACU_MAPPED_IMAGE", r"E:/ReverseEngineering/Projects/Unity/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin")
img = open(IMG, "rb").read()
def hook(p, slot, rva, label):
    checked(p, f"HOOK {slot} {rva:x} {img[rva:rva + 16].hex()} {label} 1 9 0 256 dropwatch"); armed[p].add(slot)
def callat(p, target, *args):
    emit(event="callat", target=hex(target), args=[hex(x) for x in args], result=checked(p, f"CALLAT 0 {target:x} " + " ".join(f"{x:x}" for x in args)))
    st = {}
    for _ in range(300):
        st = checked(p, "CALLSTATUS")
        if st["state"] == 3: break
        time.sleep(0.01)
    emit(event="callstatus", status=st)
    if st["state"] != 3: emit(event="cancel", result=checked(p, "CALLCANCEL"))
    return st
def stack_codes(e):
    h = e.get("snapshot_hex") or ""; b = bytes.fromhex(h); r = []
    for i in range(0, len(b) - 7, 8):
        v = struct.unpack_from("<Q", b, i)[0]
        if 0x140001000 <= v < 0x142E00000: r.append(hex(v))
    return r[:16]
armed = {J: set(), Hs: set()}
try:
    host_mid = desc(Hs, q(Hs, 0x145212EC8) + 32 + 8); host_gid = desc(Hs, q(Hs, 0x145212F48) + 32 + 8)
    emit(event="start", joiner=sessions(J), host=sessions(Hs), host_mission=host_mid, host_game=host_gid, rec=lan_record(J))
    if not a.skip_leave and not a.watch_only:
        mb = q(J, 0x145212EC8)
        if i32(J, mb + 196) == 3:
            hook(J, 0, 0x1C626C0, "storm_slot12_update")
            callat(J, 0x141C55040, mb, 0, 0, 0)
            checked(J, "UNHOOK 0"); armed[J].discard(0)
            emit(event="left_mission", joiner=sessions(J))
    rec = None; end = time.monotonic() + (0 if a.watch_only else 30)
    while time.monotonic() < end:
        rec = lan_record(J)
        if rec and rec["ok"] and rec["id"] == host_mid: break
        time.sleep(0.25)
    emit(event="record", rec={k: (hex(v) if isinstance(v, int) else v) for k, v in (rec or {}).items()})
    if not a.watch_only and (not rec or not rec["ok"] or rec["id"] != host_mid):
        emit(event="abort", reason="no LAN record for the host mission session"); sys.exit(1)
    hook(J, 1, 0x1B30A40, "j_leave23"); hook(J, 2, 0x1B2CFF0, "j_create17"); hook(J, 3, 0x1B2D650, "j_rdisconnect29")
    hook(Hs, 0, 0x1B309C0, "h_kick25"); hook(Hs, 1, 0x1B30A40, "h_leave23"); hook(Hs, 2, 0x1B2D650, "h_rdisconnect29"); hook(Hs, 3, 0x1B2D6F0, "h_dissolve26")
    checked(J, "EVENTS 256"); checked(Hs, "EVENTS 256")
    if not a.watch_only:
        hook(J, 0, 0x1C626C0, "storm_slot12_update")
        ms = q(J, 0x145212EC8) + 32
        callat(J, 0x141C54B30, ms, rec["info"], rec["rec"] + 24, 0)
        checked(J, "UNHOOK 0"); armed[J].discard(0)
    hook(J, 0, 0x1B35EA0, "j_secede24")
    last = {}; joined_at = drop_at = None; stop = time.monotonic() + a.max; nxt_ev = 0
    while time.monotonic() < stop:
        now = time.monotonic()
        for p, tag in ((J, "joiner"), (Hs, "host")):
            s = sessions(p)
            if s != last.get(tag): emit(event="sessions", who=tag, s=s, clock=clock(p)); last[tag] = s
        g = last["joiner"]["game"]["id"]
        if g == host_gid and joined_at is None: joined_at = now; emit(event="joined_game_session")
        if joined_at and g != host_gid and drop_at is None:
            drop_at = now; emit(event="DROP", after_join_s=round(now - joined_at, 3)); stop = min(stop, now + a.after)
        if now >= nxt_ev:
            nxt_ev = now + 1
            for p, tag in ((J, "joiner"), (Hs, "host")):
                for e in checked(p, "EVENTS 256").get("events", []):
                    emit(event="hit", who=tag, label=e["label"], tid=e["tid"], ret=e["return_address"],
                         regs=e["registers"][:4] if isinstance(e["registers"], list) else str(e["registers"])[:120], stack=stack_codes(e))
        time.sleep(0.25)
    emit(event="end", joiner=sessions(J), host=sessions(Hs), dropped=drop_at is not None)
finally:
    try: command(J, "CALLCANCEL")
    except Exception: pass
    for p, slots in armed.items():
        for s in list(slots):
            try: checked(p, f"UNHOOK {s}")
            except Exception: pass
    emit(event="disarmed"); out.close()
