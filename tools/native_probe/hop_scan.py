"""Map a whole world region live: hop Arno across it by teleport and inventory every loaded entity.

    python tools/native_probe/hop_scan.py <pid> <out-dir> [--radius 150] [--hop-min 60] [--cell 50]
        [--max-hops 400] [--max-minutes 120] [--settle 4] [--bounds XMIN YMIN XMAX YMAX]

Loop: inventory loaded entities around Arno (world_entities.py: World back-pointer heap scan,
names via [E+0x130]+0x10), add them to the map, mark --cell grid cells within --radius as
covered (--cover-radius, inside the inventory radius), then teleport (teleport.py) to the nearest upright NPC's position (known walkable
ground) that lies in an uncovered cell at least --hop-min metres away; NPC positions from every earlier hop
stay in the frontier until their cell is covered. Ends when no NPC stands in
an uncovered cell (inside --bounds, if given), or on the hop/time budget.

Safety for unattended runs (ACU-RE-0046):
- Invisible (the debug menu's flag on the behaviour component) and God Mode are switched on at
  the start and re-asserted after every hop; enemies ignore Arno and falls are not fatal.
- After each hop the clip stream is sampled; any xx_Fight_* / *Hurt* / *Death* clip marks the
  cell hostile and Arno is moved back to the previous hop.
- If the player chain breaks or the entity changes (desync, death, loading), the scan waits for
  the world to come back (up to 180 s), records it and continues from where the game put Arno.
Outputs: map.jsonl (one row per named or unnamed entity, deduplicated by file ID, else by rounded
position), hops.jsonl, coverage.json. Positions are entity origins (Z up, metres), not bounds.
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import sqlite3
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import live_state  # noqa: E402
import teleport  # noqa: E402
import world_entities  # noqa: E402

DANGER = ("xx_Fight_", "Hurt", "Death", "_Dying", "Desync")


def clip_names(db, ids: set[str], cache: dict) -> list[str]:
    out = []
    for fid in ids:
        if fid not in cache:
            row = db.execute("SELECT name FROM resource WHERE file_id = ? LIMIT 1", (fid,)).fetchone()
            cache[fid] = (row and row[0]) or fid
        out.append(cache[fid])
    return out


def danger_check(pid: int, db, cache: dict, seconds: float = 1.0) -> list[str]:
    mem, seen = live_state.Memory(pid), set()
    try:
        state: dict = {}
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            s = live_state.sample(mem, state)
            seen |= {c["file_id"] for c in s.get("clips") or []}
            time.sleep(1 / 15)
    finally:
        mem.close()
    return [n for n in clip_names(db, seen, cache) if any(d in n for d in DANGER)]


def wait_world(pid: int, timeout: float = 180) -> dict | None:
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        mem = live_state.Memory(pid)
        try:
            found = live_state.player(mem)
            if found:
                p = live_state.sample(mem, {})["position"]
                if p and math.hypot(*p) > 1:
                    return {"entity": f"0x{found[0]:X}", "position": p}
        finally:
            mem.close()
        time.sleep(2)
    return None


def safety_on(pid: int) -> dict:
    return {"invisible": teleport.set_flag(pid, teleport.MODE_INVISIBLE, True),
            "god": teleport.set_flag(pid, teleport.MODE_GOD, True)}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pid", type=int)
    ap.add_argument("output", type=Path)
    ap.add_argument("--radius", type=float, default=150)
    ap.add_argument("--cover-radius", type=float, default=75,
                    help="cells within this distance of a hop count as covered (must be < --radius so NPCs "
                         "seen beyond it can be hop targets)")
    ap.add_argument("--hop-min", type=float, default=60)
    ap.add_argument("--cell", type=float, default=50)
    ap.add_argument("--max-hops", type=int, default=400)
    ap.add_argument("--max-minutes", type=float, default=120)
    ap.add_argument("--settle", type=float, default=4)
    ap.add_argument("--bounds", type=float, nargs=4, metavar=("XMIN", "YMIN", "XMAX", "YMAX"))
    a = ap.parse_args()
    out = a.output
    out.mkdir(parents=True, exist_ok=True)
    db = sqlite3.connect(f"file:{world_entities.INDEX}?mode=ro", uri=True)
    names: dict = {}
    seen: dict = {}
    covered, hostile, frontier = set(), set(), []
    cell = lambda p: (math.floor(p[0] / a.cell), math.floor(p[1] / a.cell))
    inside = lambda p: not a.bounds or (a.bounds[0] <= p[0] <= a.bounds[2] and a.bounds[1] <= p[1] <= a.bounds[3])
    hops_file = (out / "hops.jsonl").open("a", encoding="utf-8")
    map_file = (out / "map.jsonl").open("a", encoding="utf-8")
    deadline = time.monotonic() + 60 * a.max_minutes
    hop = {"hop": 0, "safety": safety_on(a.pid)}
    previous = None
    for n in range(a.max_hops):
        if time.monotonic() > deadline:
            hop["stop"] = "time budget"
            break
        inv = world_entities.scan(a.pid, a.radius)
        here = inv["player_position"]
        new = 0
        for e in inv["entities"]:
            if e["player"] or e["animated"]:
                continue
            key = e.get("file_id") or f"{e['position'][0]:.1f},{e['position'][1]:.1f},{e['position'][2]:.1f}"
            if key not in seen:
                seen[key] = True
                map_file.write(json.dumps({k: e[k] for k in ("file_id", "name", "stem", "position", "forward", "up")
                                           if k in e}) + "\n")
                new += 1
        map_file.flush()
        r_cells = math.ceil(a.cover_radius / a.cell)
        cx, cy = cell(here)
        for i in range(-r_cells, r_cells + 1):
            for j in range(-r_cells, r_cells + 1):
                if math.hypot(i, j) * a.cell <= a.cover_radius:
                    covered.add((cx + i, cy + j))
        # Frontier: every upright NPC position seen on any hop (known walkable ground).
        frontier.extend(e["position"] for e in inv["entities"]
                        if e["animated"] and not e["player"] and e["up"][2] > 0.99 and inside(e["position"]))
        npcs = frontier
        hop.update(n=n, at=here, entities=len(inv["entities"]), new=new, mapped=len(seen), covered=len(covered),
                   seconds=time.monotonic())
        hops_file.write(json.dumps(hop) + "\n")
        hops_file.flush()
        print(json.dumps({k: hop.get(k) for k in ("n", "at", "new", "mapped", "covered", "event")}), flush=True)
        targets = [p for p in npcs if cell(p) not in covered and cell(p) not in hostile
                   and math.dist(p[:2], here[:2]) >= a.hop_min]
        if not targets:
            hop = {"stop": "no NPC in an uncovered cell"}
            hops_file.write(json.dumps(hop) + "\n")
            break
        target = min(targets, key=lambda p: math.dist(p[:2], here[:2]))
        frontier[:] = [p for p in frontier if cell(p) not in covered]
        previous = here
        hop = {"hop": n + 1, "target": target}
        t = teleport.teleport(a.pid, *target, settle=a.settle)
        hop["teleport"] = {k: t.get(k) for k in ("status", "error_m", "after", "attempts")}
        if t.get("fell_through"):
            # Ground never caught up after retries: go back to the last good hop, skip that cell.
            hostile.add(cell(target))
            teleport.teleport(a.pid, *previous, settle=a.settle)
            hop["event"] = "fell through after retries; cell skipped, moved back"
        elif t.get("status") != 1 or t.get("error_m", 99) > 2:
            back = wait_world(a.pid)
            hop["event"] = f"teleport failed; world {'back' if back else 'not back'}"
            if not back:
                hop["stop"] = "world did not come back"
                hops_file.write(json.dumps(hop) + "\n")
                break
        hop["safety"] = safety_on(a.pid)
        danger = danger_check(a.pid, db, names)
        if danger:
            hostile.add(cell(target))
            hop["event"] = f"danger clips {sorted(set(danger))[:4]}; cell marked hostile, moved back"
            teleport.teleport(a.pid, *previous, settle=a.settle)
    (out / "coverage.json").write_text(json.dumps({"cell_m": a.cell, "covered": sorted(covered),
                                                    "hostile": sorted(hostile), "mapped": len(seen)}) + "\n")
    hops_file.close()
    map_file.close()


if __name__ == "__main__":
    main()
