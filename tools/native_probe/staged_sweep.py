"""Staged approach sweep: run Arno at one spot from several angles and record what the game does.

    python tools/native_probe/staged_sweep.py <pid> <out-dir> --spot X Y --heading DEG
        [--angles -45 -20 0 20 45] [--distance 6] [--run-seconds 3] [--repeats 1] [--image <mapped image>]

For each angle theta: approach yaw psi = heading + theta, approach point = spot - distance * dir(psi).
Arno walks there (navigate.py closed loop), turns to psi, then free-runs along psi for
--run-seconds, re-steering as the camera follows. One native probe session (parkour selection
profile) and one live_state.py sampler run across the whole sweep; afterwards the decisions are
decoded, paired with clip starts (names from the local asset index) and split per trial, with
Arno's actual position and heading at each chosen decision.

The game's own movement and collision are used throughout (no teleport). The approach angle in
the result is the measured one, not the requested one. Requires the game in the world with the
focus shim and probe attached (capture_run.py --until world).
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))
import focus  # noqa: E402
import live_state  # noqa: E402
import navigate  # noqa: E402

PROFILE = HERE / "parkour_selection.profile.json"
STAGING_BACK = 3.0  # metres behind the requested run-up start; facing steps eat most of it
sys.path.insert(0, str(ROOT / "tools"))
from parkour_clip_pairs import clip_starts, load_samples  # noqa: E402
# Plain locomotion cycles are left out of the per-trial clip list.
LOCOMOTION_PREFIXES = ("xx_Nav_Run_", "xx_Nav_Sprint_3", "xx_Nav_Walk", "xx_Nav_PreNavWalk", "xx_Nav_SlopeSprint_3")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pid", type=int)
    ap.add_argument("output", type=Path)
    ap.add_argument("--spot", type=float, nargs=2, required=True, metavar=("X", "Y"))
    ap.add_argument("--heading", type=float, required=True, help="reference approach direction, degrees")
    ap.add_argument("--angles", type=float, nargs="+", default=[-45, -20, 0, 20, 45])
    ap.add_argument("--distance", type=float, default=6)
    ap.add_argument("--run-seconds", type=float, default=3)
    ap.add_argument("--repeats", type=int, default=1)
    ap.add_argument("--image", type=Path)
    ap.add_argument("--route", type=Path, help="live-state.jsonl of an earlier run that reached the spot: "
                    "its path (from the point nearest Arno to the point nearest the spot) is followed first")
    a = ap.parse_args()
    out = a.output
    out.mkdir(parents=True, exist_ok=False)
    stop = out / "stop"
    probe = subprocess.Popen([sys.executable, str(HERE / "client.py"), str(a.pid), "session", str(PROFILE),
                              str(out / "native-session.jsonl"), "--seconds", "3600", "--stop-file", str(stop)],
                             stdout=(out / "probe-session.log").open("w"), stderr=subprocess.STDOUT)
    sampler = subprocess.Popen([sys.executable, str(HERE / "live_state.py"), str(a.pid), str(out / "live-state.jsonl"),
                                "--hz", "30", "--stop-file", str(stop)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    trials = []
    try:
        time.sleep(2)
        with focus.InProcessPad(a.pid) as pad:
            nav = navigate.Navigator(a.pid, pad)
            if a.route:
                route = route_to_spot(a.route, nav.state()["position"], a.spot)
                approach = nav.follow(route, free_run=False)
                (out / "route.json").write_text(json.dumps({"points": len(route), **approach}) + "\n")
                if not approach["completed"]:
                    raise RuntimeError(f"route to the spot not completed: {approach}")
            for repeat in range(a.repeats):
                for theta in a.angles:
                    psi = math.radians(a.heading + theta)
                    # Stage STAGING_BACK metres further out: facing takes steps along psi (window-sweep-2
                    # run-ups were 2-3 m instead of 6), so this leaves about --distance of run-up.
                    back = a.distance + STAGING_BACK
                    ax, ay = a.spot[0] - back * math.cos(psi), a.spot[1] - back * math.sin(psi)
                    trial = {"repeat": repeat, "theta_deg": theta, "psi_deg": a.heading + theta,
                             "approach_point": [round(ax, 3), round(ay, 3)]}
                    trial["goto"] = nav.goto(ax, ay, tolerance=0.5, timeout=30)
                    if not trial["goto"]["reached"]:
                        trial["skipped"] = "approach point not reached"
                        trials.append(trial)
                        continue
                    trial["face"] = nav.face(psi, tolerance_deg=6)
                    here = nav.state()["position"]
                    trial["run_up_m"] = round(math.hypot(a.spot[0] - here[0], a.spot[1] - here[1]), 2)
                    trial["qpc_start"] = live_state.qpc()
                    trial["run"] = nav.run_along(psi, a.run_seconds)
                    trial["qpc_end"] = live_state.qpc()
                    time.sleep(1.5)  # let landings and clip starts after the run complete
                    trial["qpc_settled"] = live_state.qpc()
                    # Come back the way he went (through the window, back over the obstacle) so the
                    # next approach point is reachable; free run so the game's traversal is used.
                    back = list(reversed(trial["run"]["path"]))
                    trial["return"] = {k: v for k, v in nav.follow(back, spacing=1.5, free_run=True).items()
                                       if k != "path"}
                    trial["run"] = {k: trial["run"][k] for k in ("start", "end")}
                    trials.append(trial)
                    (out / "trials.json").write_text(json.dumps(trials, indent=2) + "\n", encoding="utf-8")
                    print(json.dumps({k: trial[k] for k in ("repeat", "theta_deg", "face", "run_up_m")}), flush=True)
    finally:
        stop.touch()
        probe.wait(timeout=60)
        sampler.wait(timeout=10)
        (out / "trials.json").write_text(json.dumps(trials, indent=2) + "\n", encoding="utf-8")
    decode = [sys.executable, str(ROOT / "tools/parkour_trace.py"), str(out / "native-session.jsonl"), str(out / "decoded")]
    subprocess.run(decode + (["--image", str(a.image)] if a.image else []), check=True, capture_output=True)
    subprocess.run([sys.executable, str(ROOT / "tools/parkour_clip_pairs.py"), str(out / "decoded/decisions.jsonl"),
                    str(out / "live-state.jsonl"), str(out / "clips"), "--names"], check=True, capture_output=True)
    summarize(out, trials)


def route_to_spot(state_file: Path, here: list[float], spot: list[float]) -> list[list[float]]:
    """Positions of an earlier run, from the sample nearest `here` to the one nearest `spot`."""
    path = [json.loads(l)["position"] for l in state_file.read_text().splitlines()[1:]]
    path = [p for p in path if p]
    near = lambda q: min(range(len(path)), key=lambda i: math.hypot(path[i][0] - q[0], path[i][1] - q[1]))
    i, j = near(here), near(spot)
    return path[i:j + 1] if i <= j else list(reversed(path[j:i + 1]))


def summarize(out: Path, trials: list[dict]) -> None:
    names = json.loads((out / "clips/summary.json").read_text())["names"]
    pairs = [json.loads(l) for l in (out / "clips/pairs.jsonl").read_text().splitlines()]
    samples = [json.loads(l) for l in (out / "live-state.jsonl").read_text().splitlines()[1:]]
    samples = [s for s in samples if s.get("position")]
    freq, all_samples = load_samples(out / "live-state.jsonl")
    starts = clip_starts(freq, all_samples)
    rows = []
    for t in trials:
        if "qpc_start" not in t:
            rows.append({"theta_deg": t["theta_deg"], "skipped": t.get("skipped")})
            continue
        chosen = []
        for p in pairs:
            if t["qpc_start"] <= p["qpc"] <= t["qpc_settled"]:
                near = min(samples, key=lambda s: abs(s["qpc"] - p["qpc"]))
                f = near["forward"]
                chosen.append({"enum": p["enum"], "fitness": round(p["fitness"], 3), "distance": round(p["distance"], 3),
                               "at": near["position"], "heading_deg": round(math.degrees(math.atan2(f[1], f[0])), 1),
                               "first_clip": (names.get(p["clip_starts"][0]["file_id"], {}).get("name")
                                              or p["clip_starts"][0]["file_id"]) if p["clip_starts"] else None})
        # Every clip that starts during the trial, whichever system chose it: window entries, for
        # one, start without a parkour-selection decision (window-sweep-2, 2026-11-14).
        window = [s for s in samples if t["qpc_start"] <= s["qpc"] <= t["qpc_settled"]]
        clips = [{"t_s": round((c["qpc"] - t["qpc_start"]) / freq, 3),
                  "name": names.get(c["file_id"], {}).get("name", c["file_id"]), "at": c["position"]}
                 for c in starts if t["qpc_start"] <= c["qpc"] <= t["qpc_settled"]]
        clips = [c for c in clips if not c["name"].startswith(LOCOMOTION_PREFIXES)]
        rows.append({"theta_deg": t["theta_deg"], "repeat": t["repeat"], "face_error_deg": t["face"]["error_deg"],
                     "run_up_m": t.get("run_up_m"), "run_start": t["run"]["start"], "z_range": [min(s["position"][2] for s in window),
                                                                max(s["position"][2] for s in window)] if window else None,
                     "chosen": chosen, "clip_starts": clips})
    (out / "sweep-summary.json").write_text(json.dumps(rows, indent=2) + "\n", encoding="utf-8")
    for r in rows:
        print(json.dumps(r))


if __name__ == "__main__":
    main()
