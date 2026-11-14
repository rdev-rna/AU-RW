"""List the world's loaded entities and their transforms from a running ACU (reads only).

    python tools/native_probe/world_entities.py <pid> <out.json> [--radius 200]

Every entity holds its World pointer at +0x60 (ACU-RE-0044: the player entity's back-pointer).
The private read/write heap is scanned for 8-byte-aligned copies of the current World pointer;
each hit at address p is accepted as an entity E = p - 0x60 when its world matrix (+0x20) is a
rigid transform: w column (0, 0, 0, 1), rotation rows of unit length, finite position within
--radius metres of the player. Entities with an AtomAnimComponent (vtable 0x142E7F780 owned by E,
ACU-RE-0025) are marked `animated` (people, animals); the rest are props, doors, triggers, etc.
Identity (observed 2026-11-14, hypothesis until joined with offline placements): [E + 0x130] is a
handle whose +0x10 holds the entity's own Entity (0984415e) file ID; names come from the local asset
index. `stem` drops the trailing instance number (GEN_ALL_CratesWine_01_008 -> GEN_ALL_CratesWine_01).
This is a heuristic inventory: an object that happens to store the World pointer at +0x60 and a
rigid matrix at +0x20 would pass too. Positions are entity origins, not collision bounds.
"""
from __future__ import annotations

import argparse
import ctypes as C
from ctypes import wintypes as W
import json
import math
from pathlib import Path
import re
import sqlite3
import struct
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import live_state  # noqa: E402

K = C.WinDLL("kernel32", use_last_error=True)
K.VirtualQueryEx.argtypes = [W.HANDLE, C.c_void_p, C.c_void_p, C.c_size_t]
K.VirtualQueryEx.restype = C.c_size_t


class MBI(C.Structure):
    _fields_ = [("BaseAddress", C.c_void_p), ("AllocationBase", C.c_void_p), ("AllocationProtect", W.DWORD),
                ("PartitionId", W.WORD), ("RegionSize", C.c_size_t), ("State", W.DWORD), ("Protect", W.DWORD),
                ("Type", W.DWORD)]


def heap_regions(mem: live_state.Memory):
    addr, m = 0, MBI()
    while addr < 0x7FFFFFFF0000 and K.VirtualQueryEx(mem.handle, C.c_void_p(addr), C.byref(m), C.sizeof(m)):
        base, size = m.BaseAddress or 0, m.RegionSize
        if m.State == 0x1000 and m.Type == 0x20000 and m.Protect in (0x04, 0x40):  # committed private RW
            yield base, size
        addr = base + size


def rigid(m: tuple) -> bool:
    if not all(math.isfinite(v) for v in m):
        return False
    if abs(m[3]) > 1e-4 or abs(m[7]) > 1e-4 or abs(m[11]) > 1e-4 or abs(m[15] - 1) > 1e-4:
        return False
    return all(abs(math.hypot(*m[i:i + 3]) - 1) < 0.02 for i in (0, 4, 8))


ENTITY_HANDLE, HANDLE_FILE_ID, ENTITY_TYPE = 0x130, 0x10, "0984415e"
INDEX = Path(__file__).resolve().parents[2] / "local/asset_index.sqlite"


def identity(mem: live_state.Memory, e: int, db) -> dict:
    handle = mem.u64(e + ENTITY_HANDLE)
    fid = mem.u64(handle + HANDLE_FILE_ID) if handle and 0x10000 < handle < 0x7FFFFFFFFFFF else None
    if not fid or not 0 < fid < 1 << 48:
        return {}
    row = db.execute("SELECT name, type_id FROM resource WHERE file_id = ? LIMIT 1", (f"{fid:016x}",)).fetchone()
    if not row or row[1] != ENTITY_TYPE:
        return {}
    return {"file_id": f"{fid:016x}", "name": row[0], "stem": re.sub(r"_\d+$", "", row[0] or "")}


def scan(pid: int, radius: float) -> dict:
    mem = live_state.Memory(pid)
    entity, world = live_state.player(mem)
    me = struct.unpack("<16f", mem.read(entity + live_state.ENTITY_MATRIX, 64))[12:15]
    needle = struct.pack("<Q", world)
    hits, scanned = set(), 0
    for base, size in heap_regions(mem):
        for off in range(0, size, 64 << 20):
            raw = mem.read(base + off, min(64 << 20, size - off))
            if not raw:
                continue
            scanned += len(raw)
            i = raw.find(needle)
            while i != -1:
                if i % 8 == 0:
                    hits.add(base + off + i - live_state.ENTITY_WORLD)
                i = raw.find(needle, i + 1)
    out = []
    db = sqlite3.connect(f"file:{INDEX}?mode=ro", uri=True)
    for e in sorted(hits):
        raw = mem.read(e + live_state.ENTITY_MATRIX, 64)
        if not raw:
            continue
        m = struct.unpack("<16f", raw)
        if not rigid(m) or math.dist(m[12:15], me) > radius:
            continue
        out.append({"entity": f"0x{e:X}", "position": [round(v, 3) for v in m[12:15]],
                    "forward": [round(v, 4) for v in m[4:7]], "up": [round(v, 4) for v in m[8:11]],
                    "animated": live_state.anim_component(mem, e) is not None, "player": e == entity,
                    **identity(mem, e, db)})
    mem.close()
    return {"world": f"0x{world:X}", "player": f"0x{entity:X}", "player_position": [round(v, 3) for v in me],
            "scanned_bytes": scanned, "pointer_hits": len(hits), "entities": out}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pid", type=int)
    ap.add_argument("output", type=Path)
    ap.add_argument("--radius", type=float, default=200)
    a = ap.parse_args()
    t = time.monotonic()
    r = scan(a.pid, a.radius)
    r["seconds"] = round(time.monotonic() - t, 1)
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(r, indent=1) + "\n", encoding="utf-8")
    ents = r["entities"]
    print(json.dumps({k: r[k] for k in ("scanned_bytes", "pointer_hits", "seconds")} |
                     {"entities": len(ents), "animated": sum(e["animated"] for e in ents),
                      "named": sum("name" in e for e in ents)}))


if __name__ == "__main__":
    main()
