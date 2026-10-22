#!/usr/bin/env python3
"""Package the audited local asset index for distribution (#132).

    python tools/package_asset_index.py --links-summary SUMMARY.json --game-dir INSTALL
        [--index local/asset_index.sqlite] [--out-dir local/dist] [--manifest-dir data]
        [--version 2026-10-22.1] [--repo OWNER/NAME] [--tag asset-index-2026-10-22.1]

Runs the completeness audit first and refuses to package an index with any FAIL. Then, on a
COPY (the working index is untouched): removes machine paths from `meta`, stamps the version,
VACUUMs and gzips into --out-dir (never committed), and writes data/asset_index.manifest.json
(committed: URL, checksums, counts, audit summary). The archive is then uploaded as a GitHub
Release asset under the manifest's tag; see docs/ASSET_INDEX.md for the maintainer steps.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import gzip
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess
import sys
import tempfile

import audit_asset_db
import index_status

ROOT = Path(__file__).resolve().parents[1]
PATH_LIKE = re.compile(r"([A-Za-z]:[\\/]|/Users/|/home/|\\Users\\)")


class PackageError(ValueError):
    pass


def scrub_meta(db):
    """Replace machine-specific paths in meta with neutral values; return what changed."""
    changed = {}
    for key, value in db.execute("SELECT key, value FROM meta").fetchall():
        if not PATH_LIKE.search(value):
            continue
        if key == "overrides":
            new = ",".join(Path(p.replace("\\", "/")).parent.name or "scan"
                           for p in value.split(",") if p)
            new = f"rescans: {new}"
        else:
            new = {"game_dir": "<install>", "source": "<scan>"}.get(key, "<local path removed>")
        db.execute("UPDATE meta SET value = ? WHERE key = ?", (new, key))
        changed[key] = new
    return changed


def assert_no_paths(db):
    """No text value in meta or the small tables may still look like a machine path."""
    for table, column in (("meta", "value"), ("archive", "name"), ("loose_file", "path")):
        try:
            rows = db.execute(f"SELECT {column} FROM {table}").fetchall()
        except sqlite3.Error:
            continue
        for (value,) in rows:
            if isinstance(value, str) and PATH_LIKE.search(value):
                raise PackageError(f"machine path left in {table}.{column}: {value[:60]!r}")


COUNT_KEYS = {"archive": "archives", "entry": "entries", "resource": "resources",
              "texture": "textures", "link": "links", "type_label": "types",
              "loose_file": "loose_files"}


def counts_of(db):
    out = {}
    for table, key in COUNT_KEYS.items():
        if db.execute("SELECT 1 FROM sqlite_master WHERE name = ?", (table,)).fetchone():
            out[key] = db.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
    return out


def repo_from_remote():
    """OWNER/NAME of the origin remote on github.com, or None."""
    try:
        url = subprocess.run(["git", "remote", "get-url", "origin"], capture_output=True,
                             text=True, timeout=10, cwd=ROOT).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return None
    match = re.search(r"github\.com[:/]([^/\s]+/[^/\s]+?)(?:\.git)?$", url)
    return match.group(1) if match else None


def package(index, out_dir, links_summary=None, tables=None, version=None, manifest_dir=None,
            repo=None, tag=None):
    index, out_dir = Path(index), Path(out_dir)
    manifest_dir = Path(manifest_dir) if manifest_dir else out_dir
    repo = repo or repo_from_remote()
    if not repo:
        raise PackageError("cannot work out the GitHub repo; pass --repo OWNER/NAME")
    if not index.is_file():
        raise PackageError(f"no index at {index}")
    db = sqlite3.connect(f"file:{index.as_posix()}?mode=ro", uri=True, timeout=30)
    try:
        audit = audit_asset_db.run(db, links_summary, tables)
    finally:
        db.close()
    failed = [(n, d) for s, n, d in audit.rows if s == "FAIL"]
    if failed:
        raise PackageError("audit found holes, not packaging: " +
                           "; ".join(f"{n}: {d}" for n, d in failed[:4]))
    stamp = datetime.now(timezone.utc)
    version = version or stamp.strftime("%Y-%m-%d.1")
    tag = tag or f"asset-index-{version}"
    out_dir.mkdir(parents=True, exist_ok=True)
    manifest_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=out_dir) as tmp:
        work = Path(tmp) / "asset_index.sqlite"
        shutil.copyfile(index, work)
        db = sqlite3.connect(work)
        try:
            scrubbed = scrub_meta(db)
            db.execute("DELETE FROM meta WHERE key IN ('index_version','built_utc')")
            db.executemany("INSERT INTO meta VALUES (?,?)", [
                ("index_version", version), ("built_utc", stamp.isoformat(timespec="seconds"))])
            db.commit()
            assert_no_paths(db)
            counts = counts_of(db)
            db.execute("VACUUM")
        finally:
            db.close()
        sqlite_bytes, sqlite_sha = work.stat().st_size, index_status.sha256_of(work)
        gz_tmp = Path(tmp) / "asset_index.sqlite.gz"
        with open(work, "rb") as source, gzip.GzipFile(gz_tmp, "wb", compresslevel=6,
                                                       mtime=0) as target:
            shutil.copyfileobj(source, target, 1 << 20)
        gz_sha = index_status.sha256_of(gz_tmp)
        manifest = {
            "schema_version": 1, "index_version": version,
            "built_utc": stamp.isoformat(timespec="seconds"),
            "gz_file": "asset_index.sqlite.gz", "release_tag": tag,
            "url": f"https://github.com/{repo}/releases/download/{tag}/asset_index.sqlite.gz",
            "gz_bytes": gz_tmp.stat().st_size,
            "gz_sha256": gz_sha, "sqlite_bytes": sqlite_bytes, "sqlite_sha256": sqlite_sha,
            "counts": counts,
            "audit": {"result": "no holes found",
                      "checks_passed": sum(1 for s, *_ in audit.rows if s == "PASS"),
                      "info": [f"{n}: {d}" for s, n, d in audit.rows if s == "INFO"]},
            "meta_paths_scrubbed": sorted(scrubbed),
            "contents": "metadata only: archive/entry/resource/texture/link/type_label/"
                        "loose_file tables; no game bytes",
        }
        os.replace(gz_tmp, out_dir / "asset_index.sqlite.gz")
        (manifest_dir / "asset_index.manifest.json").write_text(
            json.dumps(manifest, indent=1) + "\n", encoding="utf-8")
    return manifest


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--index", default=str(ROOT / "local" / "asset_index.sqlite"))
    parser.add_argument("--out-dir", default=str(ROOT / "local" / "dist"),
                        help="where the archive is written (not committed)")
    parser.add_argument("--manifest-dir", default=str(ROOT / "data"),
                        help="where the small manifest is written (committed)")
    parser.add_argument("--repo", help="OWNER/NAME for the release URL (default: git origin)")
    parser.add_argument("--tag", help="release tag (default: asset-index-<version>)")
    parser.add_argument("--links-summary", help="acu-links summary.json (cross-checks)")
    parser.add_argument("--game-dir", help="install dir (fresh entry-table comparison)")
    parser.add_argument("--version")
    args = parser.parse_args(argv)
    summary = None
    if args.links_summary:
        summary = json.loads(Path(args.links_summary).read_text(encoding="utf-8"))
    tables = None
    if args.game_dir:
        import forge_probe

        def tables(name):
            path = Path(args.game_dir) / name
            with path.open("rb") as stream:
                _, entries = forge_probe.tables(stream, path.stat().st_size)
            return [(e["index"], e["file_id"], e["offset"], e["stored_length"])
                    for e in entries]
    try:
        manifest = package(args.index, args.out_dir, summary, tables, args.version,
                           args.manifest_dir, args.repo, args.tag)
    except (PackageError, OSError, sqlite3.Error, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(json.dumps({k: manifest[k] for k in ("index_version", "release_tag", "url", "gz_bytes",
                                               "sqlite_bytes", "gz_sha256", "counts")}, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
