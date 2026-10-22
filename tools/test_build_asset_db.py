"""Asset index builder on synthetic scan rows; no game data required."""

import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

import build_asset_db as bdb

GAME = "D:/Games/ACU"


def control(archive):
    return {"archive": archive, "control": {"kind": "global_metadata"},
            "entry": {"file_id": "0000000000000010", "index": 0, "offset": 100,
                      "record_offset": 10, "stored_length": 8},
            "stored_sha256": "aa", "typed_status": "parsed_control"}


def data(archive, index, file_id, resources):
    return {"archive": archive, "decoded_sha256": "dd", "stored_sha256": "ss",
            "entry": {"file_id": file_id, "index": index, "offset": 200 + index,
                      "record_offset": 20 + index, "stored_length": 64},
            "resource_envelopes_parsed": len(resources), "typed_status": "parsed",
            "resources": resources}


def resource(file_id, type_id, name=None):
    return {"file_id": file_id, "type_id": type_id, "name": name,
            "record_offset": 0, "record_length": 40, "payload": {"start": 8, "end": 40},
            "payload_sha256": "pp"}


def write(path, rows):
    path.write_text("\n".join(json.dumps(r) for r in rows) + "\n", encoding="utf-8")


class AssetDbTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.root = Path(self.dir.name)
        self.scan = self.root / "entries.jsonl"
        self.out = self.root / "out" / "index.sqlite"
        a = GAME.replace("/", "\\") + "\\DataPC.forge"
        b = GAME + "/dlc_11/Extra.forge"
        write(self.scan, [
            control(a),
            data(a, 1, "000000000000aa01", [resource("000000000000aa01", "a2b7e917", "Diffuse"),
                                           resource("000000000000aa02", "b3c8f028")]),
            data(b, 1, "000000000000aa01", [resource("000000000000aa01", "a2b7e917")]),
        ])

    def tearDown(self):
        self.dir.cleanup()

    def root_file(self, name):
        return self.root / name

    def query(self, sql, *args):
        db = sqlite3.connect(self.out)
        try:
            return db.execute(sql, args).fetchall()
        finally:
            db.close()

    def test_counts_and_archive_names_normalised(self):
        counts = bdb.build(self.scan, self.out, GAME, baseline_names={"DataPC.forge"})
        self.assertEqual(counts, {"entries": 2, "controls": 1, "resources": 3, "textures": 0,
                                  "archives": 2})
        self.assertEqual(self.query("SELECT name, baseline FROM archive ORDER BY id"),
                         [("DataPC.forge", 1), ("dlc_11/Extra.forge", 0)])

    def test_where_does_this_id_live(self):
        bdb.build(self.scan, self.out, GAME)
        rows = self.query("""SELECT a.name FROM entry e JOIN archive a ON a.id = e.archive_id
                             WHERE e.file_id = ? ORDER BY a.name""", "000000000000aa01")
        self.assertEqual(rows, [("DataPC.forge",), ("dlc_11/Extra.forge",)])

    def test_resource_provenance_and_type_names(self):
        bdb.build(self.scan, self.out, GAME, type_names={"a2b7e917": "TextureMap"})
        rows = self.query("""SELECT r.name, r.type_name, r.payload_start, r.payload_end,
                                    e.offset FROM resource r JOIN entry e ON e.id = r.entry_id
                             WHERE r.type_id = ? ORDER BY e.id""", "a2b7e917")
        self.assertEqual(rows, [("Diffuse", "TextureMap", 8, 40, 201),
                                (None, "TextureMap", 8, 40, 201)])

    def test_texture_descriptor_is_attached_to_its_resource(self):
        tex = resource("000000000000cc01", "a2b7e917", "Normal")
        tex["texture"] = {"map_type": 11, "category": 5, "user_category": 0, "kind": 1,
                          "dxgi_format": 87, "width": 4, "height": 8, "depth": 1, "layers": 1,
                          "mip_count": 3, "storage": "Resident", "gamma": 1, "dynamic": False}
        write(self.scan, [data(GAME + "/DataPC.forge", 1, "000000000000cc01",
                               [resource("000000000000cc02", "b3c8f028"), tex])])
        counts = bdb.build(self.scan, self.out, GAME)
        self.assertEqual(counts["textures"], 1)
        self.assertEqual(self.query("""SELECT r.name, t.map_type, t.width, t.height, t.dynamic
                                       FROM texture t JOIN resource r ON r.id = t.resource_id"""),
                         [("Normal", 11, 4, 8, 0)])

    def test_override_replaces_an_archive_whole_and_keeps_the_rest(self):
        a, b = GAME + "/A.forge", GAME + "/B.forge"
        write(self.scan, [data(a, 1, "00000000000000a1", [resource("00000000000000a1", "t1")]),
                          data(a, 2, "00000000000000a2", [resource("00000000000000a2", "t1")]),
                          data(b, 1, "00000000000000b1", [resource("00000000000000b1", "t1")])])
        newer = self.root_file("rescan.jsonl")
        write(newer, [data(a, 1, "00000000000000c1", [resource("00000000000000c1", "t9")])])
        counts = bdb.build(self.scan, self.out, GAME, overrides=[newer])
        self.assertEqual((counts["entries"], counts["resources"], counts["archives"]), (2, 2, 2))
        rows = self.query("""SELECT a.name, e.idx, e.file_id FROM entry e
                             JOIN archive a ON a.id = e.archive_id ORDER BY a.name, e.idx""")
        self.assertEqual(rows, [("A.forge", 1, "00000000000000c1"),
                                ("B.forge", 1, "00000000000000b1")])
        self.assertEqual(self.query("SELECT COUNT(*) FROM entry WHERE file_id = ?",
                                    "00000000000000a2"), [(0,)])  # stale A rows are gone
        self.assertEqual(self.query("SELECT value FROM meta WHERE key = 'overrides'"),
                         [(str(newer),)])

    def test_scan_time_sizes_come_from_the_manifest_and_overrides_win(self):
        a, b = GAME + "/A.forge", GAME + "/B.forge"
        write(self.scan, [data(a, 1, "00000000000000a1", []), data(b, 1, "00000000000000b1", [])])
        (self.root / "summary.json").write_text(json.dumps(
            {"manifest": [{"path": a.replace("/", "\\"), "length": 100},
                          {"path": b, "length": 200}]}), encoding="utf-8")
        (self.root / "rescan").mkdir()
        newer = self.root_file("rescan/entries.jsonl")
        write(newer, [data(a, 1, "00000000000000c1", [])])
        newer.with_name("summary.json").write_text(json.dumps(
            {"manifest": [{"path": a, "length": 150}]}), encoding="utf-8")
        bdb.build(self.scan, self.out, GAME, overrides=[newer])
        self.assertEqual(self.query("SELECT name, scan_size_bytes FROM archive ORDER BY name"),
                         [("A.forge", 150), ("B.forge", 200)])

    def test_override_errors_name_the_override_file(self):
        newer = self.root_file("rescan.jsonl")
        newer.write_text("{broken", encoding="utf-8")
        with self.assertRaises(bdb.ScanError) as caught:
            bdb.build(self.scan, self.out, GAME, overrides=[newer])
        self.assertIn("rescan.jsonl line 1: invalid JSON", str(caught.exception))
        self.assertFalse(self.out.exists())

    def test_empty_name_is_stored_as_null(self):
        write(self.scan, [data(GAME + "/DataPC.forge", 1, "000000000000bb01",
                               [resource("000000000000bb01", "a2b7e917", "")])])
        bdb.build(self.scan, self.out, GAME)
        self.assertEqual(self.query("SELECT name FROM resource"), [(None,)])

    def test_unknown_baseline_is_null_not_true(self):
        bdb.build(self.scan, self.out, None)
        self.assertEqual({r[0] for r in self.query("SELECT baseline FROM archive")}, {None})

    def test_controls_are_marked(self):
        bdb.build(self.scan, self.out, GAME)
        self.assertEqual(self.query("SELECT kind FROM entry WHERE file_id = ?",
                                    "0000000000000010"), [("control",)])

    def test_existing_output_is_not_overwritten(self):
        bdb.build(self.scan, self.out, GAME)
        before = self.out.read_bytes()
        with self.assertRaises(FileExistsError):
            bdb.build(self.scan, self.out, GAME)
        self.assertEqual(before, self.out.read_bytes())
        bdb.build(self.scan, self.out, GAME, replace=True)

    def test_bad_input_fails_and_leaves_no_output(self):
        for bad, message in ((["{not json"], "line 1: invalid JSON"),
                             ([json.dumps({"archive": "x"})], "line 1: missing or invalid 'entry'"),
                             ([], "empty")):
            self.scan.write_text("\n".join(bad), encoding="utf-8")
            with self.assertRaises(bdb.ScanError) as caught:
                bdb.build(self.scan, self.out, GAME)
            self.assertIn(message, str(caught.exception))
            self.assertFalse(self.out.exists())
            self.assertFalse(self.out.with_name(self.out.name + ".partial").exists())

    def test_wrong_field_type_names_the_line(self):
        rows = [control(GAME + "/DataPC.forge")]
        bad = data(GAME + "/DataPC.forge", 1, "000000000000aa01", [])
        bad["entry"]["offset"] = "100"
        write(self.scan, rows + [bad])
        with self.assertRaises(bdb.ScanError) as caught:
            bdb.build(self.scan, self.out, GAME)
        self.assertIn("line 2: missing or invalid 'offset'", str(caught.exception))

    def test_archive_name_without_install_uses_basename(self):
        self.assertEqual(bdb.archive_name("C:\\x\\y\\A.forge", None), "A.forge")


if __name__ == "__main__":
    unittest.main()
