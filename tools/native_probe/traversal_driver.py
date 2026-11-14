"""Closed-loop traversal driver: put Arno at a planned start next to a target (open window), run a
maneuver with the in-process pad using live position and playing clips as feedback, and record
everything. Automated; the user's input is kept out by the focus shim.

    python tools/native_probe/traversal_driver.py PID OUT_DIR --targets windows.json
        [--plan run_in,climb_up] [--max N] [--attempts 2]

Targets: tools/native_probe/traversal_targets.py output (window centre, outward normal, ground,
sill/top heights). Each plan step yields attempts; an attempt succeeds when a clip whose name
matches the step's expected pattern plays (graph weight > 0.05). Names come from the local asset
index (file ID -> resource name).

Recorded per attempt (OUT_DIR/attempts.jsonl, samples/<n>.jsonl): plan, target, teleport result,
pad commands, ~60 Hz samples (QPC, entity, position, forward, leaves with names), the clip
sequence (collapsed), success and the reason it stopped. The player entity is re-resolved on
every sample (live_state.sample), so an entity swap after a teleport does not blind the recorder.

Layout: the driver talks to a `Game` (teleport, sample, pad). `LiveGame` wraps teleport.py,
live_state.py and focus.InProcessPad; tests/test_traversal_driver.py drives a simulated game.
World conventions (observed earlier, see navigate.py): Z up, matrix row 1 = Arno's forward,
yaw = atan2(y, x).
"""
from __future__ import annotations

import argparse
import json
import math
import re
import sqlite3
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
STICK = 32767

# Expected clip families per maneuver (names from ACU's own animations, see window-clips).
EXPECT = {
    "run_in": re.compile(r"Window(Grating)?_?Run|WindowEntry|Window_Run"),
    "wall": re.compile(r"Window(Grating)?Entry"),
}
FALLING = re.compile(r"(?i)fallcycle|freefall|landing")
WALL = re.compile(r"(?i)climb|wallentry|hang")


def wrap(a: float) -> float:
    return (a + math.pi) % (2 * math.pi) - math.pi


def yaw_of(v) -> float:
    return math.atan2(v[1], v[0])


@dataclass
class Plan:
    maneuver: str
    window: dict
    start: list[float]           # x, y, z on the ground outside
    face_yaw: float              # radians, toward the wall
    params: dict = field(default_factory=dict)

    def describe(self) -> dict:
        return {"maneuver": self.maneuver, "window": self.window["file_id"], "stem": self.window["stem"],
                "floor": self.window["floor"], "start": [round(v, 3) for v in self.start],
                "face_yaw_deg": round(math.degrees(self.face_yaw), 1), **self.params}


def plan_run_in(w: dict, distance: float = 6.0, high_profile: bool = True) -> Plan:
    """Ground-floor window: start `distance` m out on the normal through the placement position
    (the opening: earlier Window_Run entries started within ~0.1 m of that line, while the bbox
    centre is ~0.45 m to one side; traversal-2 analysis) and run along the normal."""
    o, c = w["outward"], w["position"]
    start = [c[0] + o[0] * distance, c[1] + o[1] * distance, w["ground_z"] + 0.05]
    return Plan("run_in", w, start, yaw_of((-o[0], -o[1])),
                {"distance_m": distance, "high_profile": high_profile, "seconds": distance / 4.0 + 2.0, "buttons": []})


def plan_run_in_angle(w: dict, angle_deg: float, profile: str, distance: float = 6.0) -> Plan:
    """Ground approach at `angle_deg` from the window normal (positive = toward +row1), in low
    (`lpf`, stick only) or high profile (`hpf`, RT held)."""
    o, c = w["outward"], w["position"]
    a = math.radians(angle_deg)
    r1 = w["row1"]
    d = (o[0] * math.cos(a) + r1[0] * math.sin(a), o[1] * math.cos(a) + r1[1] * math.sin(a))
    start = [c[0] + d[0] * distance, c[1] + d[1] * distance, w["ground_z"] + 0.05]
    p = plan_run_in(w, distance, profile == "hpf")
    return Plan("run_in", w, start, yaw_of((-d[0], -d[1])),
                {**p.params, "angle_deg": angle_deg, "profile": profile})


# Body centre while climbing, above the entity origin (feet). An estimate; recorded per attempt
# and refined from the entry heights.
BODY_Z = 1.0


def plan_wall(w: dict, direction_deg: float, profile: str, radius: float = 1.8, out: float = 1.5) -> Plan:
    """Upper window approached on the wall. Wall-plane coordinates: u along row 1 from the
    placement position, v = height. The approach direction (Arno -> window) has angle
    `direction_deg` in (u, v): 0 = moving toward +u, 90 = moving up, 270 = moving down.
    Arno starts on the wall `radius` m from the window centre opposite that direction. He is
    teleported to the ground under the window (the column under it has handholds; beside it the
    climb stalled at ~3 m, traversal-4), climbs, crosses the wall to the start, then pushes toward
    the window centre in `profile` (lpf: stick only; hpf: RT held)."""
    o, p, r1 = w["outward"], w["position"], w["row1"]
    a = math.radians(direction_deg)
    centre_v = (w["sill_z"] + w["top_z"]) / 2
    su, sv = -radius * math.cos(a), centre_v - radius * math.sin(a)
    start = [p[0] + o[0] * out, p[1] + o[1] * out, w["ground_z"] + 0.05]  # under the window
    return Plan("wall", w, start, yaw_of((-o[0], -o[1])),
                {"direction_deg": direction_deg, "profile": profile, "radius_m": radius,
                 "start_u": round(su, 3), "start_body_z": round(sv, 3), "window_centre_z": round(centre_v, 3)})


def wall_route(su: float, sv: float, low_v: float, half: float, sill_z: float) -> list[tuple[float, float]]:
    """Waypoints (u, v) from the climb point under the window (u = 0, v = low_v) to the start:
    sideways at the low height first; a start at or above the sill within the window's width
    is reached round the window's side (half width + 0.9 m), never across the opening."""
    if sv > sill_z - 0.3 and abs(su) < half + 0.5:
        side_u = math.copysign(half + 0.9, su if su else 1.0)
        return [(side_u, low_v), (side_u, sv), (su, sv)]
    return [(su, low_v), (su, sv)]


class Names:
    def __init__(self, db: Path = ROOT / "local" / "asset_index.sqlite"):
        self.db = sqlite3.connect(f"file:{db.as_posix()}?mode=ro", uri=True, check_same_thread=False) if db.exists() else None
        self.cache: dict[str, str | None] = {}

    def __call__(self, file_id: str) -> str | None:
        if file_id not in self.cache:
            row = self.db.execute("SELECT name FROM resource WHERE file_id = ? LIMIT 1", (file_id,)).fetchone() if self.db else None
            self.cache[file_id] = row[0] if row else None
        return self.cache[file_id]


class Recorder:
    """Samples the game on its own thread; the driver reads the latest sample and the log."""

    def __init__(self, game, names, hz: float = 60):
        self.game, self.names, self.period = game, names, 1 / hz
        self.samples: list[dict] = []
        self.lock = threading.Lock()
        self.halt = threading.Event()
        self.thread = threading.Thread(target=self._run, daemon=True)

    def start(self):
        self.thread.start()
        return self

    def _run(self):
        while not self.halt.is_set():
            s = self.game.sample()
            for c in s.get("clips") or ():
                if not c.get("name"):
                    c["name"] = self.names(c["file_id"])
            with self.lock:
                self.samples.append(s)
            time.sleep(self.period)

    def latest(self) -> dict | None:
        with self.lock:
            return self.samples[-1] if self.samples else None

    def since(self, index: int) -> list[dict]:
        with self.lock:
            return self.samples[index:]

    def mark(self) -> int:
        with self.lock:
            return len(self.samples)

    def stop(self):
        self.halt.set()
        self.thread.join(timeout=2)


def clip_sequence(samples: list[dict], min_weight: float = 0.05) -> list[dict]:
    """Collapse samples into the order clips appeared (by name), with first/last QPC and peak weight."""
    seq: dict[str, dict] = {}
    order = []
    for s in samples:
        for c in s.get("clips") or ():
            if c["weight"] < min_weight:
                continue
            key = c.get("name") or c["file_id"]
            if key not in seq:
                seq[key] = {"name": key, "file_id": c["file_id"], "first_qpc": s["qpc"], "last_qpc": s["qpc"], "peak": c["weight"]}
                order.append(key)
            else:
                seq[key]["last_qpc"] = s["qpc"]
                seq[key]["peak"] = max(seq[key]["peak"], c["weight"])
    return [seq[k] for k in order]


def matched(samples, pattern, min_weight=0.05) -> str | None:
    for s in samples:
        for c in s.get("clips") or ():
            if c["weight"] >= min_weight and c.get("name") and pattern.search(c["name"]):
                return c["name"]
    return None


class Driver:
    def __init__(self, game, recorder, log=None, step: float = 0.05):
        self.game, self.rec, self.step = game, recorder, step
        self.offset = None  # stick yaw minus world motion yaw (camera), estimated from motion
        self.commands: list[dict] = []
        self.shots: list | None = None  # (tag, PIL image or error) per attempt when the game can capture
        self.log = log or (lambda *_: None)

    # -- pad helpers -------------------------------------------------------------------------
    def hold(self, action: dict, seconds: float, until=None) -> str | None:
        """Hold a pad action, re-sent every step; stop early when until(sample) returns a reason."""
        self.commands.append({"action": action, "seconds": seconds})
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            self.game.pad(action)
            time.sleep(self.step)
            if until:
                s = self.rec.latest()
                reason = until(s) if s else None
                if reason:
                    return reason
        return None

    def release(self, seconds: float = 0.3):
        self.game.pad({})
        time.sleep(seconds)

    def position(self) -> list[float] | None:
        s = self.rec.latest()
        return s.get("position") if s else None

    def stick(self, world_yaw: float, magnitude: float = 1.0) -> list[int]:
        a = wrap(world_yaw + (self.offset or 0.0))
        return [round(STICK * magnitude * math.cos(a)), round(STICK * magnitude * math.sin(a))]

    def calibrate(self, seconds: float = 0.5) -> float:
        """Stick-up walk; offset = stick yaw (90 deg) minus the observed world motion yaw."""
        for stick in ([0, STICK], [STICK, 0], [0, -STICK], [-STICK, 0]):
            a = self.position()
            self.hold({"left_stick": stick}, seconds)
            self.release(0.4)
            b = self.position()
            if a and b and math.hypot(b[0] - a[0], b[1] - a[1]) > 0.08:
                self.offset = wrap(math.atan2(stick[1], stick[0]) - math.atan2(b[1] - a[1], b[0] - a[0]))
                return self.offset
        raise RuntimeError("calibration: Arno did not move")

    def steer(self, world_yaw: float, seconds: float, extra: dict, until=None, gain: float = 0.2) -> str | None:
        """Hold a world direction, re-estimating the camera offset from motion while moving."""
        end = time.monotonic() + seconds
        last = self.position()
        while time.monotonic() < end:
            st = self.stick(world_yaw)
            reason = self.hold({"left_stick": st, **extra}, min(0.1, end - time.monotonic()), until)
            if reason:
                return reason
            now = self.position()
            if last and now and math.hypot(now[0] - last[0], now[1] - last[1]) > 0.05:
                observed = wrap(math.atan2(st[1], st[0]) - math.atan2(now[1] - last[1], now[0] - last[0]))
                self.offset = wrap((self.offset or 0.0) + gain * wrap(observed - (self.offset or 0.0)))
            last = now
        return None

    def home(self, point, seconds: float, extra: dict, until=None, gain: float = 0.2) -> str | None:
        """Steer toward a fixed world point (pure pursuit), re-aiming every step, until `until`
        fires or time runs out. Past the point (within 0.6 m) the last heading is kept, so a run
        carries through an opening."""
        end = time.monotonic() + seconds
        last = self.position()
        yaw = None
        while time.monotonic() < end:
            p = self.position()
            if p and (yaw is None or math.hypot(point[0] - p[0], point[1] - p[1]) > 0.6):
                yaw = math.atan2(point[1] - p[1], point[0] - p[0])
            st = self.stick(yaw if yaw is not None else 0.0)
            reason = self.hold({"left_stick": st, **extra}, min(0.1, end - time.monotonic()), until)
            if reason:
                return reason
            now = self.position()
            if last and now and math.hypot(now[0] - last[0], now[1] - last[1]) > 0.05:
                observed = wrap(math.atan2(st[1], st[0]) - math.atan2(now[1] - last[1], now[0] - last[0]))
                self.offset = wrap((self.offset or 0.0) + gain * wrap(observed - (self.offset or 0.0)))
            last = now
        return None

    def wall_uv(self, w: dict, p) -> tuple[float, float]:
        """Arno's wall-plane coordinates: u along row 1 from the placement, v = body height."""
        r1, c = w["row1"], w["position"]
        return (p[0] - c[0]) * r1[0] + (p[1] - c[1]) * r1[1], p[2] + BODY_Z

    def on_wall(self, s) -> bool:
        return any(c["weight"] > 0.3 and c.get("name") and WALL.search(c["name"]) for c in s.get("clips") or ())

    def wall_right_sign(self, w: dict) -> float:
        """Stick right on the wall moves Arno along forward x up = (fy, -fx) (traversal-4: stick
        right gave +1.65 m along row 1 where row 1 == (fy, -fx), camera behind Arno). Returns the
        sign of that direction on row 1; wall_goto flips it if motion disagrees."""
        f = (-w["outward"][0], -w["outward"][1])
        return 1.0 if f[1] * w["row1"][0] - f[0] * w["row1"][1] >= 0 else -1.0

    def wall_goto(self, w: dict, goal, prof: dict, hit, timeout: float, tolerance: float = 0.35, until_hit: bool = True,
                  flip: bool = False):
        """Pursue a wall-plane point (u, v). Returns 'reached', a matched clip name, 'fell off the
        wall', 'stalled' (no progress for 2 s) or 'timeout'."""
        end = time.monotonic() + timeout
        best, best_at = math.inf, time.monotonic()
        while time.monotonic() < end:
            s = self.rec.latest()
            if s and until_hit and hit(s):
                return hit(s)
            if s and not self.on_wall(s) and any(c.get("name") and FALLING.search(c["name"]) for c in s.get("clips") or ()):
                return "fell off the wall"
            u, v = self.wall_uv(w, self.position())
            du, dv = goal[0] - u, goal[1] - v
            d = math.hypot(du, dv)
            if d <= tolerance:
                return "reached"
            if d < best - 0.15:
                best, best_at = d, time.monotonic()
            elif time.monotonic() - best_at > 2.0:
                # lateral mapping check: pushing sideways but moving away -> flip once
                # Off by default: a stall usually means no handhold that way (the climb "intent"
                # pose), not a reversed stick; flipping sent Arno 3.5 m the wrong way (window-replay-1).
                if flip and abs(du) > abs(dv) and not getattr(self, "_flipped", False):
                    self.right_sign, self._flipped, best, best_at = -self.right_sign, True, math.inf, time.monotonic()
                    continue
                return "stalled"
            stick = [round(STICK * self.right_sign * du / d), round(STICK * dv / d)]
            r = self.hold({"left_stick": stick, **prof}, 0.1, until=hit if until_hit else None)
            if r:
                return r
        return "timeout"

    def wall_approach(self, plan: Plan, prof: dict, hit, out: dict) -> str | None:
        """Climb the column under the window (frames give handholds), stop below the sill, move
        across the wall to the start point (around the window's side when the start is above
        it), then push toward the window centre in the plan's profile."""
        w = plan.window
        su, sv = plan.params["start_u"], plan.params["start_body_z"]
        centre = (0.0, plan.params["window_centre_z"])
        half = max(0.6, w["width"] / 2)
        low_v = min(sv, w["sill_z"] - 0.6)
        self.right_sign, self._flipped = self.wall_right_sign(w), False

        def climbed(s):
            p = s.get("position")
            return "height" if p and p[2] + BODY_Z >= low_v and self.on_wall(s) else None
        # 1. climb (RT + A into the wall); a window clip on the way is recorded, not a stop
        stop = self.steer(plan.face_yaw, 4.0 + low_v, {"right_trigger": 255, "buttons": ["A"]}, until=climbed)
        if stop != "height":
            return stop or "did not reach the climb height"
        self.release(0.5)
        s = self.rec.latest()
        if not (s and self.on_wall(s)):
            return "not on the wall after climbing"
        # 2. route to the start: sideways first; above the window go round its side
        route = wall_route(su, sv, low_v, half, w["sill_z"])
        out["wall_route"] = [[round(a, 2), round(b, 2)] for a, b in route]
        legs = []
        for goal in route:
            r = self.wall_goto(w, goal, {"right_trigger": 255}, hit, timeout=8.0, until_hit=False)
            legs.append(r)
            if r != "reached":
                out["wall_legs"] = legs
                return f"route: {r}"
        out["wall_legs"] = legs
        self.release(0.4)
        start_uv = self.wall_uv(w, self.position())
        out["wall_start_uv"] = [round(start_uv[0], 3), round(start_uv[1], 3)]
        out["wall_start_angle_deg"] = round(math.degrees(math.atan2(centre[1] - start_uv[1], centre[0] - start_uv[0])), 1)
        out["right_sign"] = self.right_sign
        # 3. approach in the plan's profile
        first = self.rec.mark()
        r = self.wall_goto(w, centre, prof, hit, timeout=6.0, tolerance=0.2)
        if matched(self.rec.since(first), EXPECT["wall"]) is None and r == "reached":
            return "reached the centre without an entry"
        return r

    def snapshot(self, tag: str):
        if self.shots is not None:
            try:
                self.shots.append((tag, self.game.screenshot()))
            except Exception as error:  # a missing frame never fails the attempt
                self.shots.append((tag, f"error: {error}"))

    # -- attempt -----------------------------------------------------------------------------
    def attempt(self, plan: Plan, timeout: float = 25.0) -> dict:
        expect = EXPECT[plan.maneuver]
        out = {"plan": plan.describe(), "t0": time.time()}
        self.commands = []
        self.shots = [] if hasattr(self.game, "screenshot") else None
        tp = self.game.teleport(*plan.start, math.degrees(plan.face_yaw))
        out["teleport"] = tp
        if tp.get("status") != 1 or tp.get("error_m", 9) > 1.0 or tp.get("fell_through"):
            out.update(ok=False, stop="teleport failed")
            return out
        time.sleep(0.6)
        try:
            self.calibrate()
        except RuntimeError as e:
            out.update(ok=False, stop=str(e))
            return out
        # calibration moved Arno: put him back on the start, keep the offset estimate
        tp2 = self.game.teleport(*plan.start, math.degrees(plan.face_yaw))
        out["teleport_after_calibration"] = {k: tp2.get(k) for k in ("status", "error_m", "fell_through")}
        time.sleep(0.6)
        out["calibration_offset_deg"] = round(math.degrees(self.offset), 1)
        first = self.rec.mark()
        hit = lambda s: matched([s], expect)  # noqa: E731
        hpf = plan.params.get("profile", "hpf" if plan.params.get("high_profile") else "lpf") == "hpf"
        prof = {"right_trigger": 255} if hpf else {}
        stop = None
        self.snapshot("start")
        if plan.maneuver == "run_in":
            # ACU-RE-0045: window entries came from free running (RT only) at 4-6 m/s; holding A
            # (parkour up) makes Arno climb the wall instead (traversal-1, 2026-11-14). Home on
            # the window centre each step so the run arrives centred.
            # Pursue a point 1.5 m inside: heading stays near the normal, lateral drift is corrected.
            c, o = plan.window["position"], plan.window["outward"]
            aim = [c[0] - o[0] * 1.5, c[1] - o[1] * 1.5]
            extra = {**prof, **({"buttons": plan.params["buttons"]} if plan.params.get("buttons") else {})}
            stop = self.home(aim, plan.params["seconds"], extra, until=hit)
        else:
            stop = self.wall_approach(plan, prof, hit, out)
        self.release(1.5)
        self.snapshot("end")
        samples = self.rec.since(first)
        out["samples"] = samples
        out["sequence"] = clip_sequence(samples)
        out["matched"] = matched(samples, expect)
        out["ok"] = out["matched"] is not None
        out["stop"] = stop if isinstance(stop, str) and stop != "height" else ("matched" if out["ok"] else "timeout")
        out["end_position"] = self.position()
        out["commands"] = self.commands
        return out


class LiveGame:
    def __init__(self, pid: int):
        import sys
        sys.path.insert(0, str(HERE))
        import focus
        import live_state
        import teleport
        self.pid, self._tp, self._ls = pid, teleport, live_state
        self.mem = live_state.Memory(pid)
        self.cache: dict = {}
        self.padobj = focus.InProcessPad(pid)
        status = focus.status(pid) or {}
        self.hwnd = int(status.get("window", "0"), 16) or None
        teleport.safety_on(pid)

    def screenshot(self):
        """The game window only (PrintWindow, PW_RENDERFULLCONTENT); never the desktop."""
        import controller_autoplay
        if not self.hwnd:
            raise RuntimeError("game window handle unknown")
        return controller_autoplay.capture_window(self.hwnd)

    def teleport(self, x, y, z, yaw_deg):
        r = self._tp.teleport(self.pid, x, y, z, yaw_deg=yaw_deg, settle=0.8)
        self._tp.safety_on(self.pid)
        return r

    def sample(self) -> dict:
        return self._ls.sample(self.mem, self.cache)

    def pad(self, action: dict):
        self.padobj.update(action)

    def close(self):
        self.padobj.update({})
        self.padobj.close()
        self.mem.close()


RUN_ANGLES = (-60, -30, 0, 30, 60)
WALL_DIRECTIONS = (0, 45, 90, 135, 180, 225, 270, 315)
PROFILES = ("lpf", "hpf")


def build_plans(targets: list[dict], kinds: list[str]) -> list[Plan]:
    """Every approach angle twice per window: once low profile, once high profile."""
    plans = []
    for w in targets:
        if not w.get("clear"):
            continue
        if "run_in" in kinds and w["floor"] == 1:
            plans += [plan_run_in_angle(w, a, pf) for a in RUN_ANGLES for pf in PROFILES]
        if "wall" in kinds and w["floor"] >= 2:
            plans += [plan_wall(w, d, pf) for d in WALL_DIRECTIONS for pf in PROFILES]
    return plans


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("pid", type=int)
    ap.add_argument("out", type=Path)
    ap.add_argument("--targets", type=Path, required=True)
    ap.add_argument("--plan", default="run_in,wall")
    ap.add_argument("--floor", type=int, help="only windows on this floor")
    ap.add_argument("--skip", type=int, default=0, help="skip the first N plans")
    ap.add_argument("--max", type=int, default=6)
    ap.add_argument("--near", type=float, nargs=2, help="prefer windows nearest this XY")
    a = ap.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    (a.out / "samples").mkdir(exist_ok=True)
    targets = json.loads(a.targets.read_text())["windows"]
    if a.near:
        targets.sort(key=lambda w: math.dist(w["center"][:2], a.near))
    if a.floor:
        targets = [w for w in targets if w["floor"] == a.floor]
    plans = build_plans(targets, a.plan.split(","))[a.skip: a.skip + a.max]
    game = LiveGame(a.pid)
    rec = Recorder(game, Names()).start()
    drv = Driver(game, rec)
    try:
        with open(a.out / "attempts.jsonl", "a", encoding="utf-8") as fh:
            for i, p in enumerate(plans):
                r = drv.attempt(p)
                samples = r.pop("samples", [])
                shots = []
                for tag, img in drv.shots or ():
                    if isinstance(img, str):
                        shots.append({"tag": tag, "error": img})
                        continue
                    path = a.out / "frames" / f"{i:03d}-{tag}.jpg"
                    path.parent.mkdir(exist_ok=True)
                    img.convert("RGB").resize((img.width // 2, img.height // 2)).save(path, quality=80)
                    shots.append({"tag": tag, "path": str(path)})
                r["frames"] = shots
                (a.out / "samples" / f"{i:03d}.jsonl").write_text("".join(json.dumps(s) + "\n" for s in samples))
                r["index"] = i
                fh.write(json.dumps(r) + "\n")
                fh.flush()
                print(json.dumps({"i": i, "maneuver": p.maneuver, "angle": p.params.get("angle_deg", p.params.get("direction_deg")),
                                  "profile": p.params.get("profile"), "stem": p.window["stem"], "ok": r["ok"],
                                  "stop": r["stop"], "matched": r.get("matched"),
                                  "clips": [c["name"] for c in r.get("sequence", [])][:12]}), flush=True)
    finally:
        rec.stop()
        game.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
