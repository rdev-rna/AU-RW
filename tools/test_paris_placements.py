"""Format-boundary tests for tools/paris_placements.py (synthetic payloads, no game data).

Run: python -m unittest tools/test_paris_placements.py
"""
import math
from pathlib import Path
import struct
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import paris_placements as pp  # noqa: E402

ENTITY_ID = 0x00000010AEA08064


def header(type_id, self_id=ENTITY_ID, version=1):
    return struct.pack("<BQI", version, self_id, type_id)


def matrix(yaw_deg=0.0, translation=(0.0, 0.0, 0.0), w_column=(0.0, 0.0, 0.0, 1.0)):
    c, s = math.cos(math.radians(yaw_deg)), math.sin(math.radians(yaw_deg))
    rows = [c, s, 0.0, w_column[0], -s, c, 0.0, w_column[1], 0.0, 0.0, 1.0, w_column[2],
            *translation, w_column[3]]
    return struct.pack("<16f", *rows)


def bbox_tail(lo, hi):
    """bbox tag + 6 floats + 12 bytes, then the tail tag 40 bytes after the bbox tag."""
    return pp.TAG_BBOX + struct.pack("<6f", *lo, *hi) + bytes(12) + pp.TAG_TAIL + bytes(8)


def entity_flag3(yaw=0.0, translation=(553.877, -747.872, 4.689), body=b"",
                 lo=(-1.0, -0.5, 0.0), hi=(1.0, 0.5, 1.2)):
    return (header(pp.TYPE_ENTITY) + b"\x03" + matrix(yaw, translation)
            + struct.pack("<I", 1) + body + bbox_tail(lo, hi))


def entity_flag0(world_t=(10.0, 20.0, 3.0), local_t=(-6.0, 1.5, 6.5), tag=0x788BAA0D):
    payload = (header(pp.TYPE_ENTITY) + b"\x00" + struct.pack("<QI", ENTITY_ID + 3, tag)
               + matrix(0.0, local_t) + bytes(25) + matrix(0.0, world_t))
    assert len(payload) == 179
    return payload + struct.pack("<I", 2) + bbox_tail((0, 0, 0), (1, 1, 1))


class DumpReader(unittest.TestCase):
    def record(self, payload=b"abc", name=b"n"):
        return (struct.pack("<IIQIH", 5, 0, ENTITY_ID, pp.TYPE_ENTITY, len(name)) + name
                + struct.pack("<I", len(payload)) + payload)

    def test_round_trip(self):
        data = pp.DUMP_MAGIC + self.record() + self.record(b"xy", b"other")
        rows = list(pp.read_dump(data))
        self.assertEqual([r[4] for r in rows], ["n", "other"])
        self.assertEqual(data[rows[1][5]:rows[1][5] + rows[1][6]], b"xy")

    def test_bad_magic_and_truncation(self):
        with self.assertRaises(pp.FormatError):
            list(pp.read_dump(b"ACUTD999"))
        full = pp.DUMP_MAGIC + self.record(b"0123456789")
        for cut in (len(full) - 1, len(pp.DUMP_MAGIC) + 10, len(pp.DUMP_MAGIC) + 23):
            with self.assertRaises(pp.FormatError, msg=f"cut {cut}"):
                list(pp.read_dump(full[:cut]))


class Cell(unittest.TestCase):
    def cell(self, children, declared=None, extra=b""):
        n = len(children) if declared is None else declared
        body = b"".join(struct.pack("<HQ", 1, c) for c in children)
        return header(pp.TYPE_CELL) + struct.pack("<I", n) + body + struct.pack("<III", 114, 5201, 0) + extra

    def test_valid(self):
        cell = pp.parse_cell(self.cell([1, 2, 3]))
        self.assertEqual([c for _, c in cell["children"]], [1, 2, 3])
        self.assertEqual(cell["tail"], (114, 5201, 0))

    def test_count_mismatch_and_trailing_bytes(self):
        with self.assertRaises(pp.FormatError):
            pp.parse_cell(self.cell([1, 2], declared=3))
        with self.assertRaises(pp.FormatError):
            pp.parse_cell(self.cell([1], extra=b"\x00"))

    def test_wrong_type_short_and_over_limit(self):
        with self.assertRaises(pp.FormatError):
            pp.parse_cell(header(pp.TYPE_ENTITY) + bytes(16))
        with self.assertRaises(pp.FormatError):
            pp.parse_cell(header(pp.TYPE_CELL)[:10])
        with self.assertRaises(pp.FormatError):
            pp.parse_cell(header(pp.TYPE_CELL) + struct.pack("<I", pp.MAX_CELL_CHILDREN + 1))

    def test_bad_version(self):
        with self.assertRaises(pp.FormatError):
            pp.parse_cell(header(pp.TYPE_CELL, version=2) + bytes(16))


class Transform(unittest.TestCase):
    def test_flag3_world_matrix(self):
        placed = pp.parse_placed(entity_flag3(yaw=47.1), pp.TYPE_ENTITY)
        self.assertEqual(placed["layout"], "plain-matrix@14")
        self.assertAlmostEqual(placed["position"][0], 553.877, places=3)
        self.assertAlmostEqual(placed["yaw_deg_about_row2"], 47.1, places=3)
        self.assertEqual(placed["components"], 1)
        self.assertLess(placed["row2_deviation"], 1e-6)

    def test_flag0_uses_world_matrix_at_115(self):
        placed = pp.parse_placed(entity_flag0(), pp.TYPE_ENTITY)
        self.assertEqual(placed["position"], [10.0, 20.0, 3.0])
        self.assertEqual(placed["local_translation"], [-6.0, 1.5, 6.5])
        self.assertEqual(placed["components"], 2)

    def test_flag0_wrong_tag(self):
        with self.assertRaises(pp.FormatError):
            pp.parse_placed(entity_flag0(tag=0x12345678), pp.TYPE_ENTITY)

    def test_non_affine_and_unsupported_flag(self):
        bad = header(pp.TYPE_ENTITY) + b"\x03" + matrix(w_column=(0, 0, 0, 2.0)) + bytes(4)
        with self.assertRaises(pp.FormatError):
            pp.parse_transform(bad)
        nan = header(pp.TYPE_ENTITY) + b"\x03" + matrix(translation=(math.nan, 0, 0)) + bytes(4)
        with self.assertRaises(pp.FormatError):
            pp.parse_transform(nan)
        with self.assertRaises(pp.FormatError):
            pp.parse_transform(header(pp.TYPE_ENTITY) + b"\x07" + matrix() + bytes(4))

    def test_truncated_payloads(self):
        full = entity_flag3()
        for cut in (13, 40, 78, 80):
            with self.assertRaises(pp.FormatError, msg=f"cut {cut}"):
                pp.parse_transform(full[:cut])
        with self.assertRaises(pp.FormatError):
            pp.parse_transform(entity_flag0()[:150])

    def test_entity_needs_exactly_one_bbox_block(self):
        payload = entity_flag3()
        with self.assertRaises(pp.FormatError):
            pp.parse_placed(payload[:-len(bbox_tail((0, 0, 0), (1, 1, 1)))], pp.TYPE_ENTITY)
        with self.assertRaises(pp.FormatError):
            pp.parse_placed(payload + bbox_tail((0, 0, 0), (1, 1, 1)), pp.TYPE_ENTITY)

    def test_group_bbox_not_attributed(self):
        group = header(pp.TYPE_GROUP) + entity_flag3()[13:]
        placed = pp.parse_placed(group + bbox_tail((0, 0, 0), (1, 1, 1)), pp.TYPE_GROUP)
        self.assertIsNone(placed["bbox_local"])
        self.assertEqual(placed["bbox_blocks"], 2)


class References(unittest.TestCase):
    def test_tag_anchored_references(self):
        selector, mesh = 0x00000015470AA6D7, 0x000000102D232F68
        body = (struct.pack("<Q", 1) + pp.TAG_VISUAL + b"\x01\x00\x01\x00" + struct.pack("<Q", selector)
                + struct.pack("<Q", 2) + pp.TAG_MESH_INSTANCE + b"\x01" + struct.pack("<Q", mesh)
                + struct.pack("<Q", 3) + pp.TAG_MESH_INSTANCE + b"\x00" + struct.pack("<Q", 99))
        placed = pp.parse_placed(entity_flag3(body=body), pp.TYPE_ENTITY)
        self.assertEqual(placed["visual_refs"], [selector])
        self.assertEqual(placed["mesh_refs"], [mesh])   # the u8 0 record is not a reference

    def test_tag_at_payload_end_is_ignored(self):
        visuals, meshes = pp.tag_references(bytes(8) + pp.TAG_VISUAL + b"\x01\x00")
        self.assertEqual((visuals, meshes), ([], []))


class Geometry(unittest.TestCase):
    def test_world_aabb_independent_expectation(self):
        # yaw 90: local +x maps to world +y (row 0 = (0, 1, 0)), local +y to world -x.
        m = struct.unpack("<16f", matrix(90.0, (100.0, 200.0, 5.0)))
        lo, hi = pp.world_aabb(m, ((-1.0, -0.25, 0.0), (1.0, 0.25, 1.5)))
        expect_lo, expect_hi = (99.75, 199.0, 5.0), (100.25, 201.0, 6.5)
        for a, b in zip(lo + hi, expect_lo + expect_hi):
            self.assertAlmostEqual(a, b, places=4)

    def test_name_stem(self):
        self.assertEqual(pp.name_stem("FRV_Wall_WL_09_042"), "FRV_Wall_WL_09")
        self.assertEqual(pp.name_stem("GEN_ALL_Beam_2m_01"), "GEN_ALL_Beam_2m_01")
        self.assertEqual(pp.name_stem(""), "")

    def test_kit_family(self):
        self.assertEqual(pp.kit_family("GEN_ALL_AnchorClimbing_02"), "anchor-climbing")
        self.assertEqual(pp.kit_family("GEN_Store_Ledge11_3m"), "store-ledge")
        self.assertIsNone(pp.kit_family("GEN_ALL_BushSmall_02"))


if __name__ == "__main__":
    unittest.main()
