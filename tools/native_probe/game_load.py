"""Measure the load of every running ACU game: CPU share, memory, shim frame rate, cores.

    python tools/native_probe/game_load.py [--seconds 30] [--output local/native-probe/load.json]

Read-only: GetProcessTimes / GetSystemTimes deltas over the window, the process working set, the
affinity mask, and the focus shim's status file (focus-<pid>.json: measured fps, cap). CPU shares
are of the whole machine (100 % = every logical processor busy). Frame rates are sampled once per
second from the shim and averaged; a game without a live shim reports none.
"""
from __future__ import annotations

import argparse
import ctypes as C
from ctypes import wintypes as W
import json
import os
from pathlib import Path
import statistics
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import focus  # noqa: E402
import instance_guard  # noqa: E402

kernel = C.WinDLL("kernel32", use_last_error=True)
psapi = C.WinDLL("psapi")
kernel.OpenProcess.restype = W.HANDLE
kernel.OpenProcess.argtypes = [W.DWORD, W.BOOL, W.DWORD]
kernel.CloseHandle.argtypes = [W.HANDLE]
kernel.GetProcessTimes.argtypes = [W.HANDLE] + [C.POINTER(W.FILETIME)] * 4
kernel.GetSystemTimes.argtypes = [C.POINTER(W.FILETIME)] * 3
kernel.GetProcessAffinityMask.argtypes = [W.HANDLE, C.POINTER(C.c_size_t), C.POINTER(C.c_size_t)]
kernel.GetPriorityClass.argtypes = [W.HANDLE]
PRIORITY_NAMES = {0x20: "normal", 0x4000: "below_normal", 0x40: "idle", 0x8000: "above_normal", 0x80: "high"}


class _Counters(C.Structure):  # PROCESS_MEMORY_COUNTERS
    _fields_ = [("cb", W.DWORD), ("faults", W.DWORD), ("peak_ws", C.c_size_t), ("ws", C.c_size_t),
                ("a", C.c_size_t), ("b", C.c_size_t), ("c", C.c_size_t), ("d", C.c_size_t),
                ("pagefile", C.c_size_t), ("peak_pagefile", C.c_size_t)]


psapi.GetProcessMemoryInfo.argtypes = [W.HANDLE, C.POINTER(_Counters), W.DWORD]


def _ft(ft: W.FILETIME) -> int:
    return (ft.dwHighDateTime << 32) | ft.dwLowDateTime  # 100 ns units


def _process_cpu(handle) -> int:
    times = [W.FILETIME() for _ in range(4)]
    if not kernel.GetProcessTimes(handle, *[C.byref(t) for t in times]):
        raise OSError(C.get_last_error())
    return _ft(times[2]) + _ft(times[3])  # kernel + user


def _system_busy() -> tuple[int, int]:
    idle, kern, usr = W.FILETIME(), W.FILETIME(), W.FILETIME()
    kernel.GetSystemTimes(C.byref(idle), C.byref(kern), C.byref(usr))
    total = _ft(kern) + _ft(usr)  # kernel time includes idle
    return total - _ft(idle), total


def measure(seconds: float = 30) -> dict:
    cpus = os.cpu_count() or 1
    pids = instance_guard.acu_pids()
    handles = {pid: kernel.OpenProcess(0x1000 | 0x0400, False, pid) for pid in pids}  # QUERY_LIMITED|QUERY
    handles = {pid: h for pid, h in handles.items() if h}
    try:
        start_cpu = {pid: _process_cpu(h) for pid, h in handles.items()}
        start_sys, t0 = _system_busy(), time.perf_counter()
        fps = {pid: [] for pid in handles}
        while time.perf_counter() - t0 < seconds:
            for pid in handles:
                s = focus.live(pid)
                if s:
                    fps[pid].append(s["frame"]["fps"])
            time.sleep(1)
        wall = time.perf_counter() - t0
        end_sys = _system_busy()
        games = []
        for pid, h in handles.items():
            used = (_process_cpu(h) - start_cpu[pid]) / 1e7
            mem = _Counters()
            mem.cb = C.sizeof(mem)
            psapi.GetProcessMemoryInfo(h, C.byref(mem), mem.cb)
            mask, system = C.c_size_t(), C.c_size_t()
            kernel.GetProcessAffinityMask(h, C.byref(mask), C.byref(system))
            status = focus.status(pid) or {}
            samples = fps[pid]
            games.append({
                "pid": pid,
                "cpu_pct_of_machine": round(100 * used / wall / cpus, 1),
                "cpu_cores_busy": round(used / wall, 2),
                "working_set_mb": round(mem.ws / 2**20),
                "affinity": f"0x{mask.value:x}",
                "priority": PRIORITY_NAMES.get(kernel.GetPriorityClass(h), hex(kernel.GetPriorityClass(h))),
                "fps_cap": (status.get("frame") or {}).get("fps_cap"),
                "fps_mean": round(statistics.mean(samples), 1) if samples else None,
                "fps_min": min(samples) if samples else None,
                "fps_samples": len(samples),
            })
        busy = end_sys[0] - start_sys[0]
        total = end_sys[1] - start_sys[1]
        return {"schema_version": 1, "seconds": round(wall, 1), "logical_processors": cpus,
                "machine_cpu_pct": round(100 * busy / total, 1) if total else None, "games": games}
    finally:
        for h in handles.values():
            kernel.CloseHandle(h)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seconds", type=float, default=30)
    ap.add_argument("--output", type=Path)
    a = ap.parse_args()
    report = measure(a.seconds)
    text = json.dumps(report, indent=2)
    if a.output:
        a.output.write_text(text + "\n", encoding="utf-8")
    print(text)


if __name__ == "__main__":
    main()
