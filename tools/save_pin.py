"""Pin an ACU save as a fixed capture start, and swap it in and out around a capture.

    python tools/save_pin.py pin <name>        # copy the current ACU saves (ACU closed)
    python tools/save_pin.py list
    python tools/save_pin.py status
    python tools/save_pin.py use <name>        # snapshot current saves, then copy the pin in
    python tools/save_pin.py putback <snapshot-dir>   # restore a snapshot made by `use`
    python tools/save_pin.py lease             # show the shared pin lease and which holders live
    python tools/save_pin.py release <holder>  # drop a holder (capture_run prints this for --until world)
    python tools/save_pin.py settle            # put the saves back once no holder and no ACU remain

ACU (Ubisoft Connect product 720) keeps its saves under the launcher's
savegames/<user>/720/ folder (1.save, 2.save on 2026-11-14) and autosaves during play, so
every capture would otherwise start where the previous one ended. Pins and snapshots live
in ignored local/game-backups/save-pins/ with SHA-256 per file; every copy is verified.
Ubisoft Connect cloud save sync may run at launch and exit; copies get a fresh modification
time so the local files are the newest. While it uploads after the game exits it keeps
<slot>.z and <slot>.upload files next to the saves (seen 2026-11-14, gone within a minute);
these are ignored, and `use`/`putback` wait for them to disappear before copying anything.
`pin`, `use` and `putback` refuse while any ACU.exe runs.

Several ACU instances (one per capture session) share the one savegames/<user>/720 folder, so a
pin cannot be swapped per game. Captures therefore share a lease, PINS/_lease.json:
{pin, snapshot, holders: [{holder, pid, owner_pid, since_unix}]}. `acquire` swaps the pin in only
when no lease exists and no ACU runs; a second capture on the same pin joins without copying; a
different pin, or an ACU running without a lease, is refused. The last `release` puts the saves
back only when no ACU.exe runs at all; otherwise the lease stays for `settle`. A holder is dropped
once its game PID (`set_holder_pid`) no longer runs, or, before the PID is known, once the
process that acquired it (owner_pid) is gone. Every lease change runs under PINS/_lease.lock
(created with O_CREAT|O_EXCL; a lock older than LOCK_STALE_S seconds is taken over).
A joined game loads the slot as it is now: another running instance may have autosaved over the
pin since it was copied in, so `acquire` reports whether the live saves still match the pin.
"""
from __future__ import annotations

import argparse
import contextlib
import ctypes as C
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
import winreg

ROOT = Path(__file__).resolve().parents[1]
PINS = ROOT / "local/game-backups/save-pins"
SNAPSHOTS = PINS / "_snapshots"
PRODUCT = "720"
SYNC_SUFFIXES = (".z", ".upload")  # Ubisoft Connect upload in progress
LOCK_STALE_S = 120.0  # a lock file older than this is left over from a crashed process


def save_dir() -> Path:
    with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Ubisoft\Launcher") as key:
        install = Path(winreg.QueryValueEx(key, "InstallDir")[0])
    found = [d for d in (install / "savegames").glob(f"*/{PRODUCT}") if d.is_dir()]
    if len(found) != 1:
        raise SystemExit(f"expected one savegames/<user>/{PRODUCT} folder, found {len(found)}")
    return found[0]


def acu_pids() -> list[int]:
    out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq ACU.exe", "/FO", "CSV", "/NH"],
                         capture_output=True, text=True).stdout
    return [int(line.split('","')[1]) for line in out.splitlines() if line.startswith('"ACU.exe"')]


def game_running() -> bool:
    return bool(acu_pids())


def process_alive(pid: int | None) -> bool:
    """True while process `pid` exists (used for holders whose game PID is not known yet)."""
    if not pid:
        return False
    kernel = C.WinDLL("kernel32")
    kernel.OpenProcess.restype = C.c_void_p
    handle = kernel.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
    if not handle:
        return False
    try:
        code = C.c_ulong()
        ok = kernel.GetExitCodeProcess(C.c_void_p(handle), C.byref(code))
        return bool(ok) and code.value == 259  # STILL_ACTIVE
    finally:
        kernel.CloseHandle(C.c_void_p(handle))


def refuse_if_running():
    if game_running():
        raise SystemExit("close ACU first: it writes its saves while running")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def files_of(folder: Path) -> dict:
    return {p.name: {"sha256": sha256(p), "bytes": p.stat().st_size}
            for p in sorted(folder.iterdir())
            if p.is_file() and p.name != "manifest.json" and not p.name.endswith(SYNC_SUFFIXES)}


def wait_for_sync(folder: Path, timeout: float = 90) -> None:
    deadline = time.monotonic() + timeout
    while True:
        pending = sorted(p.name for p in folder.iterdir() if p.name.endswith(SYNC_SUFFIXES))
        if not pending:
            return
        if time.monotonic() > deadline:
            raise SystemExit(f"Ubisoft Connect still syncing ({pending}) after {timeout}s; nothing copied")
        time.sleep(1)


def copy_set(source: Path, target: Path, files: dict) -> None:
    target.mkdir(parents=True, exist_ok=True)
    for name, info in files.items():
        if sha256(source / name) != info["sha256"]:
            raise SystemExit(f"{source / name} does not match its recorded SHA-256; nothing more copied")
        shutil.copyfile(source / name, target / name)  # fresh mtime on purpose (see module doc)
        if sha256(target / name) != info["sha256"]:
            raise SystemExit(f"copy of {name} to {target} failed verification")


def pin(name: str) -> dict:
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,63}", name) or name.startswith("_"):
        raise SystemExit("pin names: letters, digits, '.', '_', '-'; not starting with '_'")
    refuse_if_running()
    target = PINS / name
    if target.exists():
        raise SystemExit(f"pin {name} exists; choose another name")
    files = files_of(save_dir())
    copy_set(save_dir(), target, files)
    manifest = {"name": name, "pinned_unix": time.time(), "files": files}
    (target / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def load_pin(name: str) -> dict:
    path = PINS / name / "manifest.json"
    if not path.exists():
        raise SystemExit(f"no pin named {name}; see `save_pin.py list`")
    return json.loads(path.read_text())


def use(name: str) -> dict:
    """Snapshot the current saves, then put the pin in place. Returns what putback needs."""
    refuse_if_running()
    manifest = load_pin(name)
    live = save_dir()
    wait_for_sync(live)
    current = files_of(live)
    stamp, n = time.strftime("%Y%m%d-%H%M%S"), 0
    snapshot = SNAPSHOTS / stamp
    while snapshot.exists():  # a lease recovered and swapped again within the same second
        n += 1
        snapshot = SNAPSHOTS / f"{stamp}-{n}"
    snapshot.mkdir(parents=True, exist_ok=False)
    copy_set(live, snapshot, current)
    (snapshot / "manifest.json").write_text(json.dumps({"files": current, "taken_unix": time.time(),
                                                         "before_pin": name}, indent=2) + "\n")
    extra = sorted(set(current) - set(manifest["files"]))
    if extra:  # a slot the pin does not have could be the one Continue loads
        raise SystemExit(f"current saves have files the pin lacks ({extra}); snapshot kept at {snapshot}")
    copy_set(PINS / name, live, manifest["files"])
    return {"pin": name, "snapshot": str(snapshot), "pin_files": manifest["files"], "replaced": current}


def putback(snapshot: str | Path) -> dict:
    refuse_if_running()
    snapshot = Path(snapshot)
    manifest = json.loads((snapshot / "manifest.json").read_text())
    live = save_dir()
    wait_for_sync(live)
    extra = sorted(set(files_of(live)) - set(manifest["files"]))
    if extra:
        raise SystemExit(f"the game created {extra} since the snapshot; left everything as is")
    copy_set(snapshot, live, manifest["files"])
    return {"snapshot": str(snapshot), "restored": manifest["files"]}


@contextlib.contextmanager
def file_lock(path: Path, stale: float = LOCK_STALE_S, timeout: float = 60.0):
    """Cross-process lock: whoever creates `path` (O_CREAT|O_EXCL) holds it until the block ends.

    A lock file older than `stale` seconds belongs to a process that died inside the block and is
    taken over. Keep the guarded work well under `stale`."""
    path.parent.mkdir(parents=True, exist_ok=True)
    deadline = time.monotonic() + timeout
    while True:
        try:
            fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError:
            try:
                age = time.time() - path.stat().st_mtime
            except FileNotFoundError:
                continue  # released between the two calls
            if age > stale:
                with contextlib.suppress(FileNotFoundError, PermissionError):
                    path.unlink()
                continue
            if time.monotonic() > deadline:
                raise SystemExit(f"{path} is held by another process for over {timeout:.0f}s; "
                                 f"if none runs, delete it (taken over automatically after {stale:.0f}s)")
            time.sleep(.2)
            continue
        try:
            os.write(fd, json.dumps({"pid": os.getpid(), "since_unix": time.time()}).encode())
        finally:
            os.close(fd)
        break
    try:
        yield
    finally:
        with contextlib.suppress(FileNotFoundError):
            path.unlink()


def lease_path() -> Path:
    return PINS / "_lease.json"


def lease_lock():
    return file_lock(PINS / "_lease.lock")


def read_lease() -> dict | None:
    path = lease_path()
    return json.loads(path.read_text()) if path.exists() else None


def write_lease(lease: dict | None) -> None:
    path = lease_path()
    if lease is None:
        with contextlib.suppress(FileNotFoundError):
            path.unlink()
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f"{path.name}.{os.getpid()}.tmp")
    tmp.write_text(json.dumps(lease, indent=2) + "\n")
    os.replace(tmp, path)


def holder_alive(holder: dict, running: list[int]) -> bool:
    if holder.get("pid") is not None:
        return holder["pid"] in running
    return process_alive(holder.get("owner_pid"))


def prune(lease: dict, running: list[int]) -> list[dict]:
    """Drop holders whose game (or, before launch, whose capture process) no longer runs."""
    dead = [h for h in lease["holders"] if not holder_alive(h, running)]
    lease["holders"] = [h for h in lease["holders"] if h not in dead]
    return dead


def live_matches_pin(name: str) -> bool:
    files = load_pin(name)["files"]
    current = files_of(save_dir())
    return set(current) == set(files) and all(current[k]["sha256"] == v["sha256"] for k, v in files.items())


def acquire(name: str, holder: str) -> dict:
    """Join or open the shared save-pin lease for one capture (see module doc)."""
    manifest = load_pin(name)
    with lease_lock():
        running = acu_pids()
        lease = read_lease()
        recovered = None
        pruned = prune(lease, running) if lease else []
        if lease and not lease["holders"] and not running:
            # Left over from crashed captures: restore what was there before, then start over.
            write_lease(lease)
            recovered = putback(lease["snapshot"])
            write_lease(None)
            lease = None
        if lease is None:
            if running:
                raise SystemExit(f"ACU is running (PIDs {running}) without a save-pin lease: saves cannot be "
                                 "swapped under it; run without --save-pin")
            used = use(name)
            lease = {"pin": name, "snapshot": used["snapshot"], "opened_unix": time.time(), "holders": []}
            action = "swapped"
        elif lease["pin"] != name:
            if pruned:
                write_lease(lease)
            raise SystemExit(f"save pin {lease['pin']!r} is leased by {[h['holder'] for h in lease['holders']]}; "
                             f"a different pin ({name!r}) cannot be swapped in until they release it")
        else:
            action = "joined"
        if any(h["holder"] == holder for h in lease["holders"]):
            raise SystemExit(f"{holder} already holds the save-pin lease")
        lease["holders"].append({"holder": holder, "pid": None, "owner_pid": os.getpid(), "since_unix": time.time()})
        write_lease(lease)
        return {"action": action, "pin": name, "snapshot": lease["snapshot"], "pin_files": manifest["files"],
                "live_matches_pin": live_matches_pin(name), "holders": [h["holder"] for h in lease["holders"]],
                "pruned": [h["holder"] for h in pruned], "recovered": recovered}


def set_holder_pid(holder: str, pid: int) -> dict:
    """Record the game PID of a holder; from then on the holder lives exactly as long as that game."""
    with lease_lock():
        lease = read_lease()
        entry = next((h for h in (lease or {}).get("holders", []) if h["holder"] == holder), None)
        if entry is None:
            raise SystemExit(f"{holder} holds no save-pin lease")
        entry["pid"] = pid
        write_lease(lease)
        return entry


def _close(lease: dict, running: list[int], keep_saves: bool) -> dict:
    """With no holders left: put the saves back unless any ACU still runs (then leave it for settle)."""
    if running:
        write_lease(lease)
        return {"put_back": False, "deferred": True, "running": running,
                "note": "ACU still running; run `python tools/save_pin.py settle` after it closes"}
    if keep_saves:
        write_lease(None)
        return {"put_back": False, "kept_run_save": True, "snapshot": lease["snapshot"]}
    write_lease(lease)  # kept if putback refuses, so settle can retry
    restored = putback(lease["snapshot"])
    write_lease(None)
    return {"put_back": True, "snapshot": restored["snapshot"]}


def release(holder: str, keep_saves: bool = False) -> dict:
    """Drop one holder. The last one puts the saves back, but only while no ACU.exe runs."""
    with lease_lock():
        lease = read_lease()
        if lease is None:
            return {"released": False, "put_back": False, "note": "no save-pin lease"}
        found = any(h["holder"] == holder for h in lease["holders"])
        lease["holders"] = [h for h in lease["holders"] if h["holder"] != holder]
        running = acu_pids()
        pruned = prune(lease, running)
        result = {"released": found, "pin": lease["pin"], "pruned": [h["holder"] for h in pruned]}
        if lease["holders"]:
            write_lease(lease)
            return {**result, "put_back": False, "remaining": [h["holder"] for h in lease["holders"]]}
        return {**result, **_close(lease, running, keep_saves)}


def settle() -> dict:
    """Put the saves back for a lease whose holders are all gone, once no ACU runs."""
    with lease_lock():
        lease = read_lease()
        if lease is None:
            return {"lease": None, "put_back": False}
        running = acu_pids()
        pruned = prune(lease, running)
        result = {"pin": lease["pin"], "pruned": [h["holder"] for h in pruned]}
        if lease["holders"]:
            write_lease(lease)
            return {**result, "put_back": False, "remaining": [h["holder"] for h in lease["holders"]]}
        return {**result, **_close(lease, running, keep_saves=False)}


def lease_status() -> dict:
    lease = read_lease()
    running = acu_pids()
    if lease is None:
        return {"lease": None, "acu_pids": running}
    return {"lease": {**lease, "holders": [{**h, "alive": holder_alive(h, running)} for h in lease["holders"]]},
            "acu_pids": running}


def status() -> dict:
    current = files_of(save_dir())
    pins = {}
    for path in sorted(PINS.glob("*/manifest.json")):
        if path.parent.name.startswith("_"):
            continue
        files = json.loads(path.read_text())["files"]
        pins[path.parent.name] = files == {k: v for k, v in current.items() if k in files} and set(files) == set(current)
    return {"current": current, "matches_pin": pins}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("mode", choices=("pin", "list", "status", "use", "putback", "lease", "settle", "release"))
    ap.add_argument("arg", nargs="?")
    a = ap.parse_args()
    if a.mode in ("pin", "use", "putback", "release") and not a.arg:
        ap.error(f"{a.mode} needs an argument")
    result = {"pin": lambda: pin(a.arg), "use": lambda: use(a.arg), "putback": lambda: putback(a.arg),
              "status": status, "lease": lease_status, "settle": settle, "release": lambda: release(a.arg),
              "list": lambda: {p.parent.name: json.loads(p.read_text())["pinned_unix"]
                               for p in sorted(PINS.glob("*/manifest.json")) if not p.parent.name.startswith("_")},
              }[a.mode]()
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
