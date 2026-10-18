"""Bounded ACU reference/handle BuildTable diagnostic (ACU-RE-0016).

Consumes acu-mat --hex JSON; no extraction or game install mutation.
Unsupported schemas stop at the cursor, never scan/resynchronize.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct


class Reader:
    def __init__(self, data):
        if len(data) > 16 * 1024 * 1024:
            raise ValueError('payload exceeds diagnostic limit')
        self.data, self.pos = data, 0

    def read(self, fmt):
        size = struct.calcsize('<' + fmt)
        if self.pos + size > len(self.data):
            self.fail('truncated field')
        values = struct.unpack_from('<' + fmt, self.data, self.pos)
        self.pos += size
        if 'f' in fmt and any(not math.isfinite(v) for v in values):
            self.fail('nonfinite scalar')
        return values[0] if len(values) == 1 else list(values)

    def fail(self, message):
        raise ValueError(f'{message} at payload offset {self.pos}')

    def count(self, minimum=1):
        n = self.read('i')
        if n < 0 or n > 10000 or n > (len(self.data) - self.pos) // minimum:
            self.fail('invalid count')
        return n

    def boolean(self):
        value = self.read('B')
        if value > 1:
            self.fail('invalid boolean')
        return bool(value)

    def fid(self):
        return f'{self.read("Q"):016x}'

    def header(self, expected):
        fid, cls = self.fid(), self.read('I')
        if cls != expected:
            self.fail(f'unexpected class {cls:08x}, expected {expected:08x}')
        return fid

    def handle(self):
        return dict(tag=self.read('B'), file_id=self.fid())

    def pointer(self):
        offset, tag = self.pos, self.read('B')
        if tag == 3:
            return dict(offset=offset, tag=tag)
        if tag != 1:
            self.fail('unsupported object pointer')
        return dict(offset=offset, tag=tag, file_id=self.fid())

    def reference(self):
        offset = self.pos
        tag, scope, fid = self.read('B'), self.read('B'), self.fid()
        if tag not in (1, 3) or (tag == 3 and fid != '0000000000000000'):
            self.fail('unsupported file reference')
        return dict(offset=offset, tag=tag, scope=scope, file_id=fid)

    def property(self):
        offset = self.pos
        dtype, kind, extra = self.read('III')
        if kind not in (0x1c0000, 0x120000):
            self.fail(f'unsupported dynamic property kind {kind:08x}')
        return dict(offset=offset, data_type=f'{dtype:08x}', kind=f'{kind:08x}',
                    extra=extra, reference=self.reference() if kind == 0x1c0000 else self.handle())

    def tag(self):
        return dict(file_id=self.header(0xb332698e), value=f'{self.read("I"):08x}')

    def tags(self):
        fid = self.header(0x11bd5345)
        return dict(file_id=fid, tags=[self.tag() for _ in range(self.count(16))])

    def column(self):
        result = dict(file_id=self.header(0x36839608), pass_id=self.read('I'))
        result['path_id'] = self.header(0x3c00b2e0)
        if self.count() != 0:
            self.fail('unsupported nonempty property path')
        result['target_unique'] = self.boolean()
        result['set_whole_array'] = self.boolean()
        result['index'] = self.read('i')
        if self.count() != 0:
            self.fail('unsupported entity position selections')
        result['table'] = self.pointer()
        props = []
        for _ in range(self.count(26)):
            key = self.read('I')
            props.append(dict(key=key, property=self.property()))
        result['properties'] = props
        return result

    def row(self):
        result = dict(file_id=self.header(0x348b28d6), table=self.pointer(),
                      weight=self.read('f'), tag=self.tag(), possible_tags=self.tags())
        result['dependencies'] = [self.fid() for _ in range(self.count(8))]
        props = []
        seen = set()
        for _ in range(self.count(26)):
            key = self.read('i')
            if key in seen:
                self.fail('duplicate row component')
            seen.add(key)
            props.append(dict(column=key, property=self.property()))
        result['components'] = props
        return result

    def selector(self):
        result = dict(file_id=self.header(0xf5bd7b8a), selected_tags=self.tags(),
                      column_mask=self.read('I'), random_seed=self.read('I'))
        result['selections'] = [self.pointer() for _ in range(self.count())]
        result['entity_builder'] = self.handle()
        result['additional_tables'] = [self.handle() for _ in range(self.count(9))]
        return result


def parse(data):
    r = Reader(data)
    marker = r.read('B')
    if marker != 1:
        r.fail('unsupported resource marker')
    result = dict(file_id=r.header(0x22ecbe63))
    result['columns'] = [r.column() for _ in range(r.count(12))]
    indices = [c['index'] for c in result['columns']]
    if len(set(indices)) != len(indices):
        r.fail('duplicate column index')
    result['rows'] = [r.row() for _ in range(r.count(12))]
    for row in result['rows']:
        for component in row['components']:
            if component['column'] not in indices:
                r.fail('row component references undefined column')
    result['shuffle'] = r.boolean()
    result['default_selections'] = r.selector()
    for name in ('ignore_defaults_with_tags', 'always_merge_defaults',
                 'dynamic', 'force_not_dynamic'):
        result[name] = r.boolean()
    result['subtables'] = [r.handle() for _ in range(r.count(9))]
    if r.pos != len(data):
        r.fail('trailing bytes')
    result['consumed'] = r.pos
    return result


def probe(source):
    if source.stat().st_size > 40 * 1024 * 1024:
        raise ValueError('input exceeds diagnostic limit')
    raw = source.read_bytes()
    report = json.loads(raw)
    if len(report['payload_hex']) > 32 * 1024 * 1024:
        raise ValueError('payload exceeds diagnostic limit')
    data = bytes.fromhex(report['payload_hex'])
    if (len(data) != report['payload_length'] or
            hashlib.sha256(data).hexdigest() != report['payload_sha256']):
        raise ValueError('payload identity mismatch')
    if report['resource']['type_id'] != '22ecbe63':
        raise ValueError('input resource is not BuildTable')
    table = parse(data)
    if table['file_id'] != report['resource']['file_id']:
        raise ValueError('resource/header ID mismatch')
    return dict(finding='ACU-RE-0016', input_sha256=hashlib.sha256(raw).hexdigest(),
                source={k: report[k] for k in ('entry', 'stored_sha256', 'resource',
                        'payload_length', 'payload_sha256')}, table=table,
                limits='Reference/handle diagnostic; serialized rows do not prove runtime selection')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('input', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = probe(args.input)
    args.output.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    t = result['table']
    print(f"{result['source']['resource']['name']}: {len(t['columns'])} columns, "
          f"{len(t['rows'])} rows; exact end {t['consumed']}")


if __name__ == '__main__':
    main()
