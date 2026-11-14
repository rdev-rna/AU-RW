"""Sample the player's transform and playing animation clips from a running ACU (reads only).

    python tools/native_probe/live_state.py <pid> <out.jsonl> [--hz 30] [--seconds N | --stop-file F]

Each line: QPC (same clock as the native probe's events), position, forward, and every recognised
animation-graph leaf with its clip file ID, graph weight and playback time.

Evidence for each read:
- Player entity and transform: ACU-RE-0044 (`player_state.py` chain, entity+0x20 matrix).
- Animation component vtable 0x142E7F780, owner +0x10, controller +0xD0 (ACU-RE-0025).
- Graph rows at controller+0x6A8 (array +0, u16 count +10), 24 bytes:
  state u64, node u64, weight f32, span u16, kind u8 (0 = leaf) (ACU-RE-0025, run 49).
- Leaf node vtable method +0x58 == 0x1400BCC20 (playback leaf), slot u16 at node+0x64,
  playback = [[state+0x260] + 8*slot], asset handle [playback+0x28], file ID u64 at handle+0x10,
  playback time f32 at +0x384, offset f32 at +0x36C (ACU-RE-0025 focused IDA/corpus checks).
Graph weights are presence/blend inputs, not proven final contribution (ACU-RE-0025 limits).
Reads use ReadProcessMemory: much faster than probe round trips; nothing is written.
"""
from __future__ import annotations

import argparse
import ctypes as C
from ctypes import wintypes as W
import json
from pathlib import Path
import struct
import time

K = C.WinDLL("kernel32", use_last_error=True)
K.OpenProcess.restype = W.HANDLE
K.ReadProcessMemory.argtypes = [W.HANDLE, C.c_void_p, C.c_void_p, C.c_size_t, C.POINTER(C.c_size_t)]
K.QueryPerformanceCounter.argtypes = [C.POINTER(C.c_int64)]

WORLD_HOLDER_GLOBAL, HOLDER_WORLD, WORLD_PLAYER, ENTITY_WORLD = 0x145217368, 0x98, 0xD8, 0x60
ENTITY_MATRIX, ENTITY_COMPONENTS = 0x20, 0x78
ANIM_COMPONENT_VTABLE, ANIM_CONTROLLER = 0x142E7F780, 0xD0
GRAPH_HEADER, PLAYBACK_LEAF_METHOD = 0x6A8, 0x1400BCC20
# Travel manager (ACU-RE-0049): state +644 is 1 from the travel request until the player is in the
# new World, then 0. Every arrival builds a new World object and a new player entity (obs. 15).
TRAVEL_MANAGER_GLOBAL, TRAVEL_STATE = 0x145219860, 644


def qpc() -> int:
    v = C.c_int64()
    K.QueryPerformanceCounter(C.byref(v))
    return v.value


class Memory:
    def __init__(self, pid: int):
        self.handle = K.OpenProcess(0x0010 | 0x0400, False, pid)  # VM_READ | QUERY_INFORMATION
        if not self.handle:
            raise OSError(f"cannot open process {pid} for reading")

    def read(self, va: int, n: int) -> bytes | None:
        if not va or n <= 0:
            return None
        buf = C.create_string_buffer(n)
        got = C.c_size_t()
        ok = K.ReadProcessMemory(self.handle, C.c_void_p(va), buf, n, C.byref(got))
        return buf.raw if ok and got.value == n else None

    def u64(self, va: int) -> int | None:
        raw = self.read(va, 8)
        return struct.unpack("<Q", raw)[0] if raw else None

    def close(self):
        K.CloseHandle(self.handle)


def travel_state(mem: Memory) -> int | None:
    """Travel manager state (+644): 0 idle, 1 travelling; None if unreadable."""
    manager = mem.u64(TRAVEL_MANAGER_GLOBAL)
    raw = mem.read(manager + TRAVEL_STATE, 4) if manager else None
    return struct.unpack("<i", raw)[0] if raw else None


def player(mem: Memory) -> tuple[int, int] | None:
    """(entity, world) when the ACU-RE-0044 cycle holds."""
    holder = mem.u64(WORLD_HOLDER_GLOBAL)
    world = mem.u64(holder + HOLDER_WORLD) if holder else None
    entity = mem.u64(world + WORLD_PLAYER) if world else None
    if entity and mem.u64(entity + ENTITY_WORLD) == world:
        return entity, world
    return None


def anim_component(mem: Memory, entity: int) -> int | None:
    array = mem.u64(entity + ENTITY_COMPONENTS)
    raw = mem.read(array, 64 * 8) if array else None
    for ptr in struct.unpack("<64Q", raw) if raw else ():
        if ptr and mem.u64(ptr) == ANIM_COMPONENT_VTABLE and mem.u64(ptr + 0x10) == entity:
            return ptr
    return None


def leaves(mem: Memory, controller: int) -> list[dict]:
    header = mem.read(controller + GRAPH_HEADER, 16)
    if not header:
        return []
    array, count = struct.unpack_from("<Q", header)[0], struct.unpack_from("<H", header, 10)[0]
    if not 0 < count <= 512:
        return []
    rows = mem.read(array, count * 24) or b""
    out = []
    for i in range(len(rows) // 24):
        state, node, weight, _span, kind, _ = struct.unpack_from("<QQfHBB", rows, i * 24)
        if kind != 0:
            continue
        node_raw = mem.read(node, 104)
        if not node_raw:
            continue
        vt = struct.unpack_from("<Q", node_raw)[0]
        if mem.u64(vt + 0x58) != PLAYBACK_LEAF_METHOD:
            continue
        slot = struct.unpack_from("<H", node_raw, 0x64)[0]
        playbacks = mem.u64(state + 0x260)
        playback = mem.u64(playbacks + 8 * slot) if playbacks and slot < 4096 else None
        raw = mem.read(playback, 0x388) if playback else None
        if not raw:
            continue
        handle = struct.unpack_from("<Q", raw, 0x28)[0]
        file_id = mem.u64(handle + 0x10)
        if file_id is None:
            continue
        out.append({"row": i, "file_id": f"{file_id:016x}", "weight": round(weight, 4),
                    "time": round(struct.unpack_from("<f", raw, 0x384)[0], 4),
                    "offset": round(struct.unpack_from("<f", raw, 0x36C)[0], 4)})
    return out


def sample(mem: Memory, cache: dict) -> dict:
    t = qpc()
    found = player(mem)
    if not found:
        return {"qpc": t, "player": None}
    entity, _ = found
    if cache.get("entity") != entity:
        cache.update(entity=entity, anim=anim_component(mem, entity))
    m = mem.read(entity + ENTITY_MATRIX, 64)
    rows = struct.unpack("<16f", m) if m else None
    controller = mem.u64(cache["anim"] + ANIM_CONTROLLER) if cache.get("anim") else None
    return {"qpc": t, "entity": f"0x{entity:X}",
            "position": [round(x, 4) for x in rows[12:15]] if rows else None,
            "forward": [round(x, 4) for x in rows[4:7]] if rows else None,
            "clips": leaves(mem, controller) if controller else None}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pid", type=int)
    ap.add_argument("output", type=Path)
    ap.add_argument("--hz", type=float, default=30)
    ap.add_argument("--seconds", type=float)
    ap.add_argument("--stop-file", type=Path)
    a = ap.parse_args()
    freq = C.c_int64()
    K.QueryPerformanceFrequency(C.byref(freq))
    mem, cache = Memory(a.pid), {}
    end = time.monotonic() + a.seconds if a.seconds else None
    a.output.parent.mkdir(parents=True, exist_ok=True)
    with a.output.open("w", encoding="utf-8") as out:
        out.write(json.dumps({"kind": "header", "pid": a.pid, "qpc_frequency": freq.value, "hz": a.hz}) + "\n")
        while (end is None or time.monotonic() < end) and not (a.stop_file and a.stop_file.exists()):
            start = time.monotonic()
            out.write(json.dumps({"kind": "sample", **sample(mem, cache)}) + "\n")
            time.sleep(max(0.0, 1 / a.hz - (time.monotonic() - start)))
    mem.close()


if __name__ == "__main__":
    main()
