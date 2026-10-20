"""Export the inputs of the modifier evaluation-order check (ACU-RE-0028 "Evaluation order").
Slots come from the per-class object dumps (objs-<class>-<pid>.json, 128 bytes each) and the
MaxLookAt entry snapshots (snap 1 = 128 object bytes); the hierarchy from a gate capture's meta;
the observed order from a WWATCH write trace: model-record stores (0x1400994f2) inside the pose
array, labelled by the modifier apply return site on the stack, first frame only.
Object byte layout: bone ref +16 (cached index u16 +28), weight f32 +32, key u16 +36, flags +38.
Usage: modifier_order_export.py <capture_dir> <pid> <wtrace.bin> <array_base_hex> <out.json>"""
import json, os, struct, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tap_events as E

cap, pid, trace, base, out = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4], 16), sys.argv[5]
REC = 376
SITES = {0x1400494fd: 'mla', 0x14004b61a: 'poscon', 0x140049ef6: 'oricon', 0x140049438: 'hinge', 0x14004bd36: 'reflex'}

def fields(cls, b):
    return dict(cls=cls, bone=struct.unpack_from('<H', b, 28)[0], weight=struct.unpack_from('<f', b, 32)[0],
                key=struct.unpack_from('<H', b, 36)[0], enabled=bool(b[38] & 1))

slots = []
for cls in ('hinge', 'oricon', 'poscon', 'reflex'):
    for r in json.load(open(f'{cap}/objs-{cls}-{pid}.json')):
        slots.append(fields(cls, bytes.fromhex(r['hex'])))
seen = set()
for e in E.parse(f'{cap}/ev-mla-idle1-{pid}.bin'):
    if e['phase'] == 0 and e['a'][0] not in seen and e['snaps'].get(1):
        seen.add(e['a'][0]); slots.append(fields('mla', e['snaps'][1]))
meta = json.load(open(f'{cap}/ev-oc-idle1-{pid}.json'))
h = bytes.fromhex(meta['hierarchy_hex'])
hierarchy = [dict(parent=struct.unpack_from('<H', h, 12 * i)[0], lod=(h[12 * i + 10] >> 4) & 7) for i in range(len(h) // 12)]
end = base + 32 * len(hierarchy)
rets = {s + 5: n for s, n in SITES.items()}
data = open(trace, 'rb').read()
hits = []
for k in range(len(data) // REC):
    qpc, rip, addr = struct.unpack_from('<QQQ', data, k * REC)
    if rip != 0x1400994f2 or not base <= addr < end: continue
    stack = struct.unpack_from('<40Q', data, k * REC + 40)
    cls = next((rets[v] for v in stack if v in rets), None)
    if cls: hits.append((qpc, cls, (addr - base) // 32))
hits.sort()
observed = []
for _, cls, bone in hits:
    if dict(cls=cls, bone=bone) in observed: break  # the next frame repeats the sequence
    observed.append(dict(cls=cls, bone=bone))
json.dump(dict(source=dict(capture_dir=cap, pid=pid, trace=trace, array_base=hex(base)),
               hierarchy=hierarchy, slots=slots, observed=observed), open(out, 'w'), indent=1)
print(f'{len(slots)} slots, {len(hierarchy)} joints, {len(observed)} observed applies -> {out}')
