import copy
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

import animation_catalogue as catalogue


def record(archive="base.forge"):
    return {
        "archive": archive,
        "entry": {"index": 4, "file_id": "ffffffffffffffff", "offset": 100, "stored_length": 50},
        "stored_sha256": "a" * 64, "decoded_sha256": "b" * 64,
        "resources": [{"file_id": "fedcba9876543210", "type_id": "0fa3067f",
                       "name": "Arno_Nav_Walk_Loop", "payload": {"start": 10, "end": 40},
                       "payload_sha256": "c" * 64}],
    }


class CatalogueTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.source = Path(self.temporary.name) / "entries.jsonl"
        self.database = Path(self.temporary.name) / "animations.sqlite"

    def write(self, records):
        self.source.write_text("".join(json.dumps(x) + "\n" for x in records), encoding="utf-8")

    def test_occurrences_preserve_patch_duplicates_and_uint64_ids(self):
        self.write([record(), record("patch.forge")])
        result = catalogue.build(self.source, self.database)
        self.assertEqual(result["occurrences"], 2)
        rows = catalogue.query(self.database, file_id="fedcba9876543210")
        self.assertEqual([x["archive"] for x in rows], ["base.forge", "patch.forge"])
        self.assertTrue(all(x["entry_file_id"] == "ffffffffffffffff" for x in rows))
        self.assertEqual(catalogue.stats(self.database)["types"][0]["distinct_ids"], 1)

    def test_name_hint_does_not_establish_actor_ownership(self):
        self.write([record()])
        catalogue.build(self.source, self.database)
        row = catalogue.query(self.database, role="walking")[0]
        self.assertEqual(row["role_hints"], [{"role": "walking", "evidence_kind": "name_hint"}])
        self.assertEqual(row["actor_ownership"], "unknown")
        self.assertEqual(catalogue.query(self.database, actor="Arno"), [])
        catalogue.annotate(self.database, row["id"], "actor", "Arno", "manual_hypothesis", "name only")
        self.assertEqual(catalogue.query(self.database, actor="Arno"), [])
        catalogue.annotate(self.database, row["id"], "actor", "Shared player", "decoded_reference", "fixture: reference at byte 12")
        self.assertEqual(len(catalogue.query(self.database, actor="player")), 1)

    def test_failed_build_is_atomic_and_does_not_leave_database(self):
        invalid = record("invalid.forge")
        invalid["resources"][0]["file_id"] = 18446744073709551615
        self.write([record(), invalid])
        with self.assertRaisesRegex(ValueError, "hexadecimal"):
            catalogue.build(self.source, self.database)
        self.assertFalse(self.database.exists())
        self.assertEqual(list(self.database.parent.glob("*.tmp")), [])

    def test_rebuild_cannot_discard_annotations(self):
        self.write([record()])
        catalogue.build(self.source, self.database)
        before = self.database.read_bytes()
        with self.assertRaisesRegex(ValueError, "already exists"):
            catalogue.build(self.source, self.database)
        self.assertEqual(self.database.read_bytes(), before)

    def test_unknown_types_are_not_clips_and_bad_ranges_rejected(self):
        unrelated = record()
        unrelated["resources"][0]["type_id"] = "00000000"
        self.write([unrelated])
        self.assertEqual(catalogue.build(self.source, self.database)["occurrences"], 0)
        invalid = copy.deepcopy(record())
        invalid["resources"][0]["payload"]["end"] = 0
        self.write([invalid])
        with self.assertRaisesRegex(ValueError, "reversed"):
            catalogue.build(self.source, self.database.with_suffix(".other"))

    def test_filters_are_literal_and_bindings_require_existing_occurrence(self):
        self.write([record()])
        catalogue.build(self.source, self.database)
        self.assertEqual(catalogue.query(self.database, name="' OR 1=1 --"), [])
        self.assertEqual(catalogue.query(self.database, name="%"), [])
        with self.assertRaises(sqlite3.IntegrityError):
            catalogue.annotate(self.database, 99, "actor", "Arno", "decoded_reference", "fixture")
        with self.assertRaises(ValueError):
            catalogue.query(self.database, limit=10000)


if __name__ == "__main__":
    unittest.main()
