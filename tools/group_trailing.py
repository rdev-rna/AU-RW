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


def dump(entry, fid):
    out = ROOT / "local/probe" / f"gg-{fid}.bin"
    r = subprocess.run([str(EXE), str(GAME / "DataPC_ACU_Paris.forge"), "--entry", str(entry),
                        "--fid", fid, "--len", "0", "--out", str(out)], capture_output=True)
    return out.read_bytes() if r.returncode == 0 else None


def all_matrices(d):
    out = []
    for off in range(0, len(d) - 63):
        vals = [struct.unpack_from("<f", d, off + i * 4)[0] for i in range(16)]
        if abs(vals[15] - 1.0) > 1e-4:
            continue
        rows = [vals[0:4], vals[4:8], vals[8:12]]
        if all(abs(sum(x * x for x in r[0:3]) - 1.0) < 1e-3 for r in rows):
            out.append((off, vals))
    return out


def objects(d):
    starts, pos = [], 0
    pat = bytes.fromhex("298d65ec")
    while True:
        at = d.find(pat, pos)
        if at < 0:
            break
        starts.append(at - 8)
        pos = at + 1
    return starts


rows = db.execute(
    "SELECT r.file_id, r.name FROM resource r JOIN entry e ON e.id=r.entry_id "
    "WHERE r.type_id='3f742d26' AND e.idx=4884"
).fetchall()
for fid, name in rows:
    d = dump(4884, fid)
    if d is None:
        print(fid, "dump failed")
        continue
    ms = all_matrices(d)
    objs = objects(d)
    far = max(ms, key=lambda m: m[1][12] ** 2 + m[1][13] ** 2) if ms else None
    print(f"\n{name} ({fid}) {len(d)} B objects {len(objs)} matrices {len(ms)}")
    print(f"  farthest matrix @{far[0] if far else '-'} "
          f"t={tuple(round(v,1) for v in far[1][12:15]) if far else None}")
    far_objs = 0
    for index, start in enumerate(objs):
        end = objs[index + 1] if index + 1 < len(objs) else min(len(d), start + 4096)
        m = next((v for o, v in ms if start <= o < end), None)
        if m is None:
            continue
        if abs(m[12]) > 100 or abs(m[13]) > 100:
            far_objs += 1
    print(f"  objects with a far matrix: {far_objs}")
    tails = [(o, tuple(round(v, 1) for v in vals[12:15]))
             for o, vals in ms if o > (objs[-1] if objs else 0) + 4096]
    print(f"  matrices in the trailing section: {len(tails)} {tails[:4]}")
