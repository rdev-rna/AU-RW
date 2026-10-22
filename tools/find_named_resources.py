"""Stream the Paris census and print resources whose name starts with a prefix.

Read-only over `local/scan-paris/entries.jsonl`. Usage:
    python tools/find_named_resources.py FRV_Street_Row_BievreRiver_26 [...]
"""
import json
import sys
from pathlib import Path

CENSUS = Path(r"C:\Users\Administrator\AU-RW\local\scan-paris\entries.jsonl")


def main(prefixes):
    wanted = {p: [] for p in prefixes}
    with CENSUS.open("r", encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            record = json.loads(line)
            entry = record.get("entry_index")
            if entry is None:
                entry = (record.get("entry") or {}).get("index")
            archive = record.get("archive")
            for resource in record.get("resources", []):
                name = resource.get("name") or ""
                for prefix in prefixes:
                    if name.startswith(prefix):
                        wanted[prefix].append({
                            "name": name,
                            "file_id": resource.get("file_id"),
                            "type_id": resource.get("type_id"),
                            "entry": entry,
                            "archive": archive,
                            "length": resource.get("record_length"),
                        })
    for prefix, rows in wanted.items():
        print(f"\n== {prefix}: {len(rows)} resources")
        for row in sorted(rows, key=lambda r: r["name"]):
            print(f"  {row['file_id']} {row['type_id']} entry {row['entry']} "
                  f"len {row['length']} {row['name']} [{row['archive']}]")


if __name__ == "__main__":
    main(sys.argv[1:])
