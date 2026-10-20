"""Static data of live Reflex objects (ACU-RE-0028), read once next to a capture: connector entries
(24 bytes; kind byte +2, record sub-index +3), their 96-byte records, joint-ref tables, plane trees
(walked over every mask to bound the bytes) and int16 sample tables. Only kind 0 (three samples) and an
empty second list (+0x32) are supported; anything else raises so the gate never runs on unread data."""
import struct

MAX_TREE = 1 << 16


def tree_extent(rd, base):
    """Walk every branch of the plane tree at `base` (layout of 0x14006cf00); return the byte length."""
    nodes, ends, stack = set(), [], [0]
    while stack:
        o = stack.pop()
        if o in nodes: continue
        if len(nodes) > 4096 or not 0 <= o < MAX_TREE: raise ValueError(f"tree walk out of bounds at {o}")
        nodes.add(o); b = rd(base + o, 0x50); ends.append(o + 0x50)
        for mask in range(16):
            code = struct.unpack_from("<h", b, 0x30 + 2 * mask)[0]; nxt = o + 4 * code
            if code & 3 == 0: stack.append(nxt); continue
            if code & 1:
                p = nxt + 4 * code - 8; ends.append(p + 16); pb = rd(base + p, 16)
                leaves = [struct.unpack_from("<H", pb, 0xc + 2 * k)[0] * 64 for k in (0, 1)]
            else:
                leaves = [((code & ~3) << 4)]
            for leaf in leaves:
                if not 0 <= leaf < MAX_TREE: raise ValueError(f"leaf out of bounds {leaf}")
                ends.append(leaf + 64)
    return max(ends)


def read_object(rd, obj):
    q = lambda a: struct.unpack("<Q", rd(a, 8))[0]
    b = rd(obj, 0x40)
    if struct.unpack_from("<H", b, 0x32)[0]: raise ValueError(f"Reflex {obj:#x}: second list (+0x28) not supported")
    cp, count = struct.unpack_from("<Q", b, 0x34)[0], struct.unpack_from("<H", b, 0x3e)[0]
    conns = []
    for k in range(count):
        e = rd(cp + 24 * k, 24); mul, kind, sub = struct.unpack_from("<HBB", e, 0); v18, v17 = struct.unpack_from("<QQ", e, 8)
        if kind != 0: raise ValueError(f"Reflex {obj:#x}: connector kind {kind} not supported")
        rec = rd(q(v18 + 60) + 96 * sub, 96)
        tree = struct.unpack_from("<Q", rec, 0x1e)[0]; table = struct.unpack_from("<Q", rec, 0x50)[0]
        rows = struct.unpack_from("<H", rec, 0x5a)[0]
        refs_ptr, refs_n = q(v17 + 16), struct.unpack("<H", rd(v17 + 26, 2))[0]
        conns.append(dict(entry_hex=e.hex(), record_hex=rec.hex(), cache_table=hex(q(v17 + 40)),
                          refs_hex=rd(refs_ptr, 16 * refs_n).hex() if refs_n else "",
                          tree_hex=rd(tree, tree_extent(rd, tree)).hex(), samples_hex=rd(table, 16 * rows).hex() if rows else ""))
    return dict(object_hex=b.hex(), connectors=conns)
