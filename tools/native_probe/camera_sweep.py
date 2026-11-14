"""Scripted stick sweep against the live player camera (ACU-RE-0052 oracle source).

Drives the shim's in-process pad through fixed phases and logs, every ``--interval`` seconds,
the camera pose and player state (``camera_state.sample``) with the active phase and stick
values. Output: one JSONL file (``kind`` = ``phase`` or ``sample``) for
``tools/oracle_export.py camera``.

    python tools/native_probe/camera_sweep.py PID OUT.jsonl [--interval 0.05]

The pad is released between phases and at exit. Uses only the in-process pad and memory reads;
it never installs, removes or swaps the shim.
"""
from __future__ import annotations

import argparse
import json
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import camera_state  # noqa: E402
import focus  # noqa: E402
import live_state  # noqa: E402
import teleport  # noqa: E402

FULL, HALF = 32000, 16000
# (name, left stick, right stick, seconds); every active phase is followed by a neutral rest.
PHASES = [
    ("rest", (0, 0), (0, 0), 1.5),
    ("look_right_full", (0, 0), (FULL, 0), 2.0),
    ("look_left_full", (0, 0), (-FULL, 0), 2.0),
    ("look_right_half", (0, 0), (HALF, 0), 2.0),
    ("look_up_full", (0, 0), (0, FULL), 1.5),
    ("look_down_full", (0, 0), (0, -FULL), 1.5),
    ("move_forward", (0, FULL), (0, 0), 2.5),
    ("move_right", (FULL, 0), (0, 0), 2.5),
    ("move_back", (0, -FULL), (0, 0), 2.5),
    ("move_left", (-FULL, 0), (0, 0), 2.5),
    ("move_forward_turning", (0, FULL), (HALF, 0), 3.0),
]
REST_S = 1.0


def run(pid: int, out: Path, interval: float) -> None:
    teleport.safety_on(pid)
    mem = live_state.Memory(pid)
    pad = focus.InProcessPad(pid)
    state = {"phase": "start", "ls": [0, 0], "rs": [0, 0]}
    stop = threading.Event()
    lock = threading.Lock()

    with open(out, "w", encoding="utf-8") as fh:
        def write(rec: dict) -> None:
            with lock:
                fh.write(json.dumps(rec) + "\n")

        def sampler() -> None:
            while not stop.is_set():
                try:
                    rec = camera_state.sample(mem)
                except teleport.NotReady as exc:
                    rec = {"t": time.time(), "not_ready": str(exc)}
                rec.update(kind="sample", phase=state["phase"], ls=state["ls"], rs=state["rs"])
                write(rec)
                time.sleep(interval)

        th = threading.Thread(target=sampler, daemon=True)
        th.start()
        try:
            for name, ls, rs, secs in PHASES:
                for phase, l, r, s in ((name, ls, rs, secs), (name + "/rest", (0, 0), (0, 0), REST_S)):
                    state.update(phase=phase, ls=list(l), rs=list(r))
                    write({"kind": "phase", "t": time.time(), "phase": phase, "ls": list(l),
                           "rs": list(r), "seconds": s})
                    pad.update({"left_stick": list(l), "right_stick": list(r)})
                    time.sleep(s)
        finally:
            pad.update({})
            pad.close()
            stop.set()
            th.join(timeout=2)
            mem.close()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("pid", type=int)
    ap.add_argument("out")
    ap.add_argument("--interval", type=float, default=0.05)
    a = ap.parse_args()
    run(a.pid, Path(a.out), a.interval)
    print(a.out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
