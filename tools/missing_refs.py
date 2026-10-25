import sqlite3

db = sqlite3.connect(r"C:\Users\Administrator\AU-RW\local\asset_index.sqlite")

for entity in (
    "0000003f5767a70c",
    "0000001eb1980215",
    "00000023404bcf7a",
):
    row = db.execute(
        "SELECT r.name, e.idx, r.type_id FROM resource r JOIN entry e ON e.id=r.entry_id "
        "WHERE r.file_id=?",
        (entity,),
    ).fetchone()
    print(f"\n{entity} {row}")
    rows = db.execute(
        "SELECT l.dst_file_id, dr.type_id, dr.name, de.idx FROM link l "
        "JOIN resource sr ON sr.id=l.src_resource_id "
        "LEFT JOIN resource dr ON dr.file_id=l.dst_file_id "
        "LEFT JOIN entry de ON de.id=dr.entry_id "
        "WHERE sr.file_id=? LIMIT 15",
        (entity,),
    ).fetchall()
    for dst, type_id, name, entry in rows:
        print(f"   -> {dst} {type_id} entry {entry} {name!r}")
    if not rows:
        print("   (no candidate links)")
