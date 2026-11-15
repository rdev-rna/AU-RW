#!/usr/bin/env python3
"""Offline atlas of ACU's NetProtocol message registry (ACU-RE-0072).

Input: a captured mapped main image (post-static-init memory layout, image base
0x140000000). Output: deterministic JSON with three sections:

  type_records  - lazily built net type records found in .data: name pointer,
                  ancestor count, ancestor CRC-32s resolved to names. A record is
                  accepted only if its last ancestor hash equals CRC-32(own name).
  static_rfcs   - the static-init registrar list (head array REGISTRAR_LISTS,
                  category 1): each node's register/unregister functions, and the
                  "zenitar_*_NetProtocol_ID" message whose broadcast or _UNICAST
                  CRC-32 appears as an imm32 inside the registrar (.pdata bounds).
  object_ids    - object-scoped "<...>MSG_NetProtocol_ID[_UNICAST]" and
                  "_<Owner>_NetProtocol_ID" literals, with the functions that load
                  them by RIP-relative LEA. At runtime the game prefixes these with
                  a per-object string (vf0); the prefix is not resolved here.

Finding-linked constants are listed below with their provenance. Everything
here is static: strings and pointers in one capture. See the finding for which
roles were confirmed in IDA and which are inferred.
"""
from __future__ import annotations

import argparse
import bisect
import collections
import hashlib
import json
import re
import struct
import sys
import zlib
from pathlib import Path

IMAGE_BASE = 0x140000000
MAX_IMAGE = 512 * 1024 * 1024
# ACU-RE-0072: sub_1427488D0 links {register, unregister, next} nodes into
# off_14525FAE8[category]; NetProtocol registrars use category 1 (sub_142D80170).
REGISTRAR_LISTS = 0x14525FAE8
REGISTRAR_CATEGORY = 1
MAX_LIST = 100_000
MAX_ANCESTORS = 32

STR_RE = re.compile(rb"[\x20-\x7e]{3,200}\x00")
LEA_RIP_RE = re.compile(rb"[\x48\x4C]\x8D[\x05\x0D\x15\x1D\x25\x2D\x35\x3D](....)", re.S)
MOV_STACK_IMM_RE = re.compile(rb"\xC7\x44\x24.(....)|\xC7\x84\x24....(....)", re.S)
MOV_REG_IMM_RE = re.compile(rb"(?<![\x48\x49])\x41?[\xB8-\xBF](....)", re.S)


class Image:
    def __init__(self, data: bytes):
        self.b = data
        if data[:2] != b"MZ":
            raise ValueError("not a PE image (no MZ)")
        pe = struct.unpack_from("<I", data, 0x3C)[0]
        if pe + 0x18 > len(data) or data[pe:pe + 4] != b"PE\0\0":
            raise ValueError("bad PE header")
        nsec = struct.unpack_from("<H", data, pe + 6)[0]
        optsz = struct.unpack_from("<H", data, pe + 20)[0]
        opt = pe + 24
        if struct.unpack_from("<H", data, opt)[0] != 0x20B:
            raise ValueError("not PE32+")
        self.secs = {}
        for i in range(nsec):
            o = opt + optsz + 40 * i
            name = data[o:o + 8].rstrip(b"\0").decode("ascii", "replace")
            vsz, rva = struct.unpack_from("<II", data, o + 8)
            self.secs[name] = (rva, vsz)
        # Exception directory (index 3) -> .pdata RUNTIME_FUNCTION table.
        exc_rva, exc_sz = struct.unpack_from("<II", data, opt + 112 + 3 * 8)
        self.funcs: list[tuple[int, int]] = []
        for o in range(exc_rva, min(exc_rva + exc_sz, len(data)) - 11, 12):
            beg, end, _ = struct.unpack_from("<III", data, o)
            if beg and end > beg:
                self.funcs.append((IMAGE_BASE + beg, IMAGE_BASE + end))
        self.funcs.sort()
        self.starts = [f[0] for f in self.funcs]

    def sec(self, name: str) -> tuple[int, int]:
        rva, vsz = self.secs[name]
        return IMAGE_BASE + rva, IMAGE_BASE + rva + vsz

    def q(self, va: int) -> int:
        o = va - IMAGE_BASE
        if not 0 <= o <= len(self.b) - 8:
            raise IndexError(hex(va))
        return struct.unpack_from("<Q", self.b, o)[0]

    def d(self, va: int) -> int:
        return struct.unpack_from("<I", self.b, va - IMAGE_BASE)[0]

    def cstr(self, va: int, limit: int = 200) -> str | None:
        o = va - IMAGE_BASE
        if not 0 <= o < len(self.b):
            return None
        end = self.b.find(b"\0", o, o + limit)
        if end <= o:
            return None
        s = self.b[o:end]
        return s.decode("ascii") if all(0x20 <= c < 0x7F for c in s) else None

    def func_of(self, va: int) -> tuple[int, int] | None:
        i = bisect.bisect_right(self.starts, va) - 1
        if i >= 0 and self.funcs[i][0] <= va < self.funcs[i][1]:
            return self.funcs[i]
        return None

    def in_image(self, va: int) -> bool:
        return IMAGE_BASE <= va < IMAGE_BASE + len(self.b)


def crc(s: str) -> int:
    return zlib.crc32(s.encode("ascii"))


def hx(v: int) -> str:
    return f"0x{v:X}"


def strings(img: Image) -> dict[int, str]:
    lo, hi = img.sec(".rdata")
    out = {}
    for m in STR_RE.finditer(img.b, lo - IMAGE_BASE, hi - IMAGE_BASE):
        out[IMAGE_BASE + m.start()] = m.group()[:-1].decode("ascii")
    return out


def type_records(img: Image, strs: dict[int, str], names_by_crc: dict[int, str]) -> list[dict]:
    lo, hi = img.sec(".data")
    recs = []
    b = img.b
    for o in range((lo - IMAGE_BASE + 7) & ~7, hi - IMAGE_BASE - 0x20, 8):
        p = struct.unpack_from("<Q", b, o)[0]
        name = strs.get(p)
        if name is None:
            continue
        cnt = struct.unpack_from("<I", b, o + 8)[0]
        if not 1 <= cnt <= MAX_ANCESTORS or o + 0x10 + 16 * cnt > len(b):
            continue
        hashes = [struct.unpack_from("<I", b, o + 0x10 + 16 * i)[0] for i in range(cnt)]
        if hashes[-1] != crc(name):
            continue
        recs.append({
            "record_va": hx(IMAGE_BASE + o),
            "name": name,
            "ancestors": [names_by_crc.get(h, f"crc:{h:08X}") for h in hashes],
        })
    return recs


ID_BASES = ("NetObjectRemoteFunctionCall", "NetObjectMemento")


def wire_ids(type_name: str) -> tuple[int, int]:
    """ACU-RE-0075: wire ID = CRC-32("zenitar_" + type name + "_ID"), unicast adds "_UNICAST"."""
    s = f"zenitar_{type_name}_ID"
    return crc(s), crc(s + "_UNICAST")


def add_wire_ids(img: Image, recs: list[dict]) -> dict:
    """Attach computed IDs to RFC and memento records and check each appears as a .text constant."""
    lo, hi = img.sec(".text")
    text = bytes(img.b[lo - IMAGE_BASE:hi - IMAGE_BASE])
    stats = collections.Counter()
    for r in recs:
        base = next((b for b in ID_BASES if b in r["ancestors"][:-1]), None)
        if base is None or r["name"].startswith("FactoryEntry"):
            continue
        bc, uc = wire_ids(r["name"])
        seen_b = text.find(struct.pack("<I", bc)) >= 0
        seen_u = text.find(struct.pack("<I", uc)) >= 0
        r["wire_id"] = {"broadcast": f"{bc:08X}", "unicast": f"{uc:08X}",
                        "broadcast_in_text": seen_b, "unicast_in_text": seen_u}
        stats[f"{base}:total"] += 1
        stats[f"{base}:broadcast_in_text"] += seen_b
        stats[f"{base}:both_in_text"] += seen_b and seen_u
    return dict(sorted(stats.items()))


def static_rfcs(img: Image, zen_by_crc: dict[int, tuple[str, str]]) -> list[dict]:
    head = img.q(REGISTRAR_LISTS + 8 * REGISTRAR_CATEGORY)
    nodes, seen = [], set()
    while head:
        if head in seen or len(nodes) >= MAX_LIST or not img.in_image(head):
            raise ValueError(f"registrar list broken or cyclic at {hx(head)}")
        seen.add(head)
        nodes.append((head, img.q(head), img.q(head + 8)))
        head = img.q(head + 16)
    out = []
    for node, reg, unreg in nodes:
        f = img.func_of(reg)
        if f is None:
            raise ValueError(f"registrar {hx(reg)} has no .pdata entry")
        code = img.b[f[0] - IMAGE_BASE:f[1] - IMAGE_BASE]
        keys = {}
        for rx in (MOV_STACK_IMM_RE, MOV_REG_IMM_RE):
            for m in rx.finditer(code):
                g = next(x for x in m.groups() if x)
                v = struct.unpack("<I", g)[0]
                if v in zen_by_crc:
                    keys[hx(v)] = zen_by_crc[v]
        leas = []
        for m in LEA_RIP_RE.finditer(code):
            tgt = f[0] + m.end() + struct.unpack("<i", m.group(1))[0]
            tf = img.func_of(tgt)
            if tf and tf[0] == tgt:
                leas.append(hx(tgt))
        msgs = sorted({v[0] for v in keys.values()})
        out.append({
            "node_va": hx(node),
            "register": hx(reg),
            "unregister": hx(unreg),
            "message": msgs[0] if len(msgs) == 1 else msgs,
            "keys": {k: v[1] for k, v in sorted(keys.items())},
            "callbacks": leas,
        })
    return out


def object_ids(img: Image, strs: dict[int, str]) -> dict:
    targets = {va: s for va, s in strs.items()
               if (s.endswith("_NetProtocol_ID") or s.endswith("_NetProtocol_ID_UNICAST"))
               and not s.startswith("zenitar_")}
    lo, hi = img.sec(".text")
    refs = collections.defaultdict(set)
    for m in LEA_RIP_RE.finditer(img.b, lo - IMAGE_BASE, hi - IMAGE_BASE):
        tgt = IMAGE_BASE + m.end() + struct.unpack("<i", m.group(1))[0]
        if tgt in targets:
            f = img.func_of(IMAGE_BASE + m.start())
            refs[tgt].add(f[0] if f else IMAGE_BASE + m.start())
    msgs, channels = [], []
    for va in sorted(targets):
        s = targets[va]
        e = {"va": hx(va), "s": s, "functions": [hx(x) for x in sorted(refs.get(va, ()))]}
        (channels if s.startswith("_") else msgs).append(e)
    return {
        "message_literals": len(msgs),
        "message_literals_referenced": sum(1 for e in msgs if e["functions"]),
        "channel_literals": len(channels),
        "messages": msgs,
        "channels": channels,
    }


def sha256(p: Path) -> str:
    h = hashlib.sha256()
    with p.open("rb") as f:
        for c in iter(lambda: f.read(1 << 20), b""):
            h.update(c)
    return h.hexdigest()


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--image", type=Path, required=True)
    ap.add_argument("--expect-sha256")
    ap.add_argument("--output", type=Path, required=True)
    a = ap.parse_args(argv)
    if a.image.stat().st_size > MAX_IMAGE:
        print("error: image too large", file=sys.stderr)
        return 2
    digest = sha256(a.image)
    if a.expect_sha256 and digest != a.expect_sha256.lower():
        print(f"error: image sha256 {digest} != expected", file=sys.stderr)
        return 2
    img = Image(a.image.read_bytes())
    strs = strings(img)
    names_by_crc: dict[int, str] = {}
    for s in sorted(set(strs.values())):
        names_by_crc.setdefault(crc(s), s)
    zen_by_crc: dict[int, tuple[str, str]] = {}
    for s in sorted({s for s in strs.values() if s.startswith("zenitar_") and s.endswith("_NetProtocol_ID")}):
        zen_by_crc[crc(s)] = (s, "broadcast")
        zen_by_crc[crc(s + "_UNICAST")] = (s, "unicast")

    recs = type_records(img, strs, names_by_crc)
    id_stats = add_wire_ids(img, recs)
    rfcs = static_rfcs(img, zen_by_crc)
    objs = object_ids(img, strs)
    roots = collections.Counter(r["ancestors"][0] for r in recs)
    bases = collections.Counter(r["ancestors"][-2] for r in recs if len(r["ancestors"]) > 1)
    matched = {m for r in rfcs for m in ([r["message"]] if isinstance(r["message"], str) else r["message"])}
    zen_all = sorted({v[0] for v in zen_by_crc.values()})
    out = {
        "schema": 1,
        "finding": "ACU-RE-0072",
        "image": {"sha256": digest, "image_base": hx(IMAGE_BASE)},
        "tool_sha256": sha256(Path(__file__)),
        "constants": {"registrar_lists": hx(REGISTRAR_LISTS), "registrar_category": REGISTRAR_CATEGORY},
        "summary": {
            "type_records": len(recs),
            "type_record_roots": roots.most_common(),
            "type_record_direct_bases": bases.most_common(25),
            "static_registrars": len(rfcs),
            "static_registrars_with_one_message": sum(1 for r in rfcs if isinstance(r["message"], str)),
            "zenitar_id_literals": len(zen_all),
            "zenitar_without_registrar": [s for s in zen_all if s not in matched],
            "object_message_literals": objs["message_literals"],
            "object_message_literals_referenced": objs["message_literals_referenced"],
            "object_channel_literals": objs["channel_literals"],
            "wire_ids": id_stats,
        },
        "static_rfcs": rfcs,
        "type_records": recs,
        "object_ids": objs,
    }
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(out, indent=1) + "\n", encoding="utf-8")
    print(json.dumps(out["summary"], indent=1)[:3000])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
