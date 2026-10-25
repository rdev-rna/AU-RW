import sqlite3

db = sqlite3.connect(r"C:\Users\Administrator\AU-RW\local\asset_index.sqlite")
row = db.execute(
    "SELECT r.file_id, r.name, r.record_length, e.idx, a.name FROM resource r "
    "JOIN entry e ON e.id=r.entry_id JOIN archive a ON a.id=e.archive_id "
    "WHERE r.type_id='eebb2443' LIMIT 3"
).fetchall()
for r in row:
    print(r)
