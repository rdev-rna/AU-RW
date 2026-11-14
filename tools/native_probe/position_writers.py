"""Who writes the player's transform? Hardware data-write watches on the live player entity
(ADR-0005 validation; needs the probe build with HOOKDATA).

    python tools/native_probe/position_writers.py PID OUT.jsonl

Slots (all on the current World's player E, resolved fresh through teleport.ready):
  0  E+0x50, 8 bytes (position x, y); sample E+0x20..E+0x60 (the matrix)
  1  E+0x58, 4 bytes (position z)
  2  E+0x30, 8 bytes (forward row x, y)
A write trap fires after the writing instruction: each event's `rip` is the next instruction,
which names the writer. The in-process pad drives fixed phases (idle, walk, stop, free-run with
RT+A, idle); every record carries the phase. Uses memory reads, the pad and the probe only; it never
touches the shim. Unhooks all three slots at exit.
"""
from __future__ import annotations

import argparse
import json
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import focus  # noqa: E402
import live_state  # noqa: E402
import teleport  # noqa: E402
from client import checked, command  # noqa: E402

FULL = 32000
PHASES = [
    ("idle", {}, 2.0),
    ("walk_forward", {"left_stick": [0, FULL]}, 3.0),
    ("stop", {}, 2.0),
    ("walk_right", {"left_stick": [FULL, 0]}, 2.0),
    ("freerun", {"left_stick": [0, FULL], "right_trigger": 255, "buttons": ["A"]}, 5.0),
    ("idle_end", {}, 2.0),
]
SLOTS = [
    (0, 0x50, 8, "pos_xy", -0x30, 64),
    (1, 0x58, 4, "pos_z", 0, 4),
    (2, 0x30, 8, "fwd_xy", 0, 8),
]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("pid", type=int)
    ap.add_argument("out")
    ap.add_argument("--poll", type=float, default=0.05)
    a = ap.parse_args()
    mem = live_state.Memory(a.pid)
    entity, _ = teleport.ready(mem)
    mem.close()
    teleport.safety_on(a.pid)
    state = {"phase": "arming"}
    stop = threading.Event()
    with open(a.out, "w", encoding="utf-8") as fh:
        fh.write(json.dumps({"kind": "header", "t": time.time(), "entity": f"{entity:016x}",
                             "status": checked(a.pid, "STATUS")}) + "\n")
        checked(a.pid, "EVENTS 256")
        for slot, off, length, label, s_off, s_bytes in SLOTS:
            r = checked(a.pid, f"HOOKDATA {slot} {entity + off:x} {length} {label} 1 {s_off} {s_bytes} player_transform")
            fh.write(json.dumps({"kind": "armed", "slot": slot, "label": label, "result": r}) + "\n")

        def poll() -> None:
            while not stop.is_set():
                r = command(a.pid, "EVENTS 256")
                if r.get("ok") and r.get("events"):
                    fh.write(json.dumps({"kind": "events", "t": time.time(), "phase": state["phase"],
                                         "events": r["events"]}) + "\n")
                time.sleep(a.poll)

        th = threading.Thread(target=poll, daemon=True)
        th.start()
        pad = focus.InProcessPad(a.pid)
        try:
            for name, action, secs in PHASES:
                state["phase"] = name
                fh.write(json.dumps({"kind": "phase", "t": time.time(), "phase": name, "action": action}) + "\n")
                pad.update(action)
                time.sleep(secs)
        finally:
            pad.update({})
            pad.close()
            stop.set()
            th.join(timeout=2)
            for slot, *_ in SLOTS:
                command(a.pid, f"UNHOOK {slot}")
            fh.write(json.dumps({"kind": "status", "t": time.time(), "status": command(a.pid, "STATUS")}) + "\n")
    print(a.out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
