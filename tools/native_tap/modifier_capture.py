"""Atomic per-apply capture of one modifier class for one pose component (game thread, DR entry+exit probe on the
apply, filtered on r9 = component). Snap ids: 1 modifier (0x80), 2/3 entry local/model, 4/5 exit local/model,
6..9 constraint target records, 30 entity scale. Replay with acu-rs `modifier_gate <dump> <meta>`.
Usage: modifier_capture.py <pid> <pose-comp-hex> <apply-rva-hex> <guard-hex> <tag> [seconds]
Applies (ACU.exe 748abdc6...7e40): MaxLookAt 494a0 48895c2408574883ec50488379480049,
PositionConstraint 4b490 405355574883ec70440fb75132804934, OrientationConstraint 49db0 48895c2408574883ec50440fb7513280,
Hinge 492a0 488bc44889580848897010574881ec30 (snap 1 is 0xc0 bytes, 6 = up to six 32-byte sphere entries),
Reflex 4b640 488bc4488958104c8948205556574154 (snap 1 is 0x40 bytes, 7 = base record, 8/9 = model/local pose
object headers, exit 10 = connector 0 cache records; static connector data goes to meta["reflex"])."""
import os, sys, time, json, struct, hashlib
os.environ.setdefault("ACU_TAP_STEM", "acu_tap10")
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__))); import tap_client as t, tap_events as E, reflex_static
ROOT = os.path.abspath(os.environ.get("ACU_TAP_OUT") or os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../local/native-probe/s2-maxlookat")).replace("\\", "/")
os.makedirs(ROOT, exist_ok=True)   # the game writes the dump, so the path must be absolute
pid, comp, rva, guard, tag = int(sys.argv[1]), int(sys.argv[2], 16), sys.argv[3], sys.argv[4], sys.argv[5]
secs = float(sys.argv[6]) if len(sys.argv) > 6 else 1.5
c = lambda s: t.command(pid, s, timeout=8)
pose = struct.unpack("<Q", t.read(pid, comp + 0x288, 8))[0]; skel = struct.unpack("<Q", t.read(pid, pose + 0x170, 8))[0]
hier_ptr, _cap, n = struct.unpack("<QHH", t.read(pid, skel + 0x68, 12)); hier = t.read(pid, hier_ptr, n * 12)
A = n * 32
c("RESET"); c("CLEARSNAPS"); assert c(f"DRFILTER 0 {comp:x} 3")["ok"]
for spec in (f"SNAP 0 entry 1 { {'492a0': 0xc0, '4b640': 0x40}.get(rva, 128) } a0", f"SNAP 0 entry 2 {A} a3 +280 * +178 *", f"SNAP 0 entry 3 {A} a3 +288 * +178 *",
             f"SNAP 0 exit 4 {A} a3 +280 * +178 *", f"SNAP 0 exit 5 {A} a3 +288 * +178 *"):
    assert c(spec)["ok"], spec
if rva in ("4b490", "49db0"):   # constraints: target records via the pointer array (+ entity scale for position)
    for k in range(4): assert c(f"SNAP 0 entry {6 + k} 16 a0 +28 * +{8 * k:x} *")["ok"]
    assert c("SNAP 0 entry 30 4 a3 +10 * +c0")["ok"]
if rva == "492a0":   # hinge: sphere entries (radius +8, bone ref +0x10, index +0x1c) and entity scale
    assert c("SNAP 0 entry 6 192 a0 +70 *")["ok"]
    assert c("SNAP 0 entry 30 4 a3 +10 * +c0")["ok"]
if rva == "4b640":   # reflex: base record, pose object headers (world transform, flags +0x8c), scale, exit cache records
    for spec in ("SNAP 0 entry 7 80 a0 +8 *", "SNAP 0 entry 8 144 a3 +288 *", "SNAP 0 entry 9 144 a3 +280 *",
                 "SNAP 0 entry 30 4 a3 +ca4", "SNAP 0 exit 10 256 a0 +34 * +10 * +28 *"):
        assert c(spec)["ok"], spec
assert c(f"DRHOOK 0 {rva} {guard} {tag} exit")["ok"]
time.sleep(secs); st = c("STATUS"); c("DRUNHOOK 0")
dump = f"{ROOT}/ev-{tag}-{pid}.bin"; d = c(f"DUMP {dump}")
meta = dict(pid=pid, component=hex(comp), pose_model_obj=hex(pose), skeleton=hex(skel), joints=n, hierarchy_hex=hier.hex(),
            apply_rva=rva, guard=guard, seconds=secs, status=st, dump=d, dump_sha256=hashlib.sha256(open(dump, "rb").read()).hexdigest())
if rva == "4b640":
    rd = lambda a, k: t.read(pid, a, k)
    objs = sorted({e["a"][0] for e in E.parse(dump) if e["phase"] == 0})
    meta["reflex"] = {hex(o): reflex_static.read_object(rd, o) for o in objs}
json.dump(meta, open(f"{ROOT}/ev-{tag}-{pid}.json", "w"), indent=1)
print(json.dumps({k: meta[k] for k in ("joints", "dump", "dump_sha256")}), st["hooks"])
