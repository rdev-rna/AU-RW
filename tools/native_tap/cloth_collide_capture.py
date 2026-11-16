"""Cloth collision-op capture for one body, ACU-RE-0081.

    python tools/native_tap/cloth_collide_capture.py <pid> <body hex> <tag> [seconds] [--rcx HEX]

Two hardware execution hooks (tap DRHOOK, acu_tap13):
- slot 0: the step entry 0x1423d4b60(solver, body a1, dt xmm2, wind a3, shapes [rsp+0x28]),
  filtered on a1 = body, entry only. Snapshots as in cloth_capture.py (ids 1-11).
- slot 1: the collision path, entry and exit: the discrete 0x1423d68d0(obj a0, shapes a1,
  flags a2) by default (the path the flap runs live), or the swept 0x1423d58a0 with --swept.
  *obj is the body. The tap keeps one pending exit, so the hook is filtered on one per-thread
  collision object (rcx); without --rcx a short unfiltered pass picks the object that most
  often carries this body. Entry snapshots: 20 = *obj (8 bytes), 21 positions, 22 previous,
  23 = the shapes list {records, count}, 24 = up to 16 records; exit: 25 positions.
The step hook has no exit trap (the nested collision trap would replace it).
Read-only. Writes local/native-probe/cloth-live/collide-<tag>-<pid>-<ns>.{bin,json}.
"""
import os
os.environ.setdefault("ACU_TAP_STEM", "acu_tap13")
import hashlib, json, struct, sys, time
from collections import Counter
from pathlib import Path
import tap_client as t
import tap_events as E

STEP_RVA = 0x23D4B60
SWEPT_RVA = 0x23D58A0
DISCRETE_RVA = 0x23D68D0
BODY_SIZE = 736
MAX_RECORDS = 16

args = [a for a in sys.argv[1:]]
rcx = None
path_rva = DISCRETE_RVA
if "--swept" in args:
    args.remove("--swept")
    path_rva = SWEPT_RVA
if "--rcx" in args:
    i = args.index("--rcx")
    rcx = int(args[i + 1], 16)
    del args[i:i + 2]
pid, body, tag = int(args[0]), int(args[1], 16), args[2]
seconds = float(args[3]) if len(args) > 3 else 1.0
root = Path(__file__).resolve().parents[2] / "local/native-probe/cloth-live"
root.mkdir(parents=True, exist_ok=True)
c = lambda s: t.command(pid, s, timeout=8)
base = int(c("STATUS")["image_base"], 16)
vt, kind, count = struct.unpack("<QII", t.read(pid, body, 16))
assert vt == base + 0x35F4DE0 and kind == 1, (hex(vt), kind)
cap = (count + 15) // 16 * 16
arrays = 16 * cap
g_step = t.read(pid, base + STEP_RVA, 16).hex()
g_swept = t.read(pid, base + path_rva, 16).hex()
stamp = time.time_ns()

if rcx is None:
    assert c("RESET")["ok"] and c("CLEARSNAPS")["ok"]
    assert c("SNAP 1 entry 101 8 a0")["ok"]
    assert c(f"DRHOOK 1 {path_rva:x} {g_swept} cloth_collide_scan")["ok"]
    try:
        time.sleep(0.4)
    finally:
        c("DRUNHOOK 1")
    scan = root / f"collide-scan-{pid}-{stamp}.bin"
    assert c(f"DUMP {scan.as_posix()}")["ok"]
    objs = Counter()
    for e in E.parse(str(scan)):
        s = e["snaps"].get(101)
        if e["hook"] == 1 and s and struct.unpack("<Q", s)[0] == body:
            objs[e["a"][0]] += 1
    print("collision objects carrying the body:", {hex(k): v for k, v in objs.items()})
    if not objs:
        raise SystemExit("No collision-path calls for this body: unavailable")
    rcx = objs.most_common(1)[0][0]

assert c("RESET")["ok"] and c("CLEARSNAPS")["ok"]
assert c(f"DRFILTER 0 {body:x} 1")["ok"]
assert c(f"DRFILTER 1 {rcx:x} 0")["ok"]
for sid, length, expr in [(1, BODY_SIZE, "a1"), (2, arrays, "a1 +10 *"), (3, arrays, "a1 +20 *"),
                          (4, arrays, "a1 +30 *"), (5, 16 * count, "a1 +40 *"),
                          (6, 16 * count, "a1 +50 *"), (7, 4, "xmm2"), (8, 16, "a3"),
                          (9, 16, "rsp +28 *"), (10, 160 * MAX_RECORDS, "rsp +28 * *"),
                          (11, 160, "a1 +180 *")]:
    assert c(f"SNAP 0 entry {sid} {length} {expr}")["ok"], expr
for phase, sid, length, expr in [("entry", 20, 8, "a0"), ("entry", 21, arrays, "a0 * +10 *"),
                                 ("entry", 22, arrays, "a0 * +20 *"), ("entry", 23, 16, "a1"),
                                 ("entry", 24, 160 * MAX_RECORDS, "a1 *"),
                                 ("exit", 25, arrays, "a0 * +10 *")]:
    assert c(f"SNAP 1 {phase} {sid} {length} {expr}")["ok"], expr
assert c(f"DRHOOK 0 {STEP_RVA:x} {g_step} cloth_step")["ok"]
assert c(f"DRHOOK 1 {path_rva:x} {g_swept} cloth_collide exit")["ok"]
try:
    time.sleep(seconds)
finally:
    c("DRUNHOOK 1")
    c("DRUNHOOK 0")
dump = root / f"collide-{tag}-{pid}-{stamp}.bin"
assert c(f"DUMP {dump.as_posix()}")["ok"]
es = E.parse(str(dump))
steps = [e for e in es if e["hook"] == 0 and not e["phase"]]
coll = [e for e in es if e["hook"] == 1]
mine = [e for e in coll if e["phase"] or struct.unpack("<Q", e["snaps"][20])[0] == body]
meta = dict(pid=pid, body=hex(body), rcx=hex(rcx), particles=count, capacity=cap,
            image_base=hex(base), step_rva=hex(STEP_RVA), path_rva=hex(path_rva),
            guards=dict(step=g_step, path=g_swept), seconds=seconds, steps=len(steps),
            collision_entries=sum(1 for e in coll if not e["phase"]),
            collision_exits=sum(1 for e in coll if e["phase"]), dump=str(dump),
            dump_sha256=hashlib.sha256(dump.read_bytes()).hexdigest(), status=c("STATUS"))
dump.with_suffix(".json").write_text(json.dumps(meta, indent=1))
print(dump, meta["steps"], "steps", meta["collision_entries"], "collision entries",
      meta["collision_exits"], "exits")
if not steps or not meta["collision_exits"]:
    raise SystemExit("No calls captured: unavailable, not a passing capture")
