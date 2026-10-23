#!/usr/bin/env python3
"""Add type labels, the baseline flag and loose-file inventory to the asset index (#132).

    python tools/annotate_asset_db.py INDEX.sqlite --game-dir INSTALL --scan SCAN
        [--override SCAN]... [--names identifiers.txt]... [--no-hash] [--replace]

Adds three things, each with its evidence stated:
  type_label  one row per resource type: a label, a category and HOW the label is known.
              observed-decode  the Rust parser decoded resources of this type into a family
              crc-name         a class name whose CRC-32 equals the type ID (a hash match)
              unknown          neither
  loose_file  install files outside the Forge archives (audio, video, localization, ...),
              with size and SHA-256, excluding the mod layer, backups and logs
  archive.baseline  1 for every archive the baseline rules accept

Categories are an implementation choice for browsing, not something the game records.
Standard library only; the install is read, never modified.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import sys
import zlib

import baseline as baseline_rules
import build_asset_db as bdb

# Resource keys the Rust scan adds only after its own typed decode of that family succeeded.
DECODE_KEYS = {"texture": "TextureMap", "bone_count": "Skeleton", "mesh_unresolved": "Mesh"}

# Class names recorded in the repo's findings (ACU-RE-0002, 0016, 0034, 0035, ANIMATION_DATABASE).
# They are only candidates: a label is claimed solely when the CRC-32 equals a type ID present.
KNOWN_NAMES = (
    "Material TextureSet Mask Mask16 TextureSelector UVTransform LODSelector Entity MeshShape "
    "ShadowRigidBody EntityGroup CollisionMaterial Animation ReferenceListShape GridCellDataBlock "
    "OmniLight NavMeshManager WorldTransitionPortal DistrictRegionCellData GenericRegionCellData "
    "RegionLayout GILoader WorldAmbiance LayeredSky Climate AmbianceZone BuildTable BulkMesh "
    "HairMesh AtomGraph StreamableAnimationDataBlock TimeOfDayPropertyControllerData "
    "ContactTable FloorHeightData CrowdDensityGrid MinimapTextures Mesh Skeleton TextureMap "
    "AnimTrackData AnimTrackDataMapping AnimTrackEvent"
).split()

CATEGORIES = {
    "texture": "TextureMap TextureSet Mask Mask16 TextureSelector UVTransform MinimapTextures",
    "material": "Material",
    "geometry": "Mesh MeshShape ReferenceListShape BulkMesh HairMesh LODSelector",
    "rig": "Skeleton",
    "animation": "Animation AtomGraph StreamableAnimationDataBlock AnimTrackData "
                 "AnimTrackDataMapping AnimTrackEvent",
    "world": "Entity EntityGroup GridCellDataBlock DistrictRegionCellData GenericRegionCellData "
             "RegionLayout WorldTransitionPortal FloorHeightData CrowdDensityGrid BuildTable",
    "lighting": "GILoader WorldAmbiance LayeredSky Climate OmniLight AmbianceZone "
                "TimeOfDayPropertyControllerData",
    "physics": "CollisionMaterial ShadowRigidBody ContactTable",
    "navigation": "NavMeshManager",
}
CATEGORY_OF = {name: cat for cat, names in CATEGORIES.items() for name in names.split()}

LOOSE_CATEGORIES = {
    ".pck": "audio", ".bnk": "audio", ".wem": "audio", ".bk2": "video", ".lang": "localization",
    ".map": "input", ".dll": "code", ".exe": "code", ".ini": "config", ".json": "config",
    ".manifest": "install", ".state": "install", ".id": "audio",
}

# Reverse-engineering tool databases sit beside the executable but are not game files.
TOOL_SUFFIXES = {".i64", ".idb", ".id0", ".id1", ".id2", ".nam", ".til"}

SCHEMA = (
    """CREATE TABLE type_label (
    type_id TEXT PRIMARY KEY, label TEXT, category TEXT NOT NULL, confidence TEXT NOT NULL,
    basis TEXT NOT NULL, rows INTEGER NOT NULL, distinct_ids INTEGER NOT NULL,
    named_rows INTEGER NOT NULL, decoded_rows INTEGER NOT NULL, sample_names TEXT NOT NULL)""",
    """CREATE TABLE loose_file (
    path TEXT PRIMARY KEY, size_bytes INTEGER NOT NULL, sha256 TEXT, category TEXT NOT NULL)""",
)


class AnnotateError(ValueError):
    pass


def crc_hex(name):
    return f"{zlib.crc32(name.encode('utf-8')):08x}"


def read_identifiers(path):
    """One identifier per line; surrounding double quotes are tolerated (rg -o output)."""
    names = set()
    with open(path, encoding="utf-8", errors="replace") as stream:
        for line in stream:
            name = line.strip().strip('"')
            if name:
                names.add(name)
    return names


def observe_decodes(scan, overrides, game_dir):
    """{type_id: {family: resources}} for resources the Rust parser decoded into a family."""
    seen = {}
    for _, row in bdb.merged_rows(scan, list(overrides), game_dir):
        for res in row.get("resources") or ():
            for key, family in DECODE_KEYS.items():
                if key in res:
                    fam = seen.setdefault(res.get("type_id"), {})
                    fam[family] = fam.get(family, 0) + 1
    return seen


def label_types(rows, names, decodes):
    """rows: (type_id, count, distinct, named, [samples]). Returns type_label tuples."""
    by_hash = {}
    for name in sorted(names):
        by_hash.setdefault(crc_hex(name), []).append(name)
    out = []
    for type_id, count, distinct, named, samples in rows:
        decoded = decodes.get(type_id, {})
        matches = by_hash.get(type_id, [])
        label = confidence = basis = None
        if decoded:
            family, n = max(decoded.items(), key=lambda kv: kv[1])
            label, confidence = family, "observed-decode"
            basis = f"Rust parser decoded {n:,} resources of this type as {family}"
            if len(decoded) > 1:
                basis += f" (also {sorted(decoded)})"
            if matches:
                basis += ("; CRC-32 name agrees: " if family in matches else
                          "; CRC-32 names differ: ") + ", ".join(matches[:3])
        elif matches:
            label = matches[0] if len(matches) == 1 else " | ".join(matches[:4])
            confidence = "crc-name" if len(matches) == 1 else "crc-ambiguous"
            basis = (f"CRC-32 of '{label}' equals the type ID "
                     f"({len(matches)} candidate name{'s' if len(matches) > 1 else ''})")
        else:
            confidence, basis = "unknown", "no decoded family and no class name hashes to this ID"
        primary = label.split(" | ")[0] if label else None
        out.append((type_id, label, CATEGORY_OF.get(primary, "uncategorised"), confidence, basis,
                    count, distinct, named, sum(decoded.values()), json.dumps(samples)))
    return out


def file_sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def loose_files(game_dir, hash_files=True):
    """Install files outside the Forge archives, minus anything the baseline rules exclude."""
    game = Path(game_dir)
    report = baseline_rules.classify(game)
    excluded = {e["path"] for e in report["excluded"]}
    rows = []
    for current, dirs, files in os.walk(game, followlinks=False):
        here = Path(current)
        rel_dir = here.relative_to(game).as_posix()
        dirs[:] = sorted(d for d in dirs
                         if (f"{rel_dir}/{d}" if rel_dir != "." else d) not in excluded
                         and not (here / d).is_symlink())
        for name in sorted(files):
            rel = f"{rel_dir}/{name}" if rel_dir != "." else name
            if (rel in excluded or name.endswith(".forge") or (here / name).is_symlink()
                    or Path(name).suffix.lower() in TOOL_SUFFIXES):
                continue
            path = here / name
            category = LOOSE_CATEGORIES.get(path.suffix.lower(), "other")
            rows.append((rel, path.stat().st_size, file_sha256(path) if hash_files else None,
                         category))
    return rows


def annotate(db_path, game_dir, scan, overrides=(), names=(), hash_files=True, replace=False):
    """Compute everything first, then write it in one transaction, so a failure changes nothing."""
    db = sqlite3.connect(db_path)
    try:
        have = db.execute("SELECT COUNT(*) FROM sqlite_master WHERE name IN "
                          "('type_label','loose_file')").fetchone()[0]
        if have and not replace:
            raise FileExistsError("type_label/loose_file exist; pass --replace")
        decodes = observe_decodes(scan, overrides, game_dir)
        candidates = set(KNOWN_NAMES)
        for path in names:
            candidates |= read_identifiers(path)
        type_rows = []
        for type_id, count, distinct, named in db.execute(
                """SELECT type_id, COUNT(*), COUNT(DISTINCT file_id), COUNT(name)
                   FROM resource GROUP BY type_id""").fetchall():
            samples = [r[0] for r in db.execute(
                "SELECT DISTINCT name FROM resource WHERE type_id = ? AND name IS NOT NULL "
                "LIMIT 3", (type_id,))]
            type_rows.append((type_id, count, distinct, named, samples))
        labelled = label_types(type_rows, candidates, decodes)
        loose = loose_files(game_dir, hash_files)
        carriers = {c["path"] for c in baseline_rules.classify(game_dir)["carriers"]}
        archive_names = [n for (n,) in db.execute("SELECT name FROM archive").fetchall()]
        stats = {"annotate_types": len(labelled), "annotate_loose_files": len(loose),
                 "annotate_candidate_names": len(candidates)}
        with db:  # one transaction: DDL via execute() does not commit implicitly
            db.execute("DROP TABLE IF EXISTS type_label")
            db.execute("DROP TABLE IF EXISTS loose_file")
            for statement in SCHEMA:
                db.execute(statement)
            db.executemany("INSERT INTO type_label VALUES (?,?,?,?,?,?,?,?,?,?)", labelled)
            # Convenience for plain SQL (WHERE type_name = 'Material'): only confident labels.
            db.execute("UPDATE resource SET type_name = NULL")
            db.execute("""UPDATE resource SET type_name = (
                SELECT label FROM type_label t WHERE t.type_id = resource.type_id
                  AND t.confidence IN ('observed-decode', 'crc-name'))""")
            db.executemany("INSERT INTO loose_file VALUES (?,?,?,?)", loose)
            db.executemany("UPDATE archive SET baseline = ? WHERE name = ?",
                           [(int(name in carriers), name) for name in archive_names])
            db.execute("DELETE FROM meta WHERE key LIKE 'annotate_%'")
            db.executemany("INSERT INTO meta VALUES (?,?)",
                           [(k, str(v)) for k, v in stats.items()])
        by_conf = dict(db.execute(
            "SELECT confidence, COUNT(*) FROM type_label GROUP BY confidence").fetchall())
        return stats | {"by_confidence": by_conf}
    finally:
        db.close()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("index")
    parser.add_argument("--game-dir", required=True)
    parser.add_argument("--scan", required=True, help="acu-scan directory or entries.jsonl")
    parser.add_argument("--override", action="append", default=[])
    parser.add_argument("--names", action="append", default=[], help="identifier list file(s)")
    parser.add_argument("--no-hash", action="store_true", help="skip SHA-256 of loose files")
    parser.add_argument("--replace", action="store_true")
    args = parser.parse_args(argv)

    def entries(p):
        p = Path(p)
        return p / "entries.jsonl" if p.is_dir() else p
    try:
        print(json.dumps(annotate(
            args.index, args.game_dir, entries(args.scan), [entries(o) for o in args.override],
            args.names, not args.no_hash, args.replace), indent=1))
    except (AnnotateError, bdb.ScanError, FileExistsError, ValueError, OSError,
            sqlite3.Error) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
