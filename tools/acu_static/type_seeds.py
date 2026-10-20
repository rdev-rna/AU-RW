"""Find exact ASCII type-name pointers in a preserved mapped image.

Discovery only: a pointer is not proof of a reflection layout or execution.
Offsets in the input must equal RVAs. No target is opened or modified.
"""
import argparse
import hashlib
import json
import struct
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", type=Path)
    parser.add_argument("names", nargs="+")
    parser.add_argument("--base", type=lambda x: int(x, 0), default=0x140000000)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    data = args.image.read_bytes()
    results = []
    for name in args.names:
        needle = name.encode("ascii") + b"\0"
        start = 0
        occurrences = []
        while (start := data.find(needle, start)) >= 0:
            address = args.base + start
            pointers = []
            cursor = 0
            while (cursor := data.find(struct.pack("<Q", address), cursor)) >= 0:
                begin, end = max(0, cursor - 24), min(len(data), cursor + 136)
                pointers.append({
                    "va": f"0x{args.base + cursor:x}",
                    "context_rva": f"0x{begin:x}",
                    "context_hex": data[begin:end].hex(),
                })
                cursor += 1
            occurrences.append({"va": f"0x{address:x}", "pointers": pointers})
            start += 1
        results.append({"name": name, "occurrences": occurrences})
    report = {
        "image": str(args.image.resolve()),
        "sha256": hashlib.sha256(data).hexdigest(),
        "image_base": f"0x{args.base:x}",
        "limitation": "Exact ASCII suffix matches and raw u64 pointers only; no verified type layout or execution.",
        "results": results,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    for result in results:
        for occurrence in result["occurrences"]:
            print(result["name"], occurrence["va"], [p["va"] for p in occurrence["pointers"]])


if __name__ == "__main__":
    main()
