"""List resources by type id in the census.

Usage: python tools/find_type.py <type-hex> [--limit N]
"""
import json
import sys
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
CENSUS = ROOT / "local/scan-paris/entries.jsonl"


def main(args):
    type_id = args[0].lower()
    limit = 40
    if "--limit" in args:
        limit = int(args[args.index("--limit") + 1])
    seen = 0
    with CENSUS.open("r", encoding="utf-8") as handle:
        for line in handle:
            if not line.strip() or f'"{type_id}"' not in line:
                continue
            record = json.loads(line)
            entry = record.get("entry_index")
            if entry is None:
                entry = (record.get("entry") or {}).get("index")
            for resource in record.get("resources", []):
                if resource.get("type_id") != type_id:
                    continue
                print(f"{resource.get('file_id')} entry {entry} "
                      f"len {resource.get('record_length')} {resource.get('name')!r}")
                seen += 1
                if seen >= limit:
                    print(f"... limit {limit}")
                    return
    print(f"rows: {seen}")


if __name__ == "__main__":
    main(sys.argv[1:])
