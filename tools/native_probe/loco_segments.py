"""Summarise a live_state.py capture as run-length segments by dominant clip.

Each segment prints its time span (seconds from the first sample), the dominant clip name
(asset index name, `xx_Nav_` dropped), the ground speed over the segment (planar position
change / time, m/s) and the yaw at its ends (degrees, left positive about +Z).
With `--windows S`, it also prints the planar speed in S-second windows.

Reads only the capture files and the local asset index (read-only).

    python tools/native_probe/loco_segments.py <run>/live-state.jsonl [--timeline <run>/scenario/input-timeline.jsonl] [--windows 0.5]
"""
from __future__ import annotations

import argparse
import json
import math
import sqlite3
from pathlib import Path

INDEX = Path(__file__).resolve().parents[2] / "local" / "asset_index.sqlite"


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("live_state")
    ap.add_argument("--timeline", help="controller_autoplay input-timeline.jsonl")
    ap.add_argument("--windows", type=float, default=0.0, help="window length in seconds")
    a = ap.parse_args()

    rows = [json.loads(line) for line in open(a.live_state)]
    freq = rows[0]["qpc_frequency"]
    s = [r for r in rows if r.get("kind") == "sample" and r.get("position")]
    if not s:
        raise SystemExit("no samples with a position")
    db = sqlite3.connect(f"file:{INDEX.as_posix()}?mode=ro", uri=True) if INDEX.exists() else None
    names: dict[str, str] = {}

    def name(fid: str) -> str:
        if fid not in names:
            row = None
            if db is not None:
                row = db.execute(
                    "select name from resource where file_id=? limit 1", (int(fid, 16),)
                ).fetchone() or db.execute(
                    "select name from resource where file_id=? limit 1", (fid,)
                ).fetchone()
            names[fid] = (row[0] if row else fid).replace("xx_Nav_", "")
        return names[fid]

    t0 = s[0]["qpc"] / freq

    def t(x: dict) -> float:
        return x["qpc"] / freq - t0

    def yaw(x: dict) -> float:
        return math.degrees(math.atan2(-x["forward"][0], x["forward"][1]))

    def dominant(x: dict) -> str:
        clips = x.get("clips") or []
        return name(max(clips, key=lambda c: c["weight"])["file_id"]) if clips else "-"

    def dist(a: dict, b: dict) -> float:
        return math.hypot(b["position"][0] - a["position"][0], b["position"][1] - a["position"][1])

    segs: list[list] = []
    for i, x in enumerate(s):
        d = dominant(x)
        if segs and segs[-1][0] == d:
            segs[-1][2] = i
        else:
            segs.append([d, i, i])
    for clip, i0, i1 in segs:
        a0, b0 = s[i0], s[min(i1 + 1, len(s) - 1)]
        dt = t(b0) - t(a0)
        v = dist(a0, b0) / dt if dt > 0 else 0.0
        print(f"{t(a0):7.2f}-{t(b0):7.2f}  {clip[:44]:44s} n={i1 - i0 + 1:3d} "
              f"v={v:5.2f} m/s yaw {yaw(a0):7.1f}->{yaw(b0):7.1f}")
    if a.windows > 0:
        print("windows:")
        i = 0
        while i < len(s) - 1:
            j = i
            while j < len(s) - 1 and t(s[j]) - t(s[i]) < a.windows:
                j += 1
            dt = t(s[j]) - t(s[i])
            print(f"  {t(s[i]):7.2f} v={dist(s[i], s[j]) / dt if dt > 0 else 0:5.2f} "
                  f"yaw={yaw(s[i]):7.1f} {dominant(s[i])}")
            i = j
    if a.timeline:
        print("timeline (s from the first step; not aligned with the capture clock):")
        steps = [x for x in map(json.loads, open(a.timeline)) if x["kind"] == "step_start"]
        for x in steps:
            st = x["state"]
            print(f"  {x['unix_time'] - steps[0]['unix_time']:7.2f} {x['name']} "
                  f"stick={st.get('left_stick')} rt={st.get('right_trigger')}")


if __name__ == "__main__":
    main()
