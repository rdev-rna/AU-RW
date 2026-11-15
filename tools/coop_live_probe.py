"""Read-only live probe of ACU's co-op layer in a running game (ACU-RE-0085, #85).

    python tools/coop_live_probe.py <pid> <output.json> [--seconds 20]

Needs the native probe already injected (tools/native_probe, e.g. through
`capture_run.py --until world`). The probe:
1. reads the heap singletons named in ACU-RE-0072 to 0084 and the vtables they point at, and records
   each vtable slot's target as an image RVA, so heap virtuals can be resolved statically;
2. reads the intention instance counter twice to get the rate at which intentions start;
3. arms hardware execute hooks in two rounds of four (`HOOK slot rva expected-bytes ...`), counts hits
   per slot over a fixed time, and keeps a few sampled events. A rate guard removes a hook that fires
   more than --max-rate times per second, so hot routines cannot stall the game.

Every address is an RVA in the installed ACU.exe (SHA-256 748abdc6...27e40, preferred base
0x140000000). Expected bytes come from the hash-bound mapped image. Nothing is written to the game.
The output stays in ignored local/ data. Close the game only with capture_run.py --close.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import struct
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent / "native_probe"))
from client import checked  # noqa: E402

PREFERRED_BASE = 0x140000000

# name -> (rva, sub-object offsets whose first qword is a vtable)
GLOBALS = {
    "session_owner_145212F48": (0x5212F48, [0x0, 0x20]),
    "dirty_lists_145213880": (0x5213880, [0x0]),
    "net_manager_1451B1E90": (0x51B1E90, [0x0]),
    "storm_registry_145212EB8": (0x5212EB8, [0x0]),
    "peer_rfc_sender_1452136C0": (0x52136C0, [0x0]),
    "peer_list_1452217B8": (0x52217B8, [0x0]),
    "global_sender_145139D00": (0x5139D00, [0x0, 0x110]),
    "cookie_ctx_1451C5F78": (0x51C5F78, [0x0]),
    "allocator_145221CF8": (0x5221CF8, [0x0]),
    "size_lookup_145221E08": (0x5221E08, [0x0]),
    "travel_manager_145219860": (0x5219860, [0x0]),
}
COUNTERS = {
    "intention_instance_counter_145212F3C": (0x5212F3C, 4),
    "clock_offset_14521E148": (0x521E148, 8),
}
VTABLE_SLOTS = 0x48  # slots read per vtable

# (label, rva, expected prologue bytes from the mapped image, sample (register_index, offset, bytes))
ROUNDS = [
    [
        ("start_intention", 0x5D17F0, "4c894c24204489442418555341544155", (0, 0x60, 16)),
        ("memento_push_entry", 0x1C55BB0, "40574883ec20488db9a001000048895c", (0, 0, 0)),
        ("memento_push_gated", 0x1C55BEC, "488b0d55d35b034881c360010000488b", (0, 0, 0)),
        ("replicate_object_path", 0x1C66360, "405356415441574883ec28488b19458b", (0, 0, 0)),
    ],
    [
        ("batch_close", 0x1C46880, "48896c2418574883ec2080796c00488b", (0, 0x60, 32)),
        ("record_flush", 0x1C4CB50, "48895c241048896c2418565741564881", (0, 0, 0)),
        ("memento_write_vf170", 0x1C77FE0, "405356574883ec40488bda488bf9e85d", (0, 0, 0)),
        ("intention_builder", 0x71F090, "40555356574154488dac24e0efffffb8", (0, 0, 0)),
    ],
]


def read(pid: int, va: int, n: int) -> bytes | None:
    try:
        r = checked(pid, f"READ {va:x} {n}")
    except Exception:
        return None
    data = r.get("hex") or r.get("bytes") or r.get("data")
    return bytes.fromhex(data) if isinstance(data, str) else None


def describe_vtable(pid: int, vt: int, base: int, size: int) -> dict:
    raw = read(pid, vt, VTABLE_SLOTS * 8)
    out = {"vtable_rva": f"{vt - base:x}" if base <= vt < base + size else None, "slots": {}}
    if raw is None:
        out["error"] = "unreadable"
        return out
    for i in range(VTABLE_SLOTS):
        t = struct.unpack_from("<Q", raw, i * 8)[0]
        if base <= t < base + size:
            out["slots"][f"{i * 8:#x}"] = f"{t - base:x}"
        else:
            break  # the vtable ends at the first non-image pointer
    return out


def singletons(pid: int, base: int, size: int) -> dict:
    result = {}
    for name, (rva, subs) in GLOBALS.items():
        raw = read(pid, base + rva, 8)
        entry = {"rva": f"{rva:x}"}
        if raw is None:
            entry["error"] = "unreadable"
        else:
            ptr = struct.unpack("<Q", raw)[0]
            entry["value_is_null"] = ptr == 0
            entry["value_in_image"] = base <= ptr < base + size
            for off in subs if ptr and not entry["value_in_image"] else []:
                vraw = read(pid, ptr + off, 8)
                if vraw is None:
                    entry[f"vtable@{off:#x}"] = {"error": "unreadable"}
                    continue
                vt = struct.unpack("<Q", vraw)[0]
                entry[f"vtable@{off:#x}"] = describe_vtable(pid, vt, base, size) if base <= vt < base + size \
                    else {"error": "not an image pointer"}
        result[name] = entry
    return result


def counters(pid: int, base: int) -> dict:
    out = {}
    for name, (rva, n) in COUNTERS.items():
        raw = read(pid, base + rva, n)
        out[name] = int.from_bytes(raw, "little", signed=(n == 8)) if raw else None
    return out


def run_round(pid: int, probes: list, seconds: float, max_rate: float) -> dict:
    armed, result = {}, {}
    for slot, (label, rva, expected, (reg, off, nbytes)) in enumerate(probes):
        cmd = f"HOOK {slot} {rva:x} {expected} {label} 1 {reg} {off} {nbytes} coop"
        try:
            checked(pid, cmd)
            armed[slot] = label
            result[label] = {"rva": f"{rva:x}", "armed": True}
        except Exception as exc:  # byte mismatch or no slot: record and go on
            result[label] = {"rva": f"{rva:x}", "armed": False, "error": str(exc)[:300]}
    start = time.monotonic()
    removed = set()
    samples: dict[str, list] = {label: [] for label in armed.values()}
    while time.monotonic() - start < seconds:
        time.sleep(0.5)
        st = checked(pid, "STATUS")
        elapsed = time.monotonic() - start
        for h in st["hooks"]:
            label = armed.get(h["slot"])
            if label and h["slot"] not in removed and h["hits"] / elapsed > max_rate and elapsed > 1:
                checked(pid, f"UNHOOK {h['slot']}")
                removed.add(h["slot"])
                result[label]["removed_by_rate_guard_after_s"] = round(elapsed, 2)
        ev = checked(pid, "EVENTS 256")
        for e in ev.get("events", []):
            label = e.get("label")
            if label in samples and len(samples[label]) < 8:
                keep = {k: e.get(k) for k in ("thread", "rip", "return_address", "registers", "snapshot_address",
                                              "snapshot_hex") if k in e}
                if "stack_hex" in e:
                    keep["stack_hex_first32"] = str(e["stack_hex"])[:64]
                samples[label].append(keep)
    st = checked(pid, "STATUS")
    elapsed = time.monotonic() - start
    for h in st["hooks"]:
        label = armed.get(h["slot"])
        if label:
            result[label]["hits"] = h["hits"]
            result[label]["seconds"] = round(elapsed, 2)
            result[label]["per_second"] = round(h["hits"] / elapsed, 2)
            result[label]["samples"] = samples[label]
    for slot in armed:
        if slot not in removed:
            checked(pid, f"UNHOOK {slot}")
    result["_status_after"] = {k: st.get(k) for k in ("dropped_or_overwritten", "thread_slot_conflicts",
                                                      "stray_traps")}
    return result


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pid", type=int)
    ap.add_argument("output", type=Path)
    ap.add_argument("--seconds", type=float, default=20)
    ap.add_argument("--max-rate", type=float, default=2000, help="hits per second above which a hook is removed")
    a = ap.parse_args()
    st = checked(a.pid, "STATUS")
    base = int(st["image_base"], 16)
    mods = checked(a.pid, "MODULES")
    size = next((int(str(m.get("size")), 0) for m in mods.get("modules", [])
                 if str(m.get("name", "")).lower() == "acu.exe"), 0x6000000)
    report = {"pid": a.pid, "image_base": f"{base:x}", "image_size": f"{size:x}",
              "preferred_base_match": base == PREFERRED_BASE, "time": time.strftime("%Y-%m-%dT%H:%M:%S")}
    report["counters_before"] = counters(a.pid, base)
    t0 = time.monotonic()
    report["singletons"] = singletons(a.pid, base, size)
    report["rounds"] = [run_round(a.pid, probes, a.seconds, a.max_rate) for probes in ROUNDS]
    report["counters_after"] = counters(a.pid, base)
    report["counter_interval_s"] = round(time.monotonic() - t0, 2)
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(report, indent=1), encoding="utf-8")
    print(json.dumps({r: {k: (v.get("hits"), v.get("armed")) for k, v in rr.items() if not k.startswith("_")}
                      for r, rr in enumerate(report["rounds"])}))


if __name__ == "__main__":
    main()
