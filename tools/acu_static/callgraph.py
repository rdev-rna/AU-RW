"""Static x64 call graph over the preserved mapped ACU runtime image (capstone).
Usage: callgraph.py <mapped.bin> callees <VA> [depth] | callers <VA> | disasm <VA> [n]
Image base 0x140000000; mapped file offset == RVA. Direct calls/jmps only; indirect
(vtable) calls are listed as 'indirect' and are NOT followed (observation limit)."""
import struct, sys, functools
import capstone
BASE = 0x140000000
IMG = sys.argv[1]
data = open(IMG, 'rb').read()
md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64); md.detail = True
# .pdata RUNTIME_FUNCTION table: rva 86503424 size 3163956
pd_rva, pd_size = 86503424, 3163956
funcs = sorted({(b, e) for b, e, _ in struct.iter_unpack('<III', data[pd_rva:pd_rva + pd_size - pd_size % 12]) if b and e > b})
starts = [b for b, e in funcs]
import bisect
def func_of(rva):
    i = bisect.bisect_right(starts, rva) - 1
    if i >= 0 and funcs[i][0] <= rva < funcs[i][1]: return funcs[i]
    return None
@functools.lru_cache(None)
def scan(rva):
    f = func_of(rva); b, e = (f if f else (rva, rva + 0x400))
    calls, ind = [], 0
    for ins in md.disasm(data[b:e], BASE + b):
        if ins.mnemonic in ('call', 'jmp') and ins.operands:
            op = ins.operands[0]
            if op.type == capstone.x86.X86_OP_IMM: calls.append((ins.mnemonic, op.imm))
            elif ins.mnemonic == 'call': ind += 1
    return b, e, calls, ind
if __name__ == '__main__':
    cmd, va = sys.argv[2], int(sys.argv[3], 16)
    if cmd == 'disasm':
        n = int(sys.argv[4]) if len(sys.argv) > 4 else 200
        f = func_of(va - BASE); b, e = f if f else (va - BASE, va - BASE + 0x400)
        for k, ins in enumerate(md.disasm(data[b:e], BASE + b)):
            if k >= n: break
            print(f"{ins.address:x}  {ins.mnemonic} {ins.op_str}")
    elif cmd == 'callees':
        depth = int(sys.argv[4]) if len(sys.argv) > 4 else 1
        seen = {}
        def go(v, d):
            if v in seen or d > depth: return
            b, e, calls, ind = scan(v - BASE); seen[v] = (e - b, ind, d)
            for m, t in calls:
                if m == 'call' or t < BASE + b or t >= BASE + e: go(t, d + 1)
        go(va, 0)
        for v, (sz, ind, d) in sorted(seen.items(), key=lambda x: (x[1][2], x[0])):
            print(f"{'  ' * d}{v:x} size={sz} indirect_calls={ind}")
    elif cmd == 'callers':   # callers <VA> [VA...]: every E8/E9 rel32 whose target is one of the VAs (vectorized byte scan)
        import numpy as np
        targets = [int(a, 16) for a in sys.argv[3:]]
        text = np.frombuffer(data, dtype=np.uint8, count=48658351, offset=4096)
        pos = np.flatnonzero((text[:-5] == 0xE8) | (text[:-5] == 0xE9))
        rel = (text[pos + 1].astype(np.int64) | text[pos + 2].astype(np.int64) << 8 | text[pos + 3].astype(np.int64) << 16 | text[pos + 4].astype(np.int64) << 24)
        rel = np.where(rel >= 1 << 31, rel - (1 << 32), rel)
        dest = BASE + 4096 + pos + 5 + rel
        for t in targets:
            for i in pos[dest == t]:
                f = func_of(4096 + int(i))
                print(f"{t:x} site {BASE + 4096 + int(i):x} {'call' if text[i] == 0xE8 else 'jmp'} in func {BASE + f[0] if f else 0:x}")
