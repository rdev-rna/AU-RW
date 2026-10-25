import sqlite3

db = sqlite3.connect(r"C:\Users\Administrator\AU-RW\local\asset_index.sqlite")

print("water meshes (sample):")
rows = db.execute(
    "SELECT r.id, r.file_id, r.name, e.idx FROM resource r JOIN entry e ON e.id=r.entry_id "
    "WHERE r.type_id='415d9568' AND r.name LIKE '%Water_Seine%' LIMIT 5"
).fetchall()
for row in rows:
    print("  ", row[1], row[2], "entry", row[3])

if rows:
    rid, fid, name, entry = rows[0]
    print(f"\nlinks of {fid} {name}:")
    for dst, type_id, dname, dentry in db.execute(
        "SELECT dr.file_id, dr.type_id, dr.name, de.idx FROM link l "
        "LEFT JOIN resource dr ON dr.file_id=l.dst_file_id "
        "LEFT JOIN entry de ON de.id=dr.entry_id WHERE l.src_resource_id=? LIMIT 12",
        (rid,),
    ):
        print(f"   {dst} {type_id} entry {dentry} {dname!r}")

    print("\nwater-named materials:")
    for row in db.execute(
        "SELECT r.file_id, r.name, e.idx FROM resource r JOIN entry e ON e.id=r.entry_id "
        "WHERE r.type_id='85c817c3' AND r.name LIKE '%Water%' LIMIT 10"
    ):
        print("   ", row)
