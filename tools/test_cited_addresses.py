import struct
import unittest

import verify_cited_addresses as audit

BASE = 0x140000000
TEXT = 0x60000020
RDATA = 0x40000040


def pe(sections):
    """Minimal x64 PE laid out by RVA, enough for image_sections()."""
    size = max(rva + vsize for _, rva, vsize, _, _ in sections)
    size = (size + 0xFFF) & ~0xFFF
    data = bytearray(size)
    data[:2] = b"MZ"
    offset = 0x80
    struct.pack_into("<I", data, 0x3C, offset)
    data[offset:offset + 4] = b"PE\0\0"
    struct.pack_into("<HHIIIHH", data, offset + 4, 0x8664, len(sections), 0, 0, 0, 0xF0, 0)
    opt = offset + 24
    struct.pack_into("<H", data, opt, 0x20B)
    struct.pack_into("<Q", data, opt + 24, BASE)
    struct.pack_into("<I", data, opt + 56, size)
    struct.pack_into("<I", data, opt + 60, 0x200)
    struct.pack_into("<I", data, opt + 108, 16)
    for index, (name, rva, vsize, flags, payload) in enumerate(sections):
        struct.pack_into("<8sIIIIIIHHI", data, opt + 0xF0 + index * 40,
                         name.encode(), vsize, rva, len(payload), rva, 0, 0, 0, 0, flags)
        data[rva:rva + len(payload)] = payload
    return bytes(data)


def write(tmpdir, payload):
    path = tmpdir / "image.bin"
    path.write_bytes(payload)
    return path


class PairingTests(unittest.TestCase):
    def test_same_line_pair_needs_guard_after_va(self):
        lines = ["- exit VA `0x1400e9cc0`, guard `40534883ec30488bd9488b89d0000000`."]
        self.assertEqual(audit.pairing(lines, 0),
                         ("0x1400e9cc0", "40534883ec30488bd9488b89d0000000"))

    def test_guard_before_va_is_not_paired(self):
        lines = ["- `40534883ec30488bd9488b89d0000000` then VA `0x1400e9cc0`."]
        self.assertEqual(audit.pairing(lines, 0), (None, None))

    def test_two_vas_on_one_line_are_ambiguous(self):
        lines = ["- `0x1400e9cc0` and `0x140177c40` share `40534883ec30`."]
        self.assertEqual(audit.pairing(lines, 0), (None, None))

    def test_next_line_guard_after_bytes(self):
        lines = ["- Live probe site `0x1400988e3`, bytes",
                 "  `0f2970e80f2978d8440fb7d9440f2940`. Following text."]
        self.assertEqual(audit.pairing(lines, 0),
                         ("0x1400988e3", "0f2970e80f2978d8440fb7d9440f2940"))

    def test_next_line_guard_without_bytes_word_is_not_paired(self):
        lines = ["- See `0x1400988e3` for detail.", "  `0f2970e80f2978d8440fb7d9440f2940`"]
        self.assertEqual(audit.pairing(lines, 0), (None, None))


class ClassifyTests(unittest.TestCase):
    def fixture(self):
        # .text at 0x1000, 0x100 bytes; a function entry at 0x1000 and padding at 0x1040
        payload = bytearray(b"\x90" * 0x100)
        payload[0x40] = 0xCC
        return write(self.tmp, pe([(".text", 0x1000, 0x100, TEXT, bytes(payload)),
                                   (".rdata", 0x2000, 0x100, RDATA, b"\x11" * 0x100)]))

    def setUp(self):
        import tempfile
        from pathlib import Path
        self.tmp = Path(tempfile.mkdtemp())

    def test_module_base_is_not_a_code_address(self):
        image, base, sections = audit.image_sections(self.fixture())
        row = audit.classify("0x140000000", None, image, base, sections)
        self.assertEqual(row["status"], "image-base")

    def test_function_entry_and_padding_are_separated(self):
        image, base, sections = audit.image_sections(self.fixture())
        starts = {0x1000}
        self.assertEqual(audit.classify_unpaired("0x140001000", image, base, sections, starts)["status"],
                         "function-entry")
        self.assertEqual(audit.classify_unpaired("0x140001040", image, base, sections, starts)["status"],
                         "inside-padding")

    def test_data_section_is_not_called_executable(self):
        image, base, sections = audit.image_sections(self.fixture())
        row = audit.classify_unpaired("0x140002000", image, base, sections, set())
        self.assertEqual(row["section"], ".rdata")
        self.assertFalse(row["executable"])

    def test_live_pointer_is_external_not_an_error(self):
        image, base, sections = audit.image_sections(self.fixture())
        self.assertEqual(audit.classify_unpaired("0x1e684c790", image, base, sections, set())["status"],
                         audit.EXTERNAL)

    def test_guard_mismatch_reports_first_difference(self):
        image, base, sections = audit.image_sections(self.fixture())
        row = audit.classify("0x140001000", "90909090", image, base, sections)
        self.assertEqual(row["status"], "verified")
        row = audit.classify("0x140001000", "90909000", image, base, sections)
        self.assertEqual(row["status"], "mismatch")
        self.assertEqual(row["first_difference"], 3)


class GuardAuditTests(unittest.TestCase):
    def test_off_by_is_reported_with_delta(self):
        from pathlib import Path
        import tempfile
        tmp = Path(tempfile.mkdtemp())
        payload = bytearray(b"\x00" * 0x100)
        payload[0x30:0x40] = bytes.fromhex("deadbeef" + "00" * 12)
        image_path = write(tmp, pe([(".text", 0x1000, 0x100, TEXT, bytes(payload))]))
        image, base, _ = audit.image_sections(image_path)
        doc = tmp / "doc.md"
        doc.write_text("- site `0x140001020` guard `deadbeef000000000000000000000000`\n",
                       encoding="utf-8")
        rows = audit.guard_audit([doc], image, base)
        self.assertEqual(rows[0]["status"], "off-by")
        self.assertEqual(rows[0]["delta"], 0x10)
        self.assertEqual(rows[0]["found_va"], "0x140001030")


if __name__ == "__main__":
    unittest.main()
