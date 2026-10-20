"""Authored table vectors and rejection boundaries; no original game data."""
import hashlib
import json
from pathlib import Path
import struct
import tempfile
import unittest

from build_table_probe import parse, probe


def pack(fmt, *values):
    return struct.pack('<' + fmt, *values)


def header(fid, cls):
    return pack('QI', fid, cls)


def fixture(kind=0x1c0000, column=7):
    # Nonzero IDs above JSON's exact numeric range, nonboolean scope, and a
    # nondefault row tag distinguish decoded references from raw ID searches.
    b = b'\x01' + header(0xf123456789abcdef, 0x22ecbe63) + pack('i', 1)
    b += header(20, 0x36839608) + pack('I', 5)
    b += header(0, 0x3c00b2e0) + pack('iBBii', 0, 1, 0, 7, 0)
    b += b'\x03' + pack('i', 0)
    b += pack('i', 1) + header(30, 0x348b28d6) + b'\x03' + pack('f', 0.25)
    b += header(31, 0xb332698e) + pack('I', 0x12345678)
    b += header(32, 0x11bd5345) + pack('ii', 0, 0)
    b += pack('iiIII', 1, column, 0x24aecb7c, kind, 123)
    if kind == 0x120000:
        b += pack('BQ', 0, 0xfedcba9876543210)
    else:
        b += pack('BBQ', 1, 4, 0xfedcba9876543210)
    b += b'\x00' + header(40, 0xf5bd7b8a)
    b += header(41, 0x11bd5345) + pack('iIIi', 0, 0x80, 99, 0)
    b += pack('BQ', 0, 9) + pack('i', 0)
    b += bytes([0, 1, 0, 1]) + pack('iBQ', 1, 0, 50)
    return b


class BuildTableProbe(unittest.TestCase):
    def test_values_and_handle_variant(self):
        for kind in (0x1c0000, 0x120000):
            with self.subTest(kind=kind):
                t = parse(fixture(kind))
                self.assertEqual(t['file_id'], 'f123456789abcdef')
                self.assertEqual(t['consumed'], len(fixture(kind)))
                self.assertEqual(t['columns'][0]['index'], 7)
                row = t['rows'][0]
                self.assertEqual(row['weight'], 0.25)
                self.assertEqual(row['tag']['value'], '12345678')
                p = row['components'][0]['property']
                self.assertEqual(p['data_type'], '24aecb7c')
                self.assertEqual(p['extra'], 123)
                self.assertEqual(p['reference']['file_id'], 'fedcba9876543210')
                if kind == 0x1c0000:
                    self.assertEqual(p['reference']['scope'], 4)
                    self.assertEqual(fixture()[p['reference']['offset']], 1)
                self.assertEqual(t['default_selections']['random_seed'], 99)
                self.assertEqual(t['subtables'], [{'tag': 0, 'file_id': '0000000000000032'}])

    def test_every_truncated_prefix_and_trailing_data(self):
        b = fixture()
        for n in range(len(b)):
            with self.subTest(prefix=n), self.assertRaises(ValueError):
                parse(b[:n])
        with self.assertRaisesRegex(ValueError, 'trailing'):
            parse(b + b'\0')

    def test_invalid_and_unsupported_variants(self):
        for offset, replacement in ((0, b'\x00'), (9, pack('I', 0)),
                                    (13, pack('i', -1)), (13, pack('I', 0xffffffff)),
                                    (45, pack('i', 1)), (49, b'\x02')):
            b = bytearray(fixture())
            b[offset:offset + len(replacement)] = replacement
            with self.subTest(offset=offset), self.assertRaises(ValueError):
                parse(b)
        with self.assertRaisesRegex(ValueError, 'unsupported dynamic property'):
            parse(fixture(kind=0x140000))
        with self.assertRaisesRegex(ValueError, 'undefined column'):
            parse(fixture(column=8))

    def test_report_identity(self):
        data = fixture()
        source = dict(entry=1, stored_sha256='fixture',
                      resource=dict(type_id='22ecbe63', file_id='f123456789abcdef'),
                      payload_hex=data.hex(), payload_length=len(data),
                      payload_sha256=hashlib.sha256(data).hexdigest())
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / 'report.json'
            p.write_text(json.dumps(source), encoding='utf-8')
            self.assertEqual(probe(p)['table']['consumed'], len(data))
            source['payload_length'] += 1
            p.write_text(json.dumps(source), encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'identity mismatch'):
                probe(p)


if __name__ == '__main__':
    unittest.main()
