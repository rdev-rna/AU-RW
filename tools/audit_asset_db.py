#!/usr/bin/env python3
"""Completeness audit of the local asset index (#132). Read-only.

    python tools/audit_asset_db.py local/asset_index.sqlite [--links-summary SUMMARY.json]
                                   [--game-dir INSTALL]

`--game-dir` re-parses every archive's entry table and compares it with the index entry by entry
(index, ID, offset, stored length). It is the check that catches an archive changed after the scan.

Every FAIL is a hole. INFO lines are known, documented gaps, not failures. Expected values
name their source; none was produced by the builder being audited, except where marked.
"""

from __future__ import annotations

import argparse
import json
import sqlite3
import sys

# (value, source). Resource and texture totals are deliberately not fixed here: the 2026-10-18
# STATUS.md totals came from a scan of a Paris archive that has since changed, and are not a
# reference. Counts are checked per archive against the metadata parse (--links-summary).
EXPECTED = {
    "archives": (63, "acu_assets::forge::discover over the baseline install (ACU-RE-0037)"),
    "entries_total": (180245, "reachable entries, fresh parse of the current files (ACU-RE-0037)"),
    "control_entries": (126, "one global-metadata + one prefetch entry per archive"),
}


class Audit:
    def __init__(self, db):
        self.db = db
        self.rows = []
        self.failed = False

    def q(self, sql, *args):
        return self.db.execute(sql, args).fetchall()

    def one(self, sql, *args):
        return self.db.execute(sql, args).fetchone()[0]

    def check(self, name, ok, detail):
        self.rows.append(("PASS" if ok else "FAIL", name, detail))
        self.failed |= not ok

    def info(self, name, detail):
        self.rows.append(("INFO", name, detail))

    def equals(self, name, actual, key):
        want, source = EXPECTED[key]
        self.check(name, actual == want, f"{actual:,} (expected {want:,}; {source})")


def run(db, links_summary=None, tables=None):
    """`tables(name)` -> [(index, file_id, offset, stored_length)] from a fresh parse, or None."""
    a = Audit(db)
    ok = a.one("PRAGMA integrity_check") == "ok"
    a.check("sqlite integrity_check", ok, "ok" if ok else "corrupt")
    fk = a.q("PRAGMA foreign_key_check")
    a.check("foreign keys", not fk, f"{len(fk)} violations")

    a.equals("archives", a.one("SELECT COUNT(*) FROM archive"), "archives")
    a.equals("entries total", a.one("SELECT COUNT(*) FROM entry"), "entries_total")
    a.equals("control entries", a.one("SELECT COUNT(*) FROM entry WHERE kind='control'"),
             "control_entries")
    a.info("resources / distinct types / textures",
           f"{a.one('SELECT COUNT(*) FROM resource'):,} / "
           f"{a.one('SELECT COUNT(DISTINCT type_id) FROM resource')} / "
           f"{a.one('SELECT COUNT(*) FROM texture'):,}")

    # Structure: contiguous entry indexes and resource ordinals.
    gaps = a.one("""SELECT COUNT(*) FROM (SELECT archive_id, COUNT(*) c, MAX(idx) m
                    FROM entry GROUP BY archive_id) WHERE m + 1 != c""")
    a.check("entry indexes contiguous per archive", gaps == 0, f"{gaps} archives with gaps")
    gaps = a.one("""SELECT COUNT(*) FROM (SELECT entry_id, COUNT(*) c, MAX(ordinal) m
                    FROM resource GROUP BY entry_id) WHERE m + 1 != c""")
    a.check("resource ordinals contiguous per entry", gaps == 0, f"{gaps} entries with gaps")
    dup = a.one("""SELECT COUNT(*) FROM (SELECT archive_id, idx FROM entry
                   GROUP BY archive_id, idx HAVING COUNT(*) > 1)""")
    a.check("no duplicate (archive, index)", dup == 0, f"{dup} duplicates")
    nulls = {c: a.one(f"SELECT COUNT(*) FROM resource WHERE {c} IS NULL")
             for c in ("file_id", "type_id", "payload_sha256", "record_length")}
    a.check("resources have id, type, hash, length", not any(nulls.values()), str(nulls))
    bad = a.one("SELECT COUNT(*) FROM entry WHERE kind='data' AND stored_sha256 IS NULL")
    a.check("data entries have a stored hash", bad == 0, f"{bad} without")

    # The archive must be the one that was scanned.
    moved = a.q("""SELECT name, scan_size_bytes, size_bytes FROM archive
                   WHERE scan_size_bytes IS NOT NULL AND size_bytes IS NOT NULL
                     AND scan_size_bytes != size_bytes""")
    a.check("archives unchanged since their scan", not moved,
            f"{len(moved)} changed: {moved[:3]}")
    unknown = a.one("SELECT COUNT(*) FROM archive WHERE scan_size_bytes IS NULL")
    a.check("every archive has a scan-time size", unknown == 0, f"{unknown} without")

    if tables:
        bad = []
        for (name,) in a.q("SELECT name FROM archive ORDER BY name"):
            fresh = {i: (fid, off, length) for i, fid, off, length in tables(name)}
            mine = {i: (fid, off, length) for i, fid, off, length in a.q(
                """SELECT e.idx, e.file_id, e.offset, e.stored_length FROM entry e
                   JOIN archive ar ON ar.id = e.archive_id WHERE ar.name = ?""", name)}
            wrong = (sum(1 for i in fresh if mine.get(i) != fresh[i])
                     + sum(1 for i in mine if i not in fresh))
            if wrong:
                bad.append((name, len(mine), len(fresh), wrong))
        a.check("entry tables match a fresh parse of every archive", not bad,
                f"{len(bad)} archives differ: {bad[:3]}" if bad else "all archives agree")
    else:
        a.info("entry-table cross-check", "skipped (no --game-dir)")

    # Cross-checks against the independent Rust metadata pass, when supplied.
    if links_summary:
        ids = a.one("""SELECT COUNT(*) FROM (SELECT file_id FROM entry WHERE kind='data'
                       UNION SELECT file_id FROM resource)""")
        want = links_summary["target_ids"]
        a.check("distinct IDs match acu-links pass 1", ids == want,
                f"{ids:,} in the index vs {want:,} from the Rust metadata parse")
        per = links_summary.get("metadata_records_per_archive")
        if per:
            mine = {name: (entries, records) for name, entries, records in a.q(
                """SELECT ar.name, COUNT(DISTINCT e.id), COUNT(r.id) FROM archive ar
                   JOIN entry e ON e.archive_id = ar.id AND e.kind = 'data'
                   LEFT JOIN resource r ON r.entry_id = e.id GROUP BY ar.name""")}
            theirs = {n: (v["data_entries"], v["resource_records"]) for n, v in per.items()}
            wrong = [(n, mine.get(n), theirs.get(n)) for n in sorted(set(mine) | set(theirs))
                     if mine.get(n) != theirs.get(n)]
            a.check("per-archive entries and resources match the metadata parse", not wrong,
                    f"{len(theirs)} archives agree" if not wrong
                    else f"{len(wrong)} differ (archive, index, metadata): {wrong[:3]}")
        else:
            a.info("per-archive resource counts", "summary has no metadata_records_per_archive")
        a.check("all entries scanned by acu-links",
                links_summary["entries_scanned"] == a.one(
                    "SELECT COUNT(*) FROM entry WHERE kind='data'"),
                f"{links_summary['entries_scanned']:,} scanned, "
                f"{links_summary['entry_decode_failures']} decode failures")
    else:
        a.info("acu-links cross-check", "skipped (no --links-summary)")

    # Links, when merged.
    if a.one("SELECT COUNT(*) FROM sqlite_master WHERE name='link'"):
        n = a.one("SELECT COUNT(*) FROM link")
        a.check("links present", n > 0, f"{n:,} rows")
        a.check("every link has a source resource", a.one(
            """SELECT COUNT(*) FROM link l LEFT JOIN resource r ON r.id=l.src_resource_id
               WHERE r.id IS NULL""") == 0, "no dangling sources")
        missing = a.one("""SELECT COUNT(DISTINCT l.dst_file_id) FROM link l
            WHERE NOT EXISTS (SELECT 1 FROM resource r WHERE r.file_id=l.dst_file_id)
              AND NOT EXISTS (SELECT 1 FROM entry e WHERE e.file_id=l.dst_file_id)""")
        a.check("every link target exists in the index", missing == 0,
                f"{missing} targets absent (targets come from the same ID set)")
        merge = dict(a.q("SELECT key, value FROM meta WHERE key LIKE 'link_%'"))
        a.check("merge report is recorded", "link_staged" in merge,
                "present" if merge else "missing: rerun asset_graph.py merge --replace")
        clean = (merge.get("link_orphans") == "0"
                 and merge.get("link_source_id_mismatches") == "0"
                 and merge.get("link_staged") == merge.get("link_linked"))
        a.check("merge dropped no rows", clean, str(merge))
        kinds = dict(a.q("SELECT kind, COUNT(*) FROM link GROUP BY kind"))
        a.info("link kinds", str(kinds))
    else:
        a.info("links", "not merged")

    # Known, documented gaps.
    zero = a.q("""SELECT a.name, e.idx FROM entry e JOIN archive a ON a.id=e.archive_id
                  WHERE e.kind='data' AND e.id NOT IN (SELECT entry_id FROM resource)""")
    a.info("data entries with no resources", f"{len(zero)} (e.g. {zero[:4]})")
    a.info("resources without a name", f"{a.one('SELECT COUNT(*) FROM resource WHERE name IS NULL'):,}")
    named = a.one("SELECT COUNT(*) FROM resource WHERE type_name IS NOT NULL")
    a.info("resources with a type name", f"{named:,} (type names need --type-names)")
    unk = a.one("SELECT COUNT(*) FROM archive WHERE baseline IS NULL")
    a.info("archives with unknown baseline flag", str(unk))
    return a


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("index")
    parser.add_argument("--links-summary", help="acu-links summary.json for cross-checks")
    parser.add_argument("--game-dir", help="install dir: re-parse every archive's entry table")
    args = parser.parse_args(argv)
    summary = json.load(open(args.links_summary, encoding="utf-8")) if args.links_summary else None
    try:
        db = sqlite3.connect(f"file:{args.index}?mode=ro", uri=True)
        tables = None
        if args.game_dir:
            import forge_probe
            from pathlib import Path

            def tables(name):
                path = Path(args.game_dir) / name
                with path.open("rb") as stream:
                    _, entries = forge_probe.tables(stream, path.stat().st_size)
                return [(e["index"], e["file_id"], e["offset"], e["stored_length"])
                        for e in entries]
        audit = run(db, summary, tables)
    except (sqlite3.Error, OSError, KeyError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    for status, name, detail in audit.rows:
        print(f"{status:4} {name}: {detail}")
    print("RESULT:", "HOLES FOUND" if audit.failed else "no holes found")
    return 1 if audit.failed else 0


if __name__ == "__main__":
    sys.exit(main())
