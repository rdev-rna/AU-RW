"""Install/remove the ACU background shim (acu_focus.dll) and drive its in-process controller.

    python tools/native_probe/focus.py install PID
    python tools/native_probe/focus.py status PID
    python tools/native_probe/focus.py remove PID

The shim keeps the game running and reading the controller while another window is focused,
neutralises mouse/keyboard while the game is really in the background, and serves a scripted
XInput pad at user index 0 through the shared section Local\\acu-pad-<pid>. `InProcessPad` is the
runner side of that section. Windows only; standard library only.
"""
from __future__ import annotations

import argparse
import ctypes as C
from ctypes import wintypes as W
import json
from pathlib import Path
import shutil
import struct
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "local/native-probe"
GAME_SHA256 = "748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40"
PAD_FORMAT = "<IIIHBBhhhh"  # magic, enabled, sequence, buttons, LT, RT, LX, LY, RX, RY
PAD_MAGIC = 0x50554341      # "ACUP"
BUTTONS = {
    "DPAD_UP": 0x0001, "DPAD_DOWN": 0x0002, "DPAD_LEFT": 0x0004,
    "DPAD_RIGHT": 0x0008, "START": 0x0010, "BACK": 0x0020,
    "LEFT_THUMB": 0x0040, "RIGHT_THUMB": 0x0080,
    "LEFT_SHOULDER": 0x0100, "RIGHT_SHOULDER": 0x0200,
    "A": 0x1000, "B": 0x2000, "X": 0x4000, "Y": 0x8000,
}

K = C.WinDLL("kernel32", use_last_error=True)
K.OpenEventW.restype = W.HANDLE
K.OpenEventW.argtypes = [W.DWORD, W.BOOL, W.LPCWSTR]
K.SetEvent.argtypes = [W.HANDLE]
K.CloseHandle.argtypes = [W.HANDLE]
K.OpenFileMappingW.restype = W.HANDLE
K.OpenFileMappingW.argtypes = [W.DWORD, W.BOOL, W.LPCWSTR]
K.MapViewOfFile.restype = C.c_void_p
K.MapViewOfFile.argtypes = [W.HANDLE, W.DWORD, W.DWORD, W.DWORD, C.c_size_t]
K.UnmapViewOfFile.argtypes = [C.c_void_p]
K.GetTickCount64.restype = C.c_uint64
BACKSLASH = chr(92)


def status_path(pid: int) -> Path:
    return OUT / f"focus-{pid}.json"


def status(pid: int) -> dict | None:
    # The shim replaces the file every second; a read can land mid-replace (pinned-3b, 2026-11-14,
    # stopped a replay), so retry briefly before reporting no status.
    for attempt in range(3):
        try:
            return json.loads(status_path(pid).read_text(encoding="utf-8"))
        except (OSError, ValueError):
            if attempt < 2:
                time.sleep(0.05)
    return None


def live(pid: int) -> dict | None:
    """The shim's status when installed, all patches active and refreshed in the last 5 s."""
    s = status(pid)
    if not s or s.get("pid") != pid or s.get("state") != "installed" or not s.get("subclassed"):
        return None
    if K.GetTickCount64() - int(s.get("tick_ms", 0)) > 5000:
        return None
    return s if all(p.get("active") for p in s.get("patches", [])) else None


def install(pid: int, dll: Path = OUT / "acu_focus.dll", timeout: float = 15, fps_cap: int = 0,
            block_user_input: bool = False, return_focus: int = 0) -> dict:
    """block_user_input: the user's mouse/keyboard never reach the game, even when it is really
    focused. return_focus: window handle to give the foreground back to if the game holds it."""
    if live(pid):
        return live(pid)
    if not 0 <= fps_cap <= 240:
        raise ValueError("fps_cap must be 0 (off) .. 240")
    # Read by the shim at install: [focus] fps_cap paces IDXGISwapChain::Present.
    (OUT / "acu_focus.ini").write_text(
        f"[focus]\nfps_cap={fps_cap}\nblock_user_input={int(block_user_input)}\nreturn_focus={return_focus:x}\n",
        encoding="ascii")
    # LoadLibrary of an already-loaded path is a no-op, so every install uses a fresh copy. The
    # copy stays in OUT because the shim writes focus-<pid>.json beside its own DLL.
    copy = OUT / f"acu_focus-{pid}-{time.time_ns()}.dll"
    shutil.copy2(dll, copy)
    launched = subprocess.run([str(OUT / "acu_probe_launcher.exe"), "--attach", str(pid), "--dll", str(copy)],
                              capture_output=True, text=True, timeout=60)
    record = next((json.loads(s) for s in launched.stdout.splitlines() if '"injected"' in s), None)
    if not record or not record.get("injected") or record.get("sha256") != GAME_SHA256:
        raise RuntimeError(f"focus shim injection failed: {launched.stdout}{launched.stderr}")
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        s = live(pid)
        if s:
            return s
        s = status(pid)
        if s and s.get("state") == "failed":
            raise RuntimeError(f"focus shim failed: {s}")
        time.sleep(.2)
    raise RuntimeError(f"focus shim did not report installed within {timeout}s: {status(pid)}")


def remove(pid: int, timeout: float = 10) -> dict | None:
    handle = K.OpenEventW(0x0002, False, f"Local{BACKSLASH}acu-focus-stop-{pid}")  # EVENT_MODIFY_STATE
    if not handle:
        return status(pid)
    try:
        K.SetEvent(handle)
    finally:
        K.CloseHandle(handle)
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        s = status(pid)
        if s and s.get("state") == "removed":
            return s
        time.sleep(.2)
    raise RuntimeError(f"focus shim did not confirm removal: {status(pid)}")


def encode(action: dict) -> tuple[int, int, int, int, int, int, int]:
    names = action.get("buttons", [])
    if len(set(names)) != len(names) or any(n not in BUTTONS for n in names):
        raise ValueError("unsupported or duplicate X360 button")
    mask = 0
    for n in names:
        mask |= BUTTONS[n]
    left, right = action.get("left_stick", [0, 0]), action.get("right_stick", [0, 0])
    lt, rt = action.get("left_trigger", 0), action.get("right_trigger", 0)
    values = [*left, *right]
    if len(left) != 2 or len(right) != 2 or any(not isinstance(v, int) for v in [*values, lt, rt]):
        raise ValueError("sticks are [x, y] integer pairs; triggers are integers")
    if any(not -32768 <= v <= 32767 for v in values) or not 0 <= lt <= 255 or not 0 <= rt <= 255:
        raise ValueError("stick values are -32768..32767; triggers are 0..255")
    return mask, lt, rt, left[0], left[1], right[0], right[1]


class InProcessPad:
    """Controller served by the shim inside the game; nothing else on the system sees it."""

    user_index = 0
    reported_user_index = None
    index_verified = True

    def __init__(self, pid: int):
        if not live(pid):
            raise RuntimeError("focus shim is not installed in the target; run focus.py install PID")
        self.pid, self.sequence = pid, 0
        self.handle = K.OpenFileMappingW(0x0002 | 0x0004, False, f"Local{BACKSLASH}acu-pad-{pid}")  # write|read
        if not self.handle:
            raise RuntimeError("the shim's pad section is missing; reinstall the current shim build")
        self.view = K.MapViewOfFile(self.handle, 0x0002 | 0x0004, 0, 0, struct.calcsize(PAD_FORMAT))
        if not self.view:
            K.CloseHandle(self.handle)
            raise RuntimeError("cannot map the shim's pad section")
        self.calibration_probe = {"method": "in_process_pad", "user_index": 0}
        self.update({})

    def _write(self, enabled: int, fields: tuple) -> None:
        self.sequence += 1
        data = struct.pack(PAD_FORMAT, PAD_MAGIC, enabled, self.sequence, *fields)
        C.memmove(self.view, data, len(data))

    def update(self, action: dict) -> None:
        self._write(1, encode(action))

    def verify_first_report(self, action, observed) -> None:  # the runner's own XInput cannot see this pad
        pass

    def close(self) -> None:
        if getattr(self, "view", None):
            self._write(0, (0, 0, 0, 0, 0, 0, 0))
            K.UnmapViewOfFile(self.view)
            self.view = None
        if getattr(self, "handle", None):
            K.CloseHandle(self.handle)
            self.handle = None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("mode", choices=("install", "status", "remove"))
    ap.add_argument("pid", type=int)
    ap.add_argument("--dll", type=Path, default=OUT / "acu_focus.dll")
    ap.add_argument("--fps-cap", type=int, default=0, help="frame limiter for install (0 = off)")
    ap.add_argument("--block-user-input", action="store_true",
                    help="install: the user's mouse/keyboard never reach the game, even when it is focused")
    a = ap.parse_args()
    result = install(a.pid, a.dll, fps_cap=a.fps_cap, block_user_input=a.block_user_input) if a.mode == "install" else remove(a.pid) if a.mode == "remove" else status(a.pid)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
