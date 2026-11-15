"""Which cloth bodies the solver steps, ACU-RE-0081: python cloth_census.py <pid> [seconds].

Hardware execution hook (tap DRHOOK, acu_tap13) on the step entry 0x1423d4b60(solver, body, dt,
wind, shapes) for a short window; each hit snapshots the body header (vtable, kind +8, particle
count +12) and dt (xmm2). Prints one row per distinct body: address, count, hits, dt values.
Read-only: no game memory is written.
"""
import os
os.environ.setdefault("ACU_TAP_STEM", "acu_tap13")
import struct, sys, time, json
from collections import defaultdict
from pathlib import Path
import tap_client as t
import tap_events as E

STEP_RVA = 0x23D4B60
pid = int(sys.argv[1])
seconds = float(sys.argv[2]) if len(sys.argv) > 2 else 0.5
c = lambda s: t.command(pid, s, timeout=8)
base = int(c("STATUS")["image_base"], 16)
guard = t.read(pid, base + STEP_RVA, 16).hex()
assert c("RESET")["ok"] and c("CLEARSNAPS")["ok"]
assert c("SNAP 0 entry 1 16 a1")["ok"]
assert c("SNAP 0 entry 2 4 xmm2")["ok"]
assert c(f"DRHOOK 0 {STEP_RVA:x} {guard} cloth_step")["ok"]
try:
    time.sleep(seconds)
finally:
    c("DRUNHOOK 0")
out = Path(__file__).resolve().parents[2] / f"local/native-probe/cloth-census-{pid}.bin"
assert c(f"DUMP {out.as_posix()}")["ok"]
rows = defaultdict(lambda: {"hits": 0, "dt": set()})
for e in E.parse(str(out)):
    if e["phase"] or e["hook"] != 0:
        continue
    vt, kind, count = struct.unpack("<QII", e["snaps"][1])
    r = rows[e["a"][1]]
    r["hits"] += 1
    r["count"], r["kind"], r["vtable"] = count, kind, vt
    r["dt"].add(round(struct.unpack("<f", e["snaps"][2])[0], 6))
res = [{"body": hex(b), "vtable": hex(r["vtable"]), "kind": r["kind"], "count": r["count"],
        "hits": r["hits"], "dt": sorted(r["dt"])} for b, r in sorted(rows.items())]
print(json.dumps({"guard": guard, "seconds": seconds, "bodies": res}, indent=1))
