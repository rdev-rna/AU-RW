#!/usr/bin/env python3
"""Decode the field layout of every ACU memento (NetObjectMemento state type) from its write delegate.

Input: the captured mapped main image used by tools/net_protocol_atlas.py, and the atlas JSON it wrote
(ACU-RE-0072). Output: JSON with, per memento, its codec binding (read and write delegates) and the slots its
write function fills: slot offset, value offset inside the slot, type, and the owner field it comes from.

Method (ACU-RE-0095):
- Binding: every memento's broadcast wire ID is stored as an immediate in a bind function that also loads the
  read delegate (`lea rdx, [rip+read]`, before the ID) and the write delegate (`lea r8, [rip+write]`, after it,
  before the registry's virtual call). ACU-RE-0084 described this for HumanAvatar.
- Write delegate (owner in RCX, memento in RDX): each slot is marked changed with the triple
  `mov rax, [m+K]; mov byte [m+K+8], 1; mov byte [rax], 1`, so K is the slot. The value store `[m+V]` next to it
  gives the value offset V-K and its type; the register stored gives the owner field. Other slots are filled by
  setter calls (`lea rcx, [m+K]` plus the value in RDX), and some writers call a nested writer for an embedded
  struct; those are decoded recursively.
Read only. Needs capstone.
"""
from __future__ import annotations

import argparse
import collections
import json
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import net_protocol_atlas as atlas  # noqa: E402

try:
    import capstone
    from capstone import x86
except ImportError:  # pragma: no cover
    capstone = None

B = atlas.IMAGE_BASE
# Setters seen in memento writers (this capture only). Message-builder setters (ACU-RE-0079) are reused.
SETTERS = {
    0x1401D7650: "u32",
    0x14012AA10: "bool",
    0x140783190: "u8",
    0x14018C150: "u64",
    0x14012A9E0: "f32",
    0x1401D7670: "vec3_f32",
    0x14018C1D0: "vec3_q4000",
    0x140782B00: "f32_q7",
    0x14018BF70: "f32_q2",       # HumanAvatar +0x30, quantized over [-2, 2] (ACU-RE-0075)
    0x14018BE90: "f32_q1",       # quantized over [-1, 1] (ACU-RE-0075)
    0x141BB38D0: "vslot_u32",    # virtual slot: get +0x38, compare, set +0x50; value in EDX
    0x1426D07D0: "array",        # (slot, pointer RDX, count R8D)
    0x141C70970: "quat",         # normalises a quaternion, then quantizes it
}
THUNKS = {0x1401B78D0}
# Mementos registered under a wire ID that is not CRC-32("zenitar_" + type + "_ID") (ACU-RE-0084 observation 1).
ALT_WIRE = {"BhvGenericNPC_NetProtocol": 0xA837A90F, "CharacterAI_NetProtocol": 0x9862798E,
            "AIComponent_NetProtocol": 0x57218EFC, "CharacterBehavior_NetProtocol": 0x02FECC66}
MAX_INSNS = 6000
VOLATILE = {"rax", "rcx", "rdx", "r8", "r9", "r10", "r11"}

_FAM: dict[str, str] = {}
for _fam in [("rax", "eax", "ax", "al"), ("rbx", "ebx", "bx", "bl"), ("rcx", "ecx", "cx", "cl"),
             ("rdx", "edx", "dx", "dl"), ("rsi", "esi", "si", "sil"), ("rdi", "edi", "di", "dil"),
             ("rbp", "ebp", "bp", "bpl"), ("rsp", "esp", "sp", "spl")] + \
        [(f"r{i}", f"r{i}d", f"r{i}w", f"r{i}b") for i in range(8, 16)]:
    for _r in _fam:
        _FAM[_r] = _fam[0]

STORE_KIND = {1: "u8", 2: "u16", 4: "u32", 8: "u64", 16: "vec128"}


class Decoder:
    def __init__(self, img: atlas.Image):
        if capstone is None:
            raise RuntimeError("capstone is required (pip install capstone)")
        self.img = img
        self.md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
        self.md.detail = True
        self.unknown_calls: collections.Counter = collections.Counter()

    # -- helpers -------------------------------------------------------------------------------------------
    def r(self, reg: int) -> str:
        name = self.md.reg_name(reg)
        return _FAM.get(name, name)

    def code(self, va: int, size: int = 0x2000) -> list:
        off = va - B
        out = []
        for ins in self.md.disasm(self.img.b[off:off + size], va):
            out.append(ins)
            if len(out) >= MAX_INSNS:
                break
        return out

    @staticmethod
    def rip(ins, op) -> int:
        return ins.address + ins.size + op.mem.disp

    # -- binding -------------------------------------------------------------------------------------------
    def find_bind(self, wire: int, sites: list[int]) -> dict | None:
        for s in sites:
            f = self.img.func_of(s)
            if not f:
                continue
            ins = self.code(f[0], f[1] - f[0])
            k = next((j for j, i in enumerate(ins) if i.mnemonic == "mov" and len(i.operands) == 2
                      and i.operands[0].type == x86.X86_OP_MEM and i.operands[1].type == x86.X86_OP_IMM
                      and (i.operands[1].imm & 0xFFFFFFFF) == wire), None)
            if k is None:
                continue
            read = write = None
            for i in ins[:k]:
                if i.mnemonic == "lea" and self.r(i.operands[0].reg) == "rdx" and i.operands[1].mem.base == x86.X86_REG_RIP:
                    read = self.rip(i, i.operands[1])
            for i in ins[k:]:
                if i.mnemonic == "lea" and self.r(i.operands[0].reg) == "r8" and i.operands[1].mem.base == x86.X86_REG_RIP:
                    write = self.rip(i, i.operands[1])
                if i.mnemonic == "call" and i.operands[0].type == x86.X86_OP_MEM:
                    break
            if read and write and write not in THUNKS:
                return {"bind": f[0], "read": read, "write": write}
        return None

    # -- write delegate ------------------------------------------------------------------------------------
    def decode_write(self, va: int, src0: tuple[str, int] = ("", 0), dst0: int = 0, depth: int = 0) -> dict:
        md = self.md
        src: dict[str, tuple[str, int]] = {"rcx": src0}   # reg -> (pointer chain, offset) into the owner
        dst: dict[str, int] = {"rdx": dst0}               # reg -> offset into the memento
        val: dict = {}                                    # reg or ("stk", disp) -> (source path, kind, note)
        flag_load: dict[str, int] = {}                    # reg holding the flag pointer of slot K
        slots: set[int] = set()
        stores: list[tuple[int, int, tuple | None, str]] = []  # (memento offset, size, value, mnemonic)
        calls: list[dict] = []
        pending_bool = None
        nested = []
        sub: dict[str, str] = {}                          # reg -> element tag ("+0x80[]") of a sub-memento array
        vt_load: dict[str, int] = {}                      # reg holding the vtable of the slot object at K
        vt_bases: set[int] = set()
        targets: set[int] = set()                         # jump targets seen so far (code continues past a ret)
        stop_next = False

        def path(base: str, disp: int) -> str:
            chain, off = src[base]
            return f"{chain}+{off + disp:#x}"

        def clobber():
            for reg in VOLATILE:
                for table in (src, dst, val, flag_load, sub):
                    table.pop(reg, None)

        for ins in self.code(va):
            mn, ops = ins.mnemonic, ins.operands
            if stop_next:
                if ins.address not in targets:
                    break
                stop_next = False
            if mn == "int3":
                break
            if mn == "ret":
                stop_next = True
                continue
            if mn.startswith("j") and mn != "jmp" and ops and ops[0].type == x86.X86_OP_IMM:
                targets.add(ops[0].imm)
                continue
            if mn in ("call", "jmp") and ops and ops[0].type == x86.X86_OP_IMM:
                t = ops[0].imm
                f = self.img.func_of(t)
                is_entry = (f is not None and f[0] == t) or t in SETTERS
                if mn == "jmp" and not is_entry and va <= t < ins.address + 0x2000:
                    targets.add(t)
                    stop_next = True
                    continue
                if t in SETTERS and "rcx" in dst:
                    v = val.get("rdx")
                    source = v[0] if v else None
                    extra = {}
                    if SETTERS[t] == "array":
                        c = val.get("r8")
                        extra = {"count_source": c[0] if c else None}
                    calls.append({"slot": dst["rcx"], "type": SETTERS[t], "source": source, **extra})
                elif "rcx" in src and ("rdx" in dst or "rdx" in sub) and depth < 3 and t not in THUNKS:
                    inner = self.decode_write(t, src["rcx"], dst.get("rdx", 0), depth + 1)
                    if inner["fields"]:
                        nested.append(t)
                        tag = sub.get("rdx")
                        for f in inner["fields"]:
                            g = {**f, "via": f.get("via", atlas.hx(t))}
                            if tag:
                                g["slot"] = f"{tag}/{atlas.hx(f['slot']) if isinstance(f['slot'], int) else f['slot']}"
                            calls.append(g)
                    else:
                        self.unknown_calls[t] += 1
                elif "rcx" in dst:
                    self.unknown_calls[t] += 1
                    calls.append({"slot": dst["rcx"], "type": f"call:{t:#x}", "source": (val.get("rdx") or (None,))[0]})
                clobber()
                if mn == "jmp":
                    break
                continue
            if mn == "call":
                if ops and ops[0].type == x86.X86_OP_MEM and ops[0].mem.base and self.r(ops[0].mem.base) in vt_load:
                    vt_bases.add(vt_load[self.r(ops[0].mem.base)])
                clobber()
                vt_load.clear()
                continue
            if mn == "jmp":
                if ops and ops[0].type == x86.X86_OP_MEM:
                    break
                continue
            if mn.startswith("set") and ops and ops[0].type == x86.X86_OP_REG and pending_bool:
                val[self.r(ops[0].reg)] = (pending_bool, "bool", None)
                pending_bool = None
                continue
            if len(ops) == 1 and ops[0].type == x86.X86_OP_REG and mn in ("inc", "dec", "neg", "not"):
                reg = self.r(ops[0].reg)
                if reg in val:
                    p, k, note = val[reg]
                    val[reg] = (p, k, (note + " " if note else "") + mn)
                continue
            if len(ops) != 2:
                continue
            a, b = ops
            # register to register
            if a.type == x86.X86_OP_REG and b.type == x86.X86_OP_REG:
                ra, rb = self.r(a.reg), self.r(b.reg)
                if mn in ("mov", "movaps", "movss", "movups"):
                    for table in (src, dst, val, flag_load, sub):
                        if rb in table:
                            table[ra] = table[rb]
                        elif ra in table and ra != rb:
                            table.pop(ra)
                elif mn in ("xor", "sub") and ra == rb:
                    val[ra] = ("0", "const", None); src.pop(ra, None); dst.pop(ra, None)
                elif ra in val:
                    p, k, note = val[ra]
                    val[ra] = (p, k, (note + " " if note else "") + f"{mn} {self.md.reg_name(b.reg)}")
                continue
            if mn in ("shr", "sar", "shl", "and", "or", "add", "sub", "imul") and a.type == x86.X86_OP_REG and b.type == x86.X86_OP_IMM:
                ra = self.r(a.reg)
                if ra in val:
                    p, k, note = val[ra]
                    val[ra] = (p, "bits" if mn in ("shr", "sar", "and", "shl") else k,
                               (note + " " if note else "") + f"{mn} {b.imm:#x}")
                elif ra in dst and mn == "add":
                    dst[ra] += b.imm
                elif ra in src and mn == "add":
                    src[ra] = (src[ra][0], src[ra][1] + b.imm)
                continue
            if mn == "add" and a.type == x86.X86_OP_REG and b.type == x86.X86_OP_MEM and b.mem.base:
                ra, base = self.r(a.reg), self.r(b.mem.base)
                if base in dst:
                    sub[ra] = f"+{dst[base] + b.mem.disp:#x}[]"
                    dst.pop(ra, None); src.pop(ra, None)
                elif base in src:
                    src[ra] = (path(base, b.mem.disp) + "->[]", 0)
                    dst.pop(ra, None); val.pop(ra, None)
                continue
            if mn == "lea" and b.type == x86.X86_OP_MEM:
                ra = self.r(a.reg)
                base = self.r(b.mem.base) if b.mem.base else None
                src.pop(ra, None); dst.pop(ra, None); val.pop(ra, None)
                if base in dst and not b.mem.index:
                    dst[ra] = dst[base] + b.mem.disp
                elif base in src and not b.mem.index:
                    src[ra] = (src[base][0], src[base][1] + b.mem.disp)
                    val[ra] = (path(base, b.mem.disp), "ptr", None)
                elif base in ("rsp", "rbp") and ("stk", b.mem.disp) in val:
                    val[ra] = val[("stk", b.mem.disp)]
                continue
            # loads
            if a.type == x86.X86_OP_REG and b.type == x86.X86_OP_MEM and b.mem.base and mn.startswith(("mov", "cvt")):
                ra, base, disp = self.r(a.reg), self.r(b.mem.base), b.mem.disp
                src.pop(ra, None); dst.pop(ra, None); flag_load.pop(ra, None); sub.pop(ra, None)
                if base in dst and b.size == 8 and mn == "mov":
                    flag_load[ra] = dst[base] + disp
                    vt_load[ra] = dst[base] + disp
                    val.pop(ra, None)
                elif base in src:
                    kind = {"movss": "f32", "movsd": "f64", "movsx": f"i{b.size * 8}", "movsxd": "i32"}.get(mn, STORE_KIND.get(b.size, f"b{b.size}"))
                    if mn in ("movaps", "movups"):
                        kind = "vec128"
                    val[ra] = (path(base, disp), kind, None)
                    if b.size == 8 and mn == "mov":
                        src[ra] = (path(base, disp) + "->", 0)
                elif base in ("rsp", "rbp") and ("stk", disp) in val:
                    val[ra] = val[("stk", disp)]
                else:
                    val.pop(ra, None)
                continue
            # compares that feed setne/sete
            if mn in ("cmp", "test") and a.type == x86.X86_OP_MEM and a.mem.base and self.r(a.mem.base) in src \
                    and b.type in (x86.X86_OP_IMM, x86.X86_OP_REG):
                pending_bool = path(self.r(a.mem.base), a.mem.disp)
                continue
            if mn in ("bt",) and a.type == x86.X86_OP_MEM and a.mem.base and self.r(a.mem.base) in src:
                pending_bool = path(self.r(a.mem.base), a.mem.disp) + f" bit {b.imm if b.type == x86.X86_OP_IMM else '?'}"
                continue
            # stores
            if a.type == x86.X86_OP_MEM and a.mem.base:
                base, disp = self.r(a.mem.base), a.mem.disp
                if base in ("rsp", "rbp"):
                    if b.type == x86.X86_OP_REG and self.r(b.reg) in val:
                        val[("stk", disp)] = val[self.r(b.reg)]
                    continue
                if base in flag_load and disp == 0 and b.type == x86.X86_OP_IMM and b.imm == 1:
                    slots.add(flag_load[base])
                    continue
                if base in dst and mn.startswith("mov"):
                    off = dst[base] + disp
                    if b.type == x86.X86_OP_IMM:
                        stores.append((off, a.size, ("imm", b.imm), mn))
                    elif b.type == x86.X86_OP_REG:
                        stores.append((off, a.size, val.get(self.r(b.reg)), mn))
                continue
        # Pair each changed-marked slot K with the value stored at K+0x10..K+0x24 (not the changed byte K+8).
        fields = []
        slot_list = sorted(slots)
        used = set()
        for off, size, v, mnem in stores:
            k = max((s for s in slot_list if 0x10 <= off - s <= 0x24), default=None)
            if k is None or (k, off) in used or (v and v[0] == "imm"):
                continue
            used.add((k, off))
            kind = "f32" if mnem == "movss" else (v[1] if v and v[1] in ("bool", "bits", "f32", "i8", "i16", "i32") else STORE_KIND.get(size, f"b{size}"))
            base = k - 8 if (k - 8) in vt_bases else k   # slot objects with a vtable start 8 bytes before the flag pointer
            fields.append({"slot": base, "value_offset": off - base, "type": kind, "source": v[0] if v else None,
                           **({"vtable": True} if base != k else {}),
                           **({"note": v[2]} if v and v[2] else {}), "how": "inline"})
        have = {f["slot"] for f in fields}
        for c in calls:
            if c["slot"] not in have:
                fields.append({**c, "how": c.get("how", "call") if "via" not in c else "nested"})
                have.add(c["slot"])
        have |= {f["slot"] + 8 for f in fields if f.get("vtable")}
        for s in slot_list:
            if s not in have:
                fields.append({"slot": s, "type": "unpaired", "source": None, "how": "inline"})
        fields.sort(key=lambda f: (1, 0, f["slot"]) if isinstance(f["slot"], str) else (0, f["slot"], ""))
        return {"fields": fields, "nested_writers": [atlas.hx(t) for t in nested]}


def decode_read(dec: "Decoder", va: int) -> dict:
    """Inline copies in the read delegate (owner RCX, memento RDX): memento value offset -> owner field.
    Also the stamp rule: the first `mov e, [m+0x50]; cmp e, [o+T]; jb` gives the owner's stamp field T."""
    m = {"rdx"}; o = {"rcx"}
    val: dict[str, tuple[int, str | None]] = {}
    pending = None
    pairs: dict[int, dict] = {}
    stamp = None
    for ins in dec.code(va, 0x1800)[:2500]:
        mn, ops = ins.mnemonic, ins.operands
        if mn in ("ret", "int3") or (mn == "jmp" and ops and ops[0].type == x86.X86_OP_MEM):
            break
        if mn == "call":
            for reg in VOLATILE:
                val.pop(reg, None); m.discard(reg); o.discard(reg)
            continue
        if len(ops) == 1 and ops[0].type == x86.X86_OP_REG and mn in ("inc", "dec", "neg", "not"):
            r = dec.r(ops[0].reg)
            if r in val:
                val[r] = (val[r][0], ((val[r][1] or "") + " " + mn).strip())
            continue
        if mn.startswith("set") and ops and pending is not None:
            val[dec.r(ops[0].reg)] = (pending, "bool")
            pending = None
            continue
        if len(ops) != 2:
            continue
        a, b = ops
        if a.type == x86.X86_OP_REG and b.type == x86.X86_OP_REG and mn == "mov":
            ra, rb = dec.r(a.reg), dec.r(b.reg)
            for st in (m, o):
                if rb in st:
                    st.add(ra)
                elif ra in st and ra != rb:
                    st.discard(ra)
            if rb in val:
                val[ra] = val[rb]
            continue
        if a.type == x86.X86_OP_REG and b.type == x86.X86_OP_MEM and b.mem.base and mn.startswith("mov"):
            ra, base = dec.r(a.reg), dec.r(b.mem.base)
            m.discard(ra); o.discard(ra)
            if base in m:
                val[ra] = (b.mem.disp, None)
            else:
                val.pop(ra, None)
            continue
        if mn == "cmp" and a.type == x86.X86_OP_REG and b.type == x86.X86_OP_MEM and b.mem.base and dec.r(b.mem.base) in o \
                and stamp is None and val.get(dec.r(a.reg), (None,))[0] == 0x50:
            stamp = b.mem.disp
            continue
        if mn in ("cmp", "test") and a.type == x86.X86_OP_MEM and a.mem.base and dec.r(a.mem.base) in m:
            pending = a.mem.disp
            continue
        if a.type == x86.X86_OP_MEM and a.mem.base and b.type == x86.X86_OP_REG and mn.startswith("mov"):
            base = dec.r(a.mem.base)
            r = dec.r(b.reg)
            if base in o and r in val and val[r][0] != 0x50:
                pairs.setdefault(val[r][0], {"owner": f"+{a.mem.disp:#x}", **({"note": val[r][1]} if val[r][1] else {})})
            continue
    return {"stamp_owner_field": atlas.hx(stamp) if stamp is not None else None,
            "pairs": {atlas.hx(k): v for k, v in sorted(pairs.items())}}


def read_check(fields: list[dict], rd: dict) -> dict:
    """Compare write-side inline fields (slot + value offset -> owner source) with read-side pairs."""
    match, differ, missing = 0, [], 0
    for f in fields:
        if f.get("how") != "inline" or not isinstance(f["slot"], int) or not f.get("source"):
            continue
        key = atlas.hx(f["slot"] + f["value_offset"])
        p = rd["pairs"].get(key)
        if p is None:
            missing += 1
        elif p["owner"] == f["source"]:
            match += 1
        else:
            differ.append({"slot": atlas.hx(f["slot"]), "write_source": f["source"], "read_owner": p["owner"]})
    return {"inline_matched": match, "inline_differ": differ, "inline_not_read_inline": missing}


def decode_all(img: atlas.Image, atlas_json: dict) -> dict:
    dec = Decoder(img)
    text_lo, text_hi = img.sec(".text")
    recs = [r for r in atlas_json["type_records"]
            if "NetObjectMemento" in r["ancestors"] and r["name"] != "NetObjectMemento"]
    out = []
    for r in recs:
        row = {"name": r["name"]}
        w = r.get("wire_id")
        if not w:
            row["status"] = "no wire id"
            out.append(row); continue
        wire = ALT_WIRE.get(r["name"], int(w["broadcast"], 16))
        row["wire_id"] = f"{wire:08X}"
        if r["name"] in ALT_WIRE:
            row["wire_id_source"] = "ACU-RE-0084 registration (not the zenitar CRC)"
        needle = struct.pack("<I", wire)
        sites, i = [], img.b.find(needle, text_lo - B)
        while i != -1 and i < text_hi - B:
            sites.append(B + i)
            i = img.b.find(needle, i + 1)
        b = dec.find_bind(wire, sites)
        if not b:
            row["status"] = "no bind found"
            row["wire_sites"] = [atlas.hx(s) for s in sites]
            out.append(row); continue
        d = dec.decode_write(b["write"])
        rd = decode_read(dec, b["read"])
        d["read"] = {"stamp_owner_field": rd["stamp_owner_field"], **read_check(d["fields"], rd)}
        for f in d["fields"]:
            if isinstance(f["slot"], int):
                f["slot"] = atlas.hx(f["slot"])
        check = d.pop("read")
        row.update({"status": "decoded" if d["fields"] else "bind only", "bind": atlas.hx(b["bind"]),
                    "read": atlas.hx(b["read"]), "write": atlas.hx(b["write"]), **d, "read_check": check})
        out.append(row)
    counts = collections.Counter(x["status"] for x in out)
    rc = [x["read_check"] for x in out if "read_check" in x]
    kinds = collections.Counter(f["type"] for x in out for f in x.get("fields", []))
    return {"mementos": out, "summary": {"total": len(out), **counts,
                                         "fields": sum(len(x.get("fields", [])) for x in out),
                                         "read_check": {"inline_matched": sum(c["inline_matched"] for c in rc),
                                                        "inline_differ": sum(len(c["inline_differ"]) for c in rc),
                                                        "inline_not_read_inline": sum(c["inline_not_read_inline"] for c in rc),
                                                        "stamp_found": sum(1 for c in rc if c["stamp_owner_field"])},
                                         "types": dict(kinds.most_common())},
            "unknown_calls": {atlas.hx(k): v for k, v in dec.unknown_calls.most_common()}}


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--image", required=True, type=Path)
    ap.add_argument("--atlas", required=True, type=Path)
    ap.add_argument("--expect-sha256")
    ap.add_argument("--output", required=True, type=Path)
    a = ap.parse_args(argv)
    if a.expect_sha256 and atlas.sha256(a.image) != a.expect_sha256.lower():
        ap.error("image SHA-256 mismatch")
    img = atlas.Image(a.image.read_bytes())
    res = decode_all(img, json.loads(a.atlas.read_text(encoding="utf-8")))
    res = {"schema": "acu-memento-codecs/1", "finding": "ACU-RE-0095", "image_sha256": atlas.sha256(a.image),
           "tool_sha256": atlas.sha256(Path(__file__)),
           "constants": {"setters": {atlas.hx(k): v for k, v in SETTERS.items()}}, **res}
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(res, indent=1), encoding="utf-8")
    print(json.dumps(res["summary"]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
