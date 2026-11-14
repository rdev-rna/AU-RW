"""Install or restore placeholder startup videos so ACU reaches the title screen quickly.

    python tools/intro_skip.py install <extracted-mod-dir>   # dir containing videos/...
    python tools/intro_skip.py status
    python tools/intro_skip.py restore

Each replacement must be a small Bink 2 file (`KB2` header, at most 64 KiB) whose relative path
already exists under the game's videos/ folder. Originals are copied to
local/game-backups/intro-skip/ with SHA-256 hashes before anything is overwritten; restore
copies them back and verifies the hashes. Only these video files are touched.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys

ROOT = Path(__file__).resolve().parents[1]
BACKUP = ROOT / "local/game-backups/intro-skip"
MANIFEST = BACKUP / "manifest.json"
MAX_PLACEHOLDER = 64 * 1024


def game_dir() -> Path:
    sys.path.insert(0, str(ROOT / "tools"))
    from acu import settings  # noqa: E402  (project path resolution: env, local/project.json)
    game = settings()[1]["game_dir"]
    if not game or not (game / "ACU.exe").is_file():
        raise SystemExit("ACU game folder not configured (ACU_GAME_DIR or local/project.json)")
    return game


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def placeholders(source: Path) -> list[Path]:
    files = sorted(p for p in (source / "videos").rglob("*") if p.is_file())
    if not files:
        raise SystemExit(f"no videos/ files under {source}")
    for p in files:
        data = p.read_bytes()
        if p.suffix.lower() != ".bk2" or not data.startswith(b"KB2") or len(data) > MAX_PLACEHOLDER:
            raise SystemExit(f"refusing {p}: not a small Bink 2 placeholder")
    return files


def install(source: Path) -> dict:
    game = game_dir()
    files = placeholders(source)
    manifest = json.loads(MANIFEST.read_text()) if MANIFEST.exists() else {"game_dir": str(game), "files": {}}
    for p in files:
        rel = p.relative_to(source).as_posix()
        target = game / rel
        if not target.is_file():
            raise SystemExit(f"refusing {rel}: no such video in the game folder")
        if rel not in manifest["files"]:
            saved = BACKUP / rel
            saved.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(target, saved)
            if sha256(saved) != sha256(target):
                raise SystemExit(f"backup of {rel} does not match the original")
            manifest["files"][rel] = {"original_sha256": sha256(saved), "original_bytes": saved.stat().st_size}
            MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n")
        shutil.copyfile(p, target)
        manifest["files"][rel]["placeholder_sha256"] = sha256(target)
    MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n")
    return status()


def restore() -> dict:
    if not MANIFEST.exists():
        raise SystemExit("no intro-skip backup manifest; nothing to restore")
    game, manifest = Path(json.loads(MANIFEST.read_text())["game_dir"]), json.loads(MANIFEST.read_text())
    for rel, info in manifest["files"].items():
        saved = BACKUP / rel
        if sha256(saved) != info["original_sha256"]:
            raise SystemExit(f"backup of {rel} is corrupt; not restoring it")
        shutil.copyfile(saved, game / rel)
        if sha256(game / rel) != info["original_sha256"]:
            raise SystemExit(f"restore of {rel} did not verify")
    return status()


def status() -> dict:
    if not MANIFEST.exists():
        return {"files": 0, "placeholder": 0, "original": 0, "other": 0, "backup": None}
    manifest = json.loads(MANIFEST.read_text())
    game = Path(manifest["game_dir"])
    counts = {"placeholder": 0, "original": 0, "other": 0}
    for rel, info in manifest["files"].items():
        digest = sha256(game / rel)
        key = ("placeholder" if digest == info.get("placeholder_sha256")
               else "original" if digest == info["original_sha256"] else "other")
        counts[key] += 1
    return {"files": len(manifest["files"]), **counts, "backup": str(BACKUP)}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("mode", choices=("install", "status", "restore"))
    ap.add_argument("source", nargs="?", type=Path, help="extracted mod folder containing videos/")
    a = ap.parse_args()
    if a.mode == "install" and not a.source:
        ap.error("install needs the extracted mod folder")
    result = install(a.source) if a.mode == "install" else restore() if a.mode == "restore" else status()
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
