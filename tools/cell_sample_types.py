"""Sample cells of each size and resolve their children through the asset index."""
import json
import sqlite3
import struct
import subprocess
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
GAME = Path(r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity")
PARIS = GAME / "DataPC_ACU_Paris.forge"
EXE = ROOT / "acu-rs/target/debug/acu-resource.exe"
DB = ROOT / "local/asset_index.sqlite"
OUT = ROOT / "local/probe"

rows = json.loads((ROOT / "local/paris-cells.json").read_text(encoding="utf-8"))


def sample(predicate, count):
    out = []
    for row in rows:
        if predicate(row):
            out.append(row)
            if len(out) >= count:
                break
    return out


def children(cell):
    target = OUT / f"cell-{cell['entry_index']}.bin"
    if not target.exists():
        subprocess.run([str(EXE), str(PARIS), "--entry", str(cell["entry_index"]),
                        "--type", "ac2bbf68", "--len", "0", "--out", str(target)],
                       capture_output=True)
    data = target.read_bytes()
    count = struct.unpack_from("<I", data, 13)[0]
    return [struct.unpack_from("<Q", data, 17 + i * 10 + 2)[0] for i in range(count)]


db = sqlite3.connect(DB)
for label, predicate in (
    ("71 B", lambda r: r["record_length"] <= 80),
    ("~300 B", lambda r: 200 <= r["record_length"] <= 400),
    ("~800 B", lambda r: 700 <= r["record_length"] <= 900),
    (">1000 B", lambda r: r["record_length"] > 1000),
):
    for cell in sample(predicate, 2):
        kids = children(cell)
        print(f"\n{label} {cell['name']} entry {cell['entry_index']} "
              f"len {cell['record_length']} children {len(kids)}")
        for kid in kids[:6]:
            row = db.execute(
                "SELECT e.archive_id, e.idx, r.type_id, r.name FROM resource r JOIN entry e ON e.id=r.entry_id WHERE r.file_id=?",
                (f"{kid:016x}",)).fetchone()
            print(f"   {kid:016x} {row}")
