"""Pair the game's parkour decisions with the animation clips that start right after them.

    python tools/parkour_clip_pairs.py <decoded/decisions.jsonl> <live_state.jsonl> <out-dir> [--window 1.5] [--names]

Inputs share the QPC clock: decisions from tools/parkour_trace.py (entry `qpc`), samples from
tools/native_probe/live_state.py. A clip *start* is a graph-leaf file ID that appears in a sample
but not in the previous one, or whose playback time went backwards; its start time is estimated
as sample QPC minus its playback time (when 0 <= time < 2 s), else the sample QPC.
For each decision that selected an action, every clip start within --window seconds after it is
listed with its delay and position. Per type, the first clip start is counted.

This is temporal association, not proof of causation: other systems (locomotion, cloth-free
additive layers) start clips too. Names (--names) come from the local asset index
(tools/asset_query.py; label/confidence reported as given there).
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def load_samples(path: Path) -> tuple[int, list[dict]]:
    freq, samples = None, []
    for line in path.read_text(encoding="utf-8").splitlines():
        row = json.loads(line)
        if row.get("kind") == "header":
            freq = row["qpc_frequency"]
        elif row.get("kind") == "sample" and row.get("clips") is not None:
            samples.append(row)
    if not freq:
        raise SystemExit(f"{path}: no header with qpc_frequency")
    return freq, samples


def clip_starts(freq: int, samples: list[dict]) -> list[dict]:
    starts, previous = [], {}
    for s in samples:
        current = {c["file_id"]: c for c in s["clips"]}
        for fid, c in current.items():
            before = previous.get(fid)
            if before is None or c["time"] < before["time"] - 1e-3:
                lag = c["time"] if 0 <= c["time"] < 2 else 0.0
                starts.append({"file_id": fid, "qpc": s["qpc"] - int(lag * freq), "sample_qpc": s["qpc"],
                               "weight": c["weight"], "position": s.get("position"), "forward": s.get("forward"),
                               "first_sample": not previous})
        previous = current
    return [x for x in starts if not x["first_sample"]]  # clips already playing at the start are not starts


def names_for(ids: set[str]) -> dict[str, dict]:
    out = {}
    for fid in sorted(ids):
        r = subprocess.run([sys.executable, str(ROOT / "tools/asset_query.py"), "where", fid],
                           capture_output=True, text=True)
        for line in r.stdout.splitlines():
            parts = line.split()
            if len(parts) >= 7 and parts[0].endswith(".forge"):
                out[fid] = {"name": parts[6], "label": parts[4], "confidence": parts[5], "archive": parts[0],
                            "entry": parts[1], "ordinal": parts[2]}
                break
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("decisions", type=Path)
    ap.add_argument("samples", type=Path)
    ap.add_argument("output", type=Path)
    ap.add_argument("--window", type=float, default=1.5)
    ap.add_argument("--names", action="store_true")
    a = ap.parse_args()
    freq, samples = load_samples(a.samples)
    starts = clip_starts(freq, samples)
    first_q, last_q = (samples[0]["qpc"], samples[-1]["qpc"]) if samples else (0, 0)
    pairs, per_type = [], defaultdict(Counter)
    for line in a.decisions.read_text(encoding="utf-8").splitlines():
        d = json.loads(line)
        sel = d.get("selected")
        if not sel or d.get("selected_index", -1) < 0 or not first_q <= d["qpc"] <= last_q:
            continue
        after = [{"file_id": s["file_id"], "delay_s": round((s["qpc"] - d["qpc"]) / freq, 3), "weight": s["weight"],
                  "position": s["position"]}
                 for s in starts if 0 <= s["qpc"] - d["qpc"] <= a.window * freq]
        after.sort(key=lambda x: x["delay_s"])
        pairs.append({"qpc": d["qpc"], "enum": sel["enum"], "fitness": sel["fitness"], "distance": sel["distance"],
                      "clip_starts": after})
        if after:
            per_type[sel["enum"]][after[0]["file_id"]] += 1
    names = names_for({s["file_id"] for s in starts}) if a.names else {}
    a.output.mkdir(parents=True, exist_ok=True)
    (a.output / "pairs.jsonl").write_text("".join(json.dumps(p) + "\n" for p in pairs), encoding="utf-8")
    summary = {"samples": len(samples), "clip_starts": len(starts), "selected_decisions_in_window": len(pairs),
               "window_s": a.window,
               "first_clip_by_type": {str(k): [{"file_id": f, "count": n, **({"name": names[f]["name"]} if f in names else {})}
                                               for f, n in v.most_common()] for k, v in sorted(per_type.items())},
               "names": names,
               "limits": "temporal association within the window; not causation; names from the local asset index"}
    (a.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: summary[k] for k in ("samples", "clip_starts", "selected_decisions_in_window")}))


if __name__ == "__main__":
    main()
