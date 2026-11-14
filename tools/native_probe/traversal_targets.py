"""Window targets for automated traversal capture, from offline placements and collision.

    python tools/native_probe/traversal_targets.py windows OUT.json --bounds XMIN YMIN XMAX YMAX
        [--archive DataPC_ACU_Paris.forge]

Sources (no game process):
- Placements: `local/paris/placements.jsonl` (tools/paris_placements.py, ACU-RE-0047). Open
  windows are entities whose stem matches `Interior_*_<floor>F_WD*_Open` (an inference from the
  names: WD = window, `_Open` = open; confirmed or refuted live by the driver).
- Collision: tools/world_collision.py `World.column` (ACU-RE-0050), ground = highest surface at
  or below a probe height.

Per window (implementation choices, each checked against collision where possible):
- The bbox is thin along rotation row 0, so row 0 is the window plane's normal and row 1 runs
  along its width.
- Outside vs inside: a column 1.0 m along +row0 and one along -row0 are probed. The side whose
  surfaces are named `Interior_*` (floors) is inside; the other must have ground below the sill
  that is not an interior floor. Windows where this is ambiguous are dropped.
- `ground_z`: support under the outside column (probe = sill). `sill_z` / `top_z`: bbox z
  extent. `clear`: no surface between ground + 0.3 m and top_z + 1 m in the outside columns at
  1.0, 2.5 and 4.0 m (awnings, balconies, carts).
"""
from __future__ import annotations

import argparse
import json
import math
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
WINDOW = re.compile(r"Interior_.*_(\d)F_WD.*_Open")


def windows_from_placements(rows, bounds):
    xmin, ymin, xmax, ymax = bounds
    for r in rows:
        m = WINDOW.search(r["stem"])
        if not m:
            continue
        p = r["position"]
        if not (xmin <= p[0] <= xmax and ymin <= p[1] <= ymax):
            continue
        b = r.get("bbox_world")
        if not b:
            continue
        yield {"file_id": r["file_id"], "stem": r["stem"], "floor": int(m.group(1)), "position": p,
               "row0": r["rotation_rows"][0], "row1": r["rotation_rows"][1],
               "center": [(b["min"][i] + b["max"][i]) / 2 for i in range(3)],
               "sill_z": b["min"][2], "top_z": b["max"][2],
               "width": abs(sum((b["max"][i] - b["min"][i]) * r["rotation_rows"][1][i] for i in range(2)))}


def horizontal(v):
    n = math.hypot(v[0], v[1])
    return (v[0] / n, v[1] / n) if n > 1e-6 else None


def offset(p, d, k):
    return (p[0] + d[0] * k, p[1] + d[1] * k)


def is_interior(hit):
    return "Interior_" in (hit.get("shape_name") or "")


def classify(window, column):
    """column(x, y) -> hits (top first, dicts with z and shape_name). Returns the window with
    outward normal, ground and clearance, or None with a reason."""
    n = horizontal(window["row0"])
    if n is None:
        return None, "row0 not horizontal"
    c = window["center"]
    sides = {}
    for sign in (1, -1):
        d = (n[0] * sign, n[1] * sign)
        hits = column(*offset(c, d, 1.0))
        below = [h for h in hits if h["z"] <= window["sill_z"] + 0.3]
        interior = any(is_interior(h) for h in below)
        ground = next((h for h in below), None)
        sides[sign] = (d, interior, ground)
    inside = [s for s in sides if sides[s][1]]
    if len(inside) != 1:
        return None, f"inside side ambiguous ({len(inside)} interior sides)"
    out = -inside[0]
    d, interior, ground = sides[out]
    if ground is None or is_interior(ground):
        return None, "no exterior ground below the sill"
    w = dict(window, outward=list(d), ground_z=ground["z"], ground_shape=ground.get("shape_name"))
    w["sill_height"] = w["sill_z"] - w["ground_z"]
    blockers = []
    for k in (1.0, 2.5, 4.0):
        for h in column(*offset(c, d, k)):
            if w["ground_z"] + 0.3 < h["z"] < w["top_z"] + 1.0:
                blockers.append({"at_m": k, "z": round(h["z"], 3), "shape": h.get("shape_name")})
    w["blockers"] = blockers
    w["clear"] = not blockers
    return w, None


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    w = sub.add_parser("windows")
    w.add_argument("out", type=Path)
    w.add_argument("--bounds", type=float, nargs=4, required=True)
    w.add_argument("--archive", default="DataPC_ACU_Paris.forge")
    w.add_argument("--placements", type=Path, default=ROOT / "local" / "paris" / "placements.jsonl")
    a = ap.parse_args()
    sys.path.insert(0, str(ROOT / "tools"))
    import world_collision  # noqa: E402

    rows = (json.loads(line) for line in open(a.placements, encoding="utf-8"))
    found = list(windows_from_placements(rows, a.bounds))
    xmin, ymin, xmax, ymax = a.bounds
    world = world_collision.World(a.archive, (xmin - 10, ymin - 10, xmax + 10, ymax + 10))
    kept, dropped = [], []
    for win in found:
        res, why = classify(win, world.column)
        (kept.append(res) if res else dropped.append({"file_id": win["file_id"], "stem": win["stem"], "why": why}))
    report = {"source": {"placements": str(a.placements), "archive": a.archive, "bounds": a.bounds},
              "windows": kept, "dropped": dropped}
    a.out.write_text(json.dumps(report, indent=1) + "\n")
    print(json.dumps({"found": len(found), "kept": len(kept), "clear": sum(k["clear"] for k in kept),
                      "dropped": len(dropped)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
