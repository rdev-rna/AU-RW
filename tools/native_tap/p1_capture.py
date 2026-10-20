"""P1 atomic capture for one component: graph leaves + all pose contexts/arrays at entry and exit (game thread).
Usage: python tools/native_tap/p1_capture.py <pid> <component-hex> <profile.json> <tag> [--skip on_s off_s]"""
import json, os, subprocess, sys, time
from pathlib import Path
os.environ.setdefault("ACU_TAP_STEM", "acu_tap7")
sys.path.insert(0, str(Path(__file__).parent))
import tap_client as t
ROOT = Path(__file__).resolve().parents[2]; NP = ROOT / "local/native-probe"
SHA = "748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40"

def arm(c, comp):
    c("RESET"); c("CLEARSNAPS"); assert c(f"DRFILTER 0 {comp:x}")["ok"]   # filter first: arming suspends every thread and an unfiltered probe floods the buffer
    r = c("SNAP 0 entry 1 768 a0"); assert r["ok"], r
    for ph, gid in (("entry", 2), ("exit", 3)): assert c(f"SNAPGRAPH 0 {ph} {gid} a0 +d0 *")["ok"]
    for k in range(6):
        off = 0x278 + 8 * k
        for ph, hb, ab in (("entry", 40, 30), ("exit", 10, 20)):
            assert c(f"SNAP 0 {ph} {hb + k} 384 a0 +d8 * +{off:x} *")["ok"]
            assert c(f"SNAP 0 {ph} {ab + k} 16384 a0 +d8 * +{off:x} * +178 *")["ok"]
    r = c("DRHOOK 0 e9cc0 40534883ec30488bd9488b89d0000000 comp_update exit"); assert r["ok"], r

def main():
    pid = int(sys.argv[1]); comp = int(sys.argv[2], 16); profile = sys.argv[3]; tag = sys.argv[4]
    skip = None
    if "--skip" in sys.argv: i = sys.argv.index("--skip"); skip = (float(sys.argv[i + 1]), float(sys.argv[i + 2]))
    c = lambda s: t.command(pid, s, timeout=8)
    arm(c, comp); r0 = time.time()
    p = subprocess.Popen([sys.executable, str(ROOT / "tools/native_probe/controller_autoplay.py"), str(pid), profile, str(NP / f"run-{tag}"),
                          "--expected-sha256", SHA, "--snapshot", "none"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, cwd=ROOT)
    marks = {"r0": r0}
    if skip:
        time.sleep(max(0, skip[0] - (time.time() - r0))); marks["skip_on"] = time.time(); c(f"DRSKIP 0 {comp:x}")
        time.sleep(max(0, skip[1] - (time.time() - r0))); c("DRSKIP 0 0"); marks["skip_off"] = time.time()
    out, err = p.communicate(timeout=120)
    st = c("STATUS"); c("DRUNHOOK 0")
    dump = NP / f"ev-{tag}.bin"; d = c(f"DUMP {dump.as_posix()}")
    (NP / f"{tag}-meta.json").write_text(json.dumps({"pid": pid, "component": hex(comp), "marks": marks, "status": st, "dump": d, "runner_rc": p.returncode}, indent=1))
    print(json.dumps({"runner_rc": p.returncode, "dump": d, "hooks": st["hooks"]}))

if __name__ == "__main__":
    main()
