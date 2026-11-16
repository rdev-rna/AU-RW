"""inventory_blob.py — decode ACU's replicated player loadout (PlayerInventoryData memento, ACU-RE-0105).

    python tools/coop_lan/inventory_blob.py items <blob.bin | gearwatch.jsonl> [--db local/asset_index.sqlite]
    python tools/coop_lan/inventory_blob.py diff <old.bin> <new.bin> [--db ...]

The memento field is a byte array: u32 decoded size, then ACU's compressed frame (magic 0x1004fa9957fbaa33, the
same framing as Forge data, ACU-RE-0002). Default-path blocks are LZO1X with an Adler-32 variant whose sums start
at 0 (acu-rs/crates/acu_assets/src/frame.rs). The decoded object lists the loadout as asset references; this tool
reports every aligned-or-not u64 that the asset index names (file IDs as 16-digit hex strings).
Bounded: at most 1 MiB decoded, offsets and lengths checked, unsupported variants fail."""
from __future__ import annotations

import argparse
import json
import sqlite3
import struct
import sys
from pathlib import Path

MAGIC = 0x1004FA9957FBAA33
MAX_DECODED = 1 << 20


class BlobError(ValueError):
    pass


def adler0(data: bytes) -> int:
    a = b = 0
    for x in data:
        a = (a + x) % 65521
        b = (b + a) % 65521
    return b << 16 | a


def lzo1x(src: bytes, out_len: int) -> bytes:
    """LZO1X decoder written from the published stream format; checks every bound."""
    if out_len > MAX_DECODED:
        raise BlobError("decoded length over limit")
    out = bytearray()
    ip = 0
    n = len(src)

    def need(k: int) -> None:
        if ip + k > n:
            raise BlobError("input overrun")

    def byte() -> int:
        nonlocal ip
        need(1)
        ip += 1
        return src[ip - 1]

    def run(base: int) -> int:  # zero-extended length
        cnt = base
        while True:
            b = byte()
            if b:
                return cnt + b
            cnt += 255

    def lit(cnt: int) -> None:
        nonlocal ip
        need(cnt)
        if len(out) + cnt > out_len:
            raise BlobError("output overrun")
        out.extend(src[ip:ip + cnt])
        ip += cnt

    def copy(dist: int, cnt: int) -> None:
        s = len(out) - dist
        if s < 0 or dist <= 0:
            raise BlobError("lookbehind overrun")
        if len(out) + cnt > out_len:
            raise BlobError("output overrun")
        for i in range(cnt):
            out.append(out[s + i])

    state = 0
    if n and src[0] > 17:
        ip = 1
        k = src[0] - 17
        lit(k)
        state = 4 if k >= 4 else k
    while True:
        t = byte()
        if t >= 64:  # M2
            dist = ((t >> 2) & 7) + (byte() << 3) + 1
            copy(dist, (t >> 5) + 1)
        elif t >= 32:  # M3
            cnt = (t & 31) or run(31)
            d = byte() | byte() << 8
            copy((d >> 2) + 1, cnt + 2)
            t = d
        elif t >= 16:  # M4 or end of stream
            cnt = (t & 7) or run(7)
            d = byte() | byte() << 8
            dist = ((t & 8) << 11) + (d >> 2)
            if dist == 0:
                break
            copy(dist + 16384, cnt + 2)
            t = d
        elif state == 0:  # literal run
            lit((t or run(15)) + 3)
            state = 4
            continue
        elif state < 4:  # M1, 2 bytes
            copy((t >> 2) + (byte() << 2) + 1, 2)
        else:  # M1 after a long literal run, 3 bytes
            copy((t >> 2) + (byte() << 2) + 2049, 3)
        state = t & 3
        lit(state)
    if len(out) != out_len:
        raise BlobError(f"decoded {len(out)} bytes, expected {out_len}")
    return bytes(out)


def frame(b: bytes, at: int = 0) -> tuple[dict, bytes]:
    if at + 15 > len(b) or struct.unpack_from("<Q", b, at)[0] != MAGIC:
        raise BlobError("not an ACU compressed frame")
    version, algorithm, flags = struct.unpack_from("<HBI", b, at + 8)
    if version > 3 or algorithm > 1 or flags & 0x80000000:
        raise BlobError(f"unsupported frame version={version} algorithm={algorithm} flags={flags:#x}")
    p, out, blocks = at + 15, bytearray(), 0
    while p < len(b) and b[p] in (0, 1):
        compressed = b[p] == 1
        if p + 5 > len(b):
            raise BlobError("truncated block header")
        stored = struct.unpack_from("<I", b, p + 1)[0]
        p += 5
        decoded, check = stored, None
        if compressed:
            if p + 8 > len(b):
                raise BlobError("truncated block header")
            decoded, check = struct.unpack_from("<II", b, p)
            p += 8
        if p + stored > len(b) or len(out) + decoded > MAX_DECODED:
            raise BlobError("block out of bounds")
        src = b[p:p + stored]
        p += stored
        if check is not None and adler0(src) != check:
            raise BlobError("block checksum mismatch")
        out += lzo1x(src, decoded) if compressed and decoded != stored else src
        blocks += 1
    if not blocks:
        raise BlobError("empty frame")
    return {"version": version, "algorithm": algorithm, "flags": flags, "blocks": blocks, "end": p}, bytes(out)


def decode(blob: bytes) -> bytes:
    """Memento field: u32 decoded size, then one compressed frame."""
    if len(blob) < 4:
        raise BlobError("blob too short")
    size = struct.unpack_from("<I", blob, 0)[0]
    info, out = frame(blob, 4)
    if info["end"] != len(blob) or len(out) != size:
        raise BlobError(f"size mismatch: header {size}, decoded {len(out)}, frame end {info['end']}/{len(blob)}")
    return out


def items(decoded: bytes, db: sqlite3.Connection | None) -> list[dict]:
    found = []
    for off in range(len(decoded) - 7):
        v = struct.unpack_from("<Q", decoded, off)[0]
        if not (1 << 32 <= v < 1 << 40 and v & 0xFFFFFFFF):
            continue
        fid = f"{v:016x}"
        row = db.execute("SELECT name, type_name FROM resource WHERE file_id = ? LIMIT 1", (fid,)).fetchone() if db else None
        if row:
            found.append({"offset": off, "file_id": fid, "name": row[0], "type": row[1]})
    return found


def load(path: Path) -> list[bytes]:
    if path.suffix == ".jsonl":
        blobs = []
        for line in path.read_text(encoding="utf-8").splitlines():
            r = json.loads(line)
            if r.get("blob_hex"):
                blobs.append(bytes.fromhex(r["blob_hex"]))
        return blobs
    return [path.read_bytes()]


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("cmd", choices=("items", "diff"))
    ap.add_argument("paths", nargs="+", type=Path)
    ap.add_argument("--db", type=Path, default=Path(__file__).resolve().parents[2] / "local" / "asset_index.sqlite")
    a = ap.parse_args(argv)
    db = sqlite3.connect(f"file:{a.db.as_posix()}?mode=ro", uri=True) if a.db.exists() else None
    if a.cmd == "items":
        for path in a.paths:
            for blob in load(path):
                print(json.dumps({"stored": len(blob), "items": items(decode(blob), db)}))
        return 0
    old, new = (decode(load(p)[0]) for p in a.paths[:2])
    changed = [i for i in range(min(len(old), len(new))) if old[i] != new[i]]
    near = lambda d: [x for x in items(d, db) if any(x["offset"] <= i < x["offset"] + 8 for i in changed)]
    print(json.dumps({"changed_offsets": changed, "lengths": [len(old), len(new)], "before": near(old), "after": near(new)}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
