"""Synthetic-buffer tests for tools/symbol_index.py and tools/symbol_query.py.

The fixture is a hand-laid mapped x64 PE whose expected classes, vtables, properties and
call edges are written down independently below; nothing here reads game data."""
import contextlib
import io
import json
from pathlib import Path
import sqlite3
import struct
import tempfile
import unittest
import zlib

from native_image import Image
import symbol_index as si
import symbol_query as sq

B = 0x140000000
TEXT, RDATA, DATA, PDATA, SIZE = 0x1000, 0x2000, 0x3000, 0x4000, 0x5000


def crc(name):
    return zlib.crc32(name.encode())


class Fixture:
    def __init__(self):
        self.d = bytearray(SIZE)
        d = self.d
        d[:2] = b"MZ"
        struct.pack_into("<I", d, 0x3C, 0x80)
        d[0x80:0x84] = b"PE\0\0"
        struct.pack_into("<HHIIIHH", d, 0x84, 0x8664, 4, 0, 0, 0, 0xF0, 0)
        opt = 0x98
        struct.pack_into("<H", d, opt, 0x20B)
        struct.pack_into("<Q", d, opt + 24, B)
        struct.pack_into("<II", d, opt + 56, SIZE, 0x400)
        struct.pack_into("<I", d, opt + 108, 16)
        struct.pack_into("<II", d, opt + 112 + 3 * 8, PDATA, 36)
        for i, (name, rva, flags) in enumerate([(b".text", TEXT, 0x60000020), (b".rdata", RDATA, 0x40000040),
                                                (b".data", DATA, 0xC0000040), (b".pdata", PDATA, 0x40000040)]):
            struct.pack_into("<8sIIIIIIHHI", d, opt + 0xF0 + 40 * i, name, 0x1000, rva, 0x1000, rva, 0, 0, 0, 0, flags)

    def q(self, va, value):
        struct.pack_into("<Q", self.d, va - B, value)

    def u32(self, va, value):
        struct.pack_into("<I", self.d, va - B, value)

    def put(self, va, data):
        self.d[va - B:va - B + len(data)] = data

    def getter(self, at, target):
        self.put(at, b"\x48\x8b\x05" + struct.pack("<i", target - (at + 7)) + b"\xc3")


def build_fixture():
    f = Fixture()
    t, r, g = B + TEXT, B + RDATA, B + DATA
    # Getters: two valid, three malformed (target outside image, [X] outside image, bad name).
    f.getter(t + 0x00, g + 0x00)
    f.getter(t + 0x10, g + 0x08)
    f.put(t + 0x20, b"\x48\x8b\x05" + struct.pack("<i", 0x7FFFFFF0) + b"\xc3")
    f.getter(t + 0x30, g + 0x10)
    f.getter(t + 0x40, g + 0x18)
    for k in range(8):
        f.put(t + 0x100 + 0x10 * k, b"\xc3")
    f.put(t + 0x200, b"\x48\x89\x51\x10\xc3")                       # mov [rcx+0x10], rdx; ret
    f.put(t + 0x300, b"\x90" * 5)
    f.put(t + 0x305, b"\xe8" + struct.pack("<i", (t + 0x400) - (t + 0x305 + 5)))  # call helper
    f.put(t + 0x322, b"\xe9" + struct.pack("<i", (t + 0x410) - (t + 0x322 + 5)))  # tail jump in fragment
    f.put(t + 0x400, b"\xc3")
    f.put(t + 0x410, b"\xc3")
    # Strings.
    f.put(r + 0x00, b"Base\0")
    f.put(r + 0x10, b"Derived\0")
    f.put(r + 0x20, b"\xff\xfe\x01\0")
    f.put(r + 0x30, b"Owner\0")
    # Vtables.
    base_vt = [t + 0x100, t + 0x110, t + 0x120, t + 0x130, t + 0x140, t + 0x00, t + 0x150]
    derived_vt = [t + 0x100, t + 0x110, t + 0x120, t + 0x130, t + 0x140, t + 0x10, t + 0x300, t + 0x160]
    bad_vt = [t + 0x100, r + 0x00, t + 0x120, t + 0x130, t + 0x140, t + 0x00]  # slot 1 is not code
    for base, slots in ((r + 0x100, base_vt), (r + 0x200, derived_vt), (r + 0x300, bad_vt)):
        for k, v in enumerate(slots):
            f.q(base + 8 * k, v)
    # Class globals and class infos.
    f.q(g + 0x00, g + 0x100)
    f.q(g + 0x08, g + 0x200)
    f.q(g + 0x10, 0xDEADBEEF00000000)
    f.q(g + 0x18, g + 0x300)
    ci = g + 0x100
    f.q(ci + 0x18, r + 0x00)
    f.u32(ci + 0x24, crc("Base"))
    f.u32(ci + 0x28, 0x10)
    struct.pack_into("<HH", f.d, ci + 0x4C - B, 1, 10)
    ci = g + 0x200
    f.q(ci + 0x00, g + 0x180)
    f.q(ci + 0x18, r + 0x10)
    f.u32(ci + 0x20, crc("Base"))
    f.u32(ci + 0x24, crc("Derived"))
    f.u32(ci + 0x28, 0x20)
    f.q(ci + 0x38, g + 0x100)
    struct.pack_into("<HH", f.d, ci + 0x4C - B, 2, 3)
    f.q(g + 0x300 + 0x18, r + 0x20)
    # Two property entries for Derived, bounded by Derived's class info at g+0x200.
    struct.pack_into("<6I4Q", f.d, g + 0x180 - B, 8, crc("Owner"), crc("Base"), 0x00140000,
                     (0x10 << 18) | 0x14, 0, t + 0x200, t + 0x200, 0, 0)
    struct.pack_into("<6I4Q", f.d, g + 0x1B8 - B, 0, 0x12345678, 0, 0x000A0000,
                     (0x18 << 18) | 0x0A, 0, 0, 0, 0, 0)
    f.q(g + 0x400, g + 0x180)
    f.q(g + 0x408, g + 0x1B8)
    # .pdata: caller 0x1300 (+ chained fragment 0x1320), helpers 0x1400 and 0x1410.
    struct.pack_into("<9I", f.d, PDATA, 0x1300, 0x1320, 0x4100, 0x1320, 0x1330, 0x4120,
                     0x1400, 0x1410, 0x4100)
    f.d[0x4100] = 1
    f.d[0x4120:0x4124] = bytes([1 | (4 << 3), 0, 0, 0])
    struct.pack_into("<III", f.d, 0x4124, 0x1300, 0x1320, 0x4100)
    return f


class ScanTests(unittest.TestCase):
    def setUp(self):
        self.fx = build_fixture()
        self.view = si.View(Image(bytes(self.fx.d), mapped=True))

    def test_getter_scan_rejects_malformed_pointers(self):
        getters = si.find_getters(self.view)
        self.assertEqual(getters, {B + TEXT: (B + DATA, B + DATA + 0x100, "Base"),
                                   B + TEXT + 0x10: (B + DATA + 8, B + DATA + 0x200, "Derived")})

    def test_classinfo_layout_and_parent(self):
        classes = si.collect_classes(self.view, si.find_getters(self.view))
        derived = classes[B + DATA + 0x200]
        self.assertEqual((derived["name"], derived["parent"], derived["depth"]), ("Derived", B + DATA + 0x100, 1))
        self.assertEqual((derived["crc_ok"], derived["crc_parent"], derived["size"]), (1, crc("Base"), 0x20))
        self.assertEqual((derived["isa_lo"], derived["isa_hi"]), (2, 3))

    def test_vtables_methods_and_names(self):
        getters = si.find_getters(self.view)
        classes = si.collect_classes(self.view, getters)
        vts = si.find_vtables(self.view, getters, classes, set())
        self.assertEqual(sorted(vts), [B + RDATA + 0x100, B + RDATA + 0x200])  # bad_vt rejected
        self.assertEqual(len(vts[B + RDATA + 0x100]["slots"]), 7)
        self.assertEqual(vts[B + RDATA + 0x200]["end_reason"], "non-code-pointer")
        methods = si.classify_methods(vts, classes)
        origin = {(ci, slot): o for _fn, ci, _vt, slot, o in methods}
        d = B + DATA + 0x200
        self.assertEqual((origin[d, 0], origin[d, 6], origin[d, 7]), ("inherited", "own", "new-slot"))
        names = si.name_vtable_functions(methods, classes)
        self.assertEqual(names[B + TEXT + 0x100][:2], ("Base::vf0", "high"))
        self.assertEqual(names[B + TEXT + 0x300][:2], ("Derived::vf6", "high"))

    def test_properties_offsets_and_accessors(self):
        classes = si.collect_classes(self.view, si.find_getters(self.view))
        bounds = sorted(set(classes) | {c["props"] for c in classes.values() if c["props"]})
        rows = si.read_properties(self.view, classes[B + DATA + 0x200], {crc("Owner"): "Owner"},
                                  {crc("Base"): "Base"}, bounds, {B + DATA + 0x180})
        self.assertEqual(classes[B + DATA + 0x200]["prop_count_method"], "next-known-address")
        self.assertEqual([(x["name"], x["offset"], x["type_code"]) for x in rows],
                         [("Owner", 0x10, 0x14), (None, 0x18, 0x0A)])
        self.assertEqual((rows[0]["type_name"], rows[0]["accessor_offset"], rows[0]["offset_check"]),
                         ("Base", 0x10, "match"))
        self.assertEqual([x["pointer_global"] for x in rows], [1, 0])

    def test_invalid_entry_stops_property_scan(self):
        struct.pack_into("<Q", self.fx.d, B + DATA + 0x1B8 + 0x18 - B, B + RDATA)  # accessor in .rdata
        view = si.View(Image(bytes(self.fx.d), mapped=True))
        classes = si.collect_classes(view, si.find_getters(view))
        bounds = sorted(set(classes) | {c["props"] for c in classes.values() if c["props"]})
        self.assertEqual(si.property_count(view, classes[B + DATA + 0x200], bounds), (1, "invalid-entry"))

    def test_pdata_chain_and_call_edges(self):
        ranges = si.pdata_primary_ranges(self.view)
        self.assertIn((B + 0x1320, B + 0x1330, B + 0x1300), ranges)
        starts = {r[2] for r in ranges} | {B + TEXT + 0x300, B + 0x1410}  # 0x1410: corpus-only start
        edges = si.call_edges(self.view, ranges, starts)
        self.assertEqual(edges, {(B + 0x1300, B + 0x1400, "call"), (B + 0x1300, B + 0x1410, "jmp")})
        props = si.propagate(edges, {B + 0x1300: "Derived"})
        self.assertEqual(props[B + 0x1400][:2], ("Derived::helper_140001400", "low"))
        self.assertEqual(si.propagate(edges | {(B + 0x1100, B + 0x1400, "call")},
                                      {B + 0x1300: "Derived", B + 0x1100: "Base"}).get(B + 0x1400), None)

    def test_out_of_range_reads_fail_clearly(self):
        for va in (B - 8, B + SIZE - 4, B + 0x10_0000, -1):
            with self.assertRaises(ValueError):
                self.view.u64(va)
        self.assertIsNone(si.read_classinfo(self.view, B + SIZE + 0x100))
        self.assertIsNone(si.read_classinfo(self.view, B + TEXT))  # executable, not data
        with self.assertRaises(ValueError):
            si.View(Image(bytes(self.fx.d), mapped=False))

    def test_decode_accessor_patterns(self):
        cases = {b"\x48\x89\x51\x10\xc3": 0x10, b"\x48\x8b\x81\x00\x01\x00\x00\xc3": 0x100,
                 b"\xf3\x0f\x10\x41\x08\xc3": 8, b"\x0f\xb6\x81\xa9\x07\x00\x00\xc3": 0x7A9,
                 b"\x49\x89\x51\x10\xc3": None, b"\x48\x89\x51\x10\x90": None,
                 b"\x48\x89\x11\xc3": None, b"\x48\x89": None, b"": None, b"\x48\x89\x51\xf0\xc3": None}
        for code, want in cases.items():
            self.assertEqual(si.decode_accessor(code), want, code.hex())


class EndToEndTests(unittest.TestCase):
    def test_build_db_and_query(self):
        view = si.View(Image(bytes(build_fixture().d), mapped=True))
        with tempfile.TemporaryDirectory() as tmp:
            findings = Path(tmp) / "f.json"
            findings.write_text(json.dumps([dict(va="0x140001400", name="helper_from_finding",
                                                 finding="ACU-RE-9999", evidence_kind="live")]))
            result = si.build(view, None, findings, log=lambda m: None)
            db = Path(tmp) / "s.sqlite"
            si.write_db(result, db, dict(schema_version=1))
            cov = si.coverage(db)
            self.assertEqual((cov["classes"], cov["vtables"], cov["fields"]), (2, 2, 2))
            self.assertEqual(cov["classes_parent_crc_agrees"], 1)
            con = sqlite3.connect(db)
            self.assertEqual(con.execute("SELECT name FROM name WHERE va=? AND source='reflection-property'",
                                         (B + 0x1200,)).fetchone()[0], "Derived::Owner_accessor0")
            con.close()
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.assertEqual(sq.main(["--db", str(db), "--json", "name", "0x140001400"]), 0)
            answer = json.loads(out.getvalue())
            self.assertEqual(answer["best"], "helper_from_finding")
            self.assertEqual([n["source"] for n in answer["names"]], ["capture-finding", "propagation"])
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.assertEqual(sq.main(["--db", str(db), "--json", "fields", "Derived"]), 0)
            self.assertEqual([x["offset"] for x in json.loads(out.getvalue())], ["0x10", "0x18"])
            with contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(sq.main(["--db", str(db), "class", "Nope"]), 2)
                self.assertEqual(sq.main(["--db", str(db), "sql", "DELETE FROM name"]), 2)


if __name__ == "__main__":
    unittest.main()
