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


def dump(entry, fid, tag):
    out = ROOT / "local/probe" / f"{tag}-{fid}.bin"
    r = subprocess.run([str(EXE), str(GAME / "DataPC_ACU_Paris.forge"), "--entry", str(entry),
                        "--fid", fid, "--len", "0", "--out", str(out)], capture_output=True)
    return out.read_bytes() if r.returncode == 0 else None


def matrices(d):
    out = []
    for off in range(0, len(d) - 63):
        vals = [struct.unpack_from("<f", d, off + i * 4)[0] for i in range(16)]
        if abs(vals[15] - 1.0) > 1e-4:
            continue
        rows = [vals[0:4], vals[4:8], vals[8:12]]
        if all(abs(sum(x * x for x in r[0:3]) - 1.0) < 1e-3 for r in rows):
            out.append((off, vals))
    return out


print("=== entities of cell 4884: matrix translations")
for fid, in db.execute(
    "SELECT r.file_id FROM resource r JOIN entry e ON e.id=r.entry_id "
    "WHERE r.type_id='0984415e' AND e.idx=4884"
).fetchall():
    d = dump(4884, fid, "e")
    if d is None:
        print("  ", fid, "dump failed")
        continue
    m = [struct.unpack_from("<f", d, 14 + i * 4)[0] for i in range(16)]
    print(f"   {fid} t=({m[12]:9.1f},{m[13]:9.1f},{m[14]:6.1f}) len {len(d)}")

print("\n=== group FRV_House_Corner_F_0_03x13x2_01_002 matrices")
d = dump(4884, "000000293a48fd98", "g")
ms = matrices(d)
for off, vals in ms:
    print(f"   @{off:6d} t=({vals[12]:9.1f},{vals[13]:9.1f},{vals[14]:6.1f})")

print("\n=== rule comparison")
if ms:
    far = max(ms, key=lambda m: m[1][12] ** 2 + m[1][13] ** 2)
    print(f"   base candidate @{far[0]} t=({far[1][12]:.1f},{far[1][13]:.1f},{far[1][14]:.1f})")
    for off, vals in ms[:8]:
        rel = (far[1][12] + vals[12], far[1][13] + vals[13], far[1][14] + vals[14])
        print(f"   @{off:6d} raw=({vals[12]:8.1f},{vals[13]:8.1f},{vals[14]:5.1f}) "
              f"base+raw=({rel[0]:8.1f},{rel[1]:8.1f},{rel[2]:5.1f})")
