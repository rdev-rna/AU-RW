"""Unit tests for multi_launch's per-game fps caps and CPU split (no process is started)."""
from __future__ import annotations

from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import multi_launch  # noqa: E402


class CapsTest(unittest.TestCase):
    def test_last_cap_repeats(self):
        self.assertEqual(multi_launch.per_game_caps("60,30", 3), [60, 30, 30])
        self.assertEqual(multi_launch.per_game_caps("45", 2), [45, 45])

    def test_rejects_zero_too_many_and_empty(self):
        for spec, count in (("60,0", 2), ("60,30,30", 2), ("", 1), ("300", 1)):
            with self.assertRaises(ValueError, msg=spec):
                multi_launch.per_game_caps(spec, count)


class CpuPlanTest(unittest.TestCase):
    def test_auto_two_games_on_six_cores(self):
        self.assertEqual(multi_launch.cpu_plan("auto", 2, 6), [("0-3", "normal"), ("4-5", "normal")])

    def test_auto_three_games_on_eight_cores(self):
        self.assertEqual(multi_launch.cpu_plan("auto", 3, 8),
                         [("0-3", "normal"), ("4-5", "normal"), ("6-7", "normal")])
        self.assertEqual(multi_launch.cpu_plan("auto", 2, 6, "below_normal"),
                         [("0-3", "normal"), ("4-5", "below_normal")])

    def test_auto_falls_back_when_too_few_cores(self):
        self.assertEqual(multi_launch.cpu_plan("auto", 3, 5), [(None, "normal")] * 3)
        self.assertEqual(multi_launch.cpu_plan("auto", 1, 6), [(None, "normal")])

    def test_none_and_explicit(self):
        self.assertEqual(multi_launch.cpu_plan("none", 2, 6), [(None, "normal")] * 2)
        self.assertEqual(multi_launch.cpu_plan("0-2/3-5", 2, 6), [("0-2", "normal"), ("3-5", "normal")])
        with self.assertRaises(ValueError):
            multi_launch.cpu_plan("0-2", 2, 6)


if __name__ == "__main__":
    unittest.main()
