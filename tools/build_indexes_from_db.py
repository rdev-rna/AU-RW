"""Rebuild the viewer's compact indices from the complete SQLite asset index.

The old ACUSEL01 files were built from a partial census: `paris-world-meshes.bin` held 24,717
of the 116,526 Paris `415d9568` resources, so most building chunks could not be resolved.
This writer reads `local/asset_index.sqlite` (63 archives, 3.29 M resources) and emits the same
ACUSEL01 format for:

  local/paris-world-meshes.bin   415d9568  Paris meshes
  local/paris-selectors.bin      51dc6b80  Paris LOD selectors
  local/paris-entities.bin       0984415e  Paris entities

Read-only over the index; no game files are touched.
"""
import sqlite3
import struct
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
DB = ROOT / "local" / "asset_index.sqlite"

TARGETS = {
    "415d9568": ROOT / "local" / "paris-world-meshes.bin",
    "51dc6b80": ROOT / "local" / "paris-selectors.bin",
    "0984415e": ROOT / "local" / "paris-entities.bin",
}

db = sqlite3.connect(DB)
for type_id, out in TARGETS.items():
    rows = db.execute(
        "SELECT CAST(r.file_id AS TEXT), e.idx FROM resource r "
        "JOIN entry e ON e.id=r.entry_id JOIN archive a ON a.id=e.archive_id "
        "WHERE r.type_id=? AND a.name='DataPC_ACU_Paris.forge'",
        (type_id,),
    ).fetchall()
    records = sorted((int(file_id, 16), entry) for file_id, entry in rows)
    with out.open("wb") as handle:
        handle.write(b"ACUSEL01")
        handle.write(struct.pack("<I", len(records)))
        for file_id, entry in records:
            handle.write(struct.pack("<QI", file_id, entry))
    print(f"{type_id}: {len(records)} rows -> {out} ({out.stat().st_size / 1e6:.2f} MB)")
