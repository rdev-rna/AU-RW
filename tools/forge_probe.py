#!/usr/bin/env python3
"""Bounded plaintext Forge v27 compatibility probe (ACU-RE-0001).

Table offsets reviewed against FH forge.rs, with XXTEA removed. This is an
investigation tool, not a resource decoder. No last-byte flag is stripped.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import struct

from acu import settings, timestamp

MAX_ENTRIES = 1_000_000
MAX_TABLES = 4096
MAX_SAMPLE = 64 * 1024 * 1024
FRAME_MAGIC = bytes.fromhex('33aafb5799fa0410')


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_range(stream, length, offset, size):
    if offset < 0 or size < 0 or offset > length or size > length - offset:
        raise ValueError(f'range outside archive: offset={offset}, size={size}')
    if size > MAX_SAMPLE:
        raise ValueError(f'range exceeds probe allocation limit: {size}')
    stream.seek(offset)
    data = stream.read(size)
    if len(data) != size:
        raise ValueError('short read (archive changed?)')
    return data


def tables(stream, length):
    ranges = []

    def record(kind, offset, size):
        data = read_range(stream, length, offset, size)
        ranges.append(dict(kind=kind, offset=offset, length=size, sha256=digest(data)))
        return data

    header = record('header', 0, 0x41a)
    if header[:9] != b'scimitar\0':
        raise ValueError('unsupported encrypted/unknown magic')
    version, dho = struct.unpack_from('<IQ', header, 9)
    if version != 27:
        raise ValueError(f'unsupported version {version}')
    dh = record('data_header', dho, 44)
    declared = struct.unpack_from('<I', dh)[0]
    if declared > MAX_ENTRIES:
        raise ValueError('declared count exceeds limit')
    table = struct.unpack_from('<Q', dh, 36)[0]
    entries, seen = [], set()
    while table not in (0, 0xffffffffffffffff):
        if table in seen:
            raise ValueError('table cycle')
        if len(seen) >= MAX_TABLES:
            raise ValueError('table limit exceeded')
        seen.add(table)
        raw = record('table', table, 48)
        count = struct.unpack_from('<I', raw)[0]
        array, nxt = struct.unpack_from('<QQ', raw, 8)
        if count > MAX_ENTRIES - len(entries):
            raise ValueError('entry limit exceeded')
        records = record('entry_array', array, count * 20)
        for i in range(count):
            offset, fid, size = struct.unpack_from('<QQI', records, i * 20)
            if offset > length or size > length - offset:
                raise ValueError(f'entry {len(entries)} outside archive')
            entries.append(dict(index=len(entries), record_offset=array+i*20,
                                offset=offset, file_id=f'{fid:016x}', stored_length=size))
        table = nxt
    return dict(version=version, declared_count=declared, parsed_count=len(entries),
                count_matches=declared == len(entries), ranges=ranges), entries


def probe(path, samples):
    length = path.stat().st_size
    with path.open('rb') as stream:
        report, entries = tables(stream, length)
        report.update(path=str(path.resolve()), archive_length=length,
                      identity_scope='hashed structural ranges and sampled payloads, not whole archive')
        counts = Counter(e['file_id'] for e in entries)
        report['duplicate_ids'] = {k: v for k, v in counts.items() if v > 1}
        report['samples'] = []
        for entry in entries[:samples]:
            entry = entry.copy()
            if entry['stored_length'] > MAX_SAMPLE:
                entry['sample_status'] = 'unavailable: exceeds sample limit'
                report['samples'].append(entry)
                continue
            raw = read_range(stream, length, entry['offset'], entry['stored_length'])
            entry.update(stored_sha256=digest(raw), prefix_hex=raw[:64].hex(),
                         suffix_hex=raw[-24:].hex(), decode_status='not attempted')
            if raw.startswith(FRAME_MAGIC) and len(raw) >= 19:
                ver, algorithm, flags, blocks = struct.unpack_from('<HBII', raw, 8)
                entry['frame_candidate'] = dict(version=ver, algorithm=algorithm,
                                                flags=f'{flags:08x}', blocks=blocks,
                                                codec='unverified; not FH Oodle framing')
            extracted = path.parent / 'Extracted' / path.name
            if extracted.is_dir():
                matches = list(extracted.glob(f"{entry['index']}_-_*"))
                if len(matches) == 1 and matches[0].is_file():
                    other = matches[0]
                    match = other.stat().st_size == len(raw)
                    other_hash = digest(other.read_bytes()) if match else None
                    entry['extracted_comparison'] = dict(path=str(other), length=other.stat().st_size,
                        sha256=other_hash, equals_stored=other_hash == entry['stored_sha256'])
            report['samples'].append(entry)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archives', nargs='+', help='Archive basename within configured game directory')
    parser.add_argument('--samples', type=int, default=3)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if not 0 <= args.samples <= 32:
        parser.error('--samples must be 0..32')
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    if not any(output.is_relative_to(root / folder) for folder in ('local', 'outputs')):
        parser.error('output must be under local/ or outputs/')
    if any(Path(name).name != name or not name.endswith('.forge') for name in args.archives):
        parser.error('archives must be .forge basenames')
    game = settings()[1]['game_dir']
    report = dict(schema_version=1, timestamp=timestamp(),
                  archives=[probe(game / name, args.samples) for name in args.archives])
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    for archive in report['archives']:
        print(f"{Path(archive['path']).name}: {archive['parsed_count']} entries; "
              f"declared={archive['declared_count']}; samples={len(archive['samples'])}")


if __name__ == '__main__':
    main()
