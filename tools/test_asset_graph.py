"""Link graph merge and walk on a synthetic index; no game data required."""

import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

import asset_graph as ag
import build_asset_db as bdb

GAME = "D:/Games/ACU"
ENT, MESH, TEX, ATLAS = (f"00000040000000{n:02x}" for n in (1, 2, 3, 4))
GHOST = "0000004000000099"  # confident-size ID with no entry in the index
SMALL = "0000000000000066"  # below 2**25: a match is not evidence


def res(file_id, type_id, name, texture=None):
    row = {"file_id": file_id, "type_id": type_id, "name": name, "record_offset": 0,
           "record_length": 8, "payload": {"start": 0, "end": 8}, "payload_sha256": "p"}
    if texture:
        row["texture"] = texture
    return row


def pkg(archive, index, file_id, resources):
    return {"archive": f"{GAME}/{archive}", "entry": {
        "file_id": file_id, "index": index, "offset": 1, "record_offset": 1,
        "stored_length": 8}, "resources": resources, "typed_status": "x"}


HEADER = "archive\tentry\tordinal\tsrc_file_id\toffset\tdst_file_id\thits\n"


class GraphTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        root = Path(self.dir.name)
        scan, self.db = root / "entries.jsonl", root / "index.sqlite"
        rows = [pkg("A.forge", 1, "0000000000000a01",
                    [res(ENT, "e1", "Arno_Entity"), res(MESH, "m1", "Arno_Body"),
                     res(TEX, "t1", "Arno_Normal", {"map_type": 11, "category": 5, "width": 8,
                                                    "height": 8})]),
                pkg("B.forge", 1, "0000000000000b01",
                    [res(ATLAS, "t1", "Atlas_Props"), res(TEX, "t1", "Arno_Normal")])]
        scan.write_text("\n".join(json.dumps(r) for r in rows), encoding="utf-8")
        bdb.build(scan, self.db, GAME)
        self.tsv = root / "links.tsv"
        self.write_links([
            ("A.forge", 1, 0, ENT, 16, MESH), ("A.forge", 1, 0, ENT, 24, TEX),
            ("A.forge", 1, 1, MESH, 8, TEX), ("A.forge", 1, 1, MESH, 40, ATLAS),
            ("A.forge", 1, 2, TEX, 4, ENT),  # cycle back to the entity
            ("A.forge", 1, 1, MESH, 56, GHOST),  # target not in the index
            ("A.forge", 1, 0, ENT, 99, SMALL),   # small target: stored as weak
        ])

    def tearDown(self):
        self.dir.cleanup()

    def write_links(self, rows):
        self.tsv.write_text(HEADER + "".join("\t".join(map(str, (*r, 2))) + "\n" for r in rows),
                            encoding="utf-8")

    def conn(self):
        return sqlite3.connect(self.db)

    def test_merge_counts_and_integrity_checks(self):
        self.write_links([
            ("A.forge", 1, 0, ENT, 16, MESH),
            ("A.forge", 9, 0, ENT, 16, MESH),   # no such entry: orphan
            ("A.forge", 1, 1, ENT, 8, TEX),     # ordinal 1 is MESH, not ENT: mismatch
        ])
        counts = ag.merge(self.db, [self.tsv])
        self.assertEqual(counts, {"staged": 3, "linked": 1, "orphans": 1,
                                  "source_id_mismatches": 1})

    def test_merge_report_is_recorded_in_meta(self):
        counts = ag.merge(self.db, [self.tsv])
        db = self.conn()
        stored = dict(db.execute("SELECT key, value FROM meta WHERE key LIKE 'link_%'"))
        db.close()
        self.assertEqual(stored, {f"link_{k}": str(v) for k, v in counts.items()})
        ag.merge(self.db, [self.tsv], replace=True)  # replacing must not duplicate keys
        db = self.conn()
        self.assertEqual(db.execute(
            "SELECT COUNT(*) FROM meta WHERE key LIKE 'link_%'").fetchone()[0], 4)
        db.close()

    def test_merge_refuses_to_overwrite_then_replaces(self):
        ag.merge(self.db, [self.tsv])
        with self.assertRaises(FileExistsError):
            ag.merge(self.db, [self.tsv])
        self.assertEqual(ag.merge(self.db, [self.tsv], replace=True)["linked"], 7)

    def test_malformed_tsv_is_rejected_without_changes(self):
        cases = {
            "wrong header": "wrong\theader\n",
            "source id not hex": HEADER + "A.forge\t1\t0\tnothex\t1\t" + MESH + "\t1\n",
            "entry not a number": HEADER + "A.forge\tx\t0\t" + ENT + "\t1\t" + MESH + "\t1\n",
            "hits not a number": HEADER + "A.forge\t1\t0\t" + ENT + "\t1\t" + MESH + "\tmany\n",
            "too few columns": HEADER + "A.forge\t1\n",
        }
        for label, body in cases.items():
            self.tsv.write_text(body, encoding="utf-8")
            with self.subTest(label), self.assertRaises(ValueError) as caught:
                ag.merge(self.db, [self.tsv])
            # Each case must fail for its own reason, not because the row is the wrong width.
            expected = {"wrong header": "unexpected header", "too few columns": "malformed row",
                        "source id not hex": "malformed row"}.get(label, "non-numeric field")
            self.assertIn(expected, str(caught.exception))
        db = self.conn()
        self.assertIsNone(db.execute(
            "SELECT 1 FROM sqlite_master WHERE name = 'link'").fetchone())
        db.close()

    def test_views_resolve_names_and_fan_in(self):
        ag.merge(self.db, [self.tsv])
        db = self.conn()
        row = db.execute("""SELECT src_name, dst_name, dst_archive FROM link_view
                            WHERE dst_file_id = ? AND src_name = 'Arno_Entity'""",
                         (MESH,)).fetchone()
        self.assertEqual(row, ("Arno_Entity", "Arno_Body", "A.forge"))
        fan = dict(db.execute("SELECT dst_file_id, parents FROM fan_in").fetchall())
        self.assertEqual(fan[TEX], 2)    # entity and mesh both point at the texture
        self.assertEqual(fan[ATLAS], 1)
        db.close()

    def test_walk_down_follows_links_and_survives_the_cycle(self):
        ag.merge(self.db, [self.tsv])
        db = self.conn()
        result = ag.walk(db, "Arno_Entity", depth=5)
        ids = [r["file_id"] for r in result["rows"]]
        self.assertEqual(ids[0], ENT)
        self.assertEqual(sorted(ids), sorted({ENT, MESH, TEX, ATLAS, GHOST}))
        self.assertEqual(len(ids), len(set(ids)))
        ghost = next(r for r in result["rows"] if r["file_id"] == GHOST)
        self.assertFalse(ghost["known"])
        tex = next(r for r in result["rows"] if r["file_id"] == TEX)
        self.assertEqual(tex["texture"]["map_type"], 11)
        self.assertEqual(tex["archives"], ["A.forge", "B.forge"])
        db.close()

    def test_weak_links_are_stored_but_not_walked_unless_asked(self):
        ag.merge(self.db, [self.tsv])
        db = self.conn()
        kinds = dict(db.execute("SELECT dst_file_id, kind FROM link").fetchall())
        self.assertEqual((kinds[SMALL], kinds[MESH], kinds[GHOST]),
                         ("weak", "candidate", "candidate"))
        default = {r["file_id"] for r in ag.walk(db, ENT, depth=5)["rows"]}
        self.assertNotIn(SMALL, default)
        with_weak = {r["file_id"] for r in ag.walk(db, ENT, depth=5, weak=True)["rows"]}
        self.assertIn(SMALL, with_weak)
        db.close()

    def test_confidence_boundary_is_two_to_the_25th(self):
        below, edge = "0000000001ffffff", "0000000002000000"
        self.write_links([("A.forge", 1, 0, ENT, 1, below), ("A.forge", 1, 0, ENT, 2, edge)])
        ag.merge(self.db, [self.tsv])
        db = self.conn()
        self.assertEqual(dict(db.execute("SELECT dst_file_id, kind FROM link").fetchall()),
                         {below: "weak", edge: "candidate"})
        db.close()

    def test_walk_depth_limit_and_up_direction(self):
        ag.merge(self.db, [self.tsv])
        db = self.conn()
        shallow = ag.walk(db, ENT, depth=1)
        self.assertEqual({r["file_id"] for r in shallow["rows"]}, {ENT, MESH, TEX})
        up = ag.walk(db, TEX, up=True, depth=1)
        self.assertEqual({r["file_id"] for r in up["rows"]}, {TEX, ENT, MESH})
        self.assertEqual(next(r for r in up["rows"] if r["file_id"] == MESH)["via"], TEX)
        db.close()

    def test_walk_limit_truncates_and_unknown_name_fails(self):
        ag.merge(self.db, [self.tsv])
        db = self.conn()
        self.assertTrue(ag.walk(db, ENT, depth=5, limit=2)["truncated"])
        with self.assertRaises(ValueError):
            ag.walk(db, "No_Such_Name")
        db.close()

    def test_stats(self):
        ag.merge(self.db, [self.tsv])
        db = self.conn()
        s = ag.stats(db)
        self.assertEqual((s["links"], s["source_resources"], s["targets_not_in_index"],
                          s["weak_links"]), (7, 3, 2, 1))
        db.close()

    def test_format_walk_is_indented_by_depth(self):
        ag.merge(self.db, [self.tsv])
        db = self.conn()
        text = ag.format_walk(ag.walk(db, ENT, depth=1))
        lines = text.splitlines()
        self.assertTrue(lines[0].startswith(ENT))
        self.assertTrue(all(line.startswith("  ") for line in lines[1:]))
        db.close()


if __name__ == "__main__":
    unittest.main()
