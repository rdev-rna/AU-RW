"""Find every census row with a given resource file id, and optionally a resource name.

Read-only over `local/scan-paris/entries.jsonl`.
Usage: python tools/find_resource.py 00000010404ffc7b [...]
       python tools/find_resource.py --name FRV_BV_WoodenVatsSmall_01
"""
import json
import sys
from pathlib import Path

CENSUS = Path(r"C:\Users\Administrator\AU-RW\local\scan-paris\entries.jsonl")


def main(args):
    wanted_ids = {int(a, 16) for a in args if not a.startswith("--")}
    names = {args[i + 1] for i, a in enumerate(args) if a == "--name" and i + 1 < len(args)}
    found = 0
    with CENSUS.open("r", encoding="utf-8") as handle:
        for line in handle:
            if not line.strip():
                continue
            record = json.loads(line)
            entry = record.get("entry_index")
            if entry is None:
                entry = (record.get("entry") or {}).get("index")
            for resource in record.get("resources", []):
                fid = resource.get("file_id") or ""
                name = resource.get("name") or ""
                hit = (fid and int(fid, 16) in wanted_ids) or any(
                    name == wanted for wanted in names)
                if not hit:
                    continue
                found += 1
                print(f"{fid} {resource.get('type_id')} entry {entry} "
                      f"len {resource.get('record_length')} {name!r}")
    print(f"rows: {found}")


if __name__ == "__main__":
    main(sys.argv[1:])
