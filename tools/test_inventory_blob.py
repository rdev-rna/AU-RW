"""Synthetic tests for tools/coop_lan/inventory_blob.py (no game data needed).
Streams are hand-assembled from the LZO1X format: a first byte > 17 is a literal run of (byte - 17); 0x20|n is an
M3 match of n + 2 bytes with a little-endian (distance - 1) << 2 field; 0x11 0x00 0x00 ends the stream."""
from __future__ import annotations

import struct
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "coop_lan"))
import inventory_blob as ib  # noqa: E402

STREAM = bytes([17 + 4]) + b"abcd" + bytes([0x20 | 3, 3 << 2, 0]) + bytes([0x11, 0, 0])
EXPECTED = b"abcd" + b"abcda"  # 5-byte copy from distance 4 overlaps its own output


def blob(stream: bytes, decoded: bytes, checksum=None) -> bytes:
    check = ib.adler0(stream) if checksum is None else checksum
    fr = struct.pack("<QHBI", ib.MAGIC, 1, 0, 0x8000) + bytes([1]) + struct.pack("<III", len(stream), len(decoded), check)
    return struct.pack("<I", len(decoded)) + fr + stream


class Tests(unittest.TestCase):
    def test_lzo_literal_and_overlapping_match(self):
        self.assertEqual(ib.lzo1x(STREAM, len(EXPECTED)), EXPECTED)

    def test_adler_seed_zero(self):
        self.assertEqual(ib.adler0(b"a"), (97 << 16) | 97)  # zlib's seed-1 variant would give 0x00620062

    def test_blob_round_trip_and_items(self):
        self.assertEqual(ib.decode(blob(STREAM, EXPECTED)), EXPECTED)
        data = b"\0" * 3 + struct.pack("<Q", 0x00000011A8F25BF1) + b"\0" * 5
        self.assertEqual([x["file_id"] for x in ib.items(data, FakeDb())], ["00000011a8f25bf1"])

    def test_bad_inputs_fail(self):
        with self.assertRaises(ib.BlobError):
            ib.decode(blob(STREAM, EXPECTED, checksum=1))
        with self.assertRaises(ib.BlobError):
            ib.lzo1x(STREAM, len(EXPECTED) - 1)          # output bound
        with self.assertRaises(ib.BlobError):
            ib.lzo1x(STREAM[:-2], len(EXPECTED))         # truncated end marker
        with self.assertRaises(ib.BlobError):
            ib.lzo1x(bytes([0x20 | 3, 9 << 2, 0, 0x11, 0, 0]), 5)  # match before any output
        with self.assertRaises(ib.BlobError):
            ib.decode(b"\x09\0\0\0" + b"\0" * 20)        # wrong magic


class FakeDb:
    def execute(self, sql, args):
        class R:
            def fetchone(_):
                return ("ACU_AvatarGear_Forearms_02", "InventoryItemSettings") if args[0] == "00000011a8f25bf1" else None
        return R()


if __name__ == "__main__":
    unittest.main()
