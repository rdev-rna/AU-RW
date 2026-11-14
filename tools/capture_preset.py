"""Apply or restore the low-GPU capture preset in ACU's user settings (Documents/Assassin's
Creed Unity/: ACU.ini and the NVIDIA GSA SDK file GFXSettings.ACU.xml).

    python tools/capture_preset.py apply
    python tools/capture_preset.py status
    python tools/capture_preset.py restore

Only ACU.ini [Graphics] keys and the xml's RESOLUTION/OPTION values change. Observed 2026-11-14:
with only ACU.ini changed, a later launch rewrote ACU.ini with the xml's values about 11 s after
the Ubisoft Connect launch, so both files are set. The first apply copies both originals, with
SHA-256, to local/game-backups/capture-preset/; restore puts them back byte for byte. Both refuse
while ACU.exe runs, because the game rewrites these files itself. The frame-rate cap is not an
ACU.ini option: it comes from the focus shim (`focus.py install PID --fps-cap 60`).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
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
# The same preset in GFXSettings.ACU.xml (UTF-16LE, no BOM). The name pairing is an implementation
# choice from equal values in both files, not decoded: AA=AntiAliasingMode, ENVIRONMENT, TEXTURES,
# SHADOWS, SSAO, POST_FX=Bloom. REFLECTIONS has no ACU.ini key; it is set to its minimum, 0.
GSA_RESOLUTION = {"Width": "1280", "Height": "720", "RefreshRate": "60"}
GSA_OPTIONS = {"VSYNC": "false", "AA": "0", "ENVIRONMENT": "0", "TEXTURES": "0", "SHADOWS": "0",
               "SSAO": "0", "POST_FX": "false", "REFLECTIONS": "0"}
RESOLUTION_RE = r'<RESOLUTION Width="(\d+)" Height="(\d+)" RefreshRate="(\d+)"'


def ini_path() -> Path:
    return Path(os.environ["USERPROFILE"]) / "Documents" / "Assassin's Creed Unity" / "ACU.ini"


def gsa_path() -> Path:
    return ini_path().with_name("GFXSettings.ACU.xml")


def gsa_values(text: str) -> dict:
    res = re.search(RESOLUTION_RE, text)
    values = dict(zip(("Width", "Height", "RefreshRate"), res.groups())) if res else {}
    values.update(re.findall(r'<OPTION Name="(\w+)"[^>]*? Value="(\w+)"', text))
    return values


def with_gsa_preset(text: str) -> str:
    out, n = re.subn(RESOLUTION_RE, lambda _: '<RESOLUTION Width="{Width}" Height="{Height}" '
                     'RefreshRate="{RefreshRate}"'.format(**GSA_RESOLUTION), text)
    if n != 1:
        raise SystemExit(f"GFXSettings.ACU.xml: expected one RESOLUTION element, found {n}")
    for name, value in GSA_OPTIONS.items():
        out, n = re.subn(rf'(<OPTION Name="{name}"[^>]*? Value=")\w+(")', lambda m: m[1] + value + m[2], out)
        if n != 1:
            raise SystemExit(f"GFXSettings.ACU.xml: expected one {name} option, found {n}")
    return out


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
    ini, gsa = ini_path(), gsa_path()
    manifest_path = BACKUP / "manifest.json"
    manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else None
    if manifest is None:
        BACKUP.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ini, BACKUP / "ACU.ini")
        manifest = {"ini": str(ini), "original_sha256": sha256(BACKUP / "ACU.ini")}
    gsa_text = with_gsa_preset(gsa.read_bytes().decode("utf-16-le")) if gsa.exists() else None
    if gsa_text is not None and "gsa" not in manifest:  # also for backups made before the xml was covered
        shutil.copy2(gsa, BACKUP / gsa.name)
        manifest.update(gsa=str(gsa), gsa_original_sha256=sha256(BACKUP / gsa.name))
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    ini.write_bytes(with_preset(ini.read_bytes().decode("utf-8")).encode("utf-8"))
    if gsa_text is not None:
        gsa.write_bytes(gsa_text.encode("utf-16-le"))
    return status()


def restore() -> dict:
    if game_running():
        raise SystemExit("close ACU first: it rewrites ACU.ini")
    info = json.loads((BACKUP / "manifest.json").read_text())
    if sha256(BACKUP / "ACU.ini") != info["original_sha256"]:
        raise SystemExit("backup ACU.ini does not match its recorded hash; not restoring")
    gsa_backup = BACKUP / "GFXSettings.ACU.xml"
    if "gsa" in info and sha256(gsa_backup) != info["gsa_original_sha256"]:
        raise SystemExit("backup GFXSettings.ACU.xml does not match its recorded hash; not restoring")
    shutil.copyfile(BACKUP / "ACU.ini", info["ini"])
    if "gsa" in info:
        shutil.copyfile(gsa_backup, info["gsa"])
    return status()


def status() -> dict:
    current = graphics(ini_path().read_bytes().decode("utf-8"))
    differing = {k: current.get(k) for k, v in PRESET.items() if current.get(k) != v}
    gsa = gsa_values(gsa_path().read_bytes().decode("utf-16-le")) if gsa_path().exists() else None
    gsa_differing = None if gsa is None else {
        k: gsa.get(k) for k, v in {**GSA_RESOLUTION, **GSA_OPTIONS}.items() if gsa.get(k) != v}
    return {"ini": str(ini_path()), "gsa": str(gsa_path()) if gsa is not None else None,
            "preset_applied": not differing and not gsa_differing, "differs_from_preset": differing,
            "gsa_differs_from_preset": gsa_differing,
            "backup": str(BACKUP) if (BACKUP / "manifest.json").exists() else None}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("mode", choices=("apply", "status", "restore"))
    a = ap.parse_args()
    print(json.dumps(apply() if a.mode == "apply" else restore() if a.mode == "restore" else status(), indent=2))


if __name__ == "__main__":
    main()
