import copy
import json
import sqlite3
import tempfile
from pathlib import Path
import unittest

import animation_catalogue as catalogue
import locomotion_audit as audit
from test_animation_catalogue import record


class AuditTests(unittest.TestCase):
    def test_import_tracks_identity_and_atomic_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, database, decoded = root / "entries.jsonl", root / "db.sqlite", root / "audit.jsonl"
            original = record()
            source.write_text(json.dumps(original) + "\n", encoding="utf-8")
            catalogue.build(source, database)
            row = {k: original[k] for k in ("archive", "entry", "stored_sha256", "decoded_sha256")}
            row.update(resource_index=0, resource=original["resources"][0], payload_sha256="c" * 64,
                       status="decoded", duration=1.0, decoder="independent fixture",
                       tracks=[dict(track_index=0, bone_hash="ffffffff", channel="translation", codec=9,
                                    key_count=2, data_offset=0, data_length=30, max_component_delta=2.0)])
            decoded.write_text(json.dumps(row) + "\n", encoding="utf-8")
            self.assertEqual(audit.import_audit(database, decoded)["track_rows"], 1)
            self.assertEqual(catalogue.query(database)[0]["tracks"][0]["bone_hash"], "ffffffff")
            bad_span = copy.deepcopy(row)
            bad_span["tracks"][0]["data_length"] = 31
            decoded.write_text(json.dumps(bad_span) + "\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "track span"):
                audit.import_audit(database, decoded)
            self.assertEqual(catalogue.query(database, actor="Arno"), [])
            bad = copy.deepcopy(row)
            bad["payload_sha256"] = "d" * 64
            changed = copy.deepcopy(row)
            changed["tracks"][0]["bone_hash"] = "00000001"
            decoded.write_text(json.dumps(changed) + "\n" + json.dumps(bad) + "\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "payload_sha256 mismatch"):
                audit.import_audit(database, decoded)
            self.assertEqual(catalogue.query(database)[0]["tracks"][0]["bone_hash"], "ffffffff")
            rejected = copy.deepcopy(row)
            rejected.update(status="unsupported", error="codec unknown")
            rejected.pop("tracks")
            rejected.pop("duration")
            decoded.write_text(json.dumps(rejected) + "\n", encoding="utf-8")
            self.assertEqual(audit.import_audit(database, decoded)["track_rows"], 0)
            self.assertEqual(audit.export_candidates(database, root / "candidates.csv")["decode_status"], {"unsupported": 1})
            with sqlite3.connect(database) as con:
                self.assertEqual(con.execute("PRAGMA integrity_check").fetchone()[0], "ok")
            con.close()


if __name__ == "__main__":
    unittest.main()
