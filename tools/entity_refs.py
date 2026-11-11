"""Map an Entity payload's reference slots to the resources of its own entry.

The entity stores file ids of the components it references; which slot holds the
LODSelector is not fixed for all entity kinds (a light entity's first slot is a light).
This prints every 8-byte window that matches a resource file id of the owning entry,
with that resource's type and name, so the table shape can be read instead of guessed.

Read-only. Usage: python tools/entity_refs.py <cell-entry> <entity-hex> [...]
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

TYPES = {
    "ac2bbf68": "GridCellDataBlock",
    "0984415e": "Entity",
    "51dc6b80": "LODSelector",
    "415d9568": "WorldMesh",
    "85c817c3": "Material",
    "d70e6670": "TextureSet",
    "344780d6": "Light",
    "80320fb8": "LightCookie",
    "b22b3e61": "MeshBinding?",
}


def entry_resources(entry):
    with CENSUS.open("r", encoding="utf-8") as handle:
        for line in handle:
            if not line.strip():
                continue
            record = json.loads(line)
            index = record.get("entry_index")
            if index is None:
                index = (record.get("entry") or {}).get("index")
            if index != entry:
                continue
            return {
                resource["file_id"]: (resource.get("type_id"), resource.get("name"))
                for resource in record.get("resources", [])
            }
    return {}


def dump(entry, fid):
    OUT.mkdir(parents=True, exist_ok=True)
    target = OUT / f"ent-{fid:016x}.bin"
    result = subprocess.run(
        [str(EXE), str(PARIS), "--entry", str(entry), "--fid", f"{fid:016x}",
         "--len", "0", "--out", str(target)], capture_output=True, text=True)
    return target.read_bytes() if result.returncode == 0 else None


def main(cell, fids):
    resources = entry_resources(cell)
    print(f"entry {cell}: {len(resources)} resources")
    for fid in fids:
        data = dump(cell, fid)
        if data is None:
            print(f"\n{fid:016x}: dump failed")
            continue
        self_id = struct.unpack_from("<Q", data, 1)[0]
        print(f"\n=== {fid:016x} ({len(data)} B) self {self_id:016x} "
              f"count {struct.unpack_from('<I', data, 78)[0]}")
        seen = set()
        for offset in range(0, len(data) - 7):
            key = f"{struct.unpack_from('<Q', data, offset)[0]:016x}"
            if key not in resources or key in seen:
                continue
            seen.add(key)
            type_id, name = resources[key]
            print(f"  +{offset:4d} {key} {type_id} "
                  f"{TYPES.get(type_id, '?')} {name!r}")


if __name__ == "__main__":
    main(int(sys.argv[1]), [int(x, 16) for x in sys.argv[2:]])
