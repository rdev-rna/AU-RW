"""Reproduce the ACU-RE-0041 spot checks against a hash-bound ACU mapped image.

Read-only. Standard library only (no disassembler): instruction checks compare exact bytes.

    python tools/acufixes_spot_check.py <main-image.mapped.bin>

The image must be a runtime capture of the installed build (base 0x140000000,
file offset == RVA). The SHA-256 is printed so the result can be tied to a capture.
Exit status 0 only if every check passes.
"""
from __future__ import annotations

import hashlib
import struct
import sys
import zlib

BASE = 0x140000000
ENTRY = 0x20  # fancy-vtable entry: u32 nameHash, u32 staticIdx, u32 nestedness, pad, u64 fn @+0x10


def main(path: str) -> int:
    img = open(path, "rb").read()
    print("image", path)
    print("sha256", hashlib.sha256(img).hexdigest(), "bytes", len(img))

    def u32(va: int) -> int:
        return struct.unpack_from("<I", img, va - BASE)[0]

    def u64(va: int) -> int:
        return struct.unpack_from("<Q", img, va - BASE)[0]

    def raw(va: int, n: int) -> bytes:
        return img[va - BASE: va - BASE + n]

    failures = 0

    def check(label: str, ok: bool, detail: str) -> None:
        nonlocal failures
        failures += 0 if ok else 1
        print(("PASS " if ok else "FAIL ") + label + ": " + detail)

    # 1. Animation vtable slot 5 is `mov rax, [rip+disp32]; ret` loading TypeInfo 0x1439CD110.
    vt = 0x142E7ACC0
    fn = u64(vt + 5 * 8)
    code = raw(fn, 8)
    ok = code[:3] == b"\x48\x8b\x05" and code[7] == 0xC3
    target = fn + 7 + struct.unpack_from("<i", code, 3)[0] if ok else 0
    check("Animation GetTI", ok and u64(target) == 0x1439CD110,
          f"slot5 {fn:#x} loads [{target:#x}] = {u64(target) if ok else 0:#x}")

    # 2. Parkour-action fancy table (stored at +8 by ctor 0x1401BCD30) carries ACUFixes' hashes/indices.
    table = 0x143A6DFB0
    expected = {0x18: 0x8EB3C7D3, 0x23: 0x7A9E7BE6, 0x45: 0x986EB60D, 0x4E: 0x323150BF, 0x76: 0xF0455AE0}
    for idx, h in expected.items():
        e = table + idx * ENTRY
        check(f"fancy[{idx:#x}]", u32(e) == h and u32(e + 4) == idx,
              f"nameHash {u32(e):#010x} staticIdx {u32(e + 4):#x} fn {u64(e + 0x10):#x}")
    n = 0
    while u32(table + n * ENTRY + 4) == n:
        n += 1
    check("fancy table length", n == 0x78, f"{n:#x} sequential entries")

    # 3. Getters: movss xmm0, [rcx+disp32]; ret  (f3 0f 10 81 <disp32> c3)
    for idx, disp, name in [(0x23, 0x204, "fitness"), (0x18, 0xC8, "distance")]:
        g = u64(table + idx * ENTRY + 0x10)
        want = b"\xf3\x0f\x10\x81" + struct.pack("<I", disp) + b"\xc3"
        check(f"{name} getter", raw(g, 9) == want, f"{g:#x}: {raw(g, 9).hex()}")
    # enum getter: mov eax, 51; ret
    g = u64(table + 0x45 * ENTRY + 0x10)
    check("enum getter", raw(g, 6) == b"\xb8\x33\x00\x00\x00\xc3", f"{g:#x}: {raw(g, 6).hex()}")

    # 4. Fancy tables holding the ACU-RE-0030 scanner caller 0x1401A7000.
    needle = struct.pack("<Q", 0x1401A7000)
    hits, pos = [], 0
    while (pos := img.find(needle, pos)) >= 0:
        hits.append(pos + BASE)
        pos += 8
    good = [h for h in hits if u32(h - 0x10) == 0x518DDA9D and u32(h - 0x0C) == 0x7A]
    roots = {u32(h - 0x10 - 0x7A * ENTRY) for h in good}
    check("0x1401A7000 owners", len(hits) == len(good) and roots == {u32(table)},
          f"{len(good)}/{len(hits)} pointers at fancy index 0x7a; slot-0 hashes {sorted(hex(r) for r in roots)}")

    # 5. Codec descriptor table: 72 pointers, descriptor byte +0x21 == slot.
    lut = [u64(0x145137A40 + 8 * i) for i in range(72)]
    selfidx = sum(1 for i, p in enumerate(lut) if BASE <= p < BASE + len(img) and img[p - BASE + 0x21] == i)
    check("codec LUT", selfidx == 72, f"{selfidx}/72 descriptors self-index")

    # Name-hash check: only GetDistance is the CRC-32 of its ACUFixes name.
    print("crc32('GetDistance') =", hex(zlib.crc32(b"GetDistance")))
    return 1 if failures else 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(2)
    sys.exit(main(sys.argv[1]))
