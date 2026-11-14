"""Travel to another world (Paris, Versailles, ...) through the game's own world-portal path (ACU-RE-0049).

    python tools/native_probe/world_travel.py <pid> find  [--world ACU_Paris]
    python tools/native_probe/world_travel.py <pid> load  [--world ACU_Paris] [--destination <file id>]
    python tools/native_probe/world_travel.py <pid> go    ...   # mode 3: crashed the game, do not use

Static route (corpus, ACU.exe SHA-256 748abdc6...7e40):
  0x141E42C00(entry)  portal / fast-travel entry; ends with
      0x141D60C90([0x145219860], &entry[+72] world handle, &entry[+80] destination handle)
  0x141D60C90 stores the target world (+592) and destination (+600) in the travel manager,
  sets state +644 = 1 and requests the transition world (0x141CE1820(&off_1450A9600, 1) ->
  0x141CE1840 queues command 5 in the World holder [0x145217368]; 0x141CCA250 case 5 swaps
  holder+0x90/+0x98), then 0x141CD8A00(target world id, 1, 0, 6, destination id, 1).
World resources (type fbb63e47, local asset index): ACU_Paris 0000000f18b7e1cf, ACU_Versailles
0000000e84b48064, ACU_Prologue 00000010a04d134d, ACU_RebirthRitual 0000000fc9a64e04,
ACU-DLC_Saint-Denis 000000135dce49f2, ACU_LGS_* (Belle Epoque, Medieval, WorldWar2).

`find`: heap scan for handles whose +16 is the world's file ID (handle layout as used above: +0 object,
+8/+12 counts, +16 file ID), then for objects holding such a handle at +72 whose +80 is another handle;
the destination's file ID is named from the asset index. `go`: the focus shim (mode 3) checks the entry's
world handle carries the world ID and makes the game's call on the window thread.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sqlite3
import struct
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import live_state  # noqa: E402
import teleport  # noqa: E402
import world_entities  # noqa: E402

WORLDS = {"ACU_Paris": 0x0000000F18B7E1CF, "ACU_Versailles": 0x0000000E84B48064,
          "ACU_Prologue": 0x00000010A04D134D, "ACU_RebirthRitual": 0x0000000FC9A64E04,
          "ACU-DLC_Saint-Denis": 0x000000135DCE49F2}
MODE_TRAVEL, MODE_LOAD, MODE_START = 3, 4, 5  # mode 3 crashed the game (ACU-RE-0049); use load


def scan_qword(mem: live_state.Memory, value: int, limit: int = 4096) -> list[int]:
    needle, hits = struct.pack("<Q", value), []
    for base, size in world_entities.heap_regions(mem):
        for off in range(0, size, 64 << 20):
            raw = mem.read(base + off, min(64 << 20, size - off))
            if not raw:
                continue
            i = raw.find(needle)
            while i != -1 and len(hits) < limit:
                if i % 8 == 0:
                    hits.append(base + off + i)
                i = raw.find(needle, i + 1)
    return hits


def name_of(db, fid: int) -> str | None:
    row = db.execute("SELECT name, type_id FROM resource WHERE file_id = ? LIMIT 1", (f"{fid:016x}",)).fetchone()
    return row and f"{row[0]} ({row[1]})"


def find(pid: int, world: str) -> dict:
    mem = live_state.Memory(pid)
    db = sqlite3.connect(f"file:{world_entities.INDEX}?mode=ro", uri=True)
    try:
        wid = WORLDS[world]
        handles = [h - 16 for h in scan_qword(mem, wid)]
        entries = []
        for h in handles:
            for ref in scan_qword(mem, h, limit=256):
                entry = ref - 72
                dest = mem.u64(entry + 80)
                dfid = mem.u64(dest + 16) if dest and 0x10000 < dest < 0x7FFFFFFFFFFF else None
                if dfid and 0 < dfid < 1 << 48:
                    entries.append({"entry": f"0x{entry:X}", "world_handle": f"0x{h:X}",
                                    "destination": f"{dfid:016x}", "destination_name": name_of(db, dfid),
                                    "entry_vtable": f"0x{mem.u64(entry) or 0:X}"})
        return {"world": world, "world_id": f"{wid:016x}", "handles": [f"0x{h:X}" for h in handles],
                "entries": entries}
    finally:
        mem.close()


def go(pid: int, world: str, entry: int) -> dict:
    shared = teleport.Shared(pid)
    try:
        return shared.call(entry, WORLDS[world], [0.0] * 16, mode=MODE_TRAVEL, timeout=10)
    finally:
        shared.close()


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pid", type=int)
    ap.add_argument("mode", choices=["find", "go", "load", "start"])
    ap.add_argument("--destination", type=lambda s: int(s, 16), default=0,
                    help="destination file ID for load (e.g. a PlayerPortal in that world); 0 = none")
    ap.add_argument("--world", default="ACU_Paris", choices=sorted(WORLDS))
    ap.add_argument("--entry", type=lambda s: int(s, 16))
    a = ap.parse_args()
    if a.mode == "find":
        print(json.dumps(find(a.pid, a.world), indent=1))
        return
    if a.mode == "start":
        mem = live_state.Memory(a.pid)
        world = mem.u64(mem.u64(live_state.WORLD_HOLDER_GLOBAL) + live_state.HOLDER_WORLD)
        mem.close()
        shared = teleport.Shared(a.pid)
        try:
            r = shared.call(0, world, [0.0] * 16, mode=MODE_START, timeout=10)
        finally:
            shared.close()
        print(json.dumps({"world": f"0x{world:X}", **r}))
        return
    if a.mode == "load":
        shared = teleport.Shared(a.pid)
        try:
            r = shared.call(a.destination, WORLDS[a.world], [0.0] * 16, mode=MODE_LOAD, timeout=10)
        finally:
            shared.close()
        print(json.dumps({"world": a.world, "destination": f"{a.destination:016x}", **r}))
        return
    entry = a.entry
    if entry is None:
        found = find(a.pid, a.world)["entries"]
        if not found:
            raise SystemExit("no portal entry for that world in memory")
        entry = int(found[0]["entry"], 16)
    t = time.monotonic()
    print(json.dumps({"entry": f"0x{entry:X}", **go(a.pid, a.world, entry), "s": round(time.monotonic() - t, 2)}))


if __name__ == "__main__":
    main()
