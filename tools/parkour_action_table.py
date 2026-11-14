"""Map EnumParkourAction-style type numbers to their classes from the decompiled corpus.

    python tools/parkour_action_table.py --output local/parkour-actions/table.json [--image <mapped image>]

Source: the factory switch in `<corpus>/1401CC990.c` (allocation size, constructor per case) and
each constructor file (`*(a1+0) = vtable`, `*(a1+8) = fancy table`, first base-constructor call).
With --image (a mapped main image, see ACU-RE-0041), each fancy table's enum getter (slot 0x45,
`mov eax, imm32; ret`) is read and must return the case number, which checks the pairing.
The corpus is an analysis artifact: rows that do not parse are reported, never guessed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
FACTORIES = (0x1401CC990, 0x1401D1FC0)  # the second uses char cases: '3' = 51
IMAGE_BASE = 0x140000000
ENUM_SLOT = 0x45
CASE_RE = re.compile(r"^\s*case (\d+|'.'):", re.M)
ALLOC_RE = re.compile(r"sub_1426F3760\((\d+),")
CTOR_RE = re.compile(r"=\s*(?:\([^)]*\))?\s*sub_([0-9A-F]{9})\(v\d+")
VTABLE_RE = re.compile(r"\*(?:\(_QWORD \*\))?a1 = (?:&)?(?:off|unk)_([0-9A-F]{9});")
FANCY_RE = re.compile(r"(?:\*\(_QWORD \*\)\(a1 \+ 8\)|a1\[1\]) = (?:&)?[A-Za-z_]*?_?([0-9A-F]{9})?([A-Za-z0-9_]*);")
BASE_RE = re.compile(r"^\s*(?:\w[\w *()]*=\s*)?sub_([0-9A-F]{9})\(a1[,)]", re.M)


def corpus_dir() -> Path:
    import acu
    return Path(acu.settings()[1]["decompiled_dir"])


def function_text(corpus: Path, va: int) -> str | None:
    path = corpus / f"{va:X}.c"
    return path.read_text(encoding="utf-8", errors="replace") if path.exists() else None


def parse_factory(text: str) -> list[dict]:
    starts = [(m.start(), ord(m.group(1)[1]) if m.group(1).startswith("'") else int(m.group(1)))
              for m in CASE_RE.finditer(text)]
    rows = []
    for i, (pos, case) in enumerate(starts):
        body = text[pos: starts[i + 1][0] if i + 1 < len(starts) else len(text)]
        alloc, ctor = ALLOC_RE.search(body), CTOR_RE.search(body)
        rows.append({"type": case, "alloc_bytes": int(alloc.group(1)) if alloc else None,
                     "ctor": f"0x{ctor.group(1)}" if ctor else None})
    return rows


def fancy_symbol(text: str) -> str | None:
    """IDA names the fancy table after its first bytes (aU_191 etc.); keep the raw symbol.
    Derived constructors store the base class's table first and their own last."""
    found = re.findall(r"(?:\*\(_QWORD \*\)\(a1 \+ 8\)|a1\[1\]) = (?:&)?([A-Za-z0-9_]+);", text)
    return found[-1] if found else None


def function_size(text: str) -> int | None:
    m = re.search(r"size=(\d+)", text.splitlines()[0])
    return int(m.group(1)) if m else None


def parse_ctor(text: str) -> dict:
    vts = VTABLE_RE.findall(text)
    base = BASE_RE.search(text)
    return {"vtable": f"0x{vts[-1]}" if vts else None, "fancy_symbol": fancy_symbol(text),
            "base_ctor": f"0x{base.group(1)}" if base else None, "ctor_bytes": function_size(text)}


EVENT_RE = re.compile(
    r"(?:\*\(_QWORD \*\)\(a1 \+ 8\)|a1\[1\]) = (?:&)?(?P<sym>[A-Za-z0-9_]+);"
    r"|sub_(?P<call>[0-9A-F]{9})\((?:\([^)]*\))?a1[,)]")


def final_store_owner(corpus: Path, va: int, depth: int = 0) -> tuple[int, str, int] | None:
    """(function VA, IDA symbol, function size) of the last `+8` store a constructor performs,
    following a trailing call that takes a1 (depth <= 3). Base-constructor calls before a store
    are overridden by that store, so only the final event matters."""
    text = function_text(corpus, va)
    if text is None or depth > 3:
        return None
    events = list(EVENT_RE.finditer(text))
    for event in reversed(events):
        if event.group("sym"):
            return va, event.group("sym"), function_size(text) or 0x100
        found = final_store_owner(corpus, int(event.group("call"), 16), depth + 1)
        if found:
            return found
    return None


class Image:
    def __init__(self, path: Path):
        self.data = path.read_bytes()
        self.sha256 = hashlib.sha256(self.data).hexdigest()

    def u64(self, va: int) -> int:
        return struct.unpack_from("<Q", self.data, va - IMAGE_BASE)[0]

    def bytes(self, va: int, n: int) -> bytes:
        return self.data[va - IMAGE_BASE: va - IMAGE_BASE + n]


def fancy_from_ctor_disasm(image: Image, ctor: int, size: int) -> int | None:
    """Last `mov [reg+8], rax` (48 89 4x 08) in the constructor, with rax from the nearest
    preceding `lea rax, [rip+rel32]` (48 8D 05). A byte-pattern scan, not a disassembly:
    a result counts only if that table's enum getter returns the expected type."""
    code = image.bytes(ctor, size)
    lea, last = {}, None  # register number -> last rip-relative LEA target
    for i in range(len(code) - 3):
        modrm = code[i + 2]
        # 48 8D /r with mod=00 rm=101: lea r64, [rip+rel32]
        if code[i:i + 2] == b"\x48\x8d" and (modrm & 0xC7) == 0x05 and i + 7 <= len(code):
            lea[(modrm >> 3) & 7] = ctor + i + 7 + struct.unpack_from("<i", code, i + 3)[0]
        # 48 89 /r with mod=01 (disp8 = 8), any base except 100 (SIB): mov [base+8], r64
        elif code[i:i + 2] == b"\x48\x89" and (modrm & 0xC0) == 0x40 and (modrm & 7) != 4 \
                and code[i + 3] == 0x08 and ((modrm >> 3) & 7) in lea:
            last = lea[(modrm >> 3) & 7]
    return last


def enum_of(image: Image, fancy: int) -> int | None:
    """Same resolver as tools/parkour_trace.py (entry slot index at +4, function at +0x10)."""
    from parkour_trace import EnumResolver
    if not hasattr(image, "resolver"):
        image.resolver = EnumResolver(image.data, IMAGE_BASE)
    return image.resolver(fancy)


def build(corpus: Path, image: Image | None) -> dict:
    rows, problems = [], []
    for va in FACTORIES:
        factory = function_text(corpus, va)
        if factory is None:
            raise SystemExit(f"{va:X}.c not in the corpus")
        for row in parse_factory(factory):
            row["factory"] = f"0x{va:X}"
            rows.append(row)
    for row in rows:
        text = function_text(corpus, int(row["ctor"], 16)) if row["ctor"] else None
        if text is None:
            problems.append({"type": row["type"], "problem": "constructor not parsed or not in corpus"})
            continue
        row.update(parse_ctor(text))
        owner = final_store_owner(corpus, int(row["ctor"], 16))
        row["fancy_store_in"] = f"0x{owner[0]:X}" if owner else None
        if owner:
            row["fancy_symbol"] = owner[1]
        if image is not None:
            fancy = fancy_from_ctor_disasm(image, owner[0], owner[2]) if owner else None
            row["fancy_table"] = f"0x{fancy:X}" if fancy else None
            row["enum_from_image"] = enum_of(image, fancy) if fancy else None
            if row["enum_from_image"] is None:
                problems.append({"type": row["type"], "problem": "fancy table or its enum getter not resolved"})
            elif row["enum_from_image"] != row["type"]:
                # Observation, not a parse failure: the class's getter reports another type.
                row["reports_enum"] = row["enum_from_image"]
    return {"schema_version": 1, "factories": [f"0x{va:X}" for va in FACTORIES],
            "image_sha256": image.sha256 if image else None, "rows": rows, "problems": problems}


def live_pairs(decoded_dirs: list[Path]) -> dict[int, dict]:
    """fancy table VA -> {enum, count} from tools/parkour_trace.py decisions.jsonl files.
    Assumes the live image base equals 0x140000000, as recorded for every run so far."""
    pairs: dict[int, dict] = {}
    for folder in decoded_dirs:
        for line in (folder / "decisions.jsonl").read_text(encoding="utf-8").splitlines():
            for c in json.loads(line).get("considered", []):
                if c.get("fancy_table") and c.get("enum") is not None:
                    entry = pairs.setdefault(int(c["fancy_table"], 16), {"enum": c["enum"], "count": 0, "conflict": False})
                    entry["count"] += 1
                    entry["conflict"] |= entry["enum"] != c["enum"]
    return pairs


def cross_check(table: dict, pairs: dict[int, dict]) -> dict:
    static = {int(r["fancy_table"], 16): r for r in table["rows"] if r.get("fancy_table")}
    agree, disagree, unknown = [], [], []
    for va, live in sorted(pairs.items()):
        row = static.get(va)
        if row is None:
            unknown.append({"fancy_table": f"0x{va:X}", "live_enum": live["enum"], "count": live["count"]})
        elif row.get("enum_from_image") == live["enum"]:
            agree.append({"fancy_table": f"0x{va:X}", "enum": live["enum"], "factory_type": row["type"]})
        else:
            disagree.append({"fancy_table": f"0x{va:X}", "live_enum": live["enum"], "factory_type": row["type"],
                             "static_enum": row.get("enum_from_image")})
    return {"live_tables": len(pairs), "agree": agree, "disagree": disagree,
            "live_tables_not_built_by_factory_rows": unknown,
            "conflicting_live_tables": [f"0x{va:X}" for va, v in pairs.items() if v["conflict"]]}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--output", type=Path, required=True)
    ap.add_argument("--image", type=Path)
    ap.add_argument("--live", type=Path, action="append", default=[],
                    help="decoded run folder (decisions.jsonl) to cross-check fancy tables against")
    a = ap.parse_args()
    table = build(corpus_dir(), Image(a.image) if a.image else None)
    if a.live:
        table["live_cross_check"] = cross_check(table, live_pairs(a.live))
        table["live_sources"] = [str(p) for p in a.live]
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(table, indent=2) + "\n", encoding="utf-8")
    ok = sum(r.get("enum_from_image") == r["type"] for r in table["rows"])
    print(json.dumps({"rows": len(table["rows"]), "image_verified": ok, "problems": len(table["problems"])}))


if __name__ == "__main__":
    main()
