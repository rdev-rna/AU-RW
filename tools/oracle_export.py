"""Export live captures into portable oracle fixtures for the rewrite.

    python tools/oracle_export.py world-travel [--runs local/native-probe/runs] [--output data/oracles/world_travel.json]
    python tools/oracle_export.py locomotion   [--runs ...] [--output data/oracles/locomotion.json]
    python tools/oracle_export.py parkour      [--runs ...] [--output data/oracles/parkour.json]
    python tools/oracle_export.py camera       [--runs ...] [--output data/oracles/camera.json]
    python tools/oracle_export.py tick         [--runs ...] [--output data/oracles/tick.json]

world-travel (ACU-RE-0049): `world-samples.jsonl` written by the 50 ms world sampler during runs
map-travel-2/-3 (one row per change of holder +0x90/+0x98, World +0xD8, travel manager
+592/+600/+644, ...), plus `guard-test.jsonl` from travel-guard-1. A trip starts when the travel
manager state +644 goes 0 -> 1 and ends when it returns to 0. Times are seconds from that request,
measured by a 50 ms poll (so each is good to about +-0.05 s).

locomotion (ACU-RE-0044/0045): open-loop scripted input timelines joined to live_state.py samples
(player position, forward, playing clip leaves) per run. parkour (ACU-RE-0042/0045): every decoded
call of the selection routine with its candidates, ranking, chosen index and action type, plus the
clip starts that followed when a live_state.py stream exists.

Output: derived numbers only (timings, file IDs, decoded floats, relations, statuses) with
provenance; no game bytes, no addresses (process-specific pointers are dropped or reduced to
ordinals/"same"/"new" relations). File IDs are 16-digit hex strings as AGENTS.md requires.
"""
from __future__ import annotations

import argparse
import bisect
from collections import Counter
import hashlib
import json
import math
from pathlib import Path
import struct
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(Path(__file__).resolve().parent))
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


# ---------------------------------------------------------------------------------------------
# Shared helpers for the locomotion and parkour kinds (ACU-RE-0042/0044/0045).
#
# Clocks: live_state.py samples and the native probe's events carry QPC ticks; the scripted
# controller's input timeline carries Unix time. The probe session's `events` batches carry both
# (`time_unix` when the batch was polled, each event's `qpc`). An event happens before its batch is
# polled, so time_unix - qpc/f >= the true offset for every event; the minimum over all events is
# the tightest such bound and is used as the offset. Input-step times can therefore be late
# relative to samples by at most that event's poll latency (polls run about every 0.1 s).
# ---------------------------------------------------------------------------------------------

SAVE_PIN = "capture-start-1"
SAVE_PIN_SHA256 = "c2c07ad5fbbe1073a9e1894e13a050193bdccb8b77e3541c77c34df506284d7e"  # 1.save, ACU-RE-0044
CLIP_WINDOW_S = 1.5  # parkour_clip_pairs.py default (ACU-RE-0045)
NEAREST_SAMPLE_MAX_S = 0.1


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def read_jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def f32(v: float | None) -> float | None:
    """Shortest decimal that converts back to the same 32-bit float (the decoded fields are f32)."""
    if v is None or not math.isfinite(v):
        return None
    target = struct.pack("<f", v)
    for digits in range(6, 10):
        s = float(f"{v:.{digits}g}")
        if struct.pack("<f", s) == target:
            return s
    return v


def heading_deg(forward: list[float] | None) -> float | None:
    """Degrees of the forward row's XY direction, atan2(y, x) (counter-clockwise from +X, Z up)."""
    if not forward or (forward[0] == 0 and forward[1] == 0):
        return None
    return round(math.degrees(math.atan2(forward[1], forward[0])), 2)


def clock_offset(session_rows: list[dict]) -> dict | None:
    """Unix-minus-QPC offset (seconds) from a probe session, as the minimum over its events."""
    freq, best, n, polls = None, None, 0, []
    for r in session_rows:
        if r.get("kind") == "header":
            freq = (r.get("status") or {}).get("qpc_frequency")
        if r.get("kind") != "events" or "time_unix" not in r:
            continue
        polls.append(r["time_unix"])
        for e in r.get("events") or []:
            if isinstance(e, dict) and "qpc" in e and freq:
                n += 1
                off = r["time_unix"] - e["qpc"] / freq
                best = off if best is None or off < best else best
    if best is None:
        return None
    gaps = sorted(b - a for a, b in zip(polls, polls[1:]))
    return {"offset_s": best, "qpc_frequency": freq, "events": n,
            "poll_interval_median_s": round(gaps[len(gaps) // 2], 3) if gaps else None}


def input_steps(timeline: list[dict], t0_unix: float) -> list[dict]:
    """Scripted controller steps (step_start/step_end pairs) with times relative to t0."""
    starts, out = {}, []
    for r in timeline:
        if r.get("kind") == "step_start":
            starts[r["index"]] = r
        elif r.get("kind") == "step_end" and r["index"] in starts:
            s = starts.pop(r["index"])
            st = s.get("state") or {}
            out.append({"index": r["index"], "name": s["name"],
                        "t_start": round(s["unix_time"] - t0_unix, 4), "t_end": round(r["unix_time"] - t0_unix, 4),
                        "planned_ms": s.get("planned_duration_ms"), "actual_ms": r.get("actual_duration_ms"),
                        "left_stick": st.get("left_stick", [0, 0]), "right_stick": st.get("right_stick", [0, 0]),
                        "left_trigger": st.get("left_trigger", 0), "right_trigger": st.get("right_trigger", 0),
                        "buttons": st.get("buttons", [])})
    return out


def step_at(steps: list[dict], t: float) -> int | None:
    for s in steps:
        if s["t_start"] <= t < s["t_end"]:
            return s["index"]
    return None


def state_samples(rows: list[dict]) -> tuple[dict, list[dict]]:
    header = next((r for r in rows if r.get("kind") == "header"), {})
    return header, [r for r in rows if r.get("kind") == "sample"]


def interval_stats(qpcs: list[int], freq: int) -> dict:
    gaps = sorted((b - a) / freq for a, b in zip(qpcs, qpcs[1:]))
    if not gaps:
        return {}
    return {"median": round(gaps[len(gaps) // 2], 4), "p95": round(gaps[int(len(gaps) * 0.95)], 4),
            "max": round(gaps[-1], 4)}


def clip_starts(samples: list[dict], freq: int) -> list[dict]:
    """Clip starts as tools/parkour_clip_pairs.py defines them (new file ID, or playback time going
    backwards); start QPC = sample QPC minus playback time when 0 <= time < 2 s. Clips already
    playing in the first sample are not starts."""
    starts, previous = [], None
    for s in samples:
        if s.get("clips") is None:
            continue
        current = {c["file_id"]: c for c in s["clips"]}
        if previous is not None:
            for fid_, c in current.items():
                before = previous.get(fid_)
                if before is None or c["time"] < before["time"] - 1e-3:
                    lag = c["time"] if 0 <= c["time"] < 2 else 0.0
                    starts.append({"file_id": fid_, "qpc": s["qpc"] - int(lag * freq), "weight": c["weight"],
                                   "position": s.get("position")})
        previous = current
    return starts


def nearest_sample(samples: list[dict], qpcs: list[int], q: int, freq: int) -> tuple[dict, float] | None:
    i = bisect.bisect_left(qpcs, q)
    best = min((j for j in (i - 1, i) if 0 <= j < len(samples)), key=lambda j: abs(qpcs[j] - q), default=None)
    if best is None:
        return None
    dt = (qpcs[best] - q) / freq
    return (samples[best], dt) if abs(dt) <= NEAREST_SAMPLE_MAX_S else None


def clip_names(file_ids: set[str], db_path: Path | None = None) -> dict:
    """Names from the local asset index (tools/asset_query.py); empty when the index is absent."""
    try:
        import asset_query
        db = asset_query.connect(db_path or asset_query.DEFAULT_DB)
    except Exception:  # noqa: BLE001 - the index is optional machine-local data
        return {}
    out = {}
    for f in sorted(file_ids):
        rows = asset_query.where(db, f, limit=5)["as_resource"]
        if rows:
            r = rows[0]
            out[f] = {"name": r["name"], "label": r["label"], "confidence": r["confidence"]}
    db.close()
    return out


def run_context(capture: Path | None) -> dict:
    """Start conditions recorded by capture_run.py (save pin, preset, fps cap, world start pose)."""
    if not capture or not capture.exists():
        return {"capture_record": None}
    stages = {s["stage"]: s for s in json.loads(capture.read_text()).get("stages", [])}
    pre, world, pin = stages.get("preflight", {}), stages.get("world", {}), stages.get("save_pin", {})
    return {"capture_record": capture.name,
            "save_pin": pin.get("pin"), "save_pin_1_save_sha256": (pin.get("pin_sha256") or {}).get("1.save"),
            "capture_preset": pre.get("capture_preset"), "fps_cap": pre.get("fps_cap"),
            "world_start_position": world.get("position"), "world_start_forward": world.get("forward")}


# ---------------------------------------------------------------------------------------------
# Locomotion: scripted input timeline -> player position, heading and playing clips over time.
# ---------------------------------------------------------------------------------------------

# Runs with an open-loop input timeline AND a live_state.py transform/clip stream on the probe clock.
LOCOMOTION_RUNS = (
    {"run": "catalog-1", "state": "catalog-1/live-state.jsonl", "scenario": "catalog-1/scenario",
     "capture": "catalog-1/capture.json"},
    {"run": "nav-1/pair-1", "state": "nav-1/pair-1-state.jsonl", "scenario": "nav-1/pair-1",
     "capture": "nav-1/capture.json",
     "note": "started where an earlier closed-loop navigate.py test in the same session left Arno, not at the pin"},
)
LOCOMOTION_EXCLUDED = {
    "window-sweep-1, window-sweep-2, vault-scan-1..3, prop-probe-1":
        "closed-loop navigate.py control: the stick was re-steered every 0.1 s and not logged per frame",
    "parkour-1, parkour-2-preset, parkour-5-runner, pinned-*":
        "input timeline but no live_state.py transform/clip stream",
    "nav-1/idle.jsonl, nav-1/nav-test.jsonl": "no input timeline recorded with them",
}
CLIP_COLUMNS = ["file_id", "graph_row", "weight", "playback_time_s", "offset"]


def locomotion_run(samples: list[dict], header: dict, timeline: list[dict], offset: dict) -> dict:
    """Pure transform of one run's rows (samples on QPC, timeline on Unix time) into fixture form."""
    freq = header.get("qpc_frequency") or offset["qpc_frequency"]
    t0_unix = next(r["unix_time"] for r in timeline if r.get("kind") == "step_start")
    q0 = (t0_unix - offset["offset_s"]) * freq  # t0 on the QPC clock
    steps = input_steps(timeline, t0_unix)
    present = [s for s in samples if s.get("position")]
    entities = {s.get("entity") for s in present}
    out_samples = []
    for s in present:
        t = round((s["qpc"] - q0) / freq, 4)
        out_samples.append({"t": t, "step": step_at(steps, t), "position": s["position"], "forward": s["forward"],
                            "heading_deg": heading_deg(s["forward"]),
                            "clips": [[c["file_id"], c["row"], c["weight"], c["time"], c["offset"]]
                                      for c in s.get("clips") or []]})
    starts = [{"t": round((c["qpc"] - q0) / freq, 4), "file_id": c["file_id"], "weight": c["weight"],
               "position": c["position"]} for c in clip_starts(present, freq)]
    segments = []
    for st in steps:
        inside = [x for x in out_samples if st["t_start"] <= x["t"] <= st["t_end"]]
        if len(inside) < 2:
            continue
        a, b = inside[0], inside[-1]
        path = sum(math.dist(p["position"][:2], q["position"][:2]) for p, q in zip(inside, inside[1:]))
        dt = b["t"] - a["t"]
        dh = None
        if a["heading_deg"] is not None and b["heading_deg"] is not None:
            dh = round((b["heading_deg"] - a["heading_deg"] + 180) % 360 - 180, 2)
        segments.append({"step": st["index"], "name": st["name"], "samples": len(inside),
                         "start_position": a["position"], "end_position": b["position"],
                         "net_xy_m": round(math.dist(a["position"][:2], b["position"][:2]), 3),
                         "path_xy_m": round(path, 3), "dz": round(b["position"][2] - a["position"][2], 4),
                         "mean_xy_speed": round(path / dt, 3) if dt > 0 else None,
                         "heading_change_deg": dh,
                         "distinct_clips": len({c[0] for x in inside for c in x["clips"]})})
    return {
        "sampler": {"nominal_hz": header.get("hz"), "samples": len(out_samples),
                    "samples_without_player": len(samples) - len(present),
                    "interval_s": interval_stats([s["qpc"] for s in present], freq),
                    "player_entity_constant": len(entities) == 1},
        "clock_alignment": {"probe_events": offset["events"], "poll_interval_median_s": offset["poll_interval_median_s"]},
        "start": {"position": present[0]["position"], "forward": present[0]["forward"],
                  "heading_deg": heading_deg(present[0]["forward"])} if present else None,
        "inputs": steps,
        "step_summaries": segments,
        "clip_starts": starts,
        "samples": out_samples,
    }


def locomotion(runs: Path) -> dict:
    out_runs, ids = [], set()
    for cfg in LOCOMOTION_RUNS:
        state_path, scen = runs / cfg["state"], runs / cfg["scenario"]
        timeline_path, session_path = scen / "input-timeline.jsonl", scen / "native-session.jsonl"
        for p in (state_path, timeline_path, session_path):
            if not p.exists():
                raise SystemExit(f"missing {p} (live capture data is machine-local; see ACU-RE-0044/0045)")
        header, samples = state_samples(read_jsonl(state_path))
        offset = clock_offset(read_jsonl(session_path))
        if offset is None:
            raise SystemExit(f"{session_path}: no probe events to align the clocks")
        run_json = json.loads((scen / "run.json").read_text())
        body = locomotion_run(samples, header, read_jsonl(timeline_path), offset)
        ids |= {c[0] for s in body["samples"] for c in s["clips"]}
        out_runs.append({
            "run": cfg["run"],
            "sources": {"live_state_sha256": sha256_file(state_path), "input_timeline_sha256": sha256_file(timeline_path),
                        "probe_session_sha256": sha256_file(session_path)},
            "input_profile": Path(run_json["profile"].replace("\\", "/")).name,
            "input_profile_sha256": run_json.get("profile_sha256"),
            "controller": run_json.get("controller_api"),
            **run_context(runs / cfg["capture"]),
            **({"note": cfg["note"]} if cfg.get("note") else {}),
            **body,
        })
    return {
        "schema": "acu-oracle/locomotion/1",
        "findings": ["ACU-RE-0044", "ACU-RE-0045", "ACU-RE-0025", "ACU-RE-0026", "ACU-RE-0027"],
        "binary_sha256": BINARY_SHA256,
        "method": "open-loop scripted XInput timeline (controller_autoplay.py, in-process pad, game unfocused, "
                  "60 fps cap) while tools/native_probe/live_state.py read the player entity's world matrix "
                  "(forward = row 1, position = row 3; ACU-RE-0044) and every playback leaf of the animation graph "
                  "(file ID, graph weight, playback time; ACU-RE-0025/0045) with ReadProcessMemory at a nominal "
                  "30 Hz, asynchronous to the game's frames",
        "time": "t in seconds from the first input step's start; samples before it have negative t. Sample times "
                "come from QPC, input times from Unix time, joined through the probe session (minimum of "
                "time_unix - qpc/f over its events); input times may be late against samples by up to one probe "
                "poll (about 0.1 s), and across all runs the per-run offsets agree within about 0.013 s",
        "coordinates": "as recorded: Z up (ACU-RE-0044, one sample); position units presumed metres, not established; "
                       "heading_deg = atan2(forward.y, forward.x)",
        "sample_fields": {"t": "s from t0", "step": "index of the active input step or null",
                          "position": "entity matrix row 3 (x, y, z), 4 decimals as sampled",
                          "forward": "entity matrix row 1, 4 decimals as sampled",
                          "heading_deg": "derived from forward",
                          "clips": CLIP_COLUMNS},
        "clip_fields": {"file_id": "animation resource file ID (16 hex digits)",
                        "graph_row": "index of the leaf's row in the controller's graph row array (the same file ID can "
                                     "play in two rows)",
                        "weight": "graph row weight: a presence/blend input, not proven final contribution (ACU-RE-0025)",
                        "playback_time_s": "f32 at playback +0x384 as recorded by live_state.py ('time')",
                        "offset": "f32 at playback +0x36C as recorded by live_state.py ('offset'); meaning not established"},
        "clip_names": clip_names(ids),
        "clip_name_source": "local asset index (tools/asset_query.py where), first resource row; label/confidence as given",
        "runs": out_runs,
        "excluded_runs": LOCOMOTION_EXCLUDED,
        "not_established": [
            "position units (presumed metres) and full handedness (ACU-RE-0044 rests on one sample)",
            "camera yaw: the stick is camera-relative and the camera was not recorded, so stick -> world direction is not "
            "reproducible from these inputs alone",
            "whether root motion or simulation moves the entity, and which object owns the authoritative transform",
            "which clip the native graph selects for an input (selection logic); graph weight vs final contribution",
            "meaning of the playback 'offset' field",
            "per-frame completeness: samples are asynchronous reads at ~25-30 Hz, not every game frame",
            "repeatability: open-loop scripts from the same pinned start diverge (ACU-RE-0044 obs. 8); these are single runs",
            "native tick rate and time step (60 fps cap applied by the focus shim)",
            "float determinism of the readout",
        ],
    }


# ---------------------------------------------------------------------------------------------
# Parkour: per selection call (0x140133B00) the candidates, ranking, chosen index and clips after.
# ---------------------------------------------------------------------------------------------

# decoded: parkour_trace.py output; scenario: dir with input-timeline + native-session (scripted runs);
# session: native-session for sampler/scan runs; state: live_state.py stream; trials: (file, kind).
PARKOUR_RUNS = (
    {"run": "parkour-1", "decoded": "parkour-1/decoded", "scenario": "parkour-1"},
    {"run": "parkour-2-preset", "decoded": "parkour-2-preset/decoded", "scenario": "parkour-2-preset"},
    {"run": "parkour-5-runner", "decoded": "parkour-5-runner/decoded", "scenario": "parkour-5-runner/scenario",
     "capture": "parkour-5-runner/capture.json"},
    {"run": "pinned-1a-putback-blocked", "decoded": "pinned-1a-putback-blocked/decoded",
     "scenario": "pinned-1a-putback-blocked/scenario", "capture": "pinned-1a-putback-blocked/capture.json"},
    {"run": "pinned-2a", "decoded": "pinned-2a/decoded", "scenario": "pinned-2a/scenario", "capture": "pinned-2a/capture.json"},
    {"run": "pinned-2b", "decoded": "pinned-2b/decoded", "scenario": "pinned-2b/scenario", "capture": "pinned-2b/capture.json"},
    {"run": "pinned-3a", "decoded": "pinned-3a/decoded", "scenario": "pinned-3a/scenario", "capture": "pinned-3a/capture.json"},
    {"run": "pinned-5", "decoded": "pinned-5/decoded", "scenario": "pinned-5/scenario", "capture": "pinned-5/capture.json"},
    {"run": "catalog-1", "decoded": "catalog-1/decoded", "scenario": "catalog-1/scenario",
     "state": "catalog-1/live-state.jsonl", "capture": "catalog-1/capture.json"},
    {"run": "nav-1/pair-1", "decoded": "nav-1/pair-1/decoded", "scenario": "nav-1/pair-1",
     "state": "nav-1/pair-1-state.jsonl", "capture": "nav-1/capture.json"},
    {"run": "window-sweep-1", "decoded": "window-sweep-1/decoded", "session": "window-sweep-1/native-session.jsonl",
     "state": "window-sweep-1/live-state.jsonl", "trials": ("window-sweep-1/trials.json", "sweep"),
     "capture": "window-sweep-1-launch/capture.json"},
    {"run": "window-sweep-2", "decoded": "window-sweep-2/decoded", "session": "window-sweep-2/native-session.jsonl",
     "state": "window-sweep-2/live-state.jsonl", "trials": ("window-sweep-2/trials.json", "sweep")},
    {"run": "vault-scan-1", "decoded": "vault-scan-1/decoded", "session": "vault-scan-1/native-session.jsonl",
     "state": "vault-scan-1/live-state.jsonl", "trials": ("vault-scan-1/runs.json", "scan")},
    {"run": "vault-scan-2", "decoded": "vault-scan-2/decoded", "session": "vault-scan-2/native-session.jsonl",
     "state": "vault-scan-2/live-state.jsonl", "trials": ("vault-scan-2/runs.json", "scan")},
    {"run": "vault-scan-3", "decoded": "vault-scan-3/decoded", "session": "vault-scan-3/native-session.jsonl",
     "state": "vault-scan-3/live-state.jsonl", "trials": ("vault-scan-3/runs.json", "scan")},
    {"run": "prop-probe-1", "decoded": "prop-probe-1/decoded", "session": "prop-probe-1/native-session.jsonl",
     "state": "prop-probe-1/live-state.jsonl", "trials": ("prop-probe-1/runs.json", "scan")},
)
CANDIDATE_COLUMNS = ["action_type", "fitness", "distance",
                     "unverified_acufixes_height_difference", "unverified_acufixes_horizontal_difference"]
TRIAL_INPUT = {
    "sweep": {"script": "tools/native_probe/staged_sweep.py", "input": "navigate.run_along(psi): left stick re-steered "
              "every 0.1 s toward the commanded world yaw, right trigger 255 (free run), no buttons"},
    "scan": {"script": "tools/native_probe/vault_scan.py", "input": "teleport to origin, short calibration step, then "
             "navigate.run_along(yaw, free_run=True, buttons=['A']): left stick re-steered every 0.1 s toward the "
             "commanded world yaw, right trigger 255, A held"},
}


def candidate_row(c: dict) -> list:
    u = c.get("unverified_acufixes") or {}
    return [c.get("enum"), f32(c.get("fitness")), f32(c.get("distance")),
            f32(u.get("height_difference")), f32(u.get("horizontal_difference"))]


def encode_decision(d: dict) -> dict:
    """One parkour_trace.py decision without addresses: candidates are listed once (identity by the
    candidate pointer, used only to join the entry and exit arrays) and referenced by index."""
    table, index = [], {}

    def ref(c: dict) -> int:
        key = (c.get("ptr"), tuple(candidate_row(c)))
        if key not in index:
            index[key] = len(table)
            table.append(candidate_row(c))
        return index[key]

    considered = [ref(c) for c in d["considered"]] if "considered" in d else None
    ranked = [ref(c) for c in d.get("ranked") or []]
    sel = d.get("selected_index", -1)
    out = {"considered_count": d.get("considered_count"), "considered": considered,
           "ranked_count": d.get("ranked_count"), "ranked": ranked, "selected_index": sel,
           "action_type": d["selected"]["enum"] if d.get("selected") else None, "candidates": table}
    if sel >= 0 and not d.get("selected"):
        out["selected_outside_gather"] = True
    if considered is None:
        out["entry_missing"] = True
    return out


def trial_windows(raw: list[dict], kind: str, q0: float, freq: int) -> list[dict]:
    out = []
    for i, t in enumerate(raw):
        if "qpc_start" not in t:
            continue
        run = t.get("run") or {}
        yaw = t.get("psi_deg") if kind == "sweep" else t.get("yaw_deg")
        row = {"trial": i, "commanded_world_yaw_deg": yaw,
               "t_start": round((t["qpc_start"] - q0) / freq, 4), "t_end": round((t["qpc_end"] - q0) / freq, 4),
               "start_position": [round(v, 4) for v in (run.get("start") or t.get("start") or [])] or None,
               "end_position": [round(v, 4) for v in (run.get("end") or t.get("end") or [])] or None}
        if "qpc_settled" in t:
            row["t_settled"] = round((t["qpc_settled"] - q0) / freq, 4)
        if kind == "sweep":
            row["theta_deg"], row["repeat"] = t.get("theta_deg"), t.get("repeat")
        else:
            row["origin"] = t.get("origin")
            if (t.get("label") or {}).get("file_id"):
                row["prop_file_id"], row["prop_name"] = t["label"]["file_id"], t["label"].get("name")
        out.append(row)
    return out


def trial_at(trials: list[dict], t: float) -> list | None:
    for w in trials:
        if w["t_start"] <= t <= w["t_end"]:
            return [w["trial"], "run"]
        if w.get("t_settled") is not None and w["t_end"] < t <= w["t_settled"]:
            return [w["trial"], "settle"]
    return None


def parkour_run(decisions: list[dict], q0: float, freq: int, steps: list[dict] | None = None,
                samples: list[dict] | None = None, trials: list[dict] | None = None) -> dict:
    """Pure transform of one run's decoded decisions (and optional samples/steps/trials)."""
    samples = [s for s in samples or [] if s.get("position")]
    qpcs = [s["qpc"] for s in samples]
    starts = clip_starts(samples, freq) if samples else []
    first_q, last_q = (qpcs[0], qpcs[-1]) if qpcs else (None, None)
    callers, threads, out = {}, {}, []
    for d in sorted(decisions, key=lambda x: x["qpc"]):
        t = round((d["qpc"] - q0) / freq, 4)
        row = {"t": t,
               "thread": threads.setdefault(d.get("tid"), len(threads)),
               "tester": callers.setdefault(d.get("tester"), len(callers)) if d.get("tester") else None,
               **encode_decision(d)}
        if steps is not None:
            row["step"] = step_at(steps, t)
        if trials is not None:
            row["trial"] = trial_at(trials, t)
        if samples:
            near = nearest_sample(samples, qpcs, d["qpc"], freq)
            row["player"] = ({"position": near[0]["position"], "forward": near[0]["forward"],
                              "sample_dt_s": round(near[1], 4)} if near else None)
            if row["selected_index"] >= 0 and first_q <= d["qpc"] <= last_q:
                row["clips_after"] = [[s["file_id"], round((s["qpc"] - d["qpc"]) / freq, 3), s["weight"]]
                                      for s in sorted(starts, key=lambda s: s["qpc"])
                                      if 0 <= s["qpc"] - d["qpc"] <= CLIP_WINDOW_S * freq]
        out.append(row)
    chosen = Counter(r["action_type"] for r in out if r["selected_index"] >= 0 and r["action_type"] is not None)
    return {"counts": {"decisions": len(out), "with_selection": sum(r["selected_index"] >= 0 for r in out),
                       "no_selection": sum(r["selected_index"] < 0 for r in out),
                       "selected_outside_gather": sum(bool(r.get("selected_outside_gather")) for r in out),
                       "entry_missing": sum(bool(r.get("entry_missing")) for r in out),
                       "max_considered_count": max((r["considered_count"] or 0 for r in out), default=0),
                       "chosen_action_types": {str(k): v for k, v in sorted(chosen.items())}},
            "decisions": out}


def parkour(runs: Path) -> dict:
    out_runs, ids = [], set()
    for cfg in PARKOUR_RUNS:
        dec_path, summary_path = runs / cfg["decoded"] / "decisions.jsonl", runs / cfg["decoded"] / "summary.json"
        scen = runs / cfg["scenario"] if cfg.get("scenario") else None
        session_path = scen / "native-session.jsonl" if scen else runs / cfg["session"]
        for p in (dec_path, summary_path, session_path):
            if not p.exists():
                raise SystemExit(f"missing {p} (live capture data is machine-local; see ACU-RE-0042/0045)")
        summary = json.loads(summary_path.read_text())
        offset = clock_offset(read_jsonl(session_path))
        sources = {"decisions_sha256": sha256_file(dec_path), "probe_session_sha256": summary.get("session_sha256")}
        header, samples = ({}, [])
        if cfg.get("state"):
            state_path = runs / cfg["state"]
            header, samples = state_samples(read_jsonl(state_path))
            sources["live_state_sha256"] = sha256_file(state_path)
        freq = header.get("qpc_frequency") or (offset or {}).get("qpc_frequency") or 10_000_000
        decisions = read_jsonl(dec_path)
        steps = trials = None
        extra: dict = {}
        if scen:
            timeline = read_jsonl(scen / "input-timeline.jsonl")
            sources["input_timeline_sha256"] = sha256_file(scen / "input-timeline.jsonl")
            t0_unix = next(r["unix_time"] for r in timeline if r.get("kind") == "step_start")
            q0, t0_desc = (t0_unix - offset["offset_s"]) * freq, "first input step start"
            steps = input_steps(timeline, t0_unix)
            run_json = json.loads((scen / "run.json").read_text())
            extra = {"input_profile": Path(run_json["profile"].replace("\\", "/")).name,
                     "input_profile_sha256": run_json.get("profile_sha256"), "inputs": steps}
        else:
            q0 = samples[0]["qpc"] if samples else min(d["qpc"] for d in decisions)
            t0_desc = "first live_state.py sample" if samples else "first decision"
        if cfg.get("trials"):
            tpath, kind = runs / cfg["trials"][0], cfg["trials"][1]
            trials = trial_windows(json.loads(tpath.read_text()), kind, q0, freq)
            sources["trials_sha256"] = sha256_file(tpath)
            extra = {**TRIAL_INPUT[kind], "trials": trials}
        body = parkour_run(decisions, q0, freq, steps, samples, trials)
        ids |= {c[0] for d in body["decisions"] for c in d.get("clips_after", [])}
        out_runs.append({
            "run": cfg["run"], "t0": t0_desc, "sources": sources,
            "enum_resolution": summary.get("enum_resolution"),
            "enum_image_sha256": summary.get("image_sha256"),
            "trace_problems": summary.get("problems"),
            "probe_drops_or_overwrites": summary.get("probe_drops_or_overwrites_last_status"),
            "clock_alignment": ({"probe_events": offset["events"], "poll_interval_median_s": offset["poll_interval_median_s"]}
                                if offset and scen else None),
            "has_live_state": bool(samples),
            **run_context(runs / cfg["capture"] if cfg.get("capture") else None),
            **extra,
            **body,
        })
    return {
        "schema": "acu-oracle/parkour/1",
        "findings": ["ACU-RE-0042", "ACU-RE-0043", "ACU-RE-0044", "ACU-RE-0045", "ACU-RE-0046"],
        "binary_sha256": BINARY_SHA256,
        "method": "hardware execute breakpoints on the parkour selection routine 0x140133B00 (entry: candidate pointer "
                  "array, first 16 elements x 672 bytes gathered) and its shared epilogue (EAX = selected index into "
                  "the sorted array, or -1), decoded by tools/parkour_trace.py; clip starts from "
                  "tools/native_probe/live_state.py (nominal 30 Hz) on the same QPC clock, as in "
                  "tools/parkour_clip_pairs.py",
        "time": "t in seconds from each run's t0 (see run.t0). Scripted runs: the first input step; their QPC/Unix "
                "join uses the probe session (input times may be late by up to one poll, about 0.1 s)",
        "decision_fields": {
            "t": "entry time (s from t0)", "thread": "ordinal of the calling thread within the run",
            "tester": "ordinal of the first-argument object (parkour_trace.py 'tester') within the run",
            "considered_count": "array count at entry (all candidates)",
            "considered": "indices into candidates of the gathered entry elements (at most 16), in array order",
            "ranked_count": "array count at exit", "ranked": "indices into candidates of the gathered exit elements, "
            "in array order (the sorted array)",
            "selected_index": "EAX at exit: index into the exit array, -1 = none",
            "action_type": "action type (fancy-table entry 0x45 immediate, ACU-RE-0042/0043) of the selected candidate",
            "candidates": CANDIDATE_COLUMNS, "step": "active input step index (scripted runs)",
            "trial": "[trial index, 'run' | 'settle'] for sweep/scan runs, null between trials",
            "player": "nearest live_state sample within 0.1 s: position, forward (heading = atan2(forward.y, "
                      "forward.x)), sample_dt_s = sample time minus decision time",
            "clips_after": f"[file_id, delay_s, weight] for every clip start within {CLIP_WINDOW_S} s after a "
                           "selecting decision (temporal association only)",
            "selected_outside_gather": "selected index beyond the 16 gathered elements",
            "entry_missing": "exit seen without its entry (considered unknown)"},
        "candidate_fields": {
            "action_type": "int from fancy-table entry 0x45 ('mov eax, imm32; ret'); null if unresolved",
            "fitness": "f32 at candidate +0x204 (read by the fitness getter, ACU-RE-0042)",
            "distance": "f32 at candidate +0xC8 (read by the distance getter, ACU-RE-0042)",
            "unverified_acufixes_height_difference": "f32 at candidate +0xC0; ACUFixes name, unverified",
            "unverified_acufixes_horizontal_difference": "f32 at candidate +0xC4; ACUFixes name, unverified",
            "float_format": "shortest decimal that round-trips to the same f32"},
        "clip_names": clip_names(ids),
        "clip_name_source": "local asset index (tools/asset_query.py where), first resource row; label/confidence as given",
        "runs": out_runs,
        "not_established": [
            "meaning of any action type number",
            "the sort key (fitness x per-action weight at fancy 0x28 is inferred; the weight was not captured)",
            "names/meaning of the +0xC0/+0xC4 fields (ACUFixes, unverified)",
            "candidates beyond the first 16 of an array (only 16 gathered); per-frame completeness (probe drops)",
            "causation of any decision -> clip pair (clips_after is temporal association; window entries start with "
            "no selection call, ACU-RE-0045 obs. 3)",
            "which input caused which action type; camera yaw was not recorded (stick is camera-relative)",
            "what the 'tester' object and the calling threads are",
            "run-to-run comparability: starts, routes and settings differ between runs (ACU-RE-0042, 0044 obs. 8)",
            "input recipe of sweep/scan runs is read from the scripts as committed (a3219f6, after the runs); the "
            "per-step stick values were not logged",
            "what geometry produces a candidate (no collision data joined yet)",
        ],
    }


def dump(obj, depth: int = 0, inline_from: int = 4) -> str:
    """JSON with one record per line below `inline_from`; lists of scalars and flat dicts stay inline."""
    flat = isinstance(obj, (dict, list)) and all(
        not isinstance(v, (dict, list)) for v in (obj.values() if isinstance(obj, dict) else obj))
    if not isinstance(obj, (dict, list)) or not obj or depth >= inline_from or flat:
        return json.dumps(obj, separators=(",", ":") if depth >= inline_from else (", ", ": "))
    pad, end = " " * (depth + 1), " " * depth
    if isinstance(obj, dict):
        body = ",\n".join(f"{pad}{json.dumps(k)}: {dump(v, depth + 1, inline_from)}" for k, v in obj.items())
        return "{\n" + body + "\n" + end + "}"
    return "[\n" + ",\n".join(pad + dump(v, depth + 1, inline_from) for v in obj) + "\n" + end + "]"


# ---------------------------------------------------------------------------------------------
# camera (ACU-RE-0052): camera_sweep.py phases (both sticks, in-process pad) and 50 ms samples of
# the player camera (CameraManager component +0x50 position, +0x60 quaternion) and the player.
# ---------------------------------------------------------------------------------------------

CAMERA_RUNS = ("camera-2",)


def wrap180(d: float) -> float:
    return (d + 180.0) % 360.0 - 180.0


def camera_phase_summary(rows: list[dict]) -> dict:
    """Yaw/pitch change, camera distance and the move heading relative to the starting camera yaw."""
    if len(rows) < 2:
        return {"samples": len(rows)}
    dt = rows[-1]["t"] - rows[0]["t"]
    dyaw = sum(wrap180(b["cam_yaw"] - a["cam_yaw"]) for a, b in zip(rows, rows[1:]))
    out = {
        "samples": len(rows),
        "seconds": round(dt, 3),
        "yaw_change_deg": round(dyaw, 2),
        "yaw_rate_deg_s": round(dyaw / dt, 1) if dt else None,
        "pitch_start_deg": round(rows[0]["cam_pitch"], 2),
        "pitch_end_deg": round(rows[-1]["cam_pitch"], 2),
        "dist_range_m": [round(min(r["dist"] for r in rows), 3), round(max(r["dist"] for r in rows), 3)],
    }
    # Early move heading (first 0.6 s with speed > 0.2 m/s) relative to the camera yaw at that time.
    rel = []
    for a, b in zip(rows, rows[4:]):
        mv = (b["player"][0] - a["player"][0], b["player"][1] - a["player"][1])
        if b["t"] - rows[0]["t"] > 0.8 or math.hypot(*mv) / (b["t"] - a["t"]) < 0.2:
            continue
        rel.append(wrap180(math.degrees(math.atan2(mv[0], mv[1])) - a["cam_yaw"]))
    if rel:
        out["early_move_heading_rel_cam_deg"] = round(sorted(rel)[len(rel) // 2], 1)
    return out


def camera(runs: Path) -> dict:
    out_runs = []
    for name in CAMERA_RUNS:
        path = runs / name / "sweep.jsonl"
        if not path.exists():
            raise SystemExit(f"missing {path} (live capture data is machine-local; see ACU-RE-0052)")
        recs = read_jsonl(path)
        phases = [r for r in recs if r["kind"] == "phase"]
        samples = [r for r in recs if r["kind"] == "sample" and "cam_yaw" in r]
        t0 = phases[0]["t"]
        names = [p["phase"] for p in phases]
        out_runs.append({
            "run": name,
            "phases": [{"phase": p["phase"], "t": round(p["t"] - t0, 3), "left_stick": p["ls"],
                        "right_stick": p["rs"], "seconds": p["seconds"],
                        "summary": camera_phase_summary([s for s in samples if s["phase"] == p["phase"]])}
                       for p in phases],
            "sample_fields": ["t", "phase_index", "cam_pos", "cam_quat_xyzw", "cam_yaw_deg", "cam_pitch_deg",
                              "player_pos", "player_yaw_deg", "cam_player_dist_m"],
            "samples": [[round(s["t"] - t0, 3), names.index(s["phase"]) if s["phase"] in names else None,
                         s["cam_pos"], s["cam_quat"], s["cam_yaw"], s["cam_pitch"], s["player"],
                         s["player_yaw"], s["dist"]] for s in samples],
        })
    return {
        "schema": "acu-oracle/camera/1",
        "finding": "ACU-RE-0052",
        "binary_sha256": BINARY_SHA256,
        "save_pin": SAVE_PIN,
        "world": "ACU_Paris",
        "method": "tools/native_probe/camera_sweep.py: in-process pad (stick range +-32767; 32000 = full, "
                  "16000 = half), 50 ms memory poll of the ACUPlayerCameraComponent owned by the player "
                  "(CameraManager [0x14521AAD0] +0x40 array; component +0x50 position, +0x60 quaternion)",
        "conventions": {
            "yaw": "compass degrees atan2(fx, fy) of forward = q applied to local +Y; 0 = +Y, 90 = +X",
            "pitch": "asin(forward.z), Z up",
            "units": "world units as stored (metres per ACU-RE-0047)",
            "fps_cap": 30,
        },
        "time_resolution_s": 0.05,
        "runs": out_runs,
        "not_established": [
            "right-stick response curve between half and full deflection (two points only)",
            "camera collision and distance policy (distance varies 1.2-3.9 m with pitch and walls)",
            "auto-recentre rules (pitch returns to about -3.3 deg on movement; yaw follows a strafing player)",
            "dependence on frame rate (only a 30 fps cap measured)",
        ],
    }


# ---------------------------------------------------------------------------------------------
# tick (ACU-RE-0051): native probe sessions with tick_time.profile.json (engine frame clock,
# clock tree update, physics step planner, BeginFrame) at each shim fps cap.
# ---------------------------------------------------------------------------------------------

TICK_RUNS = (("tick-1", "events.jsonl", 60), ("tick-1", "events-cap30.jsonl", 30))


def quantiles(values: list[float], digits: int = 5) -> dict:
    v = sorted(values)
    if not v:
        return {"n": 0}
    return {"n": len(v), "p10": round(v[len(v) // 10], digits), "median": round(v[len(v) // 2], digits),
            "p90": round(v[9 * len(v) // 10], digits)}


def tick_run(rows: list[dict]) -> dict:
    hits: dict[str, list[dict]] = {}
    freq = 10_000_000
    for r in rows:
        if r.get("kind") == "header":
            freq = r["status"].get("qpc_frequency", freq)
        if r.get("kind") == "events":
            for h in r["events"]:
                hits.setdefault(h["label"], []).append(h)
    frame = sorted(hits.get("engine_frame_clock", []), key=lambda h: h["qpc"])
    if len(frame) < 3:
        raise SystemExit("too few engine_frame_clock hits")
    q = [h["qpc"] for h in frame]
    snaps = [bytes.fromhex(h["snapshot_hex"]) for h in frame]
    game = [struct.unpack_from("<Q", b, 0x88)[0] for b in snaps]
    span = (q[-1] - q[0]) / freq
    contiguous = [(a, b) for a, b in zip(range(len(q)), range(1, len(q)))]
    game_dt = [(game[b] - game[a]) / 1e5 for a, b in contiguous if game[b] > game[a]]
    wall_dt = [(q[b] - q[a]) / freq for a, b in contiguous]
    steps = Counter()
    for h in hits.get("physics_step_plan", []):
        b = bytes.fromhex(h["snapshot_hex"])
        steps[(round(struct.unpack_from("<f", b, 4)[0], 5), round(struct.unpack_from("<f", b, 8)[0], 2),
               struct.unpack_from("<i", b, 0x14)[0])] += 1
    clock_hits = hits.get("clock_update", [])
    return {
        "frames_hooked": len(frame),
        "span_s": round(span, 3),
        "frame_rate_hz": round((len(frame) - 1) / span, 2),
        "engine_mode": sorted(Counter(struct.unpack_from("<I", b, 0x94)[0] for b in snaps).items()),
        "engine_fixed_hz_field": sorted(Counter(round(struct.unpack_from("<f", b, 0x90)[0], 3) for b in snaps).items()),
        "wall_frame_dt_s": quantiles(wall_dt),
        "game_time_dt_s": quantiles(game_dt),
        "game_time_per_wall_s": round((game[-1] - game[0]) / 1e5 / span, 4),
        "clock_tree_updates_per_frame": round(len(clock_hits) / max(1, len(hits.get("begin_frame", frame))), 2),
        "clock_update_dt_ms": quantiles([struct.unpack_from("<f", bytes.fromhex(h["xmm0_5_hex"]), 16)[0]
                                         for h in clock_hits], 3),
        "physics_plan": [{"step_s": k[0], "rate_hz": k[1], "steps": k[2], "frames": n}
                         for k, n in steps.most_common()],
    }


def tick(runs: Path) -> dict:
    out = []
    for name, fname, cap in TICK_RUNS:
        path = runs / name / fname
        if not path.exists():
            raise SystemExit(f"missing {path} (live capture data is machine-local; see ACU-RE-0051)")
        out.append({"run": name, "file": fname, "fps_cap": cap, **tick_run(read_jsonl(path))})
    return {
        "schema": "acu-oracle/tick/1",
        "finding": "ACU-RE-0051",
        "binary_sha256": BINARY_SHA256,
        "world": "ACU_Paris",
        "method": "native probe entry hooks (tools/native_probe/tick_time.profile.json): engine frame clock "
                  "0x1427743F0 (rcx snapshot: +0x88 game time 1e-5 s, +0x90 f32, +0x94 mode), clock update "
                  "0x14276D750 (xmm1 = dt ms), physics step planner 0x1425FB720 (rcx+0x10F0: dt, step, rate, "
                  "count at +0x14), BeginFrame 0x141CC19A0; shim frame cap chosen at launch",
        "notes": [
            "rates come from hooked hits; the probe ring overwrites under load, so only engine_frame_clock "
            "spans are used for rates (the 30 fps file kept 6.3 s of frame-clock hits)",
            "physics_plan values are those the planner stored on its previous call",
        ],
        "runs": out,
        "not_established": [
            "100 fps and uncapped behaviour (which physics rate bucket is chosen above 60 fps)",
            "the rate bucket table and its thresholds",
            "behaviour while paused or in menus",
        ],
    }


KINDS = {
    "camera": ("data/oracles/camera.json", camera),
    "tick": ("data/oracles/tick.json", tick),
    "world-travel": ("data/oracles/world_travel.json", world_travel),
    "locomotion": ("data/oracles/locomotion.json", locomotion),
    "parkour": ("data/oracles/parkour.json", parkour),
}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("kind", choices=sorted(KINDS))
    ap.add_argument("--runs", type=Path, default=ROOT / "local/native-probe/runs")
    ap.add_argument("--output", type=Path)
    a = ap.parse_args()
    default_out, build = KINDS[a.kind]
    output = a.output or ROOT / default_out
    data = build(a.runs)
    output.parent.mkdir(parents=True, exist_ok=True)
    if a.kind == "world-travel":
        output.write_text(json.dumps(data, indent=1) + "\n")
        print(json.dumps({"output": str(output), "trips": len(data["trips"])}))
        return
    output.write_text(dump(data) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(output), "bytes": output.stat().st_size, "runs": len(data["runs"])}))


if __name__ == "__main__":
    main()
