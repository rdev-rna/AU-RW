#!/usr/bin/env python3
"""Offline catalogue of NetHumanIntention message payloads (ACU-RE-0079).

Input: the same captured mapped main image as tools/net_protocol_atlas.py, plus
that tool's JSON (for the NetHumanIntention_*MSG_NetProtocol type names).

ACU builds each intention message in one large builder function (three .pdata
chunks). Each message has one block: it hashes "<prefix><Msg>_NetProtocol_ID",
compares the hash with the source intention's id, creates the message object,
stores the broadcast or unicast wire id, then copies the intention's fields into
the message's slots. A slot is {changed-flag pointer +0x08, changed byte +0x10,
value +0x18}. Copies are either inline (load source, compare with slot value,
set changed, store) or calls to small typed setters.

This tool finds each block by its broadcast wire id imm32 (CRC-32 rule of
ACU-RE-0075), follows a shared-tail jump when present, and decodes the copies
into an ordered field list: slot offset, value type, source offset in the
intention object. It fails clearly if a message is not found exactly once.
Requires capstone (already used for the earlier co-op findings).
"""
from __future__ import annotations

import argparse
import json
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import net_protocol_atlas as atlas  # noqa: E402

try:
    import capstone
    from capstone import x86
except ImportError:  # pragma: no cover - reported at runtime
    capstone = None

B = atlas.IMAGE_BASE
# ACU-RE-0079 finding-linked constants (this capture only).
OBJECT_ID_SETTER = 0x14018DE90  # form B: called after the wire id is chosen
SETTERS = {
    0x1401D7650: "u32",
    0x14012AA10: "bool",
    0x140783190: "u8",
    0x14018C150: "u64",
    0x14012A9E0: "f32",          # raw float, changed when |delta| >= 0.001
    0x1401D7670: "vec3_f32",     # three raw floats, same change rule
    0x14018C1D0: "vec3_q4000",   # three floats quantized over [-4000, 4000]
    0x140782B00: "f32_q7",       # float quantized over [-7, 7]
}
ENTITY_HELPERS = {0x14043A1A0, 0x1401518A0}  # entity pointer -> u64 handle
MAX_BLOCK_INSNS = 400
SOURCE_REG = "rbx"  # the intention object in every builder block (its +0x8 is compared with the hash)
NOT_SOURCE = {"rsp", "rbp", "rip"}

_FAM: dict[str, str] = {}
for _fam in [("rax", "eax", "ax", "al"), ("rbx", "ebx", "bx", "bl"), ("rcx", "ecx", "cx", "cl"),
             ("rdx", "edx", "dx", "dl"), ("rsi", "esi", "si", "sil"), ("rdi", "edi", "di", "dil"),
             ("rbp", "ebp", "bp", "bpl"), ("rsp", "esp", "sp", "spl")] + \
        [(f"r{i}", f"r{i}d", f"r{i}w", f"r{i}b") for i in range(8, 16)]:
    for _r in _fam:
        _FAM[_r] = _fam[0]


def norm(reg: str) -> str:
    return _FAM.get(reg, reg)


def disasm(code: bytes, va: int, limit: int = MAX_BLOCK_INSNS):
    if capstone is None:
        raise RuntimeError("capstone is required (pip install capstone)")
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
    md.detail = True
    out = []
    for ins in md.disasm(code, va):
        out.append(ins)
        if len(out) >= limit:
            break
    return md, out


def _mem(md, op):
    m = op.mem
    return (md.reg_name(m.base) if m.base else None), m.disp, op.size


INLINE_KIND = {("mov", 4): "u32", ("mov", 8): "u64", ("movzx", 1): "u8", ("movss", 4): "f32",
               ("mov", 1): "u8", ("mov", 2): "u16", ("movzx", 2): "u16"}


def parse_fields(md, insns, dst: str | None, source_reg: str = SOURCE_REG) -> list[dict]:
    """Decode slot copies until the first jmp. dst is the message object register (form A),
    or None to take it from the first setter call (form B). source_reg holds the source object."""
    pending: dict[str, tuple[str, int, str]] = {}
    stack: dict[int, tuple[str, int, str]] = {}
    lea_rcx = lea_rdx = None
    fields: list[dict] = []
    seen: set[int] = set()

    def add(slot, typ, src):
        if slot in seen or slot < 0x20:
            return
        seen.add(slot)
        if src and src[2] == "bits" and typ in ("u8", "bits"):
            typ = "bool_bit"
        elif src and src[2] == "bits":
            typ = typ
        source = None
        if src and src[0] == source_reg:
            source = f"+{src[1]:#x}"
        elif src and src[0] != "?":
            source = "derived"
        fields.append({"slot": atlas.hx(slot), "type": typ, "source": source})

    for ins in insns:
        ops = ins.operands
        mn = ins.mnemonic
        if mn == "jmp":
            break
        if mn == "call" and ops and ops[0].type == x86.X86_OP_IMM:
            tgt = ops[0].imm
            if tgt in ENTITY_HELPERS:
                if lea_rdx and lea_rdx[0] == "stack":
                    e = stack.get(lea_rdx[1])
                    pending["rax"] = (e[0], e[1], "entity") if e else ("?", 0, "entity")
                else:
                    pending["rax"] = (lea_rdx[0], lea_rdx[1], "entity") if lea_rdx else ("?", 0, "entity")
            elif tgt in SETTERS and lea_rcx:
                base, disp = lea_rcx
                if dst is None and base not in NOT_SOURCE:
                    dst = base
                if base == dst:
                    src = None
                    if lea_rdx:
                        src = stack.get(lea_rdx[1]) if lea_rdx[0] == "stack" else (lea_rdx[0], lea_rdx[1], "ptr")
                    add(disp, SETTERS[tgt], src)
            lea_rcx = lea_rdx = None
            continue
        if mn == "setne" and "__bool" in pending:
            pending[norm(md.reg_name(ops[0].reg))] = pending.pop("__bool")
            continue
        if len(ops) != 2:
            continue
        a, b = ops
        if mn in ("shr", "sar", "and", "shl", "bt") and a.type == x86.X86_OP_REG and norm(md.reg_name(a.reg)) in pending:
            r = norm(md.reg_name(a.reg))
            pending[r] = (pending[r][0], pending[r][1], "bits")
            continue
        if mn == "lea" and b.type == x86.X86_OP_MEM:
            r = norm(md.reg_name(a.reg))
            base, disp, _ = _mem(md, b)
            if r == "rcx":
                lea_rcx = (base, disp)
            elif r == "rdx":
                lea_rdx = ("stack", disp) if base in ("rsp", "rbp") else (base, disp)
            continue
        if a.type == x86.X86_OP_REG and b.type == x86.X86_OP_REG and mn == "mov":
            src = pending.get(norm(md.reg_name(b.reg)))
            if src:
                pending[norm(md.reg_name(a.reg))] = src
                if norm(md.reg_name(a.reg)) == "rdx":
                    lea_rdx = (src[0], src[1])
            continue
        if a.type == x86.X86_OP_REG and b.type == x86.X86_OP_MEM:
            base, disp, size = _mem(md, b)
            r = norm(md.reg_name(a.reg))
            if base == "rax" and disp == 0 and "rax" in pending and pending["rax"][2] == "entity":
                pending[r] = pending["rax"]
            elif base and base not in NOT_SOURCE and base != dst:
                pending[r] = (base, disp, INLINE_KIND.get((mn, size), f"{mn}{size}"))
            continue
        if mn == "cmp" and a.type == x86.X86_OP_MEM and b.type == x86.X86_OP_REG:
            base, disp, size = _mem(md, a)
            if base != dst and base not in NOT_SOURCE and size == 1:
                pending["__bool"] = (base, disp, "bool")
            continue
        if a.type == x86.X86_OP_MEM and b.type == x86.X86_OP_REG and mn in ("mov", "movss"):
            base, disp, size = _mem(md, a)
            src = pending.get(norm(md.reg_name(b.reg)))
            if base in ("rsp", "rbp") and src:
                stack[disp] = src
            elif base == dst and dst is not None and src:
                kind = src[2] if src[2] in ("bool", "entity", "bits") else INLINE_KIND.get((mn, size), src[2])
                add(disp - 0x18, kind, src)
    fields.sort(key=lambda f: int(f["slot"], 16))
    return fields


def find_anchor(text: bytes, lo: int, bid: int) -> list[tuple[int, str, str | None]]:
    """Return (va, form, dst-reg) for builder-form stores of the broadcast id."""
    pat = struct.pack("<I", bid)
    hits = []
    i = -1
    while True:
        i = text.find(pat, i + 1)
        if i < 0:
            return hits
        if i >= 1 and text[i - 1] == 0xBA:  # mov edx, imm32
            hits.append((lo + i - 1, "B", None))
        if i >= 3 and text[i - 3] == 0xC7 and (text[i - 2] & 0xF8) == 0x40 and text[i - 1] == 0x08:
            rex = text[i - 4] == 0x41 if i >= 4 else False
            reg = ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi"][text[i - 2] & 7]
            if rex:
                reg = f"r{8 + (text[i - 2] & 7)}"
            hits.append((lo + i - 3 - (1 if rex else 0), "A", reg))


def header_source(md, body, dst: str | None) -> str:
    """The register the block reads the header (+0x8) from: RBX in most builders, RDI in FightDirector's."""
    for x in body[:4]:
        if x.mnemonic == "mov" and len(x.operands) == 2 and x.operands[1].type == x86.X86_OP_MEM:
            base, disp, _ = _mem(md, x.operands[1])
            if disp == 8 and base and base != dst and base not in NOT_SOURCE:
                return norm(base)
    return SOURCE_REG


def decode_message(img: atlas.Image, name: str) -> dict:
    lo, hi = img.sec(".text")
    text = img.b[lo - B:hi - B]
    bid, uid = atlas.wire_ids(name)
    out = {"name": name, "wire_id": {"broadcast": f"{bid:08X}", "unicast": f"{uid:08X}"}}
    good = []
    for va, form, dst in find_anchor(text, lo, bid):
        md, ins = disasm(bytes(img.b[va - B:va - B + 0x1800]), va)
        if form == "A":
            nxt = ins[1]
            if nxt.mnemonic == "jmp":
                tgt = nxt.operands[0].imm
                md, ins = disasm(bytes(img.b[tgt - B:tgt - B + 0x1800]), tgt)
                body = ins
            else:
                body = ins[1:]
            # ACU-RE-0097: several FightDirector blocks load the header, compare it, then jump into one shared tail.
            k = next((j for j, x in enumerate(body[:6]) if x.mnemonic == "jmp"), None)
            if k is not None and k > 0 and body[k].operands[0].type == x86.X86_OP_IMM:
                tgt = body[k].operands[0].imm
                _, tail = disasm(bytes(img.b[tgt - B:tgt - B + 0x1800]), tgt)
                body = body[:k] + tail
            good.append((va, form, parse_fields(md, body, dst, header_source(md, body, dst))))
        else:
            for k, x in enumerate(ins[:60]):
                if x.mnemonic == "call" and x.operands[0].type == x86.X86_OP_IMM and x.operands[0].imm == OBJECT_ID_SETTER:
                    good.append((va, form, parse_fields(md, ins[k + 1:], None)))
                    break
    if len(good) != 1:
        raise ValueError(f"{name}: expected one builder block, found {len(good)}")
    va, form, fields = good[0]
    out.update(block=atlas.hx(va), form=form, fields=fields)
    return out


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--image", type=Path, required=True)
    ap.add_argument("--atlas", type=Path, required=True, help="output of net_protocol_atlas.py")
    ap.add_argument("--expect-sha256")
    ap.add_argument("--output", type=Path, required=True)
    ap.add_argument("--family", action="append",
                    help="type-name prefix to decode, repeatable (default NetHumanIntention_). ACU-RE-0093 showed the "
                         "same builder form covers every *MSG_NetProtocol family.")
    ap.add_argument("--all", action="store_true", help="decode every *MSG_NetProtocol type record (ACU-RE-0093)")
    ap.add_argument("--systems", type=Path, help="optional coverage_systems.json; adds each message's system")
    a = ap.parse_args(argv)
    if capstone is None:
        print("error: capstone is required", file=sys.stderr)
        return 2
    digest = atlas.sha256(a.image)
    if a.expect_sha256 and digest != a.expect_sha256.lower():
        print(f"error: image sha256 {digest} != expected", file=sys.stderr)
        return 2
    img = atlas.Image(a.image.read_bytes())
    recs = json.loads(a.atlas.read_text(encoding="utf-8"))["type_records"]
    prefixes = tuple(a.family or ["NetHumanIntention_"])
    names = sorted(r["name"] for r in recs if r["name"].endswith("MSG_NetProtocol")
                   and (a.all or r["name"].startswith(prefixes)))
    msgs = [decode_message(img, n) for n in names]
    if a.systems:
        sysmap = {n: s for s, v in json.loads(a.systems.read_text(encoding="utf-8"))["systems"].items()
                  for n in v["messages"]}
        for m in msgs:
            m["system"] = sysmap.get(m["name"])
    types: dict[str, int] = {}
    for m in msgs:
        for f in m["fields"]:
            types[f["type"]] = types.get(f["type"], 0) + 1
    out = {
        "schema": 1,
        "finding": "ACU-RE-0093" if (a.all or a.family) else "ACU-RE-0079",
        "selection": "all" if a.all else list(prefixes),
        "image": {"sha256": digest, "image_base": atlas.hx(B)},
        "tool_sha256": atlas.sha256(Path(__file__)),
        "constants": {"object_id_setter": atlas.hx(OBJECT_ID_SETTER),
                      "setters": {atlas.hx(k): v for k, v in SETTERS.items()},
                      "entity_helpers": [atlas.hx(x) for x in sorted(ENTITY_HELPERS)]},
        "summary": {"messages": len(msgs), "fields": sum(len(m["fields"]) for m in msgs),
                    "forms": {f: sum(1 for m in msgs if m["form"] == f) for f in ("A", "B")},
                    "field_types": dict(sorted(types.items())),
                    "fields_without_source": sum(1 for m in msgs for f in m["fields"] if f["source"] is None),
                    "messages_without_fields": [m["name"] for m in msgs if not m["fields"]]},
        "messages": msgs,
    }
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(out, indent=1) + "\n", encoding="utf-8")
    print(json.dumps(out["summary"], indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
