"""Aggregate the per-resource records already present in acu-scan's entries.jsonl.

`acu-scan` writes one JSON line per archive entry containing a `resources` array
(file_id, name, type_id, payload start/end, payload sha256). This turns that into a
resource-level index: type histogram with names, and a lookup from a named resource to
the entry that carries it.
"""
import json
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
ENTRIES = ROOT / "local/scan-paris/entries.jsonl"
CRC = json.loads((ROOT / "local/candidate-type-names.json").read_text(encoding="utf-8"))
OUT = ROOT / "local/paris-resources-index.json"
OUT_CELLS = ROOT / "local/paris-cells.json"

type_counts = Counter()
type_examples = defaultdict(list)
by_name_prefix = Counter()
cells = []
entry_count = 0
resource_count = 0
entries_with_resources = 0
first_entry_of = {}
type_payload_bytes = Counter()

with ENTRIES.open(encoding="utf-8") as stream:
    for line in stream:
        entry_count += 1
        if '"resources"' not in line:
            continue
        record = json.loads(line)
        resources = record.get("resources") or []
        if not resources:
            continue
        entries_with_resources += 1
        entry = record["entry"]
        for resource in resources:
            resource_count += 1
            type_id = resource["type_id"]
            type_counts[type_id] += 1
            type_payload_bytes[type_id] += resource.get("record_length", 0)
            if len(type_examples[type_id]) < 3:
                type_examples[type_id].append(resource["name"])
            if type_id not in first_entry_of:
                first_entry_of[type_id] = dict(
                    entry_index=entry["index"], entry_file_id=entry["file_id"],
                    entry_offset=entry["offset"], resource_file_id=resource["file_id"],
                    name=resource["name"], record_length=resource["record_length"],
                    record_offset=resource["record_offset"],
                    payload=resource["payload"],
                    payload_sha256=resource["payload_sha256"])
            name = resource["name"]
            if type_id == "ac2bbf68":
                cells.append(dict(name=name, file_id=resource["file_id"],
                                  entry_index=entry["index"], entry_file_id=entry["file_id"],
                                  record_length=resource["record_length"],
                                  record_offset=resource["record_offset"],
                                  payload=resource["payload"],
                                  payload_sha256=resource["payload_sha256"]))

named = {}
for type_id, count in type_counts.most_common():
    names = CRC.get(type_id, [])
    named[type_id] = dict(count=count, bytes=type_payload_bytes[type_id],
                          names=names[:3], examples=type_examples[type_id])

OUT.write_text(json.dumps(dict(entries=entry_count, entries_with_resources=entries_with_resources,
                               resources=resource_count, types=named), indent=1),
               encoding="utf-8")
OUT_CELLS.write_text(json.dumps(cells, indent=1), encoding="utf-8")

print(f"entries {entry_count}, with resources {entries_with_resources}, resources {resource_count}")
print(f"types {len(type_counts)}; cells {len(cells)}")
print(f"\n{'type_id':8s} {'count':>9s} {'payload MB':>11s}  name")
for type_id, count in type_counts.most_common(18):
    names = CRC.get(type_id, ["<unnamed>"])
    mb = type_payload_bytes[type_id] / 1e6
    print(f"{type_id:8s} {count:9d} {mb:11.1f}  {', '.join(names[:2])}")
print("\nfirst 5 cells:")
for cell in cells[:5]:
    print(f"  {cell['name']:24s} {cell['file_id']} entry={cell['entry_index']} "
          f"len={cell['record_length']} payload={cell['payload']}")
