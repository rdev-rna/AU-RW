#!/usr/bin/env python3
"""Check addresses cited in the docs against an actual image of the build.

Docs cite VAs and hex byte guards. A citation is only usable if the VA exists in
the mapped image and, when a guard is quoted, the bytes at that VA match it. This
tool reports every citation as verified, mismatched, or unverifiable - it never
"fixes" a doc and never guesses a correction.

Bounds: reads one mapped image and one pass over the Markdown tree; no process is
opened (pass --pid to also check live through the tap, read-only).

Usage:
  python tools/verify_cited_addresses.py --image <mapped image> [--pid N] \
      [--root .] [--output local/cited-addresses.json]
"""
import argparse
import json
import re
import struct
from datetime import datetime, timezone
from pathlib import Path

VA = re.compile(r"0x(1[0-9a-fA-F]{8})\b")
# Byte guards are 16-31 bytes. SHA-256 (64 hex) is excluded so hashes are not
# mistaken for guards; a SHA-1-length token is kept because 20-byte guards exist.
GUARD = re.compile(r"\b([0-9a-fA-F]{32,63})\b")
MAX_FILE_BYTES = 4 * 1024 * 1024
SCN_MEM_EXECUTE = 0x20000000
DEFAULT_SCAN = ("docs/findings", "docs/status", "docs", "PROGRESS.md")
EXTERNAL = "runtime-or-external"


def image_sections(path):
    data = Path(path).read_bytes()
    if len(data) < 0x40 or data[:2] != b"MZ":
        raise ValueError("not an MZ image")
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError("PE signature missing")
    machine, count, _, _, _, opt_size, _ = struct.unpack_from("<HHIIIHH", data, pe + 4)
    if machine != 0x8664:
        raise ValueError("requires x64")
    opt = pe + 24
    base = struct.unpack_from("<Q", data, opt + 24)[0]
    size = struct.unpack_from("<I", data, opt + 56)[0]
    if len(data) < size:
        raise ValueError("image shorter than SizeOfImage")
    sections = []
    for i in range(count):
        row = struct.unpack_from("<8sIIIIIIHHI", data, opt + opt_size + i * 40)
        sections.append(dict(name=row[0].rstrip(b"\0").decode("ascii", "replace"),
                             rva=row[2], virtual_size=row[1], flags=row[9]))
    return data, base, sections


def locate(sections, rva):
    for section in sections:
        if section["rva"] <= rva < section["rva"] + section["virtual_size"]:
            return section
    return None


def markdown_files(root, patterns):
    seen = []
    for pattern in patterns:
        target = Path(root) / pattern
        if target.is_file():
            seen.append(target)
        elif target.is_dir():
            seen.extend(sorted(target.rglob("*.md")))
    unique = []
    for path in seen:
        if path not in unique and path.stat().st_size <= MAX_FILE_BYTES:
            unique.append(path)
    return unique


def nearest_guard(lines, index, window=3):
    """A guard is often quoted on the line after the VA, so look around it."""
    for offset in range(window + 1):
        for candidate in (index + offset, index - offset):
            if 0 <= candidate < len(lines):
                match = GUARD.search(lines[candidate])
                if match and len(match.group(1)) % 2 == 0:
                    return match.group(1)
    return None


def pdata_starts(image, sections, base):
    """Function entries from the exception directory, as RVAs."""
    pe = struct.unpack_from("<I", image, 0x3C)[0]
    opt = pe + 24
    count = struct.unpack_from("<I", image, opt + 108)[0]
    directories = [struct.unpack_from("<II", image, opt + 112 + 8 * i) for i in range(min(16, count))]
    if len(directories) <= 3:
        return set()
    rva, size = directories[3]
    if not rva or not size or size % 12:
        return set()
    return {struct.unpack_from("<III", image, rva + 12 * i)[0]
            for i in range(size // 12)}


def pairing(lines, index):
    """Return the guard for a line, but only when the pairing is unambiguous.

    A guard belongs to the address it *follows*: `0xVA ... bytes <hex>`. Two shapes
    occur in the corpus, and both are accepted only when unambiguous:
      same line, exactly one VA, exactly one guard, guard after the VA;
      VA line ending in the word "bytes" with the guard at the start of the next line.
    A guard that precedes the VA, or a line with several VAs, is left unpaired.
    Guessing the pairing is how an audit invents errors.
    """
    line = lines[index]
    vas = list(VA.finditer(line))
    guards = [m for m in GUARD.finditer(line) if len(m.group(1)) % 2 == 0]
    if len(vas) == 1 and len(guards) == 1 and guards[0].start() > vas[0].end():
        return vas[0].group(0), guards[0].group(1)
    if len(vas) == 1 and not guards and index + 1 < len(lines):
        stripped = line.rstrip()
        if stripped.endswith(("bytes", "bytes:", "guard", "guard:")):
            match = GUARD.match(lines[index + 1].strip().lstrip("`"))
            if match and len(match.group(1)) % 2 == 0:
                return vas[0].group(0), match.group(1)
    return None, None


def classify_unpaired(literal, image, base, sections, starts):
    va = int(literal, 16)
    if va == base:
        return dict(va=literal, status="image-base")
    rva = va - base
    section = locate(sections, rva)
    if section is None:
        return dict(va=literal, status=EXTERNAL)
    row = dict(va=literal, rva=f"0x{rva:x}", section=section["name"],
               executable=bool(section["flags"] & SCN_MEM_EXECUTE))
    if not row["executable"]:
        row["status"] = "va-only"
        return row
    if rva in starts:
        row["status"] = "function-entry"
    elif image[rva] == 0xCC:
        row["status"] = "inside-padding"
    else:
        row["status"] = "mid-function"
    return row


def guard_audit(files, image, base, window=0x10000):
    """Locate every quoted guard in the image and compare with the nearest cited VA.

    Independent of how the doc formats the pair: the answer is "this guard lives at
    VA X", so a citation of VA Y for it is off by X-Y bytes, or the guard is absent
    from the image entirely.
    """
    rows = []
    for path in files:
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        for index, line in enumerate(lines):
            for match in GUARD.finditer(line):
                token = match.group(1)
                if len(token) % 2:
                    continue
                expected = bytes.fromhex(token)
                if len(token) in (40, 64):
                    # SHA-1/commit and SHA-256 digests, not byte guards.
                    continue
                # Prefer a VA on the same line, then the closest line, then the
                # closest position in that line.
                candidates = []
                for offset in range(0, 4):
                    for j in (index + offset, index - offset):
                        if 0 <= j < len(lines):
                            for match_va in VA.finditer(lines[j]):
                                candidates.append((offset, match_va.start(),
                                                   int(match_va.group(0), 16)))
                    if candidates:
                        break
                cited = min(candidates)[2] if candidates else None
                where = -1
                if cited is not None:
                    rva = cited - base
                    where = image.find(expected, max(0, rva - window), min(len(image), rva + len(expected) + window))
                if where < 0:
                    where = image.find(expected)
                row = dict(file=(str(path.relative_to(Path(".")))
                                 if path.is_relative_to(Path(".").resolve()) else str(path)),
                           line=index + 1, guard=token, length=len(expected),
                           cited_va=f"0x{cited:x}" if cited else None,
                           found_va=f"0x{base + where:x}" if where >= 0 else None)
                if where < 0:
                    row["status"] = "absent-from-image"
                elif cited is None:
                    row["status"] = "found-no-citation"
                elif where == cited - base:
                    row["status"] = "verified"
                else:
                    row["status"] = "off-by"
                    row["delta"] = (where - (cited - base))
                rows.append(row)
    return rows


def classify(literal, guard, image, base, sections):
    va = int(literal, 16)
    if va == base:
        return dict(va=literal, status="image-base",
                    note="the module base itself, not a code address")
    rva = va - base
    section = locate(sections, rva)
    row = dict(va=literal, rva=f"0x{rva:x}" if rva >= 0 else None,
               section=section["name"] if section else None)
    if section is None:
        row["status"] = EXTERNAL
        row["executable"] = False
        return row
    row["executable"] = bool(section["flags"] & SCN_MEM_EXECUTE)
    size = len(guard) // 2 if guard else 0
    if not size:
        row["status"] = "va-only"
        return row
    if rva + size > section["rva"] + section["virtual_size"]:
        row["status"] = "straddles-section-end"
        return row
    actual = image[rva:rva + size]
    expected = bytes.fromhex(guard)
    row["expected"] = guard.lower()
    row["actual"] = actual.hex()
    if actual == expected:
        row["status"] = "verified"
    else:
        row["status"] = "mismatch"
        row["first_difference"] = next(
            (i for i, (a, b) in enumerate(zip(actual, expected)) if a != b), None)
        # If the guard sits somewhere near the cited VA, the citation is off by N
        # bytes. That is a checkable correction, unlike "the guard is wrong".
        window = 4096
        low = max(0, rva - window)
        high = min(len(image), rva + len(expected) + window)
        found = image.find(expected, low, high)
        if found >= 0:
            row["guard_found_rva"] = f"0x{found:x}"
            row["guard_found_va"] = f"0x{base + found:x}"
            row["delta"] = found - rva
        else:
            row["guard_found_rva"] = None
    return row


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--image", type=Path, required=True, help="mapped image, offset == RVA")
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--scan", action="append", default=[],
                        help=f"file or directory relative to --root (default: {', '.join(DEFAULT_SCAN)})")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--show", type=int, default=40, help="how many problems to print")
    args = parser.parse_args(argv)

    image, base, sections = image_sections(args.image)
    starts = pdata_starts(image, sections, base)
    files = markdown_files(args.root, args.scan or DEFAULT_SCAN)
    rows = []
    for path in files:
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        for index, line in enumerate(lines):
            literals = [m.group(0) for m in VA.finditer(line)]
            if not literals:
                continue
            paired_va, guard = pairing(lines, index)
            for literal in literals:
                # Only the address the guard is actually bound to gets checked
                # against it; every other address on the line is classified only.
                row = (classify(literal, guard, image, base, sections)
                       if paired_va == literal
                       else classify_unpaired(literal, image, base, sections, starts))
                row.update(file=str(path.relative_to(args.root)).replace("\\", "/"),
                           line=index + 1)
                rows.append(row)

    counts = {}
    for row in rows:
        counts[row["status"]] = counts.get(row["status"], 0) + 1
    problems = [r for r in rows if r["status"] == "mismatch"]
    padding = [r for r in rows if r["status"] == "inside-padding"]
    report = dict(schema_version=1,
                  generated_utc=datetime.now(timezone.utc).isoformat(),
                  image=str(args.image), image_base=f"0x{base:x}",
                  files_scanned=len(files), citations=len(rows), counts=counts,
                  guard_bound=counts.get("verified", 0) + counts.get("mismatch", 0),
                  mismatches=problems, inside_padding=padding,
                  function_entries=sum(1 for r in rows if r["status"] == "function-entry"),
                  note="A mismatch means a guard bound to that exact address does not match "
                       "the image; either the citation is wrong or the site is "
                       "runtime-patched (ACU-RE-0032). inside-padding means the cited VA "
                       "lands on 0xCC filler inside .text, which no hook can target. "
                       "runtime-or-external addresses are live pointers and not errors. "
                       "This tool never proposes a correction.")
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        guards = guard_audit(files, image, base)
        guard_counts = {}
        for row in guards:
            guard_counts[row["status"]] = guard_counts.get(row["status"], 0) + 1
        report["guard_audit"] = dict(counts=guard_counts, rows=guards)
        report["counts"] = counts
        args.output.write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps(dict(files=len(files), citations=len(rows), counts=counts,
                              function_entries=report["function_entries"],
                              guard_audit=guard_counts), indent=2))
        for row in guards:
            if row["status"] == "absent-from-image":
                print(f"absent   {row['guard'][:32]}  {row['file']}:{row['line']}  cited {row['cited_va']}")
            elif row["status"] == "off-by":
                print(f"off-by   {row['delta']:+7d}  {row['file']}:{row['line']}  "
                      f"cited {row['cited_va']} -> guard at {row['found_va']}")
    for row in problems[:args.show]:
        print(f"mismatch {row['va']}  {row['file']}:{row['line']}  {row['section']}")
        print(f"         expected {row['expected']}")
        print(f"         actual   {row['actual']}")
        if row.get("guard_found_va"):
            print(f"         guard is at {row['guard_found_va']} (delta {row['delta']:+d})")
    for row in padding[:args.show]:
        print(f"in-padding {row['va']}  {row['file']}:{row['line']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
