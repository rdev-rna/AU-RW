import sqlite3

db = sqlite3.connect(r"C:\Users\Administrator\AU-RW\local\asset_index.sqlite")

for type_id in ("eebb2443", "41813744", "5730d30e", "cd4f18f6", "463b1c26", "3a1a8407"):
    labels = db.execute(
        "SELECT label, confidence, basis FROM type_label WHERE type_id=?", (type_id,)
    ).fetchall()
    print(f"\ntype {type_id}: labels {labels}")
    rows = db.execute(
        "SELECT r.file_id, r.name, e.idx FROM resource r JOIN entry e ON e.id=r.entry_id "
        "WHERE r.type_id=? LIMIT 6", (type_id,)
    ).fetchall()
    for row in rows:
        print("   ", row)
    count = db.execute(
        "SELECT COUNT(*) FROM resource WHERE type_id=?", (type_id,)
    ).fetchone()[0]
    print(f"    total resources: {count}")
