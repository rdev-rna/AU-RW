"""Does the cloth drag kernel run for a body, and with what time argument? ACU-RE-0081.

    python tools/native_tap/cloth_drag_probe.py <pid> <body hex> [seconds]

1. Hooks the step entry 0x1423d4b60 (filtered on a1 = body) for a moment to learn the body's
   solver (a0), then reads the solver's vtable and its slots 0-13 (the interpreter calls slot 8,
   +64, for opcode 9).
2. Hooks slot 8's target and the drag kernel 0x1423d72b0, both filtered on a0 = solver, and
   snapshots xmm1 (the float argument) and the wind vec3 (a2). Prints hit counts and values.
Read-only (hardware execution hooks and memory reads; nothing is written).
"""
import os
os.environ.setdefault("ACU_TAP_STEM", "acu_tap13")
import json, struct, sys, time
from pathlib import Path
import tap_client as t
import tap_events as E

STEP_RVA, DRAG_RVA = 0x23D4B60, 0x23D72B0
pid, body = int(sys.argv[1]), int(sys.argv[2], 16)
seconds = float(sys.argv[3]) if len(sys.argv) > 3 else 0.5
root = Path(__file__).resolve().parents[2] / "local/native-probe/cloth-live"
c = lambda s: t.command(pid, s, timeout=8)
base = int(c("STATUS")["image_base"], 16)
guard = lambda rva: t.read(pid, base + rva, 16).hex()


def run(name):
    out = root / f"drag-{name}-{pid}-{time.time_ns()}.bin"
    assert c(f"DUMP {out.as_posix()}")["ok"]
    return [e for e in E.parse(str(out)) if not e["phase"]]


# 1. The solver driving this body.
assert c("RESET")["ok"] and c("CLEARSNAPS")["ok"]
assert c(f"DRFILTER 0 {body:x} 1")["ok"]
assert c(f"DRHOOK 0 {STEP_RVA:x} {guard(STEP_RVA)} cloth_step")["ok"]
time.sleep(0.2)
c("DRUNHOOK 0")
hits = run("step")
solvers = sorted({e["a"][0] for e in hits})   # one solver per worker thread
vts = {struct.unpack("<Q", t.read(pid, s_, 8))[0] for s_ in solvers}
vts = {v for v in vts if base <= v < base + 0x8000000}   # read after the step: keep image vtables
assert len(vts) == 1, [hex(v) for v in vts]
solver = solvers[0]
vt = vts.pop()
slots = struct.unpack("<14Q", t.read(pid, vt, 14 * 8))
report = {"pid": pid, "body": hex(body), "solvers": [hex(x) for x in solvers], "vtable": hex(vt),
          "slots": [hex(s) for s in slots], "solver_head_hex": t.read(pid, solver, 0x60).hex()}

# 2. Slot 8's target and the drag kernel, filtered on the solver.
slot8 = slots[8]
assert c("RESET")["ok"] and c("CLEARSNAPS")["ok"]
targets = [(0, slot8 - base, "slot8")]
if slot8 - base != DRAG_RVA:
    targets.append((1, DRAG_RVA, "drag_kernel"))
for slot, rva, label in targets:
    assert c(f"SNAP {slot} entry 4 8 a0 +8")["ok"]       # solver +8: the body it is stepping
    assert c(f"SNAP {slot} entry 1 4 xmm1")["ok"]
    assert c(f"SNAP {slot} entry 2 16 a2")["ok"]
    assert c(f"SNAP {slot} entry 3 16 a0 +20")["ok"]   # solver +32: dt, wind (step entry stores them)
    assert c(f"DRHOOK {slot} {rva:x} {guard(rva)} {label}")["ok"]
time.sleep(seconds)
for slot, _, _ in targets:
    c(f"DRUNHOOK {slot}")
ev = run("kernel")
for slot, rva, label in targets:
    es_all = [e for e in ev if e["hook"] == slot]
    es = [e for e in es_all if e["snaps"].get(4) and struct.unpack("<Q", e["snaps"][4])[0] == body]
    report[label] = {
        "rva": hex(rva), "hits_all_bodies": len(es_all), "hits": len(es),
        "xmm1": sorted({round(struct.unpack("<f", e["snaps"][1])[0], 7) for e in es if e["snaps"].get(1)}),
        "a2_vec": sorted({tuple(round(x, 6) for x in struct.unpack("<4f", e["snaps"][2])) for e in es if e["snaps"].get(2)}),
        "solver32": sorted({tuple(round(x, 6) for x in struct.unpack("<4f", e["snaps"][3])) for e in es if e["snaps"].get(3)}),
    }
(root / f"drag-probe-{pid}.json").write_text(json.dumps(report, indent=1, default=str))
print(json.dumps(report, indent=1, default=str))
