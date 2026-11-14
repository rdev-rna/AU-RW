"""Export live world-travel captures into a portable oracle fixture for the rewrite (ACU-RE-0049).

    python tools/oracle_export.py world-travel [--runs local/native-probe/runs] [--output data/oracles/world_travel.json]

Input: `world-samples.jsonl` written by the 50 ms world sampler during runs map-travel-2/-3 (one row
per change of holder +0x90/+0x98, World +0xD8, travel manager +592/+600/+644, ...), plus
`guard-test.jsonl` from travel-guard-1. Output: derived numbers only (timings, file IDs, relations,
statuses) with provenance; no game bytes, no addresses (process-specific pointers are reduced to
"same"/"new" relations). File IDs are 16-digit hex strings as AGENTS.md requires.

A trip starts when the travel manager state +644 goes 0 -> 1 and ends when it returns to 0.
Times are seconds from that request, measured by a 50 ms poll (so each is good to about +-0.05 s).
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY_SHA256 = "748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40"
WORLD_NAMES = {0x0000000F18B7E1CF: "ACU_Paris", 0x0000000E84B48064: "ACU_Versailles"}
DESTINATION_NAMES = {0x0000001770D432BE: "PlayerPortal to Paris", 0x00000017717B9BF1: "PlayerPortal to Versailles"}
NULL_HANDLE = 0x14525BB58  # engine null handle (image VA); holder +0x90 while in the transition world
TRAVEL_RUNS = ("map-travel-2", "map-travel-3")
GUARD_RUN = "travel-guard-1"


def hx(v: str | None) -> int | None:
    return int(v, 16) if isinstance(v, str) and v.startswith("0x") else None


def fid(v: int | None) -> str | None:
    return f"{v:016x}" if v else None


def load_samples(path: Path) -> list[dict]:
    rows = []
    for line in path.read_text().splitlines():
        if line.strip():
            d = json.loads(line)
            rows.append({k: (hx(v) if isinstance(v, str) else v) for k, v in d.items()})
    return rows


def segment_trips(rows: list[dict]) -> list[dict]:
    """Split sampler rows into trips (state 0 -> 1 -> 0) and derive their timeline and relations."""
    trips, current, prev_state = [], None, 0
    last_world_obj, last_player = None, None
    for r in rows:
        state = r.get("mgr644") or 0
        if current is None and state == 1 and prev_state == 0:
            current = {"t0": r["t"], "rows": [], "from_world_obj": last_world_obj,
                       "from_player": last_player, "from_world_id": r.get("h90_id")}
        if current is not None:
            current["rows"].append(r)
            if state == 0:
                trips.append(current)
                current = None
        if r.get("world"):
            last_world_obj = r["world"]
        if r.get("player"):
            last_player = r["player"]
        prev_state = state
    out = []
    for trip in trips:
        rows, t0 = trip["rows"], trip["t0"]
        first = rows[0]
        ev, arrival = {}, None
        for r in rows:
            t = round(r["t"] - t0, 3)
            if "queue_commands_pending" not in ev and (r.get("q_count") or 0) > 0:
                ev["queue_commands_pending"] = t
            if "transition_world" not in ev and r.get("h90") == NULL_HANDLE and not r.get("world"):
                ev["transition_world"] = t
            if ("target_world_object" not in ev and r.get("world") and r.get("h90_id") == first.get("mgr592_id")
                    and r.get("world") != trip["from_world_obj"]):
                ev["target_world_object"] = t
            if "player_spawned" not in ev and r.get("player") and ev.get("target_world_object") is not None:
                ev["player_spawned"] = t
                arrival = r
            if (r.get("mgr644") or 0) == 0:
                ev["travel_state_idle"] = t
        target = first.get("mgr592_id")
        out.append({
            "from_world": fid(trip["from_world_id"]), "from_world_name": WORLD_NAMES.get(trip["from_world_id"]),
            "to_world": fid(target), "to_world_name": WORLD_NAMES.get(target),
            "destination": fid(first.get("mgr600_id")), "destination_name": DESTINATION_NAMES.get(first.get("mgr600_id")),
            "events_s": ev,
            "arrival_position": arrival.get("pos") if arrival else None,
            "relations": {
                "new_world_object": bool(arrival) and arrival["world"] != trip["from_world_obj"],
                "new_player_entity": bool(arrival) and arrival["player"] != trip["from_player"],
                "player_world_backpointer_matches": bool(arrival) and arrival.get("player_world") == arrival.get("world"),
                "no_player_while_world_absent": all(not r.get("player") for r in rows if not r.get("world")),
                "travel_state_1_until_player": all((r.get("mgr644") or 0) == 1 for r in rows
                                                   if arrival is None or r["t"] < arrival["t"]),
            },
            "new_runtime_player_id": f"{arrival['player_fid']:08x}" if arrival and arrival.get("player_fid") else None,
        })
    return out


def world_handles_stable(rows: list[dict]) -> dict:
    """Per world file ID: did holder +0x90 always hold the same handle object in this process?"""
    seen: dict[int, set] = {}
    for r in rows:
        if r.get("h90_id") and r.get("h90") and r["h90"] != NULL_HANDLE:
            seen.setdefault(r["h90_id"], set()).add(r["h90"])
    return {WORLD_NAMES.get(k, fid(k)): len(v) == 1 for k, v in seen.items()}


def guard_results(path: Path) -> dict | None:
    if not path.exists():
        return None
    rows = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    pick = lambda stage: next((r for r in rows if r["stage"] == stage), {})
    return {
        "during_travel_raw_request_status": [pick(s).get("raw_status") for s in ("during_1_raw_old_player", "during_2_raw_old_player")],
        "during_travel_python_ready": [pick(s).get("ready") for s in ("during_1_ready", "during_2_ready")],
        "during_travel_set_flag_refused": pick("during_set_flag").get("refused"),
        "after_arrival_old_player_raw_status": pick("after_raw_old_player").get("raw_status"),
        "after_arrival_new_player_differs": pick("after_new_player").get("same_as_old") is False,
        "arrival_wait_s": pick("arrived").get("t"),
        "statuses": {"-5": "travel in progress", "-6": "not the current World's player"},
    }


def world_travel(runs: Path) -> dict:
    trips, handles = [], {}
    for name in TRAVEL_RUNS:
        path = runs / name / "world-samples.jsonl"
        if not path.exists():
            raise SystemExit(f"missing {path} (live capture data is machine-local; see ACU-RE-0049)")
        rows = load_samples(path)
        for trip in segment_trips(rows):
            trips.append({"run": name, **trip})
        handles[name] = world_handles_stable(rows)
    return {
        "schema": "acu-oracle/world-travel/1",
        "finding": "ACU-RE-0049 (observations 14-16)",
        "binary_sha256": BINARY_SHA256,
        "method": "50 ms poll of World holder [0x145217368] (+0x90 handle, +0x98 World), World +0xD8 player, "
                  "travel manager [0x145219860] (+592 world, +600 destination, +644 state); in-game map fast travel",
        "time_resolution_s": 0.05,
        "trips": trips,
        "world_handle_stable_within_process": handles,
        "guard": guard_results(runs / GUARD_RUN / "guard-test.jsonl"),
        "not_established": [
            "what writes travel state +644 on the map path",
            "whether timings depend on hardware/streaming (only one machine measured)",
            "what the player entity is created from and which state carries over between worlds",
        ],
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("kind", choices=["world-travel"])
    ap.add_argument("--runs", type=Path, default=ROOT / "local/native-probe/runs")
    ap.add_argument("--output", type=Path, default=ROOT / "data/oracles/world_travel.json")
    a = ap.parse_args()
    data = world_travel(a.runs)
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(data, indent=1) + "\n")
    print(json.dumps({"output": str(a.output), "trips": len(data["trips"])}))


if __name__ == "__main__":
    main()
