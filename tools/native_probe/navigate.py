"""Closed-loop walking for staged captures: drive Arno to a point and heading with the
in-process pad, using the live player position from player_state.py as feedback.

    python tools/native_probe/navigate.py <pid> goto X Y [--tolerance 0.3] [--run]
    python tools/native_probe/navigate.py <pid> face YAW_DEG

The stick is camera-relative and the camera yaw is not read, so the controller estimates the
offset between stick angle and world movement angle from its own motion (stick_yaw - motion_yaw,
low-pass filtered) and re-estimates continuously as the camera follows. World axes (observed once,
2026-11-14): Z up; matrix row 1 = Arno's forward. Yaw here = atan2(y, x) of a world XY direction.
Uses the game's own movement, so collisions, slopes and turning are the game's.
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import player_state  # noqa: E402

STICK = 32767


def wrap(angle: float) -> float:
    return (angle + math.pi) % (2 * math.pi) - math.pi


def stick_for(world_yaw: float, offset: float, magnitude: float = 1.0) -> list[int]:
    """Stick (x right, y up) whose world movement direction should be world_yaw.
    offset = stick_yaw - world_yaw as last estimated; stick yaw is atan2(y, x)."""
    s = wrap(world_yaw + offset)
    return [round(STICK * magnitude * math.cos(s)), round(STICK * magnitude * math.sin(s))]


def update_offset(offset: float, stick_yaw: float, motion_yaw: float, gain: float = 0.5) -> float:
    """Blend in the newly observed stick-minus-motion angle (shortest way round)."""
    observed = wrap(stick_yaw - motion_yaw)
    return wrap(offset + gain * wrap(observed - offset))


def thin(path: list[list[float]], spacing: float) -> list[list[float]]:
    """Keep points at least `spacing` apart (XY), always including the last one."""
    out = []
    for p in path:
        if not out or math.hypot(p[0] - out[-1][0], p[1] - out[-1][1]) >= spacing:
            out.append(p)
    if path and out[-1] is not path[-1]:
        out.append(path[-1])
    return out


def heading(state: dict) -> float:
    fwd = state["matrix_rows"][1]
    return math.atan2(fwd[1], fwd[0])


class Navigator:
    def __init__(self, pid: int, pad, dt: float = 0.25):
        self.pid, self.pad, self.dt, self.offset, self.log = pid, pad, dt, None, []

    def state(self) -> dict:
        s = player_state.chain(self.pid)
        if not s["cycle"] or "position" not in s:
            raise RuntimeError("player chain not valid; is the game in the world?")
        return s

    def _drive(self, stick: list[int], seconds: float, run: bool):
        end = time.monotonic() + seconds
        action = {"left_stick": stick, **({"right_trigger": 255} if run else {})}
        while time.monotonic() < end:
            self.pad.update(action)
            time.sleep(1 / 60)

    def calibrate(self, seconds: float = 0.6):
        """Walk briefly and record the camera offset (stick angle minus world motion angle).
        Tries up, right, down, left: Arno wedged against an obstacle may not move one way
        (2026-11-14, stuck on a ledge facing a wall)."""
        for stick in ([0, STICK], [STICK, 0], [0, -STICK], [-STICK, 0]):
            a = self.state()["position"]
            self._drive(stick, seconds, False)
            self.pad.update({})
            b = self.state()["position"]
            dx, dy = b[0] - a[0], b[1] - a[1]
            if math.hypot(dx, dy) >= 0.05:
                self.offset = wrap(math.atan2(stick[1], stick[0]) - math.atan2(dy, dx))
                self.log.append({"calibrate_offset_deg": round(math.degrees(self.offset), 1),
                                 "stick": stick})
                return
        raise RuntimeError("no calibration step moved Arno (boxed in?)")

    def goto(self, x: float, y: float, tolerance: float = 0.3, run: bool = False, timeout: float = 30,
             stuck_seconds: float = 4) -> dict:
        """Straight-line walk. Gives up early when the distance has not shrunk by 0.3 m for
        `stuck_seconds` (a straight line into an obstacle once cost 90 s per trial)."""
        if self.offset is None:
            self.calibrate()
        deadline, last = time.monotonic() + timeout, self.state()["position"]
        best, best_at = math.inf, time.monotonic()
        while time.monotonic() < deadline:
            dx, dy = x - last[0], y - last[1]
            distance = math.hypot(dx, dy)
            if distance <= tolerance:
                self.pad.update({})
                return {"reached": True, "distance": round(distance, 3), "position": last}
            if distance < best - 0.3:
                best, best_at = distance, time.monotonic()
            elif time.monotonic() - best_at > stuck_seconds:
                self.pad.update({})
                return {"reached": False, "stuck": True, "distance": round(distance, 3), "position": last}
            want = math.atan2(dy, dx)
            magnitude = 1.0 if distance > 1.5 else max(0.35, distance / 1.5)
            stick = stick_for(want, self.offset, magnitude)
            self._drive(stick, self.dt, run and distance > 3)
            now = self.state()["position"]
            mx, my = now[0] - last[0], now[1] - last[1]
            if math.hypot(mx, my) > 0.05:
                self.offset = update_offset(self.offset, math.atan2(stick[1], stick[0]), math.atan2(my, mx))
            last = now
        self.pad.update({})
        return {"reached": False, "distance": round(math.hypot(x - last[0], y - last[1]), 3), "position": last}

    def run_along(self, yaw: float, seconds: float, free_run: bool = True, buttons: list[str] | None = None) -> dict:
        """Hold a world direction for `seconds` (RT free run by default), re-steering every step
        as the camera follows Arno. Returns start/end positions."""
        if self.offset is None:
            self.calibrate()
        start = last = self.state()["position"]
        path = [start]
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            stick = stick_for(yaw, self.offset, 1.0)
            action = {"left_stick": stick, **({"right_trigger": 255} if free_run else {}),
                      **({"buttons": buttons} if buttons else {})}
            step_end = min(end, time.monotonic() + 0.1)
            while time.monotonic() < step_end:
                self.pad.update(action)
                time.sleep(1 / 60)
            now = self.state()["position"]
            mx, my = now[0] - last[0], now[1] - last[1]
            if math.hypot(mx, my) > 0.05:
                self.offset = update_offset(self.offset, math.atan2(stick[1], stick[0]), math.atan2(my, mx), gain=0.2)
            last = now
            path.append(now)
        self.pad.update({})
        return {"start": start, "end": last, "path": path}

    def follow(self, path: list[list[float]], spacing: float = 2.0, tolerance: float = 1.0,
               free_run: bool = False, leg_timeout: float = 20) -> dict:
        """Walk a known-traversable path (for example one Arno took earlier) waypoint by waypoint.
        Straight lines between waypoints `spacing` apart avoid the obstacles the path avoided."""
        points = thin(path, spacing)
        for i, p in enumerate(points):
            r = self.goto(p[0], p[1], tolerance=tolerance, run=free_run, timeout=leg_timeout)
            if not r["reached"]:
                return {"completed": False, "waypoint": i, "of": len(points), **r}
        return {"completed": True, "waypoints": len(points), "position": self.state()["position"]}

    def face(self, yaw: float, tolerance_deg: float = 8, timeout: float = 8) -> dict:
        """Turn toward yaw by taking short steps that way: Arno faces the direction he walks.
        Small stick pulses (0.3 magnitude, 0.12 s) did not turn him at all (2026-11-14)."""
        if self.offset is None:
            self.calibrate()
        deadline, steps = time.monotonic() + timeout, 0
        while time.monotonic() < deadline:
            before = self.state()
            error = wrap(yaw - heading(before))
            if abs(math.degrees(error)) <= tolerance_deg:
                break
            stick = stick_for(yaw, self.offset, 0.6)
            self._drive(stick, 0.25, False)
            self.pad.update({})
            time.sleep(0.35)
            after = self.state()["position"]
            mx, my = after[0] - before["position"][0], after[1] - before["position"][1]
            if math.hypot(mx, my) > 0.05:
                self.offset = update_offset(self.offset, math.atan2(stick[1], stick[0]), math.atan2(my, mx))
            steps += 1
        self.pad.update({})
        final = heading(self.state())
        return {"heading_deg": round(math.degrees(final), 1), "error_deg": round(math.degrees(wrap(yaw - final)), 1),
                "steps": steps}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pid", type=int)
    sub = ap.add_subparsers(dest="mode", required=True)
    g = sub.add_parser("goto")
    g.add_argument("x", type=float)
    g.add_argument("y", type=float)
    g.add_argument("--tolerance", type=float, default=0.3)
    g.add_argument("--run", action="store_true")
    f = sub.add_parser("face")
    f.add_argument("yaw_deg", type=float)
    a = ap.parse_args()
    import focus
    with focus.InProcessPad(a.pid) as pad:
        nav = Navigator(a.pid, pad)
        result = nav.goto(a.x, a.y, a.tolerance, a.run) if a.mode == "goto" else nav.face(math.radians(a.yaw_deg))
    print(json.dumps({**result, "log": nav.log, "offset_deg": nav.offset and round(math.degrees(nav.offset), 1)}))


if __name__ == "__main__":
    main()
