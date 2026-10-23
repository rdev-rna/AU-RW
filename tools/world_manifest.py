import sqlite3

db = sqlite3.connect(r"C:\Users\Administrator\AU-RW\local\asset_index.sqlite")

print("type fbb63e47:")
for row in db.execute(
    "SELECT type_id, name, category, confidence FROM type_label WHERE type_id='fbb63e47'"
):
    print(" ", row)

print("\nACU_Paris resource:")
for row in db.execute(
    "SELECT r.file_id, r.type_id, r.name, e.idx, a.name, r.record_length "
    "FROM resource r JOIN entry e ON e.id=r.entry_id JOIN archive a ON a.id=e.archive_id "
    "WHERE r.file_id='0000000f18b7e1cf'"
):
    print(" ", row)

print("\nwhat its links point at (by destination type):")
for row in db.execute(
    "SELECT dr.type_id, tl.name, COUNT(*) FROM link l "
    "LEFT JOIN resource dr ON dr.file_id=l.dst_file_id "
    "LEFT JOIN type_label tl ON tl.type_id=dr.type_id "
    "WHERE l.src_resource_id=(SELECT id FROM resource WHERE file_id='0000000f18b7e1cf' LIMIT 1) "
    "GROUP BY dr.type_id ORDER BY COUNT(*) DESC LIMIT 20"
):
    print(f"  {row[0]} {row[1]} {row[2]}")

print("\nhow many of its links are cell ids:")
row = db.execute(
    "SELECT COUNT(*) FROM link l WHERE l.src_resource_id="
    "(SELECT id FROM resource WHERE file_id='0000000f18b7e1cf' LIMIT 1) "
    "AND l.dst_file_id IN (SELECT r.file_id FROM resource r WHERE r.type_id='ac2bbf68')"
).fetchone()
print("  cells:", row[0])
