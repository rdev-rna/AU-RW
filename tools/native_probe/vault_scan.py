"""Scan for vaultable obstacles: teleport Arno to known ground points and free-run outwards.

    python tools/native_probe/vault_scan.py <pid> <out-dir> --from <live-state.jsonl> [...]
        [--spacing 12] [--max-origins 15] [--directions 8] [--run-seconds 3] [--image <mapped>]

Origins are positions from earlier runs' live_state.py logs where only xx_Nav_* clips played
(Arno on the ground), picked greedily at least --spacing metres apart; the scan then expands
breadth-first: a run that ends within 1 m of the origin's height adds
its end point as a new origin when it is --spacing from all others (up to --max-origins). For each origin and each
of --directions headings: teleport there (teleport.py, ACU-RE-0046), calibrate the camera offset
with one short step, then hold the direction with RT + A (free run, parkour up) for --run-seconds.
The live_state.py sampler and the parkour-selection probe session run across the whole scan.

Afterwards every clip start whose name matches VAULT_PATTERNS is a vault event, with Arno's
position and heading when it started, the origin/direction that produced it and the parkour
decision (if any) chosen just before it. Events are clustered (3 m) into spots, ranked by count
and by how many distinct directions reached them: vault-spots.json. Names come from the local
asset index (crc-name confidence); association is temporal, not proof of causation.
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(ROOT / "tools"))
import focus  # noqa: E402
import live_state  # noqa: E402
import navigate  # noqa: E402
import teleport  # noqa: E402
from parkour_clip_pairs import clip_starts, load_samples, names_for  # noqa: E402

PROFILE = HERE / "parkour_selection.profile.json"
VAULT_PATTERNS = ("SkipOver", "SlideOver", "JumpOnBlock", "JumpOn1", "JumpOn2", "JumpOn5", "_Vault")


def known_names(run_dirs: list[Path]) -> dict[str, str]:
    names = {}
    for d in run_dirs:
        f = d / "clips/summary.json"
        if f.exists():
            names.update({k: v["name"] for k, v in json.loads(f.read_text())["names"].items()})
    return names


def ground_points(state_files: list[Path], names: dict[str, str]) -> list[list[float]]:
    out = []
    for f in state_files:
        for line in f.read_text().splitlines()[1:]:
            r = json.loads(line)
            clips, p = r.get("clips"), r.get("position")
            if not p or not clips:
                continue
            labels = [names.get(c["file_id"]) for c in clips]
            if all(n and n.startswith("xx_Nav_") for n in labels):
                out.append(p)
    return out


def pick_origins(points: list[list[float]], spacing: float, limit: int) -> list[list[float]]:
    chosen: list[list[float]] = []
    for p in points:
        if all(math.dist(p[:2], q[:2]) >= spacing for q in chosen):
            chosen.append(p)
            if len(chosen) >= limit:
                break
    return chosen


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pid", type=int)
    ap.add_argument("output", type=Path)
    ap.add_argument("--from", dest="sources", type=Path, nargs="*", default=[],
                    help="live-state.jsonl files of earlier runs (their run dirs' clips/summary.json give names)")
    ap.add_argument("--seeds", type=Path, help="JSON list of [x, y, z] origins to start from instead of --from points")
    ap.add_argument("--avoid", type=Path, nargs="*", default=[],
                    help="origins.json of earlier scans: their done origins are not scanned again")
    ap.add_argument("--no-frontier", action="store_true", help="scan only the given seeds (prop probes)")
    ap.add_argument("--avoid-radius", type=float, default=8, help="metres kept from --avoid origins")
    ap.add_argument("--spacing", type=float, default=12)
    ap.add_argument("--max-origins", type=int, default=15)
    ap.add_argument("--directions", type=int, default=8)
    ap.add_argument("--run-seconds", type=float, default=3)
    ap.add_argument("--image", type=Path)
    a = ap.parse_args()
    out = a.output
    out.mkdir(parents=True, exist_ok=False)
    names = known_names([s.parent for s in a.sources])
    avoid = [q for f in a.avoid for q in json.loads(f.read_text())["done"]]
    raw = json.loads(a.seeds.read_text()) if a.seeds else ground_points(a.sources, names)
    # Seeds may be dicts {"at": [x, y, z, yaw_deg], "label": ...} (prop probes) or plain [x, y, z].
    pool = [r["at"] if isinstance(r, dict) else r for r in raw]
    labels = {tuple(round(v, 3) for v in r["at"]): r.get("label") for r in raw if isinstance(r, dict)}
    pool = [p for p in pool if all(math.dist(p[:2], q[:2]) >= a.avoid_radius for q in avoid)]
    origins = pick_origins(pool, a.spacing, a.max_origins)
    (out / "origins.json").write_text(json.dumps(origins) + "\n")
    print(json.dumps({"origins": len(origins)}), flush=True)
    stop = out / "stop"
    probe = subprocess.Popen([sys.executable, str(HERE / "client.py"), str(a.pid), "session", str(PROFILE),
                              str(out / "native-session.jsonl"), "--seconds", "7200", "--stop-file", str(stop)],
                             stdout=(out / "probe-session.log").open("w"), stderr=subprocess.STDOUT)
    sampler = subprocess.Popen([sys.executable, str(HERE / "live_state.py"), str(a.pid), str(out / "live-state.jsonl"),
                                "--hz", "30", "--stop-file", str(stop)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    runs = []
    try:
        time.sleep(2)
        with focus.InProcessPad(a.pid) as pad:
            nav = navigate.Navigator(a.pid, pad)
            queue, done = list(origins), []
            while queue and len(done) < a.max_origins:
                o = queue.pop(0)
                i = len(done)
                done.append(o)
                # A seed [x, y, z, yaw_deg] (prop probe) runs once along its own heading.
                label = labels.get(tuple(round(v, 3) for v in o))
                yaws = ([math.radians(o[3])] if len(o) > 3 else
                        [2 * math.pi * k / a.directions for k in range(a.directions)])
                o = o[:3]
                for yaw in yaws:
                    run = {"origin": i, "origin_xyz": o, "yaw_deg": round(math.degrees(yaw), 1), "label": label}
                    try:
                        run["teleport"] = {key: v for key, v in teleport.teleport(a.pid, *o, settle=0.8).items()
                                           if key in ("error_m", "status")}
                        # Enemies killed Arno mid-scan in vault-scan-3 (world reload): keep him
                        # invisible to AI and immortal (debug-menu flags, ACU-RE-0046).
                        teleport.set_flag(a.pid, teleport.MODE_INVISIBLE, True)
                        teleport.set_flag(a.pid, teleport.MODE_GOD, True)
                    except RuntimeError as error:  # player chain lost: reload or desync
                        import hop_scan
                        run["skipped"] = f"{error}; world back: {bool(hop_scan.wait_world(a.pid))}"
                        runs.append(run)
                        nav = navigate.Navigator(a.pid, pad)
                        continue
                    if run["teleport"]["status"] != 1 or run["teleport"]["error_m"] > 0.5:
                        run["skipped"] = "teleport failed"
                        runs.append(run)
                        continue
                    try:
                        nav.calibrate(0.4)
                    except RuntimeError as e:
                        run["skipped"] = str(e)
                        runs.append(run)
                        continue
                    run["qpc_start"] = live_state.qpc()
                    r = nav.run_along(yaw, a.run_seconds, free_run=True, buttons=["A"])
                    time.sleep(0.8)
                    run["qpc_end"] = live_state.qpc()
                    run["start"], run["end"] = r["start"], r["end"]
                    # Frontier: a run that ends near the origin's height (street level, a point Arno
                    # just ran over) is a new origin. A "standing still" test never passed in
                    # vault-scan-1: 0.8 s after a run Arno is still slowing down.
                    end = r["end"]
                    if (not a.no_frontier and abs(end[2] - o[2]) < 1.0
                            and all(math.dist(end[:2], q[:2]) >= a.spacing for q in done + queue)
                            and all(math.dist(end[:2], q[:2]) >= a.avoid_radius for q in avoid)):
                        queue.append(end)
                        run["new_origin"] = True
                    runs.append(run)
                    (out / "runs.json").write_text(json.dumps(runs, indent=1) + "\n")
                print(json.dumps({"origin": i, "at": [round(v, 1) for v in o], "queued": len(queue)}), flush=True)
                (out / "origins.json").write_text(json.dumps({"done": done, "queued": queue}) + "\n")
    finally:
        stop.touch()
        probe.wait(timeout=60)
        sampler.wait(timeout=10)
        (out / "runs.json").write_text(json.dumps(runs, indent=1) + "\n")
    decode = [sys.executable, str(ROOT / "tools/parkour_trace.py"), str(out / "native-session.jsonl"), str(out / "decoded")]
    subprocess.run(decode + (["--image", str(a.image)] if a.image else []), check=True, capture_output=True)
    summarize(out, runs)


def summarize(out: Path, runs: list[dict]) -> None:
    freq, samples = load_samples(out / "live-state.jsonl")
    starts = clip_starts(freq, samples)
    names = {k: v["name"] for k, v in names_for({s["file_id"] for s in starts}).items()}
    decisions = []
    dfile = out / "decoded/decisions.jsonl"
    if dfile.exists():
        decisions = [d for d in map(json.loads, dfile.read_text().splitlines())
                     if d.get("selected") and d.get("selected_index", -1) >= 0]
    events = []
    for s in starts:
        name = names.get(s["file_id"], "")
        if not any(p in name for p in VAULT_PATTERNS):
            continue
        run = next((r for r in runs if r.get("qpc_start", 0) <= s["qpc"] <= r.get("qpc_end", 0)), None)
        before = [d for d in decisions if 0 <= s["qpc"] - d["qpc"] <= 1.5 * freq]
        f = s.get("forward") or [0, 1, 0]
        events.append({"clip": name, "file_id": s["file_id"], "at": s["position"],
                       "heading_deg": round(math.degrees(math.atan2(f[1], f[0])), 1),
                       "origin": run and run["origin"], "yaw_deg": run and run["yaw_deg"],
                       "decision": before[-1]["selected"]["enum"] if before else None})
    spots: list[dict] = []
    for e in events:
        spot = next((s for s in spots if math.dist(s["center"][:2], e["at"][:2]) < 3), None)
        if not spot:
            spot = {"center": e["at"], "events": []}
            spots.append(spot)
        spot["events"].append(e)
    for s in spots:
        s["count"] = len(s["events"])
        s["directions"] = len({(e["origin"], e["yaw_deg"]) for e in s["events"]})
        s["clips"] = sorted({e["clip"] for e in s["events"]})
        s["decisions"] = sorted({e["decision"] for e in s["events"] if e["decision"] is not None})
        # Where to stand to repeat it: the run start of the first event's run (known ground).
        first = s["events"][0]
        run = next((r for r in runs if r.get("origin") == first["origin"] and r.get("yaw_deg") == first["yaw_deg"]), None)
        s["approach"] = run and {"start": run["start"], "yaw_deg": run["yaw_deg"]}
    spots.sort(key=lambda s: (-s["directions"], -s["count"]))
    (out / "vault-events.jsonl").write_text("".join(json.dumps(e) + "\n" for e in events))
    # Every non-locomotion clip that started during each run (prop labelling uses all of them).
    with (out / "run-clips.jsonl").open("w", encoding="utf-8") as f:
        for r in runs:
            if "qpc_start" not in r:
                continue
            clips = [names.get(c["file_id"], c["file_id"]) for c in starts if r["qpc_start"] <= c["qpc"] <= r["qpc_end"]]
            row = {k: r.get(k) for k in ("origin", "label", "origin_xyz", "yaw_deg", "start", "end")}
            f.write(json.dumps({**row, "clips": [c for c in clips if not c.startswith("xx_Nav_")]}) + "\n")
    (out / "vault-spots.json").write_text(json.dumps(spots, indent=1) + "\n")
    print(json.dumps({"runs": len(runs), "clip_starts": len(starts), "vault_events": len(events), "spots": len(spots)}))
    for s in spots[:10]:
        print(json.dumps({k: s[k] for k in ("center", "count", "directions", "clips", "decisions")}))


if __name__ == "__main__":
    main()
