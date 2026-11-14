"""Tests for tools/oracle_export.py trip segmentation (synthetic sampler rows; no game data)."""
from __future__ import annotations

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


if __name__ == "__main__":
    unittest.main()
