"""Read the player entity and its transform from a running ACU through the native probe.

    python tools/native_probe/player_state.py <pid> [--samples N --interval S]

Chain (static evidence, verified live by `check` before any value is trusted):
  getter 0x141CD0D30 (corpus 141CD0D30.c): holder = [0x145217368]; world = [holder + 0x98]
  12 corpus call sites read [world + 0xD8]                     -> entity  (ACUFixes: World.player)
  ACUFixes leads (unverified until this check): entity + 0x60 = World*, + 0x20 world matrix,
  + 0x50 position.
The cycle entity+0x60 == world is required before anything is reported as the player.
Reads only; nothing is written.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import struct
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from client import checked  # noqa: E402

WORLD_HOLDER_GLOBAL = 0x145217368
HOLDER_WORLD = 0x98
WORLD_PLAYER = 0xD8
ENTITY_WORLD = 0x60
ENTITY_MATRIX = 0x20  # 4x4 float, row 3 (+0x50) = position (lead)


def read(pid: int, va: int, n: int) -> bytes | None:
    try:
        return bytes.fromhex(checked(pid, f"READ {va:x} {n}")["hex"])
    except Exception:
        return None


def u64(pid: int, va: int) -> int | None:
    raw = read(pid, va, 8)
    return struct.unpack("<Q", raw)[0] if raw else None


def chain(pid: int) -> dict:
    out = {"holder": None, "world": None, "entity": None, "cycle": False}
    holder = u64(pid, WORLD_HOLDER_GLOBAL)
    out["holder"] = holder and f"0x{holder:X}"
    world = u64(pid, holder + HOLDER_WORLD) if holder else None
    out["world"] = world and f"0x{world:X}"
    entity = u64(pid, world + WORLD_PLAYER) if world else None
    out["entity"] = entity and f"0x{entity:X}"
    back = u64(pid, entity + ENTITY_WORLD) if entity else None
    out["cycle"] = bool(entity) and back == world
    if out["cycle"]:
        raw = read(pid, entity + ENTITY_MATRIX, 64)
        if raw:
            m = struct.unpack("<16f", raw)
            out["matrix_rows"] = [list(m[i:i + 4]) for i in range(0, 16, 4)]
            out["position"] = list(m[12:15])
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pid", type=int)
    ap.add_argument("--samples", type=int, default=1)
    ap.add_argument("--interval", type=float, default=0.5)
    a = ap.parse_args()
    for i in range(a.samples):
        print(json.dumps({"t": round(time.time(), 2), **chain(a.pid)}))
        if i + 1 < a.samples:
            time.sleep(a.interval)


if __name__ == "__main__":
    main()
