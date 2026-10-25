import sqlite3

db = sqlite3.connect(r"C:\Users\Administrator\AU-RW\local\asset_index.sqlite")

for entity in (
    "0000003f5767a70c",  # Largemesh_BV_SectionA01_entity_LOD2_Chunk_0000
    "0000001eb1980215",  # Largemesh_BV_Asylum_01_LOD0_Chunk_0005
    "00000023404bcf7a",  # FRV_BV_AsylumGateSmall_01_004
    "0000001dad0b5e5e",  # GEN_TOR_SidewalkWood_5X5_01_Broken_034 (no selector)
):
    sel = db.execute(
        "SELECT dr.file_id, dr.name, de.idx FROM link l "
        "JOIN resource sr ON sr.id=l.src_resource_id "
        "JOIN resource dr ON dr.file_id=l.dst_file_id "
        "JOIN entry de ON de.id=dr.entry_id "
        "WHERE sr.file_id=? AND dr.type_id='51dc6b80' LIMIT 5",
        (entity,),
    ).fetchall()
    meshes = db.execute(
        "SELECT dr.file_id, dr.name, de.idx FROM link l "
        "JOIN resource sr ON sr.id=l.src_resource_id "
        "JOIN resource dr ON dr.file_id=l.dst_file_id "
        "JOIN entry de ON de.id=dr.entry_id "
        "WHERE sr.file_id=? AND dr.type_id='415d9568' LIMIT 8",
        (entity,),
    ).fetchall()
    print(f"\n{entity}")
    print("  selectors:", sel)
    print("  meshes:")
    for row in meshes:
        print("   ", row)
