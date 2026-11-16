"""Synthetic tests for tools/net_message_handlers.py (no game data needed)."""
from __future__ import annotations

import struct
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import net_message_handlers as nmh  # noqa: E402
import net_protocol_atlas as atlas  # noqa: E402

B = atlas.IMAGE_BASE
TABLE, BUILDER, H1, H2, H3, NOOP = B + 0x1000, B + 0x1200, B + 0x1300, B + 0x1340, B + 0x1380, B + 0x13C0
STR_A, STR_B, STR_LONG = B + 0x3000, B + 0x3020, B + 0x3050


class FakeImage:
    """Bytes from the image base, a .text section at 0x1000-0x3000 and a fixed function list."""

    def __init__(self, patches: dict[int, bytes], funcs: list[tuple[int, int]]):
        b = bytearray(b"\xCC" * 0x4000)
        b[0x3000:0x4000] = b"\0" * 0x1000
        for va, data in patches.items():
            b[va - B:va - B + len(data)] = data
        self.b = bytes(b)
        self.funcs = funcs

    def sec(self, name):
        return B + 0x1000, B + 0x3000

    def func_of(self, va):
        for f in self.funcs:
            if f[0] <= va < f[1]:
                return f
        return None


def lea(at: int, prefix: bytes, target: int) -> bytes:
    return prefix + struct.pack("<i", target - (at + 7))


def table_code() -> bytes:
    """lea rdx,"Alpha"; lea r8,builder; lea r9,H1; lea rdx,"Beta"; lea r8,0x1401525E0-like common; lea r8,H2; ret"""
    out, at = b"", TABLE
    for prefix, t in [(b"\x48\x8D\x15", STR_A), (b"\x4C\x8D\x05", BUILDER), (b"\x4C\x8D\x0D", H1),
                      (b"\x48\x8D\x15", STR_B), (b"\x4C\x8D\x05", TABLE + 0x40), (b"\x4C\x8D\x05", H2)]:
        out += lea(at, prefix, t)
        at += 7
    return out + b"\xC3"


MASTER_TEST = (b"\x8B\x81\xD4\x00\x00\x00"   # mov eax, [rcx+0xd4]
               b"\x25\x00\x00\x0C\x00"       # and eax, 0xC0000
               b"\x3D\x00\x00\x04\x00"       # cmp eax, 0x40000
               b"\xC3")
REPLICA_TEST = MASTER_TEST[:11] + b"\x3D\x00\x00\x08\x00\xC3"   # cmp eax, 0x80000


def image() -> FakeImage:
    call_h3 = b"\xE8" + struct.pack("<i", H3 - (H2 + 5)) + b"\xC3"
    return FakeImage({
        TABLE: table_code(),
        BUILDER: b"\x90" * 0x20 + b"\xC3",
        H1: MASTER_TEST, H2: call_h3, H3: REPLICA_TEST, NOOP: b"\xC3",
        STR_A: b"AlphaMSG_NetProtocol_ID\0",
        STR_B: b"BetaMSG_NetProtocol_ID_UNICAST\0",
        STR_LONG: b"ChaseStartMSG_NetProtocol_ID\0",
    }, [(TABLE, TABLE + 0x100), (BUILDER, BUILDER + 0x40), (H1, H1 + 0x20), (H2, H2 + 0x10), (H3, H3 + 0x20),
        (NOOP, NOOP + 1)])


@unittest.skipIf(nmh.capstone is None, "capstone not installed")
class MapperTests(unittest.TestCase):
    def setUp(self):
        self.img = image()
        self.mp = nmh.Mapper(self.img)

    def test_id_strings_include_shared_tails(self):
        s = self.mp.id_strings()
        self.assertEqual(s[STR_A], "AlphaMSG_NetProtocol_ID")
        self.assertEqual(s[STR_LONG], "ChaseStartMSG_NetProtocol_ID")
        self.assertEqual(s[STR_LONG + 5], "StartMSG_NetProtocol_ID")   # linker-shared tail
        self.assertNotIn(STR_LONG + 8, s)                               # "rtMSG" is shorter than 3 chars + MSG

    def test_table_pairs_skip_own_code(self):
        pairs = self.mp.table_pairs(TABLE, self.mp.id_strings())
        self.assertEqual(pairs["AlphaMSG_NetProtocol_ID"], [BUILDER, H1])
        self.assertEqual(pairs["BetaMSG_NetProtocol_ID_UNICAST"], [H2])  # TABLE+0x40 is inside the table

    def test_role_guard_direct_and_one_call_deep(self):
        self.assertTrue(self.mp.role_guard(H1)["master"])
        self.assertFalse(self.mp.role_guard(H1)["replica"])
        g = self.mp.role_guard(H2)
        self.assertTrue(g["replica"])
        self.assertFalse(g["master"])
        self.assertFalse(self.mp.role_guard(H2, depth=0)["replica"])

    def test_noop(self):
        self.assertTrue(self.mp.is_noop(NOOP))
        self.assertFalse(self.mp.is_noop(H1))

    def test_build_by_builder_and_unique_short_name(self):
        payloads = {"messages": [
            {"name": "FamA_AlphaMSG_NetProtocol", "block": atlas.hx(BUILDER + 0x10), "system": "x"},
            {"name": "FamA_BetaMSG_NetProtocol", "block": atlas.hx(B + 0x2F00), "system": "x"},
            {"name": "FamB_GammaMSG_NetProtocol", "block": atlas.hx(B + 0x2F10), "system": "y"},
        ]}
        res = nmh.build(self.img, {"static_rfcs": []}, payloads)
        by = {m["name"]: m for m in res["messages"]}
        self.assertEqual(by["FamA_AlphaMSG_NetProtocol"]["handler"], atlas.hx(H1))
        self.assertEqual(by["FamA_AlphaMSG_NetProtocol"]["match"], "builder")
        self.assertEqual(by["FamA_BetaMSG_NetProtocol"]["handler"], atlas.hx(H2))
        self.assertEqual(by["FamA_BetaMSG_NetProtocol"]["match"], "unique-short-name")
        self.assertIsNone(by["FamB_GammaMSG_NetProtocol"]["handler"])
        self.assertEqual(res["missing"], ["FamB_GammaMSG_NetProtocol"])
        self.assertEqual(res["summary"]["role_guard"], {"master": 1, "replica": 1})

    def test_static_rfc_execute_slots(self):
        payloads = {"messages": [{"name": "S5_RunMSG_NetProtocol", "block": atlas.hx(B + 0x2F00)}]}
        rfcs = [{"message": "zenitar_S5_RunMSG_NetProtocol_ID", "register": atlas.hx(TABLE),
                 "callbacks": [atlas.hx(B + 0x1500), "0x1401525E0", atlas.hx(B + 0x1510), "0x14022EC80",
                               atlas.hx(NOOP)]}]
        res = nmh.build(self.img, {"static_rfcs": rfcs}, payloads)
        m = res["messages"][0]
        self.assertEqual((m["handler"], m["match"], m["noop"]), (atlas.hx(NOOP), "static-rfc", True))


if __name__ == "__main__":
    unittest.main()
