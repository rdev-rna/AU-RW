#!/usr/bin/env python3
"""Map ACU co-op messages that fan out by event hash to the AI-state listeners that receive them (ACU-RE-0101).

Input: the captured mapped main image and `local/coop/message_handlers.json` (tools/net_message_handlers.py,
ACU-RE-0098). For every message whose handler calls the fan-out `0x14275C670(component, hash, 0x32, list)`, the
tool takes the hash (the `mov edx, imm32` before the call, in the handler or one call below it).

Listener tables live in `.data` as runs of 32-byte entries `{u32 hash, u32 index, u64 flags, u64 handler, u64 0}`
whose index counts 0, 1, 2, ... An AI-state constructor stores the table at state `+24` and the state hash at
`+36`. A handler of `0x141E6D4E0` (a bare `ret`) means the state ignores the event.

Each listener is classified:
- ignore: `nullsub_7`;
- accept-only / refuse: a body of at most a few instructions, with no branch or call, that only stores 1 or 0
  through the out-flag pointer (`mov byte [reg], 1|0; ret`; the pointer may be loaded from the stack);
- action: anything else.
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
FANOUT = 0x14275C670
NULLSUB = 0x141E6D4E0
ENTRY = struct.Struct("<IIQQQ")


class Listeners:
    def __init__(self, img):
        if capstone is None:
            raise RuntimeError("capstone is required (pip install capstone)")
        self.img = img
        self.md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
        self.md.detail = True
        self.lo, self.hi = img.sec(".text")
        self.dlo, self.dhi = img.sec(".data")

    def hashes(self, va: int, depth: int = 1, seen: set | None = None) -> list[int]:
        """Event hashes a handler passes to the fan-out (`mov edx, imm32` before `call 0x14275C670`)."""
        seen = set() if seen is None else seen
        if va in seen or not (self.lo <= va < self.hi):
            return []
        seen.add(va)
        out, edx = [], None
        for k, i in enumerate(self.md.disasm(self.img.b[va - B:va - B + 0x800], va)):
            if k > 300 or i.mnemonic == "int3":
                break
            ops = i.operands
            if i.mnemonic == "mov" and len(ops) == 2 and ops[0].type == x86.X86_OP_REG \
                    and i.reg_name(ops[0].reg) == "edx" and ops[1].type == x86.X86_OP_IMM:
                edx = ops[1].imm & 0xFFFFFFFF
            elif i.mnemonic in ("call", "jmp") and ops and ops[0].type == x86.X86_OP_IMM:
                if ops[0].imm == FANOUT:
                    if edx is not None:
                        out.append(edx)
                elif depth > 0:
                    out += self.hashes(ops[0].imm, depth - 1, seen)
                if i.mnemonic == "jmp":
                    break
            elif i.mnemonic == "ret":
                break
        return out

    def entry(self, a: int) -> tuple:
        return ENTRY.unpack_from(self.img.b, a - B)

    def plausible(self, a: int) -> bool:
        if not (self.dlo <= a and a + ENTRY.size <= self.dhi):
            return False
        h, idx, fl, fn, z = self.entry(a)
        return z == 0 and fl < 0x100 and idx < 0x400 and (fn in (0, NULLSUB) or self.lo <= fn < self.hi)

    def tables(self, wanted: set[int]) -> dict[int, int]:
        """Tables (start -> entry count) that contain one of the wanted hashes."""
        found: dict[int, int] = {}
        for h in wanted:
            nd = struct.pack("<I", h)
            i = self.img.b.find(nd, self.dlo - B, self.dhi - B)
            while i != -1:
                a = B + i
                if (a - self.dlo) % 8 == 0 and self.plausible(a):
                    start = a - self.entry(a)[1] * ENTRY.size
                    if start not in found and self.plausible(start) and self.entry(start)[1] == 0:
                        n = 0
                        while self.plausible(start + n * ENTRY.size) and self.entry(start + n * ENTRY.size)[1] == n:
                            n += 1
                        if start + self.entry(a)[1] * ENTRY.size < start + n * ENTRY.size:
                            found[start] = n
                i = self.img.b.find(nd, i + 1, self.dhi - B)
        return found

    def kind(self, fn: int) -> str:
        if fn in (0, NULLSUB):
            return "ignore"
        ins = list(self.md.disasm(self.img.b[fn - B:fn - B + 16], fn, 4))
        end = next((k for k, i in enumerate(ins) if i.mnemonic == "ret"), None)
        if end is None or any(i.mnemonic in ("call", "jmp") or i.mnemonic.startswith("j") for i in ins[:end]):
            return "action"
        flags = [i.operands[1].imm for i in ins[:end] if i.mnemonic == "mov" and len(i.operands) == 2
                 and i.operands[0].type == x86.X86_OP_MEM and i.operands[0].size == 1
                 and i.operands[1].type == x86.X86_OP_IMM and i.operands[0].mem.disp == 0]
        if len(flags) == 1 and flags[0] in (0, 1):
            return "accept-only" if flags[0] == 1 else "refuse"    # only sets the out flag: accepted / refused
        return "action"


def build(img, handlers: dict) -> dict:
    ls = Listeners(img)
    msg_hashes = {m["name"]: ls.hashes(int(m["handler"], 16)) for m in handlers["messages"]
                  if m.get("dispatch") == "event-fanout"}
    wanted = {h for v in msg_hashes.values() for h in v}
    tables = ls.tables(wanted)
    by_hash: dict[int, list] = collections.defaultdict(list)
    for s, n in tables.items():
        for k in range(n):
            h, idx, fl, fn, z = ls.entry(s + k * ENTRY.size)
            if h in wanted:
                by_hash[h].append((s, n, fn))
    out = []
    for name, hs in msg_hashes.items():
        events = []
        for h in hs:
            rows = by_hash.get(h, [])
            kinds = collections.Counter(ls.kind(fn) for _, _, fn in rows)
            sizes = collections.Counter(n for _, n, _ in rows)
            actions = collections.Counter(fn for _, _, fn in rows if ls.kind(fn) == "action")
            events.append({"hash": f"{h:08X}", "tables": len(rows), "kinds": dict(kinds),
                           "table_sizes": {str(k): v for k, v in sizes.items()},
                           "actions": {atlas.hx(f): c for f, c in actions.most_common()}})
        out.append({"name": name, "events": events})
    sizes = collections.Counter(tables.values())
    return {"messages": out,
            "summary": {"fanout_messages": len(msg_hashes), "event_hashes": len(wanted), "tables": len(tables),
                        "table_sizes": {str(k): v for k, v in sorted(sizes.items())},
                        "messages_without_listener": sorted(n for n, e in ((o["name"], o["events"]) for o in out)
                                                            if not any(x["tables"] for x in e))}}


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--image", required=True, type=Path)
    ap.add_argument("--handlers", required=True, type=Path)
    ap.add_argument("--expect-sha256")
    ap.add_argument("--output", required=True, type=Path)
    a = ap.parse_args(argv)
    if a.expect_sha256 and atlas.sha256(a.image) != a.expect_sha256.lower():
        ap.error("image SHA-256 mismatch")
    img = atlas.Image(a.image.read_bytes())
    res = build(img, json.loads(a.handlers.read_text(encoding="utf-8")))
    res = {"schema": "acu-event-listeners/1", "finding": "ACU-RE-0101", "image_sha256": atlas.sha256(a.image),
           "tool_sha256": atlas.sha256(Path(__file__)), **res}
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(res, indent=1), encoding="utf-8")
    s = dict(res["summary"])
    s["messages_without_listener"] = len(s["messages_without_listener"])
    print(json.dumps(s))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
