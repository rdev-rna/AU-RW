#!/usr/bin/env python3
"""Candidate link graph over the local asset index (#132).

    python tools/asset_graph.py merge  INDEX.sqlite links.tsv [more.tsv ...] [--replace]
    python tools/asset_graph.py walk   INDEX.sqlite NAME_OR_ID [--up] [--depth 3] [--limit 200]
    python tools/asset_graph.py stats  INDEX.sqlite

`merge` loads `acu-links` output. Every link is a *candidate*: eight bytes in the source
resource's payload equal a known file ID. It is not a decoded reference and says nothing about
the field's meaning. Standard library only; the database holds metadata, never game bytes.
"""

from __future__ import annotations

import argparse
import csv
import json
import re
import sqlite3
import sys

HEX_ID = re.compile(r"^[0-9a-fA-F]{16}$")
# Calibrated on the full scan (control IDs of the same size): from 25 bits up, same-sized fake IDs
# matched ~0.002% as often as real ones. At 24 bits and below a match is not evidence.
CONFIDENT_FROM = "0000000002000000"  # 2**25 as fixed-width hex; compares as text

SCHEMA = """
CREATE TABLE link (
    id INTEGER PRIMARY KEY, src_resource_id INTEGER NOT NULL REFERENCES resource(id),
    offset INTEGER NOT NULL, dst_file_id TEXT NOT NULL, hits INTEGER NOT NULL DEFAULT 1,
    kind TEXT NOT NULL DEFAULT 'candidate');
CREATE INDEX link_src ON link(src_resource_id);
CREATE INDEX link_dst ON link(dst_file_id);
CREATE VIEW link_view AS
SELECT l.id AS link_id, sa.name AS src_archive, sr.file_id AS src_file_id, sr.name AS src_name,
       sr.type_id AS src_type, l.offset AS offset, l.hits AS hits, l.kind AS kind, l.dst_file_id AS dst_file_id,
       dr.name AS dst_name, dr.type_id AS dst_type, da.name AS dst_archive
FROM link l
JOIN resource sr ON sr.id = l.src_resource_id
JOIN entry se ON se.id = sr.entry_id JOIN archive sa ON sa.id = se.archive_id
LEFT JOIN resource dr ON dr.file_id = l.dst_file_id
LEFT JOIN entry de ON de.id = dr.entry_id LEFT JOIN archive da ON da.id = de.archive_id;
-- Shared / atlas candidates: how many distinct source resources point at each target.
CREATE VIEW fan_in AS
SELECT l.dst_file_id AS dst_file_id, COUNT(DISTINCT l.src_resource_id) AS parents,
       COUNT(DISTINCT sr.file_id) AS distinct_parent_ids
FROM link l JOIN resource sr ON sr.id = l.src_resource_id GROUP BY l.dst_file_id;
"""


def read_tsv(path):
    with open(path, encoding="utf-8", newline="") as stream:
        reader = csv.reader(stream, delimiter="\t")
        header = next(reader, None)
        expected = ["archive", "entry", "ordinal", "src_file_id", "offset", "dst_file_id", "hits"]
        if header != expected:
            raise ValueError(f"{path}: unexpected header {header}")
        for number, row in enumerate(reader, 2):
            if len(row) != 7 or not HEX_ID.match(row[3]) or not HEX_ID.match(row[5]):
                raise ValueError(f"{path} line {number}: malformed row")
            try:
                yield (row[0], int(row[1]), int(row[2]), row[3].lower(), int(row[4]), row[5].lower(),
                       int(row[6]))
            except ValueError:
                raise ValueError(f"{path} line {number}: non-numeric field") from None


def merge(db_path, tsv_paths, replace=False):
    db = sqlite3.connect(db_path)
    try:
        have = db.execute("SELECT 1 FROM sqlite_master WHERE name = 'link'").fetchone()
        if have and not replace:
            raise FileExistsError("link tables already exist; pass --replace")
        if have:
            db.executescript("DROP VIEW IF EXISTS fan_in; DROP VIEW IF EXISTS link_view;"
                             "DROP TABLE link;")
        db.execute("DROP TABLE IF EXISTS link_stage")
        db.execute("CREATE TEMP TABLE link_stage (archive TEXT, entry INTEGER, ordinal INTEGER,"
                   " src_file_id TEXT, offset INTEGER, dst_file_id TEXT, hits INTEGER)")
        staged = 0
        for path in tsv_paths:
            batch = []
            for row in read_tsv(path):
                batch.append(row)
                if len(batch) >= 50000:
                    db.executemany("INSERT INTO link_stage VALUES (?,?,?,?,?,?,?)", batch)
                    staged += len(batch)
                    batch = []
            db.executemany("INSERT INTO link_stage VALUES (?,?,?,?,?,?,?)", batch)
            staged += len(batch)
        db.executescript(SCHEMA)
        db.execute("""
            INSERT INTO link (src_resource_id, offset, dst_file_id, hits, kind)
            SELECT r.id, s.offset, s.dst_file_id, s.hits,
                   CASE WHEN s.dst_file_id >= ? THEN 'candidate' ELSE 'weak' END
            FROM link_stage s
            JOIN archive a ON a.name = s.archive
            JOIN entry e ON e.archive_id = a.id AND e.idx = s.entry
            JOIN resource r ON r.entry_id = e.id AND r.ordinal = s.ordinal
            WHERE r.file_id = s.src_file_id""", (CONFIDENT_FROM,))
        inserted = db.execute("SELECT COUNT(*) FROM link").fetchone()[0]
        # Rows with no matching (archive, entry, ordinal) vs rows whose source ID disagrees.
        located = db.execute("""
            SELECT COUNT(*) FROM link_stage s
            JOIN archive a ON a.name = s.archive
            JOIN entry e ON e.archive_id = a.id AND e.idx = s.entry
            JOIN resource r ON r.entry_id = e.id AND r.ordinal = s.ordinal""").fetchone()[0]
        report = {"staged": staged, "linked": inserted, "orphans": staged - located,
                  "source_id_mismatches": located - inserted}
        db.execute("DELETE FROM meta WHERE key LIKE 'link_%'")
        db.executemany("INSERT INTO meta VALUES (?, ?)",
                       [(f"link_{key}", str(value)) for key, value in report.items()])
        db.commit()
        return report
    except BaseException:
        db.rollback()
        raise
    finally:
        db.close()


def resolve_start(db, query, limit=20):
    if HEX_ID.match(query):
        return [query.lower()]
    rows = db.execute("SELECT DISTINCT file_id FROM resource WHERE name = ? LIMIT ?",
                      (query, limit + 1)).fetchall()
    if not rows:
        rows = db.execute("SELECT DISTINCT file_id FROM resource WHERE name LIKE ? LIMIT ?",
                          (query, limit + 1)).fetchall()
    if len(rows) > limit:
        raise ValueError(f"'{query}' matches more than {limit} resources; be more specific")
    return [r[0] for r in rows]


def describe(db, file_id):
    row = db.execute("""SELECT group_concat(DISTINCT r.name), group_concat(DISTINCT r.type_id),
                               group_concat(DISTINCT a.name), COUNT(*)
                        FROM resource r JOIN entry e ON e.id = r.entry_id
                        JOIN archive a ON a.id = e.archive_id WHERE r.file_id = ?""",
                     (file_id,)).fetchone()
    if row[3] == 0:
        entry = db.execute("""SELECT group_concat(DISTINCT a.name) FROM entry e
                              JOIN archive a ON a.id = e.archive_id WHERE e.file_id = ?""",
                           (file_id,)).fetchone()
        return {"file_id": file_id, "name": None, "type_id": None,
                "archives": (entry[0] or "").split(",") if entry[0] else [],
                "known": bool(entry[0]), "texture": None}
    tex = db.execute("""SELECT t.map_type, t.category, t.width, t.height FROM texture t
                        JOIN resource r ON r.id = t.resource_id WHERE r.file_id = ? LIMIT 1""",
                     (file_id,)).fetchone()
    return {"file_id": file_id, "name": row[0], "type_id": row[1],
            "archives": row[2].split(",") if row[2] else [], "known": True,
            "texture": None if tex is None else dict(
                zip(("map_type", "category", "width", "height"), tex))}


def neighbours(db, file_id, up, weak=False):
    kind = "" if weak else " AND l.kind = 'candidate'"
    if up:
        sql = """SELECT DISTINCT r.file_id FROM link l JOIN resource r ON r.id = l.src_resource_id
                 WHERE l.dst_file_id = ? AND r.file_id != ?""" + kind
    else:
        sql = """SELECT DISTINCT l.dst_file_id FROM link l JOIN resource r
                 ON r.id = l.src_resource_id WHERE r.file_id = ? AND l.dst_file_id != ?""" + kind
    return [row[0] for row in db.execute(sql + " ORDER BY 1", (file_id, file_id))]


def walk(db, query, up=False, depth=3, limit=200, weak=False):
    """Breadth-first walk of candidate links. Each file ID is expanded once (cycle safe)."""
    starts = resolve_start(db, query)
    if not starts:
        raise ValueError(f"no resource matches '{query}'")
    seen, rows, frontier = set(starts), [], [(s, None, 0) for s in starts]
    while frontier and len(rows) < limit:
        file_id, parent, level = frontier.pop(0)
        node = describe(db, file_id)
        node.update(depth=level, via=parent)
        rows.append(node)
        if level >= depth:
            continue
        for other in neighbours(db, file_id, up, weak):
            if other not in seen:
                seen.add(other)
                frontier.append((other, file_id, level + 1))
    return {"truncated": bool(frontier), "rows": rows}


def format_walk(result):
    lines = []
    for n in result["rows"]:
        tex = n["texture"]
        extra = f" tex map_type={tex['map_type']} {tex['width']}x{tex['height']}" if tex else ""
        label = n["name"] or ("(entry only)" if n["known"] else "(unknown ID)")
        archives = ",".join(n["archives"][:3]) + ("..." if len(n["archives"]) > 3 else "")
        lines.append(f"{'  ' * n['depth']}{n['file_id']} {label} [{n['type_id'] or '-'}]"
                     f"{extra} <{archives}>")
    if result["truncated"]:
        lines.append("... truncated at --limit")
    return "\n".join(lines)


def stats(db):
    q = lambda sql: db.execute(sql).fetchone()[0]
    return {"links": q("SELECT COUNT(*) FROM link"),
            "source_resources": q("SELECT COUNT(DISTINCT src_resource_id) FROM link"),
            "distinct_targets": q("SELECT COUNT(DISTINCT dst_file_id) FROM link"),
            "weak_links": q("SELECT COUNT(*) FROM link WHERE kind = 'weak'"),
            "targets_with_several_parents": q("SELECT COUNT(*) FROM fan_in WHERE parents > 1"),
            "targets_not_in_index": q("""SELECT COUNT(DISTINCT dst_file_id) FROM link l WHERE
                NOT EXISTS (SELECT 1 FROM resource r WHERE r.file_id = l.dst_file_id) AND
                NOT EXISTS (SELECT 1 FROM entry e WHERE e.file_id = l.dst_file_id)""")}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="cmd", required=True)
    m = sub.add_parser("merge")
    m.add_argument("index")
    m.add_argument("tsv", nargs="+")
    m.add_argument("--replace", action="store_true")
    w = sub.add_parser("walk")
    w.add_argument("index")
    w.add_argument("query")
    w.add_argument("--up", action="store_true", help="walk to parents instead of children")
    w.add_argument("--depth", type=int, default=3)
    w.add_argument("--limit", type=int, default=200)
    w.add_argument("--include-weak", action="store_true",
                   help="also follow links to IDs below 2**25 (indistinguishable from integers)")
    w.add_argument("--json", action="store_true")
    s = sub.add_parser("stats")
    s.add_argument("index")
    args = parser.parse_args(argv)
    try:
        if args.cmd == "merge":
            print(json.dumps(merge(args.index, args.tsv, args.replace)))
        else:
            db = sqlite3.connect(f"file:{args.index}?mode=ro", uri=True)
            try:
                if args.cmd == "walk":
                    result = walk(db, args.query, args.up, args.depth, args.limit,
                                  args.include_weak)
                    print(json.dumps(result, indent=1) if args.json else format_walk(result))
                else:
                    print(json.dumps(stats(db), indent=1))
            finally:
                db.close()
    except (ValueError, FileExistsError, sqlite3.Error, OSError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
