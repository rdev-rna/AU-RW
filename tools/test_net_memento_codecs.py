"""Synthetic tests for tools/net_memento_codecs.py (no game data needed)."""
from __future__ import annotations

import struct
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import net_memento_codecs as nm  # noqa: E402
import net_protocol_atlas as atlas  # noqa: E402

B = atlas.IMAGE_BASE
BASE = B + 0x1000


class FakeImage:
    """Just enough of atlas.Image: bytes from the image base, no .pdata."""

    def __init__(self, code: bytes):
        self.b = b"\0" * 0x1000 + code + b"\xCC" * 16

    def func_of(self, va):
        return None


def rel32(at: int, target: int, opcode: bytes) -> bytes:
    return opcode + struct.pack("<i", target - (at + len(opcode) + 4))


def write_fields(code: bytes):
    d = nm.Decoder(FakeImage(code)).decode_write(BASE)
    return [(f["slot"], f.get("value_offset"), f["type"], f["source"], f.get("note")) for f in d["fields"]]


# Pieces of the compare-and-set pattern (HumanAvatar write delegate 0x140173180, ACU-RE-0075).
MARK_60 = (b"\x48\x8B\x42\x60"          # mov rax, [rdx+0x60]
           b"\xC6\x42\x68\x01"          # mov byte [rdx+0x68], 1
           b"\xC6\x00\x01")             # mov byte [rax], 1


@unittest.skipIf(nm.capstone is None, "capstone not installed")
class WriteDelegateTests(unittest.TestCase):
    def test_inline_u8_compare_and_set(self):
        code = (b"\x0F\xB6\x41\x10"     # movzx eax, byte [rcx+0x10]
                b"\x38\x42\x70"         # cmp [rdx+0x70], al
                b"\x74\x12" + MARK_60 +  # je over the mark and store
                b"\x0F\xB6\x41\x10"     # movzx eax, byte [rcx+0x10]
                b"\x88\x42\x70"         # mov [rdx+0x70], al
                b"\xC3")
        self.assertEqual(write_fields(code), [(0x60, 0x10, "u8", "+0x10", None)])

    def test_setne_bool_from_register_compare(self):
        # cmp byte [owner+X], bpl with ebp = 0 (GcLDefend 0x1402DB8AA) feeds setne.
        code = (b"\x31\xED"                         # xor ebp, ebp
                b"\x40\x38\x69\x1C"                 # cmp byte [rcx+0x1c], bpl
                b"\x41\x0F\x95\xC0"                 # setne r8b
                b"\x48\x8B\x82\xA0\x00\x00\x00"     # mov rax, [rdx+0xa0]
                b"\xC6\x82\xA8\x00\x00\x00\x01"     # mov byte [rdx+0xa8], 1
                b"\xC6\x00\x01"                     # mov byte [rax], 1
                b"\x44\x88\x82\xB0\x00\x00\x00"     # mov [rdx+0xb0], r8b
                b"\xC3")
        self.assertEqual(write_fields(code), [(0xA0, 0x10, "bool", "+0x1c", None)])

    def test_increment_is_noted(self):
        # ZenHealthReplication stores life + 1 (0x140B24A70).
        code = (b"\x44\x0F\xB7\x41\x18"   # movzx r8d, word [rcx+0x18]
                b"\x66\x41\xFF\xC0"       # inc r8w
                b"\x66\x44\x39\x42\x70"   # cmp [rdx+0x70], r8w
                b"\x74\x10" + MARK_60 +
                b"\x66\x44\x89\x42\x70"   # mov [rdx+0x70], r8w
                b"\xC3")
        self.assertEqual(write_fields(code), [(0x60, 0x10, "u16", "+0x18", "inc")])

    def test_setter_tail_jump_through_saved_registers(self):
        code = bytearray(b"\x48\x8B\xDA"            # mov rbx, rdx   (memento)
                         b"\x48\x8B\xF9"            # mov rdi, rcx   (owner)
                         b"\x48\x8D\x57\x30"        # lea rdx, [rdi+0x30]
                         b"\x48\x8D\x8B\xF8\x00\x00\x00")  # lea rcx, [rbx+0xf8]
        code += rel32(BASE + len(code), 0x14018BF70, b"\xE9")   # jmp f32_q2 setter
        self.assertEqual(write_fields(bytes(code)), [(0xF8, None, "f32_q2", "+0x30", None)])

    def test_code_after_ret_is_ignored_unless_jumped_to(self):
        tail = b"\x0F\xB6\x41\x10\x38\x42\x70\x74\x12" + MARK_60 + b"\x0F\xB6\x41\x10\x88\x42\x70\xC3"
        self.assertEqual(write_fields(b"\xC3" + tail), [])
        # je over a ret into the same tail: the tail is decoded.
        self.assertEqual(write_fields(b"\x74\x01\xC3" + tail), [(0x60, 0x10, "u8", "+0x10", None)])

    def test_nested_element_writer(self):
        # Element loop: rcx = owner array element, rdx = memento sub-array element (GcLDefend 0x1402DB6F0).
        elem = (b"\x0F\xB6\x41\x10\x38\x42\x70\x74\x12" + MARK_60 + b"\x0F\xB6\x41\x10\x88\x42\x70\xC3")
        outer = bytearray(b"\x48\x8B\xF1"              # mov rsi, rcx
                          b"\x48\x8B\xFA"              # mov rdi, rdx
                          b"\x31\xC9"                  # xor ecx, ecx
                          b"\x48\x03\x4E\x60"          # add rcx, [rsi+0x60]
                          b"\x31\xD2"                  # xor edx, edx
                          b"\x48\x03\x97\x80\x00\x00\x00")  # add rdx, [rdi+0x80]
        call_at = len(outer)
        outer += b"\0" * 5 + b"\xC3"
        elem_at = len(outer)
        outer[call_at:call_at + 5] = rel32(BASE + call_at, BASE + elem_at, b"\xE8")
        outer += elem
        self.assertEqual(write_fields(bytes(outer)), [("+0x80[]/0x60", 0x10, "u8", "+0x60->[]+0x10", None)])


@unittest.skipIf(nm.capstone is None, "capstone not installed")
class ReadDelegateTests(unittest.TestCase):
    def test_stamp_and_pairs(self):
        code = (b"\x8B\x42\x50"         # mov eax, [rdx+0x50]
                b"\x3B\x41\x50"         # cmp eax, [rcx+0x50]
                b"\x72\x07"             # jb (skip)
                b"\x0F\xB6\x42\x70"     # movzx eax, byte [rdx+0x70]
                b"\x88\x41\x10"         # mov [rcx+0x10], al
                b"\xC3")
        r = nm.decode_read(nm.Decoder(FakeImage(code)), BASE)
        self.assertEqual(r["stamp_owner_field"], "0x50")
        self.assertEqual(r["pairs"], {"0x70": {"owner": "+0x10"}})

    def test_read_check_counts(self):
        fields = [{"slot": 0x60, "value_offset": 0x10, "source": "+0x10", "how": "inline"},
                  {"slot": 0x80, "value_offset": 0x10, "source": "+0x18", "how": "inline"},
                  {"slot": 0xA0, "value_offset": 0x10, "source": "+0x20", "how": "inline"},
                  {"slot": 0xF8, "source": "+0x30", "how": "call"}]
        rd = {"pairs": {"0x70": {"owner": "+0x10"}, "0x90": {"owner": "+0x1c"}}}
        c = nm.read_check(fields, rd)
        self.assertEqual(c["inline_matched"], 1)
        self.assertEqual(c["inline_differ"], [{"slot": "0x80", "write_source": "+0x18", "read_owner": "+0x1c"}])
        self.assertEqual(c["inline_not_read_inline"], 1)


if __name__ == "__main__":
    unittest.main()
