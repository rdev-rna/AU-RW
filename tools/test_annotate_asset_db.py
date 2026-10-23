"""Type labels and loose-file inventory on synthetic data; no game data required."""

import hashlib
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

import annotate_asset_db as an
import build_asset_db as bdb

GAME = "D:/Games/ACU"
MATERIAL = an.crc_hex("Material")    # 85c817c3, verified against the repo's anchors
SKELETON = an.crc_hex("Skeleton")    # 24aecb7c
MYSTERY = "deadbeef"


def res(file_id, type_id, name, **extra):
    row = {"file_id": file_id, "type_id": type_id, "name": name, "record_offset": 0,
           "record_length": 8, "payload": {"start": 0, "end": 8}, "payload_sha256": "p"}
    row.update(extra)
    return row


def write_scan(path, archive, resources):
    row = {"archive": f"{GAME}/{archive}", "entry": {"file_id": "0000004000000001", "index": 0,
           "offset": 0, "record_offset": 0, "stored_length": 8}, "stored_sha256": "s",
           "typed_status": "x", "resources": resources}
    path.write_text(json.dumps(row), encoding="utf-8")


class KnownValues(unittest.TestCase):
    def test_crc_rule_reproduces_recorded_type_ids(self):
        for name, type_id in (("Material", "85c817c3"), ("TextureSet", "d70e6670"),
                              ("LODSelector", "51dc6b80"), ("Entity", "0984415e"),
                              ("Skeleton", "24aecb7c"), ("Mesh", "415d9568"),
                              ("BuildTable", "22ecbe63")):
            self.assertEqual(an.crc_hex(name), type_id, name)


class LabelTests(unittest.TestCase):
    def label(self, rows, names=(), decodes=None):
        return {r[0]: r for r in an.label_types(rows, set(names), decodes or {})}

    def test_crc_name_match_is_labelled_with_its_evidence_level(self):
        out = self.label([(MATERIAL, 10, 4, 10, ["m"])], names=["Material", "Other"])
        _, label, category, confidence, basis, *_ = out[MATERIAL]
        self.assertEqual((label, category, confidence), ("Material", "material", "crc-name"))
        self.assertIn("CRC-32", basis)

    def test_unmatched_type_is_unknown_not_guessed(self):
        out = self.label([(MYSTERY, 5, 5, 0, [])], names=["Material"])
        self.assertEqual((out[MYSTERY][1], out[MYSTERY][2], out[MYSTERY][3]),
                         (None, "uncategorised", "unknown"))

    def test_parser_decode_outranks_a_name_and_notes_disagreement(self):
        out = self.label([(SKELETON, 20, 5, 20, [])], names=["Skeleton"],
                         decodes={SKELETON: {"Skeleton": 18}})
        self.assertEqual(out[SKELETON][3], "observed-decode")
        self.assertIn("agrees", out[SKELETON][4])
        out = self.label([(MYSTERY, 3, 3, 0, [])], names=[], decodes={MYSTERY: {"Mesh": 2}})
        self.assertEqual((out[MYSTERY][1], out[MYSTERY][3]), ("Mesh", "observed-decode"))

    def test_two_names_with_one_hash_are_ambiguous_and_both_reported(self):
        # "plumless" and "buckeroo" are a well-known CRC-32 collision (both 0x4ddb0c25).
        self.assertEqual(an.crc_hex("plumless"), an.crc_hex("buckeroo"))
        type_id = an.crc_hex("plumless")
        out = self.label([(type_id, 1, 1, 0, [])], names=["plumless", "buckeroo", "Material"])
        self.assertEqual(out[type_id][3], "crc-ambiguous")
        self.assertIn("buckeroo", out[type_id][1])
        self.assertIn("plumless", out[type_id][1])

    def test_category_is_only_for_known_names(self):
        self.assertEqual(an.CATEGORY_OF["AtomGraph"], "animation")
        self.assertNotIn("TotallyUnknownClass", an.CATEGORY_OF)

    def test_identifier_file_tolerates_quotes_and_blank_lines(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "ids.txt"
            path.write_text('"Alpha"\n\nBeta\n  "Gamma"  \n', encoding="utf-8")
            self.assertEqual(an.read_identifiers(path), {"Alpha", "Beta", "Gamma"})


class AnnotateTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        root = Path(self.dir.name)
        self.game = root / "game"
        (self.game / "videos").mkdir(parents=True)
        (self.game / "ACUFixes" / "plugins").mkdir(parents=True)
        (self.game / "Backups").mkdir()
        (self.game / "DataPC.forge").write_bytes(b"F")
        (self.game / "localization.lang").write_bytes(b"lang-bytes")
        (self.game / "videos" / "intro.bk2").write_bytes(b"video")
        (self.game / "ACU.exe").write_bytes(b"exe")
        (self.game / "ACU.exe.i64").write_bytes(b"ida")
        (self.game / "ACUFixes" / "plugins" / "x.dll").write_bytes(b"mod")
        (self.game / "Backups" / "DataPC.forge").write_bytes(b"F")
        (self.game / "ACU_dxgi.log").write_bytes(b"log")
        self.scan = root / "entries.jsonl"
        write_scan(self.scan, "DataPC.forge", [
            res("0000004000000011", MATERIAL, "M"),
            res("0000004000000012", SKELETON, "S", bone_count=3),
            res("0000004000000013", MYSTERY, None)])
        self.db = root / "index.sqlite"
        bdb.build(self.scan, self.db, self.game)

    def tearDown(self):
        self.dir.cleanup()

    def rows(self, sql):
        db = sqlite3.connect(self.db)
        try:
            return db.execute(sql).fetchall()
        finally:
            db.close()

    def run_annotate(self, **kw):
        return an.annotate(self.db, self.game, self.scan, **kw)

    def test_labels_land_in_the_table_with_evidence(self):
        stats = self.run_annotate()
        self.assertEqual(stats["by_confidence"],
                         {"crc-name": 1, "observed-decode": 1, "unknown": 1})
        rows = {r[0]: r for r in self.rows("SELECT type_id, label, confidence FROM type_label")}
        self.assertEqual(rows[SKELETON][1:], ("Skeleton", "observed-decode"))
        self.assertEqual(rows[MATERIAL][1:], ("Material", "crc-name"))

    def test_resource_type_name_is_filled_only_for_confident_labels(self):
        self.run_annotate()
        names = dict(self.rows("SELECT file_id, type_name FROM resource"))
        self.assertEqual(names["0000004000000011"], "Material")   # crc-name
        self.assertEqual(names["0000004000000012"], "Skeleton")   # observed-decode
        self.assertIsNone(names["0000004000000013"])              # unknown stays NULL

    def test_rerunning_clears_stale_type_names(self):
        self.run_annotate()
        db = sqlite3.connect(self.db)
        db.execute("UPDATE resource SET type_name = 'Stale'")
        db.commit()
        db.close()
        self.run_annotate(replace=True)
        stale = self.rows("SELECT COUNT(*) FROM resource WHERE type_name = 'Stale'")
        self.assertEqual(stale, [(0,)])

    def test_loose_files_exclude_mod_layer_backups_logs_forges_and_tool_databases(self):
        self.run_annotate()
        paths = sorted(p for (p,) in self.rows("SELECT path FROM loose_file"))
        self.assertEqual(paths, ["ACU.exe", "localization.lang", "videos/intro.bk2"])

    def test_loose_file_size_hash_and_category_are_real(self):
        self.run_annotate()
        row = dict((r[0], r[1:]) for r in self.rows(
            "SELECT path, size_bytes, sha256, category FROM loose_file"))
        self.assertEqual(row["localization.lang"],
                         (10, hashlib.sha256(b"lang-bytes").hexdigest(), "localization"))
        self.assertEqual(row["videos/intro.bk2"][2], "video")

    def test_no_hash_leaves_hash_null(self):
        self.run_annotate(hash_files=False)
        self.assertEqual({h for (h,) in self.rows("SELECT sha256 FROM loose_file")}, {None})

    def test_baseline_flag_is_set_from_the_carrier_rules(self):
        self.run_annotate()
        self.assertEqual(self.rows("SELECT name, baseline FROM archive"), [("DataPC.forge", 1)])

    def test_refuses_to_overwrite_then_replaces(self):
        self.run_annotate()
        with self.assertRaises(FileExistsError):
            self.run_annotate()
        self.run_annotate(replace=True)
        self.assertEqual(self.rows("SELECT COUNT(*) FROM type_label"), [(3,)])

    def test_missing_install_fails_cleanly_and_leaves_no_tables(self):
        with self.assertRaises(ValueError):
            an.annotate(self.db, self.game / "nope", self.scan)
        self.assertEqual(self.rows("SELECT COUNT(*) FROM sqlite_master WHERE name = 'type_label'"),
                         [(0,)])


if __name__ == "__main__":
    unittest.main()
