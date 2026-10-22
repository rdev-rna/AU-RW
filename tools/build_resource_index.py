"""Emit a compact id -> entry index for one resource type from a scan census.

The map loader needs to turn a LODSelector *resource* id into the archive *entry* that
carries it; entries and resources have different id spaces. The census already contains
the mapping, so this writes just the selector rows as a binary the viewer can load:
magic "ACUSEL01", u32 count, then count x (u64 id, u32 entry) sorted by id.
"""
import json
import struct
import sys
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
ENTRIES = ROOT / "local/scan-paris/entries.jsonl"
TYPE = sys.argv[1] if len(sys.argv) > 1 else "51dc6b80"
OUT = Path(sys.argv[2]) if len(sys.argv) > 2 else ROOT / "local/paris-selectors.bin"

rows = []
with ENTRIES.open(encoding="utf-8") as stream:
    for line in stream:
        if '"resources"' not in line or f'"{TYPE}"' not in line:
            continue
        record = json.loads(line)
        entry = record["entry"]["index"]
        for resource in record["resources"]:
            if resource["type_id"] == TYPE:
                rows.append((int(resource["file_id"], 16), entry))

rows.sort()
with OUT.open("wb") as out:
    out.write(b"ACUSEL01")
    out.write(struct.pack("<I", len(rows)))
    for file_id, entry in rows:
        out.write(struct.pack("<QI", file_id, entry))
print(f"{TYPE}: {len(rows)} rows -> {OUT} ({OUT.stat().st_size} bytes)")
