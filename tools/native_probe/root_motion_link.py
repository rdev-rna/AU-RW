"""Does the skeleton's root displacement drive the player's physics body? Hardware data-write
watches on the live player (ADR-0005 velocity source; needs the probe build with HOOKDATA).

    python tools/native_probe/root_motion_link.py PID OUT.jsonl

Chain under test (static, corpus `1400EF1B0.c`, `140037CB0.c`, `14007C840.c`, `14262AF90.c`):
AtomAnimComponent update -> SkeletonComponent [anim+0xD8]; motion object M = [skeleton+0x280];
translation delta at M+0x90, rotation delta (quaternion) at M+0xA0; velocity request
= R(entity) * (scale * delta) * (1/dt) -> BipedComponent [anim+0xE0] request slot +0x130.

Slots:
  0  M+0x90, 8 bytes (delta x, y); sample M+0x90..+0xB0 (translation and rotation delta)
  1  biped+0x130, 8 bytes (velocity request slot 1 x, y); sample 16 bytes
  2  biped+0x1D0, 8 bytes (position target, set by 0x14262AEE0), with --body
  3  [biped+0x768]+0x260, 8 bytes (physics body translation), with --body
The header records 1/dt from the entity's World clock (+0x1C, ACU-RE-0051). Pad phases as in
position_writers.py (`--stick X Y` turns the walk direction away from a wall). Memory reads, the pad and the probe only; unhooks both slots at exit.
"""
from __future__ import annotations

import argparse
import json
import struct
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


def phases(sx: int, sy: int) -> list:
    """Walk along stick (sx, sy) (camera-relative), then a perpendicular strafe, then free-run."""
    return [
        ("idle", {}, 1.5),
        ("walk_forward", {"left_stick": [sx, sy]}, 3.0),
        ("stop", {}, 1.5),
        ("walk_right", {"left_stick": [sy, -sx]}, 2.0),
        ("freerun", {"left_stick": [sx, sy], "right_trigger": 255, "buttons": ["A"]}, 3.0),
        ("idle_end", {}, 1.5),
    ]
ANIM_SKELETON, ANIM_BIPED, SKELETON_MOTION = 0xD8, 0xE0, 0x280
MOTION_DELTA, BIPED_REQUEST1 = 0x90, 0x130
BIPED_TARGET, BIPED_BODY, BODY_TRANSLATION = 0x1D0, 0x768, 0x260  # 14262AEE0.c; 142608180.c (ACU-RE-0056)


def clock_inv_dt(mem: live_state.Memory, entity: int, world: int) -> float | None:
    flags = mem.u64(entity + 0x88) or 0
    clock = world + (0x830 if flags & 0x0400000000000000 else 0x7D8)
    raw = mem.read(clock + 0x18, 8)
    return struct.unpack("<2f", raw)[1] if raw else None


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("pid", type=int)
    ap.add_argument("out")
    ap.add_argument("--poll", type=float, default=0.05)
    ap.add_argument("--stick", type=int, nargs=2, default=[0, FULL], metavar=("X", "Y"),
                    help="walk direction on the left stick (camera-relative); default forward")
    ap.add_argument("--body", action="store_true",
                    help="also watch the position target biped+0x1D0 (slot 2) and body translation [biped+0x768]+0x260 (slot 3)")
    a = ap.parse_args()
    mem = live_state.Memory(a.pid)
    entity, _ = teleport.ready(mem)
    world = mem.u64(entity + 0x60)
    anim = live_state.anim_component(mem, entity)
    skeleton = mem.u64(anim + ANIM_SKELETON)
    biped = mem.u64(anim + ANIM_BIPED)
    motion = mem.u64(skeleton + SKELETON_MOTION)
    if not (skeleton and biped and motion) or mem.u64(skeleton + 0x10) != entity or mem.u64(biped + 0x10) != entity:
        raise SystemExit("chain did not resolve to the player entity")
    teleport.safety_on(a.pid)
    state = {"phase": "arming"}
    stop = threading.Event()
    slots = [(0, motion + MOTION_DELTA, "root_delta", 32), (1, biped + BIPED_REQUEST1, "vel_request1", 16)]
    if a.body:
        body = mem.u64(biped + BIPED_BODY)
        if not body:
            raise SystemExit("biped has no physics body")
        slots += [(2, biped + BIPED_TARGET, "pos_target", 16), (3, body + BODY_TRANSLATION, "body_pos", 16)]
    with open(a.out, "w", encoding="utf-8") as fh:
        fh.write(json.dumps({"kind": "header", "t": time.time(), "entity": f"{entity:016x}", "anim": f"{anim:016x}",
                             "skeleton": f"{skeleton:016x}", "biped": f"{biped:016x}", "motion": f"{motion:016x}",
                             "inv_dt": clock_inv_dt(mem, entity, world), "status": checked(a.pid, "STATUS")}) + "\n")
        checked(a.pid, "EVENTS 256")
        for slot, addr, label, s_bytes in slots:
            r = checked(a.pid, f"HOOKDATA {slot} {addr:x} 8 {label} 1 0 {s_bytes} root_motion")
            fh.write(json.dumps({"kind": "armed", "slot": slot, "label": label, "result": r}) + "\n")

        def poll() -> None:
            while not stop.is_set():
                r = command(a.pid, "EVENTS 256")
                if r.get("ok") and r.get("events"):
                    fh.write(json.dumps({"kind": "events", "t": time.time(), "phase": state["phase"],
                                         "inv_dt": clock_inv_dt(mem, entity, world), "events": r["events"]}) + "\n")
                time.sleep(a.poll)

        th = threading.Thread(target=poll, daemon=True)
        th.start()
        pad = focus.InProcessPad(a.pid)
        try:
            for name, action, secs in phases(*a.stick):
                state["phase"] = name
                fh.write(json.dumps({"kind": "phase", "t": time.time(), "phase": name, "action": action}) + "\n")
                pad.update(action)
                time.sleep(secs)
        finally:
            pad.update({})
            pad.close()
            stop.set()
            th.join(timeout=2)
            for slot, *_ in slots:
                command(a.pid, f"UNHOOK {slot}")
            fh.write(json.dumps({"kind": "status", "t": time.time(), "status": command(a.pid, "STATUS")}) + "\n")
    mem.close()
    print(a.out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
