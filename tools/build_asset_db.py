#!/usr/bin/env python3
"""Build a local SQLite asset index from an acu-scan entries.jsonl (#128).

Standard library only. The database is derived, local and never committed: it holds
archive/entry/resource provenance (IDs, offsets, lengths, hashes), not game bytes. Nothing
is rescanned; the archives are not opened except to stat their size.

    python tools/build_asset_db.py SCAN_DIR_OR_ENTRIES_JSONL --output local/asset_index.sqlite
        [--game-dir DIR] [--type-names names.json] [--replace]
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import sqlite3
import sys

SCHEMA_VERSION = 1
BATCH = 20000

SCHEMA = """
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE archive (
    id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, size_bytes INTEGER,
    baseline INTEGER, scan_size_bytes INTEGER);
-- size_bytes: file size when this index was built. scan_size_bytes: size recorded by the scan.
-- They differ when the archive changed between the scan and the build.  -- 1 original carrier, 0 excluded, NULL unknown (no install given)
CREATE TABLE entry (
    id INTEGER PRIMARY KEY, archive_id INTEGER NOT NULL REFERENCES archive(id),
    idx INTEGER NOT NULL, file_id TEXT NOT NULL, offset INTEGER NOT NULL,
    record_offset INTEGER NOT NULL, stored_length INTEGER NOT NULL,
    kind TEXT NOT NULL, typed_status TEXT, stored_sha256 TEXT, decoded_sha256 TEXT,
    resource_envelopes_parsed INTEGER);
CREATE TABLE resource (
    id INTEGER PRIMARY KEY, entry_id INTEGER NOT NULL REFERENCES entry(id),
    ordinal INTEGER NOT NULL, file_id TEXT, name TEXT, type_id TEXT, type_name TEXT,
    record_offset INTEGER, record_length INTEGER, payload_start INTEGER,
    payload_end INTEGER, payload_sha256 TEXT);
-- Raw texture descriptor fields from the scan. map_type/category/user_category are stored as
-- recorded; their meaning (normal, specular, atlas, ...) is not established here.
CREATE TABLE texture (
    resource_id INTEGER PRIMARY KEY REFERENCES resource(id), map_type INTEGER,
    category INTEGER, user_category INTEGER, kind INTEGER, dxgi_format INTEGER,
    width INTEGER, height INTEGER, depth INTEGER, layers INTEGER, mip_count INTEGER,
    storage TEXT, gamma INTEGER, dynamic INTEGER);
"""
INDEXES = """
CREATE INDEX entry_file_id ON entry(file_id);
CREATE INDEX entry_archive ON entry(archive_id, idx);
CREATE INDEX resource_entry ON resource(entry_id);
CREATE INDEX resource_file_id ON resource(file_id);
CREATE INDEX resource_type ON resource(type_id);
CREATE INDEX resource_name ON resource(name);
"""


class ScanError(ValueError):
    pass


def archive_name(recorded, game_dir):
    """Stable archive name: path relative to the install when possible, else the basename."""
    path = recorded.replace("\\", "/")
    if game_dir:
        base = str(game_dir).replace("\\", "/").rstrip("/") + "/"
        if path.lower().startswith(base.lower()):
            return path[len(base):]
    return path.rsplit("/", 1)[-1]


def need(row, key, where, kind):
    value = row.get(key)
    if not isinstance(value, kind):
        raise ScanError(f"{where}: missing or invalid '{key}'")
    return value


def rows_from(path, label=False):
    """Yield (location, row). The location names the file only for override scans."""
    with open(path, encoding="utf-8") as stream:
        for number, line in enumerate(stream, 1):
            if not line.strip():
                continue
            where = f"{Path(path).name} line {number}" if label else f"line {number}"
            try:
                row = json.loads(line)
            except json.JSONDecodeError as error:
                raise ScanError(f"{where}: invalid JSON ({error.msg})") from None
            if not isinstance(row, dict):
                raise ScanError(f"{where}: row is not an object")
            yield where, row


def manifest_sizes(entries_path, game_dir):
    """Archive sizes the scan recorded in the summary.json next to its entries.jsonl."""
    summary = Path(entries_path).with_name("summary.json")
    if not summary.is_file():
        return {}
    try:
        manifest = json.loads(summary.read_text(encoding="utf-8")).get("manifest") or []
        return {archive_name(m["path"], game_dir): m["length"] for m in manifest}
    except (ValueError, KeyError, TypeError, AttributeError):
        raise ScanError(f"{summary}: unreadable scan manifest") from None


def merged_rows(base, overrides, game_dir):
    """Base scan rows, minus any archive an override scan contains, then the override rows.

    Used when an archive changed after the base scan: the override replaces it whole, so
    rows from two different versions of one archive never mix.
    """
    replaced = set()
    for path in overrides:
        for where, row in rows_from(path, label=True):
            replaced.add(archive_name(need(row, "archive", where, str), game_dir))
    for where, row in rows_from(base):
        if archive_name(need(row, "archive", where, str), game_dir) not in replaced:
            yield where, row
    for path in overrides:
        yield from rows_from(path, label=True)


def build(entries_path, output, game_dir=None, type_names=None, baseline_names=None,
          replace=False, overrides=()):
    output = Path(output)
    if output.exists():
        if not replace:
            raise FileExistsError(f"{output} exists; pass --replace to rebuild")
        output.unlink()
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_name(output.name + ".partial")
    if temporary.exists():
        temporary.unlink()
    type_names = type_names or {}
    scan_sizes = manifest_sizes(entries_path, game_dir)
    for extra in overrides:
        scan_sizes.update(manifest_sizes(extra, game_dir))
    db = sqlite3.connect(temporary)
    try:
        db.executescript("PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF;" + SCHEMA)
        archives, entries, resources, textures = {}, [], [], []
        counts = {"entries": 0, "controls": 0, "resources": 0, "textures": 0}
        entry_id = 0
        resource_id = 0

        def flush():
            db.executemany("INSERT INTO entry VALUES (?,?,?,?,?,?,?,?,?,?,?,?)", entries)
            db.executemany("INSERT INTO resource VALUES (?,?,?,?,?,?,?,?,?,?,?,?)", resources)
            db.executemany("INSERT INTO texture VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)", textures)
            entries.clear()
            resources.clear()
            textures.clear()

        for line_no, row in merged_rows(entries_path, list(overrides), game_dir):
            recorded = need(row, "archive", line_no, str)
            entry = need(row, "entry", line_no, dict)
            name = archive_name(recorded, game_dir)
            if name not in archives:
                size = None
                if game_dir and (Path(game_dir) / name).is_file():
                    size = (Path(game_dir) / name).stat().st_size
                baseline = None if baseline_names is None else int(name in baseline_names)
                archives[name] = len(archives) + 1
                db.execute("INSERT INTO archive VALUES (?,?,?,?,?)",
                           (archives[name], name, size, baseline, scan_sizes.get(name)))
            entry_id += 1
            kind = "control" if "control" in row else "data"
            counts["controls" if kind == "control" else "entries"] += 1
            entries.append((
                entry_id, archives[name], need(entry, "index", line_no, int),
                need(entry, "file_id", line_no, str), need(entry, "offset", line_no, int),
                need(entry, "record_offset", line_no, int),
                need(entry, "stored_length", line_no, int), kind, row.get("typed_status"),
                row.get("stored_sha256"), row.get("decoded_sha256"),
                row.get("resource_envelopes_parsed")))
            for ordinal, res in enumerate(row.get("resources") or []):
                if not isinstance(res, dict):
                    raise ScanError(f"{line_no}: resource {ordinal} is not an object")
                payload = res.get("payload") or {}
                type_id = res.get("type_id")
                resource_id += 1
                tex = res.get("texture")
                if isinstance(tex, dict):
                    textures.append((
                        resource_id, tex.get("map_type"), tex.get("category"),
                        tex.get("user_category"), tex.get("kind"), tex.get("dxgi_format"),
                        tex.get("width"), tex.get("height"), tex.get("depth"),
                        tex.get("layers"), tex.get("mip_count"), tex.get("storage"),
                        tex.get("gamma"), None if tex.get("dynamic") is None
                        else int(bool(tex["dynamic"]))))
                    counts["textures"] += 1
                resources.append((
                    resource_id, entry_id, ordinal, res.get("file_id"), res.get("name") or None, type_id,
                    type_names.get(type_id), res.get("record_offset"),
                    res.get("record_length"), payload.get("start"), payload.get("end"),
                    res.get("payload_sha256")))
                counts["resources"] += 1
            if len(entries) + len(resources) >= BATCH:
                flush()
        flush()
        if not archives:
            raise ScanError("no rows: the scan file is empty")
        meta = {"schema_version": SCHEMA_VERSION, "source": str(entries_path),
                "overrides": ",".join(str(o) for o in overrides),
                "game_dir": str(game_dir), **counts}
        for key, value in meta.items():
            db.execute("INSERT INTO meta VALUES (?,?)", (key, str(value)))
        db.executescript(INDEXES)
        db.commit()
    except BaseException:
        db.close()
        temporary.unlink(missing_ok=True)
        raise
    db.close()
    os.replace(temporary, output)
    return counts | {"archives": len(archives)}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("scan", help="acu-scan directory or its entries.jsonl")
    parser.add_argument("--output", required=True)
    parser.add_argument("--game-dir", help="install dir (default: ACU_GAME_DIR if set)")
    parser.add_argument("--type-names", help="optional JSON {type_id_hex: name}")
    parser.add_argument("--replace", action="store_true")
    parser.add_argument("--override", action="append", default=[], metavar="SCAN",
                        help="rescan of archives that changed since the base scan; its "
                             "archives replace the base rows entirely (repeatable)")
    args = parser.parse_args(argv)
    scan = Path(args.scan)
    entries = scan / "entries.jsonl" if scan.is_dir() else scan
    game_dir = args.game_dir or os.environ.get("ACU_GAME_DIR")
    names = json.loads(Path(args.type_names).read_text("utf-8")) if args.type_names else None
    baseline_names = None
    if game_dir:
        try:  # tools/baseline.py arrives with #37; until then the flag stays NULL (unknown)
            import baseline
            baseline_names = {c["path"] for c in baseline.classify(game_dir)["carriers"]}
        except ModuleNotFoundError:
            print("note: tools/baseline.py not present; archive.baseline left NULL",
                  file=sys.stderr)
    try:
        overrides = [Path(o) / "entries.jsonl" if Path(o).is_dir() else Path(o)
                     for o in args.override]
        counts = build(entries, args.output, game_dir, names, baseline_names, args.replace,
                       overrides)
    except (ScanError, FileExistsError, OSError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(json.dumps(counts))
    return 0


if __name__ == "__main__":
    sys.exit(main())
