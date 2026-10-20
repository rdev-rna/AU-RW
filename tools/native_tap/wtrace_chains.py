"""Split WWATCH write-trace hits (WHit records, 376 bytes) into writes inside / outside a
watched array and print, per writer RIP, the hit count, offsets relative to the array base
and the call chain recovered from each hit's 40-qword stack copy (qwords preceded by an
E8 rel32 call, mapped to their .pdata function). The page-protection tracer reports every
write to the guarded pages, so hits outside [base, base + count*stride) are page neighbours.
Usage: wtrace_chains.py <wtrace.bin> <mapped.bin> <array_base_hex> <count> [stride=32]"""
import bisect, collections, struct, sys

REC, BASE = 376, 0x140000000
PD_RVA, PD_SIZE = 86503424, 3163956  # .pdata of the preserved mapped image (see callgraph.py)
trace, img, base, count = sys.argv[1], sys.argv[2], int(sys.argv[3], 16), int(sys.argv[4])
stride = int(sys.argv[5]) if len(sys.argv) > 5 else 32
end = base + count * stride
image = open(img, 'rb').read()
funcs = sorted({(b, e) for b, e, _ in struct.iter_unpack('<III', image[PD_RVA:PD_RVA + PD_SIZE - PD_SIZE % 12]) if b and e > b})
starts = [b for b, _ in funcs]

def func_of(va):
    r = va - BASE
    i = bisect.bisect_right(starts, r) - 1
    return BASE + funcs[i][0] if i >= 0 and funcs[i][0] <= r < funcs[i][1] else None

def chain(stack):
    out = []
    for v in stack:
        r = v - BASE
        if 5 <= r < len(image) and image[r - 5] == 0xE8:
            f = func_of(v)
            out.append(f'{v - 5:x}@{f:x}' if f else f'{v - 5:x}')
    return ' > '.join(out[:6]) or '(none)'

data = open(trace, 'rb').read()
groups = {True: collections.Counter(), False: collections.Counter()}
spans, chains = {}, collections.defaultdict(collections.Counter)
lo = hi = None
for k in range(len(data) // REC):
    _qpc, rip, addr, _rsp, _tid, _len = struct.unpack_from('<QQQQII', data, k * REC)
    inside = base <= addr < end
    groups[inside][rip] += 1
    rel = addr - base
    a, b = spans.get((inside, rip), (rel, rel))
    spans[(inside, rip)] = (min(a, rel), max(b, rel))
    chains[(inside, rip)][chain(struct.unpack_from('<40Q', data, k * REC + 40))] += 1
    lo = addr if lo is None or addr < lo else lo
    hi = addr if hi is None or addr > hi else hi
print(f'records {len(data) // REC}; hit addresses {lo:x}..{hi:x}; array {base:x}..{end:x}')
for inside in (True, False):
    print(f"== {'inside' if inside else 'outside'} array: {sum(groups[inside].values())} hits")
    for rip, n in groups[inside].most_common(12):
        a, b = spans[(inside, rip)]
        print(f'  rip {rip:x} func {func_of(rip) or 0:x} hits {n} offsets {a:+#x}..{b:+#x}')
        for c, m in chains[(inside, rip)].most_common(2):
            print(f'      {m:6d}  {c}')
