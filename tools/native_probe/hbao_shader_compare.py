"""Disassemble the DXBC shaders inside ACU's GFSDK_SSAO.win64.dll and match them with For Honor's
HBAO+ shader dumps by opcode sequence (finding ACU-RE-0105).

Usage: python tools/native_probe/hbao_shader_compare.py [--dll PATH] [--fh DIR] [--out DIR]

Defaults: the DLL from ACU_GAME_DIR (or local/project.json via shader_quality's lookup is not
used here; pass --dll), For Honor's read-only dumps in
ForHonorRustRewrite/local-extracted/hbao, disassembly written to local/hbao/asm (ignored).
Uses D3DCompiler's D3DDisassemble through shader_quality.disassemble. Reads only.
"""
import argparse
import difflib
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from shader_quality import disassemble  # noqa: E402

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def opcodes(text):
    out = []
    for line in text.splitlines():
        line = line.split("//")[0].strip().strip("\0")
        if not line or line.startswith("dcl_"):
            continue
        out.append(line.split()[0])
    return out


def dxbc_blobs(data):
    i = -1
    while (i := data.find(b"DXBC", i + 1)) >= 0:
        size, = struct.unpack_from("<I", data, i + 24)
        if 32 < size < 2_000_000 and i + size <= len(data):
            yield i, data[i:i + size]


def main():
    ap = argparse.ArgumentParser()
    game = os.environ.get("ACU_GAME_DIR", "")
    ap.add_argument("--dll", default=os.path.join(game, "GFSDK_SSAO.win64.dll"))
    ap.add_argument("--fh", default=os.path.join(ROOT, "..", "ForHonorRustRewrite", "local-extracted", "hbao"))
    ap.add_argument("--out", default=os.path.join(ROOT, "local", "hbao", "asm"))
    a = ap.parse_args()
    if not os.path.isfile(a.dll):
        sys.exit(f"unavailable: {a.dll} (set ACU_GAME_DIR or pass --dll)")
    fh = {}
    if os.path.isdir(a.fh):
        for n in sorted(os.listdir(a.fh)):
            if n.endswith(".asm"):
                with open(os.path.join(a.fh, n), encoding="utf-8", errors="replace") as f:
                    fh[n] = opcodes(f.read())
    os.makedirs(a.out, exist_ok=True)
    with open(a.dll, "rb") as f:
        data = f.read()
    for off, blob in dxbc_blobs(data):
        text = disassemble(blob)
        if not text:
            continue
        with open(os.path.join(a.out, f"{off:07x}.asm"), "w") as f:
            f.write(text)
        ops = opcodes(text)
        stage = next((o for o in ops if o[:3] in ("ps_", "vs_", "gs_", "cs_")), "?")
        if fh:
            name, other = max(fh.items(), key=lambda kv: difflib.SequenceMatcher(None, ops, kv[1], autojunk=False).ratio())
            r = difflib.SequenceMatcher(None, ops, other, autojunk=False).ratio()
            print(f"{off:#09x} {stage:7s} {len(ops):4d} ops  FH {name} ({len(other)} ops) opcode similarity {r:.3f}")
        else:
            print(f"{off:#09x} {stage:7s} {len(ops):4d} ops  (no For Honor dumps found)")


if __name__ == "__main__":
    main()
