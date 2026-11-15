"""lanflag.py <out.json> [--value 1] [--timeout 120]
Waits for a new ACU.exe, polls the network-mode flag at 0x144E52380 (ACU-RE-0090) with ReadProcessMemory,
and as soon as it reads the unpacked retail value 2, writes the requested value (default 1 = LAN). Then it
keeps re-reading for a while and records whether anything changes it back. Process memory only."""
import argparse, ctypes as C, ctypes.wintypes as W, json, subprocess, csv, io, time

K = C.WinDLL("kernel32", use_last_error=True)
K.OpenProcess.restype = W.HANDLE
K.ReadProcessMemory.argtypes = [W.HANDLE, C.c_void_p, C.c_void_p, C.c_size_t, C.POINTER(C.c_size_t)]
K.WriteProcessMemory.argtypes = [W.HANDLE, C.c_void_p, C.c_void_p, C.c_size_t, C.POINTER(C.c_size_t)]
FLAG = 0x144E52380

ap = argparse.ArgumentParser()
ap.add_argument("out")
ap.add_argument("--value", type=int, default=1)
ap.add_argument("--timeout", type=float, default=120)
ap.add_argument("--watch", type=float, default=60, help="seconds to keep re-reading after the write")
a = ap.parse_args()


def acu_pids():
    out = subprocess.run(["tasklist", "/fo", "csv", "/nh", "/fi", "IMAGENAME eq ACU.exe"],
                         capture_output=True, text=True).stdout
    return {int(r[1]) for r in csv.reader(io.StringIO(out)) if r and r[0].lower() == "acu.exe"}


def rd(h):
    v, n = C.c_uint32(), C.c_size_t()
    ok = K.ReadProcessMemory(h, C.c_void_p(FLAG), C.byref(v), 4, C.byref(n))
    return v.value if ok and n.value == 4 else None


before = acu_pids()
log = {"flag": hex(FLAG), "value": a.value, "before": sorted(before), "events": []}
t0 = time.monotonic()
pid = None
while time.monotonic() - t0 < a.timeout and pid is None:
    new = acu_pids() - before
    if new:
        pid = min(new)
    time.sleep(0.05)
if pid is None:
    log["error"] = "no new ACU.exe"
else:
    h = K.OpenProcess(0x0010 | 0x0020 | 0x0008 | 0x0400, False, pid)  # VM_READ|VM_WRITE|VM_OPERATION|QUERY
    log["pid"] = pid
    last, written = None, False
    t1 = time.monotonic()
    while time.monotonic() - t1 < a.timeout + a.watch:
        v = rd(h)
        if v != last:
            log["events"].append({"t_s": round(time.monotonic() - t1, 3), "read": v})
            last = v
        if not written and v == 2:
            val, n = C.c_uint32(a.value), C.c_size_t()
            ok = K.WriteProcessMemory(h, C.c_void_p(FLAG), C.byref(val), 4, C.byref(n))
            log["events"].append({"t_s": round(time.monotonic() - t1, 3), "write": a.value, "ok": bool(ok),
                                  "error": 0 if ok else C.get_last_error()})
            written, t_write = True, time.monotonic()
        if written and time.monotonic() - t_write > a.watch:
            break
        time.sleep(0.001 if not written else 0.05)
    log["final"] = rd(h)
open(a.out, "w").write(json.dumps(log, indent=1))
print(json.dumps(log))
