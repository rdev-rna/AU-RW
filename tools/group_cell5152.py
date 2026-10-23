import sqlite3
import struct
import subprocess
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
GAME = Path(r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity")
EXE = ROOT / "acu-rs/target/debug/acu-resource.exe"
db = sqlite3.connect(ROOT / "local/asset_index.sqlite")


def read_index(name):
    d = (ROOT / "local" / name).read_bytes()
    n = struct.unpack_from("<I", d, 8)[0]
    out = {}
    for i in range(n):
        at = 12 + i * 12
        out.setdefault(struct.unpack_from("<Q", d, at)[0], []).append(
            struct.unpack_from("<I", d, at + 8)[0])
    return out


selectors = read_index("paris-selectors.bin")
rows = db.execute(
    "SELECT r.file_id, r.name, e.idx FROM resource r JOIN entry e ON e.id=r.entry_id "
    "WHERE r.type_id='3f742d26' AND e.idx=5152 LIMIT 4"
).fetchall()
print("groups in cell 5152:", rows)

for fid, name, entry in rows:
    out = ROOT / "local/probe" / f"g-{fid}.bin"
    subprocess.run([str(EXE), str(GAME / "DataPC_ACU_Paris.forge"), "--entry", str(entry),
                    "--fid", fid, "--len", "0", "--out", str(out)], capture_output=True)
    if not out.exists():
        print(" dump failed", fid)
        continue
    d = out.read_bytes()
    starts, pos = [], 0
    pattern = bytes.fromhex("298d65ec")
    while True:
        at = d.find(pattern, pos)
        if at < 0:
            break
        starts.append(at - 8)
        pos = at + 1
    sels = 0
    for probe in range(0, len(d) - 7):
        if struct.unpack_from("<Q", d, probe)[0] in selectors:
            sels += 1
    print(f" {name}: {len(d)} B, objects {len(starts)}, selector-like windows {sels}")
