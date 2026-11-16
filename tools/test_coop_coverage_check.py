"""Synthetic tests for tools/coop_coverage_check.py (no game data needed)."""
from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import coop_coverage_check as ccc  # noqa: E402

UNIVERSE = {"systems": {
    "combat": {"messages": ["Fight_AMSG", "Fight_BMSG"], "states": ["Fight_State"], "objects": ["NetFight"]},
    "test": {"messages": ["UnitTest_RFC_X"], "states": [], "objects": []},
}}
PAYLOADS = {"messages": [{"name": "Fight_AMSG", "fields": [{"t": "u32"}]}, {"name": "Fight_BMSG", "fields": []},
                         {"name": "UnitTest_RFC_X", "fields": [{"t": "u8"}]}]}
HANDLERS = {"messages": [{"name": "Fight_AMSG", "handler": "0x1"}, {"name": "Fight_BMSG", "handler": "0x2"},
                         {"name": "UnitTest_RFC_X", "handler": "0x3"}]}
MEMENTOS = {"mementos": [{"name": "Fight_State", "status": "decoded", "bind": "0x9"}]}
KNOWN = {"ACU-RE-0001", "ACU-RE-0002"}


def verdicts(**over):
    vf = {
        "items": {
            "Fight_AMSG": {"verdict": "port", "behaviour": ["ACU-RE-0001"]},
            "Fight_BMSG": {"verdict": "port", "behaviour": ["ACU-RE-0001"]},
            "Fight_State": {"verdict": "port", "behaviour": ["ACU-RE-0001"]},
            "NetFight": {"verdict": "engine-local", "fields": "ACU-RE-0002", "handler": "ACU-RE-0002",
                         "behaviour": ["ACU-RE-0002"]},
        },
        "patterns": [{"match": "UnitTest_RFC_*", "verdict": "diagnostic", "behaviour": ["ACU-RE-0002"]}],
        "systems": {"combat": {"trace": None, "replay": False, "shadow": False}},
        "cross_cutting": [{"id": "x", "finding": "ACU-RE-0001"}],
        "checklist": [{"id": "y", "finding": None}],
        "adapter": {"closed": False, "calls": [{"name": "c", "acu_reference": "r", "finding": "ACU-RE-0001"}]},
    }
    vf.update(over)
    return vf


class CheckTests(unittest.TestCase):
    def run_build(self, vf, root=None):
        return ccc.build(UNIVERSE, PAYLOADS, HANDLERS, MEMENTOS, vf, KNOWN, root or Path("."))

    def test_evidence_and_gaps(self):
        res = self.run_build(verdicts())
        items = {i["name"]: i for i in res["items"]}
        self.assertFalse(items["Fight_BMSG"]["fields"])            # empty field list counts as unknown
        self.assertFalse(items["Fight_AMSG"]["done"])              # port without a trace
        self.assertTrue(items["UnitTest_RFC_X"]["done"])           # pattern verdict, diagnostic needs no trace
        self.assertTrue(items["NetFight"]["done"])                 # object evidence from finding overrides
        self.assertEqual(res["summary"]["C_port_system_traces"]["port_systems"], ["combat"])
        self.assertEqual(res["summary"]["B_checklist"]["closed"], 0)
        self.assertFalse(res["complete"])
        self.assertEqual(res["errors"], [])

    def test_unknown_finding_and_verdict_are_errors(self):
        vf = verdicts()
        vf["items"]["Fight_AMSG"] = {"verdict": "maybe", "behaviour": ["ACU-RE-0999"]}
        res = self.run_build(vf)
        self.assertTrue(any("unknown verdict" in e for e in res["errors"]))
        self.assertTrue(any("ACU-RE-0999" in e for e in res["errors"]))

    def test_complete_when_everything_closed(self):
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / "trace.jsonl").write_text("{}\n", encoding="utf-8")
            vf = verdicts(systems={"combat": {"trace": "trace.jsonl", "replay": True, "shadow": True}},
                          checklist=[{"id": "y", "finding": "ACU-RE-0002"}],
                          adapter={"closed": True, "calls": [{"name": "c", "acu_reference": "r",
                                                              "finding": "ACU-RE-0001"}]})
            vf["items"]["Fight_BMSG"]["fields"] = "ACU-RE-0001"
            res = self.run_build(vf, Path(d))
            self.assertEqual(res["errors"], [])
            self.assertTrue(res["complete"], res["summary"])

    def test_missing_trace_file_is_reported(self):
        vf = verdicts(systems={"combat": {"trace": "nope.jsonl", "replay": False, "shadow": False}})
        res = self.run_build(vf, Path(tempfile.gettempdir()))
        self.assertTrue(any("not found locally" in e for e in res["errors"]))

    def test_markdown_block(self):
        md = ccc.markdown(self.run_build(verdicts()))
        self.assertTrue(md.startswith(ccc.BEGIN) and md.endswith(ccc.END))
        self.assertIn("Complete: no", md)


if __name__ == "__main__":
    unittest.main()
