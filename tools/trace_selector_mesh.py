"""Trace selector -> mesh for several distinct selectors, to find the collapse.

For each selector id: locate its entry via the selector index, dump the payload, list every
world-mesh id it contains (via the mesh index and the _assets entry table), and print the
mesh resource names. If different selectors yield the same mesh, the reference scan is
wrong; if they yield different meshes, the viewer's matching is.
"""
import json
import struct
import subprocess
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
GAME = Path(r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity")
PARIS = GAME / "DataPC_ACU_Paris.forge"
ASSETS = GAME / "DataPC_ACU_Paris_assets.forge"
EXE = ROOT / "acu-rs/target/debug/acu-resource.exe"
OUT = ROOT / "local/selector-trace"
OUT.mkdir(parents=True, exist_ok=True)

SELECTORS = [
    0x00000010422CC115,
    0x0000001086CA995D,
    0x00000019BD07A3D7,
    0x00000015470AA6D7,
]


def read_index(path):
    data = path.read_bytes()
    assert data[:8] == b"ACUSEL01", path
    count = struct.unpack_from("<I", data, 8)[0]
    out = {}
    for i in range(count):
        at = 12 + i * 12
        out.setdefault(struct.unpack_from("<Q", data, at)[0], []).append(
            struct.unpack_from("<I", data, at + 8)[0])
    return out


def dump(archive, entry, fid, tag):
    target = OUT / f"{tag}.bin"
    result = subprocess.run([str(EXE), str(archive), "--entry", str(entry), "--fid", f"{fid:016x}",
                             "--len", "0", "--out", str(target)], capture_output=True, text=True)
    return target.read_bytes() if result.returncode == 0 else None


def main():
    selectors = read_index(ROOT / "local/paris-selectors.bin")
    meshes = read_index(ROOT / "local/paris-world-meshes.bin")
    print(f"selector ids {len(selectors)}, mesh ids {len(meshes)}")

    for selector in SELECTORS:
        entries = selectors.get(selector) or []
        if not entries:
            print(f"\n{selector:016x}: not in the selector index")
            continue
        data = dump(PARIS, entries[0], selector, f"sel-{selector:016x}")
        if data is None:
            print(f"\n{selector:016x}: dump failed at entry {entries[0]}")
            continue
        hits = []
        for offset in range(0, len(data) - 7):
            candidate = struct.unpack_from("<Q", data, offset)[0]
            if candidate in meshes:
                hits.append((offset, candidate, meshes[candidate][0]))
        print(f"\n{selector:016x} entry {entries[0]} payload {len(data)}  mesh-id hits: {len(hits)}")
        seen = set()
        for offset, mesh_id, mesh_entry in hits[:6]:
            if mesh_entry in seen:
                continue
            seen.add(mesh_entry)
            mesh_data = dump(PARIS, mesh_entry, mesh_id, f"mesh-{mesh_id:016x}")
            info = ""
            if mesh_data is not None:
                info = f"payload {len(mesh_data)}"
            print(f"   +{offset:5d} mesh {mesh_id:016x} -> entry {mesh_entry} {info}")


if __name__ == "__main__":
    main()
