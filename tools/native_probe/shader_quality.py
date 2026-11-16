"""Graphics-quality live check for the character template shaders (ACU-RE-0084).

    python tools/native_probe/shader_quality.py set high      # temporary: quality options at their maxima
    python tools/native_probe/shader_quality.py set low       # back to the shared capture preset
    python tools/native_probe/shader_quality.py templates PAYLOAD...
    python tools/native_probe/shader_quality.py match SNAPSHOT [--since EARLIER] --template NAME=PAYLOAD...

`set high` changes only the quality keys of ACU.ini [Graphics] and GFXSettings.ACU.xml
(EnvironmentQuality/ENVIRONMENT 4, TextureQuality/TEXTURES 2, ShadowQuality/SHADOWS 2, SSAO 2,
Bloom/POST_FX on, REFLECTIONS 3: the maxima the xml's RANGE elements list). Resolution, window
mode, VSync and anti-aliasing stay as the capture preset sets them. `set low` re-applies
tools/capture_preset.py's preset. Both refuse while ACU runs (the game rewrites these files).

`templates` lists every DXBC blob in a template payload with its DXBC checksum (bytes 4..20),
stage, render-target outputs and texture count (d3dcompiler_47 D3DDisassemble). `match` reads an
acu_shader_probe-<pid>.json snapshot and reports, for each template pixel shader, how often the
game created and bound it (minus an earlier snapshot's binds with --since).
"""
from __future__ import annotations

import argparse
import ctypes as C
import json
import re
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))

HIGH_INI = {"EnvironmentQuality": "4", "TextureQuality": "2", "ShadowQuality": "2", "SSAO": "2", "Bloom": "1"}
HIGH_GSA = {"ENVIRONMENT": "4", "TEXTURES": "2", "SHADOWS": "2", "SSAO": "2", "POST_FX": "true",
            "REFLECTIONS": "3"}


def acu_running() -> bool:
    out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq ACU.exe"], capture_output=True, text=True).stdout
    return "ACU.exe" in out


def set_quality(level: str) -> dict:
    import capture_preset
    if acu_running():
        raise SystemExit("ACU.exe is running; quality files are rewritten by the game")
    if level == "low":
        subprocess.run([sys.executable, str(ROOT / "tools/capture_preset.py"), "apply"], check=True)
        return capture_preset.status()
    ini, gsa = capture_preset.ini_path(), capture_preset.gsa_path()
    text = ini.read_text(encoding="utf-8", errors="surrogateescape")
    for key, value in HIGH_INI.items():
        text, n = re.subn(rf"(?m)^{key}=.*$", f"{key}={value}", text)
        if n != 1:
            raise SystemExit(f"ACU.ini has {n} {key} lines")
    ini.write_text(text, encoding="utf-8", errors="surrogateescape", newline="")
    xml = gsa.read_bytes().decode("utf-16-le")
    for key, value in HIGH_GSA.items():
        xml, n = re.subn(rf'(<OPTION Name="{key}"[^>]*? Value=")\w+(")', rf"\g<1>{value}\g<2>", xml)
        if n != 1:
            raise SystemExit(f"GFXSettings.ACU.xml has {n} {key} options")
    gsa.write_bytes(xml.encode("utf-16-le"))
    return {"ini": {k: re.search(rf"(?m)^{k}=(.*)$", text).group(1) for k in HIGH_INI},
            "gsa": {k: re.search(rf'<OPTION Name="{k}"[^>]*? Value="(\w+)"', xml).group(1) for k in HIGH_GSA}}


def disassemble(blob: bytes) -> str | None:
    d3d = C.WinDLL("d3dcompiler_47.dll")
    fn = d3d.D3DDisassemble
    fn.argtypes = [C.c_void_p, C.c_size_t, C.c_uint, C.c_char_p, C.POINTER(C.c_void_p)]
    out = C.c_void_p()
    if fn(blob, len(blob), 0, None, C.byref(out)):
        return None
    vt = C.cast(C.cast(out, C.POINTER(C.c_void_p))[0], C.POINTER(C.c_void_p))
    ptr = C.WINFUNCTYPE(C.c_void_p, C.c_void_p)(vt[3])(out)
    size = C.WINFUNCTYPE(C.c_size_t, C.c_void_p)(vt[4])(out)
    text = C.string_at(ptr, size).decode(errors="replace")
    C.WINFUNCTYPE(C.c_ulong, C.c_void_p)(vt[2])(out)
    return text


def template_blobs(path: Path) -> list[dict]:
    data = path.read_bytes()
    rows, i = [], -1
    while (i := data.find(b"DXBC", i + 1)) >= 0:
        size, = struct.unpack_from("<I", data, i + 24)
        if not 32 < size < 2_000_000 or i + size > len(data):
            continue
        text = disassemble(data[i:i + size])
        if text is None:
            continue
        lines = [l.strip() for l in text.splitlines()]
        stage = next((l for l in lines if l[:3] in ("ps_", "vs_", "hs_", "ds_", "gs_", "cs_")), "?")
        rows.append({"offset": f"{i:#x}", "sum": data[i + 4:i + 20].hex(), "size": size, "stage": stage,
                     "outputs": sum(l.startswith("dcl_output o") for l in lines),
                     "textures": sum(l.startswith("dcl_resource_texture") for l in lines)})
    return rows


def match(snapshot: Path, since: Path | None, templates: list[str]) -> dict:
    now = json.loads(snapshot.read_text())
    before = {s["sum"]: s["binds"] for s in json.loads(since.read_text())["shaders"]} if since else {}
    seen = {s["sum"]: s for s in now["shaders"]}
    report = {"snapshot": str(snapshot), "since": str(since) if since else None, "status": now["status"],
              "created": now["created"], "unique": now["unique"], "unknown_binds": now["unknown_binds"],
              "templates": {}}
    for spec in templates:
        name, path = spec.split("=", 1)
        rows = []
        for blob in template_blobs(Path(path)):
            if blob["stage"] != "ps_5_0":
                continue
            s = seen.get(blob["sum"])
            blob.update(created=s["creates"] if s else 0,
                        binds=(s["binds"] - before.get(blob["sum"], 0)) if s else 0)
            rows.append(blob)
        report["templates"][name] = rows
    return report


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("set")
    s.add_argument("level", choices=("high", "low"))
    t = sub.add_parser("templates")
    t.add_argument("payload", type=Path, nargs="+")
    m = sub.add_parser("match")
    m.add_argument("snapshot", type=Path)
    m.add_argument("--since", type=Path)
    m.add_argument("--template", action="append", default=[], help="NAME=PAYLOAD")
    a = ap.parse_args()
    if a.cmd == "set":
        print(json.dumps(set_quality(a.level)))
    elif a.cmd == "templates":
        for p in a.payload:
            for row in template_blobs(p):
                print(p.name, json.dumps(row))
    else:
        print(json.dumps(match(a.snapshot, a.since, a.template), indent=1))


if __name__ == "__main__":
    main()
