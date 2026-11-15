"""leapwatch.py <pid> <out.jsonl> [--seconds 90] [--headings 0,90,180,270] [--buttons A|B|none] [--rpid P --rent E]
ACU-RE-0094 live test of the NetBehaviorAI leap payload. Hooks the shared `test r14b, r14b` (just before the
unicast/broadcast wire-id choice) of four leap message blocks in the NBAI send builder 0x14069F200:
  0 ParkourLeap 0x1406A625D   1 ParkourClimbLeap 0x1406A9371   2 ParkourHangLeap 0x1406A77B7   3 ParkourVoidLeap 0x1406AB385
At each site RBX is the source message object (the block copies +0x8, +0x14, +0x30..+0x110 from RBX). Snapshot = 256
bytes at RBX+0x28. Meanwhile drives the player with free run (navigate.Navigator) and logs the player position with
time.perf_counter (QPC based on Windows, the same clock as the probe's event qpc). With --rpid/--rent it also reads
the world matrix (+0x20) of entity E in game P each step: the other game's replica of this player. Always unhooks.
Needs tools/native_probe from branch acu-multi-launch-direct; ACU_MAPPED_IMAGE overrides the image path."""
import argparse, json, math, struct, sys, time
import os, pathlib
NP = pathlib.Path(__file__).resolve().parents[1] / "native_probe"
sys.path.insert(0, str(NP))
from client import checked
import focus, navigate

ap = argparse.ArgumentParser(); ap.add_argument("pid", type=int); ap.add_argument("out")
ap.add_argument("--seconds", type=float, default=90); ap.add_argument("--headings", default="0,90,180,270")
ap.add_argument("--leg", type=float, default=3.0); ap.add_argument("--buttons", default="A")
ap.add_argument("--no-drive", action="store_true")
ap.add_argument("--rpid", type=int); ap.add_argument("--rent")
a = ap.parse_args()
img = open(os.environ.get("ACU_MAPPED_IMAGE", r"E:/ReverseEngineering/Projects/Unity/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin"), "rb").read()
H = [("ParkourLeap", 0x6A625D), ("ParkourClimbLeap", 0x6A9371), ("ParkourHangLeap", 0x6A77B7), ("ParkourVoidLeap", 0x6AB385)]
out = open(a.out, "a"); t0 = time.perf_counter()
freq = None


def emit(**r):
    r.setdefault("t", round(time.perf_counter() - t0, 4)); out.write(json.dumps(r) + "\n"); out.flush()
    if r.get("event") not in ("pos",): print(json.dumps(r)[:400], flush=True)


def decode(b):
    # snapshot starts at RBX+0x28
    o = lambda off: off - 0x28
    vec = {hex(off): [round(x, 3) for x in struct.unpack_from("<3f", b, o(off))] for off in range(0x30, 0x120, 0x10) if off != 0x50}
    return {"hdr_u64_28": hex(struct.unpack_from("<Q", b, 0)[0]), "u32_50": struct.unpack_from("<I", b, o(0x50))[0],
            "vec": vec, "w_of_vecs": {hex(off): round(struct.unpack_from("<f", b, o(off) + 12)[0], 3) for off in range(0x30, 0x120, 0x10) if off != 0x50},
            "bools_120": list(b[o(0x120):o(0x123)]), "u32_124": struct.unpack_from("<I", b, o(0x124))[0],
            "raw": b.hex()}


armed = []
def poll_events():
    for e in checked(a.pid, "EVENTS 256").get("events", []):
        b = bytes.fromhex(e.get("snapshot_hex", "") or "")
        regs = e.get("registers")
        emit(event="leap", label=e["label"], qpc_s=round(e["qpc"] / freq - t0, 4), tid=e["tid"],
             rbx=regs[5] if regs else None, r14=regs[14] if regs else None, ret=e.get("return_address"),
             **(decode(b) if len(b) >= 0xF8 else {"short": b.hex()}))


try:
    st = checked(a.pid, "STATUS"); freq = st["qpc_frequency"]
    for i, (lab, r) in enumerate(H):
        checked(a.pid, f"HOOK {i} {r:x} {img[r:r+16].hex()} {lab} 1 5 40 256 leapwatch"); armed.append(i)
    checked(a.pid, "EVENTS 256")
    emit(event="armed", hooks=[(l, hex(0x140000000 + r)) for l, r in H])
    end = time.monotonic() + a.seconds
    if a.no_drive:
        import player_state
        while time.monotonic() < end:
            s = player_state.chain(a.pid)
            if "position" in s: emit(event="pos", p=[round(v, 3) for v in s["position"]])
            poll_events(); time.sleep(0.05)
    else:
        with focus.InProcessPad(a.pid) as pad:
            nav = navigate.Navigator(a.pid, pad)
            orig = nav.state
            def logged():
                s = orig(); emit(event="pos", p=[round(v, 3) for v in s["position"]])
                if a.rpid:
                    try:
                        import live_state
                        m = struct.unpack("<16f", bytes.fromhex(checked(a.rpid, f"READ {int(a.rent, 16) + live_state.ENTITY_MATRIX:x} 64")["hex"]))
                        emit(event="rpos", p=[round(v, 3) for v in m[12:15]])
                    except Exception as ex: emit(event="rpos_err", err=str(ex)[:100])
                return s
            nav.state = logged
            hs = [math.radians(float(h)) for h in a.headings.split(",")]; k = 0
            btn = None if a.buttons == "none" else [a.buttons]
            while time.monotonic() < end:
                nav.run_along(hs[k % len(hs)], a.leg, True, btn); k += 1
                poll_events()
    poll_events()
finally:
    for i in armed:
        try: checked(a.pid, f"UNHOOK {i}")
        except Exception: pass
    emit(event="disarmed"); out.close()
