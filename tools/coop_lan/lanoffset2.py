"""lanoffset2.py <out.json> [--port-offset 10] [--timeout 180]
Waits for a new ACU.exe; once the mode flag reads 2 it writes 1 (LAN, ACU-RE-0090). From then on it rewrites the
Storm port offset byte_145218C33 every ~1 ms (startup resets it) until the Storm config at
*(qword_1451C5F78)+1536 shows main port 3074+offset, or the timeout. Process data only; logs every change."""
import argparse, ctypes as C, ctypes.wintypes as W, json, subprocess, csv, io, time, struct
K = C.WinDLL("kernel32", use_last_error=True); K.OpenProcess.restype = W.HANDLE
K.ReadProcessMemory.argtypes = [W.HANDLE, C.c_void_p, C.c_void_p, C.c_size_t, C.POINTER(C.c_size_t)]
K.WriteProcessMemory.argtypes = [W.HANDLE, C.c_void_p, C.c_void_p, C.c_size_t, C.POINTER(C.c_size_t)]
FLAG, OFF, CFGOBJ = 0x144E52380, 0x145218C33, 0x1451C5F78
ap = argparse.ArgumentParser(); ap.add_argument("out"); ap.add_argument("--port-offset", type=int, default=10); ap.add_argument("--timeout", type=float, default=180)
a = ap.parse_args()
def pids():
    o = subprocess.run(["tasklist", "/fo", "csv", "/nh", "/fi", "IMAGENAME eq ACU.exe"], capture_output=True, text=True).stdout
    return {int(r[1]) for r in csv.reader(io.StringIO(o)) if r and r[0].lower() == "acu.exe"}
def rd(h, addr, n):
    b, g = C.create_string_buffer(n), C.c_size_t()
    return b.raw if K.ReadProcessMemory(h, C.c_void_p(addr), b, n, C.byref(g)) and g.value == n else None
def wr(h, addr, data):
    g = C.c_size_t(); return bool(K.WriteProcessMemory(h, C.c_void_p(addr), data, len(data), C.byref(g)))
before = pids(); log = {"port_offset": a.port_offset, "events": []}; t0 = time.monotonic(); pid = None
while time.monotonic() - t0 < 120 and pid is None:
    n = pids() - before
    if n: pid = min(n)
    time.sleep(0.05)
if pid is None:
    log["error"] = "no new ACU.exe"
else:
    h = K.OpenProcess(0x0010 | 0x0020 | 0x0008 | 0x0400, False, pid); log["pid"] = pid; t1 = time.monotonic()
    ev = lambda **r: log["events"].append({"t_s": round(time.monotonic() - t1, 3), **r})
    flagged = False; last = None; rewrites = 0; want = 3074 + a.port_offset
    while time.monotonic() - t1 < a.timeout:
        f = rd(h, FLAG, 4)
        if f is None: ev(note="process gone"); break
        f = struct.unpack("<I", f)[0]
        if not flagged and f == 2:
            ev(flag_write=wr(h, FLAG, struct.pack("<I", 1))); flagged = True
        if flagged:
            b = rd(h, OFF, 1)
            if b is not None and b[0] != a.port_offset:
                wr(h, OFF, bytes([a.port_offset])); rewrites += 1
            obj = rd(h, CFGOBJ, 8); obj = struct.unpack("<Q", obj)[0] if obj else 0
            ports = None
            if obj:
                p = rd(h, obj + 1536 + 8, 2); p2 = rd(h, obj + 1536 + 412, 6)
                if p and p2: ports = (struct.unpack("<H", p)[0], struct.unpack_from("<H", p2, 0)[0], struct.unpack_from("<H", p2, 4)[0])
            cur = (f, b[0] if b else None, hex(obj), ports)
            if cur != last: ev(flag=f, offset_byte=cur[1], cfg_obj=cur[2], ports=ports, rewrites=rewrites); last = cur
            if ports and ports[0] == want:
                ev(done=True, rewrites=rewrites); break
        time.sleep(0.001)
open(a.out, "w").write(json.dumps(log, indent=1)); print(json.dumps(log)[:1500])
