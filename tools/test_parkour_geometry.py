"""Synthetic tests for tools/parkour_geometry.py (ACU-RE-0055). No game data is read."""
from __future__ import annotations

import json
import math
import struct
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import parkour_geometry as pg  # noqa: E402


def candidate(source=(1.0, 2.0, 0.0), target=(2.0, 2.0, 1.5), normal=(-1.0, 0.0, 0.0),
              seg_a=(2.0, 1.5, 1.5), seg_b=(2.0, 2.5, 1.5), table=0x143A58280, fitness=0.9,
              scalars=None) -> bytes:
    raw = bytearray(pg.CANDIDATE_BYTES)
    struct.pack_into("<Q", raw, 0x08, table)
    for off, v in ((0x10, source), (0x30, target), (0x40, normal), (0x50, seg_a), (0x60, seg_b),
                   (0x190, target), (0x1A0, target)):
        struct.pack_into("<3f", raw, off, *v)
    if scalars is None:
        dz = target[2] - source[2]
        h = math.hypot(target[0] - source[0], target[1] - source[1])
        scalars = (dz, h, math.hypot(dz, h))
    struct.pack_into("<3f", raw, 0xC0, *scalars)
    struct.pack_into("<f", raw, 0x204, fitness)
    return bytes(raw)


# A 1 m high, 1 m deep block whose front wall is the plane x = 2 (open side x < 2),
# top z = 1.5, running from y = 0 to y = 4, plus floor at z = 0 in front.
TOP = [((2, 0, 1.5), (3, 0, 1.5), (3, 4, 1.5)), ((2, 0, 1.5), (3, 4, 1.5), (2, 4, 1.5))]
WALL = [((2, 0, 0.5), (2, 0, 1.5), (2, 4, 1.5)), ((2, 0, 0.5), (2, 4, 1.5), (2, 4, 0.5))]
FLOOR = [((-5, -5, 0.0), (2, -5, 0.0), (2, 9, 0.0)), ((-5, -5, 0.0), (2, 9, 0.0), (-5, 9, 0.0))]


def column(x, y):
    """Synthetic world: floor z=0 for x<2, block top z=1.5 for 2<=x<=3 (|normal z| = 1)."""
    if 2.0 <= x <= 3.0 and 0.0 <= y <= 4.0:
        return [(1.5, 1.0), (0.5, 1.0)]
    return [(0.0, 1.0)] if x < 2.0 else []


class DecodeTests(unittest.TestCase):
    def test_round_trip(self):
        c = pg.decode_candidate(candidate())
        self.assertEqual(c["table"], 0x143A58280)
        self.assertEqual(c["source"], (1.0, 2.0, 0.0))
        self.assertEqual(c["target"], (2.0, 2.0, 1.5))
        self.assertAlmostEqual(c["fitness"], 0.9, places=6)

    def test_rejects_wrong_length_and_type(self):
        with self.assertRaises(ValueError):
            pg.decode_candidate(candidate()[:-1])
        with self.assertRaises(ValueError):
            pg.decode_candidate(candidate() + b"\0")
        with self.assertRaises(ValueError):
            pg.decode_candidate("not bytes")

    def test_rejects_non_finite_position(self):
        raw = bytearray(candidate())
        struct.pack_into("<f", raw, 0x30, float("nan"))
        with self.assertRaises(ValueError):
            pg.decode_candidate(bytes(raw))


class ScalarAndSegmentTests(unittest.TestCase):
    def test_scalars_reproduced(self):
        r = pg.scalar_residuals(pg.decode_candidate(candidate()))
        for v in r.values():
            self.assertLess(v, 1e-5)

    def test_scalars_mismatch_detected(self):
        r = pg.scalar_residuals(pg.decode_candidate(candidate(scalars=(1.0, 1.0, math.sqrt(2)))))
        self.assertGreater(r["c0"], 0.4)
        self.assertLess(r["c4"], 1e-6)
        self.assertLess(r["c8_hypot"], 1e-6)   # +0xC8 = hypot(+0xC0, +0xC4) still holds

    def test_segment_checks(self):
        g = pg.segment_checks(pg.decode_candidate(candidate()))
        self.assertTrue(g["target_on"] and g["perpendicular"] and g["level"] and g["normal_unit"])
        self.assertTrue(g["normal_toward_source"])
        self.assertFalse(g["degenerate"])

    def test_degenerate_segment(self):
        g = pg.segment_checks(pg.decode_candidate(candidate(seg_a=(2, 2, 1.5), seg_b=(2, 2, 1.5))))
        self.assertTrue(g["degenerate"])
        self.assertIsNone(g["perpendicular"])

    def test_target_off_segment(self):
        g = pg.segment_checks(pg.decode_candidate(candidate(target=(2.0, 3.0, 1.5))))
        self.assertFalse(g["target_on"])

    def test_segment_angle(self):
        self.assertAlmostEqual(pg.segment_angle((0, 0, 0), (1, 0, 0), (0, 1, 0)), 90.0)
        self.assertAlmostEqual(pg.segment_angle((0, 0, 0), (1, 0, 0), (-1, 0, 0)), 0.0)
        self.assertIsNone(pg.segment_angle((0, 0, 0), (0, 0, 0), (1, 0, 0)))


class GeometryMathTests(unittest.TestCase):
    def test_ray_hits_wall_at_expected_distance(self):
        t = pg.ray_triangles((0, 2, 1.0), (1, 0, 0), np.array(WALL + TOP))
        self.assertAlmostEqual(float(t.min()), 2.0, places=6)

    def test_ray_misses_above_wall(self):
        t = pg.ray_triangles((0, 2, 1.75), (1, 0, 0), np.array(WALL))
        self.assertTrue(np.all(np.isinf(t)))

    def test_ray_behind_origin_is_miss(self):
        t = pg.ray_triangles((3, 2, 1.0), (1, 0, 0), np.array(WALL))
        self.assertTrue(np.all(np.isinf(t)))

    def test_face_normal_degenerate(self):
        with self.assertRaises(ValueError):
            pg.face_normal(((0, 0, 0), (1, 0, 0), (2, 0, 0)))

    def test_convex_ledge_edge(self):
        e = pg.nearest_edge((2.0, 2.0, 1.5), np.array(TOP + WALL + FLOOR))
        self.assertLess(e["distance"], 1e-9)
        self.assertEqual(e["kind"], "convex")
        self.assertEqual(pg.edge_class(e), "convex_ledge")
        self.assertAlmostEqual(e["top_nz"], 1.0)
        self.assertAlmostEqual(e["other_nz"], 0.0)

    def test_concave_corner_edge(self):
        # wall rising from a floor: edge at z = 0.5 is an inside corner
        floor2 = [((1, 0, 0.5), (2, 0, 0.5), (2, 4, 0.5)), ((1, 0, 0.5), (2, 4, 0.5), (1, 4, 0.5))]
        e = pg.nearest_edge((2.0, 2.0, 0.5), np.array(floor2 + WALL))
        self.assertEqual(e["kind"], "concave")
        self.assertEqual(pg.edge_class(e), "concave")

    def test_boundary_edge(self):
        e = pg.nearest_edge((2.0, 2.0, 1.5), np.array(TOP))
        self.assertEqual(e["kind"], "boundary")

    def test_nearest_edge_empty(self):
        self.assertIsNone(pg.nearest_edge((0, 0, 0), np.zeros((0, 3, 3))))

    def test_classify_edge_requires_faces(self):
        with self.assertRaises(ValueError):
            pg.classify_edge(((0, 0, 0), (1, 0, 0)), [])


class SegmentEdgeTests(unittest.TestCase):
    def test_whole_edge(self):
        r = pg.segment_vs_edge((2, 0, 1.5), (2, 4, 1.5), (2, 2, 1.5), (2, 0, 1.5), (2, 4, 1.5))
        self.assertEqual(r, {"on_edge": True, "ends_on_vertices": 2, "target_midpoint": True,
                             "longer_than_edge": False})

    def test_clipped_span_off_midpoint(self):
        r = pg.segment_vs_edge((2, 1, 1.5), (2, 4, 1.5), (2, 1.5, 1.5), (2, 0, 1.5), (2, 4, 1.5))
        self.assertTrue(r["on_edge"])
        self.assertEqual(r["ends_on_vertices"], 1)
        self.assertFalse(r["target_midpoint"])

    def test_off_edge_and_degenerate(self):
        r = pg.segment_vs_edge((2.1, 0, 1.5), (2.1, 4, 1.5), (2.1, 2, 1.5), (2, 0, 1.5), (2, 4, 1.5))
        self.assertFalse(r["on_edge"])
        self.assertIsNone(pg.segment_vs_edge((2, 2, 1.5), (2, 2, 1.5), (2, 2, 1.5), (2, 0, 1.5), (2, 4, 1.5)))


class FixtureTests(unittest.TestCase):
    def test_compact_row_and_finite(self):
        row = {"action_type": 53, "run": "r1", "selected": True, "source": [0, 0, 0], "target": [1, 0, 1],
               "normal": [-1, 0, 0], "segment": [[1, -1, 1], [1, 1, 1]], "c0": 1.0, "c4": 1.0, "c8": 1.4142,
               "scalars_reproduced": True, "source_ground_dz": None, "profile": "ledge_minus",
               "edge": None, "segment_edge": None, "fan": {}}
        out = pg.compact_row(row, ["r0", "r1"])
        self.assertEqual(len(out), len(pg.ROW_COLUMNS))
        self.assertEqual(out[1], 1)
        self.assertEqual(pg._finite({"a": [math.inf, 1.0]}), {"a": [None, 1.0]})


class ProfileTests(unittest.TestCase):
    def test_ledge_with_outward_normal(self):
        sides = pg.ledge_profile(column, (2.0, 2.0, 1.5), (-1.0, 0.0, 0.0))
        self.assertEqual(pg.classify_profile(sides), "ledge_minus")   # top behind -normal
        self.assertAlmostEqual(sides["plus"]["drop"], 1.5)

    def test_ledge_with_inward_normal(self):
        sides = pg.ledge_profile(column, (2.0, 2.0, 1.5), (1.0, 0.0, 0.0))
        self.assertEqual(pg.classify_profile(sides), "ledge_plus")

    def test_flat_ground(self):
        sides = pg.ledge_profile(lambda x, y: [(0.0, 1.0)], (0.0, 0.0, 0.0), (1.0, 0.0, 0.0))
        self.assertEqual(pg.classify_profile(sides), "top_both")

    def test_no_surface(self):
        sides = pg.ledge_profile(lambda x, y: [], (0.0, 0.0, 5.0), (1.0, 0.0, 0.0))
        self.assertEqual(pg.classify_profile(sides), "no_top")
        self.assertTrue(math.isinf(sides["plus"]["drop"]))

    def test_vertical_normal_rejected(self):
        with self.assertRaises(ValueError):
            pg.ledge_profile(column, (2.0, 2.0, 1.5), (0.0, 0.0, 1.0))


class TraceTests(unittest.TestCase):
    def write_run(self, root: Path, name: str, select_index: int):
        run = root / name
        run.mkdir(parents=True)
        a, b = candidate(), candidate(target=(2.0, 3.0, 1.5), table=0x143A1F9D0)
        el = [{"ptr": "1", "hex": a.hex()}, {"ptr": "2", "hex": b.hex()}]
        bad = [{"ptr": "3", "hex": (b"\0" * 10).hex()}]
        events = [
            {"label": pg.ENTRY_LABEL, "sequence": 1, "qpc": 10, "tid": 7, "registers": [],
             "gather": {"ok": True, "base": "aa", "count": 3, "elements": el + bad}},
            {"label": pg.EXIT_LABEL, "sequence": 2, "qpc": 11, "tid": 7,
             "registers": ["0"] * 4 + [f"{select_index & 0xFFFFFFFF:x}"],
             "gather": {"ok": True, "base": "aa", "count": 2, "elements": el}},
            {"label": "other", "sequence": 3, "qpc": 12, "tid": 7, "registers": []},
        ]
        lines = [json.dumps({"kind": "header"}), json.dumps({"kind": "events", "events": events})]
        (run / "native-session.jsonl").write_text("\n".join(lines) + "\n", encoding="utf-8")
        return run

    def test_collect_selected_and_invalid(self):
        with tempfile.TemporaryDirectory() as tmp:
            old = pg.RUNS
            pg.RUNS = Path(tmp)
            try:
                run = self.write_run(Path(tmp), "r1", 1)
                cands, stats = pg.collect([run], {0x143A58280: 53, 0x143A1F9D0: 14})
            finally:
                pg.RUNS = old
        self.assertEqual(len(cands), 2)
        self.assertEqual(stats["selections"], 1)
        self.assertEqual(sum(v for k, v in stats.items() if k.startswith("undecodable")), 1)
        chosen = [c for c in cands if c["selected"]]
        self.assertEqual([c["action_type"] for c in chosen], [14])

    def test_collect_no_selection(self):
        with tempfile.TemporaryDirectory() as tmp:
            old = pg.RUNS
            pg.RUNS = Path(tmp)
            try:
                run = self.write_run(Path(tmp), "r2", -1)
                cands, stats = pg.collect([run], {})
            finally:
                pg.RUNS = old
        self.assertFalse(any(c["selected"] for c in cands))
        self.assertNotIn("selections", stats)
        self.assertTrue(all(c["action_type"] is None for c in cands))

    def test_field_survey_counts(self):
        cands = [dict(pg.decode_candidate(candidate()), action_type=53)]
        s = pg.field_survey(cands)
        self.assertEqual(s["per_type"]["53"]["all_three_reproduced"], 1)
        self.assertEqual(s["per_type"]["53"]["p190_eq_target"], 1)
        self.assertEqual(s["per_type"]["53"]["p180_eq_target"], 0)


if __name__ == "__main__":
    unittest.main()
