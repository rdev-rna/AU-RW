"""Map ACU resource type ids to class names by CRC-32.

`docs/findings/ACU-RE-0002-runtime-framing.md` and `material.rs` establish that a
resource type id is the zlib CRC-32 of the class name (Material = 85c817c3). This
script computes that hash for every plausible identifier the binary itself contains,
so an anonymous type id in a scan report can be named.

It does not guess: a candidate is only reported when its CRC-32 equals an observed
type id. Names are candidates from the binary's string table, not proof of the
resource's contents.
"""
import json
import zlib
from pathlib import Path

STRINGS = Path(r"C:\Users\Administrator\acu-unity-re\strings_all.txt")
OUT = Path(r"C:\Users\Administrator\AU-RW\local\candidate-type-names.json")

import re

# A C++ identifier, optionally namespace-qualified. Length-bounded.
NAME = re.compile(r"^[A-Za-z_][A-Za-z0-9_]{2,63}$")
QUALIFIED = re.compile(r"^[A-Za-z_][A-Za-z0-9_:<>*& ]{2,127}$")


def main():
    names = set()
    with STRINGS.open(encoding="utf-16", errors="replace") as stream:
        for line in stream:
            text = line.rstrip("\n")
            if not text or len(text) > 128:
                continue
            # strings_all.txt is one string per line, possibly prefixed by an address.
            for token in (text, text.split(" ", 1)[-1] if " " in text else text):
                token = token.strip()
                if NAME.match(token) or (QUALIFIED.match(token) and "::" in token):
                    names.add(token)
    print(f"candidate identifiers: {len(names)}")

    by_crc = {}
    for name in names:
        crc = zlib.crc32(name.encode("utf-8")) & 0xFFFFFFFF
        by_crc.setdefault(f"{crc:08x}", []).append(name)

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(by_crc, indent=0, sort_keys=True), encoding="utf-8")
    print(f"wrote {OUT} ({len(by_crc)} distinct crc values)")

    # Show the known anchors, as a self-check of the method.
    for expected, name in (("85c817c3", "Material"), ("d70e6670", "TextureSet"),
                           ("df5d6c0e", "Mask"), ("92b95f74", "Mask16"),
                           ("7d08460d", "TextureSelector"), ("c52e2125", "UVTransform")):
        got = f"{zlib.crc32(name.encode()) & 0xFFFFFFFF:08x}"
        print(f"  {name:18s} {got}  {'OK' if got == expected else 'MISMATCH'}")


if __name__ == "__main__":
    main()
