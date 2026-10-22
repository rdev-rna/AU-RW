"""The audit must pass a sound index and fail each kind of hole; synthetic data only."""

import json
from pathlib import Path
import sqlite3
import tempfile
import unittest
from unittest import mock

import asset_graph as ag
import audit_asset_db as au
import build_asset_db as bdb

GAME = "D:/Games/ACU"
A_ENT, A_RES, B_ENT, B_RES = ("00000040000000a1", "00000040000000a2",
                              "00000040000000b1", "00000040000000b2")


def entry(archive, index, file_id, resources):
    return {"archive": f"{GAME}/{archive}", "entry": {
        "file_id": file_id, "index": index, "offset": 100 * index, "record_offset": 1,
        "stored_length": 8}, "stored_sha256": "s", "typed_status": "x", "resources": [
        {"file_id": r, "type_id": "t1", "name": "n", "record_offset": 0, "record_length": 8,
         "payload": {"start": 0, "end": 8}, "payload_sha256": "p"} for r in resources]}


class AuditTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        root = Path(self.dir.name)
        scan = root / "entries.jsonl"
        rows = [entry("A.forge", 0, A_ENT, [A_RES, "00000040000000a3"]),
                entry("B.forge", 0, B_ENT, [B_RES])]
        scan.write_text("\n".join(json.dumps(r) for r in rows), encoding="utf-8")
        (root / "summary.json").write_text(json.dumps({"manifest": [
            {"path": f"{GAME}/A.forge", "length": 1000},
            {"path": f"{GAME}/B.forge", "length": 2000}]}), encoding="utf-8")
        self.db_path = root / "index.sqlite"
        bdb.build(scan, self.db_path, GAME)
        tsv = root / "links.tsv"
        tsv.write_text("archive\tentry\tordinal\tsrc_file_id\toffset\tdst_file_id\thits\n"
                       f"A.forge\t0\t0\t{A_RES}\t8\t{B_RES}\t1\n", encoding="utf-8")
        ag.merge(self.db_path, [tsv])
        self.summary = {"target_ids": 5, "entries_scanned": 2, "entry_decode_failures": 0,
                        "metadata_records_per_archive": {
                            "A.forge": {"data_entries": 1, "resource_records": 2},
                            "B.forge": {"data_entries": 1, "resource_records": 1}}}
        self.patch = mock.patch.dict(au.EXPECTED, {
            "archives": (2, "t"), "entries_total": (2, "t"), "control_entries": (0, "t")})
        self.patch.start()

    def tearDown(self):
        self.patch.stop()
        self.dir.cleanup()

    def audit(self, summary=None, tables=None):
        db = sqlite3.connect(self.db_path)
        try:
            return au.run(db, summary, tables)
        finally:
            db.close()

    def status(self, audit, name):
        return next(s for s, n, _ in audit.rows if n == name)

    def mutate(self, sql, *args):
        db = sqlite3.connect(self.db_path)
        db.execute(sql, args)
        db.commit()
        db.close()

    def good_tables(self, name):
        return {"A.forge": [(0, A_ENT, 0, 8)], "B.forge": [(0, B_ENT, 0, 8)]}[name]

    def test_sound_index_has_no_failures(self):
        # Archive sizes on disk are unknown here, so size_bytes stays NULL and is not compared.
        audit = self.audit(self.summary, self.good_tables)
        self.assertEqual([r for r in audit.rows if r[0] == "FAIL"], [])
        self.assertFalse(audit.failed)

    def test_archive_that_changed_after_its_scan_fails(self):
        self.mutate("UPDATE archive SET size_bytes = 999 WHERE name = 'A.forge'")
        self.assertEqual(self.status(self.audit(), "archives unchanged since their scan"), "FAIL")

    def test_missing_scan_time_size_fails(self):
        self.mutate("UPDATE archive SET scan_size_bytes = NULL WHERE name = 'B.forge'")
        self.assertEqual(self.status(self.audit(), "every archive has a scan-time size"), "FAIL")

    def test_entry_table_that_differs_from_a_fresh_parse_fails(self):
        def stale(name):
            rows = self.good_tables(name)
            return [(i, f, o + 1, length) for i, f, o, length in rows] if name == "A.forge" \
                else rows
        audit = self.audit(self.summary, stale)
        self.assertEqual(self.status(audit, "entry tables match a fresh parse of every archive"),
                         "FAIL")

    def test_missing_entry_in_the_index_fails(self):
        def extra(name):
            return self.good_tables(name) + ([(1, "00000040000000ff", 9, 9)]
                                             if name == "B.forge" else [])
        audit = self.audit(self.summary, extra)
        self.assertEqual(self.status(audit, "entry tables match a fresh parse of every archive"),
                         "FAIL")

    def test_resource_count_that_disagrees_with_the_metadata_parse_fails(self):
        self.summary["metadata_records_per_archive"]["A.forge"]["resource_records"] = 3
        audit = self.audit(self.summary)
        self.assertEqual(self.status(
            audit, "per-archive entries and resources match the metadata parse"), "FAIL")

    def test_missing_merge_report_fails(self):
        self.mutate("DELETE FROM meta WHERE key LIKE 'link_%'")
        audit = self.audit()
        self.assertEqual(self.status(audit, "merge report is recorded"), "FAIL")

    def test_dropped_link_rows_fail(self):
        self.mutate("UPDATE meta SET value = '7' WHERE key = 'link_staged'")
        self.assertEqual(self.status(self.audit(), "merge dropped no rows"), "FAIL")

    def test_orphan_rows_in_the_merge_report_fail(self):
        self.mutate("UPDATE meta SET value = '5' WHERE key = 'link_orphans'")
        self.assertEqual(self.status(self.audit(), "merge dropped no rows"), "FAIL")

    def test_noncontiguous_entry_indexes_fail(self):
        self.mutate("UPDATE entry SET idx = 5 WHERE file_id = ?", A_ENT)
        self.assertEqual(self.status(self.audit(), "entry indexes contiguous per archive"),
                         "FAIL")

    def make_labels(self, named):
        """type_label says t1 is 'Thing' (confident, 3 rows); `named` resources carry the name."""
        self.mutate("""CREATE TABLE type_label (type_id TEXT PRIMARY KEY, label TEXT, category TEXT,
            confidence TEXT, basis TEXT, rows INTEGER, distinct_ids INTEGER, named_rows INTEGER,
            decoded_rows INTEGER, sample_names TEXT)""")
        self.mutate("INSERT INTO type_label VALUES ('t1','Thing','c','crc-name','b',3,3,3,0,'[]')")
        self.mutate("UPDATE resource SET type_name = 'Thing' WHERE id <= ?", named)

    def test_type_names_that_match_the_labels_pass_and_a_partial_fill_fails(self):
        self.make_labels(named=3)
        self.assertEqual(self.status(self.audit(), "resource.type_name agrees with type_label"),
                         "PASS")
        self.mutate("UPDATE resource SET type_name = NULL WHERE id = 1")
        self.assertEqual(self.status(self.audit(), "resource.type_name agrees with type_label"),
                         "FAIL")

    def test_an_archive_with_only_control_entries_is_named_not_failed(self):
        self.mutate("INSERT INTO archive (name, size_bytes, baseline, scan_size_bytes) "
                    "VALUES ('Empty.forge', 1, 1, 1)")
        details = [d for s, n, d in self.audit().rows
                   if n == "archives with only control entries (no data)"]
        self.assertIn("Empty.forge", details[0])

    def test_id_set_that_differs_from_the_metadata_pass_fails(self):
        self.summary["target_ids"] = 99
        self.assertEqual(self.status(self.audit(self.summary),
                                     "distinct IDs match acu-links pass 1"), "FAIL")


if __name__ == "__main__":
    unittest.main()
