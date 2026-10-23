import json
import sqlite3
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
db = sqlite3.connect(ROOT / "local" / "asset_index.sqlite")

cells = json.loads((ROOT / "local" / "paris-cells.json").read_text(encoding="utf-8"))
by_name = {c["name"]: c for c in cells}
cell = by_name["Cell05201_DataBlock"]
fid = cell["file_id"]
print(f"cell {cell['name']} file_id {fid} entry {cell['entry_index']}")

print("\nresources that reference this cell id:")
for row in db.execute(
    "SELECT r.file_id, r.name, r.type_id, e.idx, a.name "
    "FROM link l JOIN resource r ON r.id=l.src_resource_id "
    "JOIN entry e ON e.id=r.entry_id JOIN archive a ON a.id=e.archive_id "
    "WHERE l.dst_file_id=? LIMIT 20",
    (fid,),
):
    print(f"  {row[0]} {row[2]} entry {row[3]} {row[1]!r} [{row[4]}]")

print("\nfan-in of the cell id:")
for row in db.execute(
    "SELECT parents, distinct_parent_ids FROM fan_in WHERE dst_file_id=?", (fid,)
):
    print(f"  parents {row[0]}, distinct ids {row[1]}")

print("\nresources with the most links to other ids:")
for row in db.execute(
    "SELECT r.file_id, r.name, r.type_id, e.idx, COUNT(l.id) AS n "
    "FROM resource r JOIN entry e ON e.id=r.entry_id "
    "JOIN link l ON l.src_resource_id=r.id "
    "WHERE r.type_id NOT IN ('0984415e','415d9568') "
    "GROUP BY r.id ORDER BY n DESC LIMIT 12"
):
    print(f"  {row[0]} {row[2]} entry {row[3]} links {row[4]} {row[1]!r}")

print("\nhow many distinct resources reference anything inside entry 5311:")
row = db.execute(
    "SELECT COUNT(DISTINCT l.src_resource_id) FROM link l WHERE l.dst_file_id IN "
    "(SELECT r.file_id FROM resource r JOIN entry e ON e.id=r.entry_id WHERE e.idx=5311)"
).fetchone()
print("  distinct sources:", row[0])
