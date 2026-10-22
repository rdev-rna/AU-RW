"""The agent-facing query tool on a synthetic index; no game data required."""

import contextlib
import io
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

import asset_graph as ag
import asset_query as aq
import build_asset_db as bdb

GAME = "D:/Games/ACU"
ENT, MESH, TEX = "0000004000000001", "0000004000000002", "0000004000000003"


def res(file_id, type_id, name):
    return {"file_id": file_id, "type_id": type_id, "name": name, "record_offset": 0,
            "record_length": 8, "payload": {"start": 0, "end": 8}, "payload_sha256": "p"}


def pkg(archive, index, file_id, resources):
    return {"archive": f"{GAME}/{archive}", "entry": {
        "file_id": file_id, "index": index, "offset": 10 * index, "record_offset": 1,
        "stored_length": 8}, "stored_sha256": "s", "resources": resources}


class QueryTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        root = Path(self.dir.name)
        scan = root / "entries.jsonl"
        rows = [pkg("A.forge", 0, "00000040000000a0", [
                    res(ENT, "0984415e", "Arno_Entity"), res(MESH, "415d9568", "Arno_Body"),
                    res(TEX, "a2b7e917", "Arno_Normal")]),
                pkg("B.forge", 0, "00000040000000b0", [res(TEX, "a2b7e917", "Arno_Normal")])]
        scan.write_text("\n".join(json.dumps(r) for r in rows), encoding="utf-8")
        self.path = root / "index.sqlite"
        bdb.build(scan, self.path, GAME)
        tsv = root / "links.tsv"
        tsv.write_text("archive\tentry\tordinal\tsrc_file_id\toffset\tdst_file_id\thits\n"
                       f"A.forge\t0\t0\t{ENT}\t8\t{MESH}\t1\nA.forge\t0\t1\t{MESH}\t8\t{TEX}\t2\n",
                       encoding="utf-8")
        ag.merge(self.path, [tsv])
        self.db = aq.connect(self.path)

    def tearDown(self):
        self.db.close()
        self.dir.cleanup()

    def annotate(self):
        db = sqlite3.connect(self.path)
        db.execute("""CREATE TABLE type_label (type_id TEXT PRIMARY KEY, label TEXT,
            category TEXT, confidence TEXT, basis TEXT, rows INTEGER, distinct_ids INTEGER,
            named_rows INTEGER, decoded_rows INTEGER, sample_names TEXT)""")
        db.execute("INSERT INTO type_label VALUES ('a2b7e917','TextureMap','texture',"
                   "'observed-decode','b',2,1,2,2,'[]')")
        db.commit()
        db.close()

    def test_where_finds_every_copy_of_an_id(self):
        out = aq.where(self.db, TEX)
        self.assertEqual([(r["archive"], r["entry"]) for r in out["as_resource"]],
                         [("A.forge", 0), ("B.forge", 0)])

    def test_where_accepts_short_ids_prefix_and_case(self):
        self.assertEqual(aq.normalise_id("0X4000000003"), "0000004000000003")
        self.assertEqual(aq.where(self.db, "00000040000000A0")["as_entry"][0]["archive"],
                         "A.forge")

    def test_where_rejects_garbage_and_reports_missing(self):
        with self.assertRaises(aq.QueryError):
            aq.where(self.db, "not-hex")
        with self.assertRaises(aq.QueryError):
            aq.where(self.db, "1" * 17)
        out = aq.where(self.db, "0000000000000999")
        self.assertEqual((out["as_resource"], out["as_entry"]), ([], []))

    def test_find_by_substring_exact_type_and_archive(self):
        self.assertEqual(len(aq.find(self.db, "arno")["rows"]), 4)
        self.assertEqual(len(aq.find(self.db, "Arno_Normal", exact=True)["rows"]), 2)
        self.assertEqual(len(aq.find(self.db, "Arno", type_filter="415d9568")["rows"]), 1)
        self.assertEqual(len(aq.find(self.db, "Arno", archive="B.forge")["rows"]), 1)

    def test_find_filters_by_label_once_the_index_is_annotated(self):
        self.annotate()
        db = aq.connect(self.path)
        self.assertEqual(len(aq.find(db, "Arno", type_filter="TextureMap")["rows"]), 2)
        self.assertEqual(aq.find(db, "Arno_Normal")["rows"][0]["confidence"], "observed-decode")
        db.close()

    def test_limit_marks_truncation(self):
        out = aq.find(self.db, "Arno", limit=2)
        self.assertEqual((len(out["rows"]), out["truncated"]), (2, True))

    def test_types_works_before_and_after_annotation(self):
        before = aq.types(self.db)["rows"]
        self.assertEqual(before[0]["rows"], 2)
        self.assertIsNone(before[0]["label"])
        self.annotate()
        db = aq.connect(self.path)
        self.assertEqual(aq.types(db)["rows"][0]["label"], "TextureMap")
        db.close()

    def test_archive_summary_and_unknown_archive_suggestion(self):
        out = aq.archive_summary(self.db, "A.forge")
        self.assertEqual((out["entries"], out["resources"]), (1, 3))
        with self.assertRaises(aq.QueryError) as caught:
            aq.archive_summary(self.db, "A.forg")
        self.assertIn("A.forge", str(caught.exception))

    def test_info_lists_tables_and_meta(self):
        out = aq.info(self.db)
        self.assertEqual((out["resource"], out["link"]), (4, 2))
        self.assertEqual(out["meta"]["link_staged"], "2")

    def test_sql_accepts_select_and_rejects_everything_else(self):
        self.assertEqual(aq.run_sql(self.db, "SELECT COUNT(*) FROM resource")["rows"], [[4]])
        self.assertEqual(aq.run_sql(self.db, "WITH t AS (SELECT 1 a) SELECT a FROM t")["rows"],
                         [[1]])
        for bad in ("DROP TABLE resource", "SELECT 1; DROP TABLE resource",
                    "DELETE FROM meta", "PRAGMA writable_schema=1"):
            with self.assertRaises(aq.QueryError):
                aq.run_sql(self.db, bad)

    def test_database_is_read_only_even_for_a_clever_statement(self):
        with self.assertRaises(sqlite3.Error):
            aq.run_sql(self.db, "WITH t AS (SELECT 1) DELETE FROM meta")
        self.assertGreater(aq.info(self.db)["meta"].__len__(), 0)

    def test_missing_index_names_the_fix(self):
        with self.assertRaises(aq.QueryError) as caught:
            aq.connect(Path(self.dir.name) / "absent.sqlite")
        self.assertIn("index_status.py", str(caught.exception))

    def test_options_work_before_or_after_the_subcommand(self):
        def run(*argv):
            out = io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
                code = aq.main(list(argv))
            return code, out.getvalue()
        db = str(self.path)
        after = run("find", "Arno", "--db", db, "--limit", "1", "--json")
        before = run("--db", db, "--limit", "1", "--json", "find", "Arno")
        self.assertEqual(after[0], 0)
        self.assertEqual(after, before)
        self.assertEqual(len(json.loads(after[1])["rows"]), 1)
        self.assertEqual(run("types", "--db", db)[0], 0)

    def test_cli_walk_where_and_json_output(self):
        def run(*argv):
            out, err = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = aq.main(["--db", str(self.path), *argv])
            return code, out.getvalue(), err.getvalue()
        code, out, _ = run("walk", "Arno_Entity", "--depth", "2")
        self.assertEqual(code, 0)
        self.assertIn("Arno_Body", out)
        self.assertIn("Arno_Normal", out)
        code, out, _ = run("--json", "where", MESH)
        self.assertEqual(json.loads(out)["as_resource"][0]["name"], "Arno_Body")
        code, out, err = run("where", "zz")
        self.assertEqual((code, out), (1, ""))
        self.assertIn("not a hexadecimal", err)


if __name__ == "__main__":
    unittest.main()
