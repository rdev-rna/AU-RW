"""Check whether the placed entities actually occupy different positions.

Reads cell 05201's children, dumps each Entity payload, and prints the selector id at
offset 100 plus the translation from the matrix at offset 14. If many entities share a
selector *and* different positions, a terrace is real; identical positions mean the
resolver is still collapsing references.
"""
import struct
import subprocess
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
PARIS = Path(r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity\DataPC_ACU_Paris.forge")
EXE = ROOT / "acu-rs/target/debug/acu-resource.exe"
OUT = ROOT / "local/map-check"
OUT.mkdir(parents=True, exist_ok=True)

CELL_ENTRY = 5311


def dump(entry, fid):
    target = OUT / f"{fid}.bin"
    if target.exists():
        return target.read_bytes()
    result = subprocess.run([str(EXE), str(PARIS), "--entry", str(entry), "--fid", fid,
                             "--len", "0", "--out", str(target)],
                            capture_output=True, text=True)
    return target.read_bytes() if result.returncode == 0 else None


def main():
    # The cell payload was written earlier by the map run; read it from the archive again.
    cell = OUT / "cell.bin"
    if not cell.exists():
        result = subprocess.run([str(EXE), str(PARIS), "--entry", str(CELL_ENTRY),
                                 "--type", "ac2bbf68", "--len", "0", "--out", str(cell)],
                                capture_output=True, text=True)
        if result.returncode != 0:
            raise SystemExit(result.stderr[:300])
    payload = cell.read_bytes()
    count = struct.unpack_from("<I", payload, 13)[0]
    children = [struct.unpack_from("<Q", payload, 17 + i * 10 + 2)[0] for i in range(count)]

    # Entity -> entry map from the census so a child id can be opened.
    import json
    entity_entry = {}
    with (ROOT / "local/scan-paris/entries.jsonl").open(encoding="utf-8") as stream:
        wanted = {f"{c:016x}" for c in children}
        for line in stream:
            if not any(w in line for w in wanted):
                continue
            record = json.loads(line)
            for resource in record.get("resources") or []:
                if resource["file_id"] in wanted and resource["type_id"] == "0984415e":
                    entity_entry[resource["file_id"]] = record["entry"]["index"]

    positions = []
    by_selector = defaultdict(list)
    for child in children:
        key = f"{child:016x}"
        entry = entity_entry.get(key)
        if entry is None:
            continue
        data = dump(entry, key)
        if not data or len(data) < 108:
            continue
        selector = struct.unpack_from("<Q", data, 100)[0]
        m = struct.unpack_from("<16f", data, 14)
        positions.append((key, selector, m[12], m[13], m[14]))
        by_selector[selector].append((m[12], m[13], m[14]))

    print(f"entities with a matrix: {len(positions)}")
    print(f"distinct selectors: {len(by_selector)}")
    for selector, points in sorted(by_selector.items(), key=lambda kv: -len(kv[1]))[:5]:
        distinct = len({tuple(round(v, 3) for v in p) for p in points})
        print(f"  selector {selector:016x}: {len(points)} entities, {distinct} distinct positions")
        for point in points[:3]:
            print(f"      ({point[0]:.2f}, {point[1]:.2f}, {point[2]:.2f})")
    uniq = {tuple(round(v, 3) for v in p[2:]) for p in positions}
    print(f"\noverall distinct positions: {len(uniq)} of {len(positions)}")


if __name__ == "__main__":
    main()
