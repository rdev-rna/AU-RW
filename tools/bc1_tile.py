"""Decode a BC1 (DXT1) block-compressed tile to PNG, with no third-party modules.

Written because the archive stores the Paris map tiles as 128x128 BC1 textures and the
only thing needed to look at them is this decode plus a PNG writer.
"""
import struct
import sys
import zlib
from pathlib import Path


def rgb565(value):
    r = (value >> 11) & 0x1F
    g = (value >> 5) & 0x3F
    b = value & 0x1F
    return ((r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2))


def decode_bc1(data, width, height):
    pixels = bytearray(width * height * 3)
    blocks_x = (width + 3) // 4
    blocks_y = (height + 3) // 4
    offset = 0
    for by in range(blocks_y):
        for bx in range(blocks_x):
            if offset + 8 > len(data):
                return bytes(pixels)
            c0, c1, bits = struct.unpack_from("<HHI", data, offset)
            offset += 8
            palette = [rgb565(c0), rgb565(c1)]
            if c0 > c1:
                palette.append(tuple((2 * a + b) // 3 for a, b in zip(palette[0], palette[1])))
                palette.append(tuple((a + 2 * b) // 3 for a, b in zip(palette[0], palette[1])))
            else:
                palette.append(tuple((a + b) // 2 for a, b in zip(palette[0], palette[1])))
                palette.append((0, 0, 0))
            for row in range(4):
                for col in range(4):
                    index = (bits >> (2 * (row * 4 + col))) & 3
                    x, y = bx * 4 + col, by * 4 + row
                    if x < width and y < height:
                        r, g, b = palette[index]
                        at = (y * width + x) * 3
                        pixels[at:at + 3] = bytes((r, g, b))
    return bytes(pixels)


def write_png(path, width, height, rgb):
    raw = bytearray()
    stride = width * 3
    for y in range(height):
        raw.append(0)
        raw += rgb[y * stride:(y + 1) * stride]
    def chunk(tag, payload):
        return (struct.pack(">I", len(payload)) + tag + payload
                + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF))
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 6))
    png += chunk(b"IEND", b"")
    Path(path).write_bytes(png)


def main():
    source = Path(sys.argv[1])
    width = int(sys.argv[2]) if len(sys.argv) > 2 else 128
    height = int(sys.argv[3]) if len(sys.argv) > 3 else 128
    target = Path(sys.argv[4]) if len(sys.argv) > 4 else source.with_suffix(".png")
    data = source.read_bytes()
    # The texture payload starts with a 32-byte envelope; find the BC1 block run by size.
    expected = ((width + 3) // 4) * ((height + 3) // 4) * 8
    start = max(0, len(data) - expected)
    pixels = decode_bc1(data[start:start + expected], width, height)
    write_png(target, width, height, pixels)
    print(f"{source.name}: {len(data)} bytes, decoded {expected} BC1 bytes from offset {start} -> {target}")


if __name__ == "__main__":
    main()
