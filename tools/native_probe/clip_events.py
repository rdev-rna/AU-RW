"""Dump each playing clip's runtime event-interval list from a running ACU (reads only, ACU-RE-0068).

    python tools/native_probe/clip_events.py <pid> <out.jsonl> [--hz 30] [--seconds N | --stop-file F]

Chain (ACU.exe 748abdc6…7e40):
- leaf rows and playback slots as in live_state.py (ACU-RE-0025): row state S, node slot u16 at
  node+0x64, playback = [[S+0x260] + 8*slot] (S+0x260 = S+608, the array sub_1400CAD80 reads);
- the clip's interval list: sub_1400CAD80 reads `*(*(S + 13896) + 2096*slot + 2064)`, a list
  object (u64 data, u16 count at +10) of 32-byte entries: u64 at +0, u32 id at +8, u32 +12,
  u32 +16, f32 start +20, f32 end +24 (clip seconds), u8 +28.
One line per newly seen (file id, list address) pair with every entry raw and decoded, plus
periodic lines with the clip times, so intervals can be compared with the clip's event tracks.
"""
from __future__ import annotations

import argparse
import json
import struct
import time
from pathlib import Path

from live_state import (ANIM_CONTROLLER, GRAPH_HEADER, PLAYBACK_LEAF_METHOD, Memory, anim_component,
                        player, qpc)

CLIP_RECORDS, RECORD_SIZE, RECORD_EVENTS, ENTRY = 13896, 2096, 2064, 32


def rows(mem: Memory, controller: int):
    header = mem.read(controller + GRAPH_HEADER, 16)
    if not header:
        return
    array, count = struct.unpack_from("<Q", header)[0], struct.unpack_from("<H", header, 10)[0]
    if not 0 < count <= 512:
        return
    raw = mem.read(array, count * 24) or b""
    for i in range(len(raw) // 24):
        state, node, weight, _span, kind, _ = struct.unpack_from("<QQfHBB", raw, i * 24)
        if kind:
            continue
        nraw = mem.read(node, 104)
        if not nraw or mem.u64(struct.unpack_from("<Q", nraw)[0] + 0x58) != PLAYBACK_LEAF_METHOD:
            continue
        slot = struct.unpack_from("<H", nraw, 0x64)[0]
        pbs = mem.u64(state + 0x260)
        pb = mem.u64(pbs + 8 * slot) if pbs and slot < 4096 else None
        praw = mem.read(pb, 0x388) if pb else None
        if not praw:
            continue
        fid = mem.u64(struct.unpack_from("<Q", praw, 0x28)[0] + 0x10)
        yield {"state": state, "slot": slot, "weight": weight, "file_id": f"{fid:016x}" if fid else None,
               "time": struct.unpack_from("<f", praw, 0x384)[0]}


def events(mem: Memory, state: int, slot: int):
    recs = mem.u64(state + CLIP_RECORDS)
    lst = mem.u64(recs + RECORD_SIZE * slot + RECORD_EVENTS) if recs else None
    head = mem.read(lst, 16) if lst else None
    if not head:
        return lst, None
    data, count = struct.unpack_from("<Q", head)[0], struct.unpack_from("<H", head, 10)[0]
    if count > 1024:
        return lst, None
    raw = mem.read(data, count * ENTRY) if count else b""
    out = []
    for k in range(count):
        e = raw[k * ENTRY:(k + 1) * ENTRY]
        p, eid, a, b, s, t, flag = struct.unpack_from("<QIIIffB", e)
        out.append({"id": f"{eid:08x}", "start": round(s, 5), "end": round(t, 5), "u12": a, "u16": b,
                    "flag": flag, "ptr": f"{p:#x}", "hex": e.hex()})
    return lst, out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pid", type=int)
    ap.add_argument("out", type=Path)
    ap.add_argument("--hz", type=float, default=30)
    ap.add_argument("--seconds", type=float)
    ap.add_argument("--stop-file", type=Path)
    a = ap.parse_args()
    mem = Memory(a.pid)
    seen: set = set()
    period, end = 1.0 / a.hz, (time.monotonic() + a.seconds) if a.seconds else None
    with a.out.open("w") as f:
        while not (end and time.monotonic() >= end or a.stop_file and a.stop_file.exists()):
            t0 = time.monotonic()
            found = player(mem)
            anim = anim_component(mem, found[0]) if found else None
            ctrl = mem.u64(anim + ANIM_CONTROLLER) if anim else None
            live = []
            for r in rows(mem, ctrl) if ctrl else ():
                live.append({k: r[k] for k in ("file_id", "slot", "weight", "time")})
                key = (r["file_id"], r["state"], r["slot"])
                if key not in seen:
                    seen.add(key)
                    lst, ev = events(mem, r["state"], r["slot"])
                    f.write(json.dumps({"kind": "clip", "qpc": qpc(), "file_id": r["file_id"], "slot": r["slot"],
                                        "state": f"{r['state']:#x}", "list": f"{lst or 0:#x}", "events": ev}) + "\n")
            f.write(json.dumps({"kind": "sample", "qpc": qpc(), "leaves": live}) + "\n")
            time.sleep(max(0.0, period - (time.monotonic() - t0)))


if __name__ == "__main__":
    main()
