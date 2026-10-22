#!/usr/bin/env python3
"""Bind the ACU reverse-engineering corpus to an installed executable by hash.

`tools/decompile_xref.py` joins address-named decompile files to unwind ranges and
explicitly does not establish binary identity. This tool answers that separate
question: does the mapped runtime image (and the reconstructed PE rebuilt from it,
and the function export derived from that) belong to a specific installed
executable, and do the exported function locations resolve to real bytes there?

The installed ACU.exe is packed: `.text`, `.rdata`, `.data` and `.pdata` carry
RawSize 0 on disk and the payload lives in `.UBX1`. Code bytes therefore cannot be
compared against the disk file at all. This tool never counts that as a match and
never reports parity from a missing comparison: unavailable bytes are reported as
`unavailable`, and the strongest verdict reachable without code comparison is
`bound_by_headers_and_resources`, not `bound_by_code`.

No file is modified, no process is opened, no decompiled corpus is rewritten.
Exit status is 0 when both bindings are established -- including the
`bound_without_code_comparison` verdict that this packed build can reach at best --
and 1 when either binding is unbound.
"""
import argparse
import bisect
import hashlib
import json
import re
import struct
from datetime import datetime, timezone
from pathlib import Path

try:
    from native_image import Image
except ImportError:  # imported as a package by tests
    from tools.native_image import Image

MAX_IMAGE_BYTES = 1 << 30
MAX_EXPORT_ROWS = 2_000_000
MAX_PROBE_BYTES = 1 << 20
MAX_SAMPLE = 10
DIFF_PAGE = 0x1000
MAX_DIFF_PAGES = 0x100000
DECOMP_NAME = re.compile(r"^decomp_([0-9a-fA-F]{8,16})\.c$")
SCN_MEM_EXECUTE = 0x20000000
SCN_CNT_CODE = 0x00000020


class BindingError(ValueError):
    """The inputs cannot support a binding claim."""


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def read_bounded(path, limit=MAX_IMAGE_BYTES):
    path = Path(path)
    size = path.stat().st_size
    if size == 0:
        raise BindingError(f"{path}: empty file")
    if size > limit:
        raise BindingError(f"{path}: {size} bytes exceeds the {limit}-byte bound")
    return path.read_bytes()


def pe_headers(data, label):
    """Parse the fields that must agree between an image and its disk file."""
    if len(data) < 0x40 or data[:2] != b"MZ":
        raise BindingError(f"{label}: not an MZ image")
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if pe + 24 > len(data) or data[pe:pe + 4] != b"PE\0\0":
        raise BindingError(f"{label}: PE signature missing")
    machine, count, timestamp, _, _, opt_size, _ = struct.unpack_from("<HHIIIHH", data, pe + 4)
    opt = pe + 24
    if machine != 0x8664 or struct.unpack_from("<H", data, opt)[0] != 0x20B:
        raise BindingError(f"{label}: requires x64 PE32+")
    if count > 96:
        raise BindingError(f"{label}: implausible section count {count}")
    entry = struct.unpack_from("<I", data, opt + 16)[0]
    base = struct.unpack_from("<Q", data, opt + 24)[0]
    size_of_image = struct.unpack_from("<I", data, opt + 56)[0]
    size_of_headers = struct.unpack_from("<I", data, opt + 60)[0]
    directories = [struct.unpack_from("<II", data, opt + 112 + i * 8)
                   for i in range(min(16, struct.unpack_from("<I", data, opt + 108)[0]))]
    sections = []
    for index in range(count):
        row = struct.unpack_from("<8sIIIIIIHHI", data, opt + opt_size + index * 40)
        sections.append(dict(name=row[0].rstrip(b"\0").decode("ascii", "replace"),
                             virtual_size=row[1], rva=row[2], raw_size=row[3],
                             raw_offset=row[4], flags=row[9]))
    return dict(machine=machine, coff_timestamp=timestamp, section_count=count,
                entry_rva=entry, image_base=base, size_of_image=size_of_image,
                size_of_headers=size_of_headers, directories=directories,
                sections=sections, opt_size=opt_size, pe_offset=pe)


def load_image(path, mapped, label, strict_length=True):
    data = read_bounded(path)
    headers = pe_headers(data, label)
    image = Image(data, mapped)
    if image.base != headers["image_base"] or image.size != headers["size_of_image"]:
        raise BindingError(f"{label}: PE header parse disagrees with section reader")
    shortfall = headers["size_of_image"] - len(data)
    if mapped and shortfall > 0:
        if strict_length:
            raise BindingError(f"{label}: mapped image is shorter than SizeOfImage "
                               f"by {shortfall} bytes")
        if shortfall > DIFF_PAGE:
            raise BindingError(f"{label}: shortfall of {shortfall} bytes exceeds one page")
    return dict(path=str(Path(path)), size=len(data), sha256=sha256(data),
                headers=headers, image=image, data=data,
                shortfall_bytes=max(0, shortfall))


def section_bytes(source, section):
    """Read one section's bytes, or None when the file does not carry them."""
    if not section["raw_size"]:
        return None
    length = min(section["raw_size"], section["virtual_size"] or section["raw_size"])
    if source["image"].mapped:
        start = section["rva"]
        if start + length > source["size"]:
            return None
        return source["data"][start:start + length]
    try:
        return source["image"].read(section["rva"], length)
    except ValueError:
        return None


def diff_runs(left, right, limit=MAX_DIFF_PAGES):
    """Count differing pages and keep the first runs, without interpreting them."""
    length = min(len(left), len(right))
    runs, run_start, pages = [], None, 0
    for offset in range(0, length, DIFF_PAGE):
        end = min(offset + DIFF_PAGE, length)
        pages += 1
        if pages > limit:
            return dict(pages_checked=pages, truncated=True, differing_pages=None,
                        differing_bytes=None, runs=[])
        if left[offset:end] != right[offset:end]:
            if run_start is None:
                run_start = offset
        elif run_start is not None:
            runs.append([run_start, offset])
            run_start = None
    if run_start is not None:
        runs.append([run_start, length])
    differing = sum(end - start for start, end in runs)
    return dict(pages_checked=pages, page_size=DIFF_PAGE, truncated=False,
                differing_pages=len(runs), differing_bytes=differing,
                runs=[[f"0x{start:x}", f"0x{end:x}"] for start, end in runs[:MAX_SAMPLE]])


def compare_section(disk_section, disk_bytes, mapped_bytes):
    if disk_bytes is None:
        return dict(verdict="unavailable",
                    reason="section has RawSize 0 in the installed file; bytes exist only at runtime")
    if mapped_bytes is None:
        return dict(verdict="unavailable", reason="mapped image does not carry this range")
    if disk_bytes == mapped_bytes:
        return dict(verdict="match", bytes=len(disk_bytes), sha256=sha256(disk_bytes))
    return dict(verdict="different", bytes=len(disk_bytes),
                disk_sha256=sha256(disk_bytes), mapped_sha256=sha256(mapped_bytes),
                diff=diff_runs(disk_bytes, mapped_bytes))


def compare_sources(disk, mapped, mutable_sections):
    """Compare every section of the disk file against the mapped image."""
    by_name = {s["name"]: s for s in mapped["headers"]["sections"]}
    rows, undeclared = [], []
    for section in disk["headers"]["sections"]:
        other = by_name.get(section["name"])
        if other is None:
            rows.append(dict(name=section["name"], verdict="different",
                             reason="section name absent from the mapped image"))
            undeclared.append(section["name"])
            continue
        layout_equal = (other["rva"] == section["rva"]
                        and other["virtual_size"] == section["virtual_size"])
        row = dict(name=section["name"], rva=f"0x{section['rva']:08x}",
                   virtual_size=section["virtual_size"],
                   disk_raw_size=section["raw_size"],
                   executable=bool(section["flags"] & SCN_MEM_EXECUTE),
                   code=bool(section["flags"] & SCN_CNT_CODE),
                   layout_equal=layout_equal)
        row.update(compare_section(section, section_bytes(disk, section),
                                   section_bytes(mapped, {**section, "name": other["name"]})))
        row["declared_mutable"] = section["name"] in mutable_sections
        rows.append(row)
        if row["verdict"] == "different" and not row["declared_mutable"]:
            undeclared.append(section["name"])
        if not layout_equal:
            undeclared.append(section["name"])
    return rows, sorted(set(undeclared))


def image_binding(disk, mapped, mutable_sections):
    anchors = dict(
        image_base=disk["headers"]["image_base"] == mapped["headers"]["image_base"],
        size_of_image=disk["headers"]["size_of_image"] == mapped["headers"]["size_of_image"],
        coff_timestamp=disk["headers"]["coff_timestamp"] == mapped["headers"]["coff_timestamp"],
        section_count=disk["headers"]["section_count"] == mapped["headers"]["section_count"],
        section_table=[(s["name"], s["rva"], s["virtual_size"]) for s in disk["headers"]["sections"]]
        == [(s["name"], s["rva"], s["virtual_size"]) for s in mapped["headers"]["sections"]],
    )
    header_bytes = min(disk["headers"]["size_of_headers"], len(disk["data"]), len(mapped["data"]))
    anchors["headers"] = (disk["data"][:header_bytes] == mapped["data"][:header_bytes]
                          if header_bytes else False)
    sections, undeclared = compare_sources(disk, mapped, mutable_sections)
    matched = [r for r in sections if r["verdict"] == "match"]
    code_sections = [r for r in sections if r["executable"]]
    code_compared = bool(code_sections) and all(r["verdict"] == "match" for r in code_sections)
    reasons = []
    if not all(anchors.values()):
        reasons.append("header anchor mismatch: "
                       + ", ".join(k for k, v in anchors.items() if not v))
    if undeclared:
        reasons.append("file-backed sections differ without being declared mutable: "
                       + ", ".join(undeclared))
    if not matched:
        reasons.append("no file-backed section matched, so nothing anchors the image")
    differing_exec = [r["name"] for r in sections
                      if r["executable"] and r["verdict"] == "different" and not r["declared_mutable"]]
    if differing_exec:
        reasons.append("file-backed executable section bytes differ: " + ", ".join(differing_exec))
    if reasons:
        verdict = "unbound"
    elif code_compared:
        verdict = "bound_by_code"
    else:
        verdict = "bound_by_headers_and_resources"
        reasons.append(f"{len(code_sections)} executable section(s) have no bytes in the installed "
                       "file, so code identity is unavailable rather than verified")
    disk_backed = sum(len(section_bytes(disk, s) or b"") for s in disk["headers"]["sections"])
    return dict(verdict=verdict, anchors=anchors, sections=sections,
                declared_mutable=sorted(mutable_sections), reasons=reasons,
                code_identity=("byte_matched" if code_compared else "unavailable_on_disk"),
                disk_backed_bytes=disk_backed,
                size_of_image=disk["headers"]["size_of_image"],
                disk_backed_fraction=round(disk_backed / disk["headers"]["size_of_image"], 6))


def executable_ranges(headers):
    """Every section the PE marks executable, as RVA ranges."""
    return [(s["rva"], s["rva"] + s["virtual_size"]) for s in headers["sections"]
            if s["flags"] & SCN_MEM_EXECUTE]


def read_export(path, base, ranges):
    """Parse the function export and check its internal claims, bounded and strict."""
    path = Path(path)
    rows, seen, highest = [], 0, -1
    seen_addresses = set()
    unsorted = duplicate = 0
    counts = dict(malformed=0, outside_executable=0, name_mismatch=0)
    samples = {key: [] for key in counts}

    def note(kind, message):
        counts[kind] += 1
        if len(samples[kind]) < MAX_SAMPLE:
            samples[kind].append(message)

    with path.open(encoding="utf-8", errors="strict") as stream:
        header = stream.readline().strip()
        if header != "address,name,size":
            raise BindingError(f"{path}: unexpected header {header!r}, expected 'address,name,size'")
        for line_no, line in enumerate(stream, 2):
            if not line.strip():
                continue
            seen += 1
            if seen > MAX_EXPORT_ROWS:
                raise BindingError(f"{path}: more than {MAX_EXPORT_ROWS} rows")
            parts = line.rstrip("\r\n")
            if "," not in parts:
                note("malformed", f"line {line_no}: no field separators")
                continue
            address_text, rest = parts.split(",", 1)
            # Names may contain commas (demangled templates are unquoted), so the
            # size is always the final field and the name is everything between.
            if "," not in rest:
                note("malformed", f"line {line_no}: missing size field")
                continue
            name, size_text = rest.rsplit(",", 1)
            name = name.strip('"')
            try:
                address = int(address_text, 16)
                size = int(size_text)
            except ValueError:
                note("malformed", f"line {line_no}: unparsable address/size")
                continue
            if size <= 0:
                note("malformed", f"line {line_no}: non-positive size {size}")
                continue
            if address < highest:
                unsorted += 1
            highest = max(highest, address)
            if address in seen_addresses:
                duplicate += 1
            seen_addresses.add(address)
            rva = address - base
            if rva < 0 or not any(start <= rva and rva + size <= end for start, end in ranges):
                note("outside_executable",
                     f"line {line_no}: {address_text} + {size} is not inside an executable section")
            if name.startswith("FUN_") and name != f"FUN_{address:08x}":
                note("name_mismatch",
                     f"line {line_no}: auto-name {name!r} disagrees with {address_text}")
            rows.append((address, size))
    if not rows:
        raise BindingError(f"{path}: no usable function rows")
    return dict(path=str(path), rows=len(rows), unsorted=unsorted, duplicates=duplicate,
                problems=sum(counts.values()), problem_counts=counts, problem_sample=samples,
                executable_ranges=[[f"0x{s:08x}", f"0x{e:08x}"] for s, e in ranges],
                addresses=seen_addresses,
                sizes={address: size for address, size in rows})


def pdata_starts(image):
    """Read the x64 exception directory: the binary's own function table."""
    if len(image.directories) <= 3:
        raise BindingError("PE has no exception directory")
    rva, size = image.directories[3]
    if not rva or not size:
        raise BindingError("PE exception directory is empty")
    if size % 12 or size > 32 * 1024 * 1024:
        raise BindingError("PE exception directory has an invalid size")
    try:
        table = image.read(rva, size)
    except ValueError as error:
        raise BindingError(f"PE exception directory is not file-backed: {error}") from error
    starts = []
    for start, end, _ in struct.iter_unpack("<III", table):
        if start and end > start:
            starts.append(start)
    return rva, starts


def export_binding(export, base, directory_rva, starts, text, minimum_coverage):
    """Test the export against the binary's own unwind table.

    Chained x64 unwind entries describe fragments of one function and Ghidra merges
    them, so an exact address match cannot be required for every start. A start
    that is not an exported entry but lies inside an exported function body is
    counted as resolved-by-containment and reported separately; only a start that
    is inside no exported function at all counts as absent.
    """
    spans = sorted((address, address + export["sizes"][address]) for address in export["addresses"])
    span_starts = [start for start, _ in spans]

    def classify(rva):
        va = base + rva
        if va in export["addresses"]:
            return "exact"
        index = bisect.bisect_right(span_starts, va) - 1
        if index >= 0 and spans[index][0] < va < spans[index][1]:
            return "contained"
        return "absent"

    verdicts = [classify(rva) for rva in starts]
    exact = verdicts.count("exact")
    contained = verdicts.count("contained")
    absent = verdicts.count("absent")
    total = len(starts)
    resolved = round((exact + contained) / total, 6) if total else 0.0
    reasons = []
    if export["problems"]:
        reasons.append(f"{export['problems']} export rows failed internal checks")
    if export["unsorted"] or export["duplicates"]:
        reasons.append(f"{export['unsorted']} unsorted and {export['duplicates']} duplicate addresses")
    if resolved < minimum_coverage:
        reasons.append(f"resolved unwind-start coverage {resolved} is below the required "
                       f"{minimum_coverage}")
    absent_rvas = [rva for rva, verdict in zip(starts, verdicts) if verdict == "absent"]
    return dict(verdict="unbound" if reasons else "bound", reasons=reasons,
                exception_directory_rva=f"0x{directory_rva:08x}",
                unwind_starts=total, exact=exact, contained=contained, absent=absent,
                exact_coverage=round(exact / total, 6) if total else 0.0,
                resolved_coverage=resolved, minimum_coverage=minimum_coverage,
                absent_sample=[f"0x{base + rva:08x}" for rva in absent_rvas[:MAX_SAMPLE]],
                text_rva=f"0x{text['rva']:08x}")


def probe(addresses, export, unwind_starts, image, base, text):
    rows = []
    for address in addresses:
        rva = address - base
        row = dict(va=f"0x{address:08x}", rva=f"0x{rva:08x}")
        inside = text["rva"] <= rva < text["rva"] + text["virtual_size"]
        row["inside_text"] = inside
        row["exported"] = address in export["addresses"]
        row["export_size"] = export["sizes"].get(address)
        row["unwind_start"] = rva in unwind_starts
        if inside:
            length = min(32, text["rva"] + text["virtual_size"] - rva)
            row["first_bytes"] = image.read(rva, length).hex()
            size = row["export_size"]
            if size and size <= MAX_PROBE_BYTES and rva + size <= text["rva"] + text["virtual_size"]:
                row["body_sha256"] = sha256(image.read(rva, size))
                row["body_bytes"] = size
            row["all_zero"] = not any(image.read(rva, length))
        else:
            row["first_bytes"] = None
            row["all_zero"] = None
        rows.append(row)
    return rows


def decomp_addresses(corpus_dir, limit=32):
    """Candidate probe VAs from the address-named decompile exports, bounded."""
    if corpus_dir is None:
        return []
    found = []
    for path in sorted(Path(corpus_dir).glob("decomp_*.c"))[:limit]:
        match = DECOMP_NAME.match(path.name)
        if match:
            found.append(int(match.group(1), 16))
    return sorted(found)


def bind(executable, image_path, dump_path, export_path, corpus_dir=None,
         mutable_sections=(), extra_probes=(), minimum_coverage=0.99):
    disk = load_image(executable, False, "installed executable")
    mapped = load_image(image_path, True, "mapped image")
    report = dict(
        schema_version=1,
        generated_utc=datetime.now(timezone.utc).isoformat(),
        inputs=dict(installed_executable=dict(path=disk["path"], size=disk["size"],
                                              sha256=disk["sha256"], **{"pe": {
                                                  k: disk["headers"][k] for k in
                                                  ("image_base", "size_of_image", "entry_rva",
                                                   "coff_timestamp", "section_count")}}),
                    mapped_image=dict(path=mapped["path"], size=mapped["size"],
                                      sha256=mapped["sha256"])),
        limitations=[
            "A hash identifies a file, not a build. Header and resource agreement binds the "
            "mapped image to the installed file only as far as the compared ranges extend.",
            "Code bytes are absent from the packed installed file, so code identity is "
            "unavailable and must not be reported as verified or matched.",
            "Unwind starts are lower bounds: leaf and unregistered functions are absent from "
            "the exception directory, so export coverage is a test, not a function count.",
            "This tool does not decompile, execute, hook or compare behaviour."],
    )
    if dump_path is not None:
        dump = load_image(dump_path, True, "reconstructed PE", strict_length=False)
        report["inputs"]["reconstructed_pe"] = dict(path=dump["path"], size=dump["size"],
                                                    sha256=dump["sha256"],
                                                    shortfall_bytes=dump["shortfall_bytes"])
        overlap = diff_runs(mapped["data"], dump["data"])
        same_table = [(s["name"], s["rva"]) for s in mapped["headers"]["sections"]] == \
                     [(s["name"], s["rva"]) for s in dump["headers"]["sections"]]
        report["mapped_image_vs_reconstructed_pe"] = dict(
            length_delta=dump["size"] - mapped["size"], section_layout_equal=same_table,
            **overlap)
    binding = image_binding(disk, mapped, set(mutable_sections))
    report["image_binding"] = binding
    text = next((s for s in mapped["headers"]["sections"] if s["flags"] & SCN_CNT_CODE), None)
    if text is None:
        raise BindingError("mapped image has no code section")
    ranges = executable_ranges(mapped["headers"])
    export = read_export(export_path, mapped["headers"]["image_base"], ranges)
    directory_rva, starts = pdata_starts(mapped["image"])
    unwind = set(starts)
    report["export_binding"] = export_binding(export, mapped["headers"]["image_base"],
                                              directory_rva, unwind, text, minimum_coverage)
    addresses = sorted(set(extra_probes) | set(decomp_addresses(corpus_dir)))
    report["probes"] = probe(addresses, export, unwind, mapped["image"],
                             mapped["headers"]["image_base"], text)
    report["export_binding"]["export"] = {k: v for k, v in export.items()
                                          if k not in ("addresses", "sizes")}
    image_ok = binding["verdict"].startswith("bound")
    export_ok = report["export_binding"]["verdict"] == "bound"
    if not (image_ok and export_ok):
        verdict = "unbound"
    elif binding["verdict"] == "bound_by_code":
        verdict = "bound"
    else:
        # Anchored by headers and file-backed resources only: the code sections are
        # absent from the installed file, so no code comparison happened.
        verdict = "bound_without_code_comparison"
    report["verdict"] = verdict
    report["verdict_detail"] = dict(
        image=binding["verdict"], export=report["export_binding"]["verdict"],
        note="bound means the mapped image is anchored to the installed file and the export "
             "resolves to it; it never means the code was byte-compared, unless code_identity "
             "says byte_matched")
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--executable", type=Path, required=True,
                        help="installed ACU.exe (packed or unpacked) to bind to")
    parser.add_argument("--image", type=Path, required=True,
                        help="mapped runtime image laid out by RVA")
    parser.add_argument("--dump", type=Path, help="reconstructed PE rebuilt from the same image")
    parser.add_argument("--export", type=Path, required=True,
                        help="function export CSV with header address,name,size")
    parser.add_argument("--corpus", type=Path, help="directory with decomp_<VA>.c exports to probe")
    parser.add_argument("--mutable-section", action="append", default=[],
                        help="section allowed to differ at runtime (repeatable)")
    parser.add_argument("--probe", action="append", default=[], type=lambda v: int(v, 0),
                        help="extra VA to probe (repeatable)")
    parser.add_argument("--min-coverage", type=float, default=0.99,
                        help="required unwind-start coverage of the export (default 0.99)")
    parser.add_argument("--output", type=Path, required=True, help="JSON report destination")
    args = parser.parse_args(argv)
    report = bind(args.executable, args.image, args.dump, args.export, args.corpus,
                  args.mutable_section, args.probe, args.min_coverage)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2), encoding="utf-8")
    binding = report["image_binding"]
    print(json.dumps(dict(
        verdict=report["verdict"], detail=report["verdict_detail"],
        installed_executable=report["inputs"]["installed_executable"]["sha256"],
        mapped_image=report["inputs"]["mapped_image"]["sha256"],
        code_identity=binding["code_identity"],
        disk_backed_fraction=binding["disk_backed_fraction"],
        export_coverage=report["export_binding"]["resolved_coverage"],
        export_exact_coverage=report["export_binding"]["exact_coverage"],
        export_absent=report["export_binding"]["absent"],
        probes=[(p["va"], p["exported"], p["first_bytes"]) for p in report["probes"]],
        report=str(args.output)), indent=2))
    return 0 if report["verdict"] in ("bound", "bound_without_code_comparison") else 1


if __name__ == "__main__":
    raise SystemExit(main())
