"""Apply or restore the low-GPU capture preset in ACU's user settings (Documents/.../ACU.ini).

    python tools/capture_preset.py apply
    python tools/capture_preset.py status
    python tools/capture_preset.py restore

Only keys under [Graphics] change. The first apply copies the original ACU.ini, with its
SHA-256, to local/game-backups/capture-preset/; restore puts it back byte for byte. Both refuse
while ACU.exe runs, because the game rewrites ACU.ini itself. The frame-rate cap is not an
ACU.ini option: it comes from the focus shim (`focus.py install PID --fps-cap 60`).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
BACKUP = ROOT / "local/game-backups/capture-preset"
# Lowest quality everywhere, windowed 1280x720 (a mode ACU lists), no VSync. Keep it fixed across
# runs: quality settings may change what the game simulates (for example crowd or LOD detail).
PRESET = {
    "DisplayWidth": "1280", "DisplayHeight": "720", "RefreshRate": "60", "WindowMode": "0",
    "VSync": "0", "EnvironmentQuality": "0", "TextureQuality": "0", "ShadowQuality": "0",
    "SSAO": "0", "Bloom": "0", "AntiAliasingMode": "0", "Tessellation": "0",
}


def ini_path() -> Path:
    return Path(os.environ["USERPROFILE"]) / "Documents" / "Assassin's Creed Unity" / "ACU.ini"


def game_running() -> bool:
    out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq ACU.exe", "/NH"], capture_output=True, text=True).stdout
    return "ACU.exe" in out


def graphics(text: str) -> dict:
    values, section = {}, None
    for line in text.splitlines():
        s = line.strip()
        if s.startswith("[") and s.endswith("]"):
            section = s[1:-1]
        elif section == "Graphics" and "=" in s:
            k, v = s.split("=", 1)
            values[k.strip()] = v.strip()
    return values


def with_preset(text: str) -> str:
    newline = "\r\n" if "\r\n" in text else "\n"
    lines, out, section, seen = text.splitlines(), [], None, set()
    if "[Graphics]" not in [l.strip() for l in lines]:
        raise SystemExit("ACU.ini has no [Graphics] section; launch the game once first")
    for line in lines:
        s = line.strip()
        if s.startswith("[") and s.endswith("]"):
            if section == "Graphics":  # append keys the file did not have yet
                out += [f"{k}={v}" for k, v in PRESET.items() if k not in seen]
            section = s[1:-1]
        elif section == "Graphics" and "=" in s and s.split("=", 1)[0].strip() in PRESET:
            key = s.split("=", 1)[0].strip()
            out.append(f"{key}={PRESET[key]}")
            seen.add(key)
            continue
        out.append(line)
    if section == "Graphics":
        out += [f"{k}={v}" for k, v in PRESET.items() if k not in seen]
    return newline.join(out) + newline


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def apply() -> dict:
    if game_running():
        raise SystemExit("close ACU first: it rewrites ACU.ini")
    ini = ini_path()
    manifest = BACKUP / "manifest.json"
    if not manifest.exists():
        BACKUP.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ini, BACKUP / "ACU.ini")
        manifest.write_text(json.dumps({"ini": str(ini), "original_sha256": sha256(BACKUP / "ACU.ini")}, indent=2) + "\n")
    text = ini.read_bytes().decode("utf-8")
    ini.write_bytes(with_preset(text).encode("utf-8"))
    return status()


def restore() -> dict:
    if game_running():
        raise SystemExit("close ACU first: it rewrites ACU.ini")
    info = json.loads((BACKUP / "manifest.json").read_text())
    if sha256(BACKUP / "ACU.ini") != info["original_sha256"]:
        raise SystemExit("backup ACU.ini does not match its recorded hash; not restoring")
    shutil.copyfile(BACKUP / "ACU.ini", info["ini"])
    return status()


def status() -> dict:
    current = graphics(ini_path().read_bytes().decode("utf-8"))
    differing = {k: current.get(k) for k, v in PRESET.items() if current.get(k) != v}
    return {"ini": str(ini_path()), "preset_applied": not differing, "differs_from_preset": differing,
            "backup": str(BACKUP) if (BACKUP / "manifest.json").exists() else None}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("mode", choices=("apply", "status", "restore"))
    a = ap.parse_args()
    print(json.dumps(apply() if a.mode == "apply" else restore() if a.mode == "restore" else status(), indent=2))


if __name__ == "__main__":
    main()
