"""Synthetic tests for tools/net_protocol_atlas.py (no game data needed)."""
from __future__ import annotations

import struct
import sys
import unittest
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import net_protocol_atlas as atlas  # noqa: E402

B = atlas.IMAGE_BASE


class FakeImage(atlas.Image):
    """Image with hand-placed sections; skips PE parsing."""

    def __init__(self, size: int, secs: dict[str, tuple[int, int]], funcs=()):
        self.b = bytearray(size)
        self.secs = secs
        self.funcs = sorted(funcs)
        self.starts = [f[0] for f in self.funcs]

    def put_q(self, va: int, v: int) -> None:
        struct.pack_into("<Q", self.b, va - B, v)

    def put_d(self, va: int, v: int) -> None:
        struct.pack_into("<I", self.b, va - B, v)

    def put_s(self, va: int, s: str) -> None:
        self.b[va - B:va - B + len(s) + 1] = s.encode() + b"\0"


class CrcTests(unittest.TestCase):
    def test_matches_finding_values(self):
        # Independent expected values recorded in ACU-RE-0072.
        self.assertEqual(atlas.crc("storm::DataContainer"), 0xA0217162)
        s = "zenitar_NetFightJIPManager_RFC_JIPFailedMSG_NetProtocol_ID"
        self.assertEqual(atlas.crc(s), 0xB29E970A)
        self.assertEqual(atlas.crc(s + "_UNICAST"), 0x0BD55A54)


class WireIdTests(unittest.TestCase):
    def test_static_message_matches_registrar_keys(self):
        # ACU-RE-0072 registrar keys for the JIPFailed message.
        self.assertEqual(atlas.wire_ids("NetFightJIPManager_RFC_JIPFailedMSG_NetProtocol"),
                         (0xB29E970A, 0x0BD55A54))

    def test_object_scoped_message_matches_encoder_constants(self):
        # ACU-RE-0075: constants written by the NetHumanIntention encoder at 0x140721F7A/0x140721F83.
        self.assertEqual(atlas.wire_ids("NetHumanIntention_AssassinateTargetMSG_NetProtocol"),
                         (0xDC303C6B, 0x572CC457))


class ImageTests(unittest.TestCase):
    def test_rejects_non_pe(self):
        with self.assertRaises(ValueError):
            atlas.Image(b"\0" * 0x400)

    def test_rejects_bad_pe_signature(self):
        data = bytearray(0x400)
        data[:2] = b"MZ"
        struct.pack_into("<I", data, 0x3C, 0x80)
        with self.assertRaises(ValueError):
            atlas.Image(bytes(data))

    def test_q_out_of_range(self):
        img = FakeImage(0x100, {})
        with self.assertRaises(IndexError):
            img.q(B + 0xFC)


class TypeRecordTests(unittest.TestCase):
    def setUp(self):
        self.img = FakeImage(0x2000, {".data": (0x1000, 0x800)})
        self.name_va = B + 0x100
        self.img.put_s(self.name_va, "ChildMSG")
        self.strs = {self.name_va: "ChildMSG"}
        self.names = {zlib.crc32(b"Root"): "Root", zlib.crc32(b"ChildMSG"): "ChildMSG"}

    def _record(self, va: int, hashes: list[int]) -> None:
        self.img.put_q(va, self.name_va)
        self.img.put_d(va + 8, len(hashes))
        for i, h in enumerate(hashes):
            self.img.put_d(va + 0x10 + 16 * i, h)

    def test_accepts_self_consistent_record(self):
        self._record(B + 0x1000, [zlib.crc32(b"Root"), zlib.crc32(b"ChildMSG")])
        recs = atlas.type_records(self.img, self.strs, self.names)
        self.assertEqual(len(recs), 1)
        self.assertEqual(recs[0]["ancestors"], ["Root", "ChildMSG"])

    def test_rejects_wrong_last_hash(self):
        self._record(B + 0x1000, [zlib.crc32(b"Root"), 0x12345678])
        self.assertEqual(atlas.type_records(self.img, self.strs, self.names), [])

    def test_rejects_implausible_depth(self):
        self.img.put_q(B + 0x1000, self.name_va)
        self.img.put_d(B + 0x1008, atlas.MAX_ANCESTORS + 1)
        self.assertEqual(atlas.type_records(self.img, self.strs, self.names), [])

    def test_unknown_ancestor_is_labelled_not_guessed(self):
        self._record(B + 0x1000, [0xDEADBEEF, zlib.crc32(b"ChildMSG")])
        recs = atlas.type_records(self.img, self.strs, self.names)
        self.assertEqual(recs[0]["ancestors"][0], "crc:DEADBEEF")


class RegistrarListTests(unittest.TestCase):
    def _img(self, size=0x26000000):
        # REGISTRAR_LISTS lives at RVA 0x525FAE8; allocate enough to hold it.
        return FakeImage(size, {})

    def test_cycle_is_detected(self):
        img = self._img()
        head = atlas.REGISTRAR_LISTS + 8 * atlas.REGISTRAR_CATEGORY
        n1, n2 = B + 0x1000, B + 0x1018
        img.put_q(head, n1)
        img.put_q(n1 + 16, n2)
        img.put_q(n2 + 16, n1)
        with self.assertRaises(ValueError):
            atlas.static_rfcs(img, {})

    def test_registrar_without_pdata_fails_clearly(self):
        img = self._img()
        head = atlas.REGISTRAR_LISTS + 8 * atlas.REGISTRAR_CATEGORY
        n1 = B + 0x1000
        img.put_q(head, n1)
        img.put_q(n1, B + 0x2000)  # register fn with no .pdata entry
        with self.assertRaises(ValueError):
            atlas.static_rfcs(img, {})

    def test_keys_resolved_inside_function_bounds_only(self):
        f1, f2 = (B + 0x2000, B + 0x2010), (B + 0x2010, B + 0x2020)
        img = FakeImage(0x26000000, {}, funcs=[f1, f2])
        head = atlas.REGISTRAR_LISTS + 8 * atlas.REGISTRAR_CATEGORY
        n1 = B + 0x1000
        img.put_q(head, n1)
        img.put_q(n1, f1[0])
        s = "zenitar_A_BMSG_NetProtocol_ID"
        other = "zenitar_C_DMSG_NetProtocol_ID"
        zen = {atlas.crc(s): (s, "broadcast"), atlas.crc(other): (other, "broadcast")}
        # mov dword [rsp+20h], imm32 inside f1; the other key sits in f2.
        img.b[0x2000:0x2008] = b"\xC7\x44\x24\x20" + struct.pack("<I", atlas.crc(s))
        img.b[0x2010:0x2018] = b"\xC7\x44\x24\x20" + struct.pack("<I", atlas.crc(other))
        out = atlas.static_rfcs(img, zen)
        self.assertEqual(out[0]["message"], s)


if __name__ == "__main__":
    unittest.main()
