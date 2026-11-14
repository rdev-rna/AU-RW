"""Read the live player camera (ACU-RE-0052) without hooks.

Path (observed live, run camera-1): CameraManager singleton ``[0x14521AAD0]``; component pointer
array at ``+0x40``, u16 count at ``+0x4A``; the player's camera is the element whose vtable is
``0x143050B00`` (ACUPlayerCameraComponent) and whose owner ``+0x10`` is the player entity.
Position at ``+0x50`` (3 x f32), rotation quaternion (x, y, z, w) at ``+0x60``. Forward is the
quaternion applied to local +Y. The published globals ``0x14521AB40/AB50`` are NOT the live view
(they stayed 221 m away while the rendered camera followed Arno).

Angles: ``yaw`` is a compass heading in degrees, atan2(x, y) (0 = +Y, 90 = +X), the convention
of ``teleport.py``; ``pitch`` = asin(forward.z), Z up.

    python tools/native_probe/camera_state.py PID            # one JSON line
    python tools/native_probe/camera_state.py PID --log OUT --seconds 10 --interval 0.05
"""
from __future__ import annotations

import argparse
import json
import math
import struct
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import live_state  # noqa: E402
import teleport  # noqa: E402

CAMERA_MANAGER_GLOBAL = 0x14521AAD0
COMPONENTS, COMPONENT_COUNT = 0x40, 0x4A
ACU_PLAYER_CAMERA_VTABLE = 0x143050B00
OWNER, POSITION, ROTATION = 0x10, 0x50, 0x60


def rotate(q, v):
    x, y, z, w = q
    vx, vy, vz = v
    tx, ty, tz = 2 * (y * vz - z * vy), 2 * (z * vx - x * vz), 2 * (x * vy - y * vx)
    return (vx + w * tx + (y * tz - z * ty), vy + w * ty + (z * tx - x * tz),
            vz + w * tz + (x * ty - y * tx))


def compass(v) -> float:
    return math.degrees(math.atan2(v[0], v[1]))


def player_camera(mem, entity: int) -> int:
    mgr = struct.unpack("<Q", mem.read(CAMERA_MANAGER_GLOBAL, 8))[0]
    if not mgr:
        raise teleport.NotReady("no CameraManager")
    array = struct.unpack("<Q", mem.read(mgr + COMPONENTS, 8))[0]
    count = struct.unpack("<H", mem.read(mgr + COMPONENT_COUNT, 2))[0]
    for i in range(min(count, 64)):
        comp = struct.unpack("<Q", mem.read(array + 8 * i, 8))[0]
        if not comp:
            continue
        vt, = struct.unpack("<Q", mem.read(comp, 8))
        owner, = struct.unpack("<Q", mem.read(comp + OWNER, 8))
        if vt == ACU_PLAYER_CAMERA_VTABLE and owner == entity:
            return comp
    raise teleport.NotReady(f"no player camera among {count} components")


def sample(mem) -> dict:
    entity, _ = teleport.ready(mem)
    comp = player_camera(mem, entity)
    pos = struct.unpack("<3f", mem.read(comp + POSITION, 12))
    q = struct.unpack("<4f", mem.read(comp + ROTATION, 16))
    fwd = rotate(q, (0.0, 1.0, 0.0))
    player = struct.unpack("<3f", mem.read(entity + 0x50, 12))
    pfwd = struct.unpack("<3f", mem.read(entity + 0x30, 12))
    return {
        "t": time.time(),
        "cam_pos": [round(v, 4) for v in pos],
        "cam_quat": [round(v, 6) for v in q],
        "cam_yaw": round(compass(fwd), 3),
        "cam_pitch": round(math.degrees(math.asin(max(-1.0, min(1.0, fwd[2])))), 3),
        "player": [round(v, 4) for v in player],
        "player_yaw": round(compass(pfwd), 3),
        "dist": round(math.dist(pos, player), 4),
    }


def read(pid: int) -> dict:
    mem = live_state.Memory(pid)
    try:
        return sample(mem)
    finally:
        mem.close()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("pid", type=int)
    ap.add_argument("--log")
    ap.add_argument("--seconds", type=float, default=10.0)
    ap.add_argument("--interval", type=float, default=0.05)
    a = ap.parse_args()
    if not a.log:
        print(json.dumps(read(a.pid)))
        return 0
    mem = live_state.Memory(a.pid)
    end = time.time() + a.seconds
    with open(a.log, "w", encoding="utf-8") as fh:
        while time.time() < end:
            try:
                fh.write(json.dumps(sample(mem)) + "\n")
            except teleport.NotReady as exc:
                fh.write(json.dumps({"t": time.time(), "not_ready": str(exc)}) + "\n")
            time.sleep(a.interval)
    mem.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
