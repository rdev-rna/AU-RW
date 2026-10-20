#!/usr/bin/env python3
"""Build guarded probe entries for XInput APIs loaded by one verified ACU PID."""
from __future__ import annotations
import argparse
import ctypes as C
from ctypes import wintypes as W
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def profile_for(pid: int) -> dict:
    import sys
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from client import checked

    modules = checked(pid, "MODULES")["modules"]
    selected = [m for m in modules if m["name"].casefold() in {"xinput1_3.dll", "xinput1_4.dll"}]
    if not selected:
        raise RuntimeError("the target has no loaded XInput 1.3/1.4 module")

    kernel = C.WinDLL("kernel32", use_last_error=True)
    kernel.GetProcAddress.argtypes = [W.HMODULE, C.c_char_p]
    kernel.GetProcAddress.restype = C.c_void_p
    kernel.GetModuleHandleW.argtypes = [W.LPCWSTR]
    kernel.GetModuleHandleW.restype = W.HMODULE
    kernel.GetModuleHandleExW.argtypes = [W.DWORD, C.c_void_p, C.POINTER(W.HMODULE)]
    kernel.GetModuleHandleExW.restype = W.BOOL
    kernel.GetModuleFileNameW.argtypes = [W.HMODULE, W.LPWSTR, W.DWORD]
    kernel.GetModuleFileNameW.restype = W.DWORD

    probes = []
    seen = set()
    for module in selected:
        # Load the same local system DLL path, then resolve the API owner because
        # GetProcAddress may return a forwarded export in another loaded module.
        local = C.WinDLL(module["path"])
        handle = kernel.GetModuleHandleW(Path(module["path"]).name)
        if not handle:
            raise C.WinError(C.get_last_error())
        address = kernel.GetProcAddress(handle, b"XInputGetState")
        if not address:
            continue
        owner = W.HMODULE()
        flags = 0x4 | 0x2  # FROM_ADDRESS | UNCHANGED_REFCOUNT
        if not kernel.GetModuleHandleExW(flags, address, C.byref(owner)):
            raise C.WinError(C.get_last_error())
        local_base = C.cast(owner, C.c_void_p).value
        owner_path = C.create_unicode_buffer(32768)
        if not kernel.GetModuleFileNameW(owner, owner_path, len(owner_path)):
            raise C.WinError(C.get_last_error())
        owner_name = Path(owner_path.value).name
        matches = [m for m in modules if m["name"].casefold() == owner_name.casefold()]
        if len(matches) != 1:
            continue
        target_module = matches[0]
        rva = int(address) - int(local_base)
        if not 0 <= rva < target_module["size"] or (owner_name.casefold(), rva) in seen:
            continue
        remote = int(target_module["base"], 16) + rva
        result = checked(pid, f"READ {remote:x} 16")
        expected = result["hex"][:32].lower()
        if len(expected) < 2 or len(expected) % 2:
            continue
        probes.append({"label": "xinput_get_state", "module": target_module["name"],
                       "rva": f"{rva:x}", "expected_hex": expected,
                       "every": 1, "register_index": 0, "bytes": 0,
                       "category": "input"})
        seen.add((owner_name.casefold(), rva))
        del local

    if not probes:
        raise RuntimeError("XInputGetState did not resolve to an address present in the target")
    return {"description": "Live-verified XInputGetState entry probes; entry RCX is the user index.",
            "subsystems": ["input"], "rotation_seconds": 1,
            "probes": probes}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pid", type=int)
    parser.add_argument("output", type=Path,
                        help="JSON output; use ignored local/native-probe storage")
    args = parser.parse_args()
    if not 0 < args.pid <= 0xffffffff:
        parser.error("PID must be uint32 and nonzero")
    output = args.output.resolve()
    if not output.is_relative_to((ROOT/"local/native-probe").resolve()):
        parser.error("output must stay under ignored local/native-probe")
    profile = profile_for(args.pid)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(profile, indent=2)+"\n", encoding="utf-8")
    print(json.dumps({"output": str(output), "probe_count": len(profile["probes"]),
                      "probes": profile["probes"]}))


if __name__ == "__main__":
    main()
