"""Find the player's pose component by Arno-only graph leaves (PLAN_MOTION_PARITY methodology 2)."""
import os, sys, time, collections, struct
os.environ["ACU_TAP_STEM"] = sys.argv[2] if len(sys.argv) > 2 else "acu_tap10"
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__))); import tap_client as t, tap_events as E
pid = int(sys.argv[1]); c = lambda s: t.command(pid, s, timeout=8)
OUT = os.environ.get("ACU_TAP_OUT") or os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../local/native-probe/s2-maxlookat")).replace("\\", "/")
os.makedirs(OUT, exist_ok=True)
out = f"{OUT}/findplayer-{int(time.time())}.bin"
c("RESET"); c("CLEARSNAPS")
assert c("SNAP 0 entry 1 8 a0 +d8")["ok"]; assert c("SNAPGRAPH 0 entry 2 a0 +d0 *")["ok"]
print(c("DRHOOK 0 e9cc0 40534883ec30488bd9488b89d0000000 comp_update"))
time.sleep(0.25); c("DRUNHOOK 0"); print(c(f"DUMP {out}"))
ev = E.parse(out); seen = collections.defaultdict(set)
for e in ev:
    g = e["snaps"].get(2); p = e["snaps"].get(1)
    if not g or not p: continue
    pose = struct.unpack("<Q", p)[0]
    for l in E.parse_graph(g)["leaves"]:
        if l["file_id"]: seen[(e["a"][0], pose)].add(l["file_id"])
ARNO = {"000000016737c2c4", "00000003836e1d49"}
print(len(ev), "events", len(seen), "components with leaves")
for (comp, pose), ids in seen.items():
    if ids & ARNO: print("ARNO? comp", hex(comp), "pose", hex(pose), sorted(ids))
