import sqlite3

db = sqlite3.connect(r"C:\Users\Administrator\AU-RW\local\asset_index.sqlite")
print("entry 38 in Paris:")
for row in db.execute(
    "SELECT a.name, e.idx, e.kind, e.typed_status, e.stored_length, e.file_id "
    "FROM entry e JOIN archive a ON a.id=e.archive_id "
    "WHERE a.name LIKE '%DataPC_ACU_Paris.forge' AND e.idx=38"
):
    print("  ", row)

print("\nresource 0000003f5799ac40:")
for row in db.execute(
    "SELECT a.name, e.idx, e.kind, e.typed_status, e.stored_length, r.name, r.type_id "
    "FROM resource r JOIN entry e ON e.id=r.entry_id JOIN archive a ON a.id=e.archive_id "
    "WHERE r.file_id='0000003f5799ac40'"
):
    print("  ", row)
