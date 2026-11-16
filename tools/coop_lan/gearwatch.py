"""gearwatch.py <out.jsonl> <pid> [pid ...] [--seconds 120] [--every 1] [--slots 0,1,2,3] [--extra pid:slot:label:rva:reg:off:bytes]
#85 cosmetic-sync probe (ACU-RE-0105). On each game it hooks four memento codec functions (capture image SHA 9a1124f1...):
  0 HumanAvatar read   0x140172F20 (rcx = replica net data, rdx = incoming memento; snapshot rdx+0x50, 256 B)
  1 HumanAvatar write  0x140173180 (rcx = master net data;               snapshot rcx+0x00, 128 B)
  2 PlayerInventory read  0x140505FF0 (rcx = replica object, rdx = memento; snapshot rdx+0x50, 128 B)
  3 PlayerInventory write 0x140506020 (rcx = master object;               snapshot rcx+0x00, 128 B)
For the inventory hits it then READs the byte blob at object+0x28 (count u32 at +0x30, at most 4096 B).
Decodes the HumanAvatar fields as the codec copies them (ACU-RE memento_codecs: +0x10 u8, +0x18 u64, +0x20 u64,
+0x28 u32, +0x2C u8, +0x30..+0x4C eight floats). Read-only apart from the hooks; always unhooks.
--slots picks which of the four built-in hooks to arm ('' for none). --extra arms any other site on one game and logs
its raw snapshot (reg index: 0 rcx, 1 rdx, 2 r8, 3 r9, as in the probe). Other scripts that HOOK the same slot (privhost,
lancall) replace these hooks; re-arm afterwards. Decode blobs with inventory_blob.py."""
import argparse, json, os, struct, sys, time, hashlib
sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parents[1] / "native_probe"))
from client import checked, command  # noqa

ap = argparse.ArgumentParser(); ap.add_argument("out"); ap.add_argument("pids", type=int, nargs="+")
ap.add_argument("--seconds", type=float, default=120); ap.add_argument("--every", type=int, default=1)
ap.add_argument("--slots", default="0,1,2,3")
ap.add_argument("--extra", action="append", default=[], help="pid:slot:label:hexrva:reg:off:bytes")
a = ap.parse_args()
IMG = os.environ.get("ACU_MAPPED_IMAGE", r"E:/ReverseEngineering/Projects/Unity/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin")
img = open(IMG, "rb").read()
# slot: (label, rva, reg index (0 rcx, 1 rdx), offset, bytes)
H = {0: ("ha_read", 0x172F20, 1, 0x50, 256), 1: ("ha_write", 0x173180, 0, 0, 128),
     2: ("inv_read", 0x505FF0, 1, 0x50, 128), 3: ("inv_write", 0x506020, 0, 0, 128)}
use = [int(s) for s in a.slots.split(",") if s]
out = open(a.out, "a", encoding="utf-8"); t0 = time.monotonic()
def emit(**r):
    r.setdefault("t", round(time.monotonic() - t0, 3)); out.write(json.dumps(r) + "\n"); out.flush()
    print(json.dumps(r)[:600], flush=True)
def rd(pid, va, n):
    try: return bytes.fromhex(checked(pid, f"READ {va:x} {n}")["hex"])
    except Exception as ex: return None
def ha_net(b):  # net data struct from offset 0
    return {"stamp": struct.unpack_from("<I", b, 0x50)[0] if len(b) > 0x54 else None, "u8_10": b[0x10],
            "u64_18": "%016x" % struct.unpack_from("<Q", b, 0x18)[0], "u64_20": "%016x" % struct.unpack_from("<Q", b, 0x20)[0],
            "u32_28": "%08x" % struct.unpack_from("<I", b, 0x28)[0], "u8_2c": b[0x2C],
            "f": [round(x, 4) for x in struct.unpack_from("<8f", b, 0x30)]}
def ha_memento(b):  # snapshot starts at memento+0x50
    o = lambda x: x - 0x50
    r = {"stamp": struct.unpack_from("<I", b, o(0x50))[0], "u8_70": b[o(0x70)],
         "u64_90": "%016x" % struct.unpack_from("<Q", b, o(0x90))[0], "u64_b0": "%016x" % struct.unpack_from("<Q", b, o(0xB0))[0],
         "u32_d0": "%08x" % struct.unpack_from("<I", b, o(0xD0))[0], "u8_f0": b[o(0xF0)]}
    r["q"] = [(struct.unpack_from("<I", b, o(v))[0], struct.unpack_from("<I", b, o(v) + 8)[0]) for v in (0x110, 0x138) if o(v) + 12 <= len(b)]
    return r
armed = {p: [] for p in a.pids}
EXTRA = [x.split(":") for x in a.extra]
seen = {}
try:
    for p in a.pids:
        for s in use:
            lab, rva, reg, off, n = H[s]
            checked(p, f"HOOK {s} {rva:x} {img[rva:rva + 16].hex()} {lab} {a.every} {reg} {off} {n} gearwatch"); armed[p].append(s)
        for e in EXTRA:
            if int(e[0]) == p:
                s, lab, rva = int(e[1]), e[2], int(e[3], 16)
                checked(p, f"HOOK {s} {rva:x} {img[rva:rva + 16].hex()} {lab} 1 {e[4]} {e[5]} {e[6]} gearwatch"); armed[p].append(s)
        checked(p, "EVENTS 256")
    emit(event="armed", extra=a.extra, pids=a.pids, hooks={s: (H[s][0], hex(0x140000000 + H[s][1])) for s in use})
    end = time.monotonic() + a.seconds
    while time.monotonic() < end:
        for p in a.pids:
            for e in checked(p, "EVENTS 256").get("events", []):
                b = bytes.fromhex(e.get("snapshot_hex", "") or ""); regs = e.get("registers") or []
                rcx = int(regs[0], 16) if regs and isinstance(regs[0], str) else (regs[0] if regs else 0)
                rdx = int(regs[1], 16) if regs and isinstance(regs[1], str) else (regs[1] if len(regs) > 1 else 0)
                lab = e["label"]; rec = {"event": "hit", "pid": p, "label": lab, "tid": e["tid"], "rcx": hex(rcx), "rdx": hex(rdx),
                                         "ret": e.get("return_address")}
                try:
                    if lab == "ha_read" and len(b) >= 0xF0: rec["memento"] = ha_memento(b)
                    elif lab == "ha_write" and len(b) >= 0x54: rec["net"] = ha_net(b)
                    elif lab in ("inv_read", "inv_write"):
                        hdr = rd(p, rcx, 0x78) or b''
                        if len(hdr) >= 0x34:
                            ptr, cnt = struct.unpack_from("<Q", hdr, 0x28)[0], struct.unpack_from("<I", hdr, 0x30)[0]
                            rec["obj_stamp"] = struct.unpack_from("<I", hdr, 0x70)[0] if len(hdr) >= 0x74 else None
                            rec["blob_ptr"], rec["blob_len"] = hex(ptr), cnt
                            if lab == "inv_read" and len(b) >= 0x30:
                                rec["msg_stamp"] = struct.unpack_from("<I", b, 0)[0]
                                rec["msg_len"] = struct.unpack_from("<I", b, 0x7C - 0x50)[0]
                            key = (p, lab, rcx)
                            if ptr and 0 < cnt <= 4096:
                                blob = rd(p, ptr, cnt)
                                if blob:
                                    h = hashlib.sha256(blob).hexdigest()
                                    rec["blob_sha"] = h[:16]
                                    if seen.get(key) != h: rec["blob_hex"] = blob.hex(); seen[key] = h
                    else: rec["snap"] = b.hex()
                except Exception as ex:
                    rec["decode_error"] = str(ex)[:120]
                    rec["raw"] = b.hex()
                emit(**rec)
        time.sleep(0.2)
    emit(event="end")
finally:
    for p, slots in armed.items():
        for s in slots:
            try: checked(p, f"UNHOOK {s}")
            except Exception: pass
    emit(event="disarmed"); out.close()
