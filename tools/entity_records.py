"""Decode the Entity payload's reference records (Entity -> LODSelector link).

Read-only. Dumps entities through `acu-resource` and prints the header, the count field
and every 8-byte value that resolves to a selector id, with record context.

Usage: python tools/entity_records.py <cell-entry> <entity-hex> [...]
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


def dump(entry, fid):
    OUT.mkdir(parents=True, exist_ok=True)
    target = OUT / f"ent-{fid:016x}.bin"
    result = subprocess.run(
        [str(EXE), str(PARIS), "--entry", str(entry), "--fid", f"{fid:016x}",
         "--len", "0", "--out", str(target)],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        return None, None
    return target.read_bytes(), json.loads(result.stdout)["resources"][0]


def main(cell_entry, fids):
    selectors = read_index(ROOT / "local/paris-selectors.bin")
    for fid in fids:
        data, row = dump(cell_entry, fid)
        if data is None:
            print(f"{fid:016x}: dump failed")
            continue
        version = data[0]
        self_id = struct.unpack_from("<Q", data, 1)[0]
        type_id = struct.unpack_from("<I", data, 9)[0]
        flag = data[13]
        count = struct.unpack_from("<I", data, 78)[0]
        print(f"\n=== {fid:016x} {row['name']!r} {len(data)} B type {row['type_id']}")
        print(f"  version {version} self {self_id:016x} type {type_id:08x} flag {flag} "
              f"count@78 {count}")
        print(f"  bytes 10..14 {data[10:14].hex()}  78..100 {data[78:100].hex()}")
        hits = []
        for offset in range(0, len(data) - 7):
            value = struct.unpack_from("<Q", data, offset)[0]
            if value in selectors:
                hits.append((offset, value))
        print(f"  selector hits {len(hits)}: " +
              ", ".join(f"+{o}:{v:016x}" for o, v in hits))
        if len(hits) >= 2:
            print(f"  hit offsets {[o for o, _ in hits]}")
            print(f"  deltas {[b[0] - a[0] for a, b in zip(hits, hits[1:])]}")
        for offset, value in hits:
            lo = max(0, offset - 12)
            print(f"    +{offset:4d} {value:016x}  ctx[{lo}:{offset + 16}] "
                  f"{data[lo:offset + 16].hex()}")
        # First record region after the count field.
        if len(data) > 100:
            print(f"  region 78..170: {data[78:170].hex()}")


if __name__ == "__main__":
    main(int(sys.argv[1]), [int(x, 16) for x in sys.argv[2:]])
