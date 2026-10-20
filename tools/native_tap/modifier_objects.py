"""List one modifier class's live objects for a pose component (DR probe filtered on r9, header only) and dump their bytes."""
import os, sys, time, struct, json
os.environ.setdefault("ACU_TAP_STEM", "acu_tap10")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__))); import tap_client as t, tap_events as E
pid, comp, rva, guard, tag = int(sys.argv[1]), int(sys.argv[2], 16), sys.argv[3], sys.argv[4], sys.argv[5]
c = lambda s: t.command(pid, s, timeout=8)
OUT = os.environ.get("ACU_TAP_OUT") or os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../local/native-probe/s2-maxlookat")).replace("\\", "/")
os.makedirs(OUT, exist_ok=True)
out = f"{OUT}/disc-{tag}-{int(time.time())}.bin"
c("RESET"); c("CLEARSNAPS"); assert c(f"DRFILTER 0 {comp:x} 3")["ok"]; assert c(f"DRHOOK 0 {rva} {guard} {tag}")["ok"]
time.sleep(0.3); c("DRUNHOOK 0"); c(f"DUMP {out}")
order = []
for e in E.parse(out):
    if e["a"][0] not in order: order.append(e["a"][0])
rows = []
for m in order:
    b = t.read(pid, m, int(os.environ.get("OBJ_LEN", "128"), 0)); rows.append(dict(mod=hex(m), hex=b.hex()))
json.dump(rows, open(f"{OUT}/objs-{tag}-{pid}.json", "w"), indent=1)
print(len(order), "objects")
for r in rows: print(r["mod"], r["hex"])
