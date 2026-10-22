"""Audit a cell's children against the entity index and the census.

For every child id that the entity index cannot place, search the census for that file id
so we learn whether it is a resource in a different entry, a non-entity resource, or truly
absent. Read-only.

Usage: python tools/cell_children_audit.py <cell-entry>
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
CENSUS = ROOT / "local/scan-paris/entries.jsonl"
CELL_TYPE = "ac2bbf68"


def read_children(entry):
    target = OUT / f"cell-{entry}.bin"
    if not target.exists():
        result = subprocess.run(
            [str(EXE), str(PARIS), "--entry", str(entry), "--type", CELL_TYPE,
             "--len", "0", "--out", str(target)], capture_output=True, text=True)
        if result.returncode != 0:
            raise SystemExit(result.stderr.strip())
    data = target.read_bytes()
    count = struct.unpack_from("<I", data, 13)[0]
    return [struct.unpack_from("<Q", data, 17 + i * 10 + 2)[0] for i in range(count)]


def read_index(path):
    data = path.read_bytes()
    n = struct.unpack_from("<I", data, 8)[0]
    out = {}
    for i in range(n):
        at = 12 + i * 12
        out.setdefault(struct.unpack_from("<Q", data, at)[0], []).append(
            struct.unpack_from("<I", data, at + 8)[0])
    return out


def main(entry):
    children = read_children(entry)
    entities = read_index(ROOT / "local/paris-entities.bin")
    missing = [child for child in children if child not in entities]
    print(f"cell {entry}: {len(children)} children, "
          f"{len(children) - len(missing)} in the entity index, {len(missing)} missing")

    wanted = set(missing)
    hits = {}
    with CENSUS.open("r", encoding="utf-8") as handle:
        for line in handle:
            if not line.strip():
                continue
            record = json.loads(line)
            index = record.get("entry_index")
            if index is None:
                index = (record.get("entry") or {}).get("index")
            for resource in record.get("resources", []):
                fid = resource.get("file_id") or ""
                if not fid:
                    continue
                value = int(fid, 16)
                if value in wanted:
                    hits.setdefault(value, []).append(
                        (index, resource.get("type_id"), resource.get("name")))
    types = {}
    for child in missing:
        rows = hits.get(child)
        if not rows:
            types["absent from census"] = types.get("absent from census", 0) + 1
            continue
        for index, type_id, name in rows:
            key = type_id
            types[key] = types.get(key, 0) + 1
    print("missing children by census type:",
          ", ".join(f"{k}={v}" for k, v in sorted(types.items())))
    print("\nfirst 10 missing:")
    for child in missing[:10]:
        rows = hits.get(child)
        if not rows:
            print(f"  {child:016x} absent from census")
        else:
            for index, type_id, name in rows[:3]:
                print(f"  {child:016x} entry {index} {type_id} {name!r}")


if __name__ == "__main__":
    main(int(sys.argv[1]))
