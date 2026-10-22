#!/usr/bin/env python3
"""Ask the local asset index questions (read-only). Start here instead of scanning archives.

    python tools/asset_query.py info                      what the index is and how fresh
    python tools/asset_query.py where 0000000f18b7e1cf    which archive/entry holds this ID
    python tools/asset_query.py find Wooden_Sword         resources by name (substring)
    python tools/asset_query.py types                     resource types, labels, confidence
    python tools/asset_query.py archive DataPC_ACU_Paris.forge
    python tools/asset_query.py walk Arno_Entity --depth 2   candidate links, down (or --up)
    python tools/asset_query.py sql "SELECT ..."          any read-only SELECT (tables below)

Tables: archive, entry (one per Forge entry/package), resource (envelopes inside a package),
texture, link (candidate references), type_label, loose_file, meta.  Add --json for JSON.
Links are byte matches (candidates), type labels carry a confidence level; read them as such.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sqlite3
import sys

import asset_graph

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_DB = ROOT / "local" / "asset_index.sqlite"


class QueryError(ValueError):
    pass


def connect(path):
    path = Path(path)
    if not path.is_file():
        raise QueryError(
            f"no index at {path}. Run `python tools/index_status.py` to fetch or unpack it.")
    db = sqlite3.connect(f"file:{path.as_posix()}?mode=ro", uri=True, timeout=30)
    db.execute("PRAGMA query_only = 1")
    return db


def has_table(db, name):
    return db.execute("SELECT 1 FROM sqlite_master WHERE name = ?", (name,)).fetchone() is not None


def label_columns(db):
    """SQL fragments for the type label, present only when the index was annotated."""
    if has_table(db, "type_label"):
        return "tl.label, tl.confidence", "LEFT JOIN type_label tl ON tl.type_id = r.type_id"
    return "NULL, NULL", ""


def normalise_id(text):
    text = text.strip().lower().removeprefix("0x")
    if not text or len(text) > 16 or any(c not in "0123456789abcdef" for c in text):
        raise QueryError(f"'{text}' is not a hexadecimal file ID")
    return text.zfill(16)


def where(db, file_id, limit=25):
    file_id = normalise_id(file_id)
    cols, join = label_columns(db)
    resources = db.execute(f"""
        SELECT a.name, e.idx, r.ordinal, r.type_id, {cols}, r.name, e.offset, e.stored_length
        FROM resource r JOIN entry e ON e.id = r.entry_id JOIN archive a ON a.id = e.archive_id
        {join} WHERE r.file_id = ? ORDER BY a.name, e.idx, r.ordinal LIMIT ?""",
                           (file_id, limit + 1)).fetchall()
    entries = db.execute("""
        SELECT a.name, e.idx, e.kind, e.offset, e.stored_length, e.stored_sha256
        FROM entry e JOIN archive a ON a.id = e.archive_id WHERE e.file_id = ?
        ORDER BY a.name, e.idx LIMIT ?""", (file_id, limit + 1)).fetchall()
    return {"file_id": file_id, "truncated": len(resources) > limit or len(entries) > limit,
            "as_resource": [dict(zip(("archive", "entry", "ordinal", "type_id", "label",
                                      "confidence", "name", "entry_offset", "entry_length"), r))
                            for r in resources[:limit]],
            "as_entry": [dict(zip(("archive", "entry", "kind", "offset", "stored_length",
                                   "stored_sha256"), r)) for r in entries[:limit]]}


def find(db, pattern, type_filter=None, archive=None, exact=False, limit=25):
    cols, join = label_columns(db)
    clauses, args = [], []
    clauses.append("r.name = ?" if exact else "r.name LIKE ?")
    args.append(pattern if exact else f"%{pattern}%")
    if type_filter:
        if has_table(db, "type_label"):
            clauses.append("(r.type_id = ? OR tl.label = ?)")
            args += [type_filter.lower(), type_filter]
        else:
            clauses.append("r.type_id = ?")
            args.append(type_filter.lower())
    if archive:
        clauses.append("a.name = ?")
        args.append(archive)
    rows = db.execute(f"""
        SELECT r.file_id, r.name, r.type_id, {cols}, a.name, e.idx, r.ordinal
        FROM resource r JOIN entry e ON e.id = r.entry_id JOIN archive a ON a.id = e.archive_id
        {join} WHERE {' AND '.join(clauses)} ORDER BY r.name, a.name, e.idx, r.ordinal LIMIT ?""",
                      (*args, limit + 1)).fetchall()
    keys = ("file_id", "name", "type_id", "label", "confidence", "archive", "entry", "ordinal")
    return {"truncated": len(rows) > limit, "rows": [dict(zip(keys, r)) for r in rows[:limit]]}


def types(db, min_rows=1, limit=60):
    if not has_table(db, "type_label"):
        rows = db.execute("""SELECT type_id, NULL, NULL, COUNT(*), COUNT(DISTINCT file_id)
                             FROM resource GROUP BY type_id HAVING COUNT(*) >= ?
                             ORDER BY 4 DESC LIMIT ?""", (min_rows, limit + 1)).fetchall()
    else:
        rows = db.execute("""SELECT type_id, label, confidence, rows, distinct_ids
                             FROM type_label WHERE rows >= ? ORDER BY rows DESC LIMIT ?""",
                          (min_rows, limit + 1)).fetchall()
    keys = ("type_id", "label", "confidence", "rows", "distinct_ids")
    return {"truncated": len(rows) > limit, "rows": [dict(zip(keys, r)) for r in rows[:limit]]}


def archive_summary(db, name):
    row = db.execute("""SELECT a.id, a.size_bytes, a.baseline,
                               (SELECT COUNT(*) FROM entry e WHERE e.archive_id = a.id),
                               (SELECT COUNT(*) FROM resource r JOIN entry e ON e.id = r.entry_id
                                WHERE e.archive_id = a.id)
                        FROM archive a WHERE a.name = ?""", (name,)).fetchone()
    if row is None:
        close = [n for (n,) in db.execute(
            "SELECT name FROM archive WHERE name LIKE ? ORDER BY name LIMIT 5", (f"%{name}%",))]
        raise QueryError(f"no archive '{name}'" + (f"; did you mean {close}?" if close else ""))
    cols, join = label_columns(db)
    top = db.execute(f"""SELECT r.type_id, {cols}, COUNT(*) FROM resource r
                         JOIN entry e ON e.id = r.entry_id {join}
                         WHERE e.archive_id = ? GROUP BY r.type_id ORDER BY 4 DESC LIMIT 10""",
                     (row[0],)).fetchall()
    return {"archive": name, "size_bytes": row[1], "baseline": row[2], "entries": row[3],
            "resources": row[4],
            "top_types": [dict(zip(("type_id", "label", "confidence", "resources"), t))
                          for t in top]}


def info(db):
    meta = dict(db.execute("SELECT key, value FROM meta"))
    out = {"meta": meta}
    for table in ("archive", "entry", "resource", "texture", "link", "type_label", "loose_file"):
        if has_table(db, table):
            out[table] = db.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
    return out


def run_sql(db, statement, limit=50):
    text = statement.strip().rstrip(";")
    if not text.lower().startswith(("select", "with")) or ";" in text:
        raise QueryError("only a single read-only SELECT (or WITH ... SELECT) is accepted")
    cursor = db.execute(text)
    names = [d[0] for d in cursor.description]
    rows = cursor.fetchmany(limit + 1)
    return {"columns": names, "truncated": len(rows) > limit, "rows": [list(r) for r in rows[:limit]]}


def table_text(headers, rows):
    cells = [[("-" if v is None else str(v)) for v in row] for row in rows]
    widths = [max(len(h), *(len(r[i]) for r in cells)) if cells else len(h)
              for i, h in enumerate(headers)]
    lines = ["  ".join(h.ljust(w) for h, w in zip(headers, widths))]
    lines += ["  ".join(c.ljust(w) for c, w in zip(row, widths)).rstrip() for row in cells]
    return "\n".join(lines)


def render(command, result, limit):
    if command == "where":
        out = [f"ID {result['file_id']}"]
        if result["as_resource"]:
            out.append("as a resource (archive, entry, ordinal, type, label, name):")
            out.append(table_text(
                ("archive", "entry", "ord", "type", "label", "confidence", "name"),
                [(r["archive"], r["entry"], r["ordinal"], r["type_id"], r["label"],
                  r["confidence"], r["name"]) for r in result["as_resource"]]))
        if result["as_entry"]:
            out.append("as an archive entry (the package container):")
            out.append(table_text(("archive", "entry", "kind", "offset", "stored_length"),
                                  [(r["archive"], r["entry"], r["kind"], r["offset"],
                                    r["stored_length"]) for r in result["as_entry"]]))
        if not result["as_resource"] and not result["as_entry"]:
            out.append("not found in the index")
        text = "\n".join(out)
    elif command in ("find", "types"):
        keys = {"find": ("file_id", "name", "type_id", "label", "confidence", "archive", "entry"),
                "types": ("type_id", "label", "confidence", "rows", "distinct_ids")}[command]
        text = table_text(keys, [[r[k] for k in keys] for r in result["rows"]]) \
            if result["rows"] else "no matches"
    elif command == "sql":
        text = table_text(result["columns"], result["rows"]) if result["rows"] else "no rows"
    else:
        text = json.dumps(result, indent=1, default=str)
    if result.get("truncated"):
        text += f"\n... more rows exist; raise --limit (now {limit})"
    return text


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0],
                                     formatter_class=argparse.RawDescriptionHelpFormatter,
                                     epilog="\n".join(__doc__.splitlines()[2:]))
    parser.add_argument("--db", default=str(DEFAULT_DB))
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--limit", type=int, default=25)
    # The same options are accepted before or after the subcommand (models write both).
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--db", default=argparse.SUPPRESS)
    common.add_argument("--json", action="store_true", default=argparse.SUPPRESS)
    common.add_argument("--limit", type=int, default=argparse.SUPPRESS)
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("info", parents=[common])
    sub.add_parser("where", parents=[common]).add_argument("file_id")
    f = sub.add_parser("find", parents=[common])
    f.add_argument("pattern")
    f.add_argument("--type", help="type ID or label, e.g. 85c817c3 or Material")
    f.add_argument("--archive")
    f.add_argument("--exact", action="store_true")
    t = sub.add_parser("types", parents=[common])
    t.add_argument("--min", type=int, default=1)
    sub.add_parser("archive", parents=[common]).add_argument("name")
    w = sub.add_parser("walk", parents=[common])
    w.add_argument("query")
    w.add_argument("--up", action="store_true")
    w.add_argument("--depth", type=int, default=2)
    w.add_argument("--include-weak", action="store_true")
    sub.add_parser("sql", parents=[common]).add_argument("statement")
    args = parser.parse_args(argv)
    try:
        db = connect(args.db)
        try:
            if args.cmd == "info":
                result = info(db)
            elif args.cmd == "where":
                result = where(db, args.file_id, args.limit)
            elif args.cmd == "find":
                result = find(db, args.pattern, args.type, args.archive, args.exact, args.limit)
            elif args.cmd == "types":
                result = types(db, args.min, args.limit)
            elif args.cmd == "archive":
                result = archive_summary(db, args.name)
            elif args.cmd == "walk":
                if not has_table(db, "link"):
                    raise QueryError("this index has no links; rebuild with asset_graph.py merge")
                result = asset_graph.walk(db, args.query, args.up, args.depth, args.limit,
                                          args.include_weak)
            else:
                result = run_sql(db, args.statement, args.limit)
        finally:
            db.close()
    except (QueryError, ValueError, sqlite3.Error) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    if args.json:
        print(json.dumps(result, indent=1, default=str))
    elif args.cmd == "walk":
        print(asset_graph.format_walk(result))
    else:
        print(render(args.cmd, result, args.limit))
    return 0


if __name__ == "__main__":
    sys.exit(main())
