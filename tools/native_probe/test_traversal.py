"""Headless tests for the automated traversal tools: window classification from collision
columns, approach planning, wall routes, clip bookkeeping, and the closed-loop driver against a
simulated game (no ACU process). Run: python -m unittest tools/native_probe/test_traversal.py
"""
from __future__ import annotations

import math
import sys
import threading
import time
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import traversal_driver as T  # noqa: E402
import traversal_targets as G  # noqa: E402


def window(row0=(1.0, 0.0, 0.0), row1=(0.0, 1.0, 0.0), floor=1, pos=(10.0, 20.0, 2.0), sill=1.6, top=3.7):
    return {"file_id": "00000000000000aa", "stem": f"FRV_Interior_D_{floor}F_WD_01_Open", "floor": floor,
            "position": list(pos), "row0": list(row0), "row1": list(row1), "center": [pos[0], pos[1] + 0.45, (sill + top) / 2],
            "sill_z": sill, "top_z": top, "width": 3.5}


def columns(inside_sign, blockers=()):
    """Collision: interior floors on the inside half-space (sign of x - 10), sidewalk at z=0 outside."""
    def column(x, y):
        side = 1 if x > 10 else -1
        if side == inside_sign:
            return [{"z": 6.0, "shape_name": "FRV_Interior_D_2F_FL_01"}, {"z": 0.5, "shape_name": "FRV_Interior_D_1F_FL_01"}]
        hits = [{"z": 0.02, "shape_name": "FRV_Sidewalk_IC_13x01"}]
        for at, z in blockers:
            if abs(abs(x - 10) - at) < 0.01:
                hits.insert(0, {"z": z, "shape_name": "GEN_ALL_Awning_6m_04"})
        return hits
    return column


class Classify(unittest.TestCase):
    def test_outward_is_the_non_interior_side(self):
        w, why = G.classify(window(), columns(inside_sign=1))
        self.assertIsNone(why)
        self.assertEqual(w["outward"], [-1.0, 0.0])
        self.assertAlmostEqual(w["ground_z"], 0.02)
        self.assertAlmostEqual(w["sill_height"], 1.58)
        self.assertTrue(w["clear"])
        w, _ = G.classify(window(), columns(inside_sign=-1))
        self.assertEqual(w["outward"], [1.0, 0.0])

    def test_blocker_and_ambiguity(self):
        w, _ = G.classify(window(), columns(inside_sign=1, blockers=[(2.5, 3.0)]))
        self.assertFalse(w["clear"])
        self.assertEqual(w["blockers"][0]["at_m"], 2.5)
        both = lambda x, y: [{"z": 0.5, "shape_name": "FRV_Interior_A_1F_FL"}]  # noqa: E731
        self.assertIsNone(G.classify(window(), both)[0])
        self.assertIsNone(G.classify(window(row0=(0.0, 0.0, 1.0)), columns(1))[0])

    def test_placement_filter(self):
        rows = [{"stem": "FRV_Interior_D_2F_WD_01_Open", "file_id": "1", "position": [1, 1, 6],
                 "rotation_rows": [[1, 0, 0], [0, 1, 0], [0, 0, 1]], "bbox_world": {"min": [0.9, 0, 5.6], "max": [1.1, 3, 8]}},
                {"stem": "FRV_Interior_D_2F_WD_01", "file_id": "2", "position": [1, 1, 6],
                 "rotation_rows": [[1, 0, 0], [0, 1, 0], [0, 0, 1]], "bbox_world": {"min": [0, 0, 0], "max": [1, 1, 1]}},
                {"stem": "FRV_Interior_D_1F_WD_01_Open", "file_id": "3", "position": [500, 1, 2],
                 "rotation_rows": [[1, 0, 0], [0, 1, 0], [0, 0, 1]], "bbox_world": {"min": [0, 0, 0], "max": [1, 1, 1]}}]
        found = list(G.windows_from_placements(rows, (0, 0, 10, 10)))
        self.assertEqual([w["file_id"] for w in found], ["1"])
        self.assertEqual(found[0]["floor"], 2)
        self.assertAlmostEqual(found[0]["width"], 3.0)


def classified(**kw):
    w, _ = G.classify(window(**kw), columns(inside_sign=1))
    return w


class Planning(unittest.TestCase):
    def test_run_in_angles_start_on_rays_through_the_placement(self):
        w = classified()
        p0 = T.plan_run_in_angle(w, 0, "lpf")
        self.assertAlmostEqual(p0.start[0], 10 - 6.0, places=6)
        self.assertAlmostEqual(p0.start[1], 20.0, places=6)
        self.assertAlmostEqual(math.degrees(p0.face_yaw), 0.0, places=6)  # facing +x, into the window
        p30 = T.plan_run_in_angle(w, 30, "hpf")
        d = (p30.start[0] - 10, p30.start[1] - 20)
        self.assertAlmostEqual(math.hypot(*d), 6.0, places=6)
        self.assertAlmostEqual(math.degrees(math.atan2(d[1], -d[0])), 30.0, places=6)  # toward +row1
        self.assertEqual(p30.params["profile"], "hpf")

    def test_every_angle_twice(self):
        w1, w2 = classified(), classified(floor=2, sill=6.6, top=8.5)
        plans = T.build_plans([w1, w2], ["run_in", "wall"])
        runs = [p for p in plans if p.maneuver == "run_in"]
        walls = [p for p in plans if p.maneuver == "wall"]
        self.assertEqual(len(runs), len(T.RUN_ANGLES) * 2)
        self.assertEqual(len(walls), len(T.WALL_DIRECTIONS) * 2)
        for angle in T.RUN_ANGLES:
            self.assertEqual(sorted(p.params["profile"] for p in runs if p.params["angle_deg"] == angle), ["hpf", "lpf"])

    def test_wall_start_geometry(self):
        w = classified(floor=2, sill=6.6, top=8.5)
        right = T.plan_wall(w, 0, "lpf")    # moving toward +u: start on the -u side, centre height
        self.assertAlmostEqual(right.params["start_u"], -1.8)
        self.assertAlmostEqual(right.params["start_body_z"], 7.55)
        up = T.plan_wall(w, 90, "hpf")      # moving up: start below the centre
        self.assertAlmostEqual(up.params["start_u"], 0.0, places=6)
        self.assertAlmostEqual(up.params["start_body_z"], 7.55 - 1.8)
        # teleport under the window, 1.5 m out
        self.assertAlmostEqual(right.start[0], 10 - 1.5, places=6)
        self.assertAlmostEqual(right.start[1], 20.0, places=6)

    def test_wall_route_goes_round_the_window(self):
        # start above the window, inside its width: round the +u side (su = 0 -> +)
        r = T.wall_route(0.0, 9.3, 6.0, 1.75, 6.6)
        self.assertEqual(r, [(2.65, 6.0), (2.65, 9.3), (0.0, 9.3)])
        # start beside the window: straight sideways then up
        self.assertEqual(T.wall_route(-1.8, 7.5, 6.0, 1.75, 6.6), [(-2.65, 6.0), (-2.65, 7.5), (-1.8, 7.5)])
        self.assertEqual(T.wall_route(-2.6, 7.5, 6.0, 1.75, 6.6), [(-2.6, 6.0), (-2.6, 7.5)])
        self.assertEqual(T.wall_route(0.0, 5.8, 5.8, 1.75, 6.6), [(0.0, 5.8), (0.0, 5.8)])

    def test_wall_right_sign(self):
        # traversal-4 window: outward (0.866, -0.5), row1 (0.5, 0.866): stick right moved +row1
        w = {"outward": [0.8660254, -0.5], "row1": [0.5, 0.8660254, 0.0]}
        self.assertEqual(T.Driver(None, None).wall_right_sign(w), 1.0)
        w["row1"] = [-0.5, -0.8660254, 0.0]
        self.assertEqual(T.Driver(None, None).wall_right_sign(w), -1.0)


class Clips(unittest.TestCase):
    def test_sequence_and_match(self):
        s = [{"qpc": 1, "clips": [{"file_id": "a", "name": "xx_Nav_Run_3-Loop", "weight": 1.0}]},
             {"qpc": 2, "clips": [{"file_id": "a", "name": "xx_Nav_Run_3-Loop", "weight": 0.6},
                                  {"file_id": "b", "name": "xx_Window_Run_WindowEntry_Fd0Deg150cm_Footr", "weight": 0.04}]},
             {"qpc": 3, "clips": [{"file_id": "b", "name": "xx_Window_Run_WindowEntry_Fd0Deg150cm_Footr", "weight": 0.9}]}]
        seq = T.clip_sequence(s)
        self.assertEqual([c["name"] for c in seq], ["xx_Nav_Run_3-Loop", "xx_Window_Run_WindowEntry_Fd0Deg150cm_Footr"])
        self.assertEqual(seq[1]["first_qpc"], 3)  # below min weight at qpc 2
        self.assertEqual(T.matched(s, T.EXPECT["run_in"]), "xx_Window_Run_WindowEntry_Fd0Deg150cm_Footr")
        self.assertIsNone(T.matched(s[:1], T.EXPECT["run_in"]))
        self.assertEqual(T.matched([{"clips": [{"name": "xx_WindowGratingEntry_Climb_Lt-High_tr_IdleHigh",
                                                "weight": 1, "file_id": "c"}]}], T.EXPECT["wall"]),
                         "xx_WindowGratingEntry_Climb_Lt-High_tr_IdleHigh")


class FakeGame:
    """Kinematic stand-in: the stick drives Arno at 5 m/s along stick yaw minus a hidden camera
    offset; RT is needed for a window entry, which plays when he crosses the window plane within
    0.8 m of the placement. Every teleport swaps the entity pointer, like the real game can."""

    def __init__(self, w, camera_offset_deg=37.0):
        self.w, self.offset = w, math.radians(camera_offset_deg)
        self.p, self.entity, self.action, self.entered = [0.0, 0.0, 0.0], 0x1000, {}, False
        self.t = time.monotonic()
        self.lock = threading.Lock()

    def _advance(self):
        now = time.monotonic()
        dt, self.t = now - self.t, now
        st = self.action.get("left_stick", [0, 0])
        if st != [0, 0] and not self.entered:
            yaw = math.atan2(st[1], st[0]) - self.offset
            m = 5.0 * min(1.0, math.hypot(*st) / T.STICK) * dt
            before = self.p[:]
            self.p[0] += math.cos(yaw) * m
            self.p[1] += math.sin(yaw) * m
            o, c = self.w["outward"], self.w["position"]
            side = lambda q: (q[0] - c[0]) * o[0] + (q[1] - c[1]) * o[1]  # noqa: E731
            lateral = (self.p[0] - c[0]) * self.w["row1"][0] + (self.p[1] - c[1]) * self.w["row1"][1]
            if side(before) > 0.5 >= side(self.p):
                if abs(lateral) < 0.8 and self.action.get("right_trigger"):
                    self.entered = True
                else:
                    self.p = before  # the wall stops him

    def teleport(self, x, y, z, yaw_deg):
        with self.lock:
            self.p, self.entity, self.entered = [x, y, z], self.entity + 0x10, False
        return {"status": 1, "error_m": 0.0, "fell_through": False}

    def sample(self):
        with self.lock:
            self._advance()
            clip = "xx_Window_Run_WindowEntry_Fd0Deg150cm_Footr" if self.entered else "xx_Nav_Run_3-Loop"
            return {"qpc": int(time.monotonic() * 1e7), "entity": f"0x{self.entity:X}", "position": self.p[:],
                    "forward": [1.0, 0.0, 0.0], "clips": [{"file_id": "f" * 16, "name": clip, "weight": 1.0}]}

    def pad(self, action):
        with self.lock:
            self._advance()
            self.action = action


class SimulatedDriver(unittest.TestCase):
    def run_plan(self, profile):
        w = classified()
        game = FakeGame(w)
        rec = T.Recorder(game, lambda fid: None, hz=100).start()
        try:
            drv = T.Driver(game, rec, step=0.02)
            return drv.attempt(T.plan_run_in_angle(w, 20, profile)), drv
        finally:
            rec.stop()

    def test_high_profile_run_enters_and_learns_camera_offset(self):
        r, drv = self.run_plan("hpf")
        self.assertTrue(r["ok"], r.get("stop"))
        self.assertEqual(r["matched"], "xx_Window_Run_WindowEntry_Fd0Deg150cm_Footr")
        self.assertAlmostEqual(math.degrees(drv.offset), 37.0, delta=6.0)
        self.assertGreaterEqual(len({s["entity"] for s in r["samples"]}), 1)

    def test_low_profile_run_is_reported_as_no_entry(self):
        r, _ = self.run_plan("lpf")
        self.assertFalse(r["ok"])
        self.assertEqual(r["stop"], "timeout")


if __name__ == "__main__":
    unittest.main()
