"""Count hits on Storm's LAN event code and the LAN menu page in a running ACU (ACU-RE-0087, #85).

    python tools/coop_lan_hooks.py <pid> <output.jsonl> --set getters|checks --seconds 300

Needs the native probe injected (`capture_run.py ... --until title|world`). Arms four read-only
hardware execute hooks (`HOOK slot rva expected-bytes ...`), appends one JSON line per second with
the cumulative hit count of each hook, and a few sampled events (thread, return address) when a
hook fires. Drive the game meanwhile; the timeline shows which phase produced hits. A rate guard
removes a hook above --max-rate hits/s. All hooks are removed at the end (or on Ctrl+C).

Sets (RVAs in ACU.exe SHA-256 748abdc6...27e40; expected bytes from the hash-bound mapped image
9a1124f1...):
- getters: the four lazily-registering type getters of storm::echo::lan::inEvent::* .
- checks:  the three "is this event a <LAN type>" checks and the LanSessionsPage constructor.
Nothing is written to the game.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent / "native_probe"))
from client import checked  # noqa: E402

SETS = {
    "getters": [
        ("lan_get_DiscoverSession", 0x1B752A0, "40534883ec2048833d82316903007527"),
        ("lan_get_SetDiscoveryHandler", 0x1B7A500, "40534883ec2048833de2de6803007527"),
        ("lan_get_GenerateSessionId", 0x1B76450, "40534883ec2048833d72206903007527"),
        ("lan_get_BroadcastLanDiscovery2", 0x1B748A0, "40534883ec2048833d72506903007527"),
    ],
    "checks": [
        ("lan_is_DiscoverSession", 0x1B3BF10, "40534883ec2048833d12c56c0300488b"),
        ("lan_is_SetDiscoveryHandler", 0x1B3D4C0, "40534883ec2048833d22af6c0300488b"),
        ("lan_is_BroadcastLanDiscovery2", 0x1B3BB60, "40534883ec2048833db2dd6c0300488b"),
        ("lansessionspage_ctor", 0x8B0E50, "40534883ec20488bd9e8e290faff488d"),
    ],
}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pid", type=int)
    ap.add_argument("output", type=Path)
    ap.add_argument("--set", choices=sorted(SETS), default="getters")
    ap.add_argument("--seconds", type=float, default=300)
    ap.add_argument("--max-rate", type=float, default=500)
    ap.add_argument("--stop-file", type=Path, help="stop early when this file appears")
    a = ap.parse_args()
    a.output.parent.mkdir(parents=True, exist_ok=True)
    out = a.output.open("a", encoding="utf-8")

    def emit(record):
        out.write(json.dumps(record) + "\n")
        out.flush()

    armed = {}
    for slot, (label, rva, expected) in enumerate(SETS[a.set]):
        try:
            checked(a.pid, f"HOOK {slot} {rva:x} {expected} {label} 1 0 0 0 coop")
            armed[slot] = label
            emit({"event": "armed", "slot": slot, "label": label, "rva": f"{rva:x}"})
        except Exception as exc:  # byte mismatch, no slot: record and go on
            emit({"event": "arm_failed", "slot": slot, "label": label, "rva": f"{rva:x}", "error": str(exc)[:300]})
    start, removed, samples = time.monotonic(), set(), {label: 0 for label in armed.values()}
    try:
        while time.monotonic() - start < a.seconds and armed:
            if a.stop_file and a.stop_file.exists():
                break
            time.sleep(1)
            elapsed = time.monotonic() - start
            st = checked(a.pid, "STATUS")
            hits = {armed[h["slot"]]: h["hits"] for h in st.get("hooks", []) if h["slot"] in armed}
            for h in st.get("hooks", []):
                if h["slot"] in armed and h["slot"] not in removed and elapsed > 2 and h["hits"] / elapsed > a.max_rate:
                    checked(a.pid, f"UNHOOK {h['slot']}")
                    removed.add(h["slot"])
                    emit({"event": "rate_guard_removed", "label": armed[h["slot"]], "t_s": round(elapsed, 1)})
            for e in checked(a.pid, "EVENTS 64").get("events", []):
                label = e.get("label")
                if label in samples and samples[label] < 6:
                    samples[label] += 1
                    emit({"event": "sample", "label": label, "t_s": round(elapsed, 1),
                          **{k: e.get(k) for k in ("thread", "rip", "return_address") if k in e}})
            emit({"event": "tick", "t_s": round(elapsed, 1), "unix": round(time.time(), 1), "hits": hits})
    finally:
        for slot in armed:
            if slot not in removed:
                try:
                    checked(a.pid, f"UNHOOK {slot}")
                except Exception:
                    pass
        emit({"event": "disarmed", "t_s": round(time.monotonic() - start, 1)})
        out.close()


if __name__ == "__main__":
    main()
