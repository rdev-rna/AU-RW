"""Synthetic-image tests for tools/acu_reflection.py (ACU-RE-0065). No game data is read."""
from __future__ import annotations

import struct
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import acu_reflection as R  # noqa: E402

BASE = 0x1000


def entry(name: str, type_name: str | None, code: int, flags: bytes = b"\x01\x00\x00") -> bytes:
    typ = R.crc(type_name) if type_name else 0
    raw = flags + b"\x02" + struct.pack("<II", R.crc(name), typ) + bytes(2) + struct.pack("<HH", code, code)
    return raw + bytes(R.ENTRY - len(raw))


def image() -> R.Image:
    """Parent `Base` (one field) and child `Child` (two fields), tables then descriptors."""
    data = bytearray(0x400)
    base_table, child_table = 0x100, 0x200
    data[base_table:base_table + R.ENTRY] = entry("Id", None, 7)
    data[base_table + R.ENTRY + 3] = 0  # terminator
    data[child_table:child_table + 2 * R.ENTRY] = entry("Expr", "Expression", 0x15) + entry("Target", None, 5)
    for desc, table, parent, name, size in ((0x300, base_table, 1, "Base", 8), (0x380, child_table, R.crc("Base"), "Child", 0x20)):
        struct.pack_into("<Q", data, desc - 0x20, BASE + table)
        struct.pack_into("<IIII", data, desc, parent, R.crc(name), size, 8)
    return R.Image(bytes(data), base=BASE, names={f"{R.crc(n):08x}": n for n in ("Base", "Child", "Id", "Expr", "Target", "Expression")})


class ReflectionTests(unittest.TestCase):
    def test_child_fields_and_parent_chain(self):
        k = image().klass(R.crc("Child"))
        self.assertEqual(k["size"], 0x20)
        self.assertEqual([f["name"] for f in k["fields"]], ["Expr", "Target"])
        self.assertEqual(k["fields"][0]["type"], "Expression")
        self.assertEqual([f["code"] for f in k["fields"]], ["0015", "0005"])
        self.assertEqual(k["parent"]["class"], "Base")
        self.assertEqual([f["name"] for f in k["parent"]["fields"]], ["Id"])
        self.assertNotIn("parent", k["parent"])

    def test_missing_class_is_reported(self):
        self.assertEqual(image().klass(R.crc("Nope"))["error"], "0 descriptors")

    def test_unresolved_names_show_the_hash(self):
        img = image()
        img.names = {}
        self.assertEqual(img.klass(R.crc("Child"))["fields"][1]["name"], f"?{R.crc('Target'):08x}")


if __name__ == "__main__":
    unittest.main()
