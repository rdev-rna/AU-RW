"""Capture traversal actions per family with per-frame root motion, pose, leaves and parkour
decisions (ACU-RE-0060; needs the probe build with HOOKDATA and the debug-register re-sync).

    python tools/native_probe/action_capture.py PID OUT_DIR --candidates action-candidates.json
        [--per-family 3] [--seconds 3] [--buttons A B]

Scenes: spots and headings from earlier scans whose runs played a clip of each family (swing,
side hop, drop, controlled descent/ascent, leap, vault, wall entry, beam, hang, window;
`action-candidates.json` from the ACU-RE-0059 replay). Each scene is run once per button:
A (with RT: free run, parkour up) and B (with RT: parkour down).

Recorded, all on the probe's QPC clock:
- native-session.jsonl: tools/native_probe/client.py session with parkour_selection.profile.json
  (slots 0/1, ACU-RE-0041/0042), plus slot 2 = data watch on the player matrix row E+0x50
  (sample E+0x20..+0x5F) and slot 3 = data watch on the skeleton motion object's root delta
  M+0x90 (sample 32 bytes; ACU-RE-0057). The session drains every event.
- leaves.jsonl: every ~5 ms, the graph leaves with playback identity and mirror bit
  (root_delta_capture.leaves).
- runs.json: scene, button, teleport result, QPC window, start/end positions.
The profile's rotation period (600 s) bounds the run: the session unhooks every slot when it
rotates, so the scene list is cut to fit. Unhooks slots 2/3 at exit; the session stops on its
stop file.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import focus  # noqa: E402
import live_state as L  # noqa: E402
import navigate  # noqa: E402
import teleport  # noqa: E402
from client import checked, command  # noqa: E402
from root_delta_capture import leaves  # noqa: E402

PROFILE = HERE / "parkour_selection.profile.json"
ROTATION_LIMIT_S = 540  # below the profile's 600 s rotation


def pick_scenes(candidates: dict, per_family: int) -> list[dict]:
    scenes, seen = [], set()
    for family, entries in candidates.items():
        taken = 0
        for run, origin, xyz, yaw, _n in entries:
            key = (tuple(round(v, 1) for v in xyz), round(yaw))
            if key in seen:
                continue
            seen.add(key)
            scenes.append({"family": family, "source_run": run, "source_origin": origin, "xyz": xyz, "yaw_deg": yaw})
            taken += 1
            if taken >= per_family:
                break
    return scenes


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("pid", type=int)
    ap.add_argument("out", type=Path)
    ap.add_argument("--candidates", type=Path, required=True)
    ap.add_argument("--per-family", type=int, default=3)
    ap.add_argument("--seconds", type=float, default=3.0)
    ap.add_argument("--buttons", nargs="+", default=["A", "B"])
    a = ap.parse_args()
    a.out.mkdir(parents=True, exist_ok=False)
    scenes = pick_scenes(json.loads(a.candidates.read_text()), a.per_family)
    budget = ROTATION_LIMIT_S / (a.seconds + 3.5)
    plan = [(s, b) for s in scenes for b in a.buttons][: int(budget)]
    (a.out / "plan.json").write_text(json.dumps([{**s, "button": b} for s, b in plan], indent=1) + "\n")
    mem = L.Memory(a.pid)
    entity, _ = teleport.ready(mem)
    anim = L.anim_component(mem, entity)
    skeleton, controller = mem.u64(anim + 0xD8), mem.u64(anim + 0xD0)
    motion = mem.u64(skeleton + 0x280)
    if not motion or mem.u64(skeleton + 0x10) != entity:
        raise SystemExit("chain did not resolve to the player entity")
    teleport.safety_on(a.pid)
    stop = a.out / "stop"
    session = subprocess.Popen([sys.executable, str(HERE / "client.py"), str(a.pid), "session", str(PROFILE),
                                str(a.out / "native-session.jsonl"), "--seconds", str(ROTATION_LIMIT_S + 30),
                                "--stop-file", str(stop)],
                               stdout=(a.out / "probe-session.log").open("w"), stderr=subprocess.STDOUT)
    halt = threading.Event()
    runs: list[dict] = []
    armed: list[int] = []
    try:
        time.sleep(3)  # the session unhooks all slots when it arms its group
        h0, t0 = checked(a.pid, "STATUS")["hits"], time.time()
        checked(a.pid, f"HOOKDATA 2 {entity + 0x50:x} 8 player_pos 1 -48 64 root_motion")
        armed.append(2)
        checked(a.pid, f"HOOKDATA 3 {motion + 0x90:x} 8 root_delta 1 0 32 root_motion")
        armed.append(3)
        time.sleep(0.5)
        rate = (checked(a.pid, "STATUS")["hits"] - h0) / (time.time() - t0)
        (a.out / "rate.json").write_text(json.dumps({"traps_per_s_all_slots": rate}) + "\n")
        if rate > 3000:
            raise SystemExit(f"rate {rate:.0f}/s; stopping")

        def sample() -> None:
            m2 = L.Memory(a.pid)
            with open(a.out / "leaves.jsonl", "w", encoding="utf-8") as fh:
                while not halt.is_set():
                    q0 = L.qpc()
                    lv = leaves(m2, controller)
                    fh.write(json.dumps({"qpc0": q0, "qpc1": L.qpc(), "leaves": lv}) + "\n")
                    time.sleep(0.005)
            m2.close()

        sampler = threading.Thread(target=sample, daemon=True)
        sampler.start()
        with focus.InProcessPad(a.pid) as pad:
            nav = navigate.Navigator(a.pid, pad)
            for scene, button in plan:
                run = {**scene, "button": button}
                try:
                    run["teleport"] = {k: v for k, v in teleport.teleport(a.pid, *scene["xyz"], settle=0.8).items()
                                       if k in ("error_m", "status")}
                    teleport.safety_on(a.pid)
                    if run["teleport"]["status"] != 1 or run["teleport"]["error_m"] > 0.5:
                        run["skipped"] = "teleport failed"
                        runs.append(run)
                        continue
                    nav.calibrate(0.4)
                    run["qpc_start"] = L.qpc()
                    import math
                    r = nav.run_along(math.radians(scene["yaw_deg"]), a.seconds, free_run=True, buttons=[button])
                    time.sleep(1.0)
                    run["qpc_end"] = L.qpc()
                    run["start"], run["end"] = r["start"], r["end"]
                except RuntimeError as error:
                    run["skipped"] = str(error)
                runs.append(run)
                (a.out / "runs.json").write_text(json.dumps(runs, indent=1) + "\n")
                print(json.dumps({"family": scene["family"], "button": button,
                                  "moved_m": round(math.dist(run.get("start", [0] * 3), run.get("end", [0] * 3)), 2)
                                  if "end" in run else None, "skipped": run.get("skipped")}), flush=True)
    finally:
        halt.set()
        for slot in armed:
            command(a.pid, f"UNHOOK {slot}")
        stop.touch()
        session.wait(timeout=60)
        (a.out / "runs.json").write_text(json.dumps(runs, indent=1) + "\n")
        mem.close()
    print(a.out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
