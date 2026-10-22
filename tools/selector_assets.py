"""Resolve LODSelector record fields against the _assets entry table.

Meshes are separate entries in `DataPC_ACU_Paris_assets.forge`; their ids are entry file
ids, not resource ids. `local/paris-assets-entries.tsv` is produced by:

    acu-entries.exe <assets.forge> > local/paris-assets-entries.tsv

Read-only. Usage: python tools/selector_assets.py <selector-hex> [...]
"""
import json
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
GAME = Path(r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity")
PARIS = GAME / "DataPC_ACU_Paris.forge"
ASSETS = GAME / "DataPC_ACU_Paris_assets.forge"
EXE = ROOT / "acu-rs/target/debug/acu-resource.exe"
OUT = ROOT / "local/probe"
ENTRIES = ROOT / "local/paris-assets-entries.tsv"
HEADER = 19
STRIDE = 35
COUNT = 5


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


def read_entries():
    out = {}
    for line in ENTRIES.read_text().splitlines():
        parts = line.split("\t")
        if len(parts) >= 2:
            out[int(parts[1], 16)] = int(parts[0])
    return out


def entry_resources(entry):
    result = subprocess.run(
        [str(EXE), str(ASSETS), "--entry", str(entry), "--len", "0"],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        return f"decode error: {result.stderr.strip()[:160]}"
    report = json.loads(result.stdout)
    return "; ".join(
        f"{r['type_id']} {r['name']!r} ({r['payload_length']} B)"
        for r in report["resources"]
    )


def main(ids):
    selectors = read_index(ROOT / "local/paris-selectors.bin")
    meshes = read_index(ROOT / "local/paris-world-meshes.bin")
    entries = read_entries()
    print(f"assets entries {len(entries)}; paris mesh resources {len(meshes)}")
    for selector in ids:
        places = selectors.get(selector) or []
        print(f"\n=== selector {selector:016x} in {len(places)} entries")
        if not places:
            continue
        target = OUT / f"{selector:016x}.bin"
        if not target.exists():
            print("  payload missing; run tools/selector_lods.py first")
            continue
        data = target.read_bytes()
        for k in range(COUNT):
            at = HEADER + k * STRIDE
            rid, tag, flag_a, flag_b, ref = struct.unpack_from("<QIBBQ", data, at)
            distance = struct.unpack_from("<f", data, at + 22)[0]
            print(f"  rec{k} id {rid:016x} tag {tag:08x} flags {flag_a}/{flag_b} "
                  f"dist {distance}")
            for label, value in (("id", rid), ("ref", ref)):
                if value in entries:
                    print(f"     {label} -> ASSETS ENTRY {entries[value]}: "
                          f"{entry_resources(entries[value])}")
                elif value in meshes:
                    print(f"     {label} -> paris mesh resource at entry {meshes[value][0]}")
                elif value in selectors:
                    print(f"     {label} -> selector")
                elif value:
                    print(f"     {label} -> unresolved ({value:016x})")


if __name__ == "__main__":
    main([int(x, 16) for x in sys.argv[1:]])
