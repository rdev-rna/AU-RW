#!/usr/bin/env python3
"""Read-only survey: which network stack does an Assassin's Creed executable carry?

Counts signature strings in each given executable (raw file bytes, no execution)
and writes deterministic JSON. Used by ACU-RE-0074 (co-op plan Phase 6) to decide,
per target title, whether co-op can reuse dormant replication code or must be
reimplemented.

Stacks recognised (string signatures only; presence is a lead, not proof that the
code is reachable):
  storm_echo  - ACU's Storm/Echo P2P session + object replication
  netprotocol - ACU's game replication layer (*_NetProtocol, zenitar_, NetObject*RFC)
  hacknetz    - HackNet/HackNetZ duplicated-object replication (Net-Z lineage)
  rdv         - Quazal RendezVous online-services client (backend; plan rule B1)
  ubiservices - Ubisoft ubiservices client (backend; plan rule B1)

A high-entropy (packed) executable shows few or no strings. The report flags that
with `packed_suspect` so an empty result is never read as "absent".
"""
from __future__ import annotations

import argparse
import collections
import hashlib
import json
import math
import re
import sys
from pathlib import Path

MAX_FILE = 1024 * 1024 * 1024
PACKED_ENTROPY = 7.5

SIGNATURES: dict[str, dict[str, bytes]] = {
    "storm_echo": {
        "storm::": rb"storm::",
        "storm::echo::object": rb"storm::echo::object",
        "storm::echo::session": rb"storm::echo::session",
        "StormNetworkThread": rb"StormNetworkThread",
    },
    "netprotocol": {
        "_NetProtocol": rb"_NetProtocol",
        "zenitar_": rb"zenitar_",
        "NetObjectRemoteFunctionCall": rb"NetObjectRemoteFunctionCall",
        "NetObjectMemento": rb"NetObjectMemento",
    },
    "hacknetz": {
        "HackNetZ": rb"HackNetZ",
        "HackNetCore": rb"HackNetCore",
        "DupSpace": rb"DupSpace",
        "MasterStation": rb"MasterStation",
        "DOCore": rb"DOCore",
    },
    "rdv": {
        "RendezVous": rb"(?i)rendez-?vous",
        "Quazal": rb"(?i)quazal",
        "PRUDP": rb"PRUDP",
        "GameSessionProtocol": rb"GameSessionProtocol",
    },
    "ubiservices": {"ubiservices": rb"(?i)ubiservices"},
}
# A stack counts as present when its first (most specific) signature is found.
PRIMARY = {"storm_echo": "storm::echo::object", "netprotocol": "_NetProtocol",
           "hacknetz": "HackNetZ", "rdv": "RendezVous", "ubiservices": "ubiservices"}
GAME_NET_CLASS = re.compile(rb"^Net[A-Z][A-Za-z]{2,40}$")
STR_RE = re.compile(rb"[\x20-\x7e]{4,200}")


def entropy(buf: bytes) -> float:
    if not buf:
        return 0.0
    c = collections.Counter(buf)
    n = len(buf)
    return -sum(v / n * math.log2(v / n) for v in c.values())


def survey(path: Path) -> dict:
    size = path.stat().st_size
    if size > MAX_FILE:
        raise ValueError(f"{path}: larger than {MAX_FILE} bytes")
    b = path.read_bytes()
    if b[:2] != b"MZ":
        raise ValueError(f"{path}: not a PE executable")
    ent = round(entropy(b[0x1000:0x401000]), 3)
    counts = {stack: {k: len(re.findall(r, b)) for k, r in sigs.items()} for stack, sigs in SIGNATURES.items()}
    net_classes = sorted({m.group().decode() for m in STR_RE.finditer(b) if GAME_NET_CLASS.match(m.group())})
    packed = ent >= PACKED_ENTROPY
    present = {s: counts[s][PRIMARY[s]] > 0 for s in SIGNATURES}
    return {
        "file": path.name,
        "size": size,
        "sha256": hashlib.sha256(b).hexdigest(),
        "entropy_0x1000_4MiB": ent,
        "packed_suspect": packed,
        "present": {s: (None if packed and not v else v) for s, v in present.items()},
        "counts": counts,
        "game_net_class_names": net_classes,
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("exe", nargs="+", type=Path)
    ap.add_argument("--output", type=Path, required=True)
    a = ap.parse_args(argv)
    rows = []
    for p in a.exe:
        try:
            rows.append(survey(p))
        except (OSError, ValueError) as e:
            print(f"error: {e}", file=sys.stderr)
            return 2
    rows.sort(key=lambda r: r["file"].lower())
    out = {"schema": 1, "finding": "ACU-RE-0074", "tool_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
           "note": "String signatures only. present=null means packed and unknown, not absent.", "titles": rows}
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(out, indent=1) + "\n", encoding="utf-8")
    for r in rows:
        print(f"{r['file']:<32} entropy {r['entropy_0x1000_4MiB']:<5} packed={r['packed_suspect']!s:<5} "
              + " ".join(f"{s}={'?' if v is None else int(v)}" for s, v in r["present"].items())
              + f"  Net* classes={len(r['game_net_class_names'])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
