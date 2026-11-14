#!/usr/bin/env python3
"""Build a provenance-tracked symbol index for the ACU pseudocode corpus.

The index is derived from the game's own runtime class reflection, read out of a
captured *mapped* main image (laid out by RVA, so data globals are populated):

* A class-name getter is ``48 8B 05 rel32 C3`` (``mov rax,[rip+X]; ret``). ``[X]`` is a
  class-info record and ``[classinfo+0x18]`` points to its C-string name.
* A vtable is a qword array whose slot 5 (``+0x28``) is such a getter.
* The class info also carries a parent pointer, CRC-32 type ids, the object size, an
  isA interval and a 0x38-byte property-descriptor array (see ACU-RE-0060).

Output is an ignored SQLite DB (default ``local/symbol_index.sqlite``). Corpus files
are only listed, never read or modified. Every proposed name carries a source and a
confidence; slot numbers are not method names. Run with ``--help`` for options.

    python tools/symbol_index.py --image <main-image.mapped.bin> [--findings data/symbol_names_findings.json]
"""
from __future__ import annotations

import argparse
import array
import bisect
import collections
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import struct
import sys
import zlib

sys.path.insert(0, str(Path(__file__).resolve().parent))
from native_image import Image  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_DB = ROOT / "local" / "symbol_index.sqlite"
DEFAULT_FINDINGS = ROOT / "data" / "symbol_names_findings.json"
SCHEMA_VERSION = 1

# Class-info layout (observed on Entity/BipedComponent/AtomAnimComponent/SkeletonComponent
# and checked across the whole scan by the CRC and isA-nesting statistics; ACU-RE-0060).
CI_PROPS, CI_ENUMS, CI_NAME, CI_CRC_PARENT, CI_CRC_OWN = 0x00, 0x08, 0x18, 0x20, 0x24
CI_SIZE, CI_FLAGS, CI_PARENT, CI_INDEX, CI_ISA_LO, CI_ISA_HI, CI_FACTORY = (
    0x28, 0x2C, 0x38, 0x48, 0x4C, 0x4E, 0x50)
CI_LEN = 0x58
ISA_UNASSIGNED = 0x7FFF  # isA interval of classes seen unregistered in this capture
PROP_STRIDE = 0x38
GETTER_SLOT = 5
MAX_SLOTS = 2048
MAX_PROPS = 4096

GETTER_RE = re.compile(rb"\x48\x8b\x05(....)\xc3", re.S)
LEA_RE = re.compile(rb"[\x48\x4c]\x8d[\x05\x0d\x15\x1d\x25\x2d\x35\x3d](....)", re.S)
BRANCH_RE = re.compile(rb"[\xe8\xe9](....)", re.S)
CLASS_NAME_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_:<>,* ]{0,127}$")
IDENT_RE = re.compile(rb"[A-Za-z_][A-Za-z0-9_:]{2,63}(?=\x00)")


def hexva(va):
    return None if va is None else f"0x{va:X}"


class View:
    """Bounded VA reads over a mapped image. Every read checks its range."""

    def __init__(self, image: Image, base: int | None = None):
        if not image.mapped:
            raise ValueError("symbol index requires a mapped (RVA-laid-out) image")
        self.image, self.data = image, image.data
        self.base = image.base if base is None else base
        self.end = self.base + min(image.size, len(image.data))
        self.exec_ranges = sorted(
            (self.base + s["rva"], self.base + s["rva"] + s["virtual_size"])
            for s in image.sections if s["flags"] & 0x20000000)
        self.exec_starts = [r[0] for r in self.exec_ranges]

    def ok(self, va, size=8):
        return isinstance(va, int) and self.base <= va and va + size <= self.end

    def u64(self, va):
        if not self.ok(va, 8):
            raise ValueError(f"VA {va:#x} outside image")
        return struct.unpack_from("<Q", self.data, va - self.base)[0]

    def u32(self, va):
        if not self.ok(va, 4):
            raise ValueError(f"VA {va:#x} outside image")
        return struct.unpack_from("<I", self.data, va - self.base)[0]

    def u16(self, va):
        if not self.ok(va, 2):
            raise ValueError(f"VA {va:#x} outside image")
        return struct.unpack_from("<H", self.data, va - self.base)[0]

    def bytes(self, va, size):
        if not self.ok(va, size):
            raise ValueError(f"VA {va:#x} outside image")
        return self.data[va - self.base:va - self.base + size]

    def executable(self, va):
        i = bisect.bisect_right(self.exec_starts, va) - 1
        return i >= 0 and va < self.exec_ranges[i][1] and self.ok(va, 1)

    def cstr(self, va, limit=128):
        if not self.ok(va, 1):
            return None
        o = va - self.base
        e = self.data.find(b"\0", o, min(o + limit + 1, len(self.data)))
        if e < 0:
            return None
        try:
            return self.data[o:e].decode("ascii")
        except UnicodeDecodeError:
            return None

    def section(self, name):
        s = next((s for s in self.image.sections if s["name"] == name), None)
        if s is None:
            return None, b""
        size = min(s["virtual_size"], len(self.data) - s["rva"])
        return self.base + s["rva"], self.data[s["rva"]:s["rva"] + max(size, 0)]


# ---------------------------------------------------------------- class info

def class_name_at(view, ci):
    try:
        name = view.cstr(view.u64(ci + CI_NAME))
    except ValueError:
        return None
    return name if name and CLASS_NAME_RE.match(name) else None


def read_classinfo(view, ci):
    """Decode the fixed class-info header, or None if the record does not validate."""
    if not view.ok(ci, CI_LEN) or view.executable(ci):
        return None
    name = class_name_at(view, ci)
    if name is None:
        return None
    parent = view.u64(ci + CI_PARENT)
    crc_own = view.u32(ci + CI_CRC_OWN)
    return dict(
        ci=ci, name=name, parent=parent if parent and class_name_at(view, parent) else None,
        crc_own=crc_own, crc_parent=view.u32(ci + CI_CRC_PARENT),
        crc_ok=int(crc_own == zlib.crc32(name.encode("ascii"))),
        size=view.u32(ci + CI_SIZE), flags=view.u32(ci + CI_FLAGS),
        index=view.u32(ci + CI_INDEX), isa_lo=view.u16(ci + CI_ISA_LO),
        isa_hi=view.u16(ci + CI_ISA_HI), props=view.u64(ci + CI_PROPS),
        enums=view.u64(ci + CI_ENUMS), factory=view.u64(ci + CI_FACTORY))


def find_getters(view):
    """Return {getter_va: (global_va, classinfo_va, name)} for validated class getters."""
    text_va, text = view.section(".text")
    found = {}
    for m in GETTER_RE.finditer(text):
        getter = text_va + m.start()
        target = getter + 7 + struct.unpack("<i", m.group(1))[0]
        if not view.ok(target) or view.executable(target):
            continue
        ci = view.u64(target)
        info = read_classinfo(view, ci)
        if info is not None:
            found[getter] = (target, ci, info["name"])
    return found


def collect_classes(view, getters):
    """Class records keyed by classinfo VA, including parents reached only by chain."""
    classes = {}
    for getter, (glob, ci, _name) in sorted(getters.items()):
        rec = classes.get(ci)
        if rec is None:
            rec = classes[ci] = read_classinfo(view, ci)
            rec.update(getters=[], globals=set(), source="getter")
        rec["getters"].append(getter)
        rec["globals"].add(glob)
    pending = [r["parent"] for r in classes.values() if r["parent"]]
    while pending:
        ci = pending.pop()
        if ci in classes:
            continue
        rec = read_classinfo(view, ci)
        if rec is None:
            continue
        rec.update(getters=[], globals=set(), source="parent-chain")
        classes[ci] = rec
        if rec["parent"]:
            pending.append(rec["parent"])
    for rec in classes.values():
        rec["parent_source"] = "pointer" if rec["parent"] else None
    # Some class infos carry a null parent pointer in this capture but a non-zero parent
    # CRC; link those through the CRC when exactly one CRC-validated class owns it.
    by_crc = collections.defaultdict(list)
    for rec in classes.values():
        if rec["crc_ok"]:
            by_crc[rec["crc_own"]].append(rec["ci"])
    for rec in classes.values():
        if rec["parent"] is None and rec["crc_parent"] and len(by_crc.get(rec["crc_parent"], ())) == 1:
            parent = by_crc[rec["crc_parent"]][0]
            if parent != rec["ci"]:
                rec["parent"], rec["parent_source"] = parent, "crc"
    for rec in classes.values():  # depth with cycle guard
        depth, cur, seen = 0, rec["parent"], {rec["ci"]}
        while cur in classes and cur not in seen:
            seen.add(cur)
            depth += 1
            cur = classes[cur]["parent"]
        rec["depth"] = depth
        rec["parent_cycle"] = int(cur in seen)
    return classes


def ancestors(classes, ci):
    out, cur, seen = [], classes[ci]["parent"], {ci}
    while cur in classes and cur not in seen:
        out.append(cur)
        seen.add(cur)
        cur = classes[cur]["parent"]
    return out


# ---------------------------------------------------------------- properties

def decode_accessor(code: bytes):
    """Return the ``[rcx+disp]`` displacement of a one-instruction accessor ending in ret.

    Accepts mov/movzx/movsx/lea/movss/movsd/movaps/movups with a disp8/disp32 rcx base
    and no SIB. Anything else returns None (unknown, not a mismatch)."""
    i = 0
    while i < len(code) and code[i] in (0x66, 0xF2, 0xF3):
        i += 1
    rex = 0
    if i < len(code) and 0x40 <= code[i] <= 0x4F:
        rex, i = code[i], i + 1
    if i >= len(code):
        return None
    op = code[i]
    if op == 0x0F:
        if i + 1 >= len(code) or code[i + 1] not in (0x10, 0x11, 0x28, 0x29, 0xB6, 0xB7, 0xBE, 0xBF):
            return None
        i += 2
    elif op in (0x88, 0x89, 0x8A, 0x8B, 0x8D):
        i += 1
    else:
        return None
    if i >= len(code) or rex & 1:
        return None
    modrm = code[i]
    mod, rm = modrm >> 6, modrm & 7
    if rm != 1 or mod not in (1, 2):
        return None
    i += 1
    width = 1 if mod == 1 else 4
    if i + width >= len(code):
        return None
    disp = int.from_bytes(code[i:i + width], "little", signed=True)
    return disp if code[i + width] == 0xC3 and disp >= 0 else None


def entry_valid(view, entry):
    if not view.ok(entry, PROP_STRIDE) or view.executable(entry):
        return False
    nhash = view.u32(entry + 4)
    fns = struct.unpack("<4Q", view.bytes(entry + 0x18, 0x20))
    return nhash != 0 and not any(f and not view.executable(f) for f in fns)


def property_count(view, rec, bounds):
    """Entry count of the property array and how it was bounded.

    No count field has been identified in the class info. Arrays sit in pools next to
    other arrays and class infos, so an array is bounded by the next known class-info or
    property-array address (``bounds``, sorted) and every entry must validate (non-zero
    name hash; accessor pointers null or executable). Returns (count, method)."""
    props = rec["props"]
    if not props:
        return 0, "null"
    if not view.ok(props, PROP_STRIDE) or view.executable(props):
        return None, "pointer-out-of-image"
    j = bisect.bisect_right(bounds, props)
    limit = bounds[j] if j < len(bounds) else props + MAX_PROPS * PROP_STRIDE
    room = min((limit - props) // PROP_STRIDE, MAX_PROPS)
    for count in range(room):
        if not entry_valid(view, props + count * PROP_STRIDE):
            return count, "invalid-entry"
    return room, "next-known-address" if j < len(bounds) else "max-props"


def read_properties(view, rec, crc_names, class_by_crc, bounds=(), pointer_globals=frozenset()):
    count, method = property_count(view, rec, bounds)
    rec["prop_count"], rec["prop_count_method"] = count, method
    rows = []
    for idx in range(count or 0):
        entry = rec["props"] + idx * PROP_STRIDE
        flags, nhash, thash, tword, packed, unk = struct.unpack("<6I", view.bytes(entry, 0x18))
        fns = list(struct.unpack("<4Q", view.bytes(entry + 0x18, 0x20)))
        type_code = tword >> 16
        offset = packed >> 18 if (packed & 0xFFFF) == type_code else None
        if offset == 0 and any(fns):
            offset = None  # accessor-backed property: packed carries no storage offset
        acc = None
        for f in dict.fromkeys(f for f in fns if f):
            acc = decode_accessor(view.bytes(f, 16)) if view.ok(f, 16) else None
            if acc is not None:
                break
        check = None
        if offset and acc is not None:
            check = "match" if acc == offset else "differs"
        name = crc_names.get(nhash)
        tname = class_by_crc.get(thash) or crc_names.get(thash) if thash else None
        rows.append(dict(entry=entry, idx=idx, flags=flags, name_hash=nhash, name=name,
                         name_source="crc32-image-string" if name else None,
                         type_hash=thash, type_name=tname, type_code=type_code, type_word=tword,
                         packed=packed, unk=unk, offset=offset,
                         offset_source="packed>>18" if offset is not None else None,
                         accessor_offset=acc, offset_check=check, fns=fns,
                         pointer_global=int(entry in pointer_globals)))
    rec["prop_entries_pointed"] = sum(r["pointer_global"] for r in rows)
    return rows


def crc_dictionary(view):
    """CRC-32 -> identifier for NUL-terminated identifiers in .rdata and .data."""
    names = {}
    for sec in (".rdata", ".data"):
        _va, blob = view.section(sec)
        for m in IDENT_RE.finditer(blob):
            names.setdefault(zlib.crc32(m.group()), m.group().decode("ascii"))
    return names


# ---------------------------------------------------------------- vtables

def lea_targets(view):
    text_va, text = view.section(".text")
    out = set()
    for m in LEA_RE.finditer(text):
        out.add(text_va + m.start() + 7 + struct.unpack("<i", m.group(1))[0])
    return out


def find_vtables(view, getters, classes, referenced):
    """Vtables whose slot 5 is a validated getter. Bounds are heuristic (no RTTI)."""
    getter_set = set(getters)
    candidates = {}
    for sec in (".rdata", ".data"):
        sec_va, blob = view.section(sec)
        words = array.array("Q")
        words.frombytes(blob[:len(blob) // 8 * 8])
        for i, word in enumerate(words):
            if word in getter_set and i >= GETTER_SLOT:
                start = sec_va + (i - GETTER_SLOT) * 8
                slots = [words[i - GETTER_SLOT + k] for k in range(GETTER_SLOT + 1)]
                if all(view.executable(s) for s in slots):
                    candidates[start] = (sec_va, words, i - GETTER_SLOT)
    starts = set(candidates)
    vtables = {}
    for start, (sec_va, words, first) in sorted(candidates.items()):
        slots, reason = [], "section-end"
        for k in range(MAX_SLOTS):
            j = first + k
            if j >= len(words):
                break
            va = sec_va + j * 8
            if k > 0 and (va in starts or va in referenced):
                reason = "next-vtable" if va in starts else "next-referenced-address"
                break
            if not view.executable(words[j]):
                reason = "non-code-pointer"
                break
            slots.append(words[j])
        else:
            reason = "max-slots"
        getter = slots[GETTER_SLOT]
        ci = getters[getter][1]
        vtables[start] = dict(va=start, ci=ci, name=classes[ci]["name"], slots=slots,
                              end_reason=reason, referenced=int(start in referenced))
    return vtables


def classify_methods(vtables, classes):
    by_class = collections.defaultdict(list)
    for vt in vtables.values():
        by_class[vt["ci"]].append(vt)
    rows = []
    for vt in vtables.values():
        parent = classes[vt["ci"]]["parent"]
        pvts = by_class.get(parent, [])
        pmax = max((len(p["slots"]) for p in pvts), default=0)
        for slot, fn in enumerate(vt["slots"]):
            if not pvts:
                origin = "unknown"
            elif any(slot < len(p["slots"]) and p["slots"][slot] == fn for p in pvts):
                origin = "inherited"
            elif slot < pmax:
                origin = "own"
            else:
                origin = "new-slot"
            rows.append((fn, vt["ci"], vt["va"], slot, origin))
    return rows


def name_vtable_functions(method_rows, classes):
    """reflection-vtable proposals: {fn: (name, confidence, detail)}."""
    users = collections.defaultdict(set)
    for fn, ci, _vt, slot, _origin in method_rows:
        users[fn].add((ci, slot))
    out = {}
    for fn, uses in users.items():
        cis = {ci for ci, _ in uses}
        slots = sorted({slot for _, slot in uses})
        names = sorted(classes[ci]["name"] for ci in cis)
        detail = dict(classes=names[:40], class_count=len(cis), slots=slots[:40])
        if len(slots) > 1:
            out[fn] = (f"folded_vf_{fn:X}", "low", dict(detail, reason="same code at several slots"))
            continue
        slot = slots[0]
        base = [ci for ci in cis if all(o == ci or ci in ancestors(classes, o) for o in cis)]
        if base:
            conf = "high"
            pick = base[0]
        else:
            conf = "medium"  # siblings share identical code; base-most by depth, then name
            pick = min(cis, key=lambda c: (classes[c]["depth"], classes[c]["name"]))
            detail["reason"] = "users are not one inheritance line"
        out[fn] = (f"{classes[pick]['name']}::vf{slot}", conf, dict(detail, base_class=classes[pick]["name"]))
    return out


# ---------------------------------------------------------------- functions and calls

def pdata_primary_ranges(view):
    """[(start, end, primary_start)] from the exception directory, chains resolved."""
    dirs = view.image.directories
    if len(dirs) <= 3 or not dirs[3][1]:
        return []
    rva, size = dirs[3]
    if size % 12 or size > 64 * 1024 * 1024:
        raise ValueError("invalid exception directory size")
    table = view.image.read(rva, size)
    rows = [r for r in struct.iter_unpack("<III", table) if r[0] and r[1] > r[0]]
    parent = {}
    for start, _end, unwind in rows:
        try:
            info = view.bytes(view.base + unwind, 4)
            if info[0] >> 3 & 4:  # UNW_FLAG_CHAININFO
                at = view.base + unwind + 4 + ((info[2] + 1) & ~1) * 2
                parent[start] = view.u32(at)
        except ValueError:
            continue
    out = []
    for start, end, _unwind in rows:
        cur, seen = start, set()
        while cur in parent and cur not in seen:
            seen.add(cur)
            cur = parent[cur]
        out.append((view.base + start, view.base + end, view.base + cur))
    out.sort()
    return out


def call_edges(view, ranges, starts):
    """(caller, callee, kind) for E8/E9 rel32 whose target is a known function start.

    Linear byte scan: a site inside another instruction can still hit a start; these
    edges are candidates. Callers come from .pdata (chain-resolved) or, outside any
    range, the nearest known start within 16 bytes (thunks without unwind data)."""
    text_va, text = view.section(".text")
    rstarts = [r[0] for r in ranges]
    sorted_starts = sorted(starts)
    edges = set()
    for m in BRANCH_RE.finditer(text):
        site = text_va + m.start()
        target = site + 5 + struct.unpack("<i", m.group(1))[0]
        if target not in starts:
            continue
        i = bisect.bisect_right(rstarts, site) - 1
        if i >= 0 and site < ranges[i][1]:
            caller = ranges[i][2]
        else:
            j = bisect.bisect_right(sorted_starts, site) - 1
            if j < 0 or site - sorted_starts[j] > 16:
                continue
            caller = sorted_starts[j]
        if caller != target:
            edges.add((caller, target, "call" if m.group()[0] == 0xE8 else "jmp"))
    return edges


def corpus_functions(corpus):
    if corpus is None or not Path(corpus).is_dir():
        return None
    out = set()
    for name in os.listdir(corpus):
        stem, dot, ext = name.partition(".")
        if ext == "c" and 8 <= len(stem) <= 16:
            try:
                out.add(int(stem, 16))
            except ValueError:
                pass
    return out


def propagate(edges, owner):
    """propagation proposals: callee called only from one class's named functions."""
    callers = collections.defaultdict(set)
    for caller, callee, _kind in edges:
        callers[callee].add(caller)
    out = {}
    for callee, cs in callers.items():
        if callee in owner:
            continue
        owners = {owner.get(c) for c in cs}
        if len(owners) == 1 and None not in owners:
            cls = owners.pop()
            out[callee] = (f"{cls}::helper_{callee:X}", "low",
                           dict(class_name=cls, caller_count=len(cs),
                                callers=[hexva(c) for c in sorted(cs)[:20]]))
    return out


# ---------------------------------------------------------------- build

SCHEMA = """
CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE class(ci_va INTEGER PRIMARY KEY, name TEXT, source TEXT, getter_va INTEGER,
  getter_count INTEGER, global_va INTEGER, parent_ci_va INTEGER, parent_name TEXT,
  parent_source TEXT, depth INTEGER,
  crc_own TEXT, crc_parent TEXT, crc_ok INTEGER, object_size INTEGER, info_flags TEXT,
  isa_lo INTEGER, isa_hi INTEGER, isa_nested INTEGER, class_index INTEGER, props_va INTEGER,
  prop_count INTEGER, prop_count_method TEXT, prop_entries_pointed INTEGER, enums_va INTEGER,
  factory_va INTEGER);
CREATE TABLE vtable(va INTEGER PRIMARY KEY, ci_va INTEGER, class_name TEXT, slot_count INTEGER,
  end_reason TEXT, lea_referenced INTEGER);
CREATE TABLE method(fn_va INTEGER, ci_va INTEGER, class_name TEXT, vtable_va INTEGER,
  slot INTEGER, origin TEXT);
CREATE TABLE field(ci_va INTEGER, class_name TEXT, idx INTEGER, entry_va INTEGER, name_hash TEXT,
  name TEXT, name_source TEXT, flags TEXT, type_hash TEXT, type_name TEXT, type_code TEXT,
  type_word TEXT, packed TEXT, unk TEXT, offset INTEGER, offset_source TEXT,
  accessor_offset INTEGER, offset_check TEXT, pointer_global INTEGER, fn0 INTEGER, fn1 INTEGER,
  fn2 INTEGER, fn3 INTEGER);
CREATE TABLE name(va INTEGER, name TEXT, source TEXT, confidence TEXT, detail TEXT);
CREATE TABLE function(va INTEGER PRIMARY KEY, in_corpus INTEGER, in_pdata INTEGER,
  corpus_name TEXT);
CREATE TABLE call(caller_va INTEGER, callee_va INTEGER, kind TEXT);
"""
INDEXES = """
CREATE INDEX class_name_ix ON class(name);
CREATE INDEX method_fn_ix ON method(fn_va);
CREATE INDEX method_class_ix ON method(class_name);
CREATE INDEX field_class_ix ON field(class_name);
CREATE INDEX name_va_ix ON name(va);
CREATE INDEX name_name_ix ON name(name);
CREATE INDEX call_callee_ix ON call(callee_va);
CREATE INDEX call_caller_ix ON call(caller_va);
"""
SOURCE_RANK = {"capture-finding": 0, "reflection-vtable": 1, "reflection-property": 2, "propagation": 3}


def load_findings(path):
    if path is None or not Path(path).is_file():
        return []
    rows = json.loads(Path(path).read_text(encoding="utf-8"))
    out = []
    for r in rows:
        out.append((int(r["va"], 16), r["name"], "capture-finding",
                    {"live": "finding-live", "static": "finding-static"}.get(r.get("evidence_kind"),
                                                                            "finding-inferred"),
                    {k: r[k] for k in ("finding", "file", "line", "basis", "state", "evidence_kind",
                                        "branch") if k in r}))
    return out


def build(view, corpus=None, findings=None, log=print):
    getters = find_getters(view)
    log(f"getters: {len(getters)}")
    classes = collect_classes(view, getters)
    log(f"classes: {len(classes)}")
    crc_names = crc_dictionary(view)
    class_by_crc = {r["crc_own"]: r["name"] for r in classes.values() if r["crc_ok"]}
    bounds = sorted(set(classes) | {r["props"] for r in classes.values() if r["props"]})
    data_va, data = view.section(".data")
    words = array.array("Q")
    words.frombytes(data[:len(data) // 8 * 8])
    pointer_globals = set(words)  # per-entry `&props[k]` globals corroborate each count
    fields = []
    for ci in sorted(classes):
        fields.extend((ci, row) for row in read_properties(
            view, classes[ci], crc_names, class_by_crc, bounds, pointer_globals))
    log(f"fields: {len(fields)}")
    referenced = lea_targets(view)
    vtables = find_vtables(view, getters, classes, referenced)
    log(f"vtables: {len(vtables)}")
    methods = classify_methods(vtables, classes)
    vnames = name_vtable_functions(methods, classes)
    method_fns = {m[0] for m in methods}

    names = []
    for fn, (name, conf, detail) in vnames.items():
        names.append((fn, name, "reflection-vtable", conf, detail))
    owner = {fn: d["base_class"] for fn, (_n, c, d) in vnames.items() if c in ("high", "medium")}
    for ci, row in fields:
        cls = classes[ci]["name"]
        label = row["name"] or f"prop_{row['name_hash']:08X}"
        for k, fn in enumerate(dict.fromkeys(f for f in row["fns"] if f)):
            if fn in method_fns:
                continue
            names.append((fn, f"{cls}::{label}_accessor{k}", "reflection-property",
                          "medium" if row["name"] else "low",
                          dict(class_name=cls, property_index=row["idx"],
                               name_hash=f"{row['name_hash']:08X}", entry_va=hexva(row["entry"]))))
            owner.setdefault(fn, cls)

    ranges = pdata_primary_ranges(view)
    corpus_set = corpus_functions(corpus)
    starts = {r[2] for r in ranges} | method_fns | (corpus_set or set())
    edges = call_edges(view, ranges, starts)
    log(f"call edges: {len(edges)}")
    for fn, (name, conf, detail) in propagate(edges, owner).items():
        names.append((fn, name, "propagation", conf, detail))
    names.extend(load_findings(findings))
    return dict(getters=getters, classes=classes, fields=fields, vtables=vtables, methods=methods,
                names=names, ranges=ranges, corpus=corpus_set, edges=edges, referenced=referenced)


def write_db(result, path, meta):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    if tmp.exists():
        tmp.unlink()
    db = sqlite3.connect(tmp)
    db.executescript(SCHEMA)
    classes = result["classes"]
    for ci in sorted(classes):
        r = classes[ci]
        p = classes.get(r["parent"])
        nested = None
        if p is not None and ISA_UNASSIGNED not in (r["isa_lo"], p["isa_lo"]):
            nested = int(p["isa_lo"] <= r["isa_lo"] <= r["isa_hi"] <= p["isa_hi"])
        getters = sorted(r["getters"])
        db.execute("INSERT INTO class VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)", (
            ci, r["name"], r["source"], getters[0] if getters else None, len(getters),
            min(r["globals"]) if r["globals"] else None, r["parent"], p["name"] if p else None,
            r["parent_source"], r["depth"], f"{r['crc_own']:08X}", f"{r['crc_parent']:08X}", r["crc_ok"], r["size"],
            f"{r['flags']:08X}", r["isa_lo"], r["isa_hi"], nested, r["index"], r["props"] or None,
            r.get("prop_count"), r.get("prop_count_method"), r.get("prop_entries_pointed"),
            r["enums"] or None,
            r["factory"] or None))
    for vt in sorted(result["vtables"].values(), key=lambda v: v["va"]):
        db.execute("INSERT INTO vtable VALUES (?,?,?,?,?,?)", (
            vt["va"], vt["ci"], vt["name"], len(vt["slots"]), vt["end_reason"], vt["referenced"]))
    db.executemany("INSERT INTO method VALUES (?,?,?,?,?,?)", sorted(
        (fn, ci, classes[ci]["name"], vt, slot, origin) for fn, ci, vt, slot, origin in result["methods"]))
    for ci, f in result["fields"]:
        db.execute("INSERT INTO field VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)", (
            ci, classes[ci]["name"], f["idx"], f["entry"], f"{f['name_hash']:08X}", f["name"],
            f["name_source"], f"{f['flags']:08X}", f"{f['type_hash']:08X}", f["type_name"],
            f"{f['type_code']:04X}", f"{f['type_word']:08X}", f"{f['packed']:08X}", f"{f['unk']:08X}",
            f["offset"], f["offset_source"], f["accessor_offset"], f["offset_check"],
            f["pointer_global"], *f["fns"]))
    db.executemany("INSERT INTO name VALUES (?,?,?,?,?)", sorted(
        ((va, n, s, c, json.dumps(d, sort_keys=True)) for va, n, s, c, d in result["names"]),
        key=lambda r: (r[0], SOURCE_RANK.get(r[2], 9), r[1])))
    corpus = result["corpus"]
    pdata = {r[2] for r in result["ranges"]}
    fset = pdata | (corpus or set()) | {m[0] for m in result["methods"]} | {n[0] for n in result["names"]}
    db.executemany("INSERT INTO function VALUES (?,?,?,?)", (
        (va, None if corpus is None else int(va in corpus), int(va in pdata),
         f"sub_{va:X}" if corpus is not None and va in corpus else None) for va in sorted(fset)))
    db.executemany("INSERT INTO call VALUES (?,?,?)", sorted(result["edges"]))
    db.executescript(INDEXES)
    db.executemany("INSERT INTO meta VALUES (?,?)", sorted((k, str(v)) for k, v in meta.items()))
    db.commit()
    db.close()
    os.replace(tmp, path)


def coverage(db_path):
    db = sqlite3.connect(db_path)
    q = lambda sql: db.execute(sql).fetchone()[0]  # noqa: E731
    out = dict(
        classes=q("SELECT COUNT(*) FROM class"),
        classes_via_getter=q("SELECT COUNT(*) FROM class WHERE source='getter'"),
        classes_crc_ok=q("SELECT COUNT(*) FROM class WHERE crc_ok=1"),
        classes_isa_nested=q("SELECT COUNT(*) FROM class WHERE isa_nested=1"),
        classes_isa_not_nested=q("SELECT COUNT(*) FROM class WHERE isa_nested=0"),
        classes_isa_unassigned=q(f"SELECT COUNT(*) FROM class WHERE isa_lo={ISA_UNASSIGNED}"),
        classes_with_parent=q("SELECT COUNT(*) FROM class WHERE parent_ci_va IS NOT NULL"),
        classes_parent_by_crc=q("SELECT COUNT(*) FROM class WHERE parent_source='crc'"),
        classes_root_or_unlinked=q("SELECT COUNT(*) FROM class WHERE parent_ci_va IS NULL"),
        classes_parent_crc_agrees=q(
            "SELECT COUNT(*) FROM class c JOIN class p ON p.ci_va=c.parent_ci_va WHERE c.crc_parent=p.crc_own"),
        classes_size_ge_parent=q(
            "SELECT COUNT(*) FROM class c JOIN class p ON p.ci_va=c.parent_ci_va WHERE c.object_size>=p.object_size"),
        classes_with_fields=q("SELECT COUNT(DISTINCT ci_va) FROM field"),
        fields_offset_beyond_size=q(
            "SELECT COUNT(*) FROM field f JOIN class c ON c.ci_va=f.ci_va WHERE f.offset>=c.object_size"),
        classes_prop_count_unknown=q("SELECT COUNT(*) FROM class WHERE prop_count IS NULL"),
        getters=q("SELECT SUM(getter_count) FROM class"),
        vtables=q("SELECT COUNT(*) FROM vtable"),
        vtable_classes=q("SELECT COUNT(DISTINCT ci_va) FROM vtable"),
        vtables_lea_referenced=q("SELECT COUNT(*) FROM vtable WHERE lea_referenced=1"),
        method_rows=q("SELECT COUNT(*) FROM method"),
        method_functions=q("SELECT COUNT(DISTINCT fn_va) FROM method"),
        method_functions_in_corpus=q(
            "SELECT COUNT(DISTINCT m.fn_va) FROM method m JOIN function f ON f.va=m.fn_va WHERE f.in_corpus=1"),
        fields=q("SELECT COUNT(*) FROM field"),
        fields_named=q("SELECT COUNT(*) FROM field WHERE name IS NOT NULL"),
        fields_with_offset=q("SELECT COUNT(*) FROM field WHERE offset IS NOT NULL"),
        fields_offset_match=q("SELECT COUNT(*) FROM field WHERE offset_check='match'"),
        fields_offset_differs=q("SELECT COUNT(*) FROM field WHERE offset_check='differs'"),
        fields_pointer_global=q("SELECT COUNT(*) FROM field WHERE pointer_global=1"),
        classes_all_entries_pointed=q(
            "SELECT COUNT(*) FROM class WHERE prop_count>0 AND prop_entries_pointed=prop_count"),
        corpus_functions=q("SELECT COUNT(*) FROM function WHERE in_corpus=1"),
        named_functions=q("SELECT COUNT(DISTINCT va) FROM name"),
        named_corpus_functions=q(
            "SELECT COUNT(DISTINCT n.va) FROM name n JOIN function f ON f.va=n.va WHERE f.in_corpus=1"),
        call_edges=q("SELECT COUNT(*) FROM call"),
    )
    out["names_by_source_confidence"] = {
        f"{s}/{c}": n for s, c, n in db.execute(
            "SELECT source, confidence, COUNT(DISTINCT va) FROM name GROUP BY 1,2 ORDER BY 1,2")}
    out["prop_count_method"] = dict(db.execute(
        "SELECT prop_count_method, COUNT(*) FROM class GROUP BY 1 ORDER BY 1"))
    out["method_origin"] = dict(db.execute("SELECT origin, COUNT(*) FROM method GROUP BY 1 ORDER BY 1"))
    out["vtable_end_reason"] = dict(db.execute(
        "SELECT end_reason, COUNT(*) FROM vtable GROUP BY 1 ORDER BY 1"))
    db.close()
    return out


def resolve_corpus(arg):
    if arg:
        return Path(arg)
    env = os.environ.get("ACU_DECOMPILED_DIR")
    if env:
        return Path(env)
    cfg = ROOT / "local" / "project.json"
    if cfg.is_file():
        value = json.loads(cfg.read_text(encoding="utf-8")).get("decompiled_dir")
        if value:
            return Path(value)
    return None


def sha256_file(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--image", required=True, type=Path, help="captured main-image.mapped.bin")
    ap.add_argument("--expect-sha256", help="refuse to build if the image hash differs")
    ap.add_argument("--base", type=lambda x: int(x, 0), help="runtime base (default: PE header)")
    ap.add_argument("--corpus", help="ACU - Decompiled dir (default: ACU_DECOMPILED_DIR / local/project.json)")
    ap.add_argument("--findings", type=Path, default=DEFAULT_FINDINGS)
    ap.add_argument("--db", type=Path, default=DEFAULT_DB)
    args = ap.parse_args(argv)
    digest = sha256_file(args.image)
    if args.expect_sha256 and digest.lower() != args.expect_sha256.lower():
        ap.error(f"image SHA-256 {digest} != expected {args.expect_sha256}")
    view = View(Image(args.image.read_bytes(), mapped=True), args.base)
    corpus = resolve_corpus(args.corpus)
    if corpus is not None and not corpus.is_dir():
        print(f"corpus unavailable at {corpus}; in_corpus will be NULL", file=sys.stderr)
        corpus = None
    result = build(view, corpus, args.findings, log=lambda m: print(m, file=sys.stderr))
    meta = dict(schema_version=SCHEMA_VERSION, image_path=args.image.resolve().as_posix(),
                image_sha256=digest, image_base=hexva(view.base),
                tool_sha256=sha256_file(__file__),
                findings_path=args.findings.as_posix() if args.findings.is_file() else None,
                findings_sha256=sha256_file(args.findings) if args.findings.is_file() else None,
                corpus_dir=corpus.as_posix() if corpus else None,
                corpus_files=len(result["corpus"]) if result["corpus"] is not None else None)
    write_db(result, args.db, meta)
    cov = coverage(args.db)
    db = sqlite3.connect(args.db)
    db.execute("INSERT INTO meta VALUES ('coverage', ?)", (json.dumps(cov, sort_keys=True),))
    db.commit()
    db.close()
    print(json.dumps(dict(db=str(args.db), image_sha256=digest, coverage=cov), indent=2))


if __name__ == "__main__":
    main()
