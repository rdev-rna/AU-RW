import struct
import tempfile
import unittest
from pathlib import Path

from bind_corpus import (BindingError, bind, diff_runs, export_binding, image_binding,
                         load_image, pe_headers, probe, pdata_starts, read_bounded,
                         read_export)

IMAGE_BASE = 0x140000000
TEXT = 0x60000020
RDATA = 0x40000040
CNT_CODE = 0x20


def build(sections, data_dirs=None, timestamp=0x5AB00D1E, size_of_headers=0x200,
          image_base=IMAGE_BASE, overrides=None, mapped=True):
    """Lay out a bounded x64 PE.

    `mapped=True` produces a runtime image with offset == RVA, which is the layout a
    memory dump has. `mapped=False` produces a file image where a section's payload
    lives at its advertised RawOffset, which is the layout an installed PE has.
    """
    size = max(section["rva"] + section["virtual_size"] for section in sections)
    size = (size + 0xFFF) & ~0xFFF
    data = bytearray(size)
    data[:2] = b"MZ"
    pe = 0x80
    struct.pack_into("<I", data, 0x3C, pe)
    data[pe:pe + 4] = b"PE\0\0"
    struct.pack_into("<HHIIIHH", data, pe + 4, 0x8664, len(sections), timestamp, 0, 0, 0xF0, 0x22)
    opt = pe + 24
    struct.pack_into("<H", data, opt, 0x20B)
    struct.pack_into("<I", data, opt + 16, 0x1000)
    struct.pack_into("<Q", data, opt + 24, image_base)
    struct.pack_into("<I", data, opt + 56, size)
    struct.pack_into("<I", data, opt + 60, size_of_headers)
    directories = dict(data_dirs or {})
    struct.pack_into("<I", data, opt + 108, 16)
    for index, (rva, length) in directories.items():
        struct.pack_into("<II", data, opt + 112 + index * 8, rva, length)
    raw_cursor = size_of_headers
    for index, section in enumerate(sections):
        payload = section.get("payload", b"")
        raw = section.get("raw", bool(payload))
        raw_size = len(payload) if raw else 0
        if not raw:
            raw_offset = 0
        elif mapped:
            raw_offset = section["rva"]
        else:
            raw_offset = raw_cursor
        struct.pack_into("<8sIIIIIIHHI", data, opt + 0xF0 + index * 40,
                         section["name"].encode(), section["virtual_size"], section["rva"],
                         raw_size, raw_offset, 0, 0, 0, 0, section["flags"])
        if raw:
            target = section["rva"] if mapped else raw_offset
            data[target:target + len(payload)] = payload
            raw_cursor += len(payload) + 0x200
    for offset, value in (overrides or {}).items():
        data[offset] = value
    return bytes(data)


def sections(code=b"\x55\x48\x89\xe5\xc3", resource=b"resource-bytes", pdata=b""):
    rows = [
        dict(name=".text", rva=0x1000, virtual_size=0x1000, flags=TEXT, payload=code),
        dict(name=".rsrc", rva=0x2000, virtual_size=0x1000, flags=RDATA, payload=resource),
    ]
    if pdata:
        rows.append(dict(name=".pdata", rva=0x3000, virtual_size=0x1000,
                         flags=RDATA, payload=pdata))
    return rows


def write(directory, name, payload):
    path = Path(directory) / name
    path.write_bytes(payload)
    return path


def runtime(disk_bytes, runtime_sections):
    """A runtime image that keeps the installed file's header, as a memory dump does."""
    data = bytearray(build(runtime_sections))
    headers = pe_headers(disk_bytes, "disk")
    data[:headers["size_of_headers"]] = disk_bytes[:headers["size_of_headers"]]
    return bytes(data)


class SourceTests(unittest.TestCase):
    def test_packed_code_section_is_unavailable_never_matched(self):
        packed = [dict(name=".text", rva=0x1000, virtual_size=0x1000, flags=TEXT, raw=False),
                  dict(name=".rsrc", rva=0x2000, virtual_size=0x1000, flags=RDATA,
                       payload=b"resources")]
        unpacked = [dict(packed[0], payload=b"\x55\x48\x89\xe5\xc3", raw=True), dict(packed[1])]
        with tempfile.TemporaryDirectory() as folder:
            disk_bytes = build(packed, mapped=False)
            disk = load_image(write(folder, "ACU.exe", disk_bytes), False, "exe")
            image = load_image(write(folder, "image.bin", runtime(disk_bytes, unpacked)), True, "img")
            report = image_binding(disk, image, set())
            row = next(r for r in report["sections"] if r["name"] == ".text")
            self.assertEqual(row["verdict"], "unavailable")
            self.assertEqual(report["verdict"], "bound_by_headers_and_resources")
            self.assertEqual(report["code_identity"], "unavailable_on_disk")
            self.assertTrue(any("unavailable rather than verified" in r for r in report["reasons"]))

    def test_unpacked_code_that_matches_binds_by_code(self):
        with tempfile.TemporaryDirectory() as folder:
            code = sections()[0]
            disk_bytes = build([dict(code)], mapped=False)
            disk = load_image(write(folder, "exe", disk_bytes), False, "exe")
            image = load_image(write(folder, "img", runtime(disk_bytes, [dict(code)])), True, "img")
            report = image_binding(disk, image, set())
            self.assertEqual(report["verdict"], "bound_by_code")
            self.assertEqual(report["code_identity"], "byte_matched")

    def test_code_that_differs_unbinds(self):
        with tempfile.TemporaryDirectory() as folder:
            disk_bytes = build(sections(code=b"\x55\x90"), mapped=False)
            disk = load_image(write(folder, "exe", disk_bytes), False, "exe")
            image = load_image(write(folder, "img", runtime(disk_bytes, sections(code=b"\x56\x91"))),
                               True, "img")
            report = image_binding(disk, image, set())
            self.assertEqual(report["verdict"], "unbound")
            self.assertTrue(any(".text" in reason for reason in report["reasons"]))

    def test_mutable_section_difference_is_recorded_but_not_fatal(self):
        disk_sections = [dict(sections()[0]),
                         dict(name=".rsrc", rva=0x2000, virtual_size=0x1000, flags=RDATA,
                              payload=b"original"),
                         dict(name=".UBX1", rva=0x4000, virtual_size=0x1000, flags=TEXT,
                              payload=b"packed-payload")]
        image_sections = [dict(disk_sections[0]),
                          dict(disk_sections[1]),
                          dict(name=".UBX1", rva=0x4000, virtual_size=0x1000, flags=TEXT,
                               payload=b"runtime-patched")]
        with tempfile.TemporaryDirectory() as folder:
            disk_bytes = build(disk_sections, mapped=False)
            disk = load_image(write(folder, "exe", disk_bytes), False, "exe")
            image = load_image(write(folder, "img", runtime(disk_bytes, image_sections)), True, "img")
            strict = image_binding(disk, image, set())
            self.assertEqual(strict["verdict"], "unbound")
            declared = image_binding(disk, image, {".UBX1"})
            # .text matches, but .UBX1 is executable as well, so code identity stays
            # incomplete even though the difference is declared and tolerated.
            self.assertEqual(declared["verdict"], "bound_by_headers_and_resources")
            ubx = next(r for r in declared["sections"] if r["name"] == ".UBX1")
            self.assertEqual(ubx["verdict"], "different")
            self.assertTrue(ubx["declared_mutable"])

    def test_base_mismatch_unbinds_and_names_the_anchor(self):
        with tempfile.TemporaryDirectory() as folder:
            disk = load_image(write(folder, "exe", build(sections(), mapped=False)), False, "exe")
            image = load_image(write(folder, "img", build(sections(), image_base=0x150000000)),
                               True, "img")
            report = image_binding(disk, image, set())
            self.assertEqual(report["verdict"], "unbound")
            self.assertFalse(report["anchors"]["image_base"])
            self.assertTrue(any("image_base" in reason for reason in report["reasons"]))

    def test_empty_and_oversized_inputs_fail_closed(self):
        with tempfile.TemporaryDirectory() as folder:
            empty = write(folder, "empty", b"")
            with self.assertRaises(BindingError):
                read_bounded(empty)
            with self.assertRaises(BindingError):
                load_image(write(folder, "bad", b"MZ"), False, "bad")
            with self.assertRaises(BindingError):
                load_image(write(folder, "bad2", b"not-a-pe"), False, "bad2")

    def test_mapped_image_shorter_than_size_of_image_is_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            payload = build(sections())
            with self.assertRaises(BindingError):
                load_image(write(folder, "short", payload[:-0x2000]), True, "short")


class ExportTests(unittest.TestCase):
    def rows(self, folder, text):
        return write(folder, "functions.csv", text.encode())

    def test_names_with_commas_keep_address_and_size(self):
        with tempfile.TemporaryDirectory() as folder:
            path = self.rows(folder, "address,name,size\n"
                                      "140001000,FUN_140001000,32\n"
                                      "140001100,_Med3_unchecked<SC*,bool(__cdecl*)(SC&,SC&)>,48\n")
            export = read_export(path, IMAGE_BASE, [(0x1000, 0x2000)])
            self.assertEqual(export["rows"], 2)
            self.assertEqual(export["problem_counts"]["malformed"], 0)
            self.assertEqual(export["sizes"][0x140001100], 48)

    def test_auto_named_symbol_must_agree_with_its_address(self):
        with tempfile.TemporaryDirectory() as folder:
            path = self.rows(folder, "address,name,size\n"
                                      "140001000,FUN_140001000,32\n"
                                      "140001100,FUN_140009999,16\n"
                                      "140001200,ReadData,16\n"
                                      "140001300,thunk_FUN_140005920,16\n")
            export = read_export(path, IMAGE_BASE, [(0x1000, 0x2000)])
            self.assertEqual(export["problem_counts"]["name_mismatch"], 1)
            self.assertIn("FUN_140009999", export["problem_sample"]["name_mismatch"][0])

    def test_row_outside_executable_sections_is_counted(self):
        with tempfile.TemporaryDirectory() as folder:
            path = self.rows(folder, "address,name,size\n140003000,FUN_140003000,16\n")
            export = read_export(path, IMAGE_BASE, [(0x1000, 0x2000)])
            self.assertEqual(export["problem_counts"]["outside_executable"], 1)

    def test_bad_header_and_empty_export_raise(self):
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaises(BindingError):
                read_export(self.rows(folder, "addr,name,size\n140001000,FUN_140001000,4\n"),
                            IMAGE_BASE, [(0x1000, 0x2000)])
            with self.assertRaises(BindingError):
                read_export(self.rows(folder, "address,name,size\n"), IMAGE_BASE, [(0x1000, 0x2000)])

    def test_unsorted_and_duplicate_rows_are_reported(self):
        with tempfile.TemporaryDirectory() as folder:
            path = self.rows(folder, "address,name,size\n"
                                      "140001100,FUN_140001100,16\n"
                                      "140001000,FUN_140001000,16\n"
                                      "140001000,FUN_140001000,16\n")
            export = read_export(path, IMAGE_BASE, [(0x1000, 0x2000)])
            self.assertEqual(export["unsorted"], 2)
            self.assertEqual(export["duplicates"], 1)


class UnwindCoverageTests(unittest.TestCase):
    def fixture(self, folder, starts, export_rows):
        table = b"".join(struct.pack("<III", start, start + 8, 0x1000) for start in starts)
        sections_ = sections(pdata=table)
        image_bytes = build(sections_, data_dirs={3: (0x3000, len(table))})
        image = load_image(write(folder, "img", image_bytes), True, "img")
        header = "address,name,size\n" + "".join(
            f"{IMAGE_BASE + address:x},FUN_{IMAGE_BASE + address:x},{size}\n"
            for address, size in export_rows)
        export = read_export(write(folder, "f.csv", header.encode()), IMAGE_BASE,
                             [(0x1000, 0x2000)])
        return image, export

    def test_exact_contained_and_absent_are_separated(self):
        with tempfile.TemporaryDirectory() as folder:
            image, export = self.fixture(folder, [0x1000, 0x1020, 0x9000],
                                         [(0x1000, 0x40)])
            report = export_binding(export, IMAGE_BASE, 0x3000, set([0x1000, 0x1020, 0x9000])
                                    , {"rva": 0x1000}, 0.6)
            self.assertEqual((report["exact"], report["contained"], report["absent"]), (1, 1, 1))
            self.assertEqual(report["verdict"], "bound")
            self.assertEqual(report["absent_sample"], ["0x140009000"])

    def test_low_coverage_refuses_the_export(self):
        with tempfile.TemporaryDirectory() as folder:
            image, export = self.fixture(folder, [0x1000, 0x2000, 0x8000],
                                         [(0x1000, 0x10)])
            report = export_binding(export, IMAGE_BASE, 0x3000,
                                    set([0x1000, 0x2000, 0x8000]), {"rva": 0x1000}, 0.99)
            self.assertEqual(report["verdict"], "unbound")
            self.assertLess(report["resolved_coverage"], 0.99)

    def test_probes_report_bytes_and_membership(self):
        with tempfile.TemporaryDirectory() as folder:
            image, export = self.fixture(folder, [0x1000], [(0x1000, 0x40)])
            rows = probe([IMAGE_BASE + 0x1000, IMAGE_BASE + 0x5000], export, {0x1000},
                         image["image"], IMAGE_BASE, {"rva": 0x1000, "virtual_size": 0x100})
            self.assertTrue(rows[0]["exported"])
            self.assertTrue(rows[0]["unwind_start"])
            self.assertEqual(rows[0]["first_bytes"], (b"\x55\x48\x89\xe5\xc3" + b"\x00" * 27).hex())
            self.assertFalse(rows[1]["inside_text"])


class EndToEndTests(unittest.TestCase):
    def test_packed_build_binds_without_code_comparison(self):
        table = struct.pack("<III", 0x1000, 0x1008, 0x2000)
        packed = [dict(name=".text", rva=0x1000, virtual_size=0x1000, flags=TEXT, raw=False),
                  dict(name=".rsrc", rva=0x2000, virtual_size=0x1000, flags=RDATA,
                       payload=b"resources"),
                  dict(name=".pdata", rva=0x3000, virtual_size=0x1000, flags=RDATA, raw=False)]
        unpacked = [dict(packed[0], payload=b"\x55\x48\x89\xe5\xc3", raw=True),
                    dict(packed[1]),
                    dict(packed[2], payload=table, raw=True)]
        export_text = f"address,name,size\n{IMAGE_BASE + 0x1000:x},FUN_{IMAGE_BASE + 0x1000:x},16\n"
        with tempfile.TemporaryDirectory() as folder:
            disk_bytes = build(packed, data_dirs={3: (0x3000, len(table))}, mapped=False)
            report = bind(write(folder, "ACU.exe", disk_bytes),
                          write(folder, "image.bin", runtime(disk_bytes, unpacked)),
                          None,
                          write(folder, "f.csv", export_text.encode()),
                          corpus_dir=None, mutable_sections=(), extra_probes=[])
            self.assertEqual(report["verdict"], "bound_without_code_comparison")
            self.assertEqual(report["image_binding"]["code_identity"], "unavailable_on_disk")
            self.assertEqual(report["export_binding"]["verdict"], "bound")
            self.assertEqual(report["probes"], [])

    def test_image_that_does_not_match_the_export_is_unbound(self):
        table = struct.pack("<III", 0x1800, 0x1808, 0x2000)
        unpacked = [dict(name=".text", rva=0x1000, virtual_size=0x1000, flags=TEXT,
                         payload=b"\x55\x48\x89\xe5\xc3"),
                    dict(name=".rsrc", rva=0x2000, virtual_size=0x1000, flags=RDATA,
                         payload=b"resources"),
                    dict(name=".pdata", rva=0x3000, virtual_size=0x1000, flags=RDATA,
                         payload=table)]
        export_text = f"address,name,size\n{IMAGE_BASE + 0x1000:x},FUN_{IMAGE_BASE + 0x1000:x},16\n"
        with tempfile.TemporaryDirectory() as folder:
            # The unwind table names a start the export never declares.
            disk_bytes = build([dict(s, raw=False if s["name"] == ".pdata" else True) for s in unpacked],
                               data_dirs={3: (0x3000, len(table))}, mapped=False)
            report = bind(write(folder, "ACU.exe", disk_bytes),
                          write(folder, "image.bin", runtime(disk_bytes, unpacked)),
                          None,
                          write(folder, "f.csv", export_text.encode()),
                          corpus_dir=None, mutable_sections=(), extra_probes=[])
            self.assertEqual(report["verdict"], "unbound")
            self.assertEqual(report["export_binding"]["absent"], 1)


class DiffTests(unittest.TestCase):
    def test_runs_are_located_and_truncation_is_reported(self):
        left = b"\x00" * 0x3000
        right = bytearray(left)
        right[0x1200] = 1
        report = diff_runs(left, bytes(right))
        self.assertEqual(report["differing_pages"], 1)
        self.assertEqual(report["runs"], [["0x1000", "0x2000"]])
        self.assertFalse(report["truncated"])


if __name__ == "__main__":
    unittest.main()
