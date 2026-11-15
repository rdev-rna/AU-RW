#!/usr/bin/env python3
"""Census of ACU networking/co-op strings and reflected classes (static, offline).

Reads a captured mapped main image (the same one the symbol index uses) and,
optionally, local/symbol_index.sqlite. Writes a deterministic JSON catalogue that
sorts every hit into one of the layers in docs/COOP_RE_PLAN.md:

  game      - "zenitar" NetProtocol replication types/messages (the part to port)
  transport - Storm / Echo peer-to-peer session, replica and NAT-punch layer
  backend   - Ubisoft online services (Quazal RendezVous, Hermes, ubiservices,
              Uplay). Catalogued only as a boundary; never decoded (plan, rule B1).
  os        - Winsock imports/strings

String locations are leads, not proof of behaviour (AGENTS.md). Nothing here reads
the pseudocode corpus or modifies any input.

Example:
  python tools/net_census.py --image <main-image.mapped.bin> \
      --expect-sha256 9a1124f1... --symbols local/symbol_index.sqlite \
      --output local/coop/net_census.json
"""
from __future__ import annotations

import argparse
import collections
import hashlib
import json
import re
import sqlite3
import sys
from pathlib import Path

IMAGE_BASE = 0x140000000
MAX_IMAGE = 512 * 1024 * 1024
STRING_RE = re.compile(rb"[\x20-\x7e]{5,200}")

# Layer patterns. Order matters: the first matching layer wins.
LAYERS: list[tuple[str, re.Pattern[str]]] = [
    ("backend", re.compile(r"(?i)quazal|rendez-?vous|[\\/]rdv[\\/]|\bRDV\b|ubiservices?|\bHermes|uplay|upc_|Commerce|Leaderboard")),
    ("transport", re.compile(r"storm::|\bStorm |StormNetwork|NetSession|PeerPacket")),
    ("game", re.compile(r"_NetProtocol|NetData\b|Replica\b|^zenitar_")),
    ("os", re.compile(r"^(WSA\w*|sendto|recvfrom|getaddrinfo|WS2_32\.dll|IPHLPAPI\.DLL)$")),
]

CLASS_RE = re.compile(
    r"Net|Replic|Session|Coop|Hermes|Ghost|Peer|Remote|Matchmak|Synced|JIP|"
    r"Proxy|Snapshot|Lobby|Online|Party|HostMigr|Desynch"
)


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def classify(s: str) -> str | None:
    for name, pat in LAYERS:
        if pat.search(s):
            return name
    return None


def game_summary(strings: list[tuple[int, str]]) -> dict:
    proto = {s for _, s in strings if s.endswith("_NetProtocol") and not s.startswith(("zenitar", "_"))}
    msgs = sorted(s for s in proto if "MSG" in s or "RFC" in s)
    data = sorted(proto - set(msgs))
    unicast = sorted({s for _, s in strings if s.endswith("_NetProtocol_ID_UNICAST")})
    zen = sorted({s for _, s in strings if s.startswith("zenitar_") and s.endswith("_NetProtocol_ID")})
    owners = collections.Counter(m.split("_")[0] for m in msgs)
    return {
        "protocol_types": len(proto),
        "message_types": len(msgs),
        "data_types": len(data),
        "unicast_ids": len(unicast),
        "zenitar_ids": len(zen),
        "top_message_owners": owners.most_common(40),
        "data_types_list": data,
        "message_types_list": msgs,
    }


def transport_summary(strings: list[tuple[int, str]]) -> dict:
    storm = sorted({s for _, s in strings if s.startswith("storm::")})
    ns = collections.Counter("::".join(s.split("::")[:3]) for s in storm)
    return {"storm_strings": len(storm), "namespaces": sorted(ns.items(), key=lambda kv: (-kv[1], kv[0]))}


def classes(db: Path) -> list[dict]:
    con = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    rows = con.execute("SELECT name, object_size, parent_name, ci_va FROM class ORDER BY name").fetchall()
    return [
        {"name": n, "size": f"0x{sz or 0:X}", "parent": p, "class_info_va": f"0x{va:X}"}
        for n, sz, p, va in rows
        if n and CLASS_RE.search(n)
    ]


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--image", type=Path, required=True)
    ap.add_argument("--expect-sha256")
    ap.add_argument("--symbols", type=Path, help="local/symbol_index.sqlite (optional)")
    ap.add_argument("--output", type=Path, required=True)
    a = ap.parse_args(argv)

    size = a.image.stat().st_size
    if size > MAX_IMAGE:
        print(f"error: image larger than {MAX_IMAGE} bytes", file=sys.stderr)
        return 2
    digest = sha256(a.image)
    if a.expect_sha256 and digest != a.expect_sha256.lower():
        print(f"error: image sha256 {digest} != expected {a.expect_sha256}", file=sys.stderr)
        return 2

    data = a.image.read_bytes()
    strings = [(m.start(), m.group().decode("ascii")) for m in STRING_RE.finditer(data)]
    hits: dict[str, list[dict]] = collections.defaultdict(list)
    for off, s in strings:
        layer = classify(s)
        if layer:
            hits[layer].append({"va": f"0x{IMAGE_BASE + off:X}", "s": s})

    out = {
        "schema": 1,
        "image": {"sha256": digest, "size": size, "image_base": f"0x{IMAGE_BASE:X}"},
        "tool_sha256": sha256(Path(__file__)),
        "note": "String/class locations are leads, not behaviour. Backend layer is a boundary register only.",
        "counts": {k: len(v) for k, v in sorted(hits.items())},
        "game": game_summary(strings),
        "transport": transport_summary(strings),
        "backend_samples": hits.get("backend", [])[:400],
        "os": hits.get("os", []),
    }
    if a.symbols:
        if not a.symbols.exists():
            print(f"error: {a.symbols} not found", file=sys.stderr)
            return 2
        cl = classes(a.symbols)
        out["reflected_classes"] = {"count": len(cl), "list": cl}
    else:
        out["reflected_classes"] = None  # unavailable, not zero

    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(out, indent=1, sort_keys=False) + "\n", encoding="utf-8")
    g = out["game"]
    print(f"image {digest[:12]}  layers {out['counts']}")
    print(f"game: {g['protocol_types']} NetProtocol types ({g['message_types']} messages, {g['data_types']} data)")
    print(f"transport: {out['transport']['storm_strings']} storm:: strings")
    if out["reflected_classes"]:
        print(f"reflected net/coop classes: {out['reflected_classes']['count']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
