"""Independent LZO oracle for explicitly selected package metadata variants.

Reads original archive ranges in memory and writes identities, never asset copies.
The first frame marker is recorded without assuming it must be canonical; the
payload frame must have canonical magic. This is an investigation, not runtime.
"""
import argparse
import ctypes
import hashlib
import json
from pathlib import Path
import struct
import zlib

from forge_probe import FRAME_MAGIC, read_range, tables

MAX_DECODED = 512 * 1024 * 1024


def digest(data):
    return hashlib.sha256(data).hexdigest()


def inspect(raw, fn, allow_checksum_mismatch=False, allow_payload_marker=False):
    cursor = 0
    frames = []
    streams = []
    for metadata in (True, False):
        start = cursor
        marker = raw[cursor:cursor+8]
        if len(marker) != 8 or (not metadata and marker != FRAME_MAGIC and not allow_payload_marker):
            raise ValueError('missing canonical payload frame magic')
        version, algorithm, flags, count = struct.unpack_from('<HBII', raw, cursor+8)
        if version not in (0, 1, 2, 3) or algorithm not in (0, 1) or not flags & 0x80000000:
            raise ValueError('oracle supports only observed TOC frames')
        cursor += 19
        width = 4 if version == 1 else 8
        if count > 1_000_000 or count * width > len(raw)-cursor:
            raise ValueError('block table exceeds bounds')
        lengths = [struct.unpack_from('<HH' if width == 4 else '<II', raw, cursor+i*width) for i in range(count)]
        cursor += count * width
        if sum(a for a, b in lengths) > MAX_DECODED-sum(len(s) for s in streams):
            raise ValueError('decoded budget exceeded')
        blocks = []
        mismatches = []
        for decoded, stored in lengths:
            checksum = struct.unpack_from('<I', raw, cursor)[0]
            cursor += 4
            src = raw[cursor:cursor+stored]
            cursor += stored
            computed = zlib.adler32(src, 0)
            if len(src) != stored:
                raise ValueError('truncated block')
            if computed != checksum:
                mismatches.append(dict(block=len(blocks), expected=f'{checksum:08x}', computed=f'{computed:08x}'))
                if not allow_checksum_mismatch:
                    raise ValueError('compressed checksum mismatch')
            if decoded == stored:
                blocks.append(src)
            else:
                dst = ctypes.create_string_buffer(decoded)
                size = ctypes.c_size_t(decoded)
                result = fn(src, stored, dst, ctypes.byref(size), None)
                if result != 0 or size.value != decoded:
                    raise ValueError('native LZO output mismatch')
                blocks.append(dst.raw)
        stream = b''.join(blocks)
        streams.append(stream)
        frames.append(dict(offset=start, marker=marker.hex(), canonical_marker=marker == FRAME_MAGIC,
                           decoded_length=len(stream), decoded_sha256=digest(stream), stored_length=cursor-start,
                           checksum_mismatches=mismatches))
    trailer = raw[cursor:]
    metadata, payload = streams
    count = struct.unpack_from('<H', metadata)[0]
    pos = 2
    end = 0
    resources = []
    for _ in range(count):
        fid, size, tags = struct.unpack_from('<QIH', metadata, pos)
        pos += 14+tags*2
        record = payload[end:end+size]
        if len(record) != size:
            raise ValueError('record outside payload')
        cls, length, namesize = struct.unpack_from('<III', record)
        name = record[12:12+namesize].decode('utf-8')
        at = 12+namesize
        flag = record[at]
        at += 1
        if flag == 1:
            refs = struct.unpack_from('<I', record, at+3)[0]
            at += 7+12*refs
        elif flag != 0:
            raise ValueError('unknown resource header flag')
        if at+length != len(record):
            raise ValueError('resource payload length mismatch')
        end += size
        resources.append(dict(file_id=f'{fid:016x}', type_id=f'{cls:08x}', name=name,
                              payload_length=length, payload_sha256=digest(record[at:])))
    if end != len(payload) or pos != len(metadata):
        raise ValueError('resource table does not consume streams')
    return dict(frames=frames, resource_count=count, resources=resources[:16],
                trailing_length=len(trailer), trailing_sha256=digest(trailer),
                trailing_prefix=trailer[:96].hex(),
                conformant=not trailer and all(f['canonical_marker'] and not f['checksum_mismatches'] for f in frames))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('lzo_dll', type=Path)
    p.add_argument('archive', type=Path)
    p.add_argument('indices', nargs='+', type=int)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--allow-checksum-mismatch', action='store_true',
                   help='diagnostic only: preserve mismatches while trying native decompression')
    p.add_argument('--allow-payload-marker', action='store_true',
                   help='diagnostic only: record noncanonical payload marker')
    args = p.parse_args()
    root = Path(__file__).resolve().parents[1]
    if not any(args.output.resolve().is_relative_to(root/folder) for folder in ('local', 'outputs')):
        p.error('output must be in local/ or outputs/')
    if args.output.exists():
        p.error('choose a fresh report path')
    dll = ctypes.CDLL(str(args.lzo_dll.resolve()))
    fn = dll.lzo1x_decompress_safe
    fn.restype = ctypes.c_int
    fn.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p,
                   ctypes.POINTER(ctypes.c_size_t), ctypes.c_void_p]
    rows = []
    with args.archive.open('rb') as f:
        length = args.archive.stat().st_size
        archive_report, entries = tables(f, length)
        for index in args.indices:
            if index < 0 or index >= len(entries):
                raise ValueError('entry index outside archive')
            entry = entries[index]
            raw = read_range(f, length, entry['offset'], entry['stored_length'])
            row = dict(entry=entry, stored_sha256=digest(raw))
            try:
                row.update(inspect(raw, fn, args.allow_checksum_mismatch, args.allow_payload_marker))
            except (ValueError, struct.error, UnicodeError) as error:
                row['error'] = str(error)
            rows.append(row)
    report = dict(archive=str(args.archive.resolve()), archive_length=length,
                  archive_table=archive_report, provider=str(args.lzo_dll.resolve()),
                  provider_sha256=digest(args.lzo_dll.read_bytes()),
                  diagnostic_checksum_tolerance=args.allow_checksum_mismatch, samples=rows)
    report['diagnostic_payload_marker_tolerance'] = args.allow_payload_marker
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
    for row in rows:
        print(row['entry']['file_id'], row.get('error'), row.get('resource_count'),
              [(f['decoded_length'], len(f['checksum_mismatches'])) for f in row.get('frames', [])])


if __name__ == '__main__':
    main()
