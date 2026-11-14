"""One-command, hands-off ACU capture: launch, load the save, run a movement script with a
native probe profile, decode, close.

    python tools/native_probe/capture_run.py local/native-probe/runs/<name>
    python tools/native_probe/capture_run.py local/native-probe/runs/<name> --attach <pid> --keep-game
    python tools/native_probe/capture_run.py --close <pid>   # the only way to close ACU (see shutdown())

Stages (each recorded with timings in capture.json):
 1. launch   uplay://launch/720/0 (ACU.exe started directly exits); wait for the engine window
             (class ScimitarEngineWindowClass) and check the executable SHA-256.
 2. attach   focus shim (background running, user's mouse/keyboard kept out, in-process pad,
             --fps-cap) and the native probe.
 3. world    tap A every 5 s (title, Continue = the top menu item, any post-load prompt) until the
             player entity (player_state.py chain) leaves the loading placeholder's origin, then
             wait until its position holds still for 1 s. Arno is never moved, so with --save-pin
             every run starts at the same place. Failure saves not-in-world.png.
 4. scenario controller_autoplay --background --in-process-pad --probe-profile (game unfocused).
 5. decode   tools/parkour_trace.py (action types need --image, a hash-bound mapped image); with the
             live_state.py sampler (default) decisions are paired with clip starts and named
             (tools/parkour_clip_pairs.py --names, local asset index) in clips/.
 6. close    shutdown(): probe SHUTDOWN, graceful close with the shim kept, ClipCursor(NULL) from
             outside, taskkill /F after 15 s (unless --keep-game).

A minimised game stops rendering; a watchdog restores it without activating it. Nothing here
touches game files: run tools/intro_skip.py and tools/capture_preset.py beforehand if wanted.
"""
from __future__ import annotations

import argparse
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(ROOT / "tools"))
import focus  # noqa: E402
import player_state  # noqa: E402
from client import checked, command  # noqa: E402

GAME_SHA256 = focus.GAME_SHA256
ENGINE_CLASS = "ScimitarEngineWindowClass"
user = C.WinDLL("user32")
kernel = C.WinDLL("kernel32", use_last_error=True)
EnumProc = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
kernel.OpenProcess.restype = W.HANDLE
kernel.QueryFullProcessImageNameW.argtypes = [W.HANDLE, W.DWORD, W.LPWSTR, C.POINTER(W.DWORD)]


def engine_windows() -> list[tuple[int, int]]:
    found = []

    def cb(hwnd, _):
        cls = C.create_unicode_buffer(64)
        user.GetClassNameW(hwnd, cls, 64)
        if cls.value == ENGINE_CLASS:
            pid = W.DWORD()
            user.GetWindowThreadProcessId(hwnd, C.byref(pid))
            found.append((pid.value, hwnd))
        return True
    user.EnumWindows(EnumProc(cb), 0)
    return found


def process_image(pid: int) -> Path:
    handle = kernel.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
    if not handle:
        raise RuntimeError(f"cannot open process {pid}")
    try:
        size = W.DWORD(32768)
        buf = C.create_unicode_buffer(size.value)
        if not kernel.QueryFullProcessImageNameW(handle, 0, buf, C.byref(size)):
            raise RuntimeError("QueryFullProcessImageNameW failed")
        return Path(buf.value)
    finally:
        kernel.CloseHandle(handle)


def acu_running() -> list[int]:
    out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq ACU.exe", "/FO", "CSV", "/NH"],
                         capture_output=True, text=True).stdout
    return [int(line.split('","')[1]) for line in out.splitlines() if line.startswith('"ACU.exe"')]


class Run:
    def __init__(self, output: Path):
        self.output = output
        self.record = {"schema_version": 1, "stages": [], "started_unix": time.time()}
        self.t0 = time.monotonic()

    def stage(self, name: str, **data):
        entry = {"stage": name, "t_s": round(time.monotonic() - self.t0, 1), **data}
        self.record["stages"].append(entry)
        print(json.dumps(entry), flush=True)
        self.save()

    def save(self):
        (self.output / "capture.json").write_text(json.dumps(self.record, indent=2) + "\n", encoding="utf-8")


def launch(run: Run, timeout: float) -> tuple[int, int]:
    if acu_running():
        raise SystemExit(f"ACU is already running (PIDs {acu_running()}); close it or pass --attach PID")
    subprocess.run(["cmd", "/c", "start", "", "uplay://launch/720/0"], check=True)
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        windows = engine_windows()
        if windows:
            run.stage("launch", pid=windows[0][0], window=f"{windows[0][1]:x}")
            return windows[0]
        time.sleep(.25)
    raise RuntimeError(f"no ACU engine window within {timeout}s of the Ubisoft Connect launch")


def keep_restored(hwnd: int, stop: threading.Event, run: Run):
    """A minimised ACU stops presenting; bring it back without stealing focus."""
    while not stop.wait(2):
        if user.IsIconic(hwnd):
            user.ShowWindow(hwnd, 4)  # SW_SHOWNOACTIVATE
            run.record.setdefault("restored_from_minimised", 0)
            run.record["restored_from_minimised"] += 1


def hold(pad, action: dict, seconds: float):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        pad.update(action)
        time.sleep(1 / 60)
    pad.update({})


def reach_world(pid: int, run: Run, timeout: float, hwnd: int) -> dict:
    """Tap A (title, Continue, post-load prompt) until the player entity has a real position, then
    wait until it holds still. Nothing moves Arno, so a pinned save gives a fixed start.

    Earlier versions waited for the parkour-selection hook after free-run nudges; the nudges moved
    Arno by different amounts with load time (2026-11-14: 2 vs 6 nudges, different streets)."""
    with focus.InProcessPad(pid) as pad:
        hold(pad, {}, 4)  # let the game enumerate the pad
        start, presses, last_a, previous, placeholder = time.monotonic(), 0, -1e9, None, None
        while time.monotonic() - start < timeout:
            state = player_state.chain(pid)
            pos = state.get("position") if state["cycle"] else None
            if placeholder is None and state["entity"]:
                placeholder = state["entity"]
            loaded = pos is not None and sum(x * x for x in pos) > 1.0  # the loading placeholder sits at 0
            if loaded and previous and max(abs(pos[i] - previous[i]) for i in range(3)) < 0.02:
                rect = W.RECT()
                user.GetClientRect(hwnd, C.byref(rect))  # 1280x720 when the capture preset took effect
                run.stage("world", load_s=round(time.monotonic() - start, 1), a_presses=presses,
                          client=[rect.right, rect.bottom], user_input_blocked_counts=(focus.status(pid) or {}).get("blocked"),
                          entity=state["entity"], loading_entity=placeholder,
                          position=[round(x, 3) for x in pos], forward=[round(x, 4) for x in state["matrix_rows"][1][:3]])
                return state
            if not loaded and time.monotonic() - last_a >= 5:
                hold(pad, {"buttons": ["A"]}, .18)
                presses, last_a = presses + 1, time.monotonic()
            previous = pos if loaded else None
            hold(pad, {}, 1)
    path = run.output / "not-in-world.png"
    from controller_autoplay import capture_window
    capture_window(hwnd).save(path)
    run.stage("world_failed", a_presses=presses, window=str(path))
    raise RuntimeError(f"world not reached within {timeout}s; game window saved to {path}")


def shutdown(pid: int, grace: float = 15.0) -> dict:
    """Close ACU without ever handing it the user's mouse and keyboard.

    2026-11-14 incident: the old order removed the shim first, then asked the game to close and waited.
    The game hung while shutting down; with the shim gone it could clip and hide the cursor, read the
    user's real mouse/keyboard and take the foreground, so the PC was unusable (video stuttered too)
    until it finally exited. Now:
      1. the shim stays installed, so the game keeps seeing no user input and its ClipCursor,
         SetCursorPos and ShowCursor(FALSE) calls stay dropped;
      2. graceful close (taskkill without /F), up to `grace` seconds;
      3. then force-terminate (taskkill /F). Saves are put back from the pin afterwards anyway;
      4. throughout, ClipCursor(NULL) from this process releases any cursor clip.
    """
    user.ClipCursor.argtypes = [C.c_void_p]
    try:
        command(pid, "SHUTDOWN")  # native probe: unhook and stop its pipe
    except Exception:
        pass
    subprocess.run(["taskkill", "/PID", str(pid)], capture_output=True)
    start, forced = time.monotonic(), False
    while pid in acu_running():
        user.ClipCursor(None)
        elapsed = time.monotonic() - start
        if not forced and elapsed >= grace:
            subprocess.run(["taskkill", "/F", "/PID", str(pid)], capture_output=True)
            forced = True
        if elapsed >= grace + 15:
            break
        time.sleep(.1)
    user.ClipCursor(None)
    return {"closed": pid not in acu_running(), "forced": forced, "s": round(time.monotonic() - start, 1)}


def close_game(pid: int, run: Run):
    run.stage("close", **shutdown(pid))


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--close":
        print(json.dumps(shutdown(int(sys.argv[2]))))
        return
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("output", type=Path, help="new run directory under local/native-probe/")
    ap.add_argument("--attach", type=int, help="use an already running ACU at the main menu or in the world")
    ap.add_argument("--keep-game", action="store_true", help="leave the game running afterwards")
    ap.add_argument("--moves", type=Path, default=HERE / "parkour_moves.example.json")
    ap.add_argument("--probe-profile", type=Path, default=HERE / "parkour_selection.profile.json")
    ap.add_argument("--fps-cap", type=int, default=60)
    ap.add_argument("--no-state", action="store_true", help="do not sample player transform and clips")
    ap.add_argument("--allow-user-input", action="store_true",
                    help="let the user's mouse/keyboard reach the game while it is really focused")
    ap.add_argument("--load-timeout", type=float, default=180, help="seconds from the first title press")
    ap.add_argument("--image", type=Path, default=os.environ.get("ACU_MAPPED_IMAGE"),
                    help="hash-bound mapped main image for action-type names (or ACU_MAPPED_IMAGE)")
    ap.add_argument("--screenshot-step", action="append", default=[],
                    help="moves step name to capture the game window after (default: the last step)")
    ap.add_argument("--save-pin", help="start from this pinned save (tools/save_pin.py); the saves found "
                    "before the run are put back after the game closes")
    ap.add_argument("--keep-run-save", action="store_true", help="with --save-pin: leave the run's autosave")
    ap.add_argument("--until", choices=("done", "world"), default="done",
                    help="world: stop once in the world and leave the game running with shim and probe "
                         "attached (for live RE); the pinned-save put-back then has to be run by hand")
    a = ap.parse_args()
    if a.save_pin and a.attach:
        ap.error("--save-pin needs a launch (no --attach)")
    if a.save_pin and a.keep_game and a.until == "done":
        ap.error("--save-pin with --keep-game cannot put the saves back; use --until world for live sessions")
    if a.until == "world":
        a.keep_game = True
    steps = [s.get("name") for s in json.loads(a.moves.read_text())["steps"]]
    unknown = [s for s in a.screenshot_step if s not in steps]
    if unknown:
        ap.error(f"unknown --screenshot-step {unknown}; steps are {steps}")
    a.screenshot_step = a.screenshot_step or steps[-1:]
    output = a.output.resolve()
    if not output.is_relative_to((ROOT / "local/native-probe").resolve()):
        ap.error("output must stay under local/native-probe/")
    output.mkdir(parents=True, exist_ok=False)
    run = Run(output)
    sys.path.insert(0, str(ROOT / "tools"))
    import capture_preset
    import intro_skip
    import save_pin
    run.stage("preflight", capture_preset=capture_preset.status()["preset_applied"],
              intro_skip_placeholders=intro_skip.status()["placeholder"], fps_cap=a.fps_cap)

    pid, hwnd, launched, used = None, None, a.attach is None, None
    stop_watchdog = threading.Event()
    try:
        if a.save_pin:
            if acu_running():
                raise SystemExit(f"ACU is already running (PIDs {acu_running()}); close it first")
            used = save_pin.use(a.save_pin)
            run.stage("save_pin", pin=a.save_pin, snapshot=used["snapshot"],
                      pin_sha256={k: v["sha256"] for k, v in used["pin_files"].items()})
        user_window = user.GetForegroundWindow()  # handed back once the game grabs the foreground
        if launched:
            pid, hwnd = launch(run, 120)
        else:
            pid = a.attach
            hwnd = next((h for p, h in engine_windows() if p == pid), None)
            if not hwnd:
                raise SystemExit(f"no ACU engine window for PID {pid}")
        image = process_image(pid)
        digest = hashlib.sha256(image.read_bytes()).hexdigest()
        if digest != GAME_SHA256:
            raise SystemExit(f"{image} SHA-256 {digest} is not the supported build")
        threading.Thread(target=keep_restored, args=(hwnd, stop_watchdog, run), daemon=True).start()
        time.sleep(1)  # let the engine finish creating its window before the shim subclasses it
        # Block the user's mouse/keyboard from the start: right after launch ACU really holds the
        # foreground, so the "only while unfocused" rule would let the user's mouse drive it.
        shim = focus.install(pid, fps_cap=a.fps_cap, block_user_input=not a.allow_user_input,
                             return_focus=user_window or 0)
        try:
            checked(pid, "STATUS")
        except Exception:
            probe = subprocess.run([str(focus.OUT / "acu_probe_launcher.exe"), "--attach", str(pid),
                                    "--dll", str(focus.OUT / "acu_probe.dll")], capture_output=True, text=True)
            if '"injected":true' not in probe.stdout:
                raise RuntimeError(f"probe injection failed: {probe.stdout}{probe.stderr}")
        rect = W.RECT()
        user.GetClientRect(hwnd, C.byref(rect))  # 1280x720 when the capture preset took effect
        run.stage("attach", pid=pid, exe_sha256=digest, shim=shim["state"], fps_cap=shim["frame"]["fps_cap"],
                  block_user_input=shim.get("block_user_input"), returned_focus=shim.get("returned_focus"),
                  client=[rect.right, rect.bottom])

        if launched:
            reach_world(pid, run, a.load_timeout, hwnd)
        if a.until == "world":
            run.stage("until_world", pid=pid, note="game left running; shim and probe attached")
            return

        scenario = output / "scenario"
        cmd = [sys.executable, str(HERE / "controller_autoplay.py"), str(pid), str(a.moves), str(scenario),
               "--expected-sha256", GAME_SHA256, "--background", "--in-process-pad",
               "--probe-profile", str(a.probe_profile)]
        for step in a.screenshot_step:
            cmd += ["--screenshot-step", step]
        # Player transform + playing clips on the probe's QPC clock, for decision/clip pairing.
        state_path, state_stop = output / "live-state.jsonl", output / "live-state.stop"
        sampler = None if a.no_state else subprocess.Popen(
            [sys.executable, str(HERE / "live_state.py"), str(pid), str(state_path), "--hz", "30",
             "--stop-file", str(state_stop)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            result = subprocess.run(cmd, capture_output=True, text=True)
        finally:
            if sampler:
                state_stop.touch()
                sampler.wait(timeout=10)
        (output / "scenario.log").write_text(result.stdout + result.stderr, encoding="utf-8")
        if result.returncode:
            raise RuntimeError(f"scenario failed ({result.returncode}); see scenario.log")
        run.stage("scenario", moves=str(a.moves), trace=str(scenario / "native-session.jsonl"),
                  probe_drops=checked(pid, "STATUS")["dropped_or_overwritten"])

        decode = [sys.executable, str(ROOT / "tools/parkour_trace.py"), str(scenario / "native-session.jsonl"),
                  str(output / "decoded")] + (["--image", str(a.image)] if a.image else [])
        decoded = subprocess.run(decode, capture_output=True, text=True)
        summary = json.loads((output / "decoded/summary.json").read_text()) if decoded.returncode == 0 else None
        run.stage("decode", ok=decoded.returncode == 0, decisions=summary and summary["decisions"],
                  chosen=summary and summary["with_selection"], enum_resolution=summary and summary["enum_resolution"])
        if sampler and decoded.returncode == 0:
            paired = subprocess.run([sys.executable, str(ROOT / "tools/parkour_clip_pairs.py"),
                                     str(output / "decoded/decisions.jsonl"), str(state_path), str(output / "clips"),
                                     "--names"], capture_output=True, text=True)
            clips = json.loads((output / "clips/summary.json").read_text()) if paired.returncode == 0 else None
            run.stage("clips", ok=paired.returncode == 0, samples=clips and clips["samples"],
                      clip_starts=clips and clips["clip_starts"],
                      first_clip_by_type=clips and {k: v[0].get("name", v[0]["file_id"])
                                                    for k, v in clips["first_clip_by_type"].items()})
    finally:
        stop_watchdog.set()
        if pid and not a.keep_game:
            close_game(pid, run)
        if used and not a.keep_run_save:
            if acu_running():
                run.stage("save_putback", ok=False, note=f"ACU still running; put back later with "
                          f"`python tools/save_pin.py putback {used['snapshot']}`")
            else:
                save_pin.putback(used["snapshot"])
                run.stage("save_putback", ok=True, snapshot=used["snapshot"])
        run.record["finished_unix"] = time.time()
        run.save()


if __name__ == "__main__":
    main()
