"""Tests for tools/oracle_export.py (synthetic sampler, trace and timeline rows; no game data)."""
from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import oracle_export as ox  # noqa: E402

PARIS, VERSAILLES, PORTAL = 0x0000000F18B7E1CF, 0x0000000E84B48064, 0x0000001770D432BE


def row(t, h90, h90_id, world, player, state, q=0, target=PARIS, dest=PORTAL, player_world=None, pos=None):
    return {"t": t, "h90": h90, "h90_id": h90_id, "world": world, "player": player, "mgr644": state,
            "q_count": q, "mgr592_id": target if state else 0, "mgr600_id": dest,
            "player_world": player_world if player_world is not None else world, "pos": pos}


class SegmentTrips(unittest.TestCase):
    def rows(self):
        return [
            row(0.0, 0xA, VERSAILLES, 0x100, 0x200, 0),
            row(10.0, 0xA, VERSAILLES, 0x100, 0x200, 1),
            row(11.5, 0xA, VERSAILLES, 0x100, 0x200, 1, q=2),
            row(11.6, ox.NULL_HANDLE, 0, 0, 0, 1, q=2),
            row(14.5, 0xB, PARIS, 0x300, 0, 1),
            row(33.0, 0xB, PARIS, 0x300, 0x400, 1, pos=[768.55, -46.65, -0.44]),
            row(38.0, 0xB, PARIS, 0x300, 0x400, 0),
        ]

    def test_one_trip_timeline_and_relations(self):
        trips = ox.segment_trips(self.rows())
        self.assertEqual(len(trips), 1)
        trip = trips[0]
        self.assertEqual(trip["from_world"], "0000000e84b48064")
        self.assertEqual(trip["to_world"], "0000000f18b7e1cf")
        self.assertEqual(trip["destination_name"], "PlayerPortal to Paris")
        self.assertEqual(trip["events_s"], {"queue_commands_pending": 1.5, "transition_world": 1.6,
                                            "target_world_object": 4.5, "player_spawned": 23.0,
                                            "travel_state_idle": 28.0})
        self.assertEqual(trip["arrival_position"], [768.55, -46.65, -0.44])
        self.assertTrue(all(trip["relations"].values()))

    def test_player_without_world_is_flagged(self):
        rows = self.rows()
        rows[3] = row(11.6, ox.NULL_HANDLE, 0, 0, 0x999, 1, q=2)  # a player while no World exists
        relations = ox.segment_trips(rows)[0]["relations"]
        self.assertFalse(relations["no_player_while_world_absent"])

    def test_unfinished_trip_is_dropped(self):
        self.assertEqual(ox.segment_trips(self.rows()[:-1]), [])

    def test_handle_stability(self):
        self.assertEqual(ox.world_handles_stable(self.rows()), {"ACU_Versailles": True, "ACU_Paris": True})
        rows = self.rows() + [row(50.0, 0xC, PARIS, 0x500, 0x600, 0)]
        self.assertEqual(ox.world_handles_stable(rows)["ACU_Paris"], False)


FREQ = 10_000_000
OFFSET = 1_000_000.0  # synthetic Unix - QPC/f offset


def session(event_qpcs_and_latency):
    """Probe session rows: one events batch per (qpc, poll latency in s)."""
    rows = [{"kind": "header", "status": {"qpc_frequency": FREQ}}]
    for q, lat in event_qpcs_and_latency:
        rows.append({"kind": "events", "time_unix": q / FREQ + OFFSET + lat, "events": [{"qpc": q}]})
    return rows


def timeline(t0_unix, steps):
    rows, t = [], t0_unix
    for i, (name, dur, state) in enumerate(steps):
        rows.append({"unix_time": t, "kind": "step_start", "index": i, "name": name,
                     "planned_duration_ms": dur * 1000, "state": {"name": name, **state}})
        rows.append({"unix_time": t, "kind": "xinput_readback", "index": i, "name": name})
        t += dur
        rows.append({"unix_time": t, "kind": "step_end", "index": i, "name": name, "actual_duration_ms": dur * 1000})
    return rows


def sample(t_s, pos, fwd, clips, entity="0x1234"):
    return {"kind": "sample", "qpc": int((t_s - OFFSET) * FREQ), "entity": entity, "position": pos, "forward": fwd,
            "clips": [{"row": r, "file_id": f, "weight": w, "time": tm, "offset": 0.0} for f, r, w, tm in clips]}


IDLE, RUN, LEAP = "00000003836e1d49", "0000000f9f512072", "0000000fd77ee848"


class Helpers(unittest.TestCase):
    def test_clock_offset_is_minimum_over_events(self):
        off = ox.clock_offset(session([(1_000, 0.2), (2_000, 0.004), (3_000, 0.05)]))
        self.assertAlmostEqual(off["offset_s"], OFFSET + 0.004, places=6)
        self.assertEqual(off["events"], 3)

    def test_f32_shortest_round_trip(self):
        import struct
        v = struct.unpack("<f", struct.pack("<f", 0.9917837381362915))[0]
        out = ox.f32(v)
        self.assertEqual(struct.pack("<f", out), struct.pack("<f", v))
        self.assertLessEqual(len(repr(out)), 11)
        self.assertIsNone(ox.f32(float("nan")))

    def test_heading(self):
        self.assertEqual(ox.heading_deg([0.2588, 0.9659, 0.0]), 75.0)
        self.assertEqual(ox.heading_deg([0.0, -1.0, 0.0]), -90.0)
        self.assertIsNone(ox.heading_deg([0.0, 0.0, 1.0]))

    def test_dump_is_valid_json_with_one_record_per_line(self):
        data = {"a": 1, "runs": [{"run": "x", "samples": [{"t": 0, "p": [1, 2]}, {"t": 1, "p": [3, 4]}]}]}
        text = ox.dump(data)
        self.assertEqual(json.loads(text), data)
        self.assertIn('    {"t":0,"p":[1,2]},', text.splitlines())


class Locomotion(unittest.TestCase):
    def build(self):
        t0 = 2_000_000.0
        tl = timeline(t0, [("settle", 1.0, {}), ("walk_forward", 2.0, {"left_stick": [0, 32767]}),
                           ("free_run", 1.0, {"left_stick": [0, 32767], "right_trigger": 255, "buttons": ["A"]})])
        samples = [sample(t0 - 0.5, [0, 0, 0], [0, 1, 0], [(IDLE, 6, 1.0, 3.0)]),
                   sample(t0 + 0.5, [0, 0, 0], [0, 1, 0], [(IDLE, 6, 1.0, 4.0)]),
                   sample(t0 + 1.5, [0, 1, 0], [0, 1, 0], [(RUN, 2, 0.7, 0.4), (IDLE, 6, 0.3, 5.0)]),
                   sample(t0 + 2.5, [0, 3, 0.1], [1, 0, 0], [(RUN, 2, 1.0, 1.4)]),
                   sample(t0 + 3.5, [0, 7, 0.1], [1, 0, 0], [(RUN, 2, 1.0, 0.1)]),
                   {"kind": "sample", "qpc": int((t0 + 3.6 - OFFSET) * FREQ), "player": None}]
        off = ox.clock_offset(session([(int((t0 - OFFSET) * FREQ), 0.0)]))
        return ox.locomotion_run(samples, {"qpc_frequency": FREQ, "hz": 30.0}, tl, off)

    def test_samples_are_timed_from_first_step_and_tagged_with_inputs(self):
        run = self.build()
        self.assertEqual([s["t"] for s in run["samples"]], [-0.5, 0.5, 1.5, 2.5, 3.5])
        self.assertEqual([s["step"] for s in run["samples"]], [None, 0, 1, 1, 2])
        self.assertEqual(run["sampler"]["samples_without_player"], 1)
        self.assertTrue(run["sampler"]["player_entity_constant"])
        self.assertEqual(run["samples"][2]["clips"], [[RUN, 2, 0.7, 0.4, 0.0], [IDLE, 6, 0.3, 5.0, 0.0]])
        self.assertEqual(run["samples"][3]["heading_deg"], 0.0)

    def test_inputs_and_step_summaries(self):
        run = self.build()
        self.assertEqual(run["inputs"][2]["buttons"], ["A"])
        self.assertEqual(run["inputs"][2]["right_trigger"], 255)
        self.assertEqual(run["inputs"][0]["left_stick"], [0, 0])
        walk = next(s for s in run["step_summaries"] if s["name"] == "walk_forward")
        self.assertEqual(walk["net_xy_m"], 2.0)
        self.assertEqual(walk["heading_change_deg"], -90.0)
        self.assertEqual(walk["mean_xy_speed"], 2.0)

    def test_clip_starts_new_id_and_time_reset(self):
        starts = self.build()["clip_starts"]
        # RUN starts at t=1.5-0.4; RUN restarts (time went backwards) at t=3.5-0.1; idle playing at start is not a start.
        self.assertEqual([(s["file_id"], s["t"]) for s in starts], [(RUN, 1.1), (RUN, 3.4)])

    def test_no_addresses_leak(self):
        text = json.dumps(self.build())
        self.assertNotIn("0x1234", text)
        self.assertNotIn("entity\":", text)


def cand(ptr, enum, fit, dist):
    return {"ptr": ptr, "bytes": 672, "fancy_table": "0000000143a35ff0", "enum": enum, "fitness": fit,
            "distance": dist, "unverified_acufixes": {"height_difference": 1.5, "horizontal_difference": 0.25}}


class Parkour(unittest.TestCase):
    def decisions(self, q0):
        a, b, c = cand("00000001aaaa0000", 3, 0.994, 1.0), cand("00000001bbbb0000", 8, 3.58, 5.5), \
            cand("00000001cccc0000", 53, 0.0, 0.5)
        return [
            {"qpc": q0 + 2 * FREQ, "tid": 77, "tester": "0000000156152a00", "array": "00000000043beaa0",
             "considered_count": 3, "considered": [a, b, c], "ranked_count": 2, "ranked": [a, b],
             "selected_index": 1, "selected": b},
            {"qpc": q0 + 1 * FREQ, "tid": 78, "tester": "0000000156152a00", "considered_count": 1,
             "considered": [c], "ranked_count": 0, "ranked": [], "selected_index": -1, "selected": None},
            {"qpc": q0 + 3 * FREQ, "tid": 77, "tester": "000000015e052af0", "considered_count": 40,
             "considered": [a], "ranked_count": 30, "ranked": [a], "selected_index": 20, "selected": None},
            {"qpc": q0 + 4 * FREQ, "tid": 77, "ranked_count": 1, "ranked": [a], "selected_index": 0, "selected": a},
        ]

    def test_decision_encoding_without_addresses(self):
        q0 = 5 * FREQ
        run = ox.parkour_run(self.decisions(q0), q0, FREQ)
        d = run["decisions"]
        self.assertEqual([x["t"] for x in d], [1.0, 2.0, 3.0, 4.0])  # sorted by entry time
        sel = d[1]
        self.assertEqual(sel["considered"], [0, 1, 2])
        self.assertEqual(sel["ranked"], [0, 1])  # same candidates referenced, not duplicated
        self.assertEqual(len(sel["candidates"]), 3)
        self.assertEqual(sel["candidates"][1], [8, 3.58, 5.5, 1.5, 0.25])
        self.assertEqual(sel["action_type"], 8)
        self.assertEqual((d[0]["thread"], d[1]["thread"]), (0, 1))  # ordinals in time order (tid 78, then 77)
        self.assertEqual((d[1]["tester"], d[2]["tester"]), (0, 1))
        self.assertTrue(d[2]["selected_outside_gather"])
        self.assertIsNone(d[2]["action_type"])
        self.assertTrue(d[3]["entry_missing"])
        self.assertIsNone(d[3]["considered"])
        self.assertEqual(run["counts"]["chosen_action_types"], {"3": 1, "8": 1})
        self.assertEqual(run["counts"]["selected_outside_gather"], 1)
        text = json.dumps(run)
        for addr in ("aaaa", "0000000156152a00", "043beaa0", "43a35ff0", "ptr"):
            self.assertNotIn(addr, text)

    def test_steps_player_and_clips_after(self):
        q0 = 5 * FREQ
        t0_unix = q0 / FREQ + OFFSET
        steps = ox.input_steps(timeline(t0_unix, [("settle", 1.5, {}), ("up", 5.0, {"buttons": ["A"]})]), t0_unix)
        samples = [sample(t0_unix + 0.0, [0, 0, 0], [0, 1, 0], [(IDLE, 6, 1.0, 1.0)]),
                   sample(t0_unix + 1.98, [0, 1, 0], [0, 1, 0], [(IDLE, 6, 1.0, 2.98)]),
                   sample(t0_unix + 2.6, [0, 2, 0], [0, 1, 0], [(LEAP, 3, 0.9, 0.3)]),  # starts at t=2.3
                   sample(t0_unix + 4.5, [0, 3, 0], [0, 1, 0], [(LEAP, 3, 1.0, 2.2)])]
        run = ox.parkour_run(self.decisions(q0)[:2], q0, FREQ, steps, samples)
        early, chosen = run["decisions"]
        self.assertEqual((early["step"], chosen["step"]), (0, 1))
        self.assertEqual(chosen["player"]["position"], [0, 1, 0])
        self.assertAlmostEqual(chosen["player"]["sample_dt_s"], -0.02, places=3)
        self.assertEqual(chosen["clips_after"], [[LEAP, 0.3, 0.9]])
        self.assertNotIn("clips_after", early)  # only selecting decisions are paired

    def test_trial_windows_and_tags(self):
        q0 = 0
        raw = [{"repeat": 0, "theta_deg": -20.0, "psi_deg": 55.5, "goto": {"reached": False}, "skipped": "x"},
               {"repeat": 0, "theta_deg": 0.0, "psi_deg": 75.5, "qpc_start": 1 * FREQ, "qpc_end": 4 * FREQ,
                "qpc_settled": 5 * FREQ, "run": {"start": [1.0, 2.0, 0.0], "end": [3.0, 4.0, 0.5], "path": []}}]
        trials = ox.trial_windows(raw, "sweep", q0, FREQ)
        self.assertEqual(len(trials), 1)
        self.assertEqual(trials[0]["trial"], 1)
        self.assertEqual(trials[0]["commanded_world_yaw_deg"], 75.5)
        self.assertEqual(ox.trial_at(trials, 2.0), [1, "run"])
        self.assertEqual(ox.trial_at(trials, 4.5), [1, "settle"])
        self.assertIsNone(ox.trial_at(trials, 6.0))
        scan = ox.trial_windows([{"origin": 2, "yaw_deg": 45.0, "qpc_start": 0, "qpc_end": FREQ,
                                  "start": [0, 0, 0], "end": [1, 1, 0],
                                  "label": {"file_id": "00000012fd65edab", "name": "prop_002"}}], "scan", 0, FREQ)
        self.assertEqual(scan[0]["prop_file_id"], "00000012fd65edab")
        self.assertEqual(scan[0]["origin"], 2)


if __name__ == "__main__":
    unittest.main()
