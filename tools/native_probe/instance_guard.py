"""ACU's single-instance guard: find and release the named semaphore `scimitar_semaphore`.

    python tools/native_probe/instance_guard.py list            # which ACU.exe PIDs hold the guard
    python tools/native_probe/instance_guard.py release <pid>   # close that game's guard handle

Observed 2026-11-15 (ACU.exe SHA-256 748abdc6...27e40, the build focus.GAME_SHA256 names):
- A second ACU.exe started directly from the game folder exits with code 0 before it opens a
  window while another ACU.exe runs. Ubisoft Connect (uplay://launch/720/0) does not start a
  second game at all.
- The running game holds a semaphore named \\Sessions\\<n>\\BaseNamedObjects\\scimitar_semaphore,
  current count 1 of maximum 1. Nobody holds it, so the game is not using it as a lock. It also
  holds an unowned mutex CDdf212806D6EmB31yE0c, which did not need to be touched.
- After that one semaphore handle was closed in the running game (DuplicateHandle with
  DUPLICATE_CLOSE_SOURCE, from outside), a second direct launch opened its own window in 3.3 s.
  Both games kept running, rendering their title screens and reporting a live focus shim.
The name strings are not in the protected on-disk image or the decompiled corpus, so the check
itself (where ACU creates the semaphore and what it does with ERROR_ALREADY_EXISTS) is not located.
Closing the handle is therefore an observed workaround, not a reconstructed rule. The game keeps the
closed handle value and may later pass it to CloseHandle. Release the guard only in a game you
launched, never in another session's game.

Standard library only (ctypes, ntdll). Nothing is written to the game's memory or files.
"""
from __future__ import annotations

import argparse
import ctypes as C
from ctypes import wintypes as W
import json
import subprocess
import sys

GUARD_NAME = "scimitar_semaphore"
GUARD_TYPE = "Semaphore"

PROCESS_DUP_HANDLE = 0x0040
DUPLICATE_CLOSE_SOURCE = 0x1
DUPLICATE_SAME_ACCESS = 0x2
SYSTEM_EXTENDED_HANDLE_INFORMATION = 64
OBJECT_NAME_INFORMATION = 1
OBJECT_TYPE_INFORMATION = 2
STATUS_INFO_LENGTH_MISMATCH = 0xC0000004
# Types whose name queries cannot block (NtQueryObject on some File handles can hang).
NAMED_TYPES = ("Semaphore", "Mutant", "Event", "Section")

kernel = C.WinDLL("kernel32", use_last_error=True)
ntdll = C.WinDLL("ntdll")
kernel.OpenProcess.restype = W.HANDLE
kernel.OpenProcess.argtypes = [W.DWORD, W.BOOL, W.DWORD]
kernel.GetCurrentProcess.restype = W.HANDLE
kernel.CloseHandle.argtypes = [W.HANDLE]
kernel.DuplicateHandle.argtypes = [W.HANDLE, W.HANDLE, W.HANDLE, C.POINTER(W.HANDLE), W.DWORD, W.BOOL, W.DWORD]
ntdll.NtQuerySystemInformation.argtypes = [W.ULONG, C.c_void_p, W.ULONG, C.POINTER(W.ULONG)]
ntdll.NtQueryObject.argtypes = [W.HANDLE, W.ULONG, C.c_void_p, W.ULONG, C.POINTER(W.ULONG)]


class _HandleEntry(C.Structure):  # SYSTEM_HANDLE_TABLE_ENTRY_INFO_EX (x64: 40 bytes)
    _fields_ = [("object", C.c_void_p), ("pid", C.c_size_t), ("handle", C.c_size_t),
                ("access", W.ULONG), ("backtrace", W.USHORT), ("type_index", W.USHORT),
                ("attributes", W.ULONG), ("reserved", W.ULONG)]


class _UnicodeString(C.Structure):
    _fields_ = [("length", W.USHORT), ("maximum", W.USHORT), ("buffer", C.c_void_p)]


def is_guard(object_type: str | None, name: str | None) -> bool:
    """True for the guard semaphore in any session namespace (Local or Global)."""
    return object_type == GUARD_TYPE and bool(name) and name.rsplit("\\", 1)[-1] == GUARD_NAME


def _query_string(handle, info_class: int) -> str | None:
    buf = C.create_string_buffer(0x1000)
    got = W.ULONG()
    if ntdll.NtQueryObject(handle, info_class, buf, len(buf), C.byref(got)) != 0:
        return None
    us = _UnicodeString.from_buffer(buf)
    return C.wstring_at(us.buffer, us.length // 2) if us.buffer else ""


def _process_handles(pid: int) -> list[tuple[int, int]]:
    """(handle value, object type index) for every handle `pid` holds."""
    size = 1 << 22
    while True:
        buf = C.create_string_buffer(size)
        got = W.ULONG()
        status = ntdll.NtQuerySystemInformation(SYSTEM_EXTENDED_HANDLE_INFORMATION, buf, size, C.byref(got))
        status &= 0xFFFFFFFF
        if status == STATUS_INFO_LENGTH_MISMATCH and size < 1 << 30:
            size = max(size * 2, got.value + (1 << 20))
            continue
        if status:
            raise OSError(f"NtQuerySystemInformation failed: 0x{status:08x}")
        break
    count = C.c_size_t.from_buffer(buf).value
    entries = (_HandleEntry * count).from_buffer(buf, 2 * C.sizeof(C.c_size_t))
    return [(e.handle, e.type_index) for e in entries if e.pid == pid]


def named_handles(pid: int, types: tuple[str, ...] = NAMED_TYPES) -> list[dict]:
    """Named kernel objects of the given types that `pid` holds (duplicated briefly, then closed)."""
    process = kernel.OpenProcess(PROCESS_DUP_HANDLE, False, pid)
    if not process:
        raise OSError(f"cannot open process {pid} for handle duplication (win32 {C.get_last_error()})")
    me, type_names, found = kernel.GetCurrentProcess(), {}, []
    try:
        for value, type_index in _process_handles(pid):
            if type_index in type_names and type_names[type_index] not in types:
                continue  # type already known and not wanted: skip the duplication
            local = W.HANDLE()
            if not kernel.DuplicateHandle(process, value, me, C.byref(local), 0, False, DUPLICATE_SAME_ACCESS):
                continue
            try:
                if type_index not in type_names:
                    type_names[type_index] = _query_string(local, OBJECT_TYPE_INFORMATION)
                object_type = type_names[type_index]
                if object_type in types:
                    name = _query_string(local, OBJECT_NAME_INFORMATION)
                    if name:
                        found.append({"handle": value, "type": object_type, "name": name})
            finally:
                kernel.CloseHandle(local)
    finally:
        kernel.CloseHandle(process)
    return found


def guard_handles(pid: int) -> list[dict]:
    return [h for h in named_handles(pid, (GUARD_TYPE,)) if is_guard(h["type"], h["name"])]


def release(pid: int) -> list[dict]:
    """Close every guard handle in `pid`. Returns the handles closed (empty: it held none)."""
    process = kernel.OpenProcess(PROCESS_DUP_HANDLE, False, pid)
    if not process:
        raise OSError(f"cannot open process {pid} for handle duplication (win32 {C.get_last_error()})")
    closed = []
    try:
        for h in guard_handles(pid):
            # DUPLICATE_CLOSE_SOURCE closes the handle in the source process even with no target.
            if not kernel.DuplicateHandle(process, h["handle"], None, None, 0, False, DUPLICATE_CLOSE_SOURCE):
                raise OSError(f"closing handle 0x{h['handle']:x} in {pid} failed (win32 {C.get_last_error()})")
            closed.append({**h, "handle": f"0x{h['handle']:x}"})
    finally:
        kernel.CloseHandle(process)
    if guard_handles(pid):
        raise RuntimeError(f"PID {pid} still holds {GUARD_NAME} after release")
    return closed


def acu_pids() -> list[int]:
    out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq ACU.exe", "/FO", "CSV", "/NH"],
                         capture_output=True, text=True).stdout
    return [int(line.split('","')[1]) for line in out.splitlines() if line.startswith('"ACU.exe"')]


def holders(pids: list[int] | None = None) -> list[int]:
    """The ACU.exe PIDs (of `pids`, default all running) that still hold the guard."""
    result = []
    for pid in acu_pids() if pids is None else pids:
        try:
            if guard_handles(pid):
                result.append(pid)
        except OSError:
            continue  # exited meanwhile
    return result


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("list")
    rel = sub.add_parser("release")
    rel.add_argument("pid", type=int)
    a = ap.parse_args()
    if a.cmd == "list":
        print(json.dumps({"acu_pids": acu_pids(), "guard_holders": holders()}, indent=2))
    else:
        if a.pid not in acu_pids():
            sys.exit(f"PID {a.pid} is not a running ACU.exe")
        print(json.dumps({"pid": a.pid, "closed": release(a.pid)}, indent=2))


if __name__ == "__main__":
    main()
