"""Attribute WWATCH write-trace hits (WHit records, 376 bytes) to call sites by scanning each
hit's 40-qword stack copy for return addresses (site + 5 for E8 rel32 calls).
Usage: wtrace_sites.py <wtrace.bin> <array_base_hex> <site_hex> [site_hex...]
Prints per-joint (32-byte record index from array_base) counts of the first matching site,
then the most common writer RIPs whose stacks contain none of the sites."""
import collections, struct, sys

REC = 376
path, base = sys.argv[1], int(sys.argv[2], 16)
rets = {int(s, 16) + 5: int(s, 16) for s in sys.argv[3:]}
data = open(path, 'rb').read()
by_joint = collections.defaultdict(collections.Counter); other = collections.Counter()
for k in range(len(data) // REC):
    _qpc, rip, addr, _rsp, _tid, _len = struct.unpack_from('<QQQQII', data, k * REC)
    stack = struct.unpack_from('<40Q', data, k * REC + 40)
    hit = [rets[v] for v in stack if v in rets]
    if hit: by_joint[(addr - base) // 32][f'{hit[0]:x}'] += 1
    else: other[f'{rip:x}'] += 1
for j in sorted(by_joint): print('joint', j, dict(by_joint[j]))
print('records', len(data) // REC, 'no-site writers by rip:', other.most_common(8))
