"""Swap ACU's active uplay_r1_loader64.dll between the original and the custom loader by renaming.

    python tools/loader_swap.py status
    python tools/loader_swap.py use original   # single game (Ubisoft-signed loader)
    python tools/loader_swap.py use custom     # several games at once (capture_run --direct)

Files in the game folder (ACU_GAME_DIR or local/project.json), identified by SHA-256 only:
- uplay_r1_loader64.dll         the active loader (whichever is in use);
- uplay_r1_loader64.dll.bak     the original while the custom one is active (the name found on
                                2026-11-15);
- uplay_r1_loader64.dll.custom  the custom one while the original is active (named by this tool);
- uplay_r1_loader64.3dm         a second copy of the original; never touched.

A swap is two renames (active -> its stash name, stash -> active). Before renaming, the tool checks
that every file has a known hash and that the destination names are free. If the second rename
fails, the first one is undone. Nothing is copied or deleted. It refuses while any ACU.exe runs,
because the running game has the active loader loaded. Each swap is appended to
local/game-backups/loader-swap/log.jsonl.

Why: with the custom loader, ACU.exe started directly runs, and several games can run at once
(ACU-RE-0086). The original is the stock state for single-game runs and measurements (ACU-RE-0085
records that earlier runs used the custom loader).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
LOG = ROOT / "local/game-backups/loader-swap/log.jsonl"
ACTIVE = "uplay_r1_loader64.dll"
STASH = {"original": "uplay_r1_loader64.dll.bak", "custom": "uplay_r1_loader64.dll.custom"}
SPARE = "uplay_r1_loader64.3dm"
KNOWN = {  # SHA-256 -> which loader (observed 2026-11-15)
    "0e06fee61bc4090c745f9c063993e736cd3de621b1439d5ba5882fd53803291f": "original",  # 575,192 bytes, signed
    "532faf5284334a654073b5e65fbb51caf00d3d2e8d994dc539625564a5e8743d": "custom",    # 156,672 bytes
}


def game_dir() -> Path:
    sys.path.insert(0, str(ROOT / "tools"))
    from acu import settings  # noqa: E402  (ACU_GAME_DIR, local/project.json)
    game = settings()[1]["game_dir"]
    if not game or not (Path(game) / "ACU.exe").is_file():
        raise SystemExit("ACU game folder not configured (ACU_GAME_DIR or local/project.json)")
    return Path(game)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def kind(path: Path) -> str | None:
    """'original', 'custom', 'unknown' or None when the file is absent."""
    if not path.is_file():
        return None
    return KNOWN.get(sha256(path), "unknown")


def acu_running() -> list[int]:
    out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq ACU.exe", "/FO", "CSV", "/NH"],
                         capture_output=True, text=True).stdout
    return [int(line.split('","')[1]) for line in out.splitlines() if line.startswith('"ACU.exe"')]


def status(game: Path | None = None) -> dict:
    game = game or game_dir()
    files = {name: kind(game / name) for name in (ACTIVE, *STASH.values(), SPARE)}
    return {"game_dir": str(game), "active": files[ACTIVE], "files": files, "acu_running": acu_running()}


def plan(target: str, files: dict) -> list[tuple[str, str]]:
    """The renames that make `target` active, given each file's kind; [] when it already is.
    Raises ValueError for any state this tool did not create or cannot check."""
    if target not in STASH:
        raise ValueError(f"unknown loader {target!r}; use original or custom")
    active = files.get(ACTIVE)
    if active == target:
        return []
    if active not in STASH:
        raise ValueError(f"active {ACTIVE} is {active or 'missing'}; not a known loader, nothing renamed")
    if files.get(STASH[target]) != target:
        raise ValueError(f"{STASH[target]} is {files.get(STASH[target]) or 'missing'}, expected the {target} loader")
    if files.get(STASH[active]) is not None:
        raise ValueError(f"{STASH[active]} already exists ({files[STASH[active]]}); it would be overwritten")
    return [(ACTIVE, STASH[active]), (STASH[target], ACTIVE)]


def use(target: str, game: Path | None = None) -> dict:
    game = game or game_dir()
    running = acu_running()
    if running:
        raise RuntimeError(f"ACU.exe is running (PIDs {running}); close every game before swapping loaders")
    before = status(game)
    steps = plan(target, before["files"])
    done = []
    try:
        for src, dst in steps:
            os.rename(game / src, game / dst)  # fails if dst exists: never overwrites
            done.append((src, dst))
    except OSError:
        for src, dst in reversed(done):
            os.rename(game / dst, game / src)
        raise
    after = status(game)
    if after["active"] != target:
        raise RuntimeError(f"after the swap the active loader is {after['active']}, not {target}")
    record = {"unix": round(time.time(), 1), "target": target, "renames": steps,
              "before": before["files"], "after": after["files"]}
    LOG.parent.mkdir(parents=True, exist_ok=True)
    with LOG.open("a", encoding="utf-8") as log:
        log.write(json.dumps(record) + "\n")
    return {"active": target, "renames": steps, "files": after["files"]}


def require(target: str) -> None:
    """Raise unless `target` is the active loader (for launchers)."""
    active = status()["active"]
    if active != target:
        raise RuntimeError(f"the {target} uplay loader is required but {active or 'no'} loader is active; "
                           f"run `python tools/loader_swap.py use {target}` with no ACU running")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("status")
    u = sub.add_parser("use")
    u.add_argument("target", choices=sorted(STASH))
    a = ap.parse_args()
    try:
        result = status() if a.cmd == "status" else use(a.target)
    except (RuntimeError, ValueError) as error:
        sys.exit(str(error))
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
