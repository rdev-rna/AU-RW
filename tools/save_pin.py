"""Pin an ACU save as a fixed capture start, and swap it in and out around a capture.

    python tools/save_pin.py pin <name>        # copy the current ACU saves (ACU closed)
    python tools/save_pin.py list
    python tools/save_pin.py status
    python tools/save_pin.py use <name>        # snapshot current saves, then copy the pin in
    python tools/save_pin.py putback <snapshot-dir>   # restore a snapshot made by `use`

ACU (Ubisoft Connect product 720) keeps its saves under the launcher's
savegames/<user>/720/ folder (1.save, 2.save on 2026-11-14) and autosaves during play, so
every capture would otherwise start where the previous one ended. Pins and snapshots live
in ignored local/game-backups/save-pins/ with SHA-256 per file; every copy is verified.
Ubisoft Connect cloud save sync may run at launch and exit; copies get a fresh modification
time so the local files are the newest. While it uploads after the game exits it keeps
<slot>.z and <slot>.upload files next to the saves (seen 2026-11-14, gone within a minute);
these are ignored, and `use`/`putback` wait for them to disappear before copying anything.
All commands refuse while ACU.exe runs.
"""
from __future__ import annotations

import argparse
import hashlib
import json
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


def save_dir() -> Path:
    with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Ubisoft\Launcher") as key:
        install = Path(winreg.QueryValueEx(key, "InstallDir")[0])
    found = [d for d in (install / "savegames").glob(f"*/{PRODUCT}") if d.is_dir()]
    if len(found) != 1:
        raise SystemExit(f"expected one savegames/<user>/{PRODUCT} folder, found {len(found)}")
    return found[0]


def game_running() -> bool:
    out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq ACU.exe", "/NH"], capture_output=True, text=True).stdout
    return "ACU.exe" in out


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
    snapshot = SNAPSHOTS / time.strftime("%Y%m%d-%H%M%S")
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
    ap.add_argument("mode", choices=("pin", "list", "status", "use", "putback"))
    ap.add_argument("arg", nargs="?")
    a = ap.parse_args()
    if a.mode in ("pin", "use", "putback") and not a.arg:
        ap.error(f"{a.mode} needs an argument")
    result = {"pin": lambda: pin(a.arg), "use": lambda: use(a.arg), "putback": lambda: putback(a.arg),
              "status": status,
              "list": lambda: {p.parent.name: json.loads(p.read_text())["pinned_unix"]
                               for p in sorted(PINS.glob("*/manifest.json")) if not p.parent.name.startswith("_")},
              }[a.mode]()
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
