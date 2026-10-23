import sqlite3
import struct
import subprocess
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
GAME = Path(r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity")
EXE = ROOT / "acu-rs/target/debug/acu-resource.exe"
DB = ROOT / "local/asset_index.sqlite"

db = sqlite3.connect(DB)
for fid in ("000000293a48fd98", "00000011d0c3339f"):
    row = db.execute(
        "SELECT e.idx, a.name, r.name FROM resource r JOIN entry e ON e.id=r.entry_id "
        "JOIN archive a ON a.id=e.archive_id WHERE r.file_id=? LIMIT 1", (fid,)
    ).fetchone()
    print(f"\n{fid}: {row}")
    if not row:
        continue
    entry, archive, name = row
    out = ROOT / "local/probe" / f"group-{fid}.bin"
    result = subprocess.run(
        [str(EXE), str(GAME / archive), "--entry", str(entry), "--fid", fid,
         "--len", "0", "--out", str(out)], capture_output=True, text=True)
    if result.returncode != 0:
        print("  dump failed:", result.stderr.strip()[:200])
        continue
    d = out.read_bytes()
    print(f"  {name}: len {len(d)} version {d[0]} type {d[9:13].hex()}")
    m = [struct.unpack_from("<f", d, 14 + i * 4)[0] for i in range(16)]
    for r in range(4):
        print("   ", " ".join(f"{m[r * 4 + c]:9.3f}" for c in range(4)))
    print("  first 64:", d[:64].hex())
