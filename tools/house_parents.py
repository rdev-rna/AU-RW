import sqlite3

db = sqlite3.connect(r"C:\Users\Administrator\AU-RW\local\asset_index.sqlite")

meshes = db.execute(
    "SELECT r.id, r.file_id, r.name FROM resource r "
    "WHERE r.type_id='415d9568' AND r.name LIKE 'FRV_House%LOD0' "
    "AND r.name NOT LIKE '%GI%' LIMIT 3"
).fetchall()
for rid, fid, name in meshes:
    print(f"\nmesh {fid} {name}")
    parents = db.execute(
        "SELECT sr.file_id, sr.type_id, sr.name, se.idx FROM link l "
        "JOIN resource sr ON sr.id=l.src_resource_id "
        "JOIN entry se ON se.id=sr.entry_id "
        "WHERE l.dst_file_id=? LIMIT 8",
        (fid,),
    ).fetchall()
    for pfid, ptype, pname, pentry in parents:
        print(f"   parent {pfid} {ptype} entry {pentry} {pname!r}")
