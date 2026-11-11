"""Summarise the Paris cell census: world cells vs empty/minimap tiles.

Read-only. Usage: python tools/world_cells.py [--limit N]
Writes local/paris-world-cells.json (entry index + name + record length, largest first).
"""
import json
import sys
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
CELLS = ROOT / "local/paris-cells.json"
OUT = ROOT / "local/paris-world-cells.json"


def main(limit):
    cells = json.loads(CELLS.read_text(encoding="utf-8"))
    world = [c for c in cells if c.get("record_length", 0) > 1000]
    world.sort(key=lambda c: c["record_length"], reverse=True)
    OUT.write_text(json.dumps(world, indent=1), encoding="utf-8")
    print(f"{len(cells)} cells, {len(world)} with record_length > 1000 -> {OUT}")
    print(f"{'entry':>6} {'length':>9}  name")
    for cell in world[:limit]:
        print(f"{cell['entry_index']:6} {cell['record_length']:9}  {cell['name']}")


if __name__ == "__main__":
    main(int(sys.argv[2]) if len(sys.argv) > 2 and sys.argv[1] == "--limit" else 15)
