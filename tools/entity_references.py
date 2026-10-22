"""Find which resources an Entity payload references.

Builds a file-id -> (type, name) index for the geometry-relevant types straight from
`entries.jsonl`, then scans an Entity payload for any 8-byte window that is a known id.

This is a membership test, not a layout guess: a hit means "this exact 8-byte window in
the payload is a real archive id", which is evidence; the surrounding structure is read
afterwards from offsets that repeat.
"""
import json
import struct
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
ENTRIES = ROOT / "local/scan-paris/entries.jsonl"

WANTED = {
    "b22b3e61": "MeshShape",
    "2d675ba2": "ReferenceListShape",
    "85c817c3": "Material",
    "d70e6670": "TextureSet",
    "a2b7e917": "TextureMap",
    "24aecb7c": "Skeleton",
    "51dc6b80": "LODSelector",
    "0984415e": "Entity",
    "3f742d26": "EntityGroup",
    "1cbde084": "ShadowRigidBody",
    "0fa3067f": "Animation",
}


def build_index():
    index = {}
    counts = Counter()
    with ENTRIES.open(encoding="utf-8") as stream:
        for line in stream:
            if '"resources"' not in line:
                continue
            for resource in json.loads(line)["resources"]:
                type_id = resource["type_id"]
                if type_id not in WANTED:
                    continue
                index[resource["file_id"]] = (WANTED[type_id], resource["name"])
                counts[WANTED[type_id]] += 1
    return index, counts


def main():
    payload_path = Path(sys.argv[1] if len(sys.argv) > 1 else ROOT / "local/entity_sample.bin")
    index, counts = build_index()
    print("indexed ids:", dict(counts))

    payload = payload_path.read_bytes()
    print(f"\npayload {payload_path.name}: {len(payload)} bytes")

    # Report every 8-byte window that is a known id, with its offset and alignment.
    hits = []
    for offset in range(0, len(payload) - 7):
        candidate = struct.unpack_from("<Q", payload, offset)[0]
        key = f"{candidate:016x}"
        if key in index:
            kind, name = index[key]
            hits.append((offset, key, kind, name))
    print(f"known-id windows: {len(hits)}")
    for offset, key, kind, name in hits[:40]:
        print(f"  +{offset:5d} (mod4={offset % 4}) {key} {kind:18s} {name[:44]}")
    if hits:
        gaps = Counter(b[0] - a[0] for a, b in zip(hits, hits[1:]))
        print("\nmost common gap between consecutive hits:", gaps.most_common(6))


if __name__ == "__main__":
    main()
