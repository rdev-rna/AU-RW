"""Which event arms the cloth damping boost, ACU-RE-0081.

    python tools/native_tap/cloth_boost_probe.py <pid> <body hex> <seconds> <site hex>[,<site hex>...]

A site that is a function entry (141d764a0, 141d950c0, 141d976e0) also snapshots the body its
call acts on: a0 +8 -> component, +152 -> cloth object, +160 -> body (the chain the frame
update 0x141d8b9a0 and 0x1423aa6d0 use). Entry hooks fire on every call, not only when a
flag is raised.

Hardware execution hooks (tap DRHOOK, acu_tap13, up to three) on the instructions that raise
the cloth component's reposition flags. The component's frame update 0x141d8b9a0 arms the
boost when it sees any of them:
- 141d76b99: +572 = 1 in 0x141d764a0 (instance swap, no inherited state)
- 141d76bb5: +573 = 1 in 0x141d764a0 (instance swap, inherited state)
- 141d95115: +574 = 1 in 0x141d950c0 (entity flags +696/+697: reposition every cloth)
- 141d97771: +572 = 1 in 0x141d976e0 (reset with the third argument set)
The body's damping (+176) is polled meanwhile. Times use QPC, as the hooks do. Read-only.
Prints one JSON report: hits per site with times, and the damping changes.
"""
import os
os.environ.setdefault("ACU_TAP_STEM", "acu_tap13")
import json, struct, sys, threading, time
from pathlib import Path
import tap_client as t
import tap_events as E

pid, body, seconds = int(sys.argv[1]), int(sys.argv[2], 16), float(sys.argv[3])
sites = [int(x, 16) for x in sys.argv[4].split(",")][:3]
c = lambda s: t.command(pid, s, timeout=8)
base = int(c("STATUS")["image_base"], 16)
assert c("RESET")["ok"] and c("CLEARSNAPS")["ok"]
for slot, va in enumerate(sites):
    rva = va - 0x140000000
    guard = t.read(pid, base + rva, 8).hex()
    if va in (0x141D764A0, 0x141D950C0, 0x141D976E0):
        assert c(f"SNAP {slot} entry 1 8 a0 +8 * +98 * +a0")["ok"]
        if va == 0x141D950C0:
            # The entity's flags +696/+697: *(*(*(a1+24)+16)+16) + 696.
            assert c(f"SNAP {slot} entry 2 2 a0 +18 * +10 * +10 * +2b8")["ok"]
    assert c(f"DRHOOK {slot} {rva:x} {guard} boost_{va:x}")["ok"], hex(va)

samples, stop = [], threading.Event()


def poll():
    last = None
    while not stop.is_set():
        d = struct.unpack("<f", t.read(pid, body + 176, 4))[0]
        if d != last:
            samples.append((time.perf_counter_ns() // 100, d))
            last = d
        time.sleep(0.004)


th = threading.Thread(target=poll)
th.start()
try:
    time.sleep(seconds)
finally:
    stop.set()
    th.join()
    for slot in range(len(sites)):
        c(f"DRUNHOOK {slot}")
out = Path(__file__).resolve().parents[2] / f"local/native-probe/cloth-live/boost-probe-{pid}-{time.time_ns()}.bin"
assert c(f"DUMP {out.as_posix()}")["ok"]
hits = {f"{va:x}": [] for va in sites}
mine = {f"{va:x}": [] for va in sites}
for e in E.parse(str(out)):
    if e["phase"] == 0 and e["hook"] < len(sites):
        k = f"{sites[e['hook']]:x}"
        hits[k].append(e["qpc"])
        b = e["snaps"].get(1)
        if b and struct.unpack("<Q", b)[0] == body:
            f = e["snaps"].get(2)
            if k != "141d950c0" or (f and any(f)):
                mine[k].append((e["qpc"], f.hex() if f else None))
t0 = min([q for v in hits.values() for q in v] + [s[0] for s in samples] or [0])
print(json.dumps({
    "dump": str(out), "body": hex(body),
    "hits": {k: len(v) for k, v in hits.items()},
    "this_body": {k: [(round((q - t0) / 1e7, 3), a) for q, a in v] for k, v in mine.items()},
    "damping_changes": [(round((q - t0) / 1e7, 3), round(d, 5)) for q, d in samples],
}, indent=1))
