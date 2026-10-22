"""Find where an Entity payload stores its LODSelector, across several entities.

Loads the selector index (id -> entry), dumps chosen entities, and prints every offset
where a selector id appears, together with the surrounding bytes. The point is to see the
record shape (stride, tag) rather than assume a flat id list.
"""
import json
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
PARIS = Path(r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity\DataPC_ACU_Paris.forge")
EXE = ROOT / "acu-rs/target/debug/acu-resource.exe"
INDEX = ROOT / "local/paris-selectors.bin"

TARGETS = [
    (5311, 0x00000010AEA08064, "FRV_CanalKit_RiverKit_26_008"),
    (5311, 0x000000110828E9CA, "FRV_Wall_WL_09_042"),
    (5311, 0x000000110828E9C9, "GEN_ALL_WallRock_Gate_01_011"),
    (5311, 0x0000001C32F1816C, "FRV_BV_WallRiver_3M_01_537"),
]


def load_index():
    data = INDEX.read_bytes()
    assert data[:8] == b"ACUSEL01"
    count = struct.unpack_from("<I", data, 8)[0]
    out = {}
    for i in range(count):
        at = 12 + i * 12
        out[struct.unpack_from("<Q", data, at)[0]] = struct.unpack_from("<I", data, at + 8)[0]
    return out


def dump(entry, fid, target):
    result = subprocess.run([str(EXE), str(PARIS), "--entry", str(entry), "--fid", f"{fid:016x}",
                             "--len", "0", "--out", str(target)], capture_output=True, text=True)
    return result.returncode == 0


def main():
    index = load_index()
    print("selector index rows:", len(index))
    for entry, fid, name in TARGETS:
        path = ROOT / "local" / f"entity-{fid:016x}.bin"
        if not dump(entry, fid, path):
            print(f"{name}: dump failed")
            continue
        payload = path.read_bytes()
        hits = []
        for offset in range(0, len(payload) - 7):
            candidate = struct.unpack_from("<Q", payload, offset)[0]
            if candidate in index:
                hits.append((offset, candidate))
        print(f"\n{name}  {len(payload)} B  selector hits: {len(hits)}")
        for offset, candidate in hits:
            context = payload[max(0, offset - 6):offset + 12].hex()
            print(f"   +{offset:5d} (mod 4 = {offset % 4}) id {candidate:016x}  ...{context}...")
        if len(hits) >= 2:
            print("   gap:", hits[1][0] - hits[0][0])


if __name__ == "__main__":
    main()
