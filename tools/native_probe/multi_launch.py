"""Start several ACU games at once, each with its own focus shim and native probe.

    python tools/native_probe/multi_launch.py 2                          # two games at the title screen
    python tools/native_probe/multi_launch.py 2 --until world --save-pin capture-start-1
    python tools/native_probe/capture_run.py --close <pid>                # close each one (only this way)

Each game is one `capture_run.py <run dir> --direct --until title|world` (same shim, probe,
watchdog-free hand-off and save-pin lease as a single capture). The runs start together;
capture_run's launch lock lets them start one after another. Each run starts ACU.exe directly and
closes the new game's single-instance semaphore once its window is up (instance_guard.py), so the
next run's game can start. Ubisoft Connect (uplay://) never starts a second game.

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
    out = {"run": str(run_dir), "exit_code": returncode, "pid": launch.get("pid"),
           "window": launch.get("window"), "guard_released": guard.get("released"),
           "shim": attach.get("shim"), "fps_cap": attach.get("fps_cap"),
           "in_world": "world" in s, "position": s.get("world", {}).get("position")}
    log = run_dir / "capture_run.log"
    if returncode and log.is_file():
        out["error"] = log.read_text(encoding="utf-8", errors="replace").strip().splitlines()[-1:]
    return out


def capture_command(run_dir: Path, a) -> list[str]:
    cmd = [sys.executable, str(HERE / "capture_run.py"), str(run_dir), "--direct", "--until", a.until,
           "--fps-cap", str(a.fps_cap), "--load-timeout", str(a.load_timeout)]
    if a.save_pin:
        cmd += ["--save-pin", a.save_pin]
    if a.allow_user_input:
        cmd.append("--allow-user-input")
    return cmd


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("count", type=int, help="number of games to start (1..4)")
    ap.add_argument("--until", choices=("title", "world"), default="title")
    ap.add_argument("--fps-cap", type=int, default=60, help="per game, 1..240 (never 0)")
    ap.add_argument("--save-pin", help="start every game from this pin through the shared lease")
    ap.add_argument("--load-timeout", type=float, default=240)
    ap.add_argument("--allow-user-input", action="store_true")
    ap.add_argument("--stagger", type=float, default=2.0, help="seconds between starting the runs")
    a = ap.parse_args()
    if not 1 <= a.count <= 4:
        ap.error("count must be 1..4")
    if not 1 <= a.fps_cap <= 240:
        ap.error("--fps-cap must be 1..240; an uncapped game stalled the PC")
    stamp = time.strftime("%Y%m%d-%H%M%S")
    RUNS.mkdir(parents=True, exist_ok=True)
    runs = []
    for i in range(1, a.count + 1):
        run_dir = RUNS / f"multi-{stamp}-{i}"
        log_path = RUNS / f"multi-{stamp}-{i}.log"  # moved into the run dir once capture_run made it
        log = open(log_path, "w", encoding="utf-8")
        proc = subprocess.Popen(capture_command(run_dir, a), cwd=str(ROOT), stdout=log, stderr=subprocess.STDOUT)
        runs.append((run_dir, log_path, log, proc))
        print(json.dumps({"started": str(run_dir), "runner_pid": proc.pid}), flush=True)
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
    report = {"schema_version": 1, "count": a.count, "until": a.until, "fps_cap": a.fps_cap,
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
