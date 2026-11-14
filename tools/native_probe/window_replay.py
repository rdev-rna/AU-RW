"""Replay a recorded window entry in two stages, with a check before going in.

    python tools/native_probe/window_replay.py PID OUT_DIR --targets windows.json --window FILE_ID
        --spot X Y Z [--facing FX FY] position     # climb to the recorded pre-entry spot, snapshot, hang
    python tools/native_probe/window_replay.py PID OUT_DIR ... push [--profile lpf|hpf] [--seconds 4]

`position`: teleport to the ground 1.5 m out from the wall below the spot, climb (RT + A) until
the feet reach the spot's height (or the climb stops), adjust along the wall to the spot's
wall coordinate, release (Arno hangs), save a game-window frame and report the distance to the
recorded spot. `push`: from the hang, push toward the window centre in the wall plane until a
window-entry clip plays. Both stages write JSON lines to OUT_DIR/replay.jsonl and samples.

Recorded side entry (traversal-4, attempt 2, 2026-11-14): window 0000001ff93eef60
(FRV_Interior_D_2F_WD_01_Open), spot (805.16, -168.52, 7.06), facing (-0.87, 0.50), low profile,
clip xx_WindowGratingEntry_Climb_Lt-High_tr_IdleHigh.
"""
from __future__ import annotations

import argparse
import json
import math
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import traversal_driver as T  # noqa: E402


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("pid", type=int)
    ap.add_argument("out", type=Path)
    ap.add_argument("--targets", type=Path, required=True)
    ap.add_argument("--window", required=True)
    ap.add_argument("--spot", type=float, nargs=3, required=True)
    ap.add_argument("stage", choices=("position", "push"))
    ap.add_argument("--profile", default="lpf")
    ap.add_argument("--seconds", type=float, default=4.0)
    ap.add_argument("--no-climb", action="store_true", help="position: start from the current hang")
    ap.add_argument("--short-of", type=float, default=0.0, help="position: stop this many metres before the spot (u)")
    ap.add_argument("--no-adjust", action="store_true",
                    help="position: hang where the climb stops (the high-profile adjust can trigger a climb pass)")
    a = ap.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    w = next(x for x in json.loads(a.targets.read_text())["windows"] if x["file_id"] == a.window)
    game = T.LiveGame(a.pid)
    rec = T.Recorder(game, T.Names()).start()
    drv = T.Driver(game, rec)
    drv.right_sign, drv._flipped = drv.wall_right_sign(w), False
    o = w["outward"]
    face = T.yaw_of((-o[0], -o[1]))
    spot_u, _ = drv.wall_uv(w, a.spot)
    out = {"stage": a.stage, "window": a.window, "spot": a.spot, "spot_u": round(spot_u, 3), "t0": time.time()}
    first = rec.mark()
    try:
        time.sleep(0.3)
        if a.stage == "position" and a.no_climb:
            here = drv.position()
            u, v = drv.wall_uv(w, here)
            goal_u = spot_u + math.copysign(a.short_of, u - spot_u)
            out["adjust_from"] = [round(u, 3), round(v, 3)]
            out["adjust"] = drv.wall_goto(w, (goal_u, a.spot[2] + T.BODY_Z), {"right_trigger": 255}, lambda s: None,
                                          timeout=12.0, tolerance=0.3, until_hit=False)
            drv.release(0.8)
            here = drv.position()
            out["position"] = here
            out["wall_uv"] = [round(x, 3) for x in drv.wall_uv(w, here)]
            out["distance_to_spot_m"] = round(math.dist(here, a.spot), 3)
            s = rec.latest()
            out["on_wall"] = bool(s and drv.on_wall(s))
            out["clips_now"] = [c.get("name") for c in (s.get("clips") or []) if c["weight"] > 0.05] if s else []
            path = a.out / f"position-{int(time.time())}.jpg"
            game.screenshot().convert("RGB").save(path, quality=85)
            out["frame"] = str(path)
        elif a.stage == "position":
            # the wall foot under the spot: project the spot onto the wall line, then step out 1.5 m
            p = w["position"]
            base = [p[0] + w["row1"][0] * spot_u + o[0] * 1.5, p[1] + w["row1"][1] * spot_u + o[1] * 1.5,
                    w["ground_z"] + 0.05]
            out["teleport"] = game.teleport(*base, math.degrees(face))
            time.sleep(0.6)
            drv.calibrate()
            game.teleport(*base, math.degrees(face))
            time.sleep(0.6)

            def high(s):
                q = s.get("position")
                return "height" if q and q[2] >= a.spot[2] - 0.15 and drv.on_wall(s) else None
            out["climb"] = drv.steer(face, 12.0, {"right_trigger": 255, "buttons": ["A"]}, until=high)
            drv.release(0.6)
            here = drv.position()
            out["after_climb"] = here
            # adjust along the wall (high profile, no entry allowed to stop it)
            u, v = drv.wall_uv(w, here)
            if a.no_adjust:
                out["adjust"] = "skipped"
            else:
                out["adjust"] = drv.wall_goto(w, (spot_u, v), {"right_trigger": 255}, lambda s: None, timeout=6.0,
                                              tolerance=0.25, until_hit=False)
                drv.release(0.8)
            here = drv.position()
            out["position"] = here
            out["distance_to_spot_m"] = round(math.dist(here, a.spot), 3)
            s = rec.latest()
            out["on_wall"] = bool(s and drv.on_wall(s))
            out["clips_now"] = [c.get("name") for c in (s.get("clips") or []) if c["weight"] > 0.05] if s else []
            img = game.screenshot()
            path = a.out / f"position-{int(time.time())}.jpg"
            img.convert("RGB").save(path, quality=85)
            out["frame"] = str(path)
        else:
            hit = lambda s: T.matched([s], T.EXPECT["wall"])  # noqa: E731
            out["start_position"] = drv.position()
            prof = {"right_trigger": 255} if a.profile == "hpf" else {}
            centre = (0.0, (w["sill_z"] + w["top_z"]) / 2)
            out["push"] = drv.wall_goto(w, centre, prof, hit, timeout=a.seconds, tolerance=0.2)
            drv.release(1.5)
            samples = rec.since(first)
            out["matched"] = T.matched(samples, T.EXPECT["wall"])
            out["sequence"] = [c["name"] for c in T.clip_sequence(samples)]
            out["end_position"] = drv.position()
            img = game.screenshot()
            path = a.out / f"push-{int(time.time())}.jpg"
            img.convert("RGB").save(path, quality=85)
            out["frame"] = str(path)
    finally:
        samples = rec.since(first)
        rec.stop()
        game.close()
        tag = f"{a.stage}-{int(out['t0'])}"
        (a.out / f"samples-{tag}.jsonl").write_text("".join(json.dumps(s) + "\n" for s in samples))
        with open(a.out / "replay.jsonl", "a", encoding="utf-8") as fh:
            fh.write(json.dumps(out) + "\n")
    print(json.dumps({k: v for k, v in out.items() if k != "teleport"}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
