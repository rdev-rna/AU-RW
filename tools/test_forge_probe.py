"""Authored structural fixtures; these do not establish original-game parity."""
import io
import struct
import unittest
from forge_probe import tables


def fixture():
    b = bytearray(0x500)
    b[:9] = b'scimitar\0'
    struct.pack_into('<IQ', b, 9, 27, 0x41a)
    struct.pack_into('<I', b, 0x41a, 1)
    struct.pack_into('<Q', b, 0x41a+36, 0x446)
    struct.pack_into('<I', b, 0x446, 1)
    struct.pack_into('<QQ', b, 0x44e, 0x476, 0xffffffffffffffff)
    struct.pack_into('<QQI', b, 0x476, 0x4fc, 0xfedcba9876543210, 4)
    b[0x4fc:] = b'abcd'
    return b


class Boundaries(unittest.TestCase):
    def parse(self, b):
        return tables(io.BytesIO(b), len(b))

    def test_known_record(self):
        report, entries = self.parse(fixture())
        self.assertTrue(report['count_matches'])
        self.assertEqual(entries, [dict(index=0, record_offset=0x476, offset=0x4fc,
            file_id='fedcba9876543210', stored_length=4)])

    def test_invalid_boundaries(self):
        for position, fmt, value in [(9, '<I', 28), (13, '<Q', 2**64-1),
            (0x446, '<I', 1_000_001), (0x44e, '<Q', 0x4ff),
            (0x476, '<Q', 0x4fe), (0x456, '<Q', 0x446)]:
            with self.subTest(position=position):
                b = fixture()
                struct.pack_into(fmt, b, position, value)
                with self.assertRaises(ValueError):
                    self.parse(b)

    def test_truncated_header(self):
        with self.assertRaises(ValueError):
            self.parse(fixture()[:20])


if __name__ == '__main__':
    unittest.main()
