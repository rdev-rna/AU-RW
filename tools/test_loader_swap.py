"""Unit tests for loader_swap: rename planning, refusals and rollback, in a temporary folder."""
from __future__ import annotations

from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import loader_swap as ls  # noqa: E402

ORIGINAL, CUSTOM = b"original-bytes", b"custom-bytes"


class LoaderSwapTest(unittest.TestCase):
    def setUp(self):
        self.game = Path(tempfile.mkdtemp())
        (self.game / ls.ACTIVE).write_bytes(CUSTOM)
        (self.game / ls.STASH["original"]).write_bytes(ORIGINAL)
        (self.game / ls.SPARE).write_bytes(ORIGINAL)
        known = {ls.hashlib.sha256(ORIGINAL).hexdigest(): "original", ls.hashlib.sha256(CUSTOM).hexdigest(): "custom"}
        self.patches = [mock.patch.object(ls, "KNOWN", known), mock.patch.object(ls, "acu_running", lambda: []),
                        mock.patch.object(ls, "LOG", self.game / "log.jsonl")]
        for p in self.patches:
            p.start()

    def tearDown(self):
        for p in self.patches:
            p.stop()

    def files(self):
        return {p.name: p.read_bytes() for p in self.game.iterdir() if p.name != "log.jsonl"}

    def test_round_trip(self):
        result = ls.use("original", self.game)
        self.assertEqual(result["renames"], [(ls.ACTIVE, ls.STASH["custom"]), (ls.STASH["original"], ls.ACTIVE)])
        self.assertEqual(self.files(), {ls.ACTIVE: ORIGINAL, ls.STASH["custom"]: CUSTOM, ls.SPARE: ORIGINAL})
        ls.use("custom", self.game)
        self.assertEqual(self.files(), {ls.ACTIVE: CUSTOM, ls.STASH["original"]: ORIGINAL, ls.SPARE: ORIGINAL})
        self.assertEqual(len((self.game / "log.jsonl").read_text().splitlines()), 2)

    def test_already_active_is_a_no_op(self):
        self.assertEqual(ls.use("custom", self.game)["renames"], [])

    def test_refuses_while_acu_runs(self):
        with mock.patch.object(ls, "acu_running", lambda: [42]):
            with self.assertRaises(RuntimeError):
                ls.use("original", self.game)
        self.assertEqual(self.files()[ls.ACTIVE], CUSTOM)

    def test_refuses_unknown_active(self):
        (self.game / ls.ACTIVE).write_bytes(b"something else")
        with self.assertRaises(ValueError):
            ls.use("original", self.game)

    def test_refuses_to_overwrite_a_stash(self):
        (self.game / ls.STASH["custom"]).write_bytes(CUSTOM)
        with self.assertRaises(ValueError):
            ls.use("original", self.game)
        self.assertEqual(self.files()[ls.ACTIVE], CUSTOM)

    def test_refuses_missing_target(self):
        (self.game / ls.STASH["original"]).unlink()
        with self.assertRaises(ValueError):
            ls.use("original", self.game)

    def test_second_rename_failure_is_rolled_back(self):
        real = ls.os.rename
        calls = []

        def flaky(src, dst):
            calls.append((src, dst))
            if len(calls) == 2:
                raise OSError("simulated")
            real(src, dst)
        with mock.patch.object(ls.os, "rename", flaky):
            with self.assertRaises(OSError):
                ls.use("original", self.game)
        self.assertEqual(self.files(), {ls.ACTIVE: CUSTOM, ls.STASH["original"]: ORIGINAL, ls.SPARE: ORIGINAL})


if __name__ == "__main__":
    unittest.main()
