"""Unit tests for capture_run's multi-instance launch selection and foreground guard.

Everything that would touch a real process or window is mocked: no game is launched, no window is
enumerated and Ubisoft Connect is never asked to start anything."""
from __future__ import annotations

import contextlib
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import capture_run  # noqa: E402


class LaunchTest(unittest.TestCase):
    def setUp(self):
        self.output = Path(tempfile.mkdtemp())
        self.run = capture_run.Run(self.output)
        self.started = []  # commands passed to subprocess.run (the uplay start only)
        self.windows = [(10, 0xA0)]  # another session's game, already running
        self.running = [10]
        self.after_start = [(10, 0xA0)]

        def fake_run(cmd, **_):
            self.started.append(cmd)
            self.windows = list(self.after_start)  # what appears once Ubisoft Connect reacts
            self.running = sorted({pid for pid, _ in self.windows})
            return mock.Mock(returncode=0)

        self.patches = [
            mock.patch.object(capture_run.subprocess, "run", fake_run),
            mock.patch.object(capture_run, "engine_windows", lambda: list(self.windows)),
            mock.patch.object(capture_run, "acu_running", lambda: list(self.running)),
            mock.patch.object(capture_run.save_pin, "file_lock", lambda *a, **k: contextlib.nullcontext()),
            mock.patch.object(capture_run.time, "sleep", lambda _: None),
        ]
        for p in self.patches:
            p.start()

    def tearDown(self):
        for p in self.patches:
            p.stop()

    def test_returns_the_new_pid_not_the_running_game(self):
        self.after_start = [(10, 0xA0), (20, 0xB0)]
        self.assertEqual(capture_run.launch(self.run, timeout=5), (20, 0xB0))
        self.assertEqual(self.started, [["cmd", "/c", "start", "", "uplay://launch/720/0"]])
        stage = self.run.record["stages"][-1]
        self.assertEqual((stage["stage"], stage["pid"], stage["already_running"]), ("launch", 20, [10]))

    def test_new_pid_listed_first_still_found(self):
        self.after_start = [(30, 0xC0), (10, 0xA0)]
        self.assertEqual(capture_run.launch(self.run, timeout=5)[0], 30)

    def test_no_second_instance_fails_without_touching_the_game(self):
        with self.assertRaises(RuntimeError) as raised:
            capture_run.launch(self.run, timeout=0.01)
        self.assertIn("did not start a second instance", str(raised.exception))
        self.assertIn("10", str(raised.exception))
        self.assertEqual(len(self.started), 1)  # only the uplay start: no taskkill, nothing else
        self.assertEqual(self.run.record["stages"], [])

    def test_first_instance_timeout_keeps_the_old_message(self):
        self.windows, self.running, self.after_start = [], [], []
        with self.assertRaises(RuntimeError) as raised:
            capture_run.launch(self.run, timeout=0.01)
        self.assertIn("no ACU engine window", str(raised.exception))

    def test_two_new_instances_are_refused(self):
        self.after_start = [(10, 0xA0), (20, 0xB0), (21, 0xB1)]
        with self.assertRaises(RuntimeError) as raised:
            capture_run.launch(self.run, timeout=5)
        self.assertIn("[20, 21]", str(raised.exception))

    def test_one_new_pid_with_two_windows_is_fine(self):
        self.after_start = [(10, 0xA0), (20, 0xB0), (20, 0xB1)]
        self.assertEqual(capture_run.launch(self.run, timeout=5), (20, 0xB0))

    def test_launch_holds_the_launch_lock(self):
        held = []

        @contextlib.contextmanager
        def lock(path, **_):
            held.append(path)
            yield

        self.after_start = [(10, 0xA0), (20, 0xB0)]
        with mock.patch.object(capture_run.save_pin, "file_lock", lock):
            capture_run.launch(self.run, timeout=5)
        self.assertEqual(held, [capture_run.LAUNCH_LOCK])


class ReturnFocusTest(unittest.TestCase):
    def check(self, foreground: int, owner: int, running: list[int]) -> int:
        fake_user = mock.Mock()
        fake_user.GetForegroundWindow.return_value = foreground
        with mock.patch.object(capture_run, "user", fake_user), \
                mock.patch.object(capture_run, "window_pid", lambda hwnd: owner), \
                mock.patch.object(capture_run, "acu_running", lambda: running):
            return capture_run.return_focus_window()

    def test_user_window_is_returned(self):
        self.assertEqual(self.check(0x123, owner=500, running=[10]), 0x123)

    def test_another_sessions_game_is_never_given_focus(self):
        self.assertEqual(self.check(0x456, owner=10, running=[10]), 0)

    def test_no_foreground_window(self):
        self.assertEqual(self.check(0, owner=0, running=[]), 0)


if __name__ == "__main__":
    unittest.main()
