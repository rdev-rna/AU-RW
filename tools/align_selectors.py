"""Align several LODSelector payloads to locate the LOD reference record.

Dumps each selector, prints the header as u8/u64/u32, then every 8-byte window that
equals the selector id plus a small delta (the traced sample has _LOD1 at +8). Fields that
are constant across selectors are header; fields that track the id are the reference.
"""
import struct
import subprocess
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
GAME = Path(r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity")
PARIS = GAME / "DataPC_ACU_Paris.forge"
EXE = ROOT / "acu-rs/target/debug/acu-resource.exe"
OUT = ROOT / "local/sel-align"
OUT.mkdir(parents=True, exist_ok=True)

IDS = [0x00000015470AA6D7, 0x00000010422CC115, 0x0000001086CA995D, 0x0000002F4E5CCE91]


def selector_index():
    data = (ROOT / "local/paris-selectors.bin").read_bytes()
    count = struct.unpack_from("<I", data, 8)[0]
    out = {}
    for i in range(count):
        at = 12 + i * 12
        out.setdefault(struct.unpack_from("<Q", data, at)[0], []).append(
            struct.unpack_from("<I", data, at + 8)[0])
    return out


def main():
    index = selector_index()
    payloads = {}
    for selector in IDS:
        entry = (index.get(selector) or [None])[0]
        target = OUT / f"{selector:016x}.bin"
        result = subprocess.run([str(EXE), str(PARIS), "--entry", str(entry), "--fid",
                                 f"{selector:016x}", "--len", "0", "--out", str(target)],
                                capture_output=True, text=True)
        if result.returncode != 0 or not target.exists():
            print(f"{selector:016x}: dump failed ({result.stderr.strip()[:80]})")
            continue
        payloads[selector] = target.read_bytes()

    for selector, data in payloads.items():
        print(f"\n{selector:016x}  {len(data)} bytes")
        print("  head:", data[:96].hex())
        print("  version", data[0], " self %016x" % struct.unpack_from("<Q", data, 1)[0],
              " type %08x" % struct.unpack_from("<I", data, 9)[0])
        # windows equal to the selector id plus a small delta
        hits = []
        for offset in range(0, len(data) - 7):
            value = struct.unpack_from("<Q", data, offset)[0]
            delta = value - selector
            if -64 <= delta <= 64 and delta != 0:
                hits.append((offset, delta))
        print("  id+delta windows:", hits[:8])

    # Which byte offsets are constant across the payloads?
    if len(payloads) >= 2:
        common = min(len(v) for v in payloads.values())
        constant = [i for i in range(0, 64) if len({v[i] for v in payloads.values()}) == 1]
        print("\nconstant bytes in the first 64:", constant)


if __name__ == "__main__":
    main()
