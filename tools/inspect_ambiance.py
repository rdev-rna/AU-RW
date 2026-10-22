"""Inspect a WorldAmbiance/Climate/LayeredSky payload: strings, floats, structure.

Read-only. Usage: python tools/inspect_ambiance.py <file> [--hex N]
"""
import struct
import sys
from pathlib import Path


def printable_runs(data, minimum=3):
    runs, start = [], None
    for index, byte in enumerate(data):
        if 32 <= byte < 127:
            if start is None:
                start = index
        else:
            if start is not None and index - start >= minimum:
                runs.append((start, data[start:index].decode("ascii", "replace")))
            start = None
    if start is not None and len(data) - start >= minimum:
        runs.append((start, data[start:].decode("ascii", "replace")))
    return runs


def floats(data, minimum=-1e6, maximum=1e6):
    out = []
    for offset in range(0, len(data) - 3, 4):
        value = struct.unpack_from("<f", data, offset)[0]
        if value != value or value in (float("inf"), float("-inf")):
            continue
        if minimum <= value <= maximum and (abs(value) > 1e-6 or value == 0.0):
            out.append((offset, value))
    return out


def main(path, hex_count):
    data = Path(path).read_bytes()
    print(f"{path}: {len(data)} bytes")
    print("\nstrings:")
    for offset, text in printable_runs(data)[:60]:
        print(f"  +{offset:5d} {text!r}")
    print("\nplausible floats (first 80):")
    count = 0
    for offset, value in floats(data):
        print(f"  +{offset:5d} {value:g}")
        count += 1
        if count >= 80:
            break
    if hex_count:
        print("\nhex:")
        for base in range(0, min(len(data), hex_count), 16):
            chunk = data[base:base + 16]
            text = "".join(chr(c) if 32 <= c < 127 else "." for c in chunk)
            print(f"  {base:5d}  {chunk.hex(' '):<47}  {text}")


if __name__ == "__main__":
    main(sys.argv[1], int(sys.argv[3]) if len(sys.argv) > 3 and sys.argv[2] == "--hex" else 0)
