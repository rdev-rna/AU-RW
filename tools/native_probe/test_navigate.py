"""Unit tests for navigate.py's control maths (no game): a simulated camera offset is learned
and the walk converges on the target."""
from __future__ import annotations

import math
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from navigate import stick_for, update_offset, wrap  # noqa: E402


def simulate(target, camera_offset, start_offset, steps=200, speed=0.4):
    """Arno moves `speed` per step in world direction stick_yaw - camera_offset."""
    pos, offset = [0.0, 0.0], start_offset
    for _ in range(steps):
        dx, dy = target[0] - pos[0], target[1] - pos[1]
        if math.hypot(dx, dy) < 0.3:
            return pos, offset, True
        stick = stick_for(math.atan2(dy, dx), offset)
        stick_yaw = math.atan2(stick[1], stick[0])
        move = stick_yaw - camera_offset
        step = min(speed, math.hypot(dx, dy))
        pos = [pos[0] + step * math.cos(move), pos[1] + step * math.sin(move)]
        offset = update_offset(offset, stick_yaw, move)
    return pos, offset, False


class NavigateTest(unittest.TestCase):
    def test_wrap(self):
        self.assertAlmostEqual(wrap(3 * math.pi), -math.pi)
        self.assertAlmostEqual(wrap(-0.1), -0.1)

    def test_stick_points_where_expected_with_zero_offset(self):
        self.assertEqual(stick_for(math.pi / 2, 0.0), [0, 32767])
        self.assertEqual(stick_for(0.0, 0.0), [32767, 0])

    def test_offset_learned_across_wraparound(self):
        offset = 0.0
        for _ in range(20):
            offset = update_offset(offset, math.radians(170), math.radians(-160))  # observed -30 deg
        self.assertAlmostEqual(math.degrees(offset), -30, delta=0.5)

    def test_walk_converges_with_unknown_camera(self):
        for camera in (-2.5, -0.7, 0.0, 1.2, 3.0):
            pos, offset, reached = simulate((6.0, -4.0), camera, start_offset=0.0)
            self.assertTrue(reached, camera)
            self.assertAlmostEqual(wrap(offset - camera), 0, delta=0.05)


if __name__ == "__main__":
    unittest.main()
