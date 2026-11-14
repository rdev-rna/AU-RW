#!/usr/bin/env python3
"""Read class layouts from ACU's reflection tables in a mapped image (ACU-RE-0065).

    python -I tools/acu_reflection.py --image MAPPED.bin [--names crc-names.json] CLASS [CLASS ...]

MAPPED.bin is a captured loaded image laid out by RVA (VA = 0x140000000 + offset), e.g.
`Unity/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`.

Layout observed in .rdata (ACU.exe SHA-256 748abdc6…7e40):
- class descriptor: u32 parent class CRC-32 (or a small number for a root class), u32 class
  CRC-32, u32 size, u32 alignment;
- the u64 at descriptor - 0x20 points at the class's own field table;
- field entries are 0x38 bytes: flags in bytes 0..2, byte 3 == 2, u32 field-name CRC at +4,
  u32 type CRC at +8 (0 for scalars), u16 type code at +14 (repeated at +16), u64 at +0x30
  (non-zero for accessor fields, flag 0x1d).
Inherited fields come from the parent chain. Type-code meanings are not decoded here.
Names are CRC-32 (IEEE) of the class/field name; unresolved hashes print as `?xxxxxxxx`.
"""
from __future__ import annotations

import argparse
import json
import struct
import sys
import zlib
from pathlib import Path

BASE = 0x140000000
ENTRY = 0x38
MAX_FIELDS = 128
MAX_DEPTH = 16


class Image:
    def __init__(self, data: bytes, base: int = BASE, rdata: tuple[int, int] | None = None, names=None):
        self.b, self.base = data, base
        self.lo, self.hi = rdata or (base, base + len(data))
        self.names = names or {}

    def name(self, crc: int) -> str:
        return self.names.get(f"{crc:08x}", f"?{crc:08x}")

    def u64(self, va: int) -> int:
        off = va - self.base
        if not 0 <= off <= len(self.b) - 8:
            raise ValueError(f"address {va:#x} outside the image")
        return struct.unpack_from("<Q", self.b, off)[0]

    def descriptors(self, crc: int) -> list[int]:
        """Descriptor addresses: the class CRC followed by a plausible size and alignment."""
        pat, out = struct.pack("<I", crc), []
        i, end = self.lo - self.base, self.hi - self.base
        while (i := self.b.find(pat, i + 1, end)) > 0:
            if i + 12 > len(self.b) or i < 4:
                continue
            size, align = struct.unpack_from("<II", self.b, i + 4)
            if align in (1, 2, 4, 8, 16) and 0 < size < 0x100000:
                out.append(self.base + i - 4)
        return out

    def own_fields(self, desc: int) -> list[dict]:
        table = self.u64(desc - 0x20)
        if not self.lo <= table < self.hi:
            return []
        rows = []
        for k in range(MAX_FIELDS):
            e = table + k * ENTRY - self.base
            if e + ENTRY > len(self.b) or self.b[e + 3] != 2:
                break
            name, typ = struct.unpack_from("<II", self.b, e + 4)
            code, code2 = struct.unpack_from("<HH", self.b, e + 14)
            rows.append({"name": self.name(name), "name_crc": f"{name:08x}",
                         "type": self.name(typ) if typ else None, "type_crc": f"{typ:08x}",
                         "code": f"{code:04x}", "code2": f"{code2:04x}", "flags": self.b[e:e + 3].hex(),
                         "accessor": hex(struct.unpack_from("<Q", self.b, e + 0x30)[0])})
        return rows

    def klass(self, crc: int, depth: int = 0) -> dict:
        found = self.descriptors(crc)
        if len(found) != 1:
            return {"class": self.name(crc), "class_crc": f"{crc:08x}", "error": f"{len(found)} descriptors"}
        d = found[0]
        parent, _, size, align = struct.unpack_from("<IIII", self.b, d - self.base)
        out = {"class": self.name(crc), "class_crc": f"{crc:08x}", "descriptor": hex(d), "size": size,
               "align": align, "fields": self.own_fields(d)}
        if parent > 16:
            if depth >= MAX_DEPTH:
                raise ValueError("parent chain deeper than 16")
            out["parent"] = self.klass(parent, depth + 1)
        return out


def crc(name: str) -> int:
    return zlib.crc32(name.encode()) & 0xFFFFFFFF


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--image", type=Path, required=True)
    ap.add_argument("--names", type=Path, default=Path("local/atomgraph/crc-names.json"))
    ap.add_argument("--rdata", type=lambda s: int(s, 0), nargs=2, default=(0x143900000, 0x143A00000),
                    metavar=("LO", "HI"), help="VA range searched for descriptors")
    ap.add_argument("classes", nargs="+")
    a = ap.parse_args(argv)
    names = json.loads(a.names.read_text()) if a.names.exists() else {}
    for n in a.classes:
        names.setdefault(f"{crc(n):08x}", n)
    img = Image(a.image.read_bytes(), rdata=tuple(a.rdata), names=names)
    json.dump({n: img.klass(crc(n)) for n in a.classes}, sys.stdout, indent=1)
    print()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
