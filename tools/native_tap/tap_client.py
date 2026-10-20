"""Client for acu_tap.dll's pipe. Usage: python tap_client.py <pid> <command words...>"""
import ctypes as C, json, os, sys, time
from ctypes import wintypes as W
K = C.WinDLL("kernel32", use_last_error=True)
K.CreateFileW.argtypes = [W.LPCWSTR, W.DWORD, W.DWORD, W.LPVOID, W.DWORD, W.DWORD, W.HANDLE]; K.CreateFileW.restype = W.HANDLE
K.ReadFile.argtypes = [W.HANDLE, W.LPVOID, W.DWORD, C.POINTER(W.DWORD), W.LPVOID]
K.WriteFile.argtypes = K.ReadFile.argtypes; K.CloseHandle.argtypes = [W.HANDLE]; K.WaitNamedPipeW.argtypes = [W.LPCWSTR, W.DWORD]

def command(pid, text, timeout=30.0):
    name = "\\\\.\\pipe\\%s-%d" % (os.environ.get("ACU_TAP_STEM", "acu_tap3"), pid); end = time.monotonic() + timeout
    while True:
        if not K.WaitNamedPipeW(name, 1000):
            if time.monotonic() > end: raise C.WinError(C.get_last_error())
            continue
        h = K.CreateFileW(name, 0xC0000000, 0, None, 3, 0, None)
        if h != W.HANDLE(-1).value: break
        time.sleep(0.02)
    try:
        d = W.DWORD(); data = text.encode("ascii")
        K.WriteFile(h, data, len(data), C.byref(d), None)
        out = bytearray()
        while not out.endswith(b"\n"):
            b = C.create_string_buffer(1 << 20)
            if not K.ReadFile(h, b, len(b), C.byref(d), None) and C.get_last_error() != 234: raise C.WinError(C.get_last_error())
            if not d.value: raise RuntimeError("closed")
            out.extend(b.raw[:d.value])
        K.WriteFile(h, b"ACK", 3, C.byref(d), None)
        return json.loads(out)
    finally:
        K.CloseHandle(h)

def read(pid, va, n):
    r = command(pid, f"READ {va:x} {n}")
    return bytes.fromhex(r["hex"]) if r.get("ok") else None

if __name__ == "__main__":
    print(json.dumps(command(int(sys.argv[1]), " ".join(sys.argv[2:]))))
