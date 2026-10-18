#!/usr/bin/env python3
"""Create an auditable source-only ZIP under local/; never publish or invoke Git."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import zipfile


ROOT = Path(__file__).resolve().parents[1]
ROOT_FILES = (
    ".gitignore", "AGENTS.md", "MASTER_PROMPT.md", "README.md", "PROGRESS.md",
    "CONTRIBUTING.md", "config/project.example.json", "acu-rs/README.md",
    "acu-rs/Cargo.toml", "acu-rs/Cargo.lock", "acu-rs/rust-toolchain.toml",
)
SKIP_DIRS = {"target", ".git", "__pycache__", "local", "outputs", "assets"}


def source_files(root):
    """Allow only authored document/tool/Rust locations, independent of Git ignores."""
    selected = [root / name for name in ROOT_FILES]
    for folder, extensions in (("docs", {".md"}), ("tools", {".py", ".ps1"}),
                               ("acu-rs/crates", {".rs", ".toml"})):
        base = root / folder
        if base.is_symlink() or base.is_junction():
            raise ValueError(f"linked source directory: {base}")
        for current, dirs, files in os.walk(base, followlinks=False):
            for name in dirs:
                child = Path(current) / name
                if child.is_symlink() or child.is_junction():
                    raise ValueError(f"linked source directory: {child}")
            dirs[:] = [name for name in dirs if name not in SKIP_DIRS]
            selected.extend(Path(current) / name for name in files
                            if Path(name).suffix in extensions)
    selected.append(root / "docs/templates/CAPTURE.json")
    for path in sorted(selected):
        # Check each component so even explicit files cannot traverse a junction.
        relative = path.relative_to(root)
        current = root
        for part in relative.parts:
            current = current / part
            if current.is_symlink() or current.is_junction():
                raise ValueError(f"linked source path: {current}")
        if not path.is_file():
            raise ValueError(f"missing source file: {path}")
        if path.stat().st_size > 2 * 1024 * 1024:
            raise ValueError(f"source file exceeds 2 MiB review limit: {path}")
        yield path


def bundle(root, output):
    output = output.resolve()
    if not output.is_relative_to((root / "local").resolve()) or output.suffix != ".zip":
        raise ValueError("output must be a .zip under this project's local/ directory")
    files = list(source_files(root))
    manifest = {"schema_version": 1, "files": []}
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, "x", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in files:
            data = path.read_bytes()
            name = path.relative_to(root).as_posix()
            archive.writestr(name, data)
            manifest["files"].append({"path": name, "size_bytes": len(data),
                                      "sha256": hashlib.sha256(data).hexdigest()})
        archive.writestr("SOURCE_MANIFEST.json", json.dumps(manifest, indent=2) + "\n")
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    output = args.output if args.output.is_absolute() else ROOT / args.output
    try:
        manifest = bundle(ROOT, output)
    except (OSError, ValueError) as exc:
        parser.exit(1, f"ERROR: {exc}\n")
    print(f"Source bundle: {output} ({len(manifest['files'])} files; manifest included)")
    print("Local review artifact only. No repository created or files uploaded.")


if __name__ == "__main__":
    main()
