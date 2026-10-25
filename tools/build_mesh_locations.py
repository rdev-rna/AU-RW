"""Build an id -> (archive, entry) index for world meshes in the Paris patch archives.

The census index (ACUSEL01) stores only an entry number, so a mesh that lives in
`DataPC_ACU_Paris_assets_patch_03.forge` cannot be told apart from one in
`DataPC_ACU_Paris.forge` entry 38. This writer reads the SQLite asset index and emits:

  local/paris-mesh-locations.bin   magic ACUMES02, u32 count, rows (u64 id, u32 entry, u8 slot)
  local/paris-mesh-locations.json  slot -> archive file name

Paris.forge is excluded (the ACUSEL01 census index covers it); every other archive is
included, including Paris_assets and the patch archives. Read-only over the game files.
"""
import json
import sqlite3
import struct
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
DB = ROOT / "local" / "asset_index.sqlite"
OUT_BIN = ROOT / "local" / "paris-mesh-locations.bin"
OUT_JSON = ROOT / "local" / "paris-mesh-locations.json"

SKIP = {"DataPC_ACU_Paris.forge"}

db = sqlite3.connect(DB)
rows = db.execute(
    "SELECT a.name, e.idx, r.file_id FROM resource r "
    "JOIN entry e ON e.id=r.entry_id JOIN archive a ON a.id=e.archive_id "
    "WHERE r.type_id='415d9568'"
).fetchall()

slots = sorted({name for name, _, _ in rows if name not in SKIP})
slot_of = {name: index for index, name in enumerate(slots)}
records = []
for name, entry, file_id in rows:
    if name in SKIP:
        continue
    records.append((int(file_id, 16), entry, slot_of[name]))

records.sort()
with OUT_BIN.open("wb") as out:
    out.write(b"ACUMES02")
    out.write(struct.pack("<I", len(records)))
    for file_id, entry, slot in records:
        out.write(struct.pack("<QIB", file_id, entry, slot))

OUT_JSON.write_text(json.dumps(slots, indent=1), encoding="utf-8")
print(f"{len(records)} patch mesh locations over {len(slots)} archives -> {OUT_BIN}")
for name in slots:
    print("  ", name)
