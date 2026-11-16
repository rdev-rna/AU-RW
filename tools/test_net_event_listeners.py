"""Synthetic tests for tools/net_event_listeners.py (no game data needed)."""
from __future__ import annotations

import struct
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import net_event_listeners as nel  # noqa: E402
import net_protocol_atlas as atlas  # noqa: E402

B = atlas.IMAGE_BASE
HANDLER, ACC, REF, ACT, WRAP = B + 0x1000, B + 0x1100, B + 0x1140, B + 0x1180, B + 0x11C0
TABLE1, TABLE2 = B + 0x3000, B + 0x3200
EVENT, OTHER = 0x11223344, 0x55667788


class FakeImage:
    """Bytes from the image base: .text 0x1000-0x3000 (int3 filled), .data 0x3000-0x4000 (zeroed)."""

    def __init__(self, patches: dict[int, bytes]):
        b = bytearray(bytes([0xCC]) * 0x3000 + bytes(0x1000))
        for va, data in patches.items():
            b[va - B:va - B + len(data)] = data
        self.b = bytes(b)

    def sec(self, name):
        return {".text": (B + 0x1000, B + 0x3000), ".data": (B + 0x3000, B + 0x4000)}[name]


def call(at: int, target: int, opcode: int = 0xE8) -> bytes:
    return bytes([opcode]) + struct.pack("<i", target - (at + 5))


def entry(h: int, idx: int, fn: int) -> bytes:
    return struct.pack("<IIQQQ", h, idx, 0, fn, 0)


MOV_EDX = bytes([0xBA]) + struct.pack("<I", EVENT)               # mov edx, EVENT
ACCEPT = bytes([0xC6, 0x02, 0x01, 0xC3])                         # mov byte [rdx], 1; ret
REFUSE = bytes([0x48, 0x8B, 0x44, 0x24, 0x40,                    # mov rax, [rsp+0x40]
                0xC6, 0x00, 0x00, 0xC3])                         # mov byte [rax], 0; ret
JUNK = struct.pack("<IIQQQ", 1, 1, 1, 1, 1)                      # not an entry (last qword != 0)


def image() -> FakeImage:
    handler = MOV_EDX + call(HANDLER + 5, nel.FANOUT) + bytes([0xC3])
    wrap = call(WRAP, HANDLER, 0xE9)                              # jmp handler (hash one call below)
    act = call(ACT, ACC) + bytes([0xC3])
    t1 = entry(OTHER, 0, ACC) + entry(EVENT, 1, REF) + entry(OTHER, 2, nel.NULLSUB) + JUNK
    t2 = entry(EVENT, 0, ACT) + JUNK
    return FakeImage({HANDLER: handler, WRAP: wrap, ACC: ACCEPT, REF: REFUSE, ACT: act, TABLE1: t1, TABLE2: t2})


@unittest.skipIf(nel.capstone is None, "capstone not installed")
class ListenerTests(unittest.TestCase):
    def setUp(self):
        self.ls = nel.Listeners(image())

    def test_hash_direct_and_one_call_below(self):
        self.assertEqual(self.ls.hashes(HANDLER), [EVENT])
        self.assertEqual(self.ls.hashes(WRAP), [EVENT])
        self.assertEqual(self.ls.hashes(WRAP, depth=0), [])

    def test_tables_found_from_any_entry(self):
        self.assertEqual(self.ls.tables({EVENT}), {TABLE1: 3, TABLE2: 1})

    def test_kinds(self):
        self.assertEqual(self.ls.kind(ACC), "accept-only")
        self.assertEqual(self.ls.kind(REF), "refuse")
        self.assertEqual(self.ls.kind(ACT), "action")
        self.assertEqual(self.ls.kind(nel.NULLSUB), "ignore")

    def test_build(self):
        res = nel.build(image(), {"messages": [
            {"name": "Fam_XMSG_NetProtocol", "handler": atlas.hx(WRAP), "dispatch": "event-fanout"},
            {"name": "Fam_YMSG_NetProtocol", "handler": atlas.hx(ACC), "dispatch": "direct"}]})
        self.assertEqual(len(res["messages"]), 1)
        ev = res["messages"][0]["events"][0]
        self.assertEqual((ev["hash"], ev["tables"]), ("11223344", 2))
        self.assertEqual(ev["kinds"], {"refuse": 1, "action": 1})
        self.assertEqual(ev["actions"], {atlas.hx(ACT): 1})
        self.assertEqual(res["summary"]["messages_without_listener"], [])


if __name__ == "__main__":
    unittest.main()
