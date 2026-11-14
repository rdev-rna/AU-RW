"""Compare the native per-frame root delta with one computed from decoded root tracks (ACU-RE-0058).

    python tools/root_delta_compare.py CAPTURE.jsonl ROOT_TRACKS.json [--out report.json]

CAPTURE.jsonl: tools/native_probe/root_delta_capture.py (setter events at rip 0x14009999B carry
the native translation/rotation delta; leaf samples carry clip, weight, time, offset, mirror).
ROOT_TRACKS.json: acu-root-tracks (track 0 translation, track 1 rotation; ACU-RE-0018).

Model under test (static, ACU-RE-0058): for a leaf going from clip time A to B,
  rotation delta = conj(Q(A)) * Q(B);  translation delta = R(Q(A))^-1 (T(B) - T(A));
mirror bit 0 negates x; times are offset +0x36C plus time +0x384. A loop wrap (B < A) composes
the segment A..end with 0..B. Only frames whose single non-negligible leaf has weight >= 0.999
are compared. Track values are interpolated linearly between keys; that is an assumption about
the native codec sampler (off_145137A40 method 2), and the residuals test it.
"""
from __future__ import annotations

import argparse
import bisect
import json
import math
import statistics

SETTER_RIP = 0x14009999B


def lerp_track(times, values, t):
    if t <= times[0]:
        return values[0][:3]
    if t >= times[-1]:
        return values[-1][:3]
    i = bisect.bisect_right(times, t)
    t0, t1 = times[i - 1], times[i]
    u = (t - t0) / (t1 - t0) if t1 > t0 else 0.0
    return [values[i - 1][k] + u * (values[i][k] - values[i - 1][k]) for k in range(3)]


def segment(clip, a, b):
    tr = clip["translation"]
    pa, pb = lerp_track(tr["times"], tr["values"], a), lerp_track(tr["times"], tr["values"], b)
    return [pb[k] - pa[k] for k in range(3)]


def predict(clip, a, b, mirror):
    rot = clip["rotation"]
    if len(rot["times"]) != 1 or rot["values"][0][:3] != [0.0, 0.0, 0.0]:
        return None  # rotating root: needs the quaternion path (not needed for these clips)
    if b >= a:
        d = segment(clip, a, b)
    else:  # loop wrap
        end = clip["duration"]
        d = [x + y for x, y in zip(segment(clip, a, end), segment(clip, 0.0, b))]
    if mirror:
        d[0] = -d[0]
    return d


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("capture")
    ap.add_argument("tracks")
    ap.add_argument("--out")
    a = ap.parse_args()
    clips = {c["file_id"]: c for c in json.load(open(a.tracks))["clips"]}
    recs = [json.loads(line) for line in open(a.capture)]
    events = sorted((e for r in recs if r["kind"] == "events" for e in r["events"]
                     if int(e["rip"], 16) == SETTER_RIP), key=lambda e: e["qpc"])
    samples = sorted((r for r in recs if r["kind"] == "leaves"), key=lambda r: r["qpc0"])
    sq = [s["qpc0"] for s in samples]

    def state_after(q0, q1):
        """Leaf state read strictly between two setter events (one graph update)."""
        i, j = bisect.bisect_right(sq, q0), bisect.bisect_left(sq, q1)
        return [s for s in samples[i:j] if s["qpc1"] < q1]

    rows = []
    for k in range(1, len(events) - 1):
        prev, cur, nxt = events[k - 1], events[k], events[k + 1]
        before, after = state_after(prev["qpc"], cur["qpc"]), state_after(cur["qpc"], nxt["qpc"])
        if not before or not after:
            continue
        lb, la = before[-1]["leaves"], after[0]["leaves"]
        main_b = [x for x in lb if x["weight"] > 0.001]
        main_a = [x for x in la if x["weight"] > 0.001]
        if len(main_a) != 1 or len(main_b) != 1 or main_a[0]["file_id"] != main_b[0]["file_id"]:
            continue
        leaf_a, leaf_b = main_b[0], main_a[0]  # A = before this frame's update, B = after
        if leaf_b["weight"] < 0.999 or leaf_b["playback"] != leaf_a["playback"] or leaf_b["file_id"] not in clips:
            continue
        ta, tb = leaf_a["offset"] + leaf_a["time"], leaf_b["offset"] + leaf_b["time"]
        p = predict(clips[leaf_b["file_id"]], ta, tb, leaf_b["mirror"])
        if p is None:
            continue
        native = list(__import__("struct").unpack("<8f", bytes.fromhex(cur["snapshot_hex"])))
        n = native[:3]
        err = math.dist(n, p)
        rows.append({"qpc": cur["qpc"], "file_id": leaf_b["file_id"], "ta": ta, "tb": tb, "mirror": leaf_b["mirror"],
                     "native": n, "native_rot": native[4:8], "predicted": p, "err_m": err,
                     "native_len": math.hypot(*n), "pred_len": math.hypot(*p)})
    by = {}
    for r in rows:
        by.setdefault(r["file_id"], []).append(r)
    summary = {}
    for fid, rs in by.items():
        moving = [r for r in rs if r["native_len"] > 1e-4]
        summary[fid] = {"name": clips[fid]["name"], "frames": len(rs), "moving": len(moving),
                        "err_median_mm": 1000 * statistics.median(r["err_m"] for r in rs),
                        "err_max_mm": 1000 * max(r["err_m"] for r in rs),
                        "len_ratio_median": statistics.median(r["native_len"] / r["pred_len"] for r in moving if r["pred_len"] > 1e-6) if moving else None,
                        "native_len_median_mm": 1000 * statistics.median(r["native_len"] for r in rs)}
    print(json.dumps({"events": len(events), "compared": len(rows), "per_clip": summary}, indent=1))
    if a.out:
        json.dump({"summary": summary, "rows": rows}, open(a.out, "w"), indent=1)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
