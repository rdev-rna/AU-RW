"""Check where each cell-5311 entity's LODSelector resource is resolvable.

Read-only. For every entity whose offset 100 holds a selector id, records:
  - the entity's entry and whether that entry package carries the selector resource
  - the entry the census index would pick (last write) and whether it resolves
  - whether the cell entry (5311) resolves it

Usage: python tools/check_selector_local.py <cell-entry>
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
SELECTOR_TYPE = "51dc6b80"


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


def resource(entry, fid):
    result = subprocess.run(
        [str(EXE), str(PARIS), "--entry", str(entry), "--fid", f"{fid:016x}", "--len", "0"],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        return None
    return json.loads(result.stdout)["resources"][0]


def main(cell):
    selectors = read_index(ROOT / "local/paris-selectors.bin")
    entities = read_index(ROOT / "local/paris-entities.bin")
    target = OUT / f"cell-{cell}.bin"
    if not target.exists():
        result = subprocess.run(
            [str(EXE), str(PARIS), "--entry", str(cell), "--type", CELL_TYPE,
             "--len", "0", "--out", str(target)], capture_output=True, text=True)
        if result.returncode != 0:
            raise SystemExit(result.stderr.strip())
    data = target.read_bytes()
    count = struct.unpack_from("<I", data, 13)[0]
    children = [struct.unpack_from("<Q", data, 17 + i * 10 + 2)[0] for i in range(count)]

    totals = {"entities": 0, "with_selector": 0, "local": 0, "indexed": 0,
              "cell": 0, "unresolved": 0, "not_indexed": 0}
    rows = []
    for child in children:
        places = entities.get(child) or []
        if not places:
            continue
        entry = places[0]
        payload = OUT / f"ent-{child:016x}.bin"
        if not payload.exists():
            result = subprocess.run(
                [str(EXE), str(PARIS), "--entry", str(entry), "--fid", f"{child:016x}",
                 "--len", "0", "--out", str(payload)], capture_output=True, text=True)
            if result.returncode != 0:
                continue
        body = payload.read_bytes()
        if struct.unpack_from("<I", body, 9)[0] != 0x0984415E or len(body) < 108:
            continue
        totals["entities"] += 1
        selector = struct.unpack_from("<Q", body, 100)[0]
        if selector not in selectors:
            continue
        totals["with_selector"] += 1
        if selector not in selectors:
            totals["not_indexed"] += 1
        local_ok = resource(entry, selector) is not None
        index_entries = selectors.get(selector) or []
        index_ok = any(resource(e, selector) is not None for e in index_entries[-1:])
        cell_ok = resource(cell, selector) is not None
        totals["local"] += local_ok
        totals["indexed"] += index_ok
        totals["cell"] += cell_ok
        if not (local_ok or index_ok or cell_ok):
            totals["unresolved"] += 1
        rows.append((child, selector, entry, local_ok, index_ok, cell_ok))

    print(json.dumps(totals, indent=2))
    print(f"\n{'entity':>16} {'selector':>16} {'entry':>6} local index cell")
    for child, selector, entry, local_ok, index_ok, cell_ok in rows:
        print(f"{child:016x} {selector:016x} {entry:6} "
              f"{str(local_ok):>5} {str(index_ok):>5} {str(cell_ok):>4}")


if __name__ == "__main__":
    main(int(sys.argv[1]))
