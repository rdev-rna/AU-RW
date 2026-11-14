"""Compare our full animation decode with an independent third-party JSON export of the same
clips (ACU-RE-0061). The export stays local; this tool reads it by path and writes only derived
statistics.

    python tools/anim_json_compare.py JSON_DIR OUT_DIR [--forge DataPC.forge] [--entry 928]

JSON_DIR: files named `<ordinal>_-_<animation name>[.Animation.anim].json`, each with `Length`
and `Tracks` (TrackID, codec = our per-track type byte, keyframes with Frame at 60/s and
LocalPosition / LocalRotation / Raw / Int). `<ordinal>` is checked against the asset index
(resource ordinal in the entry).

Per clip, our decode comes from `acu-root-tracks --all-tracks` (animation::parse). Tracks are
paired by position. For each pair, the tool checks:
  - the type byte (codec * 3 + time mode);
  - the bone ID (TrackID, decimal) against our hash;
  - key times (Frame / 60) and value components (rotation compared up to quaternion sign);
    Raw / Int event keys are compared by time only.
"""
from __future__ import annotations

import argparse
import collections
import json
import re
import sqlite3
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXE = ROOT / "acu-rs" / "target" / "debug" / "acu-root-tracks.exe"


def game_dir() -> Path:
    cfg = json.loads((ROOT / "local" / "project.json").read_text())
    return Path(cfg["game_dir"])


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("json_dir", type=Path)
    ap.add_argument("out", type=Path)
    ap.add_argument("--forge", default="DataPC.forge")
    ap.add_argument("--entry", type=int, default=928)
    a = ap.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    exports = {}
    for f in sorted(a.json_dir.glob("*.json")):
        m = re.match(r"(\d+)_-_(.+?)(?:\.Animation\.anim)?\.json$", f.name)
        if not m:
            continue
        exports.setdefault(m.group(2), (int(m.group(1)), f))  # first copy of a duplicate name
    db = sqlite3.connect(f"file:{(ROOT / 'local' / 'asset_index.sqlite').as_posix()}?mode=ro", uri=True)
    rows = db.execute(
        "SELECT r.name, r.file_id, r.ordinal FROM resource r JOIN entry e ON r.entry_id = e.id "
        "JOIN archive ar ON e.archive_id = ar.id WHERE ar.name = ? AND e.idx = ? AND r.type_id = '0fa3067f'",
        (a.forge, a.entry)).fetchall()
    by_name = {n: (fid, o) for n, fid, o in rows}
    plan, ordinal_mismatch, missing = {}, [], []
    for name, (ordinal, f) in exports.items():
        if name not in by_name:
            missing.append(name)
            continue
        fid, o = by_name[name]
        if o != ordinal:
            ordinal_mismatch.append([name, ordinal, o])
        plan[fid] = (name, f)
    dump = a.out / "ours.json"
    subprocess.run([str(EXE), "--all-tracks", str(game_dir() / a.forge), str(a.entry), str(dump), *plan],
                   check=True, stdout=subprocess.DEVNULL)
    ours = {c["file_id"]: c for c in json.load(open(dump))["clips"]}
    per_type = collections.defaultdict(lambda: {"tracks": 0, "keys": 0, "max_err": 0.0, "type_mismatch": 0,
                                                "time_mismatch": 0, "count_mismatch": 0})
    clips = []
    for fid, (name, f) in sorted(plan.items(), key=lambda x: x[1][0]):
        c = ours[fid]
        row = {"file_id": fid, "name": name}
        if "error" in c:
            row["error"] = c["error"]
            clips.append(row)
            continue
        j = json.load(open(f))
        row["duration_diff"] = abs(j["Length"] - c["duration"])
        row["track_count"] = [len(j["Tracks"]), len(c["tracks"])]
        bone_mismatch = 0
        worst = 0.0
        for jt, t in zip(j["Tracks"], c["tracks"]):
            ty = jt["codec"]
            s = per_type[ty]
            s["tracks"] += 1
            if t["codec"] != ty // 3:
                s["type_mismatch"] += 1
                continue
            if int(jt["TrackID"]) != t["hash"]:
                bone_mismatch += 1
            keys = jt["Keyframes"]
            if len(keys) != len(t["times"]):
                s["count_mismatch"] += 1
                continue
            for k, tm, v in zip(keys, t["times"], t["values"]):
                s["keys"] += 1
                if abs(k["Frame"] / 60 - tm) > 1e-5:
                    s["time_mismatch"] += 1
                ref = k.get("LocalRotation") or k.get("LocalPosition")
                if ref is None:
                    continue  # Raw / Int event payloads: times only
                got = v[:len(ref)]
                err = max(abs(x - y) for x, y in zip(ref, got))
                if "LocalRotation" in k:
                    err = min(err, max(abs(x + y) for x, y in zip(ref, got)))
                s["max_err"] = max(s["max_err"], err)
                worst = max(worst, err)
        row["bone_id_mismatch"] = bone_mismatch
        row["max_err"] = worst
        clips.append(row)
    report = {"exports": len(exports), "compared": len(plan), "missing_in_index": missing,
              "ordinal_mismatch": ordinal_mismatch,
              "decode_errors": [r for r in clips if "error" in r],
              "per_type_byte": {str(k): v for k, v in sorted(per_type.items())},
              "worst_clips": sorted((r for r in clips if "max_err" in r), key=lambda r: -r["max_err"])[:5]}
    (a.out / "compare.json").write_text(json.dumps({"report": report, "clips": clips}, indent=1) + "\n")
    print(json.dumps(report, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
