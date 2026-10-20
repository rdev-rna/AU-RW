#!/usr/bin/env python3
"""Bounded Xbox 360 virtual-controller replay for a verified ACU process.

Uses the locally installed ViGEmClient DLL through ctypes. No game memory is
written. Optional memory checkpoints use the probe's read-only dump command.
"""
from __future__ import annotations
import argparse
import ctypes as C
from ctypes import wintypes as W
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
BUTTONS = {
    "DPAD_UP": 0x0001, "DPAD_DOWN": 0x0002, "DPAD_LEFT": 0x0004,
    "DPAD_RIGHT": 0x0008, "START": 0x0010, "BACK": 0x0020,
    "LEFT_THUMB": 0x0040, "RIGHT_THUMB": 0x0080,
    "LEFT_SHOULDER": 0x0100, "RIGHT_SHOULDER": 0x0200,
    "A": 0x1000, "B": 0x2000, "X": 0x4000, "Y": 0x8000,
}
STOP_VK = 0x7B  # F12; reserved out of controller test profiles as an emergency stop.
ERROR_NONE = 0x20000000
user32 = C.WinDLL("user32", use_last_error=True)
user32.GetAsyncKeyState.argtypes = [C.c_int]
user32.GetAsyncKeyState.restype = C.c_short


class XusbReport(C.Structure):
    _fields_ = [("buttons", C.c_uint16), ("left_trigger", C.c_uint8),
                ("right_trigger", C.c_uint8), ("left_x", C.c_int16),
                ("left_y", C.c_int16), ("right_x", C.c_int16),
                ("right_y", C.c_int16)]


class XInputGamepad(C.Structure):
    _fields_ = [("buttons", C.c_uint16), ("left_trigger", C.c_uint8),
                ("right_trigger", C.c_uint8), ("left_x", C.c_int16),
                ("left_y", C.c_int16), ("right_x", C.c_int16),
                ("right_y", C.c_int16)]


class XInputState(C.Structure):
    _fields_ = [("packet_number", C.c_uint32), ("gamepad", XInputGamepad)]


def read_xinput_state(user_index):
    xinput = C.WinDLL("xinput1_4.dll")
    get_state = xinput.XInputGetState
    get_state.argtypes = [C.c_uint32, C.POINTER(XInputState)]
    get_state.restype = C.c_uint32
    state = XInputState()
    result = get_state(user_index, C.byref(state))
    gamepad = state.gamepad
    return {"user_index": user_index, "result": result,
            "packet_number": state.packet_number, "buttons": gamepad.buttons,
            "left_trigger": gamepad.left_trigger, "right_trigger": gamepad.right_trigger,
            "left_stick": [gamepad.left_x, gamepad.left_y],
            "right_stick": [gamepad.right_x, gamepad.right_y]}


class VirtualX360:
    def __init__(self, dll_path: Path, expected_user_index: int | None = None):
        if C.sizeof(XusbReport) != 12:
            raise RuntimeError("unexpected XUSB_REPORT layout")
        try:
            self.dll = C.CDLL(str(dll_path))
        except OSError as error:
            raise RuntimeError(f"cannot load ViGEmClient DLL {dll_path}: {error}") from error
        self.client = None
        self.target = None
        self.connected = False
        self.attached = False
        self._bind()
        try:
            self.client = self.dll.vigem_alloc()
            if not self.client:
                raise RuntimeError("vigem_alloc failed")
            self._check(self.dll.vigem_connect(self.client), "vigem_connect")
            self.connected = True
            self.target = self.dll.vigem_target_x360_alloc()
            if not self.target:
                raise RuntimeError("vigem_target_x360_alloc failed")
            self._check(self.dll.vigem_target_add(self.client, self.target), "vigem_target_add")
            self.attached = True
            user_index = C.c_uint32()
            self._check(self.dll.vigem_target_x360_get_user_index(
                self.client, self.target, C.byref(user_index)), "vigem_target_x360_get_user_index")
            self.reported_user_index = user_index.value
            if expected_user_index is None:
                self.user_index = self._discover_xinput_index()
                self.index_verified = True
            else:
                # The calibration pulse moves the character. For repeated
                # captures on an already mapped host slot, trust the explicit
                # guard and validate it against the first non-neutral readback.
                if not 0 <= expected_user_index <= 3:
                    raise ValueError("expected XInput user index must be 0..3")
                self.user_index = expected_user_index
                self.index_verified = False
                self.calibration_probe = {
                    "method": "explicit_expected_index",
                    "user_index": expected_user_index,
                    "stick_pulse_sent": False,
                    "first_report_verified": False,
                }
        except BaseException:
            self.close()
            raise

    def _discover_xinput_index(self):
        """Find the XInput slot that reflects this ViGEm target's reports.

        The ViGEm API's target index can differ from the slot visible through
        XInput when other controllers are present. Probe with a brief, unique
        stick state and require one uniquely matching slot.
        """
        time.sleep(0.15)  # Let the bus finish publishing the newly added target.
        before = [read_xinput_state(index) for index in range(4)]
        marker = [0, 12000]
        detected = None
        try:
            self.update({"left_stick": marker})
            deadline = time.monotonic() + 2.0
            while time.monotonic() < deadline:
                states = [read_xinput_state(index) for index in range(4)]
                matches = [index for index, state in enumerate(states)
                           if state["result"] == 0
                           and state["packet_number"] > before[index]["packet_number"]
                           and state["left_stick"] == marker]
                if len(matches) == 1:
                    detected = matches[0]
                    break
                if len(matches) > 1:
                    break
                time.sleep(0.02)
            if detected is None:
                raise RuntimeError(
                    "could not uniquely map the ViGEm target to an XInput slot; "
                    f"driver-reported index={self.reported_user_index}, states={states}")
        finally:
            self.update({})
        released = False
        deadline = time.monotonic() + 1.0
        while detected is not None and time.monotonic() < deadline:
            state = read_xinput_state(detected)
            if state["left_stick"] != marker:
                released = True
                break
            time.sleep(0.02)
        if detected is None or not released:
            raise RuntimeError("ViGEm calibration state did not release cleanly")
        self.calibration_probe = {"left_stick": marker, "released_before_replay": True}
        return detected

    def _bind(self):
        d = self.dll
        d.vigem_alloc.restype = C.c_void_p
        d.vigem_connect.argtypes = [C.c_void_p]; d.vigem_connect.restype = C.c_uint32
        d.vigem_disconnect.argtypes = [C.c_void_p]; d.vigem_disconnect.restype = None
        d.vigem_free.argtypes = [C.c_void_p]; d.vigem_free.restype = None
        d.vigem_target_x360_alloc.restype = C.c_void_p
        d.vigem_target_add.argtypes = [C.c_void_p, C.c_void_p]; d.vigem_target_add.restype = C.c_uint32
        d.vigem_target_remove.argtypes = [C.c_void_p, C.c_void_p]; d.vigem_target_remove.restype = C.c_uint32
        d.vigem_target_free.argtypes = [C.c_void_p]; d.vigem_target_free.restype = None
        d.vigem_target_x360_get_user_index.argtypes = [C.c_void_p, C.c_void_p, C.POINTER(C.c_uint32)]
        d.vigem_target_x360_get_user_index.restype = C.c_uint32
        d.vigem_target_x360_update.argtypes = [C.c_void_p, C.c_void_p, XusbReport]
        d.vigem_target_x360_update.restype = C.c_uint32

    @staticmethod
    def _check(code, operation):
        if code != ERROR_NONE:
            raise RuntimeError(f"{operation} failed: ViGEm error 0x{code:08x}")

    def update(self, action):
        mask = 0
        names = action.get("buttons", [])
        if len(set(names)) != len(names):
            raise ValueError("duplicate controller button in an action")
        for name in names:
            if name not in BUTTONS:
                raise ValueError(f"unsupported X360 button {name!r}")
            mask |= BUTTONS[name]
        left = action.get("left_stick", [0, 0])
        right = action.get("right_stick", [0, 0])
        if len(left) != 2 or len(right) != 2:
            raise ValueError("sticks are [x,y] pairs")
        values = [*left, *right, action.get("left_trigger", 0), action.get("right_trigger", 0)]
        if any(not isinstance(v, int) for v in values):
            raise ValueError("stick and trigger values must be integers")
        if any(not -32768 <= v <= 32767 for v in [*left, *right]) or any(
                not 0 <= v <= 255 for v in values[4:]):
            raise ValueError("stick values are -32768..32767; triggers are 0..255")
        report = XusbReport(mask, values[4], values[5], left[0], left[1], right[0], right[1])
        self._check(self.dll.vigem_target_x360_update(self.client, self.target, report),
                    "vigem_target_x360_update")

    def verify_first_report(self, action, observed):
        expected_buttons = 0
        for name in action.get("buttons", []):
            expected_buttons |= BUTTONS[name]
        expected = (expected_buttons, action.get("left_trigger", 0),
                    action.get("right_trigger", 0), action.get("left_stick", [0, 0]),
                    action.get("right_stick", [0, 0]))
        actual = (observed["buttons"], observed["left_trigger"],
                  observed["right_trigger"], observed["left_stick"],
                  observed["right_stick"])
        if observed["result"] != 0 or actual != expected:
            raise RuntimeError(
                f"expected XInput slot {self.user_index} did not read back the first replay report: "
                f"expected={expected}, observed={actual}, result={observed['result']}"
            )
        self.index_verified = True
        if hasattr(self, "calibration_probe"):
            self.calibration_probe["first_report_verified"] = True

    def close(self):
        if self.client and self.target:
            try:
                self.dll.vigem_target_x360_update(self.client, self.target, XusbReport())
            except Exception:
                pass
            if self.attached:
                self.dll.vigem_target_remove(self.client, self.target)
                self.attached = False
            self.dll.vigem_target_free(self.target)
            self.target = None
        if self.client and self.connected:
            self.dll.vigem_disconnect(self.client)
            self.connected = False
        if self.client:
            self.dll.vigem_free(self.client)
            self.client = None

    def __enter__(self): return self
    def __exit__(self, *_): self.close()


def client_module():
    # Importing the sibling client module also binds the existing read-only probe transport.
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import client
    return client


def foreground_pid():
    user = C.WinDLL("user32", use_last_error=True)
    user.GetForegroundWindow.argtypes = []
    user.GetForegroundWindow.restype = W.HWND
    user.GetWindowThreadProcessId.argtypes = [W.HWND, C.POINTER(W.DWORD)]
    user.GetWindowThreadProcessId.restype = W.DWORD
    hwnd = user.GetForegroundWindow()
    pid = W.DWORD()
    if hwnd:
        user.GetWindowThreadProcessId(hwnd, C.byref(pid))
    return pid.value


def process_exe_sha256(pid):
    # Bind to the executable reported by the probe and hash that exact disk path.
    probe = client_module()
    status = probe.checked(pid, "STATUS")
    modules = probe.checked(pid, "MODULES")["modules"]
    base = status["image_base"].lower()
    module = next((m for m in modules if m["base"].lower() == base), None)
    if not module:
        raise RuntimeError("probe main image is missing from MODULES")
    path = Path(module["path"])
    return hashlib.sha256(path.read_bytes()).hexdigest(), status, modules


def default_vigem_client():
    if os.environ.get("VIGEMCLIENT_DLL"):
        return os.environ["VIGEMCLIENT_DLL"]
    built = ROOT/"local/native-probe/ViGEmClient-src/bin/release/x64/ViGEmClient.dll"
    return str(built) if built.is_file() else None


def validate_profile(profile):
    if profile.get("schema_version") != 1 or not isinstance(profile.get("steps"), list):
        raise ValueError("profile needs schema_version 1 and a steps array")
    if not 1 <= len(profile["steps"]) <= 2048:
        raise ValueError("step count must be 1..2048")
    rate = profile.get("report_hz", 60)
    if not isinstance(rate, int) or not 10 <= rate <= 120:
        raise ValueError("report_hz must be an integer from 10 to 120")
    duration = 0
    for index, step in enumerate(profile["steps"]):
        ms = step.get("duration_ms")
        if not isinstance(ms, int) or not 50 <= ms <= 120_000:
            raise ValueError(f"step {index}: duration_ms must be 50..120000")
        duration += ms
        if duration > 3600_000:
            raise ValueError("total replay is limited to 60 minutes")
        if not isinstance(step.get("name"), str) or not step["name"]:
            raise ValueError(f"step {index}: missing name")
        # Validate ranges/button names before attaching a virtual controller.
        placeholder = VirtualState(step)
        del placeholder
    return duration


class VirtualState:
    def __init__(self, action):
        names = action.get("buttons", [])
        if not isinstance(names, list) or any(name not in BUTTONS for name in names):
            raise ValueError("action contains an unsupported X360 button")
        left, right = action.get("left_stick", [0, 0]), action.get("right_stick", [0, 0])
        vals = [*left, *right, action.get("left_trigger", 0), action.get("right_trigger", 0)]
        if len(left) != 2 or len(right) != 2 or any(not isinstance(x, int) for x in vals):
            raise ValueError("invalid stick/trigger state")
        if any(not -32768 <= x <= 32767 for x in vals[:4]) or any(not 0 <= x <= 255 for x in vals[4:]):
            raise ValueError("stick/trigger values are out of range")


def capture_screenshot(output, name):
    from PIL import ImageGrab
    safe_name = re.sub(r"[^A-Za-z0-9_.-]+", "_", name).strip("_.") or "checkpoint"
    folder = output / "screenshots"
    folder.mkdir(exist_ok=True)
    path = folder / f"{safe_name}.png"
    ImageGrab.grab(all_screens=True).save(path)
    return path


def run_steps(pad, profile, pid, output, snapshot, max_bytes, checkpoint_names,
              screenshot_start, screenshot_steps):
    probe = client_module()
    log_path = output / "input-timeline.jsonl"
    period = 1.0 / profile.get("report_hz", 60)
    if not 10 <= profile.get("report_hz", 60) <= 120:
        raise ValueError("report_hz must be 10..120")
    with log_path.open("x", encoding="utf-8", newline="\n") as log:
        def emit(row):
            log.write(json.dumps({"unix_time": time.time(), **row})+"\n")
            log.flush()
        if screenshot_start:
            path = capture_screenshot(output, "start")
            emit({"kind":"screenshot", "name":"start", "path":str(path.resolve())})
        for index, step in enumerate(profile["steps"]):
            if foreground_pid() != pid:
                raise RuntimeError("target game lost foreground; controller released and replay stopped")
            start = time.perf_counter()
            end = start + step["duration_ms"] / 1000.0
            observed = False
            emit({"kind":"step_start", "index":index, "name":step["name"],
                  "planned_duration_ms":step["duration_ms"], "state":step})
            while time.perf_counter() < end:
                if user32.GetAsyncKeyState(STOP_VK) & 1:
                    raise KeyboardInterrupt("F12 emergency stop")
                if foreground_pid() != pid:
                    raise RuntimeError("target game lost foreground; replay stopped")
                pad.update(step)
                if not observed:
                    state = read_xinput_state(pad.user_index)
                    emit({"kind":"xinput_readback", "index":index, "name":step["name"], **state})
                    if not pad.index_verified and (
                        step.get("buttons") or step.get("left_trigger", 0)
                        or step.get("right_trigger", 0)
                        or step.get("left_stick", [0, 0]) != [0, 0]
                        or step.get("right_stick", [0, 0]) != [0, 0]
                    ):
                        pad.verify_first_report(step, state)
                    observed = True
                time.sleep(min(period, max(0.0, end-time.perf_counter())))
            pad.update({})
            emit({"kind":"step_end", "index":index, "name":step["name"],
                  "actual_duration_ms":round((time.perf_counter()-start)*1000, 2),
                  "release_state":{"buttons":[],"left_stick":[0,0],"right_stick":[0,0],
                                   "left_trigger":0,"right_trigger":0}})
            if step["name"] in screenshot_steps:
                path = capture_screenshot(output, f"{index:03d}_{step['name']}")
                emit({"kind":"screenshot", "index":index, "name":step["name"],
                      "path":str(path.resolve())})
            if snapshot != "none" and (step.get("checkpoint") or step["name"] in checkpoint_names):
                folder = output / f"memory-{index:04d}-{step['name']}"
                manifest = probe.dump(pid, folder, all_regions=(snapshot == "all"), max_bytes=max_bytes)
                emit({"kind":"memory_checkpoint", "index":index, "name":step["name"],
                      "layout":manifest["layout"], "files":len(manifest["files"]),
                      "read_failures":len(manifest["read_failures"]),
                      "coherent_snapshot":False, "manifest":str((folder/"manifest.json").resolve())})
    return log_path


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("pid", type=int)
    ap.add_argument("profile", type=Path)
    ap.add_argument("output", type=Path, help="new run directory under local/native-probe/")
    ap.add_argument("--expected-sha256", required=True,
                    help="exact game executable SHA-256 required before any virtual input")
    ap.add_argument("--vigem-client", type=Path, default=default_vigem_client(),
                    help="local ViGEmClient.dll path (default: VIGEMCLIENT_DLL or locally built SDK)")
    ap.add_argument("--expected-user-index", type=int,
                    help="use this previously observed XInput slot (0..3) without a moving calibration pulse; the first non-neutral report must read back exactly")
    ap.add_argument("--snapshot", choices=("none","main","all"), default="none",
                    help="read-only checkpoint dumps at marked steps; all snapshots committed readable memory")
    ap.add_argument("--max-bytes", type=int, default=4*1024**3)
    ap.add_argument("--checkpoint", action="append", default=[],
                    help="step name to dump even if profile has no checkpoint flag")
    ap.add_argument("--screenshot-start", action="store_true",
                    help="save an all-monitor screenshot before the first input step")
    ap.add_argument("--screenshot-step", action="append", default=[],
                    help="save an all-monitor screenshot after the named step (repeatable)")
    ap.add_argument("--probe-profile", type=Path,
                    help="optional native hook profile to run concurrently; hooks are removed on exit")
    ap.add_argument("--category", action="append", default=[],
                    help="capture category selected by the launcher UI (repeatable)")
    ap.add_argument("--plan-file", type=Path,
                    help="optional machine-local category plan copied into run output")
    ap.add_argument("--xref-functions", type=Path,
                    help="functions.jsonl from a mapped image inventory (requires --xref-summary and --corpus)")
    ap.add_argument("--xref-summary", type=Path, help="summary.json from the same mapped image inventory")
    ap.add_argument("--corpus", type=Path, help="local ACU decompile corpus for event-to-name joins")
    args = ap.parse_args()
    if not 0 <= args.pid <= 0xffffffff or (args.expected_user_index is not None and not 0 <= args.expected_user_index <= 3):
        ap.error("PID must be uint32; expected user index, when supplied, must be 0..3")
    if len(args.expected_sha256) != 64 or any(c not in "0123456789abcdefABCDEF" for c in args.expected_sha256):
        ap.error("--expected-sha256 must be a 64-character hex digest")
    if not 1 <= args.max_bytes <= 64*1024**3:
        ap.error("--max-bytes must be 1..64GiB")
    xref_args = (args.xref_functions, args.xref_summary, args.corpus)
    if any(xref_args) and not all(xref_args):
        ap.error("--xref-functions, --xref-summary and --corpus must be supplied together")
    if not args.vigem_client:
        ap.error("ViGEm bus is present but the user-mode ViGEmClient DLL was not located; pass --vigem-client")
    # PowerShell's default UTF-8 output can include a BOM; accept it for JSON profiles.
    profile = json.loads(args.profile.read_text(encoding="utf-8-sig"))
    duration = validate_profile(profile)
    output = args.output.resolve()
    if not output.is_relative_to((ROOT/"local/native-probe").resolve()):
        ap.error("output must stay under this project's ignored local/native-probe directory")
    actual_sha, status, modules = process_exe_sha256(args.pid)
    if actual_sha.lower() != args.expected_sha256.lower():
        ap.error(f"target image SHA-256 mismatch: got {actual_sha}")
    if foreground_pid() != args.pid:
        ap.error("focus the target game's foreground window before starting the replay")
    output.mkdir(parents=True, exist_ok=False)
    if args.plan_file:
        (output/"capture-plan.json").write_bytes(args.plan_file.read_bytes())
    metadata = {"schema_version":1, "pid":args.pid, "image_sha256":actual_sha,
                "image_base":status["image_base"], "profile":str(args.profile.resolve()),
                "profile_sha256":hashlib.sha256(args.profile.read_bytes()).hexdigest(),
                "duration_ms":duration, "modules":modules, "controller_api":"ViGEmClient XUSB",
                "capture_categories":sorted(set(args.category)),
                "snapshot_mode":args.snapshot, "snapshot_max_bytes":args.max_bytes,
                "f12_emergency_stop":True, "started_utc":datetime.now(timezone.utc).isoformat()}
    (output/"run.json").write_text(json.dumps(metadata,indent=2)+"\n",encoding="utf-8")
    session_process = None
    stop_file = output / "stop-probe-session"
    trace_path = output / "native-session.jsonl"
    try:
        with VirtualX360(args.vigem_client, args.expected_user_index) as pad:
            metadata["xinput_user_index"] = pad.user_index
            metadata["vigem_reported_user_index"] = pad.reported_user_index
            metadata["xinput_index_calibration"] = pad.calibration_probe
            (output/"run.json").write_text(json.dumps(metadata,indent=2)+"\n",encoding="utf-8")
            if args.expected_user_index is not None and pad.user_index != args.expected_user_index:
                raise RuntimeError(f"virtual pad assigned XInput user index {pad.user_index}; expected {args.expected_user_index}. Disconnect other pads or set the expected index.")
            if args.probe_profile:
                with (output/"native-session.stdout.txt").open("x", encoding="utf-8") as native_log:
                    session_process = subprocess.Popen(
                        [sys.executable, str(Path(__file__).resolve().with_name("client.py")),
                         str(args.pid), "session", str(args.probe_profile.resolve()), str(trace_path),
                         "--seconds", str(duration/1000+15), "--max-bytes", str(256*1024**2),
                         "--stop-file", str(stop_file)],
                        stdout=native_log, stderr=subprocess.STDOUT)
                # Do not begin input until the session has installed its first
                # probe group. Process creation alone is not a readiness signal:
                # module hashing and HOOK setup can take several seconds.
                startup_deadline = time.monotonic() + 30
                armed = False
                while not trace_path.exists() and time.monotonic() < startup_deadline:
                    if session_process.poll() is not None:
                        raise RuntimeError("native probe session exited before creating its trace; inspect native-session.stdout.txt")
                    time.sleep(.05)
                if not trace_path.exists():
                    raise RuntimeError("native probe session did not create its trace within 30 seconds")
                with trace_path.open("r", encoding="utf-8") as trace:
                    while time.monotonic() < startup_deadline:
                        line = trace.readline()
                        if line:
                            try:
                                record = json.loads(line)
                            except json.JSONDecodeError:
                                continue
                            if record.get("kind") == "armed":
                                armed = True
                                break
                        elif session_process.poll() is not None:
                            raise RuntimeError("native probe session exited before arming; inspect native-session.stdout.txt")
                        else:
                            time.sleep(.05)
                if not armed:
                    raise RuntimeError("native probe session did not arm within 30 seconds; inspect native-session.jsonl and native-session.stdout.txt")
            log_path = run_steps(pad, profile, args.pid, output, args.snapshot,
                                 args.max_bytes, set(args.checkpoint),
                                 args.screenshot_start, set(args.screenshot_step))
    finally:
        if session_process:
            stop_file.touch(exist_ok=True)
            try:
                # Pipe commands in client.py can take 30 seconds to time out.
                # Give an in-flight EVENTS read time to finish so this parent
                # does not truncate the trace or kill the session before its
                # finally block restores all hardware breakpoints.
                code = session_process.wait(timeout=35)
            except subprocess.TimeoutExpired:
                try:
                    probe = client_module()
                    for slot in range(4):
                        probe.checked(args.pid, f"UNHOOK {slot}")
                except Exception:
                    pass
                session_process.terminate()
                code = session_process.wait(timeout=5)
            metadata["native_session_exit_code"] = code
            metadata["native_session_trace"] = str(trace_path.resolve())
        if args.xref_functions and trace_path.exists():
            xref_output = output / "decompile-xref"
            command = [sys.executable, str(ROOT/"tools/decompile_xref.py"),
                       str(args.xref_functions.resolve()), str(args.xref_summary.resolve()),
                       str(args.corpus.resolve()), str(xref_output), "--session", str(trace_path)]
            result = subprocess.run(command, capture_output=True, text=True)
            (output/"decompile-xref-command.txt").write_text(" ".join(command)+"\n"+result.stdout+result.stderr,
                                                               encoding="utf-8")
            metadata["decompile_xref_exit_code"] = result.returncode
            metadata["decompile_xref_output"] = str(xref_output.resolve())
        metadata["finished_utc"] = datetime.now(timezone.utc).isoformat()
        (output/"run.json").write_text(json.dumps(metadata,indent=2)+"\n",encoding="utf-8")
    print(json.dumps({"run":str(output),"timeline":str(log_path),
                      "controller_user_index":metadata.get("xinput_user_index"),
                      "input_steps":len(profile["steps"])}))


if __name__ == "__main__":
    main()
