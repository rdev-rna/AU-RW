"""Per-frame native root delta against blend models built from the native leaves (ACU-RE-0074).

    python tools/root_delta_blend_compare.py CAPTURE.jsonl ROOT_TRACKS.json

CAPTURE.jsonl: tools/native_probe/root_delta_capture.py; ROOT_TRACKS.json: acu-root-tracks for
every clip above 0.01 weight in the frames to compare.

Uses tools/root_delta_compare.py's track model (linear keys, A->B segment, loop wrap, mirror x).
For each frame between two setter events, the leaf state just before (A) and just after (B) the
update is matched by playback pointer. Models:
  weighted  = sum_i w_i(B) * d_i            (Rust: GraphDriver::step)
  normalized= weighted / sum_i w_i(B) over leaves with tracks
  heaviest  = d of the leaf with the largest w(B)
Frames are kept when every leaf above 0.01 weight has a track and appears in both states.
"""
import bisect
import json
import math
import statistics
import struct
import sys

sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parent))
from root_delta_compare import SETTER_RIP, predict  # noqa: E402

cap, tracks = sys.argv[1], sys.argv[2]
clips = {c["file_id"]: c for c in json.load(open(tracks))["clips"]}
recs = [json.loads(l) for l in open(cap)]
events = sorted((e for r in recs if r["kind"] == "events" for e in r["events"]
                 if int(e["rip"], 16) == SETTER_RIP), key=lambda e: e["qpc"])
seen, ev = set(), []
for e in events:
    if e["sequence"] not in seen:
        seen.add(e["sequence"])
        ev.append(e)
samples = sorted((r for r in recs if r["kind"] == "leaves"), key=lambda r: r["qpc0"])
sq = [s["qpc0"] for s in samples]


def state_between(q0, q1):
    i, j = bisect.bisect_right(sq, q0), bisect.bisect_left(sq, q1)
    return [s for s in samples[i:j] if s["qpc1"] < q1]


rows = []
for k in range(1, len(ev) - 1):
    before = state_between(ev[k - 1]["qpc"], ev[k]["qpc"])
    after = state_between(ev[k]["qpc"], ev[k + 1]["qpc"])
    if not before or not after:
        continue
    la = {x["playback"]: x for x in before[-1]["leaves"]}
    lb = [x for x in after[0]["leaves"] if x["weight"] > 0.01]
    if len(lb) < 2 or any(x["file_id"] not in clips or x["playback"] not in la for x in lb):
        continue
    parts = []
    for x in lb:
        a = la[x["playback"]]
        ta, tb = a["offset"] + a["time"], x["offset"] + x["time"]
        p = predict(clips[x["file_id"]], ta, tb, x["mirror"])
        if p is None:
            break
        parts.append((x["weight"], p, clips[x["file_id"]]["name"]))
    else:
        native = struct.unpack("<3f", bytes.fromhex(ev[k]["snapshot_hex"])[:12])
        n = math.hypot(*native)
        if n < 1e-4:
            continue
        wsum = sum(w for w, _, _ in parts)
        weighted = [sum(w * p[i] for w, p, _ in parts) for i in range(3)]
        heavy = max(parts, key=lambda t: t[0])
        rows.append({
            "native": n,
            "weighted": math.hypot(*weighted),
            "normalized": math.hypot(*weighted) / wsum,
            "heaviest": math.hypot(*heavy[1]),
            "wsum": wsum,
            "heavy_name": heavy[2],
            "names": sorted({nm for _, _, nm in parts}),
        })
print(f"{len(ev)} setter events, {len(rows)} multi-leaf frames compared")
for model in ("weighted", "normalized", "heaviest"):
    r = [x["native"] / x[model] for x in rows if x[model] > 1e-6]
    err = [abs(x["native"] - x[model]) * 1000 for x in rows]
    print(f"  {model:10s} native/pred median {statistics.median(r):.4f} "
          f"(p10 {sorted(r)[len(r) // 10]:.3f}, p90 {sorted(r)[9 * len(r) // 10]:.3f}); "
          f"|err| median {statistics.median(err):.2f} mm")
print("  weight sum median", round(statistics.median(x["wsum"] for x in rows), 4))
sprintish = [x for x in rows if x["heavy_name"].endswith("Sprint_3-Loop_V02")]
print(f"sprint-dominant frames: {len(sprintish)}")
for model in ("weighted", "heaviest"):
    r = [x["native"] / x[model] for x in sprintish if x[model] > 1e-6]
    if r:
        print(f"  {model:10s} native/pred median {statistics.median(r):.4f}")
