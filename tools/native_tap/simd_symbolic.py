"""Symbolic emulation of straight-line SSE blocks in the mapped ACU image.

Turns shuffle-heavy SIMD code into sympy expressions so a port can be checked
against the exact lane formulas instead of a hand trace. Scope is deliberately
small: packed/scalar float arithmetic, shuffles, unpacks, bit masks with
all-ones/zero/sign/abs lane patterns, loads of RIP-relative constants from the
image and symbolic memory addressed by register + displacement. Branches are
not followed; the caller picks a branch-free range and supplies call results.

Usage (as a library): see ``emulate``. Unsupported instructions raise.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass, field

import capstone
import sympy as sp
from capstone import x86

BASE = 0x140000000
FULL = 0xFFFFFFFF
SIGN = 0x80000000
ABS = 0x7FFFFFFF


@dataclass(frozen=True)
class Bits:
    """A lane that is a raw bit pattern rather than a float expression."""

    value: int


def _as_expr(lane):
    if isinstance(lane, Bits):
        if lane.value == 0:
            return sp.Integer(0)
        return sp.Float(struct.unpack("<f", struct.pack("<I", lane.value))[0])
    return lane


def _and(a, b):
    if isinstance(a, Bits) and isinstance(b, Bits):
        return Bits(a.value & b.value)
    if isinstance(b, Bits):
        a, b = b, a
    if isinstance(a, Bits):
        if a.value == FULL:
            return b
        if a.value == 0:
            return Bits(0)
        if a.value == ABS:
            return sp.Abs(b)
    raise ValueError(f"unsupported and: {a!r} & {b!r}")


def _not(a):
    if isinstance(a, Bits):
        return Bits(~a.value & FULL)
    raise ValueError(f"unsupported not of expression {a!r}")


def _or(a, b):
    if isinstance(a, Bits) and a.value == 0:
        return b
    if isinstance(b, Bits) and b.value == 0:
        return a
    if isinstance(a, Bits) and isinstance(b, Bits):
        return Bits(a.value | b.value)
    raise ValueError(f"unsupported or: {a!r} | {b!r}")


def _xor(a, b):
    if isinstance(a, Bits) and isinstance(b, Bits):
        return Bits(a.value ^ b.value)
    if isinstance(b, Bits):
        a, b = b, a
    if isinstance(a, Bits):
        if a.value == 0:
            return b
        if a.value == SIGN:
            return -b
    raise ValueError(f"unsupported xor: {a!r} ^ {b!r}")


@dataclass
class State:
    xmm: dict[str, list] = field(default_factory=dict)
    mem: dict[tuple[str, int], object] = field(default_factory=dict)
    gpr_alias: dict[str, tuple[str, int]] = field(default_factory=dict)
    log: list[str] = field(default_factory=list)


class Emulator:
    def __init__(self, image: bytes, aliases: dict[str, tuple[str, int]]):
        self.image = image
        self.md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
        self.md.detail = True
        self.st = State(gpr_alias=dict(aliases))

    # ----- memory -----------------------------------------------------
    def _addr(self, ins, op):
        m = op.mem
        if m.index != 0:
            raise ValueError(f"indexed address at {ins.address:#x}")
        reg = ins.reg_name(m.base)
        if reg == "rip":
            return ("abs", ins.address + ins.size + m.disp)
        base, off = self.st.gpr_alias.get(reg, (reg, 0))
        return (base, off + m.disp)

    def _load_lane(self, key):
        base, off = key
        if base == "abs":
            raw = struct.unpack_from("<I", self.image, off - BASE)[0]
            return Bits(raw)
        if key not in self.st.mem:
            self.st.mem[key] = sp.Symbol(f"{base}_{off:x}")
        return self.st.mem[key]

    def load(self, key, lanes=4):
        base, off = key
        return [self._load_lane((base, off + 4 * i)) for i in range(lanes)]

    def store(self, key, values):
        base, off = key
        for i, v in enumerate(values):
            self.st.mem[(base, off + 4 * i)] = v

    # ----- registers --------------------------------------------------
    def get(self, name):
        if name not in self.st.xmm:
            self.st.xmm[name] = [sp.Symbol(f"{name}_{i}") for i in range(4)]
        return list(self.st.xmm[name])

    def set(self, name, values):
        self.st.xmm[name] = list(values)

    def src(self, ins, op, lanes=4):
        if op.type == x86.X86_OP_REG:
            return self.get(ins.reg_name(op.reg))
        if op.type == x86.X86_OP_MEM:
            return self.load(self._addr(ins, op), lanes)
        raise ValueError(f"operand at {ins.address:#x}")

    # ----- execution --------------------------------------------------
    def run(self, start, end, calls=None):
        calls = calls or {}
        code = self.image[start - BASE:end - BASE]
        for ins in self.md.disasm(code, start):
            self.step(ins, calls)

    def step(self, ins, calls):
        m, ops = ins.mnemonic, ins.operands
        packed = {"addps": sp.Add, "mulps": sp.Mul}
        if m in ("movaps", "movups"):
            d, s = ops
            if d.type == x86.X86_OP_MEM:
                self.store(self._addr(ins, d), self.get(ins.reg_name(s.reg)))
            else:
                self.set(ins.reg_name(d.reg), self.src(ins, s))
        elif m == "movss":
            d, s = ops
            if d.type == x86.X86_OP_MEM:
                self.store(self._addr(ins, d), self.get(ins.reg_name(s.reg))[:1])
            elif s.type == x86.X86_OP_MEM:
                self.set(ins.reg_name(d.reg), self.src(ins, s, 1) + [Bits(0)] * 3)
            else:
                dv = self.get(ins.reg_name(d.reg))
                dv[0] = self.get(ins.reg_name(s.reg))[0]
                self.set(ins.reg_name(d.reg), dv)
        elif m in ("addps", "subps", "mulps", "divps", "addss", "subss", "mulss", "divss"):
            d, s = ops
            a = [_as_expr(v) for v in self.get(ins.reg_name(d.reg))]
            scalar = m.endswith("ss")
            b = [_as_expr(v) for v in self.src(ins, s, 1 if scalar else 4)]
            fn = {
                "add": lambda x, y: x + y,
                "sub": lambda x, y: x - y,
                "mul": lambda x, y: x * y,
                "div": lambda x, y: x / y,
            }[m[:3]]
            if scalar:
                a[0] = fn(a[0], b[0])
                out = [a[0]] + self.get(ins.reg_name(d.reg))[1:]
            else:
                out = [fn(x, y) for x, y in zip(a, b)]
            self.set(ins.reg_name(d.reg), out)
        elif m in ("andps", "andnps", "orps", "xorps"):
            d, s = ops
            a = self.get(ins.reg_name(d.reg))
            same = s.type == x86.X86_OP_REG and s.reg == d.reg
            if m == "xorps" and same:
                self.set(ins.reg_name(d.reg), [Bits(0)] * 4)
                return
            b = self.src(ins, s)
            if m == "andps":
                out = [_and(x, y) for x, y in zip(a, b)]
            elif m == "andnps":
                out = [_and(_not(x), y) for x, y in zip(a, b)]
            elif m == "orps":
                out = [_or(x, y) for x, y in zip(a, b)]
            else:
                out = [_xor(x, y) for x, y in zip(a, b)]
            self.set(ins.reg_name(d.reg), out)
        elif m in ("movhlps", "movlhps"):
            d, s = ops
            a = self.get(ins.reg_name(d.reg))
            b = self.src(ins, s)
            self.set(ins.reg_name(d.reg), b[2:] + a[2:] if m == "movhlps" else a[:2] + b[:2])
        elif m == "shufps":
            d, s, imm = ops
            a = self.get(ins.reg_name(d.reg))
            b = self.src(ins, s)
            i = imm.imm
            self.set(ins.reg_name(d.reg), [a[i & 3], a[(i >> 2) & 3], b[(i >> 4) & 3], b[(i >> 6) & 3]])
        elif m in ("unpcklps", "unpckhps"):
            d, s = ops
            a = self.get(ins.reg_name(d.reg))
            b = self.src(ins, s)
            k = 0 if m == "unpcklps" else 2
            self.set(ins.reg_name(d.reg), [a[k], b[k], a[k + 1], b[k + 1]])
        elif m in ("sqrtps", "rsqrtps", "rcpps", "sqrtss"):
            d, s = ops
            v = [_as_expr(x) for x in self.src(ins, s)]
            f = {"sqrtps": sp.sqrt, "sqrtss": sp.sqrt, "rsqrtps": lambda x: 1 / sp.sqrt(x),
                 "rcpps": lambda x: 1 / x}[m]
            if m == "sqrtss":
                out = self.get(ins.reg_name(d.reg))
                out[0] = f(v[0])
            else:
                out = [f(x) for x in v]
            self.set(ins.reg_name(d.reg), out)
        elif m == "call":
            target = ops[0].imm
            if target not in calls:
                raise ValueError(f"call {target:#x} at {ins.address:#x} has no stub")
            x0 = self.get("xmm0")
            self.set("xmm0", [calls[target](_as_expr(x0[0]))] + [Bits(0)] * 3)
        elif m in ("mov", "lea", "nop", "comiss", "ucomiss") or m.startswith("j"):
            # Branch outcomes are chosen by the caller's range selection.
            # Integer moves only matter for addressing; callers alias them.
            self.st.log.append(f"skip {ins.address:#x} {m} {ins.op_str}")
        else:
            raise ValueError(f"unsupported {m} {ins.op_str} at {ins.address:#x}")


def load_image(path):
    with open(path, "rb") as fh:
        return fh.read()
