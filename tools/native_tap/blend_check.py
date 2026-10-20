"""Compare decoded-clip blend (hemisphere-aligned weighted sum of bound, mode-transformed leaf rotations)
against atomic exit snapshots, per pose slot. Reads binding tables from the live game through the tap.
Usage: python tools/native_tap/blend_check.py <pid> <events.bin> <clipdir>[,<clipdir>...] <out.json>"""
import collections, json, math, os, struct, sys
from pathlib import Path
os.environ.setdefault("ACU_TAP_STEM", "acu_tap7")
sys.path.insert(0, str(Path(__file__).parent))
import numpy as np, tap_client as t, tap_events as ev_mod
ROOT = Path(__file__).resolve().parents[2]; ML = ROOT / "local/native-probe/motion-link-20261020"

def unit(q): return q / np.linalg.norm(q, axis=-1, keepdims=True)
def sample(tm, tr):
    ts = tr["times"]; v = tr["values"]; j = int(np.searchsorted(ts, tm, side="right")); i = max(0, min(j - 1, len(ts) - 1)); j = min(j, len(ts) - 1)
    x = unit(np.array(v[i], float)); y = unit(np.array(v[j], float))
    if i == j: return x
    u = (tm - ts[i]) / (ts[j] - ts[i]); d = float(x @ y)
    if d < 0: y = -y; d = -d
    if d > .9995: return unit(x + (y - x) * u)
    a = math.acos(min(1, d)); return (x * math.sin((1 - u) * a) + y * math.sin(u * a)) / math.sin(a)
def transform(q, mode):
    x, y, z, w = q
    return unit(np.array({0: (x, y, z, w), 1: (x, -y, -z, w), 2: (y, x, -w, -z), 3: (-x, y, -z, w), 4: (-w, z, y, -x), 5: (-x, -y, z, w), 6: (-z, w, -x, y), 7: (x, y, z, w)}[mode], float))
def ang(a, b): return math.degrees(2 * math.acos(min(1, abs(float(a @ b)))))

def slerp(a, b, u):
    if a @ b < 0: b = -b
    d = float(a @ b)
    if d > .9995: return unit(a + (b - a) * u)
    th = math.acos(d); return (a * math.sin((1 - u) * th) + b * math.sin(u * th)) / math.sin(th)
def seq_slerp(qs, ws):
    acc = qs[0]; tot = ws[0]
    for q, w in zip(qs[1:], ws[1:]):
        tot += w; acc = slerp(acc, q, w / tot)
    return acc

def read_binding(pid, playback_raw, names, primary):
    metadata, binding = struct.unpack_from("<2Q", playback_raw, 48)
    c = lambda va, n: t.read(pid, va, n)
    bh = c(binding, 96); arr = struct.unpack_from("<Q", bh)[0]; cnt = struct.unpack_from("<H", bh, 10)[0]
    if not 0 < cnt <= 4096: raise ValueError("binding count")
    tab = c(arr, cnt * 4); tmax = max(i for i, j in struct.iter_unpack("<HH", tab))
    mr = c(metadata, 192); ch = struct.unpack_from("<Q", mr, 8)[0]; craw = c(ch, (tmax + 1) * 8)
    out = []
    for target, packed in struct.iter_unpack("<HH", tab):
        h, off, kind = struct.unpack_from("<IHH", craw, target * 8)
        if h not in primary or off % 32 != 16 or (kind & 15) != 6: continue
        idx = off // 32
        if names.get(idx) != h: continue
        out.append(dict(hash=h, native_index=idx, source_track=packed & 8191, mode=packed >> 13))
    return out

def main():
    pid = int(sys.argv[1]); events = sys.argv[2]; clipdirs = [Path(p) if os.path.isabs(p) else ROOT / p for p in sys.argv[3].split(",")]; outp = Path(sys.argv[4])
    names = {i: h for h, i, _ in struct.iter_unpack("<IHH", (ML / "run-62/joint-names-459.bin").read_bytes())}
    mapping = json.loads((ML / "run-62/rig-mapping-459.json").read_text()); primary = {h for i, h in names.items() if str(i) in mapping["native_to_rust"]}
    clips = {}
    for d in clipdirs:
        for p in d.glob("????????????????.json"):
            raw = json.loads(p.read_text())
            if "animation" in raw: clips.setdefault(raw["resource"]["file_id"], raw["animation"])
    ev = ev_mod.parse(events); q0 = ev[0]["qpc"]; cache = {}; rows = []; skipped = collections.Counter()
    exits = [x for x in ev if x["phase"] == 1]
    for x in exits[::3]:
        g = ev_mod.parse_graph(x["snaps"][3]); leaves = [l for l in g["leaves"] if l["ok"] and l["weight"] > 1e-6]
        if not leaves: skipped["no leaves"] += 1; continue
        if any(l["file_id"] not in clips for l in leaves): skipped["undecoded clip"] += 1; continue
        if abs(sum(l["weight"] for l in leaves) - 1) > 1e-3: skipped["weights"] += 1; continue
        per = []
        try:
            for l in leaves:
                key = (l["file_id"], l["binding_ptr"])
                if key not in cache: cache[key] = read_binding(pid, l["playback_raw"], names, primary)
                c = clips[l["file_id"]]; tm = (l["time900"] + l["offset876"]) % c["duration"]; m = {}
                for b in cache[key]:
                    if b["source_track"] < len(c["tracks"]) and c["tracks"][b["source_track"]]["codec"] <= 8:
                        m[b["native_index"]] = transform(sample(tm, c["tracks"][b["source_track"]]), b["mode"])
                per.append(m)
        except Exception as e:
            skipped["binding read: " + str(e)[:40]] += 1; continue
        common = sorted(set.intersection(*(set(m) for m in per)))
        if len(common) < 10: skipped["few common"] += 1; continue
        ws = np.array([l["weight"] for l in leaves]); pred = {}; pred2 = {}
        for i in common:
            qs = [m[i] for m in per]; r = qs[0]; acc = sum(w * (q if q @ r >= 0 else -q) for q, w in zip(qs, ws)); pred[i] = unit(acc); pred2[i] = seq_slerp(qs, list(ws))
        for k in range(6):
            b = x["snaps"].get(20 + k); h = x["snaps"].get(10 + k)
            if b is None or h is None: continue
            arr = np.frombuffer(b[:459 * 32], dtype="<f4").reshape(459, 8); fl = struct.unpack_from("<I", h, 0x8c)[0]
            nat = {i: unit(arr[i, 4:].astype(float)) for i in common}; errs = [ang(pred[i], nat[i]) for i in common]; errs2 = [ang(pred2[i], nat[i]) for i in common]
            rows.append(dict(slerp_mean=float(np.mean(errs2)), slerp_median=float(np.median(errs2)), slerp_max=float(np.max(errs2)), t=(x["qpc"] - q0) / 1e7, slot=k, flags=hex(fl), leaves=len(leaves), common=len(common), mean=float(np.mean(errs)), median=float(np.median(errs)), p90=float(np.quantile(errs, .9)), max=float(np.max(errs)),
                             worst=[(int(i), round(e, 2)) for e, i in sorted(zip(errs, common), reverse=True)[:4]]))
    summ = collections.defaultdict(list)
    for r in rows: summ[(r["slot"], r["flags"])].append(r)
    report = dict(events=events, rows=len(rows), skipped=dict(skipped), groups={})
    for (s, f), rs in sorted(summ.items()):
        med = [r["median"] for r in rs]; mean = [r["mean"] for r in rs]
        report["groups"][f"slot{s}:{f}"] = dict(n=len(rs), median_of_median_deg=float(np.median(med)), median_of_mean_deg=float(np.median(mean)), frac_median_below_0p05=float(np.mean(np.array(med) < 0.05)), common=sorted({r["common"] for r in rs}))
    outp.write_text(json.dumps(dict(report=report, rows=rows), indent=1)); print(json.dumps(report, indent=1))
if __name__ == "__main__": main()
