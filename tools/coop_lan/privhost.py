"""privhost.py <pid> <out.jsonl> [--mission 000000104ccb592a] [--public] [--watch 60]
Emulates the world map's "Play in Private" button without the UI. The map handler 0x140D6D440 ends with
  (*(*qword_145219C80) + 136)(qword_145219C80 object, *(*(v1+240)+16) mission id, v1+172 private byte)
and slot 136 is 0x14250CD00, a thunk to sub_14256CC80(qword_145137928, mission, flag) that posts
OnlineRequestMatchmakingEvent. This script checks that slot, then makes the same call on the game thread with the
probe's CALLAT (piggy-backing on Storm interface slot 12, 0x141C626C0, per-frame update) and watches the matchmaker
context string at qword_145137930+1472+224 and the mission id at qword_145137930+0x718. Always cancels and unhooks."""
import argparse, json, re, struct, sys, time
sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parents[1] / "native_probe"))
from client import checked, command  # noqa
ap = argparse.ArgumentParser(); ap.add_argument("pid", type=int); ap.add_argument("out")
ap.add_argument("--mission", default="000000104ccb592a"); ap.add_argument("--public", action="store_true")
ap.add_argument("--watch", type=float, default=60); a = ap.parse_args(); pid = a.pid
out = open(a.out, "a", encoding="utf-8"); t0 = time.monotonic()
def emit(**r):
    r.setdefault("t_s", round(time.monotonic() - t0, 3)); r.setdefault("unix", round(time.time(), 1))
    out.write(json.dumps(r) + "\n"); out.flush(); print(json.dumps(r)[:400], flush=True)
def rd(addr, n): return bytes.fromhex(checked(pid, f"READ {addr:x} {n}")["hex"])
def q(addr): return struct.unpack("<Q", rd(addr, 8))[0]
def ctx():
    mm = q(0x145137930); raw = rd(mm + 1472 + 224, 32); m = re.match(rb"[A-Z_]{3,}", raw)
    if not m:
        p = struct.unpack_from("<Q", raw, 0)[0]; m = re.match(rb"[A-Z_]{3,}", rd(p, 48)) if p else None
    return {"ctx": m.group().decode() if m else None, "mission_718": f"{q(mm + 0x718):016x}", "start_alone": rd(mm + 1472 + 236, 1)[0]}
obj = q(0x145219C80); vt = q(obj) if obj else 0; slot = q(vt + 136) if vt else 0
mission = int(a.mission, 16); flag = 0 if a.public else 1
emit(event="target", obj=hex(obj), vtable=hex(vt), slot136=hex(slot), mission=f"{mission:016x}", private=flag, before=ctx())
if slot != 0x14250CD00:
    emit(event="abort", reason="slot 136 is not the Play thunk 0x14250CD00"); sys.exit(1)
armed = []
try:
    checked(pid, "HOOK 0 1c626c0 40534883ec2083b9b002000002488bd9 storm_slot12_update 1 0 0 0 privhost"); armed.append(0)
    emit(event="callat", result=checked(pid, f"CALLAT 0 14250cd00 {obj:x} {mission:x} {flag:x} 0"))
    st = {}
    for _ in range(300):
        st = checked(pid, "CALLSTATUS")
        if st["state"] == 3: break
        time.sleep(0.01)
    emit(event="callstatus", status=st)
    if st["state"] != 3: emit(event="cancel", result=checked(pid, "CALLCANCEL"))
    checked(pid, "UNHOOK 0"); armed.remove(0)
    end = time.monotonic() + a.watch; last = None
    while time.monotonic() < end:
        c = ctx()
        if c != last: emit(event="ctx", **c); last = c
        time.sleep(0.5)
finally:
    try: command(pid, "CALLCANCEL")
    except Exception: pass
    for s in armed:
        try: checked(pid, f"UNHOOK {s}")
        except Exception: pass
    emit(event="disarmed"); out.close()
