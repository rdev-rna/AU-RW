"""Synthetic PE boundary controls; no installed game or process required."""
import struct
import unittest
from tools.native_probe.runtime_analysis_view import create_view


def fixture():
    data = bytearray(0x3000)
    data[:2] = b'MZ'
    struct.pack_into('<I', data, 0x3c, 0x80)
    data[0x80:0x84] = b'PE\0\0'
    struct.pack_into('<HH', data, 0x84, 0x8664, 2)
    struct.pack_into('<H', data, 0x94, 240)
    optional = 0x98
    struct.pack_into('<H', data, optional, 0x20b)
    struct.pack_into('<Q', data, optional+24, 0x140000000)
    struct.pack_into('<I', data, optional+36, 0x200)
    struct.pack_into('<I', data, optional+56, len(data))
    struct.pack_into('<I', data, optional+64, 123)
    struct.pack_into('<II', data, optional+144, 0x400, 0x50)
    for i in range(2):
        offset = optional+240+i*40
        struct.pack_into('<4I', data, offset+8, 0x800, (i+1)*0x1000, 0x200, 0x400+i*0x200)
    data[0x1234:0x1238] = b'CODE'
    data[0x2ffc:0x3000] = b'TAIL'
    return data


class ViewTests(unittest.TestCase):
    def test_section_mapping_preserves_payload_and_input(self):
        original = fixture()
        saved = bytes(original)
        view, report = create_view(original, 0x180000000)
        self.assertEqual(bytes(original), saved)
        self.assertEqual(len(view), len(saved))
        self.assertEqual(view[0x1234:0x1238], b'CODE')
        self.assertEqual(view[-4:], b'TAIL')
        for section in report['sections']:
            self.assertEqual(section['analysis_raw_pointer'], section['rva'])
            self.assertEqual(section['analysis_raw_size'], 0x1000)
        self.assertEqual(struct.unpack_from('<Q', view, 0x98+24)[0], 0x180000000)
        self.assertEqual(view[0x98+144:0x98+152], bytes(8))
        self.assertEqual(struct.unpack_from('<I', view, 0x98+64)[0], 0)

    def test_invalid_headers(self):
        cases = [(0, b'NO'), (0x80, b'BAD!'), (0x98, b'\x0b\x01')]
        for offset, value in cases:
            with self.subTest(offset=offset):
                data = fixture(); data[offset:offset+len(value)] = value
                with self.assertRaises(ValueError): create_view(data, 0x140000000)

    def test_truncated_or_wrong_size(self):
        for size in (12, 0x100, 0x1000, 0x2fff):
            with self.subTest(size=size), self.assertRaises(ValueError):
                create_view(fixture()[:size], 0x140000000)

    def test_invalid_alignment(self):
        for alignment in (0, 3, 0x20000):
            data = fixture(); struct.pack_into('<I', data, 0x98+36, alignment)
            with self.subTest(alignment=alignment), self.assertRaises(ValueError):
                create_view(data, 0x140000000)

    def test_duplicate_and_overlapping_sections(self):
        for rva, size in ((0x1000, 0x800), (0x2000, 0x1800), (0x2100, 0x800), (0x3000, 1)):
            data = fixture(); struct.pack_into('<II', data, 0x98+240+40+8, size, rva)
            with self.subTest(rva=rva,size=size), self.assertRaises(ValueError):
                create_view(data, 0x140000000)


if __name__ == '__main__':
    unittest.main()
