import sqlite3

db = sqlite3.connect(r"C:\Users\Administrator\AU-RW\local\asset_index.sqlite")
print("largest meshes in entry 21267:")
for row in db.execute(
    "SELECT r.name, r.record_length FROM resource r JOIN entry e ON e.id=r.entry_id "
    "WHERE e.archive_id=63 AND e.idx=21267 AND r.type_id='415d9568' "
    "ORDER BY r.record_length DESC LIMIT 8"
):
    print(f"  {row[1]/1e6:8.2f} MB  {row[0]}")

print("\nlargemesh payload totals across Paris:")
row = db.execute(
    "SELECT COUNT(*), SUM(r.record_length), MAX(r.record_length) FROM resource r "
    "JOIN entry e ON e.id=r.entry_id WHERE e.archive_id=63 AND r.type_id='415d9568' "
    "AND r.name LIKE 'Largemesh%'"
).fetchone()
print(f"  count {row[0]}  total {row[1]/1e9:.2f} GB  max {row[2]/1e6:.1f} MB")

print("\nmesh payload size buckets (all Paris 415d9568):")
for row in db.execute(
    "SELECT CASE WHEN r.record_length<100000 THEN '<0.1MB' "
    "WHEN r.record_length<1000000 THEN '0.1-1MB' "
    "WHEN r.record_length<5000000 THEN '1-5MB' ELSE '>5MB' END AS bucket, "
    "COUNT(*), SUM(r.record_length) FROM resource r JOIN entry e ON e.id=r.entry_id "
    "WHERE e.archive_id=63 AND r.type_id='415d9568' GROUP BY bucket"
):
    print(f"  {row[0]:8} {row[1]:6}  {row[2]/1e9:.2f} GB")

print("\nhow many ground/largemesh entities are in the 8,330 real cells? (approx by resource count)")
row = db.execute(
    "SELECT COUNT(*) FROM resource r WHERE r.type_id='415d9568' AND r.name LIKE 'Largemesh%'"
).fetchone()
print(f"  largemesh meshes in index: {row[0]}")
