"""Resolve entity -> LODSelector -> LOD mesh objects, then report renderable entries.

Observed chain (ACU-RE-0035 and the 2026-10-22 experiment): an Entity payload references
a LODSelector by file id; the selector's payload references several objects whose ids sit
within a small offset of its own id; those objects live in `DataPC_ACU_Paris_assets.forge`
and are type 415d9568 (clustered world meshes, decodable by `acu-mesh`).

This script probes that neighbourhood directly instead of scanning the 4.9 GB archive:
a hit means the id really is an entry in `_assets`, and the entry's own report tells us
what it is.
"""
import json
import re
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
GAME = Path(r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity")
PARIS = GAME / "DataPC_ACU_Paris.forge"
ASSETS = GAME / "DataPC_ACU_Paris_assets.forge"
EXE = ROOT / "acu-rs/target/debug/acu-resource.exe"
PROBE = ROOT / "acu-rs/target/debug/acu-probe.exe"
ENTRIES = ROOT / "local/scan-paris/entries.jsonl"


def index_of(ids):
    """file id -> (type, name, entry) for the requested ids, from the Paris census."""
    wanted = {f"{i:016x}" for i in ids}
    found = {}
    with ENTRIES.open(encoding="utf-8") as stream:
        for line in stream:
            if not any(w in line for w in wanted):
                continue
            record = json.loads(line)
            entry = record["entry"]["index"]
            for resource in record.get("resources") or []:
                if resource["file_id"] in wanted:
                    found[resource["file_id"]] = (resource["type_id"], resource["name"], entry)
    return found


def dump(entry, fid):
    result = subprocess.run([str(EXE), str(PARIS), "--entry", str(entry), "--fid", fid,
                             "--len", "0", "--out", str(ROOT / "local" / "chain_tmp.bin")],
                            capture_output=True, text=True)
    if result.returncode != 0:
        return None
    return (ROOT / "local" / "chain_tmp.bin").read_bytes()


def probe_entry(fid):
    result = subprocess.run([str(PROBE), str(ASSETS), "--fid", f"{fid:016x}"],
                            capture_output=True, text=True)
    if result.returncode != 0:
        return None
    try:
        report = json.loads(result.stdout)
    except json.JSONDecodeError:
        return None
    resources = report.get("resources") or []
    entry = report.get("entry") or {}
    mesh = next((r for r in resources if r.get("type_id") == "415d9568"), None)
    return dict(entry=entry.get("index"), file_id=entry.get("file_id"),
                resource_count=report.get("resource_count"),
                mesh_name=mesh["name"] if mesh else None,
                mesh_index=resources.index(mesh) if mesh else None,
                names=[r.get("name") for r in resources[:4]])


def main():
    entity_id = int(sys.argv[1], 16) if len(sys.argv) > 1 else 0x000000110828E9CA
    entity = index_of([entity_id])
    if not entity:
        raise SystemExit("entity not in the Paris census")
    key = f"{entity_id:016x}"
    type_id, name, entry = entity[key]
    print(f"entity {key} {name} entry {entry}")
    payload = dump(entry, key)
    if payload is None:
        raise SystemExit("entity dump failed")

    selector_ids = []
    for offset in range(0, len(payload) - 7):
        candidate = struct.unpack_from("<Q", payload, offset)[0]
        if 0x0000000F00000000 <= candidate <= 0x0000002FFFFFFFFF:
            selector_ids.append(candidate)
    known = index_of(selector_ids)
    selectors = {k: v for k, v in known.items() if v[0] == "51dc6b80"}
    print("selectors referenced:", selectors)
    if not selectors:
        raise SystemExit("no selector resolved")

    selector_key, (_, selector_name, selector_entry) = next(iter(selectors.items()))
    print(f"selector {selector_key} {selector_name} entry {selector_entry}")
    base = int(selector_key, 16)
    hits = []
    for delta in range(-8, 33):
        info = probe_entry(base + delta)
        if info and info["mesh_name"]:
            hits.append((delta, info))
            print(f"  entry {info['entry']:6d} +{delta:3d}  {info['mesh_name']}  "
                  f"({info['resource_count']} resources)")
    if not hits:
        print("no LOD object found in the neighbourhood")


if __name__ == "__main__":
    main()
