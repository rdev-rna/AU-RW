"""Synthetic tests for tools/net_title_survey.py (no game files needed)."""
from __future__ import annotations

import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import net_title_survey as survey  # noqa: E402


def write(tmp: Path, name: str, data: bytes) -> Path:
    p = tmp / name
    p.write_bytes(data)
    return p


class SurveyTests(unittest.TestCase):
    def setUp(self):
        self._td = tempfile.TemporaryDirectory()
        self.tmp = Path(self._td.name)

    def tearDown(self):
        self._td.cleanup()

    def test_rejects_non_pe(self):
        p = write(self.tmp, "x.exe", b"ELF" + b"\0" * 100)
        with self.assertRaises(ValueError):
            survey.survey(p)

    def test_detects_stacks_in_low_entropy_file(self):
        body = b"MZ" + b"\0" * 0x1000 + b"storm::echo::object\0x_NetProtocol\0NetPlayer\0HackNetZ\0" + b"A" * 4096
        r = survey.survey(write(self.tmp, "a.exe", body))
        self.assertFalse(r["packed_suspect"])
        self.assertTrue(r["present"]["storm_echo"])
        self.assertTrue(r["present"]["netprotocol"])
        self.assertTrue(r["present"]["hacknetz"])
        self.assertFalse(r["present"]["rdv"])
        self.assertIn("NetPlayer", r["game_net_class_names"])

    def test_packed_file_reports_unknown_not_absent(self):
        body = b"MZ" + b"\0" * 0xFFE + os.urandom(1 << 20)
        r = survey.survey(write(self.tmp, "p.exe", body))
        self.assertTrue(r["packed_suspect"])
        self.assertIsNone(r["present"]["storm_echo"])

    def test_entropy_bounds(self):
        self.assertEqual(survey.entropy(b""), 0.0)
        self.assertEqual(survey.entropy(b"\0" * 64), 0.0)
        self.assertAlmostEqual(survey.entropy(bytes(range(256))), 8.0)


if __name__ == "__main__":
    unittest.main()
