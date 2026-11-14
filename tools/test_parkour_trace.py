"""Decoder tests for tools/parkour_trace.py with synthetic sessions (no game data)."""
import struct
import unittest

from parkour_trace import IMAGE_BASE, decode

LIVE_BASE = 0x140000000
TABLE = 0x143A6DFB0          # fancy table address used only inside this synthetic image
GETTER = 0x1401AF2D0


def image(enum_value=51):
    """Minimal mapped image: fancy entry 0x45 at TABLE points at `mov eax, imm32; ret`."""
    size = max(TABLE + 0x46 * 0x20, GETTER + 8) - IMAGE_BASE
    img = bytearray(size)
    struct.pack_into("<IIIIQ", img, TABLE + 0x45 * 0x20 - IMAGE_BASE, 0x986EB60D, 0x45, 0, 0, GETTER)
    img[GETTER - IMAGE_BASE:GETTER - IMAGE_BASE + 6] = b"\xb8" + struct.pack("<i", enum_value) + b"\xc3"
    return bytes(img)


def element(ptr, fitness, distance, table=TABLE, size=0x2A0):
    raw = bytearray(size)
    if size >= 0x10:
        struct.pack_into("<Q", raw, 0x08, table)
    if size >= 0xCC:
        struct.pack_into("<fff", raw, 0xC0, 1.5, 2.5, distance)
    if size >= 0x208:
        struct.pack_into("<f", raw, 0x204, fitness)
    return {"ptr": f"{ptr:016x}", "hex": bytes(raw).hex()}


def event(seq, label, base, elements, rax=0, tid=7, ok=True, count=None):
    registers = ["0"] * 16
    registers[0] = "00000001834c9be0"
    registers[4] = f"{rax:016x}"
    return {"sequence": seq, "qpc": 1000 + seq, "tid": tid, "label": label, "registers": registers,
            "gather": {"ok": ok, "base": f"{base:016x}", "data": "0", "count": len(elements) if count is None else count,
                       "elements": elements}}


def session(*events):
    header = {"kind": "header", "status": {"image_base": f"{LIVE_BASE:016x}"}}
    return [header, {"kind": "events", "events": list(events)}]


class DecodeTest(unittest.TestCase):
    def test_entry_exit_pairing_and_verified_fields(self):
        a, b = element(0x1000, 0.5, 3.0), element(0x2000, 2.0, 7.25)
        records = session(event(1, "parkour_select_entry", 0xAA, [a, b], count=3),
                          event(2, "parkour_select_exit", 0xAA, [b, a], rax=1))
        decisions, summary = decode(records, image(51))
        self.assertEqual(len(decisions), 1)
        d = decisions[0]
        self.assertEqual((d["considered_count"], d["ranked_count"], d["selected_index"]), (3, 2, 1))
        self.assertEqual(d["selected"]["ptr"], f"{0x1000:016x}")
        self.assertEqual(d["selected"]["fitness"], 0.5)
        self.assertEqual(d["selected"]["distance"], 3.0)
        self.assertEqual(d["selected"]["enum"], 51)
        self.assertEqual(d["selected"]["unverified_acufixes"], {"height_difference": 1.5, "horizontal_difference": 2.5})
        self.assertEqual(summary["selected_enum_counts"], {51: 1})
        self.assertEqual(summary["problems"], {})

    def test_no_selection_is_minus_one(self):
        records = session(event(1, "parkour_select_entry", 0xAA, [element(1, 1.0, 1.0)]),
                          event(2, "parkour_select_exit", 0xAA, [element(1, 1.0, 1.0)], rax=0xFFFFFFFF))
        decisions, summary = decode(records, image())
        self.assertEqual(decisions[0]["selected_index"], -1)
        self.assertIsNone(decisions[0]["selected"])
        self.assertEqual((summary["with_selection"], summary["no_selection"]), (0, 1))

    def test_rax_upper_bits_ignored(self):
        records = session(event(1, "parkour_select_entry", 0xAA, [element(1, 1.0, 1.0)]),
                          event(2, "parkour_select_exit", 0xAA, [element(1, 1.0, 1.0)], rax=0xDEADBEEF00000000))
        self.assertEqual(decode(records, image())[0][0]["selected_index"], 0)

    def test_pairing_is_per_thread_and_array(self):
        records = session(event(1, "parkour_select_entry", 0xAA, [], tid=1),
                          event(2, "parkour_select_entry", 0xBB, [], tid=2),
                          event(3, "parkour_select_exit", 0xBB, [], tid=2, rax=0xFFFFFFFF),
                          event(4, "parkour_select_exit", 0xAA, [], tid=1, rax=0xFFFFFFFF))
        decisions, summary = decode(records, image())
        self.assertEqual([(d["tid"], d["entry_sequence"]) for d in decisions], [(2, 2), (1, 1)])
        self.assertEqual(summary["problems"], {})

    def test_invalid_and_incomplete_inputs_are_reported_not_invented(self):
        short = element(0x3000, 9.0, 9.0, size=0x100)  # fitness lies beyond the copied bytes
        records = session(event(1, "parkour_select_entry", 0xAA, [], ok=False),
                          event(2, "parkour_select_exit", 0xCC, [short], rax=0),
                          event(3, "parkour_select_exit", 0xDD, [element(1, 1.0, 1.0)], rax=5),
                          event(4, "parkour_select_entry", 0xEE, []))
        decisions, summary = decode(records, None)
        self.assertEqual(summary["problems"], {"gather_unreadable": 1, "exit_without_entry": 2,
                                               "selected_outside_gather": 1, "entry_without_exit": 1})
        self.assertIsNone(decisions[0]["selected"]["fitness"])
        self.assertEqual(decisions[0]["selected"]["distance"], 9.0)
        self.assertIsNone(decisions[0]["selected"]["enum"])  # no image: never guessed
        self.assertEqual(summary["enum_resolution"], "unavailable (no --image)")

    def test_enum_requires_exact_getter_shape(self):
        bad = bytearray(image())
        bad[GETTER - IMAGE_BASE + 5] = 0x90  # not `ret`
        records = session(event(1, "parkour_select_entry", 0xAA, [element(1, 1.0, 1.0)]),
                          event(2, "parkour_select_exit", 0xAA, [element(1, 1.0, 1.0)], rax=0))
        self.assertIsNone(decode(records, bytes(bad))[0][0]["selected"]["enum"])
        other_table = session(event(1, "parkour_select_entry", 0xAA, [element(1, 1.0, 1.0, table=0x10)]),
                              event(2, "parkour_select_exit", 0xAA, [element(1, 1.0, 1.0, table=0x10)], rax=0))
        self.assertIsNone(decode(other_table, image())[0][0]["selected"]["enum"])


if __name__ == "__main__":
    unittest.main()
