import sqlite3

db = sqlite3.connect(r"C:\Users\Administrator\AU-RW\local\asset_index.sqlite")

for rid, label in ((3189275, "LargeMesh_IC_NotreDameCemetary_01"), (3189291, "Largemesh_IC_ISL_STR_01")):
    print(f"\n{label} (resource id {rid}) links:")
    rows = db.execute(
        "SELECT dr.file_id, dr.type_id, dr.name, de.idx FROM link l "
        "LEFT JOIN resource dr ON dr.file_id=l.dst_file_id "
        "LEFT JOIN entry de ON de.id=dr.entry_id "
        "WHERE l.src_resource_id=? LIMIT 25",
        (rid,),
    ).fetchall()
    for dst, type_id, name, entry in rows:
        print(f"   {dst} {type_id} entry {entry} {name!r}")
    print("   total:", db.execute(
        "SELECT COUNT(*) FROM link WHERE src_resource_id=?", (rid,)).fetchone()[0])
