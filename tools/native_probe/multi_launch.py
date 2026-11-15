"""Start several ACU games at once, each with its own focus shim and native probe.

    python tools/native_probe/multi_launch.py 2                          # two games at the title screen
    python tools/native_probe/multi_launch.py 2 --until world --save-pin capture-start-1
    python tools/native_probe/multi_launch.py 2 --fps-caps 60,20 --cpu-split 0-3/4-5
    python tools/native_probe/capture_run.py --close <pid>                # close each one (only this way)

Each game is one `capture_run.py <run dir> --direct --until title|world` (same shim, probe,
watchdog-free hand-off and save-pin lease as a single capture). The runs start together;
capture_run's launch lock lets them start one after another. Each run starts ACU.exe directly and
closes the new game's single-instance semaphore once its window is up (instance_guard.py), so the
next run's game can start. Ubisoft Connect (uplay://) never starts a second game.

Load (defaults, to keep two games light on one PC):
- --fps-caps 60,30: the first game (the one you watch) at 60, every further game at 30. 30 is the
  floor that keeps ACU's physics on one 1/30 s step per frame (ACU-RE-0051: 15..30 fps -> one 1/30
  step; below 15 fps it sub-steps), and gameplay steps once per frame, so a 30 fps game still
  simulates the same rules, only more coarsely.
- --cpu-split auto: further games get 2 logical processors each from the top, the first game
  keeps the rest; the first game runs at normal priority, the others below normal. Set at process
  creation (the game starts suspended), so each engine sizes its workers to its own cores.
  `none` turns this off; an explicit split is "0-3/4-5" (one core list per game).

Rules that hold for every game (tools/native_probe/README.md):
- an explicit --fps-cap of 240 or lower per game (default 60); never 0;
- close only with `capture_run.py --close <pid>`; never remove or swap the shim on a running game;
- with --save-pin the games share one save folder through the lease; after closing a game, run the
  `save_pin.py release` line printed for it.

Run directories: local/native-probe/runs/multi-<stamp>-<i>/ (capture.json + capture_run.log).
The summary (PIDs, window handles, guard release, errors) is printed and saved as multi-<stamp>.json.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
RUNS = ROOT / "local/native-probe/runs"


def stages(run_dir: Path) -> dict[str, dict]:
    """The last record of each stage in a run's capture.json ({} when it wrote none)."""
    try:
        record = json.loads((run_dir / "capture.json").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    return {s["stage"]: s for s in record.get("stages", [])}


def summary(run_dir: Path, returncode: int | None) -> dict:
    s = stages(run_dir)
    launch, guard, attach = s.get("launch", {}), s.get("guard", {}), s.get("attach", {})
    out = {"run": str(run_dir), "exit_code": returncode, "pid": launch.get("pid"), "cpu": launch.get("cpu"),
           "window": launch.get("window"), "guard_released": guard.get("released"),
           "shim": attach.get("shim"), "fps_cap": attach.get("fps_cap"),
           "in_world": "world" in s, "position": s.get("world", {}).get("position")}
    log = run_dir / "capture_run.log"
    if returncode and log.is_file():
        out["error"] = log.read_text(encoding="utf-8", errors="replace").strip().splitlines()[-1:]
    return out


def per_game_caps(spec: str, count: int) -> list[int]:
    """'60,30' -> [60, 30, 30, ...]: the last value repeats for further games."""
    caps = [int(v) for v in spec.split(",") if v.strip()]
    if not caps or any(not 1 <= c <= 240 for c in caps):
        raise ValueError("fps caps must be 1..240 each (never 0: an uncapped game stalled the PC)")
    if len(caps) > count:
        raise ValueError(f"{len(caps)} caps for {count} games")
    return caps + [caps[-1]] * (count - len(caps))


def cpu_plan(spec: str, count: int, cpu_count: int) -> list[tuple[str | None, str]]:
    """(core list or None, priority) per game. auto: games 2.. get 2 cores each from the top and
    below-normal priority; game 1 keeps the rest at normal. Fewer than 2 cores left for game 1, or a
    single game: no split."""
    if spec == "none" or (spec == "auto" and count == 1):
        return [(None, "normal")] * count
    if spec == "auto":
        first = cpu_count - 2 * (count - 1)
        if first < 2:
            return [(None, "normal")] * count
        plan = [(f"0-{first - 1}", "normal")]
        for i in range(count - 1):
            lo = first + 2 * i
            plan.append((f"{lo}-{lo + 1}", "below_normal"))
        return plan
    lists = spec.split("/")
    if len(lists) != count:
        raise ValueError(f"--cpu-split has {len(lists)} core lists for {count} games")
    return [(cores, "normal" if i == 0 else "below_normal") for i, cores in enumerate(lists)]


def capture_command(run_dir: Path, a, fps_cap: int, cores: str | None, priority: str) -> list[str]:
    cmd = [sys.executable, str(HERE / "capture_run.py"), str(run_dir), "--direct", "--until", a.until,
           "--fps-cap", str(fps_cap), "--load-timeout", str(a.load_timeout), "--priority", priority]
    if cores:
        cmd += ["--cpu-cores", cores]
    if a.save_pin:
        cmd += ["--save-pin", a.save_pin]
    if a.allow_user_input:
        cmd.append("--allow-user-input")
    return cmd


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("count", type=int, help="number of games to start (1..4)")
    ap.add_argument("--until", choices=("title", "world"), default="title")
    ap.add_argument("--fps-caps", default="60,30",
                    help="per game, comma-separated; the last repeats (default 60,30). Never 0")
    ap.add_argument("--cpu-split", default="auto", help="auto (default), none, or core lists per game: 0-3/4-5")
    ap.add_argument("--save-pin", help="start every game from this pin through the shared lease")
    ap.add_argument("--load-timeout", type=float, default=240)
    ap.add_argument("--allow-user-input", action="store_true")
    ap.add_argument("--stagger", type=float, default=2.0, help="seconds between starting the runs")
    a = ap.parse_args()
    if not 1 <= a.count <= 4:
        ap.error("count must be 1..4")
    try:
        caps = per_game_caps(a.fps_caps, a.count)
        plan = cpu_plan(a.cpu_split, a.count, os.cpu_count() or 1)
    except ValueError as error:
        ap.error(str(error))
    if any(c < 15 for c in caps):
        print("warning: below 15 fps ACU's physics sub-steps (ACU-RE-0051); that game simulates differently",
              file=sys.stderr)
    stamp = time.strftime("%Y%m%d-%H%M%S")
    RUNS.mkdir(parents=True, exist_ok=True)
    runs = []
    for i in range(1, a.count + 1):
        run_dir = RUNS / f"multi-{stamp}-{i}"
        log_path = RUNS / f"multi-{stamp}-{i}.log"  # moved into the run dir once capture_run made it
        log = open(log_path, "w", encoding="utf-8")
        cores, priority = plan[i - 1]
        proc = subprocess.Popen(capture_command(run_dir, a, caps[i - 1], cores, priority), cwd=str(ROOT),
                                stdout=log, stderr=subprocess.STDOUT)
        runs.append((run_dir, log_path, log, proc))
        print(json.dumps({"started": str(run_dir), "runner_pid": proc.pid, "fps_cap": caps[i - 1],
                          "cores": cores, "priority": priority}), flush=True)
        time.sleep(a.stagger)
    results = []
    for run_dir, log_path, log, proc in runs:
        code = proc.wait()
        log.close()
        if run_dir.is_dir():
            log_path.replace(run_dir / "capture_run.log")
        else:
            run_dir = log_path  # capture_run failed before creating its directory
        results.append(summary(run_dir, code) if run_dir.is_dir() else
                       {"run": str(run_dir), "exit_code": code,
                        "error": log_path.read_text(encoding="utf-8", errors="replace").strip().splitlines()[-1:]})
    report = {"schema_version": 2, "count": a.count, "until": a.until, "fps_caps": caps,
              "cpu_plan": [{"cores": c, "priority": p} for c, p in plan],
              "save_pin": a.save_pin, "games": results,
              "close": [f"python tools/native_probe/capture_run.py --close {r['pid']}" for r in results if r.get("pid")]}
    if a.save_pin:
        report["release_after_close"] = [f'python tools/save_pin.py release "{r["run"]}"'
                                         for r in results if r.get("pid")]
    (RUNS / f"multi-{stamp}.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    sys.exit(0 if all(r.get("exit_code") == 0 for r in results) else 1)


if __name__ == "__main__":
    main()
