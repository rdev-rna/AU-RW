"""Replay live traces against decoded clip root motion; the residual is what the clip alone does
not explain (warp toward a target, collision, physics). ACU-RE-0059.

    python tools/root_replay.py TRACKS.json RUN_DIR [RUN_DIR ...] [--out report.json]

TRACKS.json: acu-root-tracks output (root channels only, `animation::parse_root`).
RUN_DIR/live-state.jsonl: tools/native_probe/live_state.py samples (position, forward, leaves:
file ID, graph weight, playback time +0x384, offset +0x36C).

Per interval between consecutive samples that show the same single leaf (graph row and file ID,
weight >= 0.999, nothing else above 0.001):
  predicted world displacement = R(facing at the first sample) * clip root delta (ACU-RE-0058:
  R(Q(A))^-1 (T(B) - T(A)), loop wrap composed; ACU-RE-0057: world = R(entity) * delta);
  residual = actual displacement - predicted.
Implementation choices and limits:
  - R(entity) is built from the sampled forward (local +y) and world up (local +z); local +x is
    right = forward x up (HAND = +1) or the opposite (HAND = -1). Both are evaluated; the data
    picks one. The mirror bit is not in these traces: each interval also reports the residual
    with x mirrored, and the better of the two is kept as `mirror_guess` (an inference).
  - The facing changes within an interval (about 2 frames); the first sample's facing is used.
  - Graph weight is a presence/blend input (ACU-RE-0025); single-leaf intervals avoid blends.
"""
from __future__ import annotations

import argparse
import bisect
import collections
import json
import math
import re
import statistics
from pathlib import Path


def lerp(times, values, t):
    if t <= times[0]:
        return values[0]
    if t >= times[-1]:
        return values[-1]
    i = bisect.bisect_right(times, t)
    u = (t - times[i - 1]) / (times[i] - times[i - 1]) if times[i] > times[i - 1] else 0.0
    return [values[i - 1][k] + u * (values[i][k] - values[i - 1][k]) for k in range(4)]


def qnorm(q):
    n = math.sqrt(sum(x * x for x in q)) or 1.0
    return [x / n for x in q]


def slerp_track(times, values, t):
    if t <= times[0]:
        return qnorm(values[0])
    if t >= times[-1]:
        return qnorm(values[-1])
    i = bisect.bisect_right(times, t)
    u = (t - times[i - 1]) / (times[i] - times[i - 1]) if times[i] > times[i - 1] else 0.0
    a, b = qnorm(values[i - 1]), qnorm(values[i])
    d = sum(x * y for x, y in zip(a, b))
    if d < 0:
        b, d = [-x for x in b], -d
    if d > 0.9995:
        return qnorm([x + u * (y - x) for x, y in zip(a, b)])
    th = math.acos(d)
    s = math.sin(th)
    return [(math.sin((1 - u) * th) * x + math.sin(u * th) * y) / s for x, y in zip(a, b)]


def qmul(a, b):
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return [aw * bx + ax * bw + ay * bz - az * by, aw * by - ax * bz + ay * bw + az * bx,
            aw * bz + ax * by - ay * bx + az * bw, aw * bw - ax * bx - ay * by - az * bz]


def qrot(q, v):
    p = qmul(qmul(q, [v[0], v[1], v[2], 0.0]), [-q[0], -q[1], -q[2], q[3]])
    return p[:3]


def conj(q):
    return [-q[0], -q[1], -q[2], q[3]]


def segment(clip, a, b):
    tr, ro = clip["translation"], clip["rotation"]
    ta, tb = lerp(tr["times"], tr["values"], a)[:3], lerp(tr["times"], tr["values"], b)[:3]
    qa, qb = slerp_track(ro["times"], ro["values"], a), slerp_track(ro["times"], ro["values"], b)
    inv = conj(qa)
    return qrot(inv, [y - x for x, y in zip(ta, tb)]), qnorm(qmul(inv, qb))


def clip_delta(clip, a, b):
    if b >= a:
        return segment(clip, a, b)
    t1, q1 = segment(clip, a, clip["duration"])
    t2, q2 = segment(clip, 0.0, b)
    t = [x + y for x, y in zip(t1, qrot(q1, t2))]
    return t, qnorm(qmul(q1, q2))


def to_world(local, forward, hand):
    fx, fy, fz = forward
    n = math.hypot(fx, fy) or 1.0
    fx, fy = fx / n, fy / n
    right = [hand * fy, -hand * fx, 0.0]
    return [local[0] * right[k] + local[1] * f + local[2] * u for k, (f, u) in enumerate(zip((fx, fy, 0.0), (0.0, 0.0, 1.0)))]


def family(name):
    n = re.sub(r"^xx_", "", name)
    return n.split("_")[0].lower(), n


def intervals(run: Path):
    samples = [json.loads(line) for line in open(run / "live-state.jsonl")]
    samples = [s for s in samples if s.get("position") and s.get("forward") and "clips" in s]
    for s0, s1 in zip(samples, samples[1:]):
        if s0.get("entity") != s1.get("entity"):
            continue
        l0 = [c for c in s0["clips"] if c["weight"] > 0.001]
        l1 = [c for c in s1["clips"] if c["weight"] > 0.001]
        if len(l0) != 1 or len(l1) != 1:
            continue
        a, b = l0[0], l1[0]
        if a["file_id"] != b["file_id"] or a["row"] != b["row"] or min(a["weight"], b["weight"]) < 0.999:
            continue
        yield s0, s1, a, b


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("tracks")
    ap.add_argument("runs", nargs="+")
    ap.add_argument("--out")
    ap.add_argument("--max-gap", type=float, default=0.06, help="max seconds between samples")
    a = ap.parse_args()
    clips = {c["file_id"]: c for c in json.load(open(a.tracks))["clips"] if "error" not in c}
    rows = []
    for run in a.runs:
        for s0, s1, la, lb in intervals(Path(run)):
            dt_s = (s1["qpc"] - s0["qpc"]) / 1e7
            clip = clips.get(la["file_id"])
            if clip is None or not 0 < dt_s <= a.max_gap:
                continue
            ta, tb = la["offset"] + la["time"], lb["offset"] + lb["time"]
            if not (-1e-4 <= ta <= clip["duration"] + 1e-4 and -1e-4 <= tb <= clip["duration"] + 1e-4):
                continue
            if tb < ta and ta - tb < 0.5 * clip["duration"]:
                continue  # time went backwards without a wrap: a restart, not a step
            local, _ = clip_delta(clip, ta, tb)
            actual = [y - x for x, y in zip(s0["position"], s1["position"])]
            out = {"run": Path(run).name, "qpc": s0["qpc"], "dt": dt_s, "file_id": la["file_id"],
                   "name": clip["name"], "ta": ta, "tb": tb, "actual": actual}
            for hand in (1, -1):
                best = None
                for mirror in (0, 1):
                    lv = [-local[0], local[1], local[2]] if mirror else local
                    pred = to_world(lv, s0["forward"], hand)
                    res = [x - y for x, y in zip(actual, pred)]
                    cand = (math.hypot(*res), mirror, pred, res)
                    if best is None or cand[0] < best[0]:
                        best = cand
                out[f"hand{hand:+d}"] = {"err": best[0], "mirror_guess": best[1], "pred": best[2], "res": best[3]}
            out["local"] = local
            rows.append(out)
    # handedness: compare over intervals with real lateral clip motion
    lateral = [r for r in rows if abs(r["local"][0]) > 0.005]
    hand_votes = {h: statistics.median(r[f"hand{h:+d}"]["err"] for r in lateral) for h in (1, -1)} if lateral else {}
    hand = min(hand_votes, key=hand_votes.get) if hand_votes else 1
    by = collections.defaultdict(list)
    for r in rows:
        fam, _ = family(r["name"])
        by[fam].append(r)

    def stats(rs):
        e = [r[f"hand{hand:+d}"]["err"] / r["dt"] for r in rs]
        moving = [r for r in rs if math.hypot(*r["actual"]) > 1e-3]
        rel = [r[f"hand{hand:+d}"]["err"] / math.hypot(*r["actual"]) for r in moving]
        vz = [r[f"hand{hand:+d}"]["res"][2] / r["dt"] for r in rs]
        return {"intervals": len(rs), "clips": len({r["file_id"] for r in rs}),
                "residual_speed_median_m_s": statistics.median(e), "residual_speed_p90_m_s": sorted(e)[int(0.9 * (len(e) - 1))],
                "residual_over_actual_median": statistics.median(rel) if rel else None,
                "residual_vertical_speed_median_m_s": statistics.median(vz),
                "actual_speed_median_m_s": statistics.median(math.hypot(*r["actual"]) / r["dt"] for r in rs)}

    report = {"handedness_vote_median_err_m": hand_votes, "hand": hand, "intervals": len(rows),
              "families": {f: stats(rs) for f, rs in sorted(by.items(), key=lambda x: -len(x[1]))}}
    print(json.dumps(report, indent=1))
    if a.out:
        json.dump({"report": report, "rows": rows}, open(a.out, "w"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
