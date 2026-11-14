"""Sample the player's AtomGraph parameter values from a running ACU (reads only, ACU-RE-0066).

    python tools/native_probe/graph_params.py <pid> <out.jsonl> --names local/atomgraph/params-newdemo.json
        [--hz 30] [--seconds N | --stop-file F] [--probe]

Parameter storage (sub_14009EFA0, ACU.exe 748abdc6…7e40): for the graph instance I,
    def = [I + 1656]; offsets = [def + 208] (u32 per runtime parameter slot);
    value = bytes at [I + 1024 + 32] + offset.
Observed live (ACU-RE-0067): the player's animation controller (ACU-RE-0025, component +0xD0) has
the graph def at +1656 (534 entries at [def+208]) and the value block at +1056. The block follows
the descriptors' own `f20` offsets: free run toggles HighProfile (1221), MovementSpeed (1656) reads
2.2 / 6.277, TimeScale (2196) 1.0 / 0.95. The table at [def+208] is NOT that layout (it places a
float at byte 109, inside a vector3); what it indexes is unresolved and is only reported here.
Each parameter is decoded at its f20 by type: bool = 1 byte, int = i32, scalar = f32, vector3 =
3 f32, quat = 4 f32 (other types are skipped).
Values are written lazily by per-parameter providers (sub_14010A2F0) when queried, so a parameter
that no active rule reads keeps its last value. `--probe` checks the chain without sampling.
Nothing is written to the game.
"""
from __future__ import annotations

import argparse
import json
import struct
import time
from pathlib import Path

from live_state import Memory, anim_component, player, qpc, ANIM_CONTROLLER

INSTANCE_DEF, DEF_OFFSETS, INSTANCE_VALUES = 1656, 208, 1024 + 32


def chain(mem: Memory) -> dict | None:
    found = player(mem)
    if not found:
        return None
    entity = found[0]
    anim = anim_component(mem, entity)
    ctrl = mem.u64(anim + ANIM_CONTROLLER) if anim else None
    gdef = mem.u64(ctrl + INSTANCE_DEF) if ctrl else None
    table = mem.u64(gdef + DEF_OFFSETS) if gdef else None
    values = mem.u64(ctrl + INSTANCE_VALUES) if ctrl else None
    return {"entity": entity, "anim": anim, "controller": ctrl, "def": gdef, "offsets": table, "values": values}


FORMATS = {"bool": "<B", "int": "<i", "scalar": "<f", "vector3": "<3f", "quat": "<4f"}


def decode(raw: bytes, off: int, typ: str):
    v = struct.unpack_from(FORMATS[typ], raw, off)
    if typ == "bool":
        return bool(v[0])
    if typ == "int":
        return v[0]
    v = [round(x, 5) for x in v]
    return v[0] if typ == "scalar" else v


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pid", type=int)
    ap.add_argument("out", type=Path)
    ap.add_argument("--names", type=Path, required=True, help="[{index,name,type}] from the graph")
    ap.add_argument("--hz", type=float, default=30)
    ap.add_argument("--seconds", type=float)
    ap.add_argument("--stop-file", type=Path)
    ap.add_argument("--probe", action="store_true")
    a = ap.parse_args()
    params = json.loads(a.names.read_text())
    mem = Memory(a.pid)
    c = chain(mem)
    if not c or not all(c.values()):
        raise SystemExit(f"chain incomplete: {c}")
    n = len(params)
    raw = mem.read(c["offsets"], 4 * n)
    if not raw:
        raise SystemExit(f"offset table unreadable at {c['offsets']:#x}")
    offsets = list(struct.unpack(f"<{n}I", raw))
    same = sorted(offsets) == sorted(p["f20"] for p in params)
    report = {k: f"{v:#x}" for k, v in c.items()} | {"count": n, "max_offset": max(offsets),
              "table_equals_descriptor_offsets": same}
    print(json.dumps(report))
    params = [p for p in params if p["type"] in FORMATS]
    span = max(p["f20"] for p in params) + 16
    if a.probe or span > 65536:
        return
    period, end = 1.0 / a.hz, (time.monotonic() + a.seconds) if a.seconds else None
    with a.out.open("w") as f:
        f.write(json.dumps({"chain": report, "params": params}) + "\n")
        while True:
            if end and time.monotonic() >= end or a.stop_file and a.stop_file.exists():
                break
            t0 = time.monotonic()
            block = mem.read(c["values"], span)
            if block:
                row = {"qpc": qpc(), "v": {p["name"]: decode(block, p["f20"], p["type"]) for p in params}}
                f.write(json.dumps(row) + "\n")
            time.sleep(max(0.0, period - (time.monotonic() - t0)))


if __name__ == "__main__":
    main()
