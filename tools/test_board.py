import unittest
from datetime import datetime, timezone

import board

NOW = datetime(2026, 10, 22, 12, 0, tzinfo=timezone.utc)


def c(body, at):
    return {"body": body, "createdAt": at}


class LockTests(unittest.TestCase):
    def test_free_when_empty(self):
        self.assertIsNone(board.lock_state([], NOW))

    def test_acquire_then_release(self):
        cs = [c("ACQUIRE a pid=1 until=x", "2026-10-22T11:50:00Z"), c("RELEASE a", "2026-10-22T11:55:00Z")]
        self.assertIsNone(board.lock_state(cs, NOW))

    def test_held_and_fresh(self):
        h = board.lock_state([c("ACQUIRE a pid=7", "2026-10-22T11:40:00Z")], NOW)
        self.assertEqual((h["name"], h["pid"], h["stale"]), ("a", "7", False))

    def test_heartbeat_refreshes(self):
        cs = [c("ACQUIRE a pid=7", "2026-10-22T10:00:00Z"), c("HEARTBEAT a pid=7", "2026-10-22T11:45:00Z")]
        self.assertFalse(board.lock_state(cs, NOW)["stale"])

    def test_stale_without_heartbeat(self):
        h = board.lock_state([c("ACQUIRE a pid=7", "2026-10-22T10:00:00Z")], NOW)
        self.assertTrue(h["stale"])

    def test_second_acquire_ignored_while_fresh(self):
        cs = [c("ACQUIRE a pid=1", "2026-10-22T11:50:00Z"), c("ACQUIRE b pid=2", "2026-10-22T11:55:00Z")]
        self.assertEqual(board.lock_state(cs, NOW)["name"], "a")

    def test_takeover_of_stale(self):
        cs = [c("ACQUIRE a pid=1", "2026-10-22T09:00:00Z"), c("ACQUIRE b pid=2", "2026-10-22T11:55:00Z")]
        self.assertEqual(board.lock_state(cs, NOW)["name"], "b")

    def test_foreign_release_ignored(self):
        cs = [c("ACQUIRE a pid=1", "2026-10-22T11:50:00Z"), c("RELEASE b", "2026-10-22T11:55:00Z")]
        self.assertEqual(board.lock_state(cs, NOW)["name"], "a")

    def test_non_lock_comments_ignored(self):
        self.assertIsNone(board.parse_lock_comment("hello world"))
        self.assertIsNone(board.parse_lock_comment(""))


class OtherTests(unittest.TestCase):
    def test_stale_claims(self):
        issues = [
            {"number": 1, "labels": ["status:claimed"], "updatedAt": "2026-10-18T00:00:00Z"},
            {"number": 2, "labels": ["status:claimed"], "updatedAt": "2026-10-22T00:00:00Z"},
            {"number": 3, "labels": ["status:ready"], "updatedAt": "2026-10-01T00:00:00Z"},
        ]
        self.assertEqual(board.stale_claims(issues, NOW), [1])

    def test_mentions(self):
        since = datetime(2026, 10, 22, 0, 0, tzinfo=timezone.utc)
        cs = [c("@bob please check #5", "2026-10-22T08:00:00Z"),
              c("@bob old", "2026-10-21T08:00:00Z"),
              c("CLAIM bob @bob", "2026-10-22T09:00:00Z"),
              c("nothing", "2026-10-22T10:00:00Z")]
        self.assertEqual([m["body"] for m in board.mentions(cs, "bob", since)], ["@bob please check #5"])


if __name__ == "__main__":
    unittest.main()
