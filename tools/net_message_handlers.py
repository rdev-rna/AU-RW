#!/usr/bin/env python3
"""Map every ACU co-op message to its execute handler and classify the handler's role guard.

Input: the captured mapped main image, the atlas (tools/net_protocol_atlas.py, ACU-RE-0072) and the message layouts
(tools/net_intention_payloads.py --all, ACU-RE-0093/0097). Output JSON: per message, its handler, the registration
table it came from, how the name was matched, and the role guard found in the handler (ACU-RE-0098).

Method:
1. Registration tables. Each message family has a function that loads, for every message, its short ID string
   (`<Name>MSG_NetProtocol_ID[_UNICAST]`) and then the code pointer of its handler (the family's first entry also
   loads the send builder). The tool pairs every ID string with the code pointers loaded after it.
2. Names. A short string is matched to the full `<Family>_<Name>MSG_NetProtocol` name: first through the builder
   that contains the message's block, then by a short name unique across all families, then by the table's
   majority family. Static RFCs come from the atlas (their last callback is the execute callback).
3. Role guard. The handler and the functions it calls directly are searched for the entity role test
   `(x & 0xC0000) == 0x40000` (master) or `== 0x80000` (replica) (ACU-RE-0092), and for the Storm net-object
   master test `0x141C50B40`.
Read only. Needs capstone.
"""
from __future__ import annotations

import argparse
import collections
import json
import re
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
COMMON = {0x1401525E0, 0x14022EC80, 0x1401B78D0}   # shared delegate helpers seen in every registration
NETOBJ_IS_MASTER = 0x141C50B40                      # used by the distance migration (ACU-RE-0097)
NETOBJ_NOT_REMOTE = 0x141C53E20                     # true unless the Storm object's state dword +56 == 1
EVENT_FANOUT = 0x14275C670                          # collects the listeners of an event hash, then calls each
QUEUE_PUSH = 0x140D50F50                            # pushes an event into a director queue (under a lock)
ID_RE = re.compile(rb"[A-Za-z0-9_]{3,}MSG_NetProtocol_ID(?:_UNICAST)?\x00")
LEA_RIP = re.compile(rb"[\x48\x4C]\x8D[\x05\x0D\x15\x1D\x25\x2D\x35\x3D]")


def short_of(s: str) -> str:
    return s.replace("zenitar_", "").replace("_ID_UNICAST", "").replace("_ID", "")


class Mapper:
    def __init__(self, img: atlas.Image):
        if capstone is None:
            raise RuntimeError("capstone is required (pip install capstone)")
        self.img = img
        self.md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
        self.md.detail = True
        self.lo, self.hi = img.sec(".text")

    def id_strings(self) -> dict[int, str]:
        """Every ID string, including tails of longer strings (the linker shares a short name such as
        `StartMSG_NetProtocol_ID` with the end of `ChaseStartMSG_NetProtocol_ID`)."""
        out = {}
        for m in ID_RE.finditer(self.img.b):
            s = m.group()[:-1].decode()
            cut = s.index("MSG_NetProtocol_ID")
            for k in range(cut - 3 + 1):
                out.setdefault(B + m.start() + k, s[k:])
        return out

    def string_refs(self, strs: dict[int, str]) -> dict[int, set[str]]:
        t = self.img.b[self.lo - B:self.hi - B]
        byfunc: dict[int, set[str]] = collections.defaultdict(set)
        for m in LEA_RIP.finditer(t):
            i = m.start()
            va = self.lo + i + 7 + struct.unpack_from("<i", t, i + 3)[0]
            if va in strs:
                f = self.img.func_of(self.lo + i)
                if f:
                    byfunc[f[0]].add(strs[va])
        return byfunc

    def table_pairs(self, f: int, strs: dict[int, str]) -> dict[str, list[int]]:
        fb = self.img.func_of(f)
        pairs: dict[str, list[int]] = collections.defaultdict(list)
        cur = None
        for i in self.md.disasm(self.img.b[f - B:fb[1] - B], f):
            if i.mnemonic == "lea" and len(i.operands) == 2 and i.operands[1].type == x86.X86_OP_MEM \
                    and i.operands[1].mem.base == x86.X86_REG_RIP:
                t = i.address + i.size + i.operands[1].mem.disp
                if t in strs:
                    cur = strs[t]
                elif cur and self.lo <= t < self.hi and not (f <= t < fb[1]) and t not in COMMON:
                    pairs[cur].append(t)
        return pairs

    def first_code_lea(self, f: int, skip: set[int]) -> int | None:
        fb = self.img.func_of(f)
        for i in self.md.disasm(self.img.b[f - B:fb[1] - B], f):
            if i.mnemonic == "lea" and len(i.operands) == 2 and i.operands[1].type == x86.X86_OP_MEM                     and i.operands[1].mem.base == x86.X86_REG_RIP:
                t = i.address + i.size + i.operands[1].mem.disp
                if self.lo <= t < self.hi and not (f <= t < fb[1]) and t not in skip:
                    return t
        return None

    def is_noop(self, va: int) -> bool:
        """True for an empty handler: `ret`, or `xor eax,eax; ret` / `mov al,1; ret`."""
        ins = list(self.md.disasm(self.img.b[va - B:va - B + 8], va, 2))
        return bool(ins) and (ins[0].mnemonic == "ret" or (len(ins) == 2 and ins[1].mnemonic == "ret"
                                                             and ins[0].mnemonic in ("xor", "mov")))

    def role_guard(self, va: int, depth: int = 1) -> dict:
        """Search the handler (and direct callees) for (x & 0xC0000) == 0x40000 / 0x80000, or the net-object test."""
        seen: dict = {"master": False, "replica": False, "netobj_master_test": False, "netobj_not_remote": False,
                      "event_fanout": False, "queue_push": False, "where": []}
        todo, done = [(va, 0)], set()
        while todo:
            a, d = todo.pop()
            if a in done or not (self.lo <= a < self.hi):
                continue
            done.add(a)
            masked = False
            for k, ins in enumerate(self.md.disasm(self.img.b[a - B:a - B + 0x600], a)):
                if k > 220 or ins.mnemonic in ("int3",):
                    break
                ops = ins.operands
                if ins.mnemonic == "and" and len(ops) == 2 and ops[1].type == x86.X86_OP_IMM and (ops[1].imm & 0xFFFFFFFF) == 0xC0000:
                    masked = True
                elif ins.mnemonic == "cmp" and masked and len(ops) == 2 and ops[1].type == x86.X86_OP_IMM:
                    v = ops[1].imm & 0xFFFFFFFF
                    if v == 0x40000:
                        seen["master"] = True; seen["where"].append(atlas.hx(ins.address))
                    elif v == 0x80000:
                        seen["replica"] = True; seen["where"].append(atlas.hx(ins.address))
                elif ins.mnemonic in ("call", "jmp") and ops and ops[0].type == x86.X86_OP_IMM:
                    if ops[0].imm == NETOBJ_IS_MASTER:
                        seen["netobj_master_test"] = True; seen["where"].append(atlas.hx(ins.address))
                    elif ops[0].imm == NETOBJ_NOT_REMOTE:
                        seen["netobj_not_remote"] = True; seen["where"].append(atlas.hx(ins.address))
                    elif ops[0].imm == EVENT_FANOUT:
                        seen["event_fanout"] = True
                    elif ops[0].imm == QUEUE_PUSH:
                        seen["queue_push"] = True
                    elif d < depth:
                        todo.append((ops[0].imm, d + 1))
                    if ins.mnemonic == "jmp":
                        break
                elif ins.mnemonic == "ret":
                    break
        seen["where"] = seen["where"][:6]
        return seen


def guard_class(rg: dict) -> str:
    if rg["master"] and rg["replica"]:
        return "both"
    if rg["master"]:
        return "master"
    if rg["replica"]:
        return "replica"
    if rg["netobj_master_test"] or rg["netobj_not_remote"]:
        return "netobj"
    return "none"


def dispatch_class(rg: dict) -> str:
    """event-fanout: listeners of an event hash (NetHumanIntention); queued: pushed to a director queue;
    direct: applied in the handler."""
    return "event-fanout" if rg["event_fanout"] else "queued" if rg["queue_push"] else "direct"


def build(img: atlas.Image, atlas_json: dict, payloads: dict) -> dict:
    mp = Mapper(img)
    strs = mp.id_strings()
    byfunc = mp.string_refs(strs)
    msgs = payloads["messages"]
    names = {m["name"] for m in msgs}
    builder_msgs: dict[int, list[str]] = collections.defaultdict(list)
    for m in msgs:
        f = img.func_of(int(m["block"], 16))
        if f:
            builder_msgs[f[0]].append(m["name"])
    suffix: dict[str, set[str]] = collections.defaultdict(set)
    for n in names:
        parts = n.split("_")
        for k in range(1, len(parts)):
            suffix["_".join(parts[k:])].add(n)
    tables = {f: mp.table_pairs(f, strs) for f, s in byfunc.items() if len(s) >= 2}
    res: dict[str, dict] = {}

    def put(n, handler, table, how):
        if n in names and n not in res:
            res[n] = {"handler": atlas.hx(handler), "table": atlas.hx(table), "match": how}

    for tf, pairs in tables.items():            # 1. by builder
        cps = {c for lst in pairs.values() for c in lst}
        fam = {n for c in cps if c in builder_msgs for n in builder_msgs[c]}
        for s, lst in pairs.items():
            hs = [c for c in lst if c not in builder_msgs]
            c = [n for n in fam if n.endswith("_" + short_of(s))]
            if len(c) == 1 and hs:
                put(c[0], hs[-1], tf, "builder")
    for tf, pairs in tables.items():            # 2. unique short name
        for s, lst in pairs.items():
            c = suffix.get(short_of(s), set())
            if len(c) == 1 and lst:
                put(next(iter(c)), lst[-1], tf, "unique-short-name")
    tfam: dict[str, collections.Counter] = collections.defaultdict(collections.Counter)
    for n, v in res.items():
        tfam[v["table"]][n.split("_")[0]] += 1
    ends = {img.func_of(tf)[1]: tf for tf in tables}
    for tf, pairs in tables.items():            # 3. table majority family (a chunk without one takes the
        c = tfam.get(atlas.hx(tf))              #    family of the chunk that ends where it starts)
        prev = tf
        while not c and prev in ends:
            prev = ends[prev]
            c = tfam.get(atlas.hx(prev))
        if not c:
            continue
        fam = c.most_common(1)[0][0]
        for s, lst in pairs.items():
            if lst:
                put(f"{fam}_{short_of(s)}", lst[-1], tf, "table-family")
    for r in atlas_json.get("static_rfcs", []):  # 4. static RFCs
        n = r["message"].replace("zenitar_", "").replace("_ID", "")
        cbs = [int(c, 16) for c in r.get("callbacks", [])]
        if len(cbs) == 5:                      # [receive, 0x1401525E0, send, 0x14022EC80, execute]
            put(n, cbs[-1], int(r["register"], 16), "static-rfc")
        elif len(cbs) == 4:                    # execute slot left null; the register function passes the
            ex = mp.first_code_lea(int(r["register"], 16), set(cbs) | COMMON)   # handler to a separate call
            if ex:
                put(n, ex, int(r["register"], 16), "static-rfc-separate")
    for n, v in res.items():
        v["role_guard"] = mp.role_guard(int(v["handler"], 16))
        v["noop"] = mp.is_noop(int(v["handler"], 16))
        v["dispatch"] = dispatch_class(v["role_guard"])
    out = []
    for m in msgs:
        out.append({"name": m["name"], "system": m.get("system"), **res.get(m["name"], {"handler": None})})
    g = collections.Counter(guard_class(v["role_guard"]) for v in res.values())
    return {"messages": out, "summary": {"messages": len(msgs), "with_handler": len(res),
                                         "match": dict(collections.Counter(v["match"] for v in res.values())),
                                         "role_guard": dict(g),
                                         "dispatch": dict(collections.Counter(v["dispatch"] for v in res.values())),
                                         "noop": sum(1 for v in res.values() if v["noop"])},
            "missing": sorted(names - set(res))}


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--image", required=True, type=Path)
    ap.add_argument("--atlas", required=True, type=Path)
    ap.add_argument("--payloads", required=True, type=Path)
    ap.add_argument("--expect-sha256")
    ap.add_argument("--output", required=True, type=Path)
    a = ap.parse_args(argv)
    if a.expect_sha256 and atlas.sha256(a.image) != a.expect_sha256.lower():
        ap.error("image SHA-256 mismatch")
    img = atlas.Image(a.image.read_bytes())
    res = build(img, json.loads(a.atlas.read_text(encoding="utf-8")), json.loads(a.payloads.read_text(encoding="utf-8")))
    res = {"schema": "acu-message-handlers/1", "finding": "ACU-RE-0098", "image_sha256": atlas.sha256(a.image),
           "tool_sha256": atlas.sha256(Path(__file__)), **res}
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(res, indent=1), encoding="utf-8")
    print(json.dumps(res["summary"]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
