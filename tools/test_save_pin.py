"""Tests for save_pin.py against a temporary fake save folder (no real saves touched)."""
from __future__ import annotations

import json
import os
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
        self.pids: list[int] = []  # fake running ACU.exe PIDs; never the real tasklist
        self.owners: set[int] = set()  # fake live capture processes (holders before set_holder_pid)
        self.patches = [mock.patch.object(save_pin, "save_dir", lambda: self.live),
                        mock.patch.object(save_pin, "acu_pids", lambda: list(self.pids)),
                        mock.patch.object(save_pin, "process_alive", lambda pid: pid in self.owners),
                        mock.patch.object(save_pin.os, "getpid", lambda: 4242),
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


class SavePinLeaseTest(unittest.TestCase):
    """Shared lease across several ACU instances (fake PIDs; no game, no real saves)."""

    def setUp(self):
        SavePinTest.setUp(self)
        save_pin.pin("street")
        (self.live / "1.save").write_bytes(b"after-play")  # what must come back after the captures
        self.owners.add(4242)  # the capture process that acquires (os.getpid patched)

    def tearDown(self):
        SavePinTest.tearDown(self)

    def lease(self):
        return save_pin.read_lease()

    def test_first_acquire_swaps_second_joins(self):
        first = save_pin.acquire("street", "runs/a")
        self.assertEqual(first["action"], "swapped")
        self.assertEqual((self.live / "1.save").read_bytes(), b"start-slot")
        save_pin.set_holder_pid("runs/a", 100)
        self.pids.append(100)  # game A runs and autosaves
        (self.live / "1.save").write_bytes(b"autosave-a")
        second = save_pin.acquire("street", "runs/b")
        self.assertEqual(second["action"], "joined")
        self.assertFalse(second["live_matches_pin"])  # joined game loads A's autosave
        self.assertEqual((self.live / "1.save").read_bytes(), b"autosave-a")  # nothing copied
        self.assertEqual(second["snapshot"], first["snapshot"])
        self.assertEqual([h["holder"] for h in self.lease()["holders"]], ["runs/a", "runs/b"])

    def test_different_pin_refused(self):
        save_pin.pin("roof")
        save_pin.acquire("street", "runs/a")
        save_pin.set_holder_pid("runs/a", 100)
        self.pids.append(100)
        with self.assertRaises(SystemExit):
            save_pin.use("roof")  # the plain commands still refuse under a game
        with self.assertRaises(SystemExit):
            save_pin.acquire("roof", "runs/b")
        self.assertEqual(self.lease()["pin"], "street")
        self.assertEqual([h["holder"] for h in self.lease()["holders"]], ["runs/a"])

    def test_running_game_without_lease_refused(self):
        self.pids.append(55)  # another session's unpinned game
        with self.assertRaises(SystemExit) as raised:
            save_pin.acquire("street", "runs/a")
        self.assertIn("without --save-pin", str(raised.exception))
        self.assertIsNone(self.lease())
        self.assertEqual((self.live / "1.save").read_bytes(), b"after-play")

    def test_last_release_puts_back(self):
        save_pin.acquire("street", "runs/a")
        save_pin.acquire("street", "runs/b")
        for holder, pid in (("runs/a", 100), ("runs/b", 101)):
            save_pin.set_holder_pid(holder, pid)
        (self.live / "1.save").write_bytes(b"after-capture")
        self.pids[:] = [101]  # game A closed
        first = save_pin.release("runs/a")
        self.assertFalse(first["put_back"])
        self.assertEqual(first["remaining"], ["runs/b"])
        self.pids.clear()  # game B closed
        last = save_pin.release("runs/b")
        self.assertTrue(last["put_back"])
        self.assertIsNone(self.lease())
        self.assertEqual((self.live / "1.save").read_bytes(), b"after-play")
        self.assertEqual((self.live / "2.save").read_bytes(), b"profile")

    def test_release_while_game_runs_defers_to_settle(self):
        save_pin.acquire("street", "runs/a")
        save_pin.set_holder_pid("runs/a", 100)
        self.pids[:] = [100, 77]  # A still closing, plus another session's unpinned game
        released = save_pin.release("runs/a")
        self.assertTrue(released["deferred"])
        self.assertFalse(released["put_back"])
        self.assertEqual(self.lease()["holders"], [])
        self.assertEqual((self.live / "1.save").read_bytes(), b"start-slot")  # untouched under a game
        self.pids[:] = [77]
        self.assertTrue(save_pin.settle()["deferred"])
        self.pids.clear()
        settled = save_pin.settle()
        self.assertTrue(settled["put_back"])
        self.assertIsNone(self.lease())
        self.assertEqual((self.live / "1.save").read_bytes(), b"after-play")

    def test_crashed_holder_pruned(self):
        save_pin.acquire("street", "runs/a")
        save_pin.set_holder_pid("runs/a", 100)
        self.pids.append(100)
        save_pin.acquire("street", "runs/b")
        save_pin.set_holder_pid("runs/b", 101)
        self.pids[:] = [101]  # game A crashed; its capture never released
        status = save_pin.lease_status()
        self.assertEqual({h["holder"]: h["alive"] for h in status["lease"]["holders"]},
                         {"runs/a": False, "runs/b": True})
        self.assertEqual(save_pin.release("runs/b")["pruned"], ["runs/a"])
        self.assertFalse(save_pin.release("runs/b")["released"])  # nothing left to drop

    def test_holder_without_pid_lives_with_its_capture_process(self):
        save_pin.acquire("street", "runs/a")  # crashed before the game started: no PID recorded
        first_snapshot = self.lease()["snapshot"]
        self.owners.clear()  # that capture process is gone
        self.assertEqual(save_pin.lease_status()["lease"]["holders"][0]["alive"], False)
        again = save_pin.acquire("street", "runs/b")  # no live holder, no game: put back, start over
        self.assertEqual(again["recovered"]["snapshot"], first_snapshot)
        self.assertNotEqual(again["snapshot"], first_snapshot)
        self.assertEqual(again["action"], "swapped")
        self.assertEqual((self.live / "1.save").read_bytes(), b"start-slot")
        snapshot = json.loads((Path(again["snapshot"]) / "manifest.json").read_text())
        self.assertEqual(set(snapshot["files"]), {"1.save", "2.save"})
        self.assertEqual(again["pruned"], ["runs/a"])
        self.assertEqual([h["holder"] for h in self.lease()["holders"]], ["runs/b"])

    def test_stale_lock_taken_over_and_fresh_lock_waited_for(self):
        lock = self.tmp / "pins/_lease.lock"
        lock.parent.mkdir(parents=True, exist_ok=True)
        lock.write_text("{}")
        with mock.patch.object(save_pin.time, "sleep", lambda _: None):
            with self.assertRaises(SystemExit):  # fresh: another process holds it
                with save_pin.file_lock(lock, timeout=0):
                    pass
        old = save_pin.time.time() - 2 * save_pin.LOCK_STALE_S
        os.utime(lock, (old, old))
        with save_pin.file_lock(lock):
            self.assertTrue(lock.exists())
        self.assertFalse(lock.exists())


if __name__ == "__main__":
    unittest.main()
