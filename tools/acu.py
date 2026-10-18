#!/usr/bin/env python3
"""ACU preparation tools. Standard library only; never execute game code."""

from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
REQUIRED = (
    "AGENTS.md", "MASTER_PROMPT.md", "README.md", ".gitignore",
    "config/project.example.json", "tools/acu.py", "docs/STATUS.md",
    "docs/WORKFLOW.md", "docs/ROADMAP.md", "docs/ARCHITECTURE.md",
    "docs/FORGE_REUSE.md", "docs/VALIDATION.md", "docs/TASKS.md",
    "docs/FINDINGS.md", "docs/DECISIONS.md", "docs/templates/TASK.md",
    "docs/templates/FINDING.md", "docs/templates/DECISION.md",
    "docs/templates/CAPTURE.json",
    "CONTRIBUTING.md", "tools/source_bundle.py",
)
PARSER_SOURCES = (
    "fh-rs/crates/fh_assets/src/forge.rs",
    "fh-rs/crates/fh_assets/src/xxtea.rs",
    "fh-rs/crates/fh_assets/src/oodle.rs",
    "fh-rs/crates/fh_assets/src/index.rs",
    "fh-rs/crates/fh_assets/src/install.rs",
    "fh-rs/crates/fh_assets/Cargo.toml",
    "fh-rs/crates/fh_assets/tests/forge_install.rs",
    "scripts/forge_extract.py", "scripts/forge_crypt.py",
)


def timestamp():
    return datetime.now(timezone.utc).isoformat()


def config_document(path):
    doc = json.loads(path.read_text(encoding="utf-8-sig"))
    if not isinstance(doc, dict) or doc.get("schema_version") != 1:
        raise ValueError(f"{path}: expected configuration schema_version 1")
    for key in ("game_dir", "decompiled_dir"):
        if not isinstance(doc.get(key), str) or not doc[key].strip():
            raise ValueError(f"{path}: {key} must be a nonempty path string")
    reference = doc.get("for_honor_project")
    if reference is not None and (not isinstance(reference, str) or not reference.strip()):
        raise ValueError(f"{path}: for_honor_project must be null or a nonempty path string")
    return doc


def settings():
    path = ROOT / "local/project.json"
    if not path.exists():
        path = ROOT / "config/project.example.json"
    doc = config_document(path)
    paths = {}
    for key in ("game_dir", "decompiled_dir", "for_honor_project"):
        value = os.environ.get({"game_dir": "ACU_GAME_DIR",
                                "decompiled_dir": "ACU_DECOMPILED_DIR",
                                "for_honor_project": "ACU_FOR_HONOR_PROJECT"}[key], doc.get(key))
        if value is None:
            paths[key] = None
            continue
        if not value.strip():
            raise ValueError(f"{key}: path must not be empty")
        p = Path(value).expanduser()
        paths[key] = (p if p.is_absolute() else ROOT / p).resolve()
    return path, paths


def configure(args):
    """Create machine configuration without replacing an existing file."""
    doc = {"schema_version": 1, "game_dir": args.game_dir,
           "decompiled_dir": args.decompiled_dir,
           "for_honor_project": args.for_honor_project}
    for key in ("game_dir", "decompiled_dir"):
        if not doc[key].strip():
            raise ValueError(f"{key}: path must not be empty")
    if args.for_honor_project is not None and not args.for_honor_project.strip():
        raise ValueError("for_honor_project: path must not be empty")
    path = ROOT / "local/project.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(doc, stream, indent=2)
        stream.write("\n")
    print(f"Created: {path}")
    return 0


def print_paths(args):
    _, paths = settings()
    if args.key:
        if paths[args.key] is None:
            raise ValueError(f"{args.key}: not configured")
        print(paths[args.key])
    else:
        print(json.dumps({key: str(p) if p is not None else None
                          for key, p in paths.items()}, indent=2))
    return 0


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def identity(path, hashed=False):
    result = {"path": str(path), "size_bytes": path.stat().st_size}
    if hashed:
        result["sha256"] = sha256(path)
    return result


def pe_identity(path):
    result = identity(path, hashed=True)
    with path.open("rb") as stream:
        dos = stream.read(64)
        if len(dos) < 64 or dos[:2] != b"MZ":
            raise ValueError(f"{path}: not an MZ executable")
        pe_at = struct.unpack_from("<I", dos, 60)[0]
        stream.seek(pe_at)
        header = stream.read(24)
        if len(header) != 24 or header[:4] != b"PE\0\0":
            raise ValueError(f"{path}: invalid PE header")
        machine, sections, stamp, _, _, optional_size, _ = struct.unpack_from(
            "<HHIIIHH", header, 4
        )
        optional = stream.read(optional_size)
        if len(optional) < 60:
            raise ValueError(f"{path}: truncated PE optional header")
        magic = struct.unpack_from("<H", optional)[0]
        if magic not in (0x10B, 0x20B):
            raise ValueError(f"{path}: unsupported PE optional header {magic:#x}")
        base = struct.unpack_from("<Q" if magic == 0x20B else "<I",
                                  optional, 24 if magic == 0x20B else 28)[0]
        result["pe"] = {
            "machine": f"0x{machine:04x}", "sections": sections,
            "coff_timestamp_raw": stamp, "image_base": f"0x{base:x}",
            "entry_rva": f"0x{struct.unpack_from('<I', optional, 16)[0]:x}",
            "image_size_bytes": struct.unpack_from("<I", optional, 56)[0],
        }
    return result


def command_result(command):
    try:
        proc = subprocess.run(command, cwd=ROOT, capture_output=True, text=True,
                              errors="replace", timeout=10, check=False)
        lines = (proc.stdout or proc.stderr).strip().splitlines()
        return {"exit_code": proc.returncode,
                "first_line": lines[0] if lines else ""}
    except (OSError, subprocess.TimeoutExpired) as exc:
        return {"exit_code": None, "error": str(exc)}


def git_root():
    if not shutil.which("git"):
        return None
    result = command_result(["git", "rev-parse", "--show-toplevel"])
    return result.get("first_line") if result["exit_code"] == 0 else None


def write_report(output, report):
    # Generated reports may only replace JSON under this project's local/outputs.
    path = Path(output)
    path = (path if path.is_absolute() else ROOT / path).resolve()
    allowed = any(path.is_relative_to(ROOT / name) for name in ("local", "outputs"))
    if not allowed or path.suffix.lower() != ".json":
        raise ValueError("report output must be a .json file under local/ or outputs/")
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", newline="\n",
                                         dir=path.parent, delete=False) as stream:
            temporary = Path(stream.name)
            json.dump(report, stream, indent=2, ensure_ascii=False)
            stream.write("\n")
        temporary.replace(path)
    finally:
        if temporary is not None and temporary.exists():
            temporary.unlink()
    print(f"Report: {path}")


def doctor(args):
    cfg, paths = settings()
    print(f"Project: {ROOT}\nConfig: {cfg}")
    checks = {
        "game executable": (paths["game_dir"] / "ACU.exe").is_file(),
        "decompiled directory": paths["decompiled_dir"].is_dir(),
    }
    for label, ok in checks.items():
        print(f"{'OK' if ok else 'MISSING'}: {label}")
    reference = paths["for_honor_project"]
    parser_available = reference is not None and (reference / PARSER_SOURCES[0]).is_file()
    print(f"{'OK' if parser_available else 'OPTIONAL MISSING'}: reference Forge reader")
    versions = {}
    for name in ("python", "git", "rg", "cargo", "rustc"):
        executable = sys.executable if name == "python" else shutil.which(name)
        result = command_result([executable, "--version"]) if executable else {
            "exit_code": None, "error": "not on PATH"}
        versions[name] = result
        print(f"{name}: {result.get('first_line', result.get('error'))}")
    parent = git_root()
    print(f"Git root: {parent or 'unavailable'}")
    print("Doctor does not validate Rust/Bevy compatibility, the linker, GPU or binary-analysis providers; see docs/STATUS.md for completed build checks.")
    report = {"schema_version": 1, "generated_utc": timestamp(),
              "project_root": str(ROOT), "configuration": str(cfg),
              "paths": {key: str(p) if p is not None else None for key, p in paths.items()},
              "required_checks": checks, "reference_parser_available": parser_available,
              "tools": versions, "git_root": parent}
    if args.output:
        write_report(args.output, report)
    return 0 if all(checks.values()) else 1


def corpus_summary(path):
    # Metadata-only pass; no 217k-file content scan and no inferred subsystem names.
    with os.scandir(path) as entries:
        files = sorted((entry.name, entry.stat().st_size)
                       for entry in entries if entry.is_file())
    digest = hashlib.sha256()
    for name, size in files:
        digest.update(f"{name}\0{size}\n".encode("utf-8"))
    samples = []
    for name, _ in files[:3]:
        with (path / name).open("r", encoding="utf-8", errors="replace") as stream:
            header = stream.readline(300).rstrip()
        samples.append({"name": name, "first_line": header})
    return {"path": str(path), "file_count": len(files),
            "total_bytes": sum(size for _, size in files),
            "extensions": dict(sorted(Counter(Path(n).suffix for n, _ in files).items())),
            "filename_size_metadata_sha256": digest.hexdigest(),
            "digest_limitation": "filenames and sizes only; NOT a content/build identity",
            "samples": samples}


def inventory(args):
    cfg, paths = settings()
    game = paths["game_dir"]
    executable = game / "ACU.exe"
    if not executable.is_file() or not paths["decompiled_dir"].is_dir():
        raise ValueError("game executable or corpus missing; run doctor and fix paths")
    archives, dlls = [], []
    for path in sorted(game.iterdir()):
        if not path.is_file():
            continue
        if path.suffix.lower() == ".forge":
            item = identity(path)
            with path.open("rb") as stream:
                item["prefix_32_hex"] = stream.read(32).hex()
            item["identity_limitation"] = "size and prefix only; archive not fully hashed"
            archives.append(item)
        elif path.suffix.lower() == ".dll":
            dlls.append(identity(path, hashed=True))
    extracted = game / "Extracted"
    sources = []
    for relative in PARSER_SOURCES:
        if paths["for_honor_project"] is None:
            break
        path = paths["for_honor_project"] / relative
        sources.append(identity(path, hashed=True) if path.is_file() else {
            "path": str(path), "missing": True})
    report = {
        "schema_version": 1, "generated_utc": timestamp(),
        "project_root": str(ROOT), "configuration": str(cfg), "git_root": git_root(),
        "executable": pe_identity(executable),
        "corpus": corpus_summary(paths["decompiled_dir"]),
        "archives": archives, "top_level_dlls": dlls,
        "extracted_top_level": sorted(p.name for p in extracted.iterdir())
                               if extracted.is_dir() else [],
        "for_honor_reference_sources": sources,
        "for_honor_reference_configured": paths["for_honor_project"] is not None,
        "limitations": ["No archive decode, DLL load or game launch",
                        "No export/executable correspondence established",
                        "DLL presence does not prove active runtime configuration",
                        "ACUFixes nested plugin contents/configuration not inventoried"],
    }
    write_report(args.output, report)
    print(f"Executable SHA-256: {report['executable']['sha256']}")
    print(f"Corpus: {report['corpus']['file_count']:,} files; "
          f"{report['corpus']['total_bytes']:,} bytes")
    print(f"Archives: {len(archives)}; top-level DLLs: {len(dlls)}")
    return 0


def check(_args):
    errors = []
    for relative in REQUIRED:
        path = ROOT / relative
        if not path.is_file():
            errors.append(f"missing: {relative}")
            continue
        try:
            if path.suffix == ".json":
                json.loads(path.read_text(encoding="utf-8-sig"))
            if path.suffix == ".md":
                for match in re.finditer(r"\[[^\]\n]+\]\(([^)\n]+)\)",
                                         path.read_text(encoding="utf-8")):
                    target = match.group(1).strip().strip("<>").split("#", 1)[0]
                    if not target or re.match(r"[a-zA-Z][\w+.-]*:", target):
                        continue
                    if not (path.parent / target).exists():
                        errors.append(f"{relative}: broken local link: {target}")
        except (OSError, ValueError) as exc:
            errors.append(f"{relative}: {exc}")
    try:
        config_document(ROOT / "config/project.example.json")
        settings()
    except (OSError, ValueError) as exc:
        errors.append(str(exc))
    for error in errors:
        print(f"ERROR: {error}")
    print(f"Workflow check: {'FAIL' if errors else 'PASS'} ({len(REQUIRED)} required files)")
    return 1 if errors else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("configure", help="Create ignored local paths; refuses overwrite")
    p.add_argument("--game-dir", required=True)
    p.add_argument("--decompiled-dir", required=True)
    p.add_argument("--for-honor-project", help="Optional read-only reuse reference")
    p.set_defaults(run=configure)
    p = sub.add_parser("paths", help="Print resolved configuration paths without inspecting data")
    p.add_argument("--key", choices=("game_dir", "decompiled_dir", "for_honor_project"))
    p.set_defaults(run=print_paths)
    p = sub.add_parser("doctor", help="Check paths and report installed tools")
    p.add_argument("--output", help="Optional report under local/ or outputs/")
    p.set_defaults(run=doctor)
    p = sub.add_parser("inventory", help="Read local identities; do not decode or execute")
    p.add_argument("--output", default="local/inventory.json")
    p.set_defaults(run=inventory)
    p = sub.add_parser("check", help="Validate workflow files, JSON and Markdown links")
    p.set_defaults(run=check)
    args = parser.parse_args()
    try:
        return args.run(args)
    except (OSError, ValueError, struct.error) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
