"""Bounded live coat state/code inspection; ACU-RE-0028. No hooks installed."""
import hashlib
import json
import os
import struct
from pathlib import Path

os.environ.setdefault("ACU_TAP_STEM", "acu_tap12")
import tap_client as t
import tap_events as events

ROOT = Path(__file__).resolve().parents[2] / "local/native-probe/s2-maxlookat"

def capture(pid):
    status = t.command(pid, "STATUS", timeout=5)
    base = int(status["image_base"], 16)
    objects = set()
    for e in events.parse(str(ROOT / f"frame-f2-{pid}.bin")):
        b = e["snaps"].get(20)
        if e["phase"] or not b:
            continue
        for i in range(struct.unpack_from("<I", b, 12)[0]):
            off = 544 + 544 * i
            if struct.unpack_from("<Q", b, off + 16)[0] == base + 0x2e77580:
                objects.add(struct.unpack_from("<Q", b, off)[0])
        break
    result = dict(pid=pid, image_base=hex(base), objects=[], code=[])
    for rva, size in [(0x45240, 714), (0x45510, 11368)]:
        raw = t.read(pid, base + rva, size)
        path = ROOT / f"coat-code-{rva:x}-{pid}.bin"
        path.write_bytes(raw)
        result["code"].append(dict(rva=hex(rva), size=size, path=str(path), sha256=hashlib.sha256(raw).hexdigest()))
    for obj in sorted(objects):
        raw = t.read(pid, obj, 336)
        solver = struct.unpack_from("<Q", raw, 72)[0]
        s = t.read(pid, solver, 112)
        array, = struct.unpack_from("<Q", s, 8)
        count, = struct.unpack_from("<H", s, 18)
        if count > 64:
            raise ValueError("unbounded chain")
        members = struct.unpack(f"<{count}Q", t.read(pid, array, count * 8))
        result["objects"].append(dict(address=hex(obj), hex=raw.hex(), solver=hex(solver),
            solver_hex=s.hex(), members=[dict(address=hex(p), hex=t.read(pid, p, 336).hex()) for p in members]))
    path = ROOT / f"coat-inspect-{pid}.json"
    path.write_text(json.dumps(result, indent=1))
    print(path)
    print("objects", len(objects), "chains", len({o['solver'] for o in result['objects']}))
    print(result["code"])

if __name__ == "__main__":
    import sys
    capture(int(sys.argv[1]))
