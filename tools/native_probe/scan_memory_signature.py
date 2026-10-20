"""Search bounded readable memory for exact reference bytes without injection."""
import argparse
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
from pathlib import Path
import time


class Region(C.Structure):
    _fields_ = [("base", C.c_void_p), ("allocation", C.c_void_p),
                ("allocation_protect", W.DWORD), ("partition", W.WORD),
                ("size", C.c_size_t), ("state", W.DWORD),
                ("protect", W.DWORD), ("kind", W.DWORD)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pid", type=int)
    parser.add_argument("output", type=Path)
    parser.add_argument("--pattern-file", type=Path, action="append", required=True)
    parser.add_argument("--expected-sha256", required=True)
    parser.add_argument("--start", type=lambda s: int(s, 0), default=0x10000)
    parser.add_argument("--end", type=lambda s: int(s, 0), default=0x400000000)
    parser.add_argument("--max-bytes", type=int, default=4 * 1024**3)
    parser.add_argument("--max-seconds", type=float, default=45)
    parser.add_argument("--max-hits", type=int, default=256)
    args = parser.parse_args()
    if not 0 <= args.start < args.end <= 0x7FFFFFFFFFFF or args.max_bytes <= 0 or args.max_seconds <= 0:
        parser.error("invalid scan bounds")
    if not 1 <= args.max_hits <= 16384:
        parser.error("max-hits must be 1..16384")
    if args.output.exists():
        parser.error("output already exists")
    if len(args.pattern_file) > 256:
        parser.error("at most 256 patterns are allowed")
    patterns = []
    for path in args.pattern_file:
        with path.open("rb") as stream:
            patterns.append((path, stream.read(4097)))
    if any(not data or len(data) > 4096 for _, data in patterns):
        parser.error("patterns must contain 1..4096 bytes")
    kernel = C.WinDLL("kernel32", use_last_error=True)
    kernel.OpenProcess.argtypes = [W.DWORD, W.BOOL, W.DWORD]
    kernel.OpenProcess.restype = W.HANDLE
    kernel.QueryFullProcessImageNameW.argtypes = [W.HANDLE, W.DWORD, W.LPWSTR, C.POINTER(W.DWORD)]
    kernel.VirtualQueryEx.argtypes = [W.HANDLE, C.c_void_p, C.POINTER(Region), C.c_size_t]
    kernel.VirtualQueryEx.restype = C.c_size_t
    kernel.ReadProcessMemory.argtypes = [W.HANDLE, C.c_void_p, C.c_void_p, C.c_size_t, C.POINTER(C.c_size_t)]
    kernel.CloseHandle.argtypes = [W.HANDLE]
    handle = kernel.OpenProcess(0x0410, False, args.pid)
    if not handle:
        raise C.WinError(C.get_last_error())
    started = time.time()
    report = {"pid": args.pid, "started_unix": started, "coherent_snapshot": False,
              "tool_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "budgets": {"bytes": args.max_bytes, "seconds": args.max_seconds, "hits": args.max_hits},
              "start": hex(args.start), "end": hex(args.end), "hits": [],
              "patterns": [{"path": str(p), "length": len(b), "sha256": hashlib.sha256(b).hexdigest()} for p, b in patterns],
              "bytes_read": 0, "read_failures": [], "readable_regions": 0,
              "limitations": ["Exact bytes only; mutated tables are missed.", "Matches spanning separate regions are not searched.", "Reads race application writers."]}
    try:
        image = C.create_unicode_buffer(32768)
        image_size = W.DWORD(len(image))
        if not kernel.QueryFullProcessImageNameW(handle, 0, image, C.byref(image_size)):
            raise C.WinError(C.get_last_error())
        report["image_path"] = image.value
        with Path(image.value).open("rb") as executable:
            digest = hashlib.file_digest(executable, "sha256").hexdigest()
        report["image_sha256"] = digest
        if digest.lower() != args.expected_sha256.lower():
            raise RuntimeError("target executable hash mismatch")
        address = args.start
        stop = "address_bound"
        for _ in range(65536):
            if address >= args.end:
                break
            if time.time() - started >= args.max_seconds:
                stop = "time_bound"
                break
            region = Region()
            if not kernel.VirtualQueryEx(handle, C.c_void_p(address), C.byref(region), C.sizeof(region)):
                stop = "query_failed"
                report["query_error"] = C.get_last_error()
                break
            region_end = min(region.base + region.size, args.end)
            if region_end <= address:
                stop = "nonadvancing_region"
                break
            if region.state == 0x1000 and not region.protect & 0x100 and region.protect & 0xEE:
                report["readable_regions"] += 1
                cursor, tail = address, b""
                while cursor < region_end:
                    remaining = args.max_bytes - report["bytes_read"]
                    if remaining <= 0 or time.time() - started >= args.max_seconds:
                        stop = "byte_bound" if remaining <= 0 else "time_bound"
                        break
                    size = min(4 * 1024**2, region_end - cursor, remaining)
                    buffer = C.create_string_buffer(size)
                    received = C.c_size_t()
                    ok = kernel.ReadProcessMemory(handle, C.c_void_p(cursor), buffer, size, C.byref(received))
                    if ok and received.value == size:
                        raw = tail + buffer.raw
                        for path, pattern in patterns:
                            if stop == "hit_bound":
                                break
                            offset = raw.find(pattern)
                            while offset >= 0:
                                hit = cursor - len(tail) + offset
                                if hit + len(pattern) > cursor:
                                    report["hits"].append({"address": f"0x{hit:016x}", "pattern": str(path), "length": len(pattern)})
                                if len(report["hits"]) >= args.max_hits:
                                    stop = "hit_bound"
                                    report["partial_chunk"] = True
                                    report["resume_address"] = hex(hit + 1 if len(patterns) == 1 else cursor)
                                    break
                                offset = raw.find(pattern, offset + 1)
                        tail = raw[-(max(len(b) for _, b in patterns) - 1):] if max(len(b) for _, b in patterns) > 1 else b""
                        report["bytes_read"] += size
                    else:
                        report["read_failures"].append({"address": hex(cursor), "length": size, "error": C.get_last_error()})
                        tail = b""
                    if stop == "hit_bound":
                        cursor = int(report["resume_address"], 16)
                        break
                    cursor += size
                if stop != "address_bound":
                    address = cursor
                    break
            address = region_end
        else:
            stop = "region_bound"
        report.update(stop_reason=stop, next_address=hex(address), elapsed_seconds=time.time() - started)
    finally:
        kernel.CloseHandle(handle)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x", encoding="utf8") as output:
        json.dump(report, output, indent=2)
        output.write("\n")
    summary = {k: report[k] for k in ("bytes_read", "readable_regions", "stop_reason", "next_address", "elapsed_seconds")}
    summary.update(hit_count=len(report["hits"]), first_hits=report["hits"][:8])
    print(json.dumps(summary))


if __name__ == "__main__":
    main()
