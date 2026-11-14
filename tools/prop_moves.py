"""Label prop types with the parkour moves the original game used on them (ACU-RE-0046).

    python tools/prop_moves.py <prop-probe run dir> [<more run dirs> ...] --output <prop-moves.json>

Input: run-clips.jsonl from tools/native_probe/vault_scan.py run with labelled prop seeds (each run
sprints at one instance of a prop type from one side). Clip names (local asset index, crc-name
confidence) are grouped into move families by name pattern below; a family counts for a run when
any clip of that family started during it. Per prop type (name stem) the result gives runs, the
fraction of runs per family, the clips seen and the instances tried.

Limits: temporal association (whatever Arno met during the 2.5 s run counts, including nearby
geometry other than the prop); approach points are the prop origin minus 6.5 m, at the prop's
height, not checked for obstacles; few runs per type.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
import re

FAMILIES = [  # first match wins
    ("vault", r"SkipOver|SlideOver|_Vault"),
    ("slide_under", r"SlideUnder|PassUnder"),
    ("step_up", r"JumpOn|StepUp|SprintingStepUp|StepLevel"),
    ("wall_run_climb", r"WallEntry|PreWallEntry|ShortWallEntry|Climb|Muraille"),
    ("beam", r"Beam"),
    ("leap", r"Leaping|LeapFeet|LeapLevel|GroundJump"),
    ("drop", r"StepDown|SprintingStepDown|Landing|LandMid|LandSoft|LandHard|_Dn\d|Dn\d+cm"),
    ("brace_or_bump", r"WallBrace|Bump|Collide|Fail"),
    ("swing", r"Swing"),
]


def family(name: str) -> str | None:
    for fam, pattern in FAMILIES:
        if re.search(pattern, name):
            return fam
    return "other_parkour" if "Parkour" in name else None


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("runs", type=Path, nargs="+")
    ap.add_argument("--output", type=Path, required=True)
    a = ap.parse_args()
    per = defaultdict(lambda: {"runs": 0, "families": Counter(), "clips": Counter(), "instances": set()})
    for d in a.runs:
        for line in (d / "run-clips.jsonl").read_text(encoding="utf-8").splitlines():
            r = json.loads(line)
            label = r.get("label") or {}
            stem = label.get("stem")
            if not stem:
                continue
            p = per[stem]
            p["runs"] += 1
            p["instances"].add(label.get("name"))
            fams = {f for f in (family(c) for c in r["clips"]) if f}
            p["families"].update(fams)
            p["clips"].update(c for c in r["clips"] if family(c))
    out = {}
    for stem, p in sorted(per.items(), key=lambda kv: -kv[1]["families"].get("vault", 0)):
        out[stem] = {"runs": p["runs"], "instances": sorted(i for i in p["instances"] if i),
                     "moves": {f: round(n / p["runs"], 2) for f, n in p["families"].most_common()},
                     "clips": dict(p["clips"].most_common(8))}
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps({"families": FAMILIES, "prop_types": out}, indent=1) + "\n", encoding="utf-8")
    for stem, v in list(out.items())[:25]:
        print(f"{stem:45s} runs={v['runs']:2d} {v['moves']}")


if __name__ == "__main__":
    main()
