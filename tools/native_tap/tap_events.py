"""Parse acu_tap event dumps. Event: EvHdr(<IIQQIHBB4QQQQ) + nsnap * (SnapHdr <HHI + padded bytes)."""
import struct
HDR = struct.Struct("<IIQQIHBB4QQQQ")
def parse(path):
    data = open(path, "rb").read(); off = 0; out = []
    while off + HDR.size <= len(data):
        magic, size, seq, qpc, tid, hook, phase, nsnap, a0, a1, a2, a3, rsp, ret, rax = HDR.unpack_from(data, off)
        if magic != 0x45544341 or size < HDR.size or off + size > len(data): break
        p = off + HDR.size; snaps = {}
        for _ in range(nsnap):
            sid, ok, ln = struct.unpack_from("<HHI", data, p); p += 8
            snaps[sid] = data[p:p + ln] if ok else None; p += (ln + 7) & ~7
        out.append(dict(seq=seq, qpc=qpc, tid=tid, hook=hook, phase=phase, a=(a0, a1, a2, a3), rsp=rsp, ret=ret, rax=rax, snaps=snaps))
        off += size
    return out

GRAPH_LEAF = 1152
def parse_graph(blob):
    """Decode a SNAPGRAPH blob -> dict(node_count, leaves[...]). playback_raw is 1024 bytes."""
    import struct as s
    magic, nodes, leaves, _ = s.unpack_from("<IIII", blob, 0)
    out = dict(node_count=nodes, leaves=[])
    for i in range(leaves):
        o = 16 + i * GRAPH_LEAF
        idx, ok, w, slot = s.unpack_from("<IIfI", blob, o)
        state, node, playback, asset, vtable, method = s.unpack_from("<6Q", blob, o + 16)
        handle = blob[o + 80:o + 104]; raw = blob[o + 128:o + 128 + 1024]
        fid = s.unpack_from("<Q", handle, 16)[0] if ok else 0
        t900 = s.unpack_from("<f", raw, 900)[0] if ok else None; off876 = s.unpack_from("<f", raw, 876)[0] if ok else None
        out["leaves"].append(dict(index=idx, ok=bool(ok), weight=w, slot=slot, state=state, node=node, playback=playback, asset=asset, file_id=f"{fid:016x}" if ok else None, time900=t900, offset876=off876, binding_ptr=s.unpack_from("<Q", raw, 56)[0] if ok else None, playback_raw=raw))
    return out
