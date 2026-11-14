#!/usr/bin/env python3
"""Exact AtomGraph stream parser, transcribed from ACU's per-class Load functions (ACU-RE-0066).

    python tools/atomgraph_stream.py local/atomgraph/NewDemo.bin [--out tree.json] [--stats]

Unlike tools/atomgraph.py (byte-pattern decoding), every read here follows the native loader of
the class being read. ACU.exe SHA-256 748abdc6…7e40, base 0x140000000; loader VAs are listed in
LOADERS. Field names are the member offsets the loader writes (`f80` = `*(a1 + 80)`); meanings are
assigned elsewhere, with evidence. Pseudocode is an analysis artifact: the parser is validated by
consuming whole files exactly (see `--stats`).

Stream primitives (pseudocode of the named functions):
- array count: u32 (sub_14000B2A0), then the elements;
- embedded object, static class (sub_14270BA90): u64 id, u32 class hash, then the static class's
  loader (classinfo + 0x70);
- pointer, dynamic class (sub_14270BEA0 -> sub_14270B650): u8 tag; 0: u64 id, u32 hash, loader of
  the class registered for that hash (constructor at classinfo + 80 = descriptor + 0x30, vtable
  slot 2); 2: u64 reference id; 3: null; other tags read nothing;
- pointer, static loader (sub_14270B920): as above, but tag 0 runs the static class's loader;
- object through an existing vtable (sub_14270BC80): u64 id, u32 hash, loader;
- handle (sub_14270E560): u8 tag; 0: u8, u64 id, u32 hash, loader; 1: u8, u64; else: u8, u64;
- string (sub_14270E8E0): u32 n, then n + 1 bytes when n > 0;
- raw array (sub_14270C020): count * element size bytes.
"""
from __future__ import annotations

import argparse
import json
import struct
import sys
import zlib
from collections import Counter
from pathlib import Path


class StreamError(ValueError):
    pass


def crc(name: str) -> int:
    return zlib.crc32(name.encode()) & 0xFFFFFFFF


# Loader transcriptions. Each entry: (loader VA, ops). Ops run in stream order.
#   ("base", Class)              run another class's ops (the loader's first call)
#   ("u8"|"u16"|"u32"|"u64", f)  scalar written to member f ("f32" = u32 read as float bits)
#   ("raw", n, f)                n raw bytes
#   ("emb", Class, f)            embedded object (BA90)
#   ("embs", Class, f)           u32 count, then embedded objects
#   ("ptr", f) / ("ptrs", f)     dynamic pointer (BEA0) / u32 count then pointers
#   ("sptr", Class, f)           static-loader pointer (B920)
#   ("vobj", f)                  object through its own vtable (BC80), dispatched by stream hash
#   ("handle", f)                handle (E560)
#   ("str", f)                   string (E8E0)
#   ("rawarr", size, f)          u32 count, then count * size bytes
GRAPH_NODE = [("embs", "AtomInputPort", "f8"), ("embs", "AtomOutputPort", "f24"),
              ("emb", "AtomNodeID", "f56"), ("u16", "f72")]
STATE_NODE = [("ptrs", "f16"), ("u32", "f100"), ("emb", "AtomNodeID", "f104"),
              ("embs", "AtomNodeID", "f28"), ("u32", "f120"), ("rawarr", 2, "f40"),
              ("rawarr", 2, "f52"), ("ptrs", "f64"), ("rawarr", 4, "f88")]
LOADERS: dict[str, tuple[int, list]] = {
    # graph and state structure
    "AtomGraph": (0x1400C5320, [("ptr", "f16"), ("ptr", "f24"), ("ptr", "f32"),
                                ("embs", "AtomRTCPDescriptor", "f40"), ("vobj", "f56"),
                                ("sptr", "AtomCustomTransitionSystem", "f96"), ("u32", "f104"),
                                ("u32", "f108"), ("u32", "f112"), ("emb", "ExternalIdMap", "f116")]),
    "AtomStateNode": (0x14010C100, STATE_NODE),
    "AtomNullStateNode": (0x14010C100, [("base", "AtomStateNode")]),
    "AtomGraphStateNode": (0x14010B9B0, [("base", "AtomStateNode"), ("ptrs", "f128"), ("u32", "f152"),
                                         ("u16", "f160"), ("u16", "f162"), ("u16", "f164"),
                                         ("u16", "f166"), ("u16", "f168"), ("u32", "f156"),
                                         ("u8", "f170"), ("rawarr", 4, "f140"), ("u8", "f171")]),
    "AtomStateMachineNode": (0x14010BF10, [("base", "AtomStateNode"), ("ptrs", "f128"),
                                           ("embs", "AtomInitialState", "f140"), ("ptrs", "f152")]),
    "AtomLayeringStateNode": (0x1400C5D70, [("base", "AtomStateNode"), ("emb", "AtomNodeID", "f164"),
                                            ("emb", "AtomLayeringInfo", "f184"), ("u32", "f320"),
                                            ("embs", "AtomLayeringInfo", "f128"), ("ptrs", "f140"),
                                            ("u32", "f324"), ("u32", "f328"), ("rawarr", 4, "f152"),
                                            ("u8", "f332")]),
    "AtomStateDescriptor": (0x14010BE50, [("embs", "AtomStateProperty", "f8"), ("u32", "f20"),
                                          ("u32", "f24")]),
    "AtomStateProperty": (0x14010C400, [("u32", "f0"), ("u32", "f4"),
                                        ("emb", "AtomDataContainerWrapper", "f16"), ("u16", "f8"),
                                        ("u16", "f10"), ("u8", "f12.0"), ("u8", "f12.1")]),
    "AtomInitialState": (0x14010BC60, [("sptr", "AtomConditionExpression", "f0"), ("u16", "f8"),
                                       ("emb", "AtomNodeID", "f10")]),
    "AtomStateTransitionTarget": (0x14010C4E0, [
        ("sptr", "AtomConditionExpression", "f8"), ("u16", "f36"), ("u8", "f38.b11"),
        ("u32", "f38.b0_2"), ("u32", "f38.b5_6"), ("u32", "f38.b7_8"), ("u32", "f38.b9_10"),
        ("u32", "f38.b3_4"), ("u32", "f32.first"), ("f32", "f32"), ("u8", "f38.b15"),
        ("u8", "f38.b13"), ("u8", "f38.b12"), ("emb", "AtomNodeID", "f16")]),
    "AtomCustomTransitionSystem": (0x14010B930, [("embs", "AtomTransitionCellDescription", "f0")]),
    "AtomTransitionCellDescription": (0x14010C870, [
        ("emb", "AtomNodeID", "f20"), ("emb", "AtomNodeID", "f36"), ("emb", "AtomNodeID", "f52"),
        ("emb", "AtomNodeID", "f68"), ("u32", "f8.b0_3"), ("u32", "f8.b4_7"), ("u32", "f8.b8_9"),
        ("u32", "f8.b10_11"), ("u32", "f8.b12_13"), ("u32", "f8.b14_17"), ("u32", "f8.b18_21"),
        ("u32", "f8.b22_23"), ("u32", "f8.b24_25"), ("u32", "f8.b26_27"), ("u32", "f12"),
        ("u32", "f16"), ("ptr", "f0")]),
    "AtomLayeringInfo": (0x1400C5A90, [
        ("emb", "AtomNodeID", "f118"), ("rawarr", 4, "f0"), ("u32", "f80"), ("u8", "f104"),
        ("u32", "f84"), ("rawarr", 4, "f12"), ("rawarr", 4, "f24"), ("rawarr", 4, "f36"),
        ("embs", "BoneWeight", "f106"), ("u32", "f88"), ("u32", "f48"), ("ptr", "f56"),
        ("sptr", "AtomSelectBoneMaskDecisionNode", "f64"), ("handle", "f72"), ("u32", "f92"),
        ("u8", "f105"), ("u32", "f96"), ("u32", "f100")]),
    "AtomSelectBoneMaskDecisionNode": (0x1400C67F0, [
        ("u32", "f0"), ("u32", "f16"), ("embs", "BoneWeight", "f32"), ("u32", "f20"), ("u32", "f24"),
        ("embs", "AtomSelectBoneMaskDecisionNode", "f44"),
        ("sptr", "AtomConditionExpression", "f8"), ("u32", "f28"), ("emb", "AtomNodeID", "f56")]),
    "BoneWeight": (0x140B999C0, [("u32", "f0"), ("u8", "f4")]),
    "AtomRTCPDescriptor": (0x14010BD40, [("str", "f0"), ("str", "f8"), ("u32", "f16"), ("u32", "f20"),
                                         ("emb", "AtomDataContainerWrapper", "f32")]),
    "ExternalIdMap": (0x1400C6F80, [("rawarr", 4, "f0"), ("embs", "ExternalIdHierarchyData", "f12")]),
    "ExternalIdHierarchyData": (0x1400C6EE0, [("embs", "ExternalIdNodeData", "f0")]),
    "ExternalIdNodeData": (0x141E179B0, [("u32", "f0"), ("u32", "f4")]),
    # conditions and values
    "AtomConditionExpression": (0x1400C5060, [("u64", "f0"), ("embs", "AtomCondition", "f20"),
                                              ("rawarr", 4, "f8")]),
    "AtomCondition": (0x1400C4F20, [
        ("u32", "f0"), ("u32", "f4"), ("u32", "f8.b0_2"), ("u32", "f8.b3_6"), ("u32", "f8.b7_8"),
        ("u32", "f8.b9_10"), ("u8", "f8.b11"), ("u16", "f10"), ("u16", "f12"),
        ("emb", "AtomDataContainerWrapper", "f16")]),
    "AtomDataContainerWrapper": (0x1400C5210, [("u32", "type"), ("u32", "i"), ("u32", "j"),
                                               ("raw", 12, "v12"), ("raw", 16, "v16")]),
    "AtomInputPort": (0x1400C5870, [("u32", "kind"), ("u64", "q"), ("u32", "j"), ("raw", 12, "v12"),
                                    ("raw", 16, "v16")]),
    "AtomOutputPort": (0x14204A650, [("u32", "f8")]),
    "AtomNodeID": (0x1400C5FF0, [("raw", 16, "id")]),
    # graph nodes (all start with AtomGraphNode, 0x1400C5530)
    "AtomGraphNode": (0x1400C5530, GRAPH_NODE),
    "AtomLogicalConjunctionNode": (0x1400C5530, GRAPH_NODE),
    "AtomAnimationDataNode": (0x1400C4A30, GRAPH_NODE + [
        ("handle", "f80"), ("u16", "f100"), ("u16", "f102"), ("u16", "f104"), ("u16", "f106"),
        ("u32", "f88"), ("u32", "f92"), ("u32", "f96")]),
    "AtomPlayCustomActionNode": (0x1400C4A30, [("base", "AtomAnimationDataNode")]),
    "AtomAnimationRootNode": (0x1400C4B00, GRAPH_NODE + [
        ("u32", "f80"), ("u32", "f84"), ("u32", "f88"), ("u32", "f92"), ("u8", "f112"), ("u8", "f113"),
        ("u8", "f114"), ("u32", "f96"), ("u32", "f100"), ("u32", "f104"), ("u32", "f108")]),
    "AtomAnimationSequenceNode": (0x1400C4D70, GRAPH_NODE + [
        ("u32", "f80"), ("u32", "f84"), ("u16", "f88"), ("u16", "f90")]),
    "AtomAngularBlendNode": (0x1400C4D70, [("base", "AtomAnimationSequenceNode")]),
    "AtomAxialBlendNode": (0x1400C4CF0, [("base", "AtomAnimationSequenceNode"), ("rawarr", 4, "f96")]),
    "AtomTimeScaleNode": (0x1400C6E90, GRAPH_NODE + [("u16", "f80")]),
    "AtomAddSubtractNode": (0x1400C48E0, GRAPH_NODE + [("u8", "f84"), ("u32", "f80")]),
    "AtomMultiplyDivideNode": (0x1400C48E0, GRAPH_NODE + [("u8", "f84"), ("u32", "f80")]),
    "AtomCacheValueNode": (0x1400C6390, GRAPH_NODE + [("u32", "f80"), ("u32", "f84")]),
    "AtomIfNode": (0x1400C6390, GRAPH_NODE + [("u32", "f80"), ("u32", "f84")]),
    "AtomRagdollBoneControllerNode": (0x1400C6390, GRAPH_NODE + [("u32", "f80"), ("u32", "f84")]),
    "AtomSwitchNode": (0x1400C6390, GRAPH_NODE + [("u32", "f80"), ("u32", "f84")]),
    "AtomWriteParamNode": (0x1400C6390, GRAPH_NODE + [("u32", "f80"), ("u32", "f84")]),
    "AtomCompareValuesNode": (0x1400C4ED0, GRAPH_NODE + [("u32", "f80")]),
    "AtomRemapNode": (0x1400C4ED0, GRAPH_NODE + [("u32", "f80")]),
    "AtomDampRTCPNode": (0x1400C5190, GRAPH_NODE + [("u32", f"f{o}") for o in (80, 84, 88, 92, 96)]),
    "AtomGraphConditionNode": (0x1400C54D0, GRAPH_NODE + [("sptr", "AtomConditionExpression", "f80"),
                                                          ("u32", "f88")]),
    "AtomIKSolveOptionsNode": (0x1400C5810, GRAPH_NODE + [("u8", "f80"), ("u16", "f88")]),
    "AtomBipedFootIKNode": (0x1400C6340, [("base", "AtomIKSolveOptionsNode"), ("u32", "f96")]),
    "AtomModifyIKNode": (0x1400C6340, [("base", "AtomIKSolveOptionsNode"), ("u32", "f96")]),
    "AtomIKPathNode": (0x1400C57B0, [("base", "AtomBipedFootIKNode"), ("u16", "f108"), ("u32", "f104")]),
    "AtomSetIKPathParamNode": (0x1400C6BE0, GRAPH_NODE + [("u8", "f80"), ("u16", "f116")] +
                               [("u32", f"f{o}") for o in range(88, 116, 4)]),
    "AtomIkPathExtension": (0x14010BBB0, [("u16", "f8")] + [("u32", f"f{o}") for o in range(16, 44, 4)] +
                            [("u8", "f44")]),
    "AtomLayerBlendNode": (0x1400C5990, GRAPH_NODE + [("embs", "BoneWeight", "f80")] +
                           [("u32", f"f{o}") for o in range(92, 116, 4)]),
    "AtomRandomSelectionNode": (0x1400C63F0, GRAPH_NODE + [
        ("u8", "f174.b0"), ("u8", "f174.b1"), ("u32", "f174.b2_3"), ("u32", "f140"), ("u32", "f144"),
        ("rawarr", 4, "f80"), ("rawarr", 4, "f92"), ("rawarr", 4, "f104"), ("rawarr", 4, "f116"),
        ("rawarr", 4, "f128")] + [("u32", f"f{o}") for o in range(148, 172, 4)] + [("u16", "f172")]),
    "AtomRawFile": (0x141E6D4E0, []),
    "AtomReadParamNode": (0x1400C66D0, GRAPH_NODE + [("u32", f"f{o}") for o in (80, 84, 88, 92)]),
    "AtomSelectBestMatchingTagsNode": (0x1400C6740, GRAPH_NODE + [("u32", f"f{o}") for o in range(80, 104, 4)] +
                                       [("u16", "f104"), ("u16", "f106")]),
    "AtomSelectFromIntervalNode": (0x1400C69A0, GRAPH_NODE + [("u32", "f84"), ("u32", "f80")]),
    "AtomSelectionNode": (0x1400C6B00, GRAPH_NODE + [("u32", "f80"), ("u32", "f88"), ("u32", "f84")]),
    "AtomSetBipedIKParamsNode": (0x1400C6B60, GRAPH_NODE + [("u8", "f94"), ("u16", "f92"), ("u32", "f80"),
                                                            ("u32", "f84"), ("u32", "f88")]),
    "AtomSetTimeNode": (0x14010BDD0, GRAPH_NODE + [("u32", "f80"), ("u16", "f84"), ("u16", "f86"),
                                                   ("u8", "f88")]),
    "AtomSimpleBlendNode": (0x1400C6D40, GRAPH_NODE + [("u16", "f80"), ("u16", "f82")]),
    "AtomSkeletonPoseNode": (0x1400C6DA0, GRAPH_NODE + [("u16", "f84"), ("u16", "f86"), ("u32", "f80")]),
    "AtomStickyAxialBlendNode": (0x1400C6E10, GRAPH_NODE + [("u32", "f80"), ("u32", "f84"), ("u32", "f88"),
                                                            ("u16", "f92"), ("u16", "f94")]),
}
NAME = {crc(n): n for n in LOADERS}
SCALAR = {"u8": "<B", "u16": "<H", "u32": "<I", "u64": "<Q", "f32": "<f"}


class Reader:
    def __init__(self, b: bytes, names: dict[str, str] | None = None, max_depth: int = 64):
        self.b, self.p, self.names, self.max_depth = b, 0, names or {}, max_depth
        self.count: Counter[str] = Counter()
        self.ids: dict[int, dict] = {}

    def take(self, n: int) -> bytes:
        if n < 0 or self.p + n > len(self.b):
            raise StreamError(f"read of {n} bytes at {self.p} past end {len(self.b)}")
        s = self.b[self.p:self.p + n]
        self.p += n
        return s

    def scalar(self, kind: str):
        fmt = SCALAR[kind]
        return struct.unpack(fmt, self.take(struct.calcsize(fmt)))[0]

    def count32(self, limit: int = 1 << 20) -> int:
        n = self.scalar("u32")
        if n > limit:
            raise StreamError(f"array count {n} at {self.p - 4} exceeds {limit}")
        return n

    def class_of(self, h: int) -> str:
        if h not in NAME:
            raise StreamError(f"no loader for class {self.names.get(f'{h:08x}', f'{h:08x}')} at {self.p - 4}")
        return NAME[h]

    def body(self, cls: str, depth: int, at: int, oid: int, h: int) -> dict:
        if depth > self.max_depth:
            raise StreamError(f"object nesting deeper than {self.max_depth} at {self.p}")
        obj = {"class": cls, "at": at}
        if oid:
            obj["id"] = f"{oid:016x}"
            self.ids[oid] = obj
        if h != crc(cls):
            obj["stream_class"] = self.names.get(f"{h:08x}", f"{h:08x}")
        self.count[cls] += 1
        self.run(cls, obj, depth)
        return obj

    def header(self) -> tuple[int, int, int]:
        at = self.p
        return at, self.scalar("u64"), self.scalar("u32")

    def embedded(self, cls: str, depth: int) -> dict:
        at, oid, h = self.header()
        return self.body(cls, depth, at, oid, h)

    def pointer(self, static: str | None, depth: int):
        tag = self.scalar("u8")
        if tag == 0:
            at, oid, h = self.header()
            return self.body(static or self.class_of(h), depth, at, oid, h)
        if tag == 2:
            return {"ref": f"{self.scalar('u64'):016x}"}
        if tag == 3:
            return None
        return {"tag": tag}

    def handle(self, depth: int):
        tag = self.scalar("u8")
        self.take(1)
        if tag == 0:
            at, oid, h = self.header()
            return self.body(self.class_of(h), depth, at, oid, h)
        return {"handle_tag": tag, "ref": f"{self.scalar('u64'):016x}"}

    def run(self, cls: str, obj: dict, depth: int) -> None:
        for op in LOADERS[cls][1]:
            k = op[0]
            if k == "base":
                self.run(op[1], obj, depth)
            elif k in SCALAR:
                obj[op[1]] = self.scalar(k)
            elif k == "raw":
                obj[op[2]] = self.take(op[1]).hex()
            elif k == "emb":
                obj[op[2]] = self.embedded(op[1], depth + 1)
            elif k == "embs":
                obj[op[2]] = [self.embedded(op[1], depth + 1) for _ in range(self.count32())]
            elif k == "ptr":
                obj[op[1]] = self.pointer(None, depth + 1)
            elif k == "ptrs":
                obj[op[1]] = [self.pointer(None, depth + 1) for _ in range(self.count32())]
            elif k == "sptr":
                obj[op[2]] = self.pointer(op[1], depth + 1)
            elif k == "vobj":
                at, oid, h = self.header()
                obj[op[1]] = self.body(self.class_of(h), depth + 1, at, oid, h)
            elif k == "handle":
                obj[op[1]] = self.handle(depth + 1)
            elif k == "str":
                n = self.count32()
                obj[op[1]] = self.take(n + 1)[:-1].decode("latin-1") if n else ""
            elif k == "rawarr":
                n = self.count32()
                raw = self.take(n * op[1])
                obj[op[2]] = list(struct.unpack(f"<{n}{'H' if op[1] == 2 else 'I'}", raw))
            else:
                raise AssertionError(op)


def parse(b: bytes, names: dict[str, str] | None = None) -> tuple[dict, Reader]:
    """Parse a resource payload: u8 1, u64 id, u32 class hash, then that class's loader."""
    r = Reader(b, names)
    tag = r.scalar("u8")
    if tag != 1:
        raise StreamError(f"resource tag {tag}, expected 1")
    at, oid, h = r.header()
    root = r.body(r.class_of(h), 0, at, oid, h)
    return root, r


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("payload", type=Path)
    ap.add_argument("--names", type=Path, default=Path("local/atomgraph/crc-names.json"))
    ap.add_argument("--out", type=Path)
    ap.add_argument("--stats", action="store_true")
    a = ap.parse_args(argv)
    names = json.loads(a.names.read_text()) if a.names.exists() else {}
    sys.setrecursionlimit(20000)
    b = a.payload.read_bytes()
    root, r = parse(b, names)
    print(f"consumed {r.p} of {len(b)} bytes; {sum(r.count.values())} objects")
    if a.stats:
        for cls, n in r.count.most_common():
            print(f"  {n:8d} {cls}")
    if a.out:
        a.out.write_text(json.dumps(root, separators=(",", ":")))
    return 0 if r.p == len(b) else 1


if __name__ == "__main__":
    raise SystemExit(main())
