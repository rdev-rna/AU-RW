"""Synthetic-stream tests for tools/atomgraph_stream.py (ACU-RE-0066). No game data is read."""
from __future__ import annotations

import struct
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import atomgraph_stream as S  # noqa: E402


def hdr(cls: str, oid: int = 0) -> bytes:
    return struct.pack("<QI", oid, S.crc(cls))


def u32(*v: int) -> bytes:
    return struct.pack(f"<{len(v)}I", *v)


def node_id(n: int) -> bytes:
    return hdr("AtomNodeID") + bytes([n]) * 16


def state_node(transitions: bytes = u32(0)) -> bytes:
    """AtomStateNode body (0x14010C100) with the given transition-pointer array."""
    return (transitions + u32(0) + node_id(1) + u32(0) + u32(0) + u32(0) + u32(0) + u32(0) + u32(0))


def expression() -> bytes:
    return struct.pack("<Q", 0) + u32(0) + u32(0)


def transition(target: int, blend: float) -> bytes:
    """Pointer tag 0 + AtomStateTransitionTarget (0x14010C4E0)."""
    body = (b"\x00" + hdr("AtomConditionExpression") + expression() + struct.pack("<HB", target, 1)
            + u32(1, 2, 3, 0, 1) + u32(7) + struct.pack("<f", blend) + b"\x01\x00\x01" + node_id(2))
    return b"\x00" + hdr("AtomStateTransitionTarget", 0x55) + body


def graph(f16: bytes) -> bytes:
    return (b"\x01" + hdr("AtomGraph", 0x13D5BC4D8) + f16 + b"\x03" + b"\x02" + struct.pack("<Q", 0x55)
            + u32(0) + hdr("AtomStateDescriptor") + u32(0) + u32(4, 5) + b"\x03" + u32(1, 2, 3)
            + hdr("ExternalIdMap") + u32(1, 9) + u32(0))


class StreamTests(unittest.TestCase):
    def test_graph_with_machine_consumes_exactly(self):
        machine = (b"\x00" + hdr("AtomStateMachineNode", 0x77) + state_node(u32(1) + transition(3, 0.25))
                   + u32(0) + u32(0) + u32(0))
        b = graph(machine)
        root, r = S.parse(b)
        self.assertEqual(r.p, len(b))
        self.assertEqual(root["id"], "000000013d5bc4d8")
        m = root["f16"]
        self.assertEqual(m["class"], "AtomStateMachineNode")
        t = m["f16"][0]
        self.assertEqual((t["f36"], t["f32"], t["f32.first"]), (3, 0.25, 7))
        self.assertEqual(root["f24"], None)
        self.assertEqual(root["f32"], {"ref": "0000000000000055"})
        self.assertEqual(root["f116"]["f0"], [9])
        self.assertIs(r.ids[0x55], t)

    def test_unknown_class_fails_clearly(self):
        b = graph(b"\x00" + struct.pack("<QI", 0, 0x12345678))
        with self.assertRaisesRegex(S.StreamError, "no loader for class 12345678"):
            S.parse(b)

    def test_truncated_stream_fails(self):
        b = graph(b"\x03")
        with self.assertRaisesRegex(S.StreamError, "past end"):
            S.parse(b[:-3])

    def test_oversized_count_is_rejected(self):
        b = b"\x01" + hdr("ExternalIdMap") + u32(1 << 30)
        with self.assertRaisesRegex(S.StreamError, "exceeds"):
            S.parse(b)

    def test_string_reads_terminator(self):
        r = S.Reader(u32(3) + b"abc\x00" + u32(0) + u32(1, 2) + hdr("AtomDataContainerWrapper") + bytes(40))
        obj = {}
        r.run("AtomRTCPDescriptor", obj, 0)
        self.assertEqual((obj["f0"], obj["f8"], obj["f16"]), ("abc", "", 1))
        self.assertEqual(r.p, len(r.b))


if __name__ == "__main__":
    unittest.main()
