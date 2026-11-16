"""Offline self-test of acu_shader_probe.dll (no game): load it into this process, create a
D3D11 device, compile and create one pixel shader, bind it three times, and check that the probe's
JSON reports the shader's DXBC checksum with 1 creation and 3 binds.
    python tools/native_probe/test_shader_probe.py
"""
import ctypes as C
import json
import os
import struct
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DLL = ROOT / "local/native-probe/acu_shader_probe.dll"


def method(obj, slot, restype, *argtypes):
    vtable = C.cast(C.cast(obj, C.POINTER(C.c_void_p))[0], C.POINTER(C.c_void_p))
    return C.WINFUNCTYPE(restype, C.c_void_p, *argtypes)(vtable[slot])


def main():
    d3d11 = C.WinDLL("d3d11.dll")
    compiler = C.WinDLL("d3dcompiler_47.dll")
    C.WinDLL(str(DLL))
    time.sleep(1.0)  # the probe hooks from its own thread
    device, context = C.c_void_p(), C.c_void_p()
    hr = d3d11.D3D11CreateDevice(None, 1, None, 0, None, 0, 7, C.byref(device), None, C.byref(context))
    assert hr == 0, hex(hr & 0xFFFFFFFF)
    src = b"float4 main() : SV_Target { return float4(0.25, 0.5, 0.75, 1); }"
    blob, errors = C.c_void_p(), C.c_void_p()
    hr = compiler.D3DCompile(src, len(src), None, None, None, b"main", b"ps_5_0", 0, 0, C.byref(blob), C.byref(errors))
    assert hr == 0, hex(hr & 0xFFFFFFFF)
    ptr = method(blob, 3, C.c_void_p)(blob)
    size = method(blob, 4, C.c_size_t)(blob)
    code = C.string_at(ptr, size)
    shader = C.c_void_p()
    create = method(device, 15, C.c_long, C.c_void_p, C.c_size_t, C.c_void_p, C.POINTER(C.c_void_p))
    assert create(device, ptr, size, None, C.byref(shader)) == 0
    bind = method(context, 9, None, C.c_void_p, C.c_void_p, C.c_uint)
    for _ in range(3):
        bind(context, shader, None, 0)
    bind(context, None, None, 0)
    time.sleep(2.5)
    report = json.loads((DLL.parent / f"acu_shader_probe-{os.getpid()}.json").read_text())
    want = code[4:20].hex()
    hit = [s for s in report["shaders"] if s["sum"] == want]
    print(json.dumps({k: report[k] for k in ("status", "created", "unique", "unknown_binds", "null_binds")}), hit)
    assert report["status"] == "hooked"
    assert hit and hit[0]["creates"] == 1 and hit[0]["binds"] == 3 and hit[0]["size"] == size
    assert report["null_binds"] >= 1
    (DLL.parent / f"acu_shader_probe-{os.getpid()}.json").unlink()
    print("ok")


if __name__ == "__main__":
    main()
