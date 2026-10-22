import sqlite3

db = sqlite3.connect(r"C:\Users\Administrator\AU-RW\local\asset_index.sqlite")

for name in (
    "LargeMesh_IC_NotreDameCemetary_01",
    "Largemesh_IC_ISL_STR_01",
    "Largemesh_QL_Luxembourg_Garden2",
):
    row = db.execute(
        "SELECT r.id, r.file_id, e.idx FROM resource r JOIN entry e ON e.id=r.entry_id "
        "WHERE r.name=? AND r.type_id='0984415e' LIMIT 1",
        (name,),
    ).fetchone()
    print(f"\n{name}: {row}")
    if not row:
        continue
    rid = row[0]
    meshes = db.execute(
        "SELECT dr.file_id, dr.name, de.idx FROM link l "
        "JOIN resource dr ON dr.file_id=l.dst_file_id "
        "JOIN entry de ON de.id=dr.entry_id "
        "WHERE l.src_resource_id=? AND dr.type_id='415d9568' LIMIT 6",
        (rid,),
    ).fetchall()
    for dst, mname, entry in meshes:
        print(f"   -> {dst} entry {entry} {mname!r}")
