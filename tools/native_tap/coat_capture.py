"""Atomic coat subsolver entry/exit capture, ACU-RE-0028.

Requires acu_tap13 (xmm snapshots). Supports at most two nodes, verified before
arming. No collision/wind support is claimed by the replay decoder.
"""
import os
os.environ.setdefault("ACU_TAP_STEM", "acu_tap13")
import json, struct, sys, time, hashlib
from pathlib import Path
import tap_client as t
import tap_events as E

pid, comp, tag = int(sys.argv[1]), int(sys.argv[2], 16), sys.argv[3]
seconds = float(sys.argv[4]) if len(sys.argv) > 4 else 1
root = Path(__file__).resolve().parents[2] / "local/native-probe/s2-maxlookat"
c = lambda s: t.command(pid, s, timeout=8)
base = int(c("STATUS")["image_base"], 16)
pose = struct.unpack("<Q", t.read(pid, comp + 0x288, 8))[0]
skel = struct.unpack("<Q", t.read(pid, pose + 0x170, 8))[0]
hp, _, n = struct.unpack("<QHH", t.read(pid, skel + 0x68, 12))
hier = t.read(pid, hp, n * 12)
known = json.loads((root / f"coat-inspect-{pid}.json").read_text())
assert all(len(o['members']) <= 2 for o in known['objects'])
assert c("RESET")["ok"]
assert c("CLEARSNAPS")["ok"]
assert c(f"DRFILTER 0 {comp:x} 2")["ok"]
for phase in ("entry", "exit"):
    for sid, length, expr in [(1,112,"a0"),(2,336,"a0 +8 * *"),(3,336,"a0 +8 * +8 *")]:
        assert c(f"SNAP 0 {phase} {sid} {length} {expr}")["ok"]
for sid, length, expr in [(10,4,"xmm1"),(11,64,"a2"),(12,144,"a2 +288 *"),
                          (13,n*32,"a2 +288 * +178 *"),(14,1,"a2 +cd0"),
                          (15,16,"a0 +8 * * +50 *"),(16,16,"a0 +8 * * +50 * * +10")]:
    assert c(f"SNAP 0 entry {sid} {length} {expr}")["ok"]
guard = "488bc44c894018488948085541564157"
stages = "--stages" in sys.argv
if stages:
    assert c("SNAP 1 entry 20 32 rsp +510 *")["ok"]
    mid_guard=t.read(pid,base+0x46910,16).hex()
    assert c(f"DRHOOK 1 46910 {mid_guard} constraint_pass")["ok"]
assert t.read(pid,base+0x45510,16).hex() == guard
assert c(f"DRHOOK 0 45510 {guard} coat_solver exit")["ok"]
try:
    time.sleep(seconds)
finally:
    c("DRUNHOOK 0")
    if stages: c("DRUNHOOK 1")
dump = root / f"coat-{tag}-{pid}.bin"
assert c(f"DUMP {dump.as_posix()}")["ok"]
meta = dict(pid=pid, component=hex(comp), joints=n, hierarchy_hex=hier.hex(),
            dump=str(dump), dump_sha256=hashlib.sha256(dump.read_bytes()).hexdigest(),
            guard=guard, image_base=hex(base), status=c("STATUS"))
(root / f"coat-{tag}-{pid}.json").write_text(json.dumps(meta, indent=1))
es = E.parse(str(dump)); entries=[e for e in es if not e['phase'] and e['hook']==0]
print(dump, len(entries), 'entries',len({e['a'][0] for e in entries}),'receivers',
      'dt', sorted({struct.unpack('<f',e['snaps'][10])[0] for e in entries})[:8])
if not entries: raise SystemExit('No calls captured: unavailable, not a passing capture')
