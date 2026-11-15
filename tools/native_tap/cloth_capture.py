"""Cloth step entry/exit capture for one body, ACU-RE-0081.

    python tools/native_tap/cloth_capture.py <pid> <body hex> <tag> [seconds]

Hardware execution hook (tap DRHOOK with exit trap, acu_tap13) on the step entry
0x1423d4b60(solver a0, body a1, dt xmm2, wind a3, shapes [rsp+0x28]), filtered on a1 = body
(find the body with cloth_census.py). Entry snapshots: the 736-byte body, the particle arrays
it points to (+0x10 positions, +0x20 previous, +0x30, +0x40 targets, +0x50 target normals),
dt, the wind vec3, the shapes list {records, count} and up to 16 of its 160-byte records.
Exit snapshots: positions, previous and +0x30. Read-only; writes
local/native-probe/cloth-live/cloth-<tag>-<pid>.{bin,json}.
"""
import os
os.environ.setdefault("ACU_TAP_STEM", "acu_tap13")
import hashlib, json, struct, sys, time
from pathlib import Path
import tap_client as t
import tap_events as E

STEP_RVA = 0x23D4B60
BODY_SIZE = 736
MAX_RECORDS = 16

pid, body, tag = int(sys.argv[1]), int(sys.argv[2], 16), sys.argv[3]
seconds = float(sys.argv[4]) if len(sys.argv) > 4 else 1.0
root = Path(__file__).resolve().parents[2] / "local/native-probe/cloth-live"
root.mkdir(parents=True, exist_ok=True)
c = lambda s: t.command(pid, s, timeout=8)
base = int(c("STATUS")["image_base"], 16)
vt, kind, count = struct.unpack("<QII", t.read(pid, body, 16))
assert vt == base + 0x35F4DE0 and kind == 1, (hex(vt), kind)
cap = (count + 15) // 16 * 16
arrays = 16 * cap
guard = t.read(pid, base + STEP_RVA, 16).hex()
assert c("RESET")["ok"] and c("CLEARSNAPS")["ok"]
assert c(f"DRFILTER 0 {body:x} 1")["ok"]
entry = [(1, BODY_SIZE, "a1"), (2, arrays, "a1 +10 *"), (3, arrays, "a1 +20 *"),
         (4, arrays, "a1 +30 *"), (5, 16 * count, "a1 +40 *"), (6, 16 * count, "a1 +50 *"),
         (7, 4, "xmm2"), (8, 16, "a3"), (9, 16, "rsp +28 *"),
         (10, 160 * MAX_RECORDS, "rsp +28 * *"), (11, 160, "a1 +180 *")]
for sid, length, expr in entry:
    assert c(f"SNAP 0 entry {sid} {length} {expr}")["ok"], expr
for sid, length, expr in [(12, arrays, "a1 +10 *"), (13, arrays, "a1 +20 *"), (14, arrays, "a1 +30 *")]:
    assert c(f"SNAP 0 exit {sid} {length} {expr}")["ok"], expr
assert c(f"DRHOOK 0 {STEP_RVA:x} {guard} cloth_step exit")["ok"]
try:
    time.sleep(seconds)
finally:
    c("DRUNHOOK 0")
dump = root / f"cloth-{tag}-{pid}.bin"
assert c(f"DUMP {dump.as_posix()}")["ok"]
es = E.parse(str(dump))
entries = [e for e in es if e["hook"] == 0 and not e["phase"]]
exits = [e for e in es if e["hook"] == 0 and e["phase"]]
meta = dict(pid=pid, body=hex(body), particles=count, capacity=cap, guard=guard,
            image_base=hex(base), step_rva=hex(STEP_RVA), seconds=seconds,
            entries=len(entries), exits=len(exits), dump=str(dump),
            dump_sha256=hashlib.sha256(dump.read_bytes()).hexdigest(), status=c("STATUS"))
(root / f"cloth-{tag}-{pid}.json").write_text(json.dumps(meta, indent=1))
print(dump, len(entries), "entries", len(exits), "exits",
      "dt", sorted({round(struct.unpack('<f', e['snaps'][7])[0], 6) for e in entries}))
if not entries or not exits:
    raise SystemExit("No calls captured: unavailable, not a passing capture")
