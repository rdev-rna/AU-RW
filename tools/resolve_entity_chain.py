"""Follow Entity -> LODSelector -> MeshShape through the archive.

Builds a file-id index (with the entry that carries each id) from the scan census, then
walks one entity's references and dumps the referenced selector's payload to see whether
it points at mesh shapes. Membership of an 8-byte window in the archive id set is
evidence; the record layout around a hit is read from repeat spacing, not assumed.
"""
import json
import struct
import subprocess
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
ENTRIES = ROOT / "local/scan-paris/entries.jsonl"
EXE = ROOT / "acu-rs/target/debug/acu-resource.exe"
ARCHIVE = (r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity"
           r"\DataPC_ACU_Paris.forge")

WANTED = {
    "b22b3e61": "MeshShape",
    "2d675ba2": "ReferenceListShape",
    "85c817c3": "Material",
    "51dc6b80": "LODSelector",
    "0984415e": "Entity",
    "24aecb7c": "Skeleton",
    "0fa3067f": "Animation",
    "3f742d26": "EntityGroup",
}


def build_index():
    index = {}
    with ENTRIES.open(encoding="utf-8") as stream:
        for line in stream:
            if '"resources"' not in line:
                continue
            record = json.loads(line)
            entry_index = record["entry"]["index"]
            for resource in record["resources"]:
                if resource["type_id"] in WANTED:
                    index[resource["file_id"]] = (WANTED[resource["type_id"]],
                                                  resource["name"], entry_index)
    return index


def find_ids(payload, index):
    hits = []
    for offset in range(0, len(payload) - 7):
        key = f"{struct.unpack_from('<Q', payload, offset)[0]:016x}"
        if key in index:
            hits.append((offset, key, *index[key]))
    return hits


def dump(entry, fid, out):
    result = subprocess.run([str(EXE), ARCHIVE, "--entry", str(entry), "--fid", fid,
                             "--len", "128", "--out", str(out)],
                            capture_output=True, text=True)
    if result.returncode != 0:
        return None, result.stderr.strip()[:300]
    return json.loads(result.stdout), None


def main():
    index = build_index()
    print("indexed ids:", len(index))
    entity = (ROOT / "local/entity_sample.bin").read_bytes()
    hits = find_ids(entity, index)
    print(f"\nentity references ({len(hits)}):")
    for offset, key, kind, name, entry in hits:
        print(f"  +{offset:5d} {key} {kind:18s} entry {entry:6d}  {name[:40]}")

    selectors = [h for h in hits if h[2] == "LODSelector"]
    if not selectors:
        print("\nno selector referenced")
        return
    offset, key, kind, name, entry = selectors[0]
    payload_path = ROOT / "local" / f"selector_{key}.bin"
    report, error = dump(entry, key, payload_path)
    if error:
        print(f"\ndump failed: {error}")
        return
    resource = report["resources"][0]
    print(f"\nselector {resource['name']} entry {entry} payload {resource['payload_length']}")
    print("prefix:", resource["prefix_hex"][:160])
    payload = payload_path.read_bytes()
    print("u8 version", payload[0])
    print("u64 self   %016x" % struct.unpack_from("<Q", payload, 1)[0])
    print("u32 type   %08x" % struct.unpack_from("<I", payload, 9)[0])
    selector_hits = find_ids(payload, index)
    print(f"\nselector references ({len(selector_hits)}):")
    for hit_offset, hit_key, hit_kind, hit_name, hit_entry in selector_hits[:20]:
        print(f"  +{hit_offset:5d} {hit_key} {hit_kind:18s} entry {hit_entry:6d}  {hit_name[:44]}")
    if selector_hits:
        print("\ngaps:", Counter(b[0] - a[0] for a, b in zip(selector_hits, selector_hits[1:])).most_common(6))


if __name__ == "__main__":
    main()
