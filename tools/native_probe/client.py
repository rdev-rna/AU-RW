"""Local ACU probe transport, profile rotations, bounded sessions, mapped dump.

All addresses supplied to READ are hexadecimal absolute VAs. Profiles use RVAs.
No profile is enabled automatically. Windows only; standard library only.
"""
from __future__ import annotations
import argparse
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
from pathlib import Path
import time

K = C.WinDLL("kernel32", use_last_error=True)
K.CreateFileW.argtypes = [W.LPCWSTR, W.DWORD, W.DWORD, W.LPVOID, W.DWORD, W.DWORD, W.HANDLE]
K.CreateFileW.restype = W.HANDLE
K.ReadFile.argtypes = [W.HANDLE, W.LPVOID, W.DWORD, C.POINTER(W.DWORD), W.LPVOID]
K.WriteFile.argtypes = K.ReadFile.argtypes
K.CloseHandle.argtypes = [W.HANDLE]
K.WaitNamedPipeW.argtypes = [W.LPCWSTR, W.DWORD]


def command(pid: int, text: str) -> dict:
    data = (text + "\n").encode("ascii")
    if len(data) > 65535:
        raise ValueError("command too long")
    name = rf"\\.\pipe\acu-probe-{pid}"
    deadline = time.monotonic() + 30.0
    while True:
        remaining_ms = max(1, int((deadline - time.monotonic()) * 1000))
        if not K.WaitNamedPipeW(name, remaining_ms):
            raise C.WinError(C.get_last_error())
        pipe = K.CreateFileW(name, 0xC0000000, 0, None, 3, 0, None)
        if pipe != W.HANDLE(-1).value:
            break
        error = C.get_last_error()
        if error != 231 or time.monotonic() >= deadline:
            raise C.WinError(error)
        time.sleep(0.025)
    try:
        done = W.DWORD()
        if not K.WriteFile(pipe, data, len(data), C.byref(done), None):
            raise C.WinError(C.get_last_error())
        result = bytearray()
        while not result.endswith(b"\n"):
            buf = C.create_string_buffer(65536)
            ok = K.ReadFile(pipe, buf, len(buf), C.byref(done), None)
            if not ok and C.get_last_error() != 234:
                raise C.WinError(C.get_last_error())
            if not done.value:
                raise RuntimeError("probe closed pipe without a complete response")
            result.extend(buf.raw[:done.value])
            if len(result) > 1048576:
                raise RuntimeError("oversized response")
        K.WriteFile(pipe, b"ACK", 3, C.byref(done), None)
        return json.loads(result)
    finally:
        K.CloseHandle(pipe)


def checked(pid: int, text: str) -> dict:
    result = command(pid, text)
    if not result.get("ok"):
        raise RuntimeError(result)
    return result


def dump(pid: int, destination: Path, all_regions: bool = False, max_bytes: int = 4*1024**3) -> dict:
    """Preserve readable bytes in sparse RVA/region-layout files; holes recorded.

    Each failed 64KiB read is retried by page (4KiB); partial pages remain holes.
    No permission changes and no guard-page reads. Manifest emitted incrementally.
    """
    destination.mkdir(parents=True, exist_ok=True)
    status = checked(pid, "STATUS")
    modules = checked(pid, "MODULES")["modules"]
    base = int(status["image_base"], 16)
    main = next(m for m in modules if int(m["base"], 16) == base)
    disk = Path(main["path"])
    manifest = {"version": 1, "pid": pid, "qpc_status": status, "modules": modules,
                "disk_executable_sha256": hashlib.sha256(disk.read_bytes()).hexdigest(),
                "layout": "absolute-region" if all_regions else "main-module-rva",
                "regions": [], "files": [], "read_failures": [],
                "coherent_snapshot": False, "time_start_unix": time.time()}
    addr = base if not all_regions else 0
    limit = base + main["size"]
    regions = []
    for _ in range(65536):
        response = checked(pid, f"REGIONS {addr:x}")
        for region in response["regions"]:
            start = int(region["base"], 16)
            if not all_regions and start >= limit:
                break
            regions.append(region)
        next_addr = int(response["next"], 16)
        if next_addr <= addr or not response["regions"] or (not all_regions and next_addr >= limit):
            break
        addr = next_addr
    else:
        raise RuntimeError("region inventory exceeded bound")
    manifest["regions"] = regions
    image_path = destination / "main-image.mapped.bin"
    if not all_regions and main["size"] > max_bytes:
        raise ValueError("main mapped image exceeds dump budget")
    readable_total = sum(r["size"] for r in regions if r["state"] == 0x1000 and not r["protect"] & 0x100 and r["protect"] & (2 | 4 | 8 | 32 | 64 | 128))
    if readable_total > max_bytes:
        raise ValueError(f"readable region total {readable_total} exceeds dump budget {max_bytes}; raise --max-bytes explicitly")
    image = None if all_regions else image_path.open("wb")
    try:
        if image:
            image.truncate(main["size"])
        for idx, region in enumerate(regions):
            start = int(region["base"], 16)
            end = start + region["size"]
            if not all_regions:
                start, end = max(start, base), min(end, limit)
            if end <= start:
                continue
            # MEM_COMMIT, readable base protection, no PAGE_GUARD.
            prot = region["protect"]
            if region["state"] != 0x1000 or prot & 0x100 or not prot & (2 | 4 | 8 | 32 | 64 | 128):
                manifest["read_failures"].append({"address": f"{start:016x}", "length": end-start, "reason": "not readable committed region"})
                continue
            path = destination / f"region-{start:016x}.bin" if all_regions else image_path
            target = path.open("wb") if all_regions else image
            assert target is not None
            file_base = start if all_regions else base
            try:
                if all_regions:
                    target.truncate(end-start)
                pos = start
                while pos < end:
                    length = min(65536, end-pos)
                    result = command(pid, f"READ {pos:x} {length}")
                    if result.get("ok"):
                        target.seek(pos-file_base)
                        target.write(bytes.fromhex(result["hex"]))
                    else:
                        for page in range(pos, pos+length, 4096):
                            size = min(4096, pos+length-page)
                            retry = command(pid, f"READ {page:x} {size}")
                            if retry.get("ok"):
                                target.seek(page-file_base)
                                target.write(bytes.fromhex(retry["hex"]))
                            else:
                                manifest["read_failures"].append({"address": f"{page:016x}", "length": size, "reason": retry.get("error")})
                    pos += length
            finally:
                if all_regions:
                    target.close()
            if all_regions:
                manifest["files"].append({"path": path.name, "base": f"{start:016x}", "sha256": file_hash(path)})
            (destination/"manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    finally:
        if image:
            image.close()
    if not all_regions:
        manifest["files"].append({"path": image_path.name, "base": f"{base:016x}", "sha256": file_hash(image_path)})
    manifest["time_end_unix"] = time.time()
    (destination/"manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    return manifest


def file_hash(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        while data := f.read(1048576):
            h.update(data)
    return h.hexdigest()


def scan(pid: int, address: int, length: int, pattern: str, max_bytes: int = 64*1024**2, max_hits: int = 256) -> dict:
    tokens = pattern.split()
    if not 0 < length <= max_bytes <= 64*1024**2 or not 0 < len(tokens) <= 256 or not 0 < max_hits <= 256:
        raise ValueError("scan max64MiB,pattern max256bytes")
    signature = [None if token == "??" else int(token, 16) for token in tokens]
    if all(v is None for v in signature) or any(v is not None and not 0 <= v <= 255 for v in signature):
        raise ValueError("pattern needs exact byte(s), wildcard is ??")
    hits, holes, tail = [], [], b""
    anchor = next(i for i, value in enumerate(signature) if value is not None)
    anchor_byte = bytes([signature[anchor]])
    for offset in range(0, length, 65536):
        count = min(65536, length-offset)
        result = command(pid, f"READ {address+offset:x} {count}")
        if not result.get("ok"):
            holes.append({"address": f"{address+offset:016x}", "length": count})
            tail = b""
            continue
        data = tail+bytes.fromhex(result["hex"])
        candidate = data.find(anchor_byte, anchor)
        while candidate >= 0:
            i = candidate-anchor
            if i+len(signature)>len(data):
                break
            if all(v is None or data[i+j] == v for j, v in enumerate(signature)):
                hits.append(f"{address+offset-len(tail)+i:016x}")
                if len(hits) >= max_hits:
                    return {"matches": hits, "holes": holes, "truncated": True}
            candidate = data.find(anchor_byte, candidate+1)
        tail = data[-(len(signature)-1):] if len(signature)>1 else b""
    return {"matches": hits, "holes": holes, "truncated": False}


def session(pid: int, profile: Path, output: Path, seconds: float, max_bytes: int,
            stop_file: Path | None = None) -> None:
    config = json.loads(profile.read_text(encoding="utf-8"))
    probes = config.get("probes", [])
    if not probes or len(probes) > 1024 or not 0 < seconds <= 86400 or not 0 < max_bytes <= 1024**3:
        raise ValueError("invalid bounded session config")
    groups = [probes[i:i+4] for i in range(0, len(probes), 4)]
    dwell = float(config.get("rotation_seconds", 2))
    if not .1 <= dwell <= 600:
        raise ValueError("rotation_seconds outside .1..600")
    output.parent.mkdir(parents=True, exist_ok=True)
    written = 0
    with output.open("x", encoding="utf-8") as f:
        def emit(value):
            nonlocal written
            data = json.dumps({"time_unix": time.time(), **value}) + "\n"
            if written + len(data.encode()) > max_bytes:
                raise StopIteration("trace byte limit reached")
            f.write(data); f.flush(); written += len(data.encode())
        end = time.monotonic() + seconds
        group_idx = 0
        try:
            modules = checked(pid, "MODULES")["modules"]
            for module in modules:
                try:
                    module["disk_sha256"] = file_hash(Path(module["path"]))
                except OSError as error:
                    module["disk_hash_unavailable"] = str(error)
            emit({"kind": "header", "status": checked(pid, "STATUS"), "modules": modules, "profile": config})
            while time.monotonic() < end and not (stop_file and stop_file.exists()):
                for slot in range(4):
                    checked(pid, f"UNHOOK {slot}")
                group = groups[group_idx % len(groups)]
                emit({"kind": "rotation", "probes": group})
                for slot, p in enumerate(group):
                    label = p["label"]
                    if not label or any(c.isspace() for c in label):
                        raise ValueError("probe labels must contain no whitespace")
                    kind, address = "HOOK", p["rva"]
                    if "module" in p:
                        matches = [m for m in modules if m["name"].casefold() == p["module"].casefold()]
                        if len(matches) != 1:
                            raise ValueError("profile module must resolve exactly once")
                        module = matches[0]
                        rva = int(p["rva"], 16)
                        if not 0 <= rva < module["size"]:
                            raise ValueError("RVA exceeds selected module")
                        kind, address = "HOOKVA", f"{int(module['base'], 16)+rva:x}"
                    category = p.get("category", "unknown")
                    if not category or any(c.isspace() for c in category):
                        raise ValueError("category must contain no whitespace")
                    cmd = f"{kind} {slot} {address} {p['expected_hex']} {label} {int(p.get('every', 1))} {int(p.get('register_index', 0))} {int(p.get('offset', 0))} {int(p.get('bytes', 0))} {category}"
                    checked(pid, cmd)
                redirect = config.get("redirect")
                if redirect:
                    slot = int(redirect["slot"])
                    source = int(redirect["from_index"])
                    target = int(redirect["to_index"])
                    if not 0 <= slot < 4 or not 0 <= source < 4 or not 0 <= target < 4:
                        raise ValueError("redirect slot and XInput indexes must be in 0..3")
                    module_name = str(redirect["return_module"])
                    matches = [m for m in modules if m["name"].casefold() == module_name.casefold()]
                    if len(matches) != 1:
                        raise ValueError("redirect return module must resolve exactly once")
                    return_rva = int(redirect["return_rva"], 16)
                    if not 0 <= return_rva < matches[0]["size"]:
                        raise ValueError("redirect return RVA exceeds selected module")
                    return_address = int(matches[0]["base"], 16) + return_rva
                    checked(pid, f"REDIRECT {slot} {source:x} {target:x} {return_address:x}")
                    emit({"kind": "redirected", "slot": slot, "from_index": source,
                          "to_index": target, "return_address": f"{return_address:016x}"})
                emit({"kind": "armed", "probes": [p["label"] for p in group]})
                until = min(end, time.monotonic()+dwell)
                while time.monotonic() < until and not (stop_file and stop_file.exists()):
                    time.sleep(min(.1, max(0, until-time.monotonic())))
                    emit({"kind": "events", **checked(pid, "EVENTS 256")})
                if not (stop_file and stop_file.exists()):
                    emit({"kind": "status", **checked(pid, "STATUS")})
                group_idx += 1
        except StopIteration:
            pass
        finally:
            for slot in range(4):
                checked(pid, f"UNHOOK {slot}")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("pid", type=int)
    sub = ap.add_subparsers(dest="mode", required=True)
    c = sub.add_parser("command"); c.add_argument("text")
    d = sub.add_parser("dump"); d.add_argument("destination", type=Path); d.add_argument("--all-regions", action="store_true"); d.add_argument("--max-bytes", type=int, default=4*1024**3)
    q = sub.add_parser("scan"); q.add_argument("address", type=lambda s:int(s,16)); q.add_argument("length", type=int); q.add_argument("pattern"); q.add_argument("--max-bytes", type=int, default=64*1024**2); q.add_argument("--max-hits", type=int, default=256)
    s = sub.add_parser("session"); s.add_argument("profile", type=Path); s.add_argument("output", type=Path); s.add_argument("--seconds", type=float, default=60); s.add_argument("--max-bytes", type=int, default=16*1024*1024); s.add_argument("--stop-file", type=Path)
    a = ap.parse_args()
    if a.mode == "command": print(json.dumps(command(a.pid, a.text)))
    elif a.mode == "dump":
        m = dump(a.pid, a.destination, a.all_regions, a.max_bytes)
        print(json.dumps({"files": m["files"], "failures": len(m["read_failures"])}))
    elif a.mode == "scan": print(json.dumps(scan(a.pid, a.address, a.length, a.pattern, a.max_bytes, a.max_hits)))
    else: session(a.pid, a.profile, a.output, a.seconds, a.max_bytes, a.stop_file)


if __name__ == "__main__":
    main()
