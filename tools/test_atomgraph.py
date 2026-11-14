"""Tests for tools/atomgraph.py on synthetic AtomGraph bytes (no game data), plus one
install-dependent check that runs only when a local NewDemo dump exists.

Run: python -m unittest tools/test_atomgraph.py
"""
from __future__ import annotations

import json
import struct
import sys
import unittest
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import atomgraph as A  # noqa: E402

PARAM_BOOL, PARAM_HEIGHT, PARAM_DIR = 0, 1, 2


def obj(cls: str, body: bytes = b"", first: int = 0) -> bytes:
    """Header (first byte, zero u64 id, class CRC) + body."""
    return bytes([first]) + bytes(8) + struct.pack("<I", zlib.crc32(cls.encode()) & 0xFFFFFFFF) + body


def rtcp(name: str, typ: str) -> bytes:
    n, t = name.encode(), typ.encode()
    return obj("AtomRTCPDescriptor", struct.pack("<I", len(n)) + n + b"\0" + struct.pack("<I", len(t)) + t + b"\0"
               + struct.pack("<I", zlib.crc32(n) & 0xFFFFFFFF))


def wrapper(vtype: int, value=None) -> bytes:
    body = bytearray(36)
    body[0] = vtype
    if vtype == 1:
        struct.pack_into("<f", body, 4, value)
    elif vtype in (0, 2):
        struct.pack_into("<i", body, 8, int(value))
    return obj("AtomDataContainerWrapper", bytes(body))


def cond(param=0, kind=0, op=0, combine=0, count=0, vtype=10, value=None, first=0x3F, b=-1) -> bytes:
    mid = struct.pack("<III", op, combine, 2) + bytes([0]) + struct.pack("<H", 0xFFFF) + bytes([count])
    return obj("AtomCondition", struct.pack("<IiI", param, b, kind) + mid, first) + wrapper(vtype, value)


def port_link(i: int) -> bytes:
    return obj("AtomInputPort", struct.pack("<II", 9, i) + bytes(12), 0x3F)


def port_param(i: int) -> bytes:
    return obj("AtomInputPort", struct.pack("<II", 0, i) + bytes(12), 0x3F)


def port_const(x: float) -> bytes:
    return obj("AtomInputPort", struct.pack("<II", 3, 0) + bytes(4) + struct.pack("<f", x) + bytes(4), 0x3F)


def port_flag(v: int) -> bytes:
    return obj("AtomInputPort", struct.pack("<II", 2, v) + bytes(12), 0x3F)


def clip(file_id: int, flag: int = 0) -> bytes:
    """AtomAnimationDataNode: 321 bytes, clip u64 at +293, ports 4,4,2(flag),4."""
    ports = b"".join((obj("AtomInputPort", struct.pack("<II", 4, 0) + bytes(12), 0x3F),) * 2) + port_flag(flag) + \
        obj("AtomInputPort", struct.pack("<II", 4, 0) + bytes(12), 0x3F)
    node = bytearray(obj("AtomAnimationDataNode", bytes(4) + ports))
    node += bytes(321 - len(node))
    struct.pack_into("<Q", node, A.CLIP_AT, file_id)
    return bytes(node)


def initial(conds: bytes, reads: list[int], target: int) -> bytes:
    n = conds.count(struct.pack("<I", A.HASH["AtomCondition"]))
    expr = obj("AtomConditionExpression", struct.pack("<III", 0x1234, 0x5678, n) + bytes(9))
    return obj("AtomInitialState") + expr + conds + struct.pack("<fI", 1.0, len(reads)) + \
        b"".join(struct.pack("<I", r) for r in reads) + bytes([target]) + obj("AtomNodeID", bytes(16))


def initial_always(target: int) -> bytes:
    """A rule with an empty Conditions array, laid out as in NewDemo (offset 10,077,867)."""
    expr = obj("AtomConditionExpression", struct.pack("<IIII", 0, 0x5000EA8B, 0, 0))
    return obj("AtomInitialState") + expr + bytes([target]) + obj("AtomNodeID", bytes(16))


def graph_state(nodes: list[bytes]) -> bytes:
    return obj("AtomGraphStateNode", bytes(16)) + obj("AtomNodeID", bytes(16)) + b"".join(nodes)


def root(link: int) -> bytes:
    return obj("AtomAnimationRootNode", bytes(4) + port_link(link))


def params_block() -> bytes:
    return rtcp("Flag", "bool") + rtcp("Height", "scalar") + rtcp("Direction", "int")


def grating_state(flag: int) -> bytes:
    """Nodes: 0 clip A, 1 clip B, 2 condition (Height >= 0.2), 3 if, 4 cache(Height), 5 interval, 6 root."""
    return graph_state([
        clip(0xA, flag), clip(0xB, flag),
        obj("AtomGraphConditionNode", bytes(4)) + cond(kind=2, count=1, first=0) +
        cond(PARAM_HEIGHT, op=2, vtype=1, value=0.2),
        obj("AtomIfNode", bytes(4)) + port_link(2) + port_link(1) + port_link(0),
        obj("AtomCacheValueNode", bytes(4)) + port_param(PARAM_HEIGHT),
        obj("AtomSelectFromIntervalNode", bytes(4)) + port_link(4) + port_link(0) + port_const(-1.0) + port_link(3),
        root(5),
    ])


def direction_rule(value: int, target: int) -> bytes:
    return initial(cond(kind=2, count=1, first=0) + cond(PARAM_DIR, op=0, vtype=2, value=value), [PARAM_DIR], target)


def two_state_machine(rule_targets=(1, 0)) -> tuple[bytes, int]:
    pre = params_block()
    sm = obj("AtomStateMachineNode", bytes(16)) + obj("AtomNodeID", bytes(16))
    body = grating_state(1) + grating_state(0)
    rules = direction_rule(1, rule_targets[0]) + direction_rule(0, rule_targets[1])
    return pre + sm + body + rules + obj("AtomStateMachineNode", bytes(16), first=1), len(pre)


class Conditions(unittest.TestCase):
    NAMES = {0: "Flag", 1: "Height", 2: "Direction"}

    def parse(self, raw: bytes) -> list[dict]:
        return [A.condition(raw, o) for o, _ in A.objects(raw, only="AtomCondition")]

    def test_operator_codes_follow_the_runtime_switch(self):
        for op, text in A.OPS.items():
            with self.subTest(op=op):
                c = self.parse(cond(PARAM_HEIGHT, op=op, vtype=1, value=1.5))
                self.assertEqual(A.expression(c, self.NAMES), f"Height {text} 1.5")

    def test_values_by_wrapper_type(self):
        c = self.parse(cond(PARAM_DIR, vtype=2, value=-3) + cond(PARAM_BOOL, vtype=0, value=1) +
                       cond(PARAM_HEIGHT, vtype=1, value=0.25))
        self.assertEqual([x["value"] for x in c], [-3, True, 0.25])
        self.assertEqual([x["value_type"] for x in c], ["int", "bool", "scalar"])

    def test_groups_chain_left_to_right_with_combine_on_the_previous_entry(self):
        # (H >= 170 and H < 180) or (H < -158 and H >= -180), as in the window machine's Dn state
        raw = (cond(kind=2, count=2, first=0) +
               cond(kind=2, count=2, combine=1) +
               cond(PARAM_HEIGHT, op=2, vtype=1, value=170.0) + cond(PARAM_HEIGHT, op=3, vtype=1, value=180.0) +
               cond(kind=2, count=2, combine=1) +
               cond(PARAM_HEIGHT, op=3, vtype=1, value=-158.0) + cond(PARAM_HEIGHT, op=2, vtype=1, value=-180.0))
        self.assertEqual(A.expression(self.parse(raw), self.NAMES),
                         "((Height >= 170.0 and Height < 180.0) or (Height < -158.0 and Height >= -180.0))")

    def test_opaque_kinds_are_shown_raw(self):
        c = self.parse(cond(0x0040001A, kind=1, b=0))
        self.assertEqual(A.expression(c, self.NAMES), "event(0x0040001a)")

    def test_group_running_past_the_list_fails(self):
        with self.assertRaises(A.FormatError):
            A.expression(self.parse(cond(kind=2, count=2, first=0) + cond(PARAM_DIR, vtype=2, value=1)), self.NAMES)

    def test_unknown_operator_and_combine_fail(self):
        with self.assertRaises(A.FormatError):
            self.parse(cond(PARAM_HEIGHT, op=9, vtype=1, value=1.0))
        with self.assertRaises(A.FormatError):
            self.parse(cond(PARAM_HEIGHT, combine=5, vtype=1, value=1.0))

    def test_missing_wrapper_fails(self):
        raw = cond(PARAM_HEIGHT, vtype=1, value=1.0)
        broken = raw[:41] + bytes(13) + raw[54:]
        with self.assertRaises(A.FormatError):
            A.condition(broken, 0)


class Machines(unittest.TestCase):
    def test_params_in_file_order_with_crc_check(self):
        raw = params_block()
        self.assertEqual([(p["index"], p["name"], p["type"]) for p in A.params(raw)],
                         [(0, "Flag", "bool"), (1, "Height", "scalar"), (2, "Direction", "int")])
        bad = bytearray(raw)
        bad[-1] ^= 0xFF
        with self.assertRaises(A.FormatError):
            A.params(bytes(bad))

    def test_states_rules_interval_and_if(self):
        raw, at = two_state_machine()
        names = {p["index"]: p["name"] for p in A.params(raw)}
        states = A.machine(raw, at)
        self.assertEqual(len(states), 2)
        out = A.describe(states, names, {"000000000000000a": "ClipA", "000000000000000b": "ClipB"})
        self.assertEqual([s["entry"] for s in out], ["Direction == 0", "Direction == 1"])
        sel = out[0]["output"][0]
        self.assertEqual(sel["select_by"], {"param": "Height"})
        self.assertEqual(sel["below"], [{"threshold": -1.0, "value": "ClipA [mirror=1]"}])
        self.assertEqual(sel["otherwise"], {"if": {"condition": "Height >= 0.2"},
                                            "then": "ClipB [mirror=1]", "else": "ClipA [mirror=1]"})
        self.assertEqual(out[1]["output"][0]["otherwise"]["then"], "ClipB [mirror=0]")

    def test_rules_for_an_enclosing_machine_are_left_to_it(self):
        pre = params_block()
        inner = (obj("AtomStateMachineNode", bytes(16)) + grating_state(0) + grating_state(1) +
                 direction_rule(0, 0) + direction_rule(1, 1))
        outer = (obj("AtomStateMachineNode", bytes(16)) + grating_state(0) + inner +
                 direction_rule(2, 1) + direction_rule(3, 0))
        top = obj("AtomStateMachineNode", bytes(16)) + grating_state(0) + outer + direction_rule(4, 1) + \
            direction_rule(5, 0)
        raw = pre + top
        names = {p["index"]: p["name"] for p in A.params(raw)}
        out = A.describe(A.machine(raw, len(pre)), names, {})
        self.assertEqual(out[0]["entry"], "Direction == 5")
        self.assertEqual(out[1]["entry"], "Direction == 4")
        mid = out[1]["machine"]
        self.assertEqual([s["entry"] for s in mid], ["Direction == 3", "Direction == 2"])
        self.assertEqual([s["entry"] for s in mid[1]["machine"]], ["Direction == 0", "Direction == 1"])

    def test_empty_rule_is_always_and_takes_its_target_byte(self):
        sm = obj("AtomStateMachineNode", bytes(16)) + obj("AtomNodeID", bytes(16))
        states = grating_state(0) + grating_state(1)
        raw = params_block() + sm + states + direction_rule(1, 1) + initial_always(0)
        at = len(params_block())
        parsed = A.machine(raw, at)
        self.assertTrue(parsed[0].has_entry)
        self.assertEqual(parsed[0].entry_conditions, [])
        self.assertTrue(parsed[1].entry_conditions)

    def test_parameter_list_must_match_the_compared_parameters(self):
        pre = params_block()
        bad = initial(cond(kind=2, count=1, first=0) + cond(PARAM_DIR, vtype=2, value=1), [PARAM_HEIGHT], 0)
        raw = pre + obj("AtomStateMachineNode", bytes(16)) + grating_state(0) + bad
        with self.assertRaises(A.FormatError):
            A.machine(raw, len(pre))

    def test_link_out_of_range_fails(self):
        pre = params_block()
        raw = pre + obj("AtomStateMachineNode", bytes(16)) + graph_state([clip(1), root(7)]) + direction_rule(0, 0)
        states = A.machine(raw, len(pre))
        with self.assertRaises(A.FormatError):
            A.describe(states, {}, {})

    def test_not_a_machine(self):
        with self.assertRaises(A.FormatError):
            A.machine(params_block(), 0)


NEWDEMO = ROOT / "local" / "atomgraph" / "NewDemo.bin"


@unittest.skipUnless(NEWDEMO.exists(), "install-dependent: local/atomgraph/NewDemo.bin not present (unavailable)")
class NewDemoWindowClimb(unittest.TestCase):
    """Checks the decoder against values read by hand from the bytes (ACU-RE-0063)."""

    def test_climb_window_machine(self):
        b = NEWDEMO.read_bytes()
        names = {p["index"]: p["name"] for p in A.params(b)}
        self.assertEqual((names[197], names[261], names[450], names[455]),
                         ("Direction", "HeightDifferential", "SyncA_TargetAngle", "SyncB_TargetHeight"))
        out = A.describe(A.machine(b, 37005472), names, {})
        self.assertEqual([s["entry"] for s in out],
                         ["Direction == 2", "Direction == 3", "Direction == 1", "Direction == 0"])
        sel = out[2]["output"][0]
        self.assertEqual(sel["select_by"], {"param": "SyncB_TargetHeight"})
        self.assertEqual([x["threshold"] for x in sel["below"]], [-2.0, -1.25, -0.3])
        self.assertEqual(sel["below"][1]["value"]["if"], {"condition": "HeightDifferential >= 0.2"})
        self.assertEqual(sel["below"][1]["value"]["then"], "00000029595e4e0e [mirror=1]")
        self.assertEqual(json.dumps(out[3]).count("[mirror=1]"), 0)


if __name__ == "__main__":
    unittest.main()
