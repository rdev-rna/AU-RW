"""Tests for save_pin.py against a temporary fake save folder (no real saves touched)."""
from __future__ import annotations

import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import save_pin  # noqa: E402


class SavePinTest(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.live = self.tmp / "savegames/user/720"
        self.live.mkdir(parents=True)
        (self.live / "1.save").write_bytes(b"start-slot")
        (self.live / "2.save").write_bytes(b"profile")
        pins = self.tmp / "pins"
        self.patches = [mock.patch.object(save_pin, "save_dir", lambda: self.live),
                        mock.patch.object(save_pin, "game_running", lambda: False),
                        mock.patch.object(save_pin, "PINS", pins),
                        mock.patch.object(save_pin, "SNAPSHOTS", pins / "_snapshots")]
        for p in self.patches:
            p.start()

    def tearDown(self):
        for p in self.patches:
            p.stop()

    def test_use_then_putback_round_trip(self):
        save_pin.pin("street")
        (self.live / "1.save").write_bytes(b"after-play")  # the game autosaves elsewhere
        used = save_pin.use("street")
        self.assertEqual((self.live / "1.save").read_bytes(), b"start-slot")
        (self.live / "1.save").write_bytes(b"after-capture")
        save_pin.putback(used["snapshot"])
        self.assertEqual((self.live / "1.save").read_bytes(), b"after-play")
        self.assertEqual((self.live / "2.save").read_bytes(), b"profile")

    def test_refuses_while_running(self):
        with mock.patch.object(save_pin, "game_running", lambda: True):
            with self.assertRaises(SystemExit):
                save_pin.pin("street")

    def test_bad_names_and_duplicates(self):
        for bad in ("", "_x", "../up", "a b"):
            with self.assertRaises(SystemExit):
                save_pin.pin(bad)
        save_pin.pin("street")
        with self.assertRaises(SystemExit):
            save_pin.pin("street")

    def test_extra_slot_blocks_use_but_keeps_snapshot(self):
        save_pin.pin("street")
        (self.live / "3.save").write_bytes(b"new slot")
        with self.assertRaises(SystemExit):
            save_pin.use("street")
        self.assertEqual((self.live / "1.save").read_bytes(), b"start-slot")
        self.assertEqual(len(list((self.tmp / "pins/_snapshots").iterdir())), 1)

    def test_tampered_pin_is_not_copied(self):
        save_pin.pin("street")
        (self.tmp / "pins/street/1.save").write_bytes(b"tampered")
        (self.live / "1.save").write_bytes(b"after-play")
        with self.assertRaises(SystemExit):
            save_pin.use("street")
        self.assertEqual((self.live / "1.save").read_bytes(), b"after-play")

    def test_sync_files_are_ignored_and_waited_for(self):
        save_pin.pin("street")
        (self.live / "1.save.z").write_bytes(b"compressed")
        (self.live / "1.save.upload").write_bytes(b"")
        with mock.patch.object(save_pin.time, "sleep", lambda _: None):
            with self.assertRaises(SystemExit):  # still uploading: copy nothing
                save_pin.wait_for_sync(self.live, timeout=0)
        self.assertNotIn("1.save.z", save_pin.files_of(self.live))
        (self.live / "1.save.z").unlink()
        (self.live / "1.save.upload").unlink()
        save_pin.use("street")

    def test_status_reports_matching_pin(self):
        save_pin.pin("street")
        self.assertTrue(save_pin.status()["matches_pin"]["street"])
        (self.live / "1.save").write_bytes(b"moved")
        self.assertFalse(save_pin.status()["matches_pin"]["street"])
        manifest = json.loads((self.tmp / "pins/street/manifest.json").read_text())
        self.assertEqual(set(manifest["files"]), {"1.save", "2.save"})


if __name__ == "__main__":
    unittest.main()
