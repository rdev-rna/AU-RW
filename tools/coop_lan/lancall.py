"""lancall.py <pid> <out.jsonl> [--watch 40]
Calls the LAN matchmaker's slot 4 (0x1425406D0, "start search", ACU-RE-0090) once on the game thread with the
probe's CALLAT, piggy-backing on Storm interface slot 12 (0x141C626C0, per-frame update). Checks first that
*(qword_145137930+1792) has the LAN matchmaker vtable 0x14360D7E0. Then watches the discovery flag (interface
+692) and hooks on the DiscoverSession poster/constructor. Always cancels and unhooks."""
import argparse, json, struct, sys, time
sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parents[1] / "native_probe"))
from client import checked, command  # noqa
ap = argparse.ArgumentParser(); ap.add_argument("pid", type=int); ap.add_argument("out"); ap.add_argument("--watch", type=float, default=40)
a = ap.parse_args(); pid = a.pid
out = open(a.out, "a", encoding="utf-8"); t0 = time.monotonic()
def emit(**r):
    r.setdefault("t_s", round(time.monotonic() - t0, 3)); r.setdefault("unix", round(time.time(), 1)); out.write(json.dumps(r) + "\n"); out.flush(); print(json.dumps(r)[:300], flush=True)
def rd(addr, n): return bytes.fromhex(checked(pid, f"READ {addr:x} {n}")["hex"])
def q(addr): return struct.unpack("<Q", rd(addr, 8))[0]
def iface_state():
    i = q(0x145212ED0) + 8; d = rd(i + 672, 40)
    return {"iface": hex(i), "+688": struct.unpack_from("<i", d, 16)[0], "+692": d[20], "+696": struct.unpack_from("<Q", d, 24)[0], "+704": struct.unpack_from("<Q", d, 32)[0]}
mm = q(0x145137930); obj = q(mm + 1792); vt = q(obj) if obj else 0
emit(event="target", mm=hex(mm), lan_mm=hex(obj), vtable=hex(vt), iface=iface_state())
if vt != 0x14360D7E0:
    emit(event="abort", reason="matchmaker at +1792 is not the LAN one"); sys.exit(1)
HOOKS = [("storm_slot12_update", 0x1C626C0, "40534883ec2083b9b002000002488bd9"),
         ("post_discover_session", 0x1B35E10, "48895c24104889742418574883ec2083"),
         ("discover_session_ctor", 0x1B6E5E0, "4c8bdc49895b0849896b104989731849"),
         ("lan_mm_start_search", 0x25406D0, "48895c2408574883ec40488bf9488d15")]
armed = []
try:
    for slot, (label, rva, exp) in enumerate(HOOKS):
        checked(pid, f"HOOK {slot} {rva:x} {exp} {label} 1 0 0 0 lancall"); armed.append(slot); emit(event="armed", label=label)
    checked(pid, "EVENTS 256")
    emit(event="callat", result=checked(pid, f"CALLAT 0 1425406d0 {obj:x} 0 0 0"))
    for _ in range(300):
        st = checked(pid, "CALLSTATUS")
        if st["state"] == 3: break
        time.sleep(0.01)
    emit(event="callstatus", status=st)
    if st["state"] != 3:
        emit(event="cancel", result=checked(pid, "CALLCANCEL"))
    checked(pid, "UNHOOK 0"); armed.remove(0)
    end = time.monotonic() + a.watch; last = None
    while time.monotonic() < end:
        s = iface_state()
        if s != last: emit(event="iface", **s); last = s
        hits = {h["label"]: h["hits"] for h in checked(pid, "STATUS")["hooks"] if h.get("label")}
        evs = checked(pid, "EVENTS 64").get("events", [])
        for e in evs[:8]:
            emit(event="hit", label=e.get("label"), tid=e.get("tid"), ret=e.get("return_address"))
        emit(event="tick", hits=hits)
        time.sleep(1)
finally:
    try: command(pid, "CALLCANCEL")
    except Exception: pass
    for s in armed:
        try: checked(pid, f"UNHOOK {s}")
        except Exception: pass
    emit(event="disarmed"); out.close()
