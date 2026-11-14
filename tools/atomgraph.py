"""Decode AtomGraph state machines (animation graphs such as NewDemo) into readable rules.

    python -I tools/atomgraph.py params GRAPH.bin [--out params.json]
    python -I tools/atomgraph.py machine GRAPH.bin --at OFFSET [--index local/asset_index.sqlite] [--out m.json]
    python -I tools/atomgraph.py find GRAPH.bin --clip FILE_ID     # state machines holding a clip

GRAPH.bin is a decoded AtomGraph payload (type c0c861cd), e.g. from
`acu-resource "<game>/DataPC.forge" --entry 1518 --fid 000000013d5bc4d8 --len 0 --out NewDemo.bin`.
The graph data is game data: keep it and outputs derived from it local.

Format knowledge (ACU-RE-0063). The byte layouts come from NewDemo. The meanings come from
the runtime evaluators in ACU.exe (base 0x140000000):
- Object header: u8 flag, u64 id, u32 class (CRC-32 of the class name).
- AtomRTCPDescriptor: u32 len, name, NUL, u32 len, type, NUL, u32 crc32(name). A descriptor's
  order in the file is the parameter index that ports and conditions use.
- AtomInputPort: u32 kind, then kind-specific data. Kind 0 = parameter (u32 index).
  Kind 9 = output of another node (u32 index in the state's node list). Kind 3 = constant
  (f32 at +12 from the kind field).
- AtomCondition: u32 param, i32 b, u32 kind (0 = compare, 2 = group with `count` children
  following), u32 op, u32 flag, u32 m8, u8 m12, u16 entity, u8 count. Then an
  AtomDataContainerWrapper: u8 type (0 bool, 1 scalar, 2 int, 10 none), scalar f32 at +4,
  int/bool at +8.
  op: 0 ==, 1 >, 2 >=, 3 <, 4 <=, 5 != (0x14009E7C0 scalar, 0x14009E690 int, 0x14009EE70 bool;
  scalar ==/!= use |a-b| <= 1e-5).
- AtomIfNode: ports (condition, value if true, value if false) (0x1400EB790).
- AtomSelectFromIntervalNode: ports (input, opt0, thr0, opt1, thr1, ..., optN). It picks the
  first k with input < thr_k, else optN (0x1400ECF70 -> 0x1400A0730).
- AtomAnimationDataNode: clip file ID, u64 at +293. Input port 2 is the mirror flag
  (0x1400E1050 writes it to bit 0 of playback+0x37E).
- AtomInitialState: an AtomConditionExpression (u32, u32, u32 condition count, conditions),
  then a u32 that we read as the target state index (inference; see the finding).
"""
from __future__ import annotations

import argparse
import json
import sqlite3
import struct
import sys
import zlib
from dataclasses import dataclass, field
from pathlib import Path

CLASSES = (
    "AtomGraph", "AtomStateMachineNode", "AtomGraphStateNode", "AtomStateNode", "AtomNullStateNode",
    "AtomLayeringStateNode", "AtomInitialState", "AtomStateTransitionTarget",
    "AtomTransitionCellDescription", "AtomStateProperty", "AtomStateDescriptor", "AtomLayeringInfo",
    "AtomIkPathExtension", "AtomCustomTransitionSystem",
    "AtomAnimationDataNode", "AtomAnimationRootNode", "AtomAxialBlendNode", "AtomStickyAxialBlendNode",
    "AtomAngularBlendNode", "AtomSimpleBlendNode", "AtomLayerBlendNode", "AtomIfNode", "AtomSwitchNode",
    "AtomSelectFromIntervalNode", "AtomSelectionNode", "AtomRandomSelectionNode",
    "AtomSelectBestMatchingTagsNode", "AtomSelectBoneMaskDecisionNode", "AtomGraphConditionNode",
    "AtomCacheValueNode", "AtomReadParamNode", "AtomWriteParamNode", "AtomDampRTCPNode",
    "AtomCompareValuesNode", "AtomLogicalConjunctionNode", "AtomMultiplyDivideNode", "AtomAddSubtractNode",
    "AtomRemapNode", "AtomTimeScaleNode", "AtomSetTimeNode", "AtomPlayCustomActionNode",
    "AtomAnimationSequenceNode", "AtomSkeletonPoseNode", "AtomSetBipedIKParamsNode", "AtomSetIKPathParamNode",
    "AtomIKPathNode", "AtomIKSolveOptionsNode", "AtomModifyIKNode", "AtomBipedFootIKNode",
    "AtomRagdollBoneControllerNode", "AtomGraphNode",
    "AtomCondition", "AtomConditionExpression", "AtomDataContainerWrapper",
    "AtomRTCPDescriptor", "AtomInputPort", "AtomOutputPort", "AtomNodeID",
)
CRC = {zlib.crc32(n.encode()) & 0xFFFFFFFF: n for n in CLASSES}
HASH = {n: h for h, n in CRC.items()}
# classes that are parts of a node, not entries of a state's node list
PARTS = {"AtomNodeID", "AtomInputPort", "AtomOutputPort", "AtomCondition", "AtomConditionExpression",
         "AtomDataContainerWrapper", "AtomRTCPDescriptor", "AtomTransitionCellDescription", "AtomStateProperty",
         "AtomStateDescriptor", "AtomLayeringInfo", "AtomIkPathExtension", "AtomCustomTransitionSystem",
         "AtomStateTransitionTarget"}
# entries of a machine's state list (a state can itself be a machine)
STATES = {"AtomGraphStateNode", "AtomStateNode", "AtomNullStateNode", "AtomLayeringStateNode"}
OPS = {0: "==", 1: ">", 2: ">=", 3: "<", 4: "<=", 5: "!="}
# condition kinds other than 0 (compare) and 2 (group), named by their runtime branch in 0x1400C23D0
OPAQUE = {1: "event", 3: "time_window", 4: "event_other"}
JOIN = {0: "and", 1: "or"}
VALUE_TYPES = {0: "bool", 1: "scalar", 2: "int", 10: "none"}
CLIP_AT = 293


class FormatError(ValueError):
    pass


def u32(b: bytes, o: int) -> int:
    if o < 0 or o + 4 > len(b):
        raise FormatError(f"read past end at {o}")
    return struct.unpack_from("<I", b, o)[0]


def objects(b: bytes, lo: int = 0, hi: int | None = None, only: str | None = None) -> list[tuple[int, str]]:
    """Object headers in [lo, hi): (offset of the header, class name).

    The header is one byte, then a u64 id, then the class hash. Every Atom object in
    NewDemo has id 0, so a class hash preceded by eight zero bytes marks an object; the
    first byte varies (0, 3, 0x24, 0x3f, 0x64, ...), and its meaning is unknown."""
    hi = len(b) if hi is None else hi
    zero = bytes(8)
    out = []
    for h, name in ((HASH[only], only),) if only else CRC.items():
        sig, i = struct.pack("<I", h), max(lo + 9, 9)
        while (k := b.find(sig, i, hi)) >= 0:
            i = k + 1
            if b[k - 8:k] == zero:
                out.append((k - 9, name))
    out.sort()
    return out


def params(b: bytes) -> list[dict]:
    sig = struct.pack("<I", HASH["AtomRTCPDescriptor"])
    out, i = [], 0
    while True:
        k = b.find(sig, i)
        if k < 0:
            return out
        i = k + 1
        n = u32(b, k + 4)
        name = b[k + 8:k + 8 + n].decode("latin1")
        t = k + 8 + n + 1
        tn = u32(b, t)
        typ = b[t + 4:t + 4 + tn].decode("latin1")
        crc = u32(b, t + 4 + tn + 1)
        if crc != zlib.crc32(name.encode("latin1")) & 0xFFFFFFFF:
            raise FormatError(f"parameter at {k - 9}: name CRC mismatch")
        out.append({"index": len(out), "offset": k - 9, "name": name, "type": typ})


def condition(b: bytes, o: int) -> dict:
    """One AtomCondition at object offset o (the header's first byte)."""
    if u32(b, o + 9) != HASH["AtomCondition"]:
        raise FormatError(f"no AtomCondition at {o}")
    p = o + 13
    param, bb, kind = struct.unpack_from("<IiI", b, p)
    op, combine, m8 = struct.unpack_from("<III", b, p + 12)
    m12, entity, count = b[p + 24], struct.unpack_from("<H", b, p + 25)[0], b[p + 27]
    w = p + 28
    if u32(b, w + 9) != HASH["AtomDataContainerWrapper"]:
        raise FormatError(f"condition at {o}: no value wrapper at {w}")
    t = w + 13
    vt = b[t]
    if vt == 1:
        value = round(struct.unpack_from("<f", b, t + 4)[0], 6)
    elif vt == 2:
        value = struct.unpack_from("<i", b, t + 8)[0]
    elif vt == 0:
        value = bool(b[t + 8])
    elif vt == 10:
        value = None
    else:
        raise FormatError(f"condition at {o}: unsupported value type {vt}")
    if combine not in JOIN:
        raise FormatError(f"condition at {o}: unknown combine value {combine}")
    if kind == 0 and op not in OPS:
        raise FormatError(f"condition at {o}: unknown operator {op}")
    return {"offset": o, "param": param, "b": bb, "kind": kind, "op": op, "combine": combine, "m8": m8,
            "m12": m12, "entity": entity, "count": count, "value_type": VALUE_TYPES.get(vt, vt), "value": value}


def expression(conds: list[dict], names: dict[int, str]) -> str:
    """Flat condition list -> text. A group header (kind 2) owns the next `count` entries.

    Entries are chained left to right with no precedence: each entry's `combine` joins it
    to the NEXT sibling (0 and, 1 or), as in the runtime loop at 0x1400C3400. The mapping of
    the serialized field onto that runtime field is an inference (ACU-RE-0063)."""
    pos = 0

    def chain(n: int | None) -> str:
        nonlocal pos
        text, join, k = None, None, 0
        while (n is None and pos < len(conds)) or (n is not None and k < n):
            if pos >= len(conds):
                raise FormatError("condition group runs past the end of its list")
            c, k = conds[pos], k + 1
            pos += 1
            if c["kind"] == 2:
                item = chain(c["count"])
                item = f"({item})" if c["count"] > 1 else item
            elif c["kind"] in OPAQUE:
                # evaluated by other runtime paths (0x1400C23D0); the ID's meaning is not decoded
                item = f"{OPAQUE[c['kind']]}(0x{c['param']:08x})"
            elif c["kind"] == 0:
                item = f"{names.get(c['param'], '#%d' % c['param'])} {OPS[c['op']]} {c['value']}"
            else:
                raise FormatError(f"condition at {c['offset']}: unsupported kind {c['kind']}")
            text = item if text is None else f"{text} {join} {item}"
            join = JOIN[c["combine"]]
        return text or "true"

    return chain(None)


def ports(b: bytes, lo: int, hi: int) -> list[dict]:
    sig = struct.pack("<I", HASH["AtomInputPort"])
    out, i = [], lo
    while True:
        k = b.find(sig, i, hi)
        if k < 0:
            return out
        i = k + 1
        kind, data = u32(b, k + 4), u32(b, k + 8)
        if kind == 9:
            out.append({"link": data})
        elif kind == 0:
            out.append({"param": data})
        elif kind == 3:
            out.append({"const": round(struct.unpack_from("<f", b, k + 16)[0], 6)})
        else:
            out.append({"kind": kind, "data": data})


@dataclass
class Node:
    index: int
    offset: int
    cls: str
    end: int
    ports: list = field(default_factory=list)
    clip: str | None = None
    conditions: list = field(default_factory=list)


@dataclass
class State:
    index: int
    offset: int
    cls: str
    nodes: list[Node] = field(default_factory=list)
    machine: list["State"] | None = None
    entry_conditions: list = field(default_factory=list)
    has_entry: bool = False  # an empty entry_conditions list means "always" (0x1400C3400)


def machine(b: bytes, at: int, scan: int = 2_000_000) -> list[State]:
    """The state machine whose AtomStateMachineNode header starts at `at`."""
    objs = objects(b, at, min(len(b), at + scan))
    level = [(o, c) for o, c in objs if c not in PARTS]
    if not level or level[0] != (at, "AtomStateMachineNode"):
        raise FormatError(f"no AtomStateMachineNode at {at}")
    states, _ = _machine(b, level, 0, 0)
    return states


def expression_block(b: bytes, expr: int, end: int) -> tuple[list[dict], list[int], int]:
    """An AtomConditionExpression at `expr` -> (conditions, compared parameters, offset after it).

    Reflection (ACU-RE-0065) gives its fields: u32, the `Conditions` array of AtomCondition,
    then an array of u32 parameter indices. Serialized: u32, u32 (array metadata; 0x5000ea8b
    when filled, 0 in some empty arrays), u32 count, `count` AtomCondition objects (a flat
    list: group entries count too), then u32 n and n parameter indices. Each condition's
    wrapper ends with f32 1.0 (object start + 49). The count bounds the list, so conditions of
    transitions that follow are never taken."""
    count = u32(b, expr + 21)
    if count > 4096:
        raise FormatError(f"condition expression at {expr}: implausible count {count}")
    if count == 0:
        at = expr + 25
        n = u32(b, at)
        if n != 0:
            raise FormatError(f"condition expression at {expr}: parameters without conditions")
        return [], [], at + 4
    found = objects(b, expr, end, only="AtomCondition")[:count]
    if len(found) != count:
        raise FormatError(f"condition expression at {expr}: {len(found)} of {count} conditions")
    conds = [condition(b, x) for x, _ in found]
    wrap = conds[-1]["offset"] + 13 + 28
    if b[wrap + 49:wrap + 53] != struct.pack("<f", 1.0):
        raise FormatError(f"condition expression at {expr}: unexpected bytes after the last condition")
    n = u32(b, wrap + 53)
    if n > 64:
        raise FormatError(f"condition expression at {expr}: implausible parameter count {n}")
    reads = [u32(b, wrap + 57 + 4 * k) for k in range(n)]
    used = {c["param"] for c in conds if c["kind"] == 0}
    if set(reads) != used:
        raise FormatError(f"condition expression at {expr}: parameter list {reads} != compared {sorted(used)}")
    return conds, reads, wrap + 57 + 4 * n


def _initial(b: bytes, o: int, end: int) -> tuple[list[dict], int]:
    """An AtomInitialState at `o` -> (entry conditions, target state index).

    Reflection (ACU-RE-0065): AtomInitialState = AtomConditionExpression, a 1-byte target
    (type code 5), AtomNodeID; the target byte directly follows the expression and the
    AtomNodeID header follows it. An empty condition list holds: 0x1400C3400 returns 0
    (true) when its count is 0."""
    expr = objects(b, o, min(end, o + 64), only="AtomConditionExpression")
    if not expr or expr[0][0] != o + 13:
        raise FormatError(f"initial state at {o}: no condition expression")
    conds, _, at = expression_block(b, o + 13, end)
    if not any(x == at + 1 for x, _ in objects(b, at, min(end, at + 32), only="AtomNodeID")):
        raise FormatError(f"initial state at {o}: AtomNodeID does not follow the target byte")
    return conds, b[at]


def _machine(b: bytes, level: list[tuple[int, str]], i: int, depth: int) -> tuple[list[State], int]:
    """Parse level[i] (a machine) -> (states, index of the first entry after it).

    Layout (NewDemo): header, then states in order (each a node list closed by an
    AtomAnimationRootNode, or a nested machine), then its AtomInitialState entry rules.
    A machine nested as a state is followed directly by the enclosing machine's further
    states or rules, so a machine takes rules only while each targets one of its own
    states that has no rule yet; the rest belong to the enclosing machine."""
    if depth > 16:
        raise FormatError("state machines nest too deeply")
    at = level[i][0]
    states: list[State] = []
    open_state: State | None = None
    j = i + 1
    while j < len(level):
        o, c = level[j]
        if c == "AtomInitialState":
            break
        if c in STATES:
            open_state = State(len(states), o, c)
            states.append(open_state)
            if c != "AtomGraphStateNode":
                open_state = None  # no node list decoded for these state kinds
        elif c == "AtomStateMachineNode":
            if open_state is not None:
                raise FormatError(f"state machine at {o} inside a node list (unsupported)")
            sub, j = _machine(b, level, j, depth + 1)
            states.append(State(len(states), o, c, machine=sub))
            continue
        elif open_state is not None:
            open_state.nodes.append(Node(len(open_state.nodes), o, c, 0))
            if c == "AtomAnimationRootNode":
                open_state = None
        elif states:
            raise FormatError(f"machine at {at}: {c} at {o} outside any state")
        j += 1
    if not states:
        raise FormatError(f"state machine at {at}: no states")
    while j < len(level) and level[j][1] == "AtomInitialState":
        o = level[j][0]
        conds, target = _initial(b, o, level[j + 1][0] if j + 1 < len(level) else len(b))
        if target >= len(states) or states[target].has_entry:
            break
        states[target].entry_conditions = conds
        states[target].has_entry = True
        j += 1
    bounds = [o for o, _ in level] + [len(b)]
    for s in states:
        for n in s.nodes:
            n.end = next(x for x in bounds if x > n.offset)
            n.ports = ports(b, n.offset, n.end)
            if n.cls == "AtomAnimationDataNode":
                n.clip = f"{struct.unpack_from('<Q', b, n.offset + CLIP_AT)[0]:016x}"
            if n.cls == "AtomGraphConditionNode":
                n.conditions = [condition(b, x) for x, _ in objects(b, n.offset, n.end, only="AtomCondition")]
    return states, j


def clip_names(index: Path | None, ids: set[str]) -> dict[str, str]:
    if not index or not index.exists() or not ids:
        return {}
    db = sqlite3.connect(f"file:{index.as_posix()}?mode=ro", uri=True)
    q = f"SELECT DISTINCT file_id, name FROM resource WHERE file_id IN ({','.join('?' * len(ids))})"
    return {fid: name for fid, name in db.execute(q, sorted(ids))}


def all_clips(states: list[State]) -> set[str]:
    return {n.clip for s in states for n in s.nodes if n.clip} | {
        c for s in states if s.machine for c in all_clips(s.machine)}


def describe(states: list[State], names: dict[int, str], clips: dict[str, str]) -> list[dict]:
    out = []
    for s in states:
        entry = (expression(s.entry_conditions, names) if s.entry_conditions
                 else "always" if s.has_entry else None)
        if s.machine is not None:
            out.append({"state": s.index, "offset": s.offset, "entry": entry,
                        "machine": describe(s.machine, names, clips)})
            continue
        if s.cls != "AtomGraphStateNode":
            out.append({"state": s.index, "offset": s.offset, "entry": entry, "kind": s.cls})
            continue

        def val(n: Node, depth: int = 0) -> object:
            if depth > 32:
                raise FormatError("node links nest too deeply")
            if n.cls == "AtomAnimationDataNode":
                name = clips.get(n.clip, n.clip)
                flag = n.ports[2] if len(n.ports) == 4 and n.ports[2].get("kind") == 2 else None
                # port 2 is the playback mirror bit: 0x1400E1050 stores it in bit 0 of playback+0x37E
                return f"{name} [mirror={flag['data']}]" if flag else name
            if n.cls == "AtomCacheValueNode":
                p = n.ports[0] if n.ports else {}
                return {"param": names.get(p.get("param"), p)}
            if n.cls == "AtomGraphConditionNode":
                return {"condition": expression(n.conditions, names)}
            if n.cls == "AtomIfNode":
                c, t, f = (link(n, p, depth) for p in n.ports[:3])
                return {"if": c, "then": t, "else": f}
            if n.cls == "AtomSelectFromIntervalNode":
                x, rest = link(n, n.ports[0], depth), n.ports[1:]
                opts = [link(n, p, depth) for p in rest[0::2]]
                thr = [p.get("const") for p in rest[1::2]]
                return {"select_by": x, "below": [{"threshold": t, "value": v} for t, v in zip(thr, opts)],
                        "otherwise": opts[-1]}
            if n.cls == "AtomAxialBlendNode" and len(n.ports) >= 5 and len(n.ports) % 2 == 1:
                # ports: input, two settings, then (point, value) pairs (ACU-RE-0063; the point
                # interpretation is an inference from the ascending constants)
                x, rest = link(n, n.ports[0], depth), n.ports[3:]
                return {"blend_by": x, "settings": n.ports[1:3],
                        "points": [{"at": t.get("const"), "value": link(n, v, depth)}
                                   for t, v in zip(rest[0::2], rest[1::2])]}
            if n.cls == "AtomTimeScaleNode" and len(n.ports) == 3 and "const" in n.ports[1]:
                # ports: kind-2 setting, constant, input; reading the constant as a playback-rate
                # factor follows the class name and is not checked against the runtime
                return {"time_scale": n.ports[1]["const"], "setting": n.ports[0], "of": link(n, n.ports[2], depth)}
            return {"node": n.cls, "ports": n.ports}

        def link(n: Node, p: dict, depth: int) -> object:
            if "link" in p:
                if p["link"] >= len(s.nodes):
                    raise FormatError(f"node at {n.offset}: link {p['link']} out of range")
                return val(s.nodes[p["link"]], depth + 1)
            if "param" in p:
                return {"param": names.get(p["param"], p["param"])}
            return p

        roots = [n for n in s.nodes if n.cls == "AtomAnimationRootNode"]
        out.append({
            "state": s.index, "offset": s.offset, "entry": entry,
            "clips": sorted({clips.get(n.clip, n.clip) for n in s.nodes if n.clip}),
            "output": [link(r, r.ports[0], 0) for r in roots if r.ports],
        })
    return out


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("cmd", choices=("params", "machine", "find"))
    ap.add_argument("graph", type=Path)
    ap.add_argument("--at", type=int)
    ap.add_argument("--clip")
    ap.add_argument("--index", type=Path, default=Path("local/asset_index.sqlite"))
    ap.add_argument("--out", type=Path)
    a = ap.parse_args(argv)
    b = a.graph.read_bytes()
    if a.cmd == "params":
        res: object = params(b)
    elif a.cmd == "find":
        if not a.clip:
            ap.error("find needs --clip")
        pat = struct.pack("<Q", int(a.clip, 16))
        sms = [o for o, _ in objects(b, only="AtomStateMachineNode")]
        res, i = [], 0
        while True:
            k = b.find(pat, i)
            if k < 0:
                break
            i = k + 1
            owner = max((o for o in sms if o < k), default=None)
            res.append({"offset": k, "machine": owner})
    else:
        if a.at is None:
            ap.error("machine needs --at")
        names = {p["index"]: p["name"] for p in params(b)}
        states = machine(b, a.at)
        clips = clip_names(a.index, all_clips(states))
        res = describe(states, names, clips)
    text = json.dumps(res, indent=1)
    if a.out:
        a.out.parent.mkdir(parents=True, exist_ok=True)
        a.out.write_text(text, encoding="utf-8")
    else:
        sys.stdout.write(text + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
