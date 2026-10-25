"""Baseline hygiene (#37): which parts of an install count as original game data.

Pure filesystem classification; no archive is opened or hashed. The baseline is the
set of Forge carriers the coverage denominator may use. Everything else that could be
mistaken for game data is listed with the rule that excludes it, so a coverage
number can always be traced to an explicit list.
"""

from collections import Counter
import os
from pathlib import Path

SCHEMA_VERSION = 1

# Directory names matched case-insensitively on the install's top level.
MOD_LAYER_DIRS = {"acufixes": "mod-layer"}
COPY_DIRS = {"backups": "non-original-copy", "extracted": "non-original-copy"}
# Top-level files excluded by name pattern: (rule, predicate).
MOD_LAYER_FILE_PREFIXES = ("acufixes",)
COPY_FILE_SUFFIXES = (".rar", ".zip", ".7z")
LOG_SUFFIXES = (".log",)

REASONS = {
    "mod-layer": "third-party loader/plugins and their loose overrides; not shipped game data",
    "non-original-copy": "user-made backup, extraction or archive of game data; duplicates "
                         "carriers or holds derived files",
    "runtime-log": "log written by a loader or the game; not game data",
}


def _walk_counts(path):
    """File count, byte total and extension histogram under a directory (symlinks not followed)."""
    files, size, extensions = 0, 0, Counter()
    for current, dirs, names in os.walk(path, followlinks=False):
        dirs[:] = [d for d in dirs if not os.path.islink(os.path.join(current, d))]
        for name in names:
            full = os.path.join(current, name)
            try:
                size += os.lstat(full).st_size
            except OSError:
                continue
            files += 1
            extensions[Path(name).suffix or "(none)"] += 1
    return files, size, dict(sorted(extensions.items()))


def _rule_for(name, is_dir):
    lowered = name.lower()
    if is_dir:
        if lowered in MOD_LAYER_DIRS:
            return MOD_LAYER_DIRS[lowered]
        if lowered in COPY_DIRS:
            return COPY_DIRS[lowered]
        return None
    if lowered.startswith(MOD_LAYER_FILE_PREFIXES):
        return "mod-layer"
    if lowered.endswith(COPY_FILE_SUFFIXES):
        return "non-original-copy"
    if lowered.endswith(LOG_SUFFIXES):
        return "runtime-log"
    return None


def _carriers(game):
    """Original carriers: `*.forge` regular files at the top level or in `dlc_*` folders.

    Mirrors `acu_assets::forge::discover`. A directory that merely has a `.forge` name
    (the Extracted folder holds some) is never a carrier.
    """
    found = []
    for entry in sorted(game.iterdir(), key=lambda p: p.name):
        if entry.is_symlink():
            continue
        if entry.is_file() and entry.name.endswith(".forge"):
            found.append(entry)
        elif entry.is_dir() and entry.name.startswith("dlc_"):
            found.extend(sorted(p for p in entry.iterdir()
                                if p.is_file() and not p.is_symlink()
                                and p.name.endswith(".forge")))
    return found


def classify(game):
    """Classify an install directory. Raises ValueError if it is not a directory."""
    game = Path(game)
    if not game.is_dir():
        raise ValueError(f"not a directory: {game}")
    carriers = _carriers(game)
    carrier_sizes = {}
    for path in carriers:
        carrier_sizes.setdefault(path.name, []).append(path.stat().st_size)

    excluded, other = [], []
    for entry in sorted(game.iterdir(), key=lambda p: p.name):
        if entry.is_symlink():
            other.append({"path": entry.name, "kind": "symlink", "note": "not followed"})
            continue
        is_dir = entry.is_dir()
        rule = _rule_for(entry.name, is_dir)
        if rule is None:
            if entry in carriers:
                continue
            kind = "dir" if is_dir else "file"
            if kind == "dir" and entry.name.startswith("dlc_"):
                continue
            other.append({"path": entry.name, "kind": kind})
            continue
        item = {"path": entry.name, "rule": rule, "reason": REASONS[rule],
                "kind": "dir" if is_dir else "file"}
        if is_dir and rule == "mod-layer":
            item["note"] = "excluded by name; contents not inspected"
        elif is_dir:
            item["files"], item["bytes"], item["extensions"] = _walk_counts(entry)
            nested = sorted(p.name for p in entry.iterdir() if p.name.endswith(".forge"))
            if nested:
                item["forge_named_entries"] = [
                    {"name": n, "is_dir": (entry / n).is_dir(),
                     "size": None if (entry / n).is_dir() else (entry / n).stat().st_size,
                     "same_size_as_baseline": (entry / n).is_file() and
                     (entry / n).stat().st_size in carrier_sizes.get(n, [])}
                    for n in nested]
        else:
            item["bytes"] = entry.stat().st_size
        excluded.append(item)

    return {
        "schema_version": SCHEMA_VERSION,
        "game_dir": str(game),
        "carriers": [{"path": p.relative_to(game).as_posix(), "bytes": p.stat().st_size}
                     for p in carriers],
        "carrier_count": len(carriers),
        "excluded": excluded,
        "other_top_level": other,
        "rules": {"carrier": "regular *.forge file at top level or in a dlc_* folder; "
                             "mirrors acu_assets::forge::discover",
                  "exclusion_reasons": REASONS},
        "limitations": [
            "Classification is by location and name only; no archive was opened or hashed.",
            "same_size_as_baseline compares sizes, not contents.",
            "A mod can also replace a baseline carrier in place; that is not detectable here.",
            "Mod-layer folders are excluded by name and never opened.",
        ],
    }
