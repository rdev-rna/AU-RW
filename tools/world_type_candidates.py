"""Rank candidate class names that could name world/map/cell resource types.

Filters the CRC table built by `type_name_crc.py` down to identifiers whose name
suggests world, map, cell, streaming, entity, LOD or collision data, and prints them
with their type id. Presence here proves nothing; it only keeps the eventual
intersection with an observed-type census short.
"""
import json
import re
from pathlib import Path

TABLE = Path(r"C:\Users\Administrator\AU-RW\local\candidate-type-names.json")
OUT = Path(r"C:\Users\Administrator\AU-RW\local\world-type-candidates.json")

PATTERNS = [
    ("world", r"world"),
    ("map", r"(?<!bit)map|mapping"),
    ("cell", r"cell"),
    ("layer", r"layer"),
    ("entity", r"entit"),
    ("lod", r"lod|levelofdetail"),
    ("stream", r"stream"),
    ("district", r"district|cityblock|quartier"),
    ("geometry", r"geometry|ground|terrain|mesh"),
    ("collision", r"collision|havok|navmesh|navig"),
    ("scene", r"scene|level"),
    ("zone", r"zone|region|sector"),
]

COMPILED = [(label, re.compile(pattern, re.IGNORECASE)) for label, pattern in PATTERNS]


def main():
    table = json.loads(TABLE.read_text(encoding="utf-8"))
    buckets = {label: [] for label, _ in COMPILED}
    for crc, names in table.items():
        for name in names:
            for label, pattern in COMPILED:
                if pattern.search(name):
                    buckets[label].append((crc, name))
                    break
    payload = {}
    for label, rows in buckets.items():
        rows.sort(key=lambda item: (len(item[1]), item[1]))
        payload[label] = [dict(type_id=crc, name=name) for crc, name in rows]
        print(f"{label:11s} {len(rows):5d}  e.g. " +
              ", ".join(f"{name}({crc})" for crc, name in rows[:4]))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(payload, indent=1), encoding="utf-8")
    print(f"\nwrote {OUT}")
    print("total candidates:", sum(len(v) for v in payload.values()))


if __name__ == "__main__":
    main()
