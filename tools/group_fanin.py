import sqlite3

db = sqlite3.connect(r"C:\Users\Administrator\AU-RW\local\asset_index.sqlite")

for group in ("00000011d0c3339f", "0000002190209a5f"):
    print(f"\ngroup {group}: referenced by")
    rows = db.execute(
        "SELECT sr.file_id, sr.type_id, sr.name, se.idx, a.name FROM link l "
        "JOIN resource sr ON sr.id=l.src_resource_id "
        "JOIN entry se ON se.id=sr.entry_id JOIN archive a ON a.id=se.archive_id "
        "WHERE l.dst_file_id=? LIMIT 10",
        (group,),
    ).fetchall()
    for row in rows:
        print("   ", row)
    if not rows:
        print("    (nothing)")

print("\ncount of 3f742d26 groups:")
print("  ", db.execute("SELECT COUNT(*) FROM resource WHERE type_id='3f742d26'").fetchone()[0])
print("count of 51dc6b80 selectors:")
print("  ", db.execute("SELECT COUNT(*) FROM resource WHERE type_id='51dc6b80'").fetchone()[0])
