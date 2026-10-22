"""Dump every entity of a world cell and decode the Entity reference table.

Read-only. The cell's GridCellDataBlock children are resolved through
`local/paris-entities.bin`; each entity payload is dumped with `acu-resource` and the
reference region after the 78-byte header is summarised, so the Entity -> LODSelector
link is measured instead of scanned.

Usage: python tools/cell_entities.py <cell-entry> [--dump-hex N]
"""
import json
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
GAME = Path(r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity")
PARIS = GAME / "DataPC_ACU_Paris.forge"
EXE = ROOT / "acu-rs/target/debug/acu-resource.exe"
OUT = ROOT / "local/probe"
CELL_TYPE = "ac2bbf68"
ENTITY_TYPE = "0984415e"


def read_index(path):
    data = path.read_bytes()
    assert data[:8] == b"ACUSEL01", path
    n = struct.unpack_from("<I", data, 8)[0]
    out = {}
    for i in range(n):
        at = 12 + i * 12
        out.setdefault(struct.unpack_from("<Q", data, at)[0], []).append(
            struct.unpack_from("<I", data, at + 8)[0])
    return out


def run(*args):
    return subprocess.run([str(EXE), *map(str, args)], capture_output=True, text=True)


def cell_children(entry):
    OUT.mkdir(parents=True, exist_ok=True)
    target = OUT / f"cell-{entry}.bin"
    result = run(PARIS, "--entry", entry, "--type", CELL_TYPE, "--len", "0", "--out", target)
    if result.returncode != 0:
        raise SystemExit(f"cell dump failed: {result.stderr.strip()}")
    data = target.read_bytes()
    count = struct.unpack_from("<I", data, 13)[0]
    assert 17 + count * 10 + 12 == len(data), (count, len(data))
    return [struct.unpack_from("<Q", data, 17 + i * 10 + 2)[0] for i in range(count)]


def dump_entity(entry, fid):
    target = OUT / f"ent-{fid:016x}.bin"
    result = run(PARIS, "--entry", entry, "--fid", f"{fid:016x}", "--len", "0",
                 "--out", target)
    if result.returncode != 0:
        return None, result.stderr.strip()
    return target.read_bytes(), json.loads(result.stdout)["resources"][0]["name"]


def main(entry, hex_count):
    selectors = read_index(ROOT / "local/paris-selectors.bin")
    entities = read_index(ROOT / "local/paris-entities.bin")
    children = cell_children(entry)
    print(f"cell entry {entry}: {len(children)} children")
    stats = {"total": 0, "entity": 0, "sel100": 0, "sel122": 0, "both": 0,
             "counts": {}, "missing": 0}
    rows = []
    for child in children:
        places = entities.get(child) or []
        if not places:
            stats["missing"] += 1
            continue
        data, name = dump_entity(places[0], child)
        if data is None:
            stats["missing"] += 1
            continue
        stats["total"] += 1
        if struct.unpack_from("<I", data, 9)[0] != 0x0984415E:
            continue
        stats["entity"] += 1
        count = struct.unpack_from("<I", data, 78)[0]
        stats["counts"][count] = stats["counts"].get(count, 0) + 1
        a = struct.unpack_from("<Q", data, 100)[0]
        b = struct.unpack_from("<Q", data, 122)[0]
        if a in selectors:
            stats["sel100"] += 1
        if b in selectors:
            stats["sel122"] += 1
        if a in selectors and b in selectors:
            stats["both"] += 1
        rows.append((child, name, len(data), count, a, b, data))
    print(json.dumps(stats, indent=2))
    print(f"\n{'entity':>16} {'len':>5} {'cnt':>3}  {'@100':16} {'@122':16} name")
    for child, name, length, count, a, b, _ in rows:
        print(f"{child:016x} {length:5} {count:3}  {a:016x} {b:016x} {name}")
    if hex_count:
        for child, name, length, count, a, b, data in rows[:hex_count]:
            print(f"\n--- {child:016x} {name} ({length} B) ---")
            for base in range(78, min(len(data), 78 + 132), 16):
                chunk = data[base:base + 16]
                text = "".join(chr(c) if 32 <= c < 127 else "." for c in chunk)
                print(f"  {base:5d}  {chunk.hex(' '):<47}  {text}")


if __name__ == "__main__":
    entry = int(sys.argv[1])
    hex_count = 2 if "--dump-hex" in sys.argv else 0
    main(entry, hex_count)
