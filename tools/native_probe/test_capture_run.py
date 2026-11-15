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
            mock.patch.object(capture_run.instance_guard, "release", self.fake_release),
        ]
        self.released = []
        for p in self.patches:
            p.start()

    def tearDown(self):
        for p in self.patches:
            p.stop()

    def fake_release(self, pid):
        self.released.append(pid)
        return [{"handle": "0x3a4", "type": "Semaphore", "name": r"\Sessions\1\BaseNamedObjects\scimitar_semaphore"}]

    def test_returns_the_new_pid_not_the_running_game(self):
        self.after_start = [(10, 0xA0), (20, 0xB0)]
        self.assertEqual(capture_run.launch(self.run, timeout=5), (20, 0xB0))
        self.assertEqual(self.started, [["cmd", "/c", "start", "", "uplay://launch/720/0"]])
        stage, guard = self.run.record["stages"][-2:]
        self.assertEqual((stage["stage"], stage["pid"], stage["already_running"]), ("launch", 20, [10]))
        self.assertEqual((guard["stage"], guard["pid"]), ("guard", 20))
        self.assertEqual(self.released, [20])  # the uplay game is this run's own too

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


class DirectLaunchTest(LaunchTest):
    """CreateProcess of ACU.exe: the spawned PID is the game; a guard held elsewhere is refused."""

    def setUp(self):
        super().setUp()
        self.holders = []
        self.spawned = mock.Mock(pid=20, returncode=None)
        self.spawned.poll.return_value = None

        def fake_spawn(exe):
            self.started.append(["spawn", str(exe)])
            self.windows = list(self.after_start)
            self.running = sorted({pid for pid, _ in self.windows})
            return self.spawned

        for p in (mock.patch.object(capture_run, "spawn_direct", fake_spawn),
                  mock.patch.object(capture_run, "game_exe", lambda: Path("D:/Game/ACU.exe")),
                  mock.patch.object(capture_run.instance_guard, "holders", lambda pids: list(self.holders))):
            p.start()
            self.patches.append(p)

    def test_returns_the_new_pid_not_the_running_game(self):
        self.after_start = [(10, 0xA0), (20, 0xB0)]
        self.assertEqual(capture_run.launch(self.run, timeout=5, direct=True), (20, 0xB0))
        self.assertEqual(self.started, [["spawn", str(Path("D:/Game/ACU.exe"))]])
        launch, guard = self.run.record["stages"][-2:]
        self.assertEqual((launch["stage"], launch["method"], launch["already_running"]), ("launch", "direct", [10]))
        self.assertEqual((guard["stage"], guard["pid"], guard["released"]), ("guard", 20, True))
        self.assertEqual(self.released, [20])  # only the new game, never PID 10

    def test_running_guard_holder_is_refused_untouched(self):
        self.holders = [10]
        with self.assertRaises(RuntimeError) as raised:
            capture_run.launch(self.run, timeout=5, direct=True)
        self.assertIn("[10]", str(raised.exception))
        self.assertEqual((self.started, self.released, self.run.record["stages"]), ([], [], []))

    def test_spawned_exit_is_reported(self):
        self.spawned.poll.return_value = 0
        self.spawned.returncode = 0
        with self.assertRaises(RuntimeError) as raised:
            capture_run.launch(self.run, timeout=5, direct=True)
        self.assertIn("exited with code 0", str(raised.exception))
        self.assertEqual(self.released, [])

    def test_window_from_another_pid_is_refused(self):
        self.after_start = [(10, 0xA0), (33, 0xC0)]
        with self.assertRaises(RuntimeError) as raised:
            capture_run.launch(self.run, timeout=5, direct=True)
        self.assertIn("not the spawned 20", str(raised.exception))
        self.assertEqual(self.released, [])

    def test_no_second_instance_fails_without_touching_the_game(self):
        with self.assertRaises(RuntimeError) as raised:
            capture_run.launch(self.run, timeout=0.01, direct=True)
        self.assertIn("left running", str(raised.exception))
        self.assertEqual(self.released, [])

    def test_first_instance_timeout_keeps_the_old_message(self):
        self.windows, self.running, self.after_start = [], [], []
        with self.assertRaises(RuntimeError) as raised:
            capture_run.launch(self.run, timeout=0.01, direct=True)
        self.assertIn("no engine window", str(raised.exception))

    def test_two_new_instances_are_refused(self):
        self.after_start = [(10, 0xA0), (20, 0xB0), (21, 0xB1)]
        with self.assertRaises(RuntimeError) as raised:
            capture_run.launch(self.run, timeout=5, direct=True)
        self.assertIn("[20, 21]", str(raised.exception))

    def test_one_new_pid_with_two_windows_is_fine(self):
        self.after_start = [(10, 0xA0), (20, 0xB0), (20, 0xB1)]
        self.assertEqual(capture_run.launch(self.run, timeout=5, direct=True), (20, 0xB0))

    def test_launch_holds_the_launch_lock(self):
        held = []

        @contextlib.contextmanager
        def lock(path, **_):
            held.append(path)
            yield

        self.after_start = [(10, 0xA0), (20, 0xB0)]
        with mock.patch.object(capture_run.save_pin, "file_lock", lock):
            capture_run.launch(self.run, timeout=5, direct=True)
        self.assertEqual(held, [capture_run.LAUNCH_LOCK])


class GuardNameTest(unittest.TestCase):
    def test_guard_in_any_namespace(self):
        is_guard = capture_run.instance_guard.is_guard
        self.assertTrue(is_guard("Semaphore", r"\Sessions\1\BaseNamedObjects\scimitar_semaphore"))
        self.assertTrue(is_guard("Semaphore", r"\BaseNamedObjects\scimitar_semaphore"))
        self.assertFalse(is_guard("Mutant", r"\Sessions\1\BaseNamedObjects\scimitar_semaphore"))
        self.assertFalse(is_guard("Semaphore", r"\Sessions\1\BaseNamedObjects\scimitar_semaphore2"))
        self.assertFalse(is_guard("Semaphore", None))


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
