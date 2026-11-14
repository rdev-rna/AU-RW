"""Capture the native root delta per frame together with the playing clips, for comparison with
decoded root tracks (ACU-RE-0058; needs the probe build with HOOKDATA).

    python tools/native_probe/root_delta_capture.py PID OUT.jsonl [--stick X Y] [--walk 4]

- Slot 3 (default): data watch on the skeleton motion object's root delta M+0x90 (ACU-RE-0057), sample
  M+0x90..+0xB0 (translation, rotation). One trap per frame for the player.
- Leaves: every ~5 ms, the graph's playback leaves (live_state.py evidence) with file ID, weight,
  playback time f32 +0x384, offset f32 +0x36C and mirror byte +0x37E (bit 0 negates x in the
  native samplers 0x140019CA0 / 0x14001ABA0), stamped with QPC like the probe's events.
Phases idle, walk, stop. Memory reads, the pad and the probe only; unhooks at exit.
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
import live_state as L  # noqa: E402
import teleport  # noqa: E402
from client import checked, command  # noqa: E402

FULL = 32000


def leaves(mem: L.Memory, controller: int) -> list[dict]:
    header = mem.read(controller + L.GRAPH_HEADER, 16)
    if not header:
        return []
    array, count = struct.unpack_from("<Q", header)[0], struct.unpack_from("<H", header, 10)[0]
    rows = mem.read(array, count * 24) if 0 < count <= 512 else None
    out = []
    for i in range(len(rows or b"") // 24):
        state, node, weight, _span, kind, _ = struct.unpack_from("<QQfHBB", rows, i * 24)
        node_raw = mem.read(node, 104) if kind == 0 else None
        if not node_raw or mem.u64(struct.unpack_from("<Q", node_raw)[0] + 0x58) != L.PLAYBACK_LEAF_METHOD:
            continue
        slot = struct.unpack_from("<H", node_raw, 0x64)[0]
        playbacks = mem.u64(state + 0x260)
        playback = mem.u64(playbacks + 8 * slot) if playbacks and slot < 4096 else None
        raw = mem.read(playback, 0x388) if playback else None
        if not raw:
            continue
        file_id = mem.u64(struct.unpack_from("<Q", raw, 0x28)[0] + 0x10)
        if file_id is None:
            continue
        out.append({"row": i, "playback": f"{playback:x}", "file_id": f"{file_id:016x}", "weight": weight,
                    "time": struct.unpack_from("<f", raw, 0x384)[0], "offset": struct.unpack_from("<f", raw, 0x36C)[0],
                    "mirror": raw[0x37E] & 1})
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("pid", type=int)
    ap.add_argument("out")
    ap.add_argument("--stick", type=int, nargs=2, default=[0, FULL], metavar=("X", "Y"))
    ap.add_argument("--walk", type=float, default=4.0)
    ap.add_argument("--slot", type=int, default=3, help="probe slot (a slot re-armed right after a busy "
                    "watch can stay on the old address on one thread; see README)")
    a = ap.parse_args()
    mem = L.Memory(a.pid)
    entity, _ = teleport.ready(mem)
    anim = L.anim_component(mem, entity)
    skeleton, controller = mem.u64(anim + 0xD8), mem.u64(anim + 0xD0)
    motion = mem.u64(skeleton + 0x280)
    if not motion or mem.u64(skeleton + 0x10) != entity:
        raise SystemExit("chain did not resolve to the player entity")
    teleport.safety_on(a.pid)
    state = {"phase": "arming"}
    stop = threading.Event()
    with open(a.out, "w", encoding="utf-8") as fh:
        lock = threading.Lock()

        def log(rec: dict) -> None:
            with lock:
                fh.write(json.dumps(rec) + "\n")

        log({"kind": "header", "t": time.time(), "entity": f"{entity:016x}", "motion": f"{motion:016x}",
             "controller": f"{controller:016x}", "status": checked(a.pid, "STATUS")})
        checked(a.pid, "EVENTS 256")
        h0, t0 = checked(a.pid, "STATUS")["hits"], time.time()
        checked(a.pid, f"HOOKDATA {a.slot} {motion + 0x90:x} 8 root_delta 1 0 32 root_delta")
        time.sleep(0.3)
        rate = (checked(a.pid, "STATUS")["hits"] - h0) / (time.time() - t0)
        log({"kind": "rate_test", "traps_per_s": rate})
        if rate > 1000:  # one player write per frame is ~60/s; more means a stale or shared watch
            command(a.pid, f"UNHOOK {a.slot}")
            raise SystemExit(f"rate {rate:.0f}/s; unhooked")

        def poll() -> None:
            while not stop.is_set():
                r = command(a.pid, "EVENTS 256")
                if r.get("ok") and r.get("events"):
                    log({"kind": "events", "phase": state["phase"], "events": r["events"]})
                time.sleep(0.05)

        def sample() -> None:
            m2 = L.Memory(a.pid)
            while not stop.is_set():
                q0 = L.qpc()
                lv = leaves(m2, controller)
                log({"kind": "leaves", "qpc0": q0, "qpc1": L.qpc(), "phase": state["phase"], "leaves": lv})
                time.sleep(0.005)
            m2.close()

        ths = [threading.Thread(target=poll, daemon=True), threading.Thread(target=sample, daemon=True)]
        for th in ths:
            th.start()
        pad = focus.InProcessPad(a.pid)
        try:
            for name, action, secs in [("idle", {}, 1.0), ("walk", {"left_stick": list(a.stick)}, a.walk), ("stop", {}, 1.5)]:
                state["phase"] = name
                log({"kind": "phase", "qpc": L.qpc(), "phase": name, "action": action})
                pad.update(action)
                time.sleep(secs)
        finally:
            pad.update({})
            pad.close()
            stop.set()
            for th in ths:
                th.join(timeout=2)
            command(a.pid, f"UNHOOK {a.slot}")
            log({"kind": "status", "status": command(a.pid, "STATUS")})
    mem.close()
    print(a.out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
