#!/usr/bin/env python3
"""Join mapped-image unwind ranges to address-named ACU decompile files.

The join is an RVA/name discovery aid. It does not establish that the decompiler
corpus came from the supplied installed executable or that an unwind range is a
whole function. Keep the source hashes and review each candidate before hooking.
"""
import argparse
import bisect
import hashlib
import json
import os
from pathlib import Path
import re


ADDRESS_FILE = re.compile(r"^([0-9a-fA-F]{8,16})\.c$")
HEADER = re.compile(
    r"^\s*//\s*(?:0x)?([0-9a-fA-F]{8,16})\s+([A-Za-z_$?][\w@$?]*)\s+size=(\d+)"
)


def read_jsonl(path):
    rows = []
    with Path(path).open(encoding="utf-8") as source:
        for line_no, line in enumerate(source, 1):
            if not line.strip():
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError as error:
                raise ValueError(f"{path}:{line_no}: invalid JSON: {error}") from error
            start = int(row["rva"], 16)
            end = int(row["end_rva_exclusive"], 16)
            if start < 0 or end <= start:
                raise ValueError(f"{path}:{line_no}: invalid function range")
            rows.append((start, end, row))
    rows.sort(key=lambda item: (item[0], item[1]))
    return rows


def iter_candidates(root, preferred_base, image_size, max_files):
    """Walk corpus filenames only; read function headers only after an RVA join."""
    root = Path(root).resolve()
    seen = 0
    valid = 0
    stack = [root]
    while stack:
        directory = stack.pop()
        with os.scandir(directory) as entries:
            for entry in entries:
                if entry.is_symlink():
                    continue
                if entry.is_dir(follow_symlinks=False):
                    stack.append(Path(entry.path))
                    continue
                if not entry.is_file(follow_symlinks=False) or not entry.name.endswith(".c"):
                    continue
                seen += 1
                if seen > max_files:
                    raise ValueError(
                        f"corpus exceeds --max-files={max_files}; increase the explicit bound"
                    )
                match = ADDRESS_FILE.fullmatch(entry.name)
                if not match:
                    continue
                va = int(match.group(1), 16)
                if not preferred_base <= va < preferred_base + image_size:
                    continue
                valid += 1
                yield dict(va=va, rva=va - preferred_base,
                           path=Path(entry.path).relative_to(root).as_posix())
    return seen, valid


def candidate_header(corpus_root, candidate):
    path = Path(corpus_root, candidate["path"])
    with path.open("r", encoding="utf-8", errors="replace") as source:
        header = source.readline().rstrip("\r\n")
    match = HEADER.match(header)
    if not match:
        return dict(header=header[:512], symbol=None, declared_size=None,
                    header_va_matches_filename=None)
    header_va = int(match.group(1), 16)
    return dict(header=header[:512], symbol=match.group(2),
                declared_size=int(match.group(3)),
                header_va_matches_filename=header_va == candidate["va"],
                header_va=f"0x{header_va:016x}")


def correlate(function_rows, candidates, corpus_root, image_summary):
    starts = [row[0] for row in function_rows]
    exact = {}
    for start, end, row in function_rows:
        exact.setdefault(start, []).append((end, row))
    joined = []
    corpus_unmatched = 0
    exact_count = 0
    containing_count = 0
    for candidate in candidates:
        rva = candidate["rva"]
        relations = []
        for end, row in exact.get(rva, []):
            relations.append(("exact_unwind_start", row))
        if not relations:
            index = bisect.bisect_right(starts, rva) - 1
            if index >= 0:
                start, end, row = function_rows[index]
                if start < rva < end:
                    relations.append(("inside_unwind_range", row))
        if not relations:
            corpus_unmatched += 1
            continue
        header = candidate_header(corpus_root, candidate)
        for relation, row in relations:
            if relation == "exact_unwind_start":
                exact_count += 1
            else:
                containing_count += 1
            joined.append(dict(
                relation=relation,
                confidence="address-only candidate; corpus-to-installed-image identity unverified",
                corpus_va=f"0x{candidate['va']:016x}",
                rva=f"{candidate['rva']:08x}",
                corpus_path=candidate["path"],
                **header,
                native_range=dict(start_rva=row["rva"],
                                  end_rva_exclusive=row["end_rva_exclusive"],
                                  unwind_rva=row.get("unwind_rva"),
                                  expected_bytes=row.get("expected_bytes"),
                                  evidence=row.get("evidence"),
                                  semantics="unknown"),
                image_sha256=image_summary.get("source_sha256"),
                image_layout=image_summary.get("layout"),
                preferred_image_base=image_summary.get("preferred_image_base"),
                runtime_base=image_summary.get("runtime_base"),
            ))
    return joined, dict(exact_start_matches=exact_count,
                        inside_unwind_range_matches=containing_count,
                        address_candidates_without_unwind_match=corpus_unmatched)


def correlate_session(session_path, function_rows, by_range, image_summary, output_dir):
    runtime_base = image_summary.get("runtime_base")
    if runtime_base is None:
        raise ValueError("live event correlation needs --live-base in the image inventory")
    runtime_base = int(runtime_base, 16)
    image_size = int(image_summary["image_size"])
    starts = [row[0] for row in function_rows]
    modules = None
    event_count = 0
    main_count = 0
    joined_count = 0
    output_path = output_dir / "live-events.jsonl"
    with Path(session_path).open(encoding="utf-8") as source, output_path.open(
            "w", encoding="utf-8", newline="\n") as sink:
        for line_no, line in enumerate(source, 1):
            if not line.strip():
                continue
            row = json.loads(line)
            if row.get("kind") == "header":
                modules = row.get("modules", [])
            for event in row.get("events", []):
                event_count += 1
                rip = int(event["rip"], 16)
                module = next((m for m in (modules or [])
                               if int(m["base"], 16) <= rip < int(m["base"], 16)+m["size"]), None)
                item = dict(event=event, session_line=line_no,
                            module_name=module.get("name") if module else None,
                            image_rva=None, relation=None, native_range=None,
                            decompile_candidates=[])
                if module and int(module["base"], 16) == runtime_base and runtime_base <= rip < runtime_base+image_size:
                    main_count += 1
                    rva = rip-runtime_base
                    index = bisect.bisect_right(starts, rva)-1
                    match = None
                    relation = None
                    if index >= 0:
                        start, end, candidate = function_rows[index]
                        if rva == start:
                            match, relation = candidate, "exact_unwind_start"
                        elif start < rva < end:
                            match, relation = candidate, "inside_unwind_range"
                    item["image_rva"] = f"{rva:08x}"
                    if match:
                        key = (match["rva"], match["end_rva_exclusive"])
                        item["native_range"] = match
                        item["relation"] = relation
                        item["decompile_candidates"] = [dict(
                            path=x["corpus_path"], va=x["corpus_va"], symbol=x.get("symbol"),
                            relation=x["relation"]) for x in by_range.get(key, [])]
                        joined_count += 1
                    else:
                        item["relation"] = "no_unwind_range_at_event_rip"
                elif module:
                    item["relation"] = "event_in_other_module"
                else:
                    item["relation"] = "event_address_outside_captured_modules"
                item["confidence"] = "live RIP/module observation; name match is address-only and corpus identity unverified"
                sink.write(json.dumps(item, ensure_ascii=False)+"\n")
    return dict(session_path=str(Path(session_path).resolve()),
                session_sha256=hashlib.sha256(Path(session_path).read_bytes()).hexdigest(),
                live_events=event_count, events_in_main_image=main_count,
                events_joined_to_unwind_ranges=joined_count,
                events_output=str(output_path.resolve()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("functions", type=Path,
                        help="functions.jsonl from tools/native_image.py")
    parser.add_argument("image_summary", type=Path,
                        help="summary.json from the same native_image.py run")
    parser.add_argument("corpus", type=Path, help="ACU - Decompiled directory")
    parser.add_argument("output", type=Path, help="output directory; keep under ignored local/")
    parser.add_argument("--max-files", type=int, default=300_000)
    parser.add_argument("--session", type=Path,
                        help="optional JSONL trace from tools/native_probe/client.py session")
    args = parser.parse_args()
    if not 1 <= args.max_files <= 2_000_000:
        parser.error("--max-files must be 1..2000000")
    summary = json.loads(args.image_summary.read_text(encoding="utf-8"))
    if summary.get("layout") != "mapped":
        parser.error("xref requires a mapped-image inventory")
    base = int(summary["preferred_image_base"], 16)
    image_size = int(summary["image_size"])
    if not 0 < image_size <= 1024**3:
        parser.error("invalid PE image size")
    ranges = read_jsonl(args.functions)
    candidates = list(iter_candidates(args.corpus, base, image_size, args.max_files))
    matches, counts = correlate(ranges, candidates, args.corpus, summary)
    args.output.mkdir(parents=True, exist_ok=True)
    output_path = args.output / "decompile-xref.jsonl"
    with output_path.open("w", encoding="utf-8", newline="\n") as sink:
        for row in matches:
            sink.write(json.dumps(row, ensure_ascii=False) + "\n")
    by_range = {}
    for match in matches:
        key = (match["native_range"]["start_rva"],
               match["native_range"]["end_rva_exclusive"])
        by_range.setdefault(key, []).append(match)
    native_path = args.output / "native-functions.jsonl"
    with native_path.open("w", encoding="utf-8", newline="\n") as sink:
        for start, end, row in ranges:
            names = by_range.get((row["rva"], row["end_rva_exclusive"]), [])
            result_row = dict(
                native_range=row,
                decompile_match_status=(
                    "matched_exact_start" if any(x["relation"] == "exact_unwind_start" for x in names)
                    else "matched_inside_range" if names else "no_address_match"),
                decompile_candidates=[dict(
                    path=x["corpus_path"], va=x["corpus_va"], symbol=x.get("symbol"),
                    declared_size=x.get("declared_size"), relation=x["relation"],
                    header_va_matches_filename=x.get("header_va_matches_filename"))
                    for x in names],
                name_confidence=("address-only candidate; verify binary identity and native behavior"
                                 if names else "unnamed by this corpus join"),
                image_sha256=summary.get("source_sha256"),
                preferred_image_base=summary.get("preferred_image_base"),
                runtime_base=summary.get("runtime_base"),
            )
            sink.write(json.dumps(result_row, ensure_ascii=False) + "\n")
    session_summary = None
    if args.session:
        session_summary = correlate_session(args.session, ranges, by_range, summary, args.output)
    result = dict(
        schema_version=1,
        corpus_root=str(args.corpus.resolve()),
        corpus_filename_candidates=len(candidates),
        unwind_ranges=len(ranges),
        **counts,
        matched_rows=len(matches),
        native_ranges_with_decompile_names=sum(bool(by_range.get((row["rva"], row["end_rva_exclusive"])))
                                               for _, _, row in ranges),
        native_ranges_without_decompile_names=len(ranges)-sum(
            bool(by_range.get((row["rva"], row["end_rva_exclusive"]))) for _, _, row in ranges),
        image_inventory_path=str(args.image_summary.resolve()),
        image_inventory_sha256=hashlib.sha256(args.image_summary.read_bytes()).hexdigest(),
        warning=("Address joins are discovery candidates only. Decompiled filenames encode export VAs; "
                 "their correspondence to this installed executable is unverified. "
                 "Unwind ranges can be fragments, and names do not establish native semantics."),
        output=str(output_path.resolve()),
        native_functions_output=str(native_path.resolve()),
        live_session=session_summary,
    )
    (args.output / "decompile-xref-summary.json").write_text(
        json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: result[key] for key in (
        "corpus_filename_candidates", "unwind_ranges", "exact_start_matches",
        "inside_unwind_range_matches", "address_candidates_without_unwind_match",
        "matched_rows", "output")}, ensure_ascii=False))


if __name__ == "__main__":
    main()
