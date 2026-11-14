"""Synthetic-byte tests for tools/world_collision.py (ACU-RE-0050). No game data is read."""
from __future__ import annotations

import struct
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import world_collision as wc  # noqa: E402

IDENTITY = (1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)


def translate(x, y, z):
    return IDENTITY[:12] + (x, y, z, 1.0)


def header(self_id, type_id):
    return struct.pack("<BQI", 1, self_id, type_id)


def aabb(lo, hi):
    return struct.pack("<8f", *lo, 1.0, *hi, 1.0)


def mesh_shape(vertices, indices, tri_materials=b"", materials=(0x738D22E48,), havok_body=b"\0" * 24):
    havok = wc.HAVOK_MAGIC + havok_body
    out = header(0x34B4C3A4A3, wc.TYPE_MESHSHAPE) + bytes(12)
    out += struct.pack("<II", 0x390F5602, len(havok)) + havok
    out += bytes([0 if tri_materials else 1, 0])
    out += struct.pack("<I", len(vertices)) + b"".join(struct.pack("<3f", *v) for v in vertices)
    out += struct.pack("<I", len(indices)) + struct.pack(f"<{len(indices)}H", *indices)
    out += struct.pack("<I", len(tri_materials)) + bytes(tri_materials)
    out += struct.pack("<I", len(materials))
    for material in materials:
        record = bytes(8) + struct.pack("<I", 0x8F8039AF) + bytes([1, 0]) + struct.pack("<Q", material)
        out += record + bytes(wc.MATERIAL_RECORD - len(record))
    lo = [min(v[i] for v in vertices) for i in range(3)]
    hi = [max(v[i] for v in vertices) for i in range(3)]
    return out + aabb(lo, hi)


def body(shape_id, matrix, kind=1, records=0, entities=(0x2457777310,)):
    out = header(0x3F575A63A1, wc.TYPE_BODY) + bytes([1, kind]) + struct.pack("<Q", shape_id)
    out += bytes(12)  # 23..34
    out += bytes([records]) + bytes(11)  # 35..46
    out += bytes(wc.BODY_RECORD * records)
    out += struct.pack("<I", wc.TAG_BODY_RECORD) + bytes([1]) + bytes(14)
    out += struct.pack("<16f", *matrix)
    out += struct.pack("<I", len(entities)) + b"".join(bytes([0]) + struct.pack("<Q", e) for e in entities)
    return out


def reflist(children):
    out = header(0x245ABCD54D, wc.TYPE_REFLIST) + struct.pack("<II", 0, len(children))
    out += b"".join(bytes([1, 1]) + struct.pack("<Q", c) for c, _ in children)
    out += struct.pack("<I", len(children)) + b"".join(struct.pack("<16f", *m) for _, m in children)
    out += struct.pack("<I", len(children)) + bytes(wc.REFLIST_RECORD * len(children))
    return out + aabb((0, 0, 0), (1, 1, 1))


def entity_with_rigid_body(world, shape_id, inertia=IDENTITY):
    out = header(0x10CBF44427, wc.TYPE_ENTITY) + bytes([3]) + struct.pack("<16f", *world)
    out += bytes(40)  # other components
    out += struct.pack("<Q", 0x10CBF44446) + wc.TAG_RIGID_BODY_BYTES + bytes(8)
    out += struct.pack("<I", wc.TAG_BODY_RECORD) + bytes([1]) + bytes(14)
    out += bytes([1, 1]) + struct.pack("<Q", shape_id)
    out += struct.pack("<4f", 10.0, 0.1, 0.1, 0.0) + bytes(1)
    out += struct.pack("<16f", *inertia) + bytes(16)
    return out


QUAD = [(0.0, 0.0, 1.0), (2.0, 0.0, 1.0), (2.0, 2.0, 3.0), (0.0, 2.0, 3.0)]
QUAD_INDICES = (0, 1, 2, 0, 2, 3)


class MeshShapeTests(unittest.TestCase):
    def test_parses_vertices_triangles_and_materials(self):
        s = wc.parse_mesh_shape(mesh_shape(QUAD, QUAD_INDICES, bytes([0, 1]), (0x48, 0x58)))
        self.assertEqual(len(s["vertices"]), 4)
        self.assertEqual(s["triangles"], [(0, 1, 2), (0, 2, 3)])
        self.assertEqual(s["tri_materials"], bytes([0, 1]))
        self.assertEqual(s["materials"], [0x48, 0x58])
        self.assertEqual(s["aabb"], ((0.0, 0.0, 1.0), (2.0, 2.0, 3.0)))

    def test_single_material_without_triangle_array(self):
        s = wc.parse_mesh_shape(mesh_shape(QUAD, QUAD_INDICES))
        self.assertEqual(s["tri_materials"], b"")
        self.assertEqual(s["material_flag"], 1)

    def test_rejects_missing_havok_magic(self):
        payload = bytearray(mesh_shape(QUAD, QUAD_INDICES))
        payload[33] ^= 0xFF
        with self.assertRaisesRegex(wc.FormatError, "magic"):
            wc.parse_mesh_shape(bytes(payload))

    def test_rejects_index_out_of_range(self):
        with self.assertRaisesRegex(wc.FormatError, "out of range"):
            wc.parse_mesh_shape(mesh_shape(QUAD, (0, 1, 9)))

    def test_rejects_material_count_mismatch(self):
        with self.assertRaisesRegex(wc.FormatError, "triangle materials"):
            wc.parse_mesh_shape(mesh_shape(QUAD, QUAD_INDICES, bytes([0])))

    def test_rejects_trailing_and_truncated_bytes(self):
        good = mesh_shape(QUAD, QUAD_INDICES)
        with self.assertRaisesRegex(wc.FormatError, "trailing"):
            wc.parse_mesh_shape(good + b"\0")
        with self.assertRaises(wc.FormatError):
            wc.parse_mesh_shape(good[:-40])

    def test_rejects_vertex_count_over_limit(self):
        payload = bytearray(mesh_shape(QUAD, QUAD_INDICES))
        at = 33 + struct.unpack_from("<I", payload, 29)[0] + 2
        struct.pack_into("<I", payload, at, wc.MAX_VERTICES + 1)
        with self.assertRaisesRegex(wc.FormatError, "over limit"):
            wc.parse_mesh_shape(bytes(payload))

    def test_rejects_wrong_type(self):
        payload = bytearray(mesh_shape(QUAD, QUAD_INDICES))
        struct.pack_into("<I", payload, 9, wc.TYPE_BODY)
        with self.assertRaisesRegex(wc.FormatError, "expected b22b3e61"):
            wc.parse_mesh_shape(bytes(payload))


class BodyTests(unittest.TestCase):
    def test_body_matrix_follows_records(self):
        for records in (0, 1, 2):
            b = wc.parse_body(body(0x99, translate(80, -336, 0), records=records))
            self.assertEqual(b["shape"], 0x99)
            self.assertEqual(b["records"], records)
            self.assertEqual(b["matrix"][12:15], (80.0, -336.0, 0.0))

    def test_body_entity_tail(self):
        b = wc.parse_body(body(0x99, IDENTITY, entities=(0x11, 0x22)))
        self.assertEqual(b["entities"], [0x11, 0x22])
        self.assertEqual(len(body(0x99, IDENTITY)), 143)  # the common Versailles length
        with self.assertRaisesRegex(wc.FormatError, "trailing"):
            wc.parse_body(body(0x99, IDENTITY) + b"\0")

    def test_body_rejects_missing_filter_tag_and_non_affine(self):
        payload = bytearray(body(0x99, IDENTITY))
        struct.pack_into("<I", payload, 47, 0)
        with self.assertRaisesRegex(wc.FormatError, "43ef99c2"):
            wc.parse_body(bytes(payload))
        bad = list(IDENTITY)
        bad[3] = 0.5
        with self.assertRaisesRegex(wc.FormatError, "column 3"):
            wc.parse_body(body(0x99, bad))

    def test_reference_list(self):
        r = wc.parse_reflist(reflist([(0x11, translate(1, 2, 3)), (0x22, IDENTITY)]))
        self.assertEqual([c for c, _ in r["children"]], [0x11, 0x22])
        self.assertEqual(r["children"][0][1][12:15], (1.0, 2.0, 3.0))
        with self.assertRaisesRegex(wc.FormatError, "trailing"):
            wc.parse_reflist(reflist([(0x11, IDENTITY)]) + b"\0")

    def test_entity_rigid_body_uses_entity_world_not_inertia_block(self):
        inertia = (4.0, 0, 0, 0, 0, 4.0, 0, 0, 0, 0, 4.0, 0, 0, 0, 0, 1.0)
        found = list(wc.parse_rigid_bodies(entity_with_rigid_body(translate(5, 6, 7), 0x77, inertia)))
        self.assertEqual(len(found), 1)
        rb = found[0]
        self.assertNotIn("unsupported", rb)
        self.assertEqual(rb["shape"], 0x77)
        self.assertEqual(rb["owner"], 9)
        self.assertFalse(rb["inline"])
        self.assertEqual(rb["world"][12:15], (5.0, 6.0, 7.0))
        self.assertEqual(rb["local"][0], 4.0)

    def test_entity_rigid_body_without_filter_is_reported(self):
        payload = bytearray(entity_with_rigid_body(IDENTITY, 0x77))
        at = payload.find(wc.TAG_RIGID_BODY_BYTES)
        struct.pack_into("<I", payload, at + 12, 0)
        found = list(wc.parse_rigid_bodies(bytes(payload)))
        self.assertIn("unsupported", found[0])


class GeometryTests(unittest.TestCase):
    def test_vertical_hit_interpolates_height(self):
        tri = ((0.0, 0.0, 1.0), (2.0, 0.0, 1.0), (2.0, 2.0, 3.0))
        z, nz = wc.vertical_hits(tri, 1.5, 0.5)
        self.assertAlmostEqual(z, 1.5)
        self.assertAlmostEqual(nz, 1 / 2 ** 0.5)
        self.assertIsNone(wc.vertical_hits(tri, 0.1, 1.9))  # outside
        wall = ((0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (1.0, 0.0, 1.0))
        self.assertIsNone(wc.vertical_hits(wall, 0.5, 0.0))  # vertical in plan view

    def test_compose_child_then_parent(self):
        rot = (0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 10.0, 0.0, 0.0, 1.0)
        m = wc.compose(translate(1, 0, 0), rot)  # translate by +x, then rotate 90 deg, then +10 x
        self.assertEqual(tuple(round(v, 6) for v in wc.apply(m, (0, 0, 0))), (10.0, 1.0, 0.0))

    def test_support_picks_highest_surface_within_step(self):
        hits = [dict(z=5.0), dict(z=0.4), dict(z=0.0)]
        self.assertEqual(wc.support(hits, 0.0, 0.5)["z"], 0.4)
        self.assertEqual(wc.support(hits, 4.8, 0.5)["z"], 5.0)
        self.assertIsNone(wc.support(hits, -1.0, 0.5))


class DumpTests(unittest.TestCase):
    def test_dump_round_trip_and_truncation(self):
        payload = b"abc"
        record = struct.pack("<IIQIH", 7, 2, 0x1234, 0xB22B3E61, 1) + b"n" + struct.pack("<I", 3) + payload
        rows = list(wc.read_dump(wc.DUMP_MAGIC + record))
        self.assertEqual(rows, [(7, 2, 0x1234, 0xB22B3E61, "n", payload)])
        with self.assertRaisesRegex(wc.FormatError, "truncated"):
            list(wc.read_dump(wc.DUMP_MAGIC + record[:-1]))
        with self.assertRaisesRegex(wc.FormatError, "ACUTD001"):
            list(wc.read_dump(b"XXXXXXXX"))


if __name__ == "__main__":
    unittest.main()
