"""Live sun per time of day (ACU-RE-0073): switch the Season with acu_tod.dll and read g_Frame.

    python tools/native_probe/tod_capture.py PID OUT.json [--seasons 4 5 6 7] [--settle 20]

Needs a game launched with `capture_run.py ... --until world --extra-dll local/native-probe/acu_tod.dll`.
For each season ID: write it to acu_tod-<pid>.req, wait until the DLL reports the call (status 1)
and the current Season ID (season_after), wait `--settle` seconds for the transition, then scan
ACU's writable memory for g_Frame CPU copies and decode them.

g_Frame (FrameConsts, RDEF of the deferred lighting PS 0x143211F50; ACU-RE-0073): copies are found
by `VolumetricFog.SunColor` at +5936 followed by `SunColorByOneOverPi` (= SunColor / pi) at +5952.
Fields read: m_CurrentTime +0, m_WeatherInfo +80, m_ShadowMapSize +128, m_SunLight.m_Direction
+624 (towards the sun, native Z up), m_SunLight.m_Color +640 (= SunColor / pi), Fog.SunColor +5936,
m_GlobalLightingScale is not decoded here. Read-only apart from the request file.
"""
from __future__ import annotations

import argparse
import ctypes as C
import json
import math
import struct
import time
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "local/native-probe"
FIELDS = {"m_CurrentTime": 0, "m_WeatherInfo": 80, "m_ShadowMapSize": 128, "sun_direction": 624,
          "sun_color_by_pi": 640, "fog_sun_color": 5936}

k = C.WinDLL("kernel32", use_last_error=True)


class MBI(C.Structure):
    _fields_ = [("Base", C.c_void_p), ("AllocBase", C.c_void_p), ("AllocProt", C.c_ulong), ("Part", C.c_ushort),
                ("Size", C.c_size_t), ("State", C.c_ulong), ("Protect", C.c_ulong), ("Type", C.c_ulong)]


k.VirtualQueryEx.argtypes = [C.c_void_p, C.c_void_p, C.POINTER(MBI), C.c_size_t]
k.ReadProcessMemory.argtypes = [C.c_void_p, C.c_void_p, C.c_void_p, C.c_size_t, C.POINTER(C.c_size_t)]
k.OpenProcess.restype = C.c_void_p


def read(h, address: int, size: int) -> bytes:
    buf = (C.c_char * size)()
    n = C.c_size_t()
    k.ReadProcessMemory(h, C.c_void_p(address), buf, size, C.byref(n))
    return bytes(buf)[:n.value]


def scan(h) -> list[int]:
    """Addresses of Fog.SunColor whose next float4 is SunColor / pi."""
    hits, a = [], 0x10000
    while a < 0x7FF000000000:
        m = MBI()
        if not k.VirtualQueryEx(h, C.c_void_p(a), C.byref(m), C.sizeof(m)):
            break
        base, size = m.Base or 0, m.Size
        if m.State == 0x1000 and (m.Protect & 0xFF) in (0x04, 0x08, 0x40) and size < 1 << 30:
            raw = read(h, base, size)
            raw = raw[:len(raw) // 16 * 16]
            if raw:
                f = np.frombuffer(raw, dtype="<f4").reshape(-1, 4)
                with np.errstate(all="ignore"):
                    c, d = f[:-1, :3], f[1:, :3]
                    # Night suns can have zero channels: require one positive channel, none negative.
                    ok = (c.min(1) >= 0) & (c.max(1) > 1e-6) & (c.max(1) < 1e5) & np.all(
                        np.abs(d * math.pi - c) <= 1e-4 * np.abs(c) + 1e-7, axis=1)
                hits += [base + 16 * int(i) for i in np.where(ok)[0]]
        a = base + size
    return hits


def decode(h, sun_color_address: int) -> dict | None:
    base = sun_color_address - 5936
    b = read(h, base, 5968)
    if len(b) < 5968:
        return None
    out = {"base": hex(base)}
    for name, offset in FIELDS.items():
        out[name] = [round(x, 6) for x in struct.unpack_from("<4f", b, offset)]
    d = out["sun_direction"][:3]
    out["direction_norm"] = round(math.sqrt(sum(x * x for x in d)), 4)
    return out


def request(pid: int, season: int, timeout: float = 15) -> dict:
    result = OUT / f"acu_tod-{pid}.json"
    before = json.loads(result.read_text())["served"] if result.exists() else 0
    (OUT / f"acu_tod-{pid}.req").write_text(str(season))
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        time.sleep(0.25)
        try:
            r = json.loads(result.read_text())
        except (OSError, ValueError):
            continue
        if r["served"] > before:
            return r
    raise TimeoutError(f"acu_tod did not serve season {season} within {timeout}s")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pid", type=int)
    ap.add_argument("out", type=Path)
    ap.add_argument("--seasons", type=int, nargs="+", default=[4, 5, 6, 7])
    ap.add_argument("--settle", type=float, default=20)
    a = ap.parse_args()
    h = k.OpenProcess(0x0410, False, a.pid)
    if not h:
        raise SystemExit(f"cannot open PID {a.pid}")
    rows = [{"read_only": request(a.pid, 0)}]
    print(json.dumps(rows[0]))
    for season in a.seasons:
        switched = request(a.pid, season)
        time.sleep(a.settle)
        frames = [f for f in (decode(h, x) for x in scan(h)) if f and 0.98 < f["direction_norm"] < 1.02]
        row = {"season": season, "switch": switched, "frames": frames}
        rows.append(row)
        first = frames[0] if frames else {}
        print(json.dumps({"season": season, "status": switched["status"], "before": switched["season_before"],
                          "after": switched["season_after"], "copies": len(frames),
                          "dir": first.get("sun_direction"), "fog_sun_color": first.get("fog_sun_color"),
                          "time": first.get("m_CurrentTime", [None])[0]}))
        a.out.write_text(json.dumps(rows, indent=1))
    print("wrote", a.out)


if __name__ == "__main__":
    main()
