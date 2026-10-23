import sqlite3

db = sqlite3.connect(r"C:\Users\Administrator\AU-RW\local\asset_index.sqlite")

print("House meshes by archive (415d9568, name LIKE 'FRV_House%'):")
for row in db.execute(
    "SELECT a.name, COUNT(*) FROM resource r JOIN entry e ON e.id=r.entry_id "
    "JOIN archive a ON a.id=e.archive_id "
    "WHERE r.type_id='415d9568' AND r.name LIKE 'FRV_House%' GROUP BY a.name "
    "ORDER BY COUNT(*) DESC LIMIT 10"
):
    print("  ", row)

print("\nLargemesh mesh names in Paris (samples):")
for row in db.execute(
    "SELECT DISTINCT r.name FROM resource r JOIN entry e ON e.id=r.entry_id "
    "JOIN archive a ON a.id=e.archive_id "
    "WHERE r.type_id='415d9568' AND a.name='DataPC_ACU_Paris.forge' "
    "AND r.name LIKE 'Largemesh%' LIMIT 15"
):
    print("  ", row[0])

print("\nhouse selector copies:")
for row in db.execute(
    "SELECT a.name, e.idx FROM resource r JOIN entry e ON e.id=r.entry_id "
    "JOIN archive a ON a.id=e.archive_id "
    "WHERE r.type_id='51dc6b80' AND r.name='FRV_House_B_2F_WDS_09' LIMIT 8"
):
    print("  ", row)
