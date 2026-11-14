"""Who computes the root delta inside the animation graph update? Data-write watches on the graph's
output struct (ACU-RE-0057 follow-up; needs the probe build with HOOKDATA).

    python tools/native_probe/root_extract_watch.py PID OUT.jsonl [--stick X Y]

Static chain (ACU-RE-0057): the AtomAnimComponent update 0x1400EF1B0 calls
0x1400FA080(controller, dt, &out, ...) with `out` on its stack; `out` = {translation +0x00,
rotation +0x10, 32-byte entry array +0x20}. Later 0x140099980(M, &out) copies it to M+0x90.
At the M+0x90 write trap (rip 0x14009999B), rdx = out + 0x20 (`add rdx, 0x20` at 0x140099990).

Steps:
 1. Watch M+0x90 briefly; take rdx from the events -> out. The update runs on job worker threads,
    so `out` is at one stack offset on several stacks; the most frequent one is watched, which
    sees only the frames run on that worker.
 2. Watch out+0x00 (8 bytes, sample 32: translation and rotation), out+0x10 (8 bytes, sample 16)
    and keep M+0x90 as the per-frame marker. A stack slot is shared with other code, so the first
    0.3 s are a rate test: above --max-rate traps/s everything is unhooked and the run stops.
 3. Pad phases idle, walk, stop; live_state graph leaves are sampled at ~20 Hz alongside.
Unhooks every slot at exit. Memory reads, the pad and the probe only.
"""
from __future__ import annotations

import argparse
import json
import sys
import threading
import time
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import focus  # noqa: E402
import live_state  # noqa: E402
import teleport  # noqa: E402
from client import checked, command  # noqa: E402

FULL = 32000
SETTER_TRAP_RIP = 0x14009999B
ANIM_SKELETON, SKELETON_MOTION, MOTION_DELTA = 0xD8, 0x280, 0x90


def phases(sx: int, sy: int) -> list:
    return [("idle", {}, 1.0), ("walk", {"left_stick": [sx, sy]}, 3.0), ("stop", {}, 1.5)]


def drain(pid: int) -> list:
    r = command(pid, "EVENTS 256")
    return r.get("events") or [] if r.get("ok") else []


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("pid", type=int)
    ap.add_argument("out")
    ap.add_argument("--stick", type=int, nargs=2, default=[0, FULL], metavar=("X", "Y"))
    ap.add_argument("--max-rate", type=float, default=3000.0)
    ap.add_argument("--poll", type=float, default=0.05)
    a = ap.parse_args()
    mem = live_state.Memory(a.pid)
    entity, _ = teleport.ready(mem)
    anim = live_state.anim_component(mem, entity)
    skeleton = mem.u64(anim + ANIM_SKELETON)
    motion = mem.u64(skeleton + SKELETON_MOTION)
    controller = mem.u64(anim + 0xD0)
    if not (skeleton and motion) or mem.u64(skeleton + 0x10) != entity:
        raise SystemExit("chain did not resolve to the player entity")
    teleport.safety_on(a.pid)
    slots_used: list[int] = []
    with open(a.out, "w", encoding="utf-8") as fh:
        def log(rec: dict) -> None:
            fh.write(json.dumps(rec) + "\n")

        log({"kind": "header", "t": time.time(), "entity": f"{entity:016x}", "anim": f"{anim:016x}",
             "skeleton": f"{skeleton:016x}", "motion": f"{motion:016x}", "controller": f"{controller:016x}",
             "status": checked(a.pid, "STATUS")})
        try:
            # 1. find `out` from the setter's rdx
            drain(a.pid)
            checked(a.pid, f"HOOKDATA 2 {motion + MOTION_DELTA:x} 8 root_delta 1 0 32 root_extract")
            slots_used.append(2)
            time.sleep(0.4)
            evs = [e for e in drain(a.pid) if e["label"] == "root_delta" and int(e["rip"], 16) == SETTER_TRAP_RIP]
            counts = Counter(int(e["registers"][1], 16) - 0x20 for e in evs)
            log({"kind": "locate", "events": len(evs), "outs": {f"{o:016x}": n for o, n in counts.items()},
                 "rcx": sorted({e["registers"][0] for e in evs})})
            if not counts or len({o & 0xFFFFF for o in counts}) != 1:
                raise SystemExit(f"out struct not at one stack offset: {counts}")
            # The update runs on job worker threads: the same frame offset on several stacks.
            # Debug registers watch one address, so only frames run on that worker are seen.
            out = counts.most_common(1)[0][0]
            # 2. watch the out struct; rate test first
            h0 = checked(a.pid, "STATUS")["hits"]
            t0 = time.time()
            checked(a.pid, f"HOOKDATA 0 {out:x} 8 out_trans 1 0 32 root_extract")
            slots_used.append(0)
            checked(a.pid, f"HOOKDATA 1 {out + 0x10:x} 8 out_rot 1 0 16 root_extract")
            slots_used.append(1)
            time.sleep(0.3)
            rate = (checked(a.pid, "STATUS")["hits"] - h0) / (time.time() - t0)
            log({"kind": "rate_test", "out": f"{out:016x}", "traps_per_s": rate})
            if rate > a.max_rate:
                raise SystemExit(f"rate {rate:.0f}/s above {a.max_rate:.0f}/s; unhooked")
            # 3. phases
            state = {"phase": "arming"}
            stop = threading.Event()

            def poll() -> None:
                while not stop.is_set():
                    e = drain(a.pid)
                    if e:
                        log({"kind": "events", "t": time.time(), "phase": state["phase"], "events": e})
                    time.sleep(a.poll)

            def leaves() -> None:
                m2 = live_state.Memory(a.pid)
                while not stop.is_set():
                    log({"kind": "leaves", "qpc": live_state.qpc(), "phase": state["phase"],
                         "leaves": live_state.leaves(m2, controller)})
                    time.sleep(0.05)
                m2.close()

            ths = [threading.Thread(target=poll, daemon=True), threading.Thread(target=leaves, daemon=True)]
            for th in ths:
                th.start()
            pad = focus.InProcessPad(a.pid)
            try:
                for name, action, secs in phases(*a.stick):
                    state["phase"] = name
                    log({"kind": "phase", "t": time.time(), "qpc": live_state.qpc(), "phase": name, "action": action})
                    pad.update(action)
                    time.sleep(secs)
            finally:
                pad.update({})
                pad.close()
                stop.set()
                for th in ths:
                    th.join(timeout=2)
        finally:
            for slot in slots_used:
                command(a.pid, f"UNHOOK {slot}")
            log({"kind": "status", "t": time.time(), "status": command(a.pid, "STATUS")})
    mem.close()
    print(a.out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
