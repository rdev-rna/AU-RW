r"""Teleport Arno: write the player entity's matrix, then reset the Human's behaviour (ACU-RE-0046).

    python tools/native_probe/teleport.py <pid> to X Y Z [--yaw DEG] [--settle 1.0]
    python tools/native_probe/teleport.py <pid> where
    python tools/native_probe/teleport.py <pid> invisible on|off     # enemies ignore Arno
    python tools/native_probe/teleport.py <pid> godmode on|off
    python tools/native_probe/teleport.py <pid> keep-safe [--seconds N]  # re-arm both after every arrival
    python tools/native_probe/teleport.py <pid> spawn-box X Y Z [--half 100 100 0.5] [--yaw DEG]
                                          # static collision box, centre X Y Z (shim mode 8, ACU-RE-0103)

Route (binary SHA-256 748abdc6...7e40, image base 0x140000000):
  - The debug menu's "Teleport Character" (0x140EF73D0 -> 0x140EE11B0) asks the Human for
    interface 13 (0x14131B5E0 -> 0x141EC18C0); in retail that returns null (live, 2026-11-14), so
    the debug path is unavailable.
  - Instead (ACUFixes lead, re-derived): write the entity world matrix (+0x20, position row
    +0x50) and call the Human's behaviour reset 0x14131A530 so movement restarts from there.
  - Human = [AtomAnimComponent (vtable 0x142E7F780) + 0xC58], vtable 0x142FB2010. Confirmed
    live twice: same pointer as the game's debug manager (vtable 0x142F57850) field +0x20.
The focus shim (acu_focus*.dll in the game) runs both steps on the game window's thread inside
SEH when a request is written to Local\acu-teleport-<pid>; it checks the Human vtable and the
entity's world back-pointer first. Arno lands where he is put: pick Z on known ground.

World travel (ACU-RE-0049 obs. 14/15): every arrival builds a new World and a new player entity, so
pointers taken before a trip are stale and per-entity flags (Invisible, God Mode) are gone. For ~20 s
there is no World or no player. Every call here resolves the player fresh and requires `ready()`
(travel manager idle and the World chain consistent); shim build t8+ re-checks the same on the game
thread and answers -5 (travelling) or -6 (not the current player). `keep-safe` re-arms both flags
whenever a new player entity appears.
"""
from __future__ import annotations

import argparse
import ctypes as C
import json
import math
from pathlib import Path
import struct
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import live_state  # noqa: E402

K = C.WinDLL("kernel32", use_last_error=True)
K.OpenFileMappingW.restype = C.c_void_p
K.OpenFileMappingW.argtypes = [C.c_uint32, C.c_int, C.c_wchar_p]
K.MapViewOfFile.restype = C.c_void_p
K.MapViewOfFile.argtypes = [C.c_void_p, C.c_uint32, C.c_uint32, C.c_uint32, C.c_size_t]
K.UnmapViewOfFile.argtypes = [C.c_void_p]
K.CloseHandle.argtypes = [C.c_void_p]

LAYOUT = struct.Struct("<IIIiQQ16fII")  # magic request done status human entity matrix exception mode
MAGIC = 0x54554341  # "ACUT"
HUMAN_OFFSET, HUMAN_VTABLE = 0xC58, 0x142FB2010  # in the AtomAnimComponent; ACU-RE-0046
BEHAVIOUR_VTABLE, INVISIBLE_BYTE = 0x142F08230, 208  # debug "Invisible :" flag (ACU-RE-0046)
MODE_INVISIBLE, MODE_GOD, MODE_SPAWN_BOX = 1, 2, 8
FALL_M = 1.5  # metres below the target after settling = fell through unstreamed ground
STATUS = {1: "done", -1: "not the Human (vtable check)", -2: "not an entity", -3: "exception",
          -4: "unknown mode", -5: "world travel in progress (shim t8+)",
          -6: "not the current World's player (stale pointer; shim t8+)",
          -7: "spawn-box: vtable check failed after construction (nothing added)"}


class NotReady(RuntimeError):
    """The world is changing or the player chain is not consistent; nothing was sent to the game."""


def ready(mem: live_state.Memory) -> tuple[int, int]:
    """(entity, world) when the travel manager is idle and the ACU-RE-0044 chain holds; else NotReady."""
    state = live_state.travel_state(mem)
    if state != 0:
        raise NotReady(f"world travel in progress (travel manager state {state})")
    found = live_state.player(mem)
    if not found:
        raise NotReady("no current player (loading, travelling or not in the world)")
    return found


def wait_ready(pid: int, timeout: float = 180, still_s: float = 1.0) -> dict | None:
    """Wait until ready() holds and the same player entity has stayed put for still_s seconds."""
    end, last, since = time.monotonic() + timeout, None, None
    while time.monotonic() < end:
        mem = live_state.Memory(pid)
        try:
            try:
                entity, world = ready(mem)
                raw = mem.read(entity + live_state.ENTITY_MATRIX + 48, 12)
                position = struct.unpack("<3f", raw) if raw else None
            except NotReady:
                entity = world = position = None
        finally:
            mem.close()
        key = (entity, world)
        if entity and position and math.hypot(*position) > 1:
            if key != last:
                last, since = key, time.monotonic()
            elif time.monotonic() - since >= still_s:
                return {"entity": f"0x{entity:X}", "world": f"0x{world:X}",
                        "position": [round(v, 3) for v in position]}
        else:
            last = since = None
        time.sleep(0.25)
    return None


class Shared:
    def __init__(self, pid: int):
        self.handle = K.OpenFileMappingW(0x0002 | 0x0004, False, f"Local\\acu-teleport-{pid}")
        if not self.handle:
            raise OSError("teleport section not found: is the rebuilt focus shim (with teleport) installed?")
        self.view = K.MapViewOfFile(self.handle, 0x0002 | 0x0004, 0, 0, LAYOUT.size)
        if not self.view:
            raise OSError("cannot map the teleport section")

    def read(self) -> tuple:
        return LAYOUT.unpack(C.string_at(self.view, LAYOUT.size))

    def call(self, human: int, entity: int, matrix: list[float], timeout: float = 5.0, mode: int = 0) -> dict:
        _, request, done, *_ = self.read()
        request = done + 1
        body = LAYOUT.pack(MAGIC, done, done, 0, human, entity, *matrix, 0, mode)
        C.memmove(self.view, body, LAYOUT.size)
        C.memmove(self.view + 4, struct.pack("<I", request), 4)  # publish the request last
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            row = self.read()
            if row[2] == request:
                status = row[3]
                return {"status": status, "meaning": STATUS.get(status, "?"), "readback": row[7],
                        "exception": f"0x{row[22]:08X}" if row[22] else None}
            time.sleep(0.02)
        return {"status": None, "meaning": "not serviced (window thread busy or shim not polling)"}

    def close(self):
        K.UnmapViewOfFile(self.view)
        K.CloseHandle(self.handle)


def human(mem: live_state.Memory, entity: int) -> int:
    anim = live_state.anim_component(mem, entity)
    h = mem.u64(anim + HUMAN_OFFSET) if anim else None
    if not h or mem.u64(h) != HUMAN_VTABLE:
        raise RuntimeError(f"player Human not found (anim component {anim and hex(anim)}, human {h and hex(h)})")
    return h


def behaviour_component(mem: live_state.Memory, entity: int) -> int:
    array = mem.u64(entity + live_state.ENTITY_COMPONENTS)
    count = struct.unpack("<H", mem.read(entity + 0x82, 2))[0]
    for ptr in struct.unpack(f"<{count}Q", mem.read(array, 8 * count)):
        if ptr and mem.u64(ptr) == BEHAVIOUR_VTABLE and mem.u64(ptr + 0x10) == entity:
            return ptr
    raise RuntimeError("behaviour component (vtable 0x142F08230) not found on the player")


def set_flag(pid: int, mode: int, on: bool) -> dict:
    """mode 1: Invisible (enemies ignore Arno); mode 2: God Mode. Same calls as the debug menu."""
    mem, shared = live_state.Memory(pid), Shared(pid)
    try:
        entity, _ = ready(mem)
        comp = behaviour_component(mem, entity) if mode == MODE_INVISIBLE else 0
        r = shared.call(comp, entity, [1.0 if on else 0.0] + [0.0] * 15, mode=mode)
        if mode == MODE_INVISIBLE:
            r["byte_208"] = mem.read(comp + INVISIBLE_BYTE, 1)[0]
        return {"mode": {1: "invisible", 2: "god"}[mode], "requested": on, **r}
    finally:
        shared.close()
        mem.close()


def safety_on(pid: int) -> dict:
    return {"invisible": set_flag(pid, MODE_INVISIBLE, True), "god": set_flag(pid, MODE_GOD, True)}


class SafetyKeeper:
    """Keep Invisible and God Mode on the current player across world travel, deaths and reloads.

    Flags belong to the player entity, and every arrival makes a new one (ACU-RE-0049 obs. 15).
    `check()` re-arms both when the player entity differs from the one last armed. It waits for
    ready() and does nothing while travelling.
    """

    def __init__(self, pid: int):
        self.pid, self.armed_entity = pid, None

    def check(self) -> dict | None:
        mem = live_state.Memory(self.pid)
        try:
            entity, _ = ready(mem)
        except NotReady:
            return None
        finally:
            mem.close()
        if entity == self.armed_entity:
            return None
        result = safety_on(self.pid)
        ok = all(r.get("status") == 1 for r in result.values())
        if ok:
            self.armed_entity = entity
        return {"entity": f"0x{entity:X}", "armed": ok, **result}


def spawn_box(pid: int, centre: list[float], half: list[float], yaw_deg: float = 0.0) -> dict:
    """Shim mode 8: a static InertComponent entity with a BoxShape, added to the current World.

    The entity is never removed (it lives until the World unloads). centre is world metres (Z up);
    half = half extents along the box's x, y, z (each at least 0.01, the BoxShape setter's floor).
    """
    mem, shared = live_state.Memory(pid), Shared(pid)
    try:
        ready(mem)
        _, request, done, *_ = shared.read()
        request = done + 1
        matrix = [*half, *centre, math.radians(yaw_deg)] + [0.0] * 9
        C.memmove(shared.view, LAYOUT.pack(MAGIC, done, done, 0, 0, 0, *matrix, 0, MODE_SPAWN_BOX), LAYOUT.size)
        C.memmove(shared.view + 4, struct.pack("<I", request), 4)
        end = time.monotonic() + 5
        while time.monotonic() < end:
            row = shared.read()
            if row[2] == request:
                raw = C.string_at(shared.view + 32 + 8 * 4, 16)  # matrix[8..11]
                bodies = struct.unpack("<2Q", raw)
                status = row[3]
                return {"status": status, "meaning": STATUS.get(status, "spawned" if status == 1 else "?"),
                        "entity": f"0x{row[5]:X}", "component": f"0x{row[4]:X}",
                        "rigid_body_98_a0": [f"0x{b:X}" for b in bodies], "centre": centre, "half": half,
                        "yaw_deg": yaw_deg, "exception": f"0x{row[22]:08X}" if row[22] else None}
            time.sleep(0.02)
        return {"status": None, "meaning": "not serviced (window thread busy or shim without mode 8)"}
    finally:
        shared.close()
        mem.close()


def target_matrix(rows: list[list[float]], position: list[float], yaw_deg: float | None) -> list[float]:
    if yaw_deg is None:
        r0, r1, r2 = rows[0], rows[1], rows[2]
    else:  # Z up, row 1 = forward (ACU-RE-0044); row 0 = right = forward x up
        y = math.radians(yaw_deg)
        r1 = [math.cos(y), math.sin(y), 0.0, 0.0]
        r0 = [math.sin(y), -math.cos(y), 0.0, 0.0]
        r2 = [0.0, 0.0, 1.0, 0.0]
    return [*r0[:3], 0.0, *r1[:3], 0.0, *r2[:3], 0.0, *position, 1.0]


def teleport(pid: int, x: float, y: float, z: float, yaw_deg: float | None = None, settle: float = 1.0,
             retries: int = 3) -> dict:
    mem = live_state.Memory(pid)
    shared = Shared(pid)
    try:
        entity, _ = ready(mem)
        h = human(mem, entity)
        m = struct.unpack("<16f", mem.read(entity + live_state.ENTITY_MATRIX, 64))
        rows = [list(m[i:i + 4]) for i in range(0, 16, 4)]
        before = rows[3][:3]
        # Fall-through guard (user report, 2026-11-14: after a long teleport the ground's collision
        # may not be streamed in yet and Arno falls through). After settling, a position more than
        # FALL_M below the target counts as a fall: teleport again and wait longer, up to retries.
        attempts = []
        for attempt in range(1 + retries):
            result = shared.call(h, entity, target_matrix(rows, [x, y, z], yaw_deg))
            wait = settle * (2 ** attempt)
            time.sleep(wait)
            after = struct.unpack("<16f", mem.read(entity + live_state.ENTITY_MATRIX, 64))
            fell = after[14] < z - FALL_M
            attempts.append({"settle_s": wait, "z_after": round(after[14], 3), "fell": fell})
            if result.get("status") != 1 or not fell:
                break
        fwd = after[4:6]
        return {"before": [round(v, 3) for v in before], "target": [x, y, z], "yaw_deg": yaw_deg,
                "after": [round(v, 3) for v in after[12:15]],
                "heading_deg": round(math.degrees(math.atan2(fwd[1], fwd[0])), 1),
                "error_m": round(math.dist(after[12:15], (x, y, z)), 3), "fell_through": attempts[-1]["fell"],
                "attempts": attempts, **result}
    finally:
        shared.close()
        mem.close()


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pid", type=int)
    sub = ap.add_subparsers(dest="mode", required=True)
    sub.add_parser("where")
    for name in ("invisible", "godmode"):
        f = sub.add_parser(name)
        f.add_argument("state", choices=["on", "off"])
    k = sub.add_parser("keep-safe")
    k.add_argument("--seconds", type=float, default=0, help="0 = until interrupted")
    k.add_argument("--interval", type=float, default=1.0)
    t = sub.add_parser("to")
    t.add_argument("x", type=float)
    t.add_argument("y", type=float)
    t.add_argument("z", type=float)
    t.add_argument("--yaw", type=float)
    t.add_argument("--settle", type=float, default=1.0)
    b = sub.add_parser("spawn-box")
    b.add_argument("x", type=float)
    b.add_argument("y", type=float)
    b.add_argument("z", type=float)
    b.add_argument("--half", type=float, nargs=3, default=[100.0, 100.0, 0.5])
    b.add_argument("--yaw", type=float, default=0.0)
    a = ap.parse_args()
    if a.mode == "spawn-box":
        print(json.dumps(spawn_box(a.pid, [a.x, a.y, a.z], a.half, a.yaw)))
        return
    if a.mode in ("invisible", "godmode"):
        print(json.dumps(set_flag(a.pid, MODE_INVISIBLE if a.mode == "invisible" else MODE_GOD, a.state == "on")))
        return
    if a.mode == "keep-safe":
        keeper, end = SafetyKeeper(a.pid), (time.monotonic() + a.seconds if a.seconds else None)
        while end is None or time.monotonic() < end:
            event = keeper.check()
            if event:
                print(json.dumps({"t": round(time.time(), 3), **event}), flush=True)
            time.sleep(a.interval)
        return
    if a.mode == "where":
        mem = live_state.Memory(a.pid)
        entity, _ = ready(mem)
        m = struct.unpack("<16f", mem.read(entity + live_state.ENTITY_MATRIX, 64))
        print(json.dumps({"entity": f"0x{entity:X}", "human": f"0x{human(mem, entity):X}",
                          "position": [round(v, 3) for v in m[12:15]],
                          "heading_deg": round(math.degrees(math.atan2(m[5], m[4])), 1)}))
        return
    print(json.dumps(teleport(a.pid, a.x, a.y, a.z, a.yaw, a.settle)))


if __name__ == "__main__":
    main()
