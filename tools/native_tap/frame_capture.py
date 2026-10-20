"""Whole-frame modifier capture at the evaluator 0x1400377e0 (ACU-RE-0028 "Evaluation order") for one pose
component: DR entry+exit probe filtered on rcx = component. Snap ids: 2/3 entry local/model, 4/5 exit local/model,
8/9 model/local pose object headers, 11 skip-list header (r9) and 12 its u16 entries, 13 u16 [comp+232],
20 SNAPMODS blob (list objects, constraint targets, Hinge spheres, Reflex base records, run channel bytes),
30 entity scale, 31 Reflex scale. Static Reflex connector data goes to meta["reflex"].
Replay with acu-rs `modifier_frame <dump> <meta>`. Needs a tap DLL with SNAPMODS and a 512-entry cap (acu_tap12+).
Usage: frame_capture.py <pid> <pose-comp-hex> <tag> [seconds]"""
import os, sys, time, json, struct, hashlib
os.environ.setdefault("ACU_TAP_STEM", "acu_tap12")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__))); import tap_client as t, tap_events as E, reflex_static
ROOT = os.path.abspath(os.environ.get("ACU_TAP_OUT") or os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../local/native-probe/s2-maxlookat")).replace("\\", "/")
os.makedirs(ROOT, exist_ok=True)   # the game writes the dump, so the path must be absolute
RVA = 0x377e0
REFLEX_VTABLE = 0x142e768c0
MODS_HDR, MODS_CHAN, MODS_ENTRY = 32, 512, 544
pid, comp, tag = int(sys.argv[1]), int(sys.argv[2], 16), sys.argv[3]
secs = float(sys.argv[4]) if len(sys.argv) > 4 else 1.0
c = lambda s: t.command(pid, s, timeout=8)
st0 = c("STATUS"); base = int(st0["image_base"], 16)
guard = t.read(pid, base + RVA, 16).hex()   # .text is not plain on disk; the live prologue is recorded instead
pose = struct.unpack("<Q", t.read(pid, comp + 0x288, 8))[0]; skel = struct.unpack("<Q", t.read(pid, pose + 0x170, 8))[0]
hier_ptr, _cap, n = struct.unpack("<QHH", t.read(pid, skel + 0x68, 12)); hier = t.read(pid, hier_ptr, n * 12)
A = n * 32
c("RESET"); c("CLEARSNAPS"); assert c(f"DRFILTER 0 {comp:x} 0")["ok"]
for spec in (f"SNAP 0 entry 2 {A} a0 +280 * +178 *", f"SNAP 0 entry 3 {A} a0 +288 * +178 *",
             f"SNAP 0 exit 4 {A} a0 +280 * +178 *", f"SNAP 0 exit 5 {A} a0 +288 * +178 *",
             "SNAP 0 entry 8 144 a0 +288 *", "SNAP 0 entry 9 144 a0 +280 *",
             "SNAP 0 entry 11 16 a3", "SNAP 0 entry 12 512 a3 *", "SNAP 0 entry 13 2 a0 +e8",
             "SNAPMODS 0 entry 20 a0", "SNAP 0 entry 30 4 a0 +10 * +c0", "SNAP 0 entry 31 4 a0 +ca4"):
    assert c(spec)["ok"], spec
assert c(f"DRHOOK 0 {RVA:x} {guard} frame_eval exit")["ok"]
time.sleep(secs); st = c("STATUS"); c("DRUNHOOK 0")
dump = f"{ROOT}/frame-{tag}-{pid}.bin"; d = c(f"DUMP {dump}")
rd = lambda a, k: t.read(pid, a, k)
objs = set()
for e in E.parse(dump):
    blob = e["snaps"].get(20)
    if e["phase"] or not blob: continue
    for i in range(struct.unpack_from("<I", blob, 12)[0]):
        o = MODS_HDR + MODS_CHAN + i * MODS_ENTRY; obj, = struct.unpack_from("<Q", blob, o)
        if struct.unpack_from("<Q", blob, o + 16)[0] == REFLEX_VTABLE: objs.add(obj)
meta = dict(pid=pid, component=hex(comp), pose_model_obj=hex(pose), skeleton=hex(skel), joints=n, hierarchy_hex=hier.hex(),
            evaluator_rva=hex(RVA), live_guard=guard, seconds=secs, status=st, dump=d,
            dump_sha256=hashlib.sha256(open(dump, "rb").read()).hexdigest(),
            reflex={hex(o): reflex_static.read_object(rd, o) for o in sorted(objs)})
json.dump(meta, open(f"{ROOT}/frame-{tag}-{pid}.json", "w"), indent=1)
print(json.dumps({k: meta[k] for k in ("joints", "live_guard", "dump", "dump_sha256")}), st["hooks"], len(objs), "reflex objects")
