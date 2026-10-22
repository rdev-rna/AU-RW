"""Compose the Paris capture sheet from the PNGs written by the viewer.

The viewer writes RGBA8 PNGs; this reads them with the standard library only, halves them
with a nearest-neighbour step so four fit on one page, and writes a contact sheet. It is a
presentation helper for evidence, not part of the asset pipeline.
"""
import struct
import sys
import zlib
from pathlib import Path

OUT_HERE = Path(r"C:\Users\Administrator\AU-RW\local")


def read_png(path):
    data = Path(path).read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path}: not a PNG")
    offset = 8
    width = height = depth = colour = None
    idat = bytearray()
    while offset < len(data):
        length = struct.unpack_from(">I", data, offset)[0]
        tag = data[offset + 4:offset + 8]
        payload = data[offset + 8:offset + 8 + length]
        offset += 12 + length
        if tag == b"IHDR":
            width, height, depth, colour = struct.unpack_from(">IIBB", payload, 0)
        elif tag == b"IDAT":
            idat += payload
        elif tag == b"IEND":
            break
    if depth != 8 or colour not in (2, 6):
        raise ValueError(f"{path}: only 8-bit RGB/RGBA supported, got depth={depth} colour={colour}")
    channels = 3 if colour == 2 else 4
    raw = zlib.decompress(bytes(idat))
    stride = width * channels
    pixels = bytearray(width * height * 3)
    previous = bytearray(stride)
    position = 0
    for y in range(height):
        filter_type = raw[position]
        position += 1
        line = bytearray(raw[position:position + stride])
        position += stride
        if filter_type == 1:
            for i in range(channels, stride):
                line[i] = (line[i] + line[i - channels]) & 0xFF
        elif filter_type == 2:
            for i in range(stride):
                line[i] = (line[i] + previous[i]) & 0xFF
        elif filter_type == 3:
            for i in range(stride):
                left = line[i - channels] if i >= channels else 0
                line[i] = (line[i] + ((left + previous[i]) >> 1)) & 0xFF
        elif filter_type == 4:
            for i in range(stride):
                left = line[i - channels] if i >= channels else 0
                up = previous[i]
                up_left = previous[i - channels] if i >= channels else 0
                p = left + up - up_left
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - up_left)
                best = left if (pa <= pb and pa <= pc) else (up if pb <= pc else up_left)
                line[i] = (line[i] + best) & 0xFF
        elif filter_type != 0:
            raise ValueError(f"{path}: unknown filter {filter_type}")
        for x in range(width):
            at = (y * width + x) * 3
            src = x * channels
            pixels[at:at + 3] = line[src:src + 3]
        previous = line
    return width, height, pixels


def halve(width, height, pixels):
    w, h = width // 2, height // 2
    out = bytearray(w * h * 3)
    for y in range(h):
        for x in range(w):
            src = ((y * 2) * width + x * 2) * 3
            dst = (y * w + x) * 3
            out[dst:dst + 3] = pixels[src:src + 3]
    return w, h, out


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
    tiles = [
        ("streetrow-lod0", OUT_HERE / "paris-streetrow-lod0.png"),
        ("wall-lod0", OUT_HERE / "paris-wall-lod0.png"),
        ("gate-lod0", OUT_HERE / "paris-gate-lod0.png"),
        ("streetrow-lod1", OUT_HERE / "paris-hd.png"),
    ]
    margin = 12
    cells = []
    for name, path in tiles:
        if not path.exists():
            print(f"missing {path}")
            continue
        width, height, pixels = read_png(path)
        w, h, half = halve(width, height, pixels)
        cells.append((name, w, h, half))
        print(f"{name}: {width}x{height} -> {w}x{h}")
    if not cells:
        raise SystemExit("nothing to compose")
    cell_w = max(c[1] for c in cells)
    cell_h = max(c[2] for c in cells)
    cols = 2
    rows = (len(cells) + cols - 1) // cols
    sheet_w = cols * cell_w + (cols + 1) * margin
    sheet_h = rows * cell_h + (rows + 1) * margin
    sheet = bytearray(b"\x20" * (sheet_w * sheet_h * 3))
    for index, (name, w, h, half) in enumerate(cells):
        col, row = index % cols, index // cols
        x0 = margin + col * (cell_w + margin)
        y0 = margin + row * (cell_h + margin)
        for y in range(h):
            dst = ((y0 + y) * sheet_w + x0) * 3
            sheet[dst:dst + w * 3] = half[y * w * 3:(y + 1) * w * 3]
    target = OUT_HERE / "paris-sheet.png"
    write_png(target, sheet_w, sheet_h, sheet)
    print(f"wrote {target} ({sheet_w}x{sheet_h})")


if __name__ == "__main__":
    main()
