import sqlite3

db = sqlite3.connect(r"C:\Users\Administrator\AU-RW\local\asset_index.sqlite")
print("archives holding 415d9568 (world meshes), top 20:")
for row in db.execute(
    "SELECT a.name, COUNT(*) AS n FROM resource r "
    "JOIN entry e ON e.id=r.entry_id JOIN archive a ON a.id=e.archive_id "
    "WHERE r.type_id='415d9568' GROUP BY a.name ORDER BY n DESC LIMIT 20"
):
    print(f"  {row[1]:7} {row[0]}")

print("\narchives holding 51dc6b80 (selectors), top 10:")
for row in db.execute(
    "SELECT a.name, COUNT(*) AS n FROM resource r "
    "JOIN entry e ON e.id=r.entry_id JOIN archive a ON a.id=e.archive_id "
    "WHERE r.type_id='51dc6b80' GROUP BY a.name ORDER BY n DESC LIMIT 10"
):
    print(f"  {row[1]:7} {row[0]}")

print("\nParis-family archives:")
for row in db.execute(
    "SELECT name, size_bytes FROM archive WHERE name LIKE '%Paris%' ORDER BY name"
):
    print(f"  {row[1]/1e9:6.2f} GB  {row[0]}")
