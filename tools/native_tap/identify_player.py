"""Identify the player's animation component by input causality.

Arms a minimal entry probe on the component update (owner transform snapshot only), drives an
asymmetric slew-limited stick pulse train through the virtual pad, then cross-correlates every
entity's ground speed with the known input envelope. Prints ranked candidates; the top one must
stand clearly above the pack (r gap) and be confirmed by the skip test before use.
Usage: python tools/native_tap/identify_player.py <pid> [tag]
"""
import collections, json, struct, subprocess, sys
from pathlib import Path
import numpy as np
sys.path.insert(0, str(Path(__file__).parent))
import tap_client as t, tap_events as ev_mod

ROOT = Path(__file__).resolve().parents[2]
NP = ROOT / "local/native-probe"
SHA = "748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40"
COMP_UPDATE = "DRHOOK 0 e9cc0 40534883ec30488bd9488b89d0000000 comp_update"
MAG = 14000


def smooth(u):
    return u * u * u * (u * (6 * u - 15) + 10)   # minimum-jerk ramp


def build_profile():
    steps = [{"name": "idle0", "duration_ms": 2500}]
    for k, (hold, sgn) in enumerate([(0.6, 1), (1.0, -1), (0.5, 1), (1.4, -1), (0.8, 1), (1.2, -1)]):
        for i in range(5):
            steps.append({"name": f"p{k}u{i}", "duration_ms": 100, "left_stick": [0, int(sgn * MAG * smooth((i + 1) / 5))]})
        steps.append({"name": f"p{k}hold", "duration_ms": int(hold * 1000), "left_stick": [0, sgn * MAG]})
        for i in range(4):
            steps.append({"name": f"p{k}d{i}", "duration_ms": 100, "left_stick": [0, int(sgn * MAG * (1 - smooth((i + 1) / 5)))]})
        steps.append({"name": f"p{k}gap", "duration_ms": 900})
    steps.append({"name": "tail", "duration_ms": 1500})
    return {"schema_version": 1, "report_hz": 60, "steps": steps}


def main():
    pid = int(sys.argv[1]); tag = sys.argv[2] if len(sys.argv) > 2 else "ident"
    c = lambda s: t.command(pid, s, timeout=8)
    prof = build_profile(); ppath = NP / f"{tag}-profile.json"; ppath.write_text(json.dumps(prof, indent=1))
    c("RESET"); c("CLEARSNAPS"); c("SNAP 0 entry 1 64 a0 +10 * +20"); c(COMP_UPDATE)
    run = NP / f"run-{tag}"
    r = subprocess.run([sys.executable, str(ROOT / "tools/native_probe/controller_autoplay.py"), str(pid), str(ppath), str(run),
                        "--expected-sha256", SHA, "--snapshot", "none"], capture_output=True, text=True, cwd=ROOT)
    c("DRUNHOOK 0"); dump = NP / f"ev-{tag}.bin"; c(f"DUMP {dump.as_posix()}")
    if r.returncode:
        raise SystemExit("runner failed: " + r.stderr[-300:])
    ev = ev_mod.parse(str(dump)); q0 = ev[0]["qpc"]
    tl = [json.loads(l) for l in (run / "input-timeline.jsonl").read_text().splitlines()]
    st = {x["name"]: x["unix_time"] for x in tl if x["kind"] == "step_start"}
    t0 = st[prof["steps"][0]["name"]]; tt, vv = [], []
    for s in prof["steps"]:
        a = st.get(s["name"])
        if a is None: continue
        m = abs(s.get("left_stick", [0, 0])[1]) / MAG; tt += [a - t0, a - t0 + s["duration_ms"] / 1000]; vv += [m, m]
    tt = np.array(tt); vv = np.array(vv)
    by = collections.defaultdict(list)
    for x in ev:
        m = x["snaps"].get(1)
        if m:
            v = struct.unpack("<16f", m); by[x["a"][0]].append(((x["qpc"] - q0) / 1e7, v[12], v[13], v[14]))
    T = np.arange(0, tt[-1] - 0.5, 0.05); res = []
    for a, l in by.items():
        l = np.array(l)
        if len(l) < 300: continue
        px = np.interp(T, l[:, 0], l[:, 1]); py = np.interp(T, l[:, 0], l[:, 2])
        sp = np.clip(np.hypot(np.gradient(px, 0.05), np.gradient(py, 0.05)), 0, 3.0)
        if sp.std() < 1e-3: continue
        best = (-2, 0)
        for lag in np.arange(-1.0, 5.01, 0.05):
            inp = np.interp(T - lag, tt, vv, left=0, right=0)
            if inp.std() < 1e-6: continue
            rr = float(np.corrcoef(sp, inp)[0, 1])
            if rr > best[0]: best = (rr, float(lag))
        res.append((best[0], best[1], a, len(l), float(sp.mean())))
    res.sort(reverse=True)
    out = {"pid": pid, "tag": tag, "entities_scored": len(res), "median_r": float(np.median([r_[0] for r_ in res])),
           "top": [{"r": round(r_, 4), "lag_s": round(l_, 2), "component": hex(a_), "samples": n_, "mean_speed": round(s_, 3)} for r_, l_, a_, n_, s_ in res[:6]]}
    (NP / f"{tag}-result.json").write_text(json.dumps(out, indent=1))
    print(json.dumps(out, indent=1))


if __name__ == "__main__":
    main()
