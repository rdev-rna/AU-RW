"""Synthetic tests for tools/net_intention_payloads.py (no game data needed)."""
from __future__ import annotations

import struct
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import net_intention_payloads as nip  # noqa: E402
import net_protocol_atlas as atlas  # noqa: E402

B = atlas.IMAGE_BASE
BASE = B + 0x1000


def call(at: int, target: int) -> bytes:
    return b"\xE8" + struct.pack("<i", target - (at + 5))


def fields(code: bytes, dst):
    md, ins = nip.disasm(code, BASE)
    return [(f["slot"], f["type"], f["source"]) for f in nip.parse_fields(md, ins, dst)]


@unittest.skipIf(nip.capstone is None, "capstone not installed")
class InlineFormTests(unittest.TestCase):
    def test_inline_u32_and_setne_bool(self):
        # Pattern from the AssassinateTarget block (ACU-RE-0079 observation 2).
        code = (b"\x8B\x43\x08"                  # mov eax, [rbx+8]
                b"\x39\x42\x50"                  # cmp [rdx+0x50], eax
                b"\x89\x42\x50"                  # mov [rdx+0x50], eax
                b"\x44\x38\x73\x7C"              # cmp byte [rbx+0x7c], r14b
                b"\x0F\x95\xC1"                  # setne cl
                b"\x88\x8A\x10\x01\x00\x00"      # mov [rdx+0x110], cl
                b"\xEB\x00")                     # jmp (end of block)
        self.assertEqual(fields(code, "rdx"), [("0x38", "u32", "+0x8"), ("0xF8", "bool", "+0x7c")])

    def test_stops_at_jmp(self):
        code = b"\x8B\x43\x08\xEB\x00\x89\x42\x50"   # load; jmp; store after jmp is ignored
        self.assertEqual(fields(code, "rdx"), [])

    def test_non_source_register_is_derived(self):
        code = b"\x8B\x40\x08\x89\x42\x50\xEB\x00"   # mov eax,[rax+8]; mov [rdx+0x50],eax
        self.assertEqual(fields(code, "rdx"), [("0x38", "u32", "derived")])


@unittest.skipIf(nip.capstone is None, "capstone not installed")
class SetterFormTests(unittest.TestCase):
    def test_setter_calls_with_pointer_and_stack_byte(self):
        code = bytearray()
        code += b"\x48\x8D\x4F\x38"              # lea rcx, [rdi+0x38]
        code += b"\x48\x8D\x53\x08"              # lea rdx, [rbx+8]
        code += call(BASE + len(code), 0x1401D7650)
        code += b"\x0F\xB6\x43\x14"              # movzx eax, byte [rbx+0x14]
        code += b"\x48\x8D\x4F\x58"              # lea rcx, [rdi+0x58]
        code += b"\x48\x8D\x54\x24\x20"          # lea rdx, [rsp+0x20]
        code += b"\x88\x44\x24\x20"              # mov [rsp+0x20], al  (after the lea)
        code += call(BASE + len(code), 0x140783190)
        code += b"\xEB\x00"
        self.assertEqual(fields(bytes(code), None), [("0x38", "u32", "+0x8"), ("0x58", "u8", "+0x14")])

    def test_unknown_callee_is_not_a_field(self):
        code = bytearray(b"\x48\x8D\x4F\x38\x48\x8D\x53\x08")
        code += call(BASE + len(code), 0x140000000)
        code += b"\xEB\x00"
        self.assertEqual(fields(bytes(code), None), [])


class AnchorTests(unittest.TestCase):
    def test_finds_both_store_forms(self):
        imm = struct.pack("<I", 0xDC303C6B)
        text = b"\x90" + b"\xC7\x42\x08" + imm + b"\x90" + b"\xBA" + imm + b"\x41\xC7\x47\x08" + imm
        hits = nip.find_anchor(text, BASE, 0xDC303C6B)
        self.assertEqual([(h[1], h[2]) for h in hits], [("A", "rdx"), ("B", None), ("A", "r15")])
        self.assertEqual(hits[0][0], BASE + 1)

    def test_ignores_bare_constant(self):
        self.assertEqual(nip.find_anchor(b"\x90\x90\x90\x90" + struct.pack("<I", 7), BASE, 7), [])


if __name__ == "__main__":
    unittest.main()
