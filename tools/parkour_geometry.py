#!/usr/bin/env python3
"""Parkour candidate geometry joined to decoded world collision (ACU-RE-0055).

No game process is read. Input: the raw 672-byte candidate snapshots that the parkour
selection probe (`tools/native_probe/parkour_selection.profile.json`, ACU-RE-0042) already
gathered into `local/native-probe/runs/<run>/native-session.jsonl`, and the Versailles
collision dumps decoded by `tools/world_collision.py` (ACU-RE-0050).

Candidate fields used here (offsets in the 672-byte object; identities checked on every
gathered candidate by `fields`, see ACU-RE-0055 for the counts):

  +0x08  u64   fancy table (action type through slot 0x45, ACU-RE-0042/0043)
  +0x10  f32x3 source point (the player position when standing; a hold when climbing)
  +0x20  f32x3 unit vector (player forward on the ground; not interpreted further)
  +0x30  f32x3 target point
  +0x40  f32x3 unit horizontal vector perpendicular to the target segment ("segment normal")
  +0x50  f32x3 segment end A   (target lies on A..B)
  +0x60  f32x3 segment end B
  +0x70  f32x3 unit horizontal vector (not interpreted)
  +0xC0  f32   target.z - source.z
  +0xC4  f32   horizontal |target - source|
  +0xC8  f32   hypot(+0xC0, +0xC4)  (the distance getter, ACU-RE-0042)
  +0x180, +0x190, +0x1A0  f32x3 points (often equal to the target; see the finding)
  +0x204 f32   fitness (ACU-RE-0042)

Subcommands:

  fields   field identities per action type (no collision needed)
  analyze  join every distinct target to the decoded collision; --write writes a fixture

    python tools/parkour_geometry.py fields
    python tools/parkour_geometry.py analyze --write data/oracles/parkour_geometry.json
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import hashlib
import json
import math
from pathlib import Path
import struct
import sys

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
RUNS = ROOT / "local/native-probe/runs"
ENTRY_LABEL = "parkour_select_entry"
EXIT_LABEL = "parkour_select_exit"
CANDIDATE_BYTES = 672
BINARY_SHA256 = "748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40"

# Implementation choices of this analysis (not game constants).
SCALAR_TOLERANCE = 2e-3      # m, "reproduced" for +0xC0/+0xC4/+0xC8
PROFILE_OFFSET = 0.05        # m, side columns either side of the segment
TOP_TOLERANCE = 0.10         # m, a surface this close to target z counts as the ledge top
LEDGE_MIN_DROP = 0.50        # m, the open side must fall at least this far
UP_FACE_NZ = 0.70            # |normal z| of a walkable top face (about 45 degrees)
STEEP_FACE_NZ = 0.30         # |normal z| below which a face is a wall
GROUND_STEP = 0.5            # m, support query step (ACU-RE-0050)
GROUND_TOLERANCE = 0.05      # m, source counts as standing on decoded ground
EDGE_WELD = 0.01             # m, endpoints closer than this are the same vertex
EDGE_SEARCH = 0.30           # m, triangles within this box of the target are searched
FAN_HEIGHTS = (0.75, 1.0, 1.25, 1.75)   # m above the player origin (ACU-RE-0050 live check)
FAN_LENGTH = 5.0             # m (ACU-RE-0050 live check; 2.5 m rays also seen)


# ------------------------------------------------------------- candidate fields ---

def _f3(raw: bytes, offset: int) -> tuple:
    return struct.unpack_from("<3f", raw, offset)


def decode_candidate(raw: bytes) -> dict:
    """Decode the fields listed in the module docstring; ValueError on a short or bad object."""
    if not isinstance(raw, (bytes, bytearray)):
        raise ValueError("candidate must be bytes")
    if len(raw) != CANDIDATE_BYTES:
        raise ValueError(f"candidate is {len(raw)} bytes, expected {CANDIDATE_BYTES}")
    c0, c4, c8, cc = struct.unpack_from("<4f", raw, 0xC0)
    out = {
        "table": struct.unpack_from("<Q", raw, 0x08)[0],
        "source": _f3(raw, 0x10), "dir20": _f3(raw, 0x20), "target": _f3(raw, 0x30),
        "normal": _f3(raw, 0x40), "seg_a": _f3(raw, 0x50), "seg_b": _f3(raw, 0x60),
        "dir70": _f3(raw, 0x70), "c0": c0, "c4": c4, "c8": c8, "cc": cc,
        "p180": _f3(raw, 0x180), "p190": _f3(raw, 0x190), "p1a0": _f3(raw, 0x1A0),
        "fitness": struct.unpack_from("<f", raw, 0x204)[0],
    }
    for key in ("source", "target", "normal", "seg_a", "seg_b"):
        if not all(math.isfinite(v) for v in out[key]):
            raise ValueError(f"non-finite {key}")
    return out


def scalar_residuals(c: dict) -> dict:
    """|+0xC0 - dz|, |+0xC4 - horizontal|, |+0xC8 - 3D distance|, |+0xC8 - hypot(C0, C4)|."""
    s, t = c["source"], c["target"]
    dz = t[2] - s[2]
    h = math.hypot(t[0] - s[0], t[1] - s[1])
    return {"c0": abs(c["c0"] - dz), "c4": abs(c["c4"] - h), "c8": abs(c["c8"] - math.dist(s, t)),
            "c8_hypot": abs(c["c8"] - math.hypot(c["c0"], c["c4"]))}


def point_segment(p, a, b) -> tuple[float, float]:
    """(distance from p to segment a..b, parameter u clamped to [0, 1])."""
    ab = [b[i] - a[i] for i in range(3)]
    ll = sum(v * v for v in ab)
    u = 0.0 if ll < 1e-12 else max(0.0, min(1.0, sum((p[i] - a[i]) * ab[i] for i in range(3)) / ll))
    q = [a[i] + u * ab[i] for i in range(3)]
    return math.dist(p, q), u


def segment_checks(c: dict) -> dict:
    a, b, t, n = c["seg_a"], c["seg_b"], c["target"], c["normal"]
    ab = [b[i] - a[i] for i in range(3)]
    length = math.sqrt(sum(v * v for v in ab))
    out = {"length": length, "normal_unit": abs(math.sqrt(sum(v * v for v in n)) - 1) < 1e-3,
           "normal_horizontal": abs(n[2]) < 1e-3, "level": abs(a[2] - b[2]) < 0.01}
    if length < 1e-4:
        out.update(degenerate=True, target_on=math.dist(a, t) < 5e-3, perpendicular=None)
    else:
        d, _ = point_segment(t, a, b)
        out.update(degenerate=False, target_on=d < 5e-3,
                   perpendicular=abs(sum(n[i] * ab[i] for i in range(3))) / length < 0.02)
    s = c["source"]
    out["normal_toward_source"] = (n[0] * (s[0] - t[0]) + n[1] * (s[1] - t[1])) > 0
    return out


def horizontal_unit(a, b):
    dx, dy = b[0] - a[0], b[1] - a[1]
    h = math.hypot(dx, dy)
    return None if h < 1e-6 else (dx / h, dy / h)


# --------------------------------------------------------------- geometry math ---

def face_normal(tri) -> np.ndarray:
    tri = np.asarray(tri, dtype=float)
    n = np.cross(tri[1] - tri[0], tri[2] - tri[0])
    length = np.linalg.norm(n)
    if length < 1e-12:
        raise ValueError("degenerate triangle")
    return n / length


def ray_triangles(origin, direction, tris: np.ndarray, eps: float = 1e-9) -> np.ndarray:
    """Möller–Trumbore against an (n, 3, 3) array; ray parameter t per triangle (inf = miss)."""
    tris = np.asarray(tris, dtype=float).reshape(-1, 3, 3)
    o, d = np.asarray(origin, float), np.asarray(direction, float)
    e1, e2 = tris[:, 1] - tris[:, 0], tris[:, 2] - tris[:, 0]
    p = np.cross(d, e2)
    det = np.einsum("ij,ij->i", e1, p)
    ok = np.abs(det) > eps
    inv = np.where(ok, 1.0 / np.where(ok, det, 1.0), 0.0)
    s = o - tris[:, 0]
    u = np.einsum("ij,ij->i", s, p) * inv
    q = np.cross(s, e1)
    v = (q @ d) * inv
    t = np.einsum("ij,ij->i", e2, q) * inv
    hit = ok & (u >= 0) & (v >= 0) & (u + v <= 1) & (t >= 0)
    return np.where(hit, t, np.inf)


def edge_distances(point, tris: np.ndarray) -> np.ndarray:
    """(n, 3) distances from point to the three edges (v0v1, v1v2, v2v0) of each triangle."""
    tris = np.asarray(tris, dtype=float).reshape(-1, 3, 3)
    p = np.asarray(point, float)
    a = tris
    b = np.roll(tris, -1, axis=1)
    ab = b - a
    ll = np.einsum("ijk,ijk->ij", ab, ab)
    u = np.einsum("ijk,ijk->ij", p - a, ab) / np.where(ll < 1e-12, 1.0, ll)
    u = np.clip(np.where(ll < 1e-12, 0.0, u), 0.0, 1.0)
    q = a + u[..., None] * ab
    return np.linalg.norm(q - p, axis=2)


def classify_edge(edge, faces) -> dict:
    """Classify a triangle edge from the faces that share it.

    `faces` are triangles (3 points each) containing both edge endpoints. The face with the
    largest |normal z| is the top; its normal is oriented up; another face whose third vertex
    lies below that plane makes the edge convex (a ledge lip), above it concave (an inside
    corner). One face: boundary. Faces closer than 1 cm to the plane: flat.
    """
    a, b = (np.asarray(p, float) for p in edge)
    if not faces:
        raise ValueError("no faces")
    normals = [face_normal(f) for f in faces]
    top = max(range(len(faces)), key=lambda i: abs(normals[i][2]))
    nt = normals[top] if normals[top][2] >= 0 else -normals[top]
    out = {"faces": len(faces), "top_nz": float(abs(nt[2])), "kind": "boundary",
           "other_nz": None}
    for i, f in enumerate(faces):
        if i == top:
            continue
        third = max((np.asarray(v, float) for v in f),
                    key=lambda v: min(np.linalg.norm(v - a), np.linalg.norm(v - b)))
        side = float(np.dot(third - a, nt))
        kind = "convex" if side < -0.01 else "concave" if side > 0.01 else "flat"
        if out["kind"] in ("boundary", "flat") or kind == "convex":
            out["kind"], out["other_nz"] = kind, float(abs(normals[i][2]))
    return out


def nearest_edge(point, tris: np.ndarray, weld: float = EDGE_WELD) -> dict | None:
    """Nearest triangle edge to `point` and the classification of the faces sharing it."""
    tris = np.asarray(tris, dtype=float).reshape(-1, 3, 3)
    if len(tris) == 0:
        return None
    dist = edge_distances(point, tris)
    i, k = np.unravel_index(int(np.argmin(dist)), dist.shape)
    a, b = tris[i, k], tris[i, (k + 1) % 3]
    # faces containing both endpoints (welded)
    da = np.linalg.norm(tris - a, axis=2).min(axis=1) < weld
    db = np.linalg.norm(tris - b, axis=2).min(axis=1) < weld
    faces = [tris[j] for j in np.nonzero(da & db)[0]]
    try:
        cls = classify_edge((a, b), faces)
    except ValueError:
        cls = {"faces": len(faces), "kind": "degenerate", "top_nz": None, "other_nz": None}
    direction = b - a
    length = float(np.linalg.norm(direction))
    return {"distance": float(dist[i, k]), "a": a.tolist(), "b": b.tolist(), "length": length,
            "direction": (direction / length).tolist() if length > 1e-9 else None, **cls}


def ledge_profile(column, target, normal, offset=PROFILE_OFFSET, top_tol=TOP_TOLERANCE) -> dict:
    """Sample the decoded surfaces either side of the target along the segment normal.

    `column(x, y)` returns surfaces [(z, |normal z|), ...] on the vertical line. For each side
    (+n, -n): `top` = surface within top_tol of target z (closest), `drop` = target z minus the
    highest surface at or below target z + top_tol (inf when none), `headroom` = lowest surface
    above target z + top_tol, minus target z.
    """
    tx, ty, tz = target
    nh = horizontal_unit((0, 0, 0), normal)
    if nh is None:
        raise ValueError("normal has no horizontal part")
    sides = {}
    for name, sign in (("plus", 1.0), ("minus", -1.0)):
        surfaces = column(tx + sign * offset * nh[0], ty + sign * offset * nh[1])
        near = [s for s in surfaces if abs(s[0] - tz) <= top_tol]
        top = min(near, key=lambda s: abs(s[0] - tz)) if near else None
        below = [s[0] for s in surfaces if s[0] <= tz + top_tol]
        above = [s[0] for s in surfaces if s[0] > tz + top_tol]
        sides[name] = {"top_dz": None if top is None else top[0] - tz,
                       "top_nz": None if top is None else top[1],
                       "drop": tz - max(below) if below else math.inf,
                       "headroom": min(above) - tz if above else math.inf}
    return sides


def classify_profile(sides: dict, min_drop=LEDGE_MIN_DROP, up_nz=UP_FACE_NZ) -> str:
    """'ledge_minus' / 'ledge_plus' (top on that side, open drop on the other), 'top_both',
    'top_steep' (a surface at target z but steeper than up_nz), 'no_top'."""
    walk = {k: v["top_dz"] is not None and v["top_nz"] >= up_nz for k, v in sides.items()}
    any_top = any(v["top_dz"] is not None for v in sides.values())
    for top, open_ in (("minus", "plus"), ("plus", "minus")):
        if walk[top] and not walk[open_] and sides[open_]["drop"] >= min_drop:
            return f"ledge_{top}"
    if walk["plus"] and walk["minus"]:
        return "top_both"
    if walk["plus"] or walk["minus"]:
        return "top_one_side_shallow_drop"
    return "top_steep" if any_top else "no_top"


def segment_angle(a, b, direction) -> float | None:
    """Angle (degrees, 0..90) between segment a..b and an edge direction; None if degenerate."""
    ab = [b[i] - a[i] for i in range(3)]
    length = math.sqrt(sum(v * v for v in ab))
    if length < 1e-4 or direction is None:
        return None
    cos = abs(sum(ab[i] * direction[i] for i in range(3))) / length
    return math.degrees(math.acos(min(1.0, cos)))


def edge_class(e: dict) -> str:
    """'convex_ledge' = convex edge whose top face is walkable (|nz| >= UP_FACE_NZ) and whose
    other face is steep (|nz| <= STEEP_FACE_NZ); otherwise the classify_edge kind."""
    if e.get("kind") == "convex" and e["top_nz"] is not None and e["other_nz"] is not None:
        if e["top_nz"] >= UP_FACE_NZ and e["other_nz"] <= STEEP_FACE_NZ:
            return "convex_ledge"
        return "convex_other"
    return e.get("kind", "none")


def segment_vs_edge(seg_a, seg_b, target, edge_a, edge_b, tol=0.005, weld=EDGE_WELD) -> dict | None:
    """How a candidate segment sits on a decoded triangle edge: both ends on the edge (within
    tol), how many ends coincide with the edge's vertices (within weld), target at the midpoint
    (within 1 cm). None for a degenerate candidate segment."""
    if math.dist(seg_a, seg_b) < 1e-3:
        return None
    on = all(point_segment(p, edge_a, edge_b)[0] <= tol for p in (seg_a, seg_b))
    ends = sum(min(math.dist(p, edge_a), math.dist(p, edge_b)) <= weld for p in (seg_a, seg_b))
    mid = [(seg_a[i] + seg_b[i]) / 2 for i in range(3)]
    return {"on_edge": on, "ends_on_vertices": ends, "target_midpoint": math.dist(mid, target) <= 0.01,
            "longer_than_edge": math.dist(seg_a, seg_b) > math.dist(edge_a, edge_b) + 1e-3}


def quantiles(values, qs=(0.1, 0.5, 0.9)):
    v = sorted(x for x in values if x is not None and math.isfinite(x))
    if not v:
        return None
    return {f"p{int(q * 100)}": round(v[int(round(q * (len(v) - 1)))], 4) for q in qs} | {"n": len(v)}


# ------------------------------------------------------------------- traces ---

def iter_selection_events(path: Path):
    """Entry/exit events of the selection probe, in sequence order, with raw candidate bytes."""
    events = []
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if ENTRY_LABEL not in line and EXIT_LABEL not in line:
                continue
            record = json.loads(line)
            if record.get("kind") != "events":
                continue
            for e in record.get("events", []):
                if e.get("label") in (ENTRY_LABEL, EXIT_LABEL):
                    events.append(e)
    events.sort(key=lambda e: e["sequence"])
    for e in events:
        g = e.get("gather") or {}
        if not g.get("ok"):
            continue
        yield {"label": e["label"], "sequence": e["sequence"], "qpc": e["qpc"], "tid": e["tid"],
               "registers": e.get("registers", []), "base": g.get("base"),
               "elements": [(el["ptr"], bytes.fromhex(el["hex"])) for el in g.get("elements", [])]}


def table_enums(run_dirs) -> dict[int, int]:
    """Fancy table -> action type from decoded traces (tools/parkour_trace.py --image output)."""
    out = {}
    for run in run_dirs:
        for path in run.rglob("decoded/decisions.jsonl"):
            for line in path.read_text(encoding="utf-8").splitlines():
                d = json.loads(line)
                for c in d.get("considered", []) + d.get("ranked", []):
                    if c.get("enum") is not None and c.get("fancy_table"):
                        out[int(c["fancy_table"], 16)] = c["enum"]
    return out


def selection_runs(names=None) -> list[Path]:
    runs = sorted({p.parent for p in RUNS.rglob("native-session.jsonl")})
    runs = [r for r in runs if any(ENTRY_LABEL in line for line in _head_lines(r / "native-session.jsonl"))]
    if names:
        runs = [r for r in runs if r.relative_to(RUNS).parts[0] in names]
    return runs


def _head_lines(path: Path):
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            yield line


def collect(runs, enums) -> tuple[list[dict], dict]:
    """Distinct candidates (by raw bytes) with run, first sighting and selection flags."""
    by_raw, stats = {}, Counter()
    for run in runs:
        name = run.relative_to(RUNS).as_posix()
        pending = {}
        for e in iter_selection_events(run / "native-session.jsonl"):
            key = (e["tid"], e["base"])
            if e["label"] == ENTRY_LABEL:
                pending[key] = e
                stats["entry events"] += 1
                for ptr, raw in e["elements"]:
                    stats["entry candidates"] += 1
                    if raw not in by_raw:
                        try:
                            c = decode_candidate(raw)
                        except ValueError as error:
                            stats[f"undecodable: {error}"] += 1
                            continue
                        c.update(run=name, qpc=e["qpc"], action_type=enums.get(c["table"]),
                                 selected=False, sightings=0)
                        by_raw[raw] = c
                    if raw in by_raw:
                        by_raw[raw]["sightings"] += 1
                continue
            stats["exit events"] += 1
            pending.pop(key, None)
            regs = e["registers"]
            index = struct.unpack("<i", struct.pack("<I", int(regs[4], 16) & 0xFFFFFFFF))[0] if len(regs) > 4 else -1
            if 0 <= index < len(e["elements"]):
                raw = e["elements"][index][1]
                if raw not in by_raw:
                    try:
                        c = decode_candidate(raw)
                        c.update(run=name, qpc=e["qpc"], action_type=enums.get(c["table"]),
                                 selected=False, sightings=0)
                        by_raw[raw] = c
                    except ValueError:
                        continue
                by_raw[raw]["selected"] = True
                stats["selections"] += 1
    return list(by_raw.values()), dict(stats)


def live_samples(run: Path):
    path = run / "live-state.jsonl"
    if not path.exists():
        return []
    out = []
    for line in path.read_text(encoding="utf-8").splitlines():
        d = json.loads(line)
        if d.get("kind") == "sample" and "position" in d:
            out.append((d["qpc"], tuple(d["position"]), tuple(d.get("forward") or (0, 0, 0))))
    out.sort()
    return out


# ------------------------------------------------------------------- fields ---

def field_survey(cands) -> dict:
    per = defaultdict(Counter)
    resid = defaultdict(list)
    for c in cands:
        t = c["action_type"]
        S = per[t]
        S["candidates"] += 1
        r = scalar_residuals(c)
        for k, v in r.items():
            S[f"{k}_reproduced"] += v <= SCALAR_TOLERANCE
            resid[k].append(v)
        S["all_three_reproduced"] += all(r[k] <= SCALAR_TOLERANCE for k in ("c0", "c4", "c8"))
        g = segment_checks(c)
        for k in ("normal_unit", "normal_horizontal", "level", "degenerate", "target_on",
                  "normal_toward_source"):
            S[k] += bool(g[k])
        S["perpendicular"] += bool(g["perpendicular"])
        for k in ("p180", "p190", "p1a0"):
            S[f"{k}_eq_target"] += c[k] == c["target"]
        u = horizontal_unit(c["source"], c["target"])
        S["dir70_eq_source_to_target"] += u is not None and math.hypot(u[0] - c["dir70"][0], u[1] - c["dir70"][1]) < 0.01
    total = Counter()
    for S in per.values():
        total.update(S)
    return {"per_type": {str(k): dict(v) for k, v in sorted(per.items(), key=lambda kv: -kv[1]["candidates"])},
            "total": dict(total),
            "residual_quantiles": {k: quantiles(v, (0.5, 0.9, 0.95, 0.99, 1.0)) for k, v in resid.items()}}


def source_vs_player(cands, runs) -> dict:
    """Distance from +0x10 to the nearest live-state player position (|dt| <= 50 ms)."""
    import bisect
    out = Counter()
    dists = []
    by_run = defaultdict(list)
    for c in cands:
        by_run[c["run"]].append(c)
    for run in runs:
        name = run.relative_to(RUNS).as_posix()
        samples = live_samples(run)
        if not samples:
            continue
        qs = [s[0] for s in samples]
        for c in by_run.get(name, []):
            i = bisect.bisect_left(qs, c["qpc"])
            best = min((samples[j] for j in (i - 1, i) if 0 <= j < len(samples)),
                       key=lambda s: abs(s[0] - c["qpc"]))
            if abs(best[0] - c["qpc"]) > 500_000:
                out["no sample within 50 ms"] += 1
                continue
            d = math.dist(best[1], c["source"])
            dists.append(d)
            out["within 0.15 m"] += d <= 0.15
            out["joined"] += 1
    return {"counts": dict(out), "distance": quantiles(dists, (0.1, 0.5, 0.75, 0.9))}


# ------------------------------------------------------------------ geometry ---

class Geometry:
    """Adapter over tools/world_collision.py World: columns, triangles in a box, ray casts."""

    def __init__(self, world):
        self.world = world
        if world.instances is None:
            world.build_index()
        self.cache = {}

    def column(self, x, y):
        return [(h["z"], h["normal_z"]) for h in self.world.column(x, y)]

    def column_hits(self, x, y):
        return self.world.column(x, y)

    def _tris(self, n):
        if n not in self.cache:
            inst = self.world.instances[n]
            shape = self.world.shape(inst["shape"])
            if shape is None or not shape["triangles"]:
                self.cache[n] = np.zeros((0, 3, 3))
            else:
                v = np.asarray(shape["vertices"], float)
                m = np.asarray(inst["matrix"], float).reshape(4, 4)
                w = v @ m[:3, :3] + m[3, :3]
                self.cache[n] = w[np.asarray(shape["triangles"], int)]
        return self.cache[n]

    def triangles(self, lo, hi):
        """(tris (n,3,3), instance index per tri) for instances overlapping the box, tris whose
        AABB overlaps it."""
        import world_collision as wc
        grid = wc.GRID
        seen, parts, owners = set(), [], []
        for gx in range(math.floor(lo[0] / grid), math.floor(hi[0] / grid) + 1):
            for gy in range(math.floor(lo[1] / grid), math.floor(hi[1] / grid) + 1):
                for n in self.world.grid.get((gx, gy), ()):
                    if n in seen:
                        continue
                    seen.add(n)
                    inst = self.world.instances[n]
                    if any(inst["hi"][i] < lo[i] or inst["lo"][i] > hi[i] for i in range(3)):
                        continue
                    try:
                        t = self._tris(n)
                    except wc.FormatError:
                        continue
                    if len(t) == 0:
                        continue
                    tlo, thi = t.min(axis=1), t.max(axis=1)
                    keep = np.all(thi >= np.asarray(lo) - 1e-6, axis=1) & np.all(tlo <= np.asarray(hi) + 1e-6, axis=1)
                    if keep.any():
                        parts.append(t[keep])
                        owners += [n] * int(keep.sum())
        if not parts:
            return np.zeros((0, 3, 3)), []
        return np.concatenate(parts), owners

    def ray(self, origin, direction, length):
        end = [origin[i] + direction[i] * length for i in range(3)]
        lo = [min(origin[i], end[i]) - 0.05 for i in range(3)]
        hi = [max(origin[i], end[i]) + 0.05 for i in range(3)]
        tris, owners = self.triangles(lo, hi)
        if len(tris) == 0:
            return None
        t = ray_triangles(origin, direction, tris)
        k = int(np.argmin(t))
        if not math.isfinite(t[k]) or t[k] > length:
            return None
        return {"t": float(t[k]), "nz": float(abs(face_normal(tris[k])[2])), "instance": owners[k]}


def analyze_target(geo: Geometry, c: dict) -> dict:
    s, t, n = c["source"], c["target"], c["normal"]
    row = {}
    # source on decoded ground?
    import world_collision as wc
    hits = geo.column_hits(s[0], s[1])
    sup = wc.support(hits, s[2], GROUND_STEP)
    row["source_ground_dz"] = None if sup is None else s[2] - sup["z"]
    row["source_grounded"] = sup is not None and abs(s[2] - sup["z"]) <= GROUND_TOLERANCE
    row["target_above_ground"] = None if sup is None else t[2] - sup["z"]
    # profile
    try:
        sides = ledge_profile(geo.column, t, n)
        row["profile"] = classify_profile(sides)
        row["sides"] = sides
    except ValueError:
        row["profile"], row["sides"] = "normal_not_horizontal", None
    # nearest triangle edge
    lo = [t[i] - EDGE_SEARCH for i in range(3)]
    hi = [t[i] + EDGE_SEARCH for i in range(3)]
    tris, owners = geo.triangles(lo, hi)
    e = nearest_edge(t, tris)
    if e is not None:
        e["angle_to_segment_deg"] = segment_angle(c["seg_a"], c["seg_b"], e["direction"])
        e["class"] = edge_class(e)
        k = int(np.argmin(edge_distances(t, tris).min(axis=1)))
        inst = geo.world.instances[owners[k]]
        e["shape"] = f"{inst['shape']:016x}"
        e["body"] = f"{inst['body']:016x}"
        e["source"] = inst["source"]
        e["shape_name"] = geo.world.shape_source[inst["shape"]][2]
    row["edge"] = e
    # horizontal fan from the source toward the target
    u = horizontal_unit(s, t)
    fan = {}
    if u is not None:
        for h in FAN_HEIGHTS:
            hit = geo.ray((s[0], s[1], s[2] + h), (u[0], u[1], 0.0), FAN_LENGTH)
            fan[str(h)] = None if hit is None else round(hit["t"], 4)
    row["fan"] = fan
    return row


def analyze(geo: Geometry, cands: list[dict]) -> tuple[list[dict], dict]:
    """One row per distinct (action type, source, target) rounded to 1 cm."""
    groups = {}
    for c in cands:
        key = (c["action_type"], tuple(round(v, 2) for v in c["source"]), tuple(round(v, 2) for v in c["target"]))
        g = groups.get(key)
        if g is None:
            groups[key] = dict(c, sightings=c["sightings"], snapshots=1)
        else:
            g["sightings"] += c["sightings"]
            g["snapshots"] += 1
            g["selected"] = g["selected"] or c["selected"]
    target_cache = {}
    rows = []
    for key, c in groups.items():
        tkey = (tuple(round(v, 2) for v in c["source"]), tuple(round(v, 2) for v in c["target"]),
                tuple(round(v, 2) for v in c["normal"]))
        if tkey not in target_cache:
            target_cache[tkey] = analyze_target(geo, c)
        g = target_cache[tkey]
        r = scalar_residuals(c)
        e = g["edge"]
        se = (segment_vs_edge(c["seg_a"], c["seg_b"], c["target"], e["a"], e["b"])
              if e and e["distance"] <= 0.05 else None)
        rows.append({"action_type": c["action_type"], "run": c["run"], "selected": c["selected"],
                     "snapshots": c["snapshots"], "source": [round(v, 4) for v in c["source"]],
                     "target": [round(v, 4) for v in c["target"]],
                     "normal": [round(v, 4) for v in c["normal"]],
                     "segment": [[round(v, 4) for v in c["seg_a"]], [round(v, 4) for v in c["seg_b"]]],
                     "c0": round(c["c0"], 4), "c4": round(c["c4"], 4), "c8": round(c["c8"], 4),
                     "scalars_reproduced": all(r[k] <= SCALAR_TOLERANCE for k in ("c0", "c4", "c8")),
                     "fitness": round(c["fitness"], 4), "segment_edge": se, **g})
    return rows, summarize(rows)


def type_summary(rs) -> dict:
    edges = [r["edge"] for r in rs if r["edge"]]
    near = [e for e in edges if e["distance"] <= 0.05]
    grounded = [r for r in rs if r["source_grounded"]]
    fan = {}
    for h in FAN_HEIGHTS:
        deltas = [r["fan"][str(h)] - r["c4"] for r in grounded
                  if r["fan"] and r["fan"].get(str(h)) is not None]
        fan[str(h)] = {"rays": sum(bool(r["fan"]) for r in grounded), "hits": len(deltas),
                       "hit_minus_c4": quantiles(deltas),
                       "hit_within_0.3m_of_c4": sum(abs(d) <= 0.3 for d in deltas),
                       "hit_short_of_target_by_more_than_0.3m": sum(d < -0.3 for d in deltas)}
    return {
        "rows": len(rs), "selected_rows": sum(r["selected"] for r in rs),
        "scalars_reproduced": sum(r["scalars_reproduced"] for r in rs),
        "source_grounded": len(grounded),
        "c0_height_above_source": quantiles([r["c0"] for r in rs]),
        "c4_horizontal": quantiles([r["c4"] for r in rs]),
        "target_above_source_ground": quantiles([r["target_above_ground"] for r in rs]),
        "profile": dict(Counter(r["profile"] for r in rs)),
        "edge_distance": quantiles([e["distance"] for e in edges], (0.5, 0.9)),
        "edge_within_5cm": len(near),
        "edge_class_within_5cm": dict(Counter(e["class"] for e in near)),
        "edge_parallel_within_10deg": sum(e["angle_to_segment_deg"] is not None
                                          and e["angle_to_segment_deg"] <= 10 for e in near),
        "edge_source": dict(Counter(e["source"] for e in near)),
        "top_nz_within_5cm": quantiles([e["top_nz"] for e in near]),
        "other_nz_within_5cm": quantiles([e["other_nz"] for e in near]),
        "segment_on_edge": sum(bool(r["segment_edge"] and r["segment_edge"]["on_edge"]) for r in rs),
        "segment_ends_on_vertices": dict(Counter(r["segment_edge"]["ends_on_vertices"]
                                                 for r in rs if r["segment_edge"])),
        "target_at_segment_midpoint": sum(bool(r["segment_edge"] and r["segment_edge"]["target_midpoint"])
                                          for r in rs),
        "fan_from_grounded_source": fan,
    }


def summarize(rows) -> dict:
    per = defaultdict(list)
    for r in rows:
        per[r["action_type"]].append(r)
    out = {}
    for t, rs in sorted(per.items(), key=lambda kv: -len(kv[1])):
        out[str(t)] = type_summary(rs)
        chosen = [r for r in rs if r["selected"]]
        if chosen:
            out[str(t)]["selected_only"] = {k: v for k, v in type_summary(chosen).items()
                                            if k not in ("fan_from_grounded_source",)}
    return out


# --------------------------------------------------------------------- CLI ---

def provenance(runs) -> list[dict]:
    out = []
    for run in runs:
        path = run / "native-session.jsonl"
        out.append({"run": run.relative_to(RUNS).as_posix(),
                    "session_sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    return out


def cmd_fields(args):
    runs = selection_runs(args.runs)
    enums = table_enums(runs)
    cands, stats = collect(runs, enums)
    survey = field_survey(cands)
    survey["collect"] = stats
    survey["source_vs_live_player"] = source_vs_player(cands, runs)
    survey["runs"] = provenance(runs)
    print(json.dumps(survey, indent=1, default=str))


def cmd_analyze(args):
    sys.path.insert(0, str(ROOT / "tools"))
    import world_collision as wc
    runs = selection_runs(args.runs)
    enums = table_enums(runs)
    cands, stats = collect(runs, enums)
    world = wc.World(args.archive)
    geo = Geometry(world)
    rows, summary = analyze(geo, cands)
    totals = Counter()
    for r in rows:
        totals["rows"] += 1
        totals["profile " + r["profile"]] += 1
        totals["source grounded"] += r["source_grounded"]
        totals["edge within 5 cm"] += bool(r["edge"] and r["edge"]["distance"] <= 0.05)
        totals["edge within 10 cm"] += bool(r["edge"] and r["edge"]["distance"] <= 0.10)
        totals["no triangle within 0.3 m"] += r["edge"] is None
    result = {"collect": stats, "totals": dict(totals), "per_type": summary}
    print(json.dumps(result, indent=1, default=str))
    if args.write:
        provenance_runs = provenance(runs)
        fixture = {
            "schema": "acu-oracle/parkour-geometry/1",
            "finding": "ACU-RE-0055",
            "binary_sha256": BINARY_SHA256,
            "world_archive": args.archive,
            "command": "python tools/parkour_geometry.py analyze --write " + args.write,
            "method": ("raw 672-byte candidates gathered at the selection routine entry/exit "
                       "(ACU-RE-0042) decoded with the offsets in tools/parkour_geometry.py; "
                       "joined to MeshShape collision decoded by tools/world_collision.py "
                       "(ACU-RE-0050; primitives not decoded)"),
            "choices": {"scalar_tolerance_m": SCALAR_TOLERANCE, "profile_offset_m": PROFILE_OFFSET,
                        "top_tolerance_m": TOP_TOLERANCE, "ledge_min_drop_m": LEDGE_MIN_DROP,
                        "up_face_nz": UP_FACE_NZ, "ground_step_m": GROUND_STEP,
                        "ground_tolerance_m": GROUND_TOLERANCE, "edge_search_m": EDGE_SEARCH,
                        "fan_heights_m": FAN_HEIGHTS, "fan_length_m": FAN_LENGTH},
            "runs": provenance_runs, "collect": stats, "totals": dict(totals), "per_type": summary,
            "row_columns": ROW_COLUMNS,
            "row_note": ("one row per distinct (action_type, source, target) at 1 cm; positions in "
                         "world metres (Z up, ACU-RE-0047 frame); normal = candidate +0x40 xy; "
                         "profile = classify_profile at profile_offset_m along +/-normal; edge_* = "
                         "nearest decoded triangle edge to the target (class per edge_class); "
                         "fan_* = horizontal ray hit distance from source + height toward the "
                         "target (null = no hit within fan_length_m); run = index into runs"),
            "rows": [compact_row(r, [x["run"] for x in provenance_runs]) for r in rows],
        }
        text = json.dumps(_finite(fixture), separators=(",", ":"), default=_json_default, allow_nan=False)
        Path(args.write).write_text(text + "\n", encoding="utf-8")


ROW_COLUMNS = ["action_type", "run", "selected", "source", "target", "normal_xy", "segment_a",
               "segment_b", "c0", "c4", "c8", "scalars_reproduced", "source_ground_dz", "profile",
               "edge_distance", "edge_class", "edge_top_nz", "edge_other_nz", "edge_angle_deg",
               "segment_on_edge", "segment_ends_on_vertices", "target_midpoint", "edge_shape",
               "edge_body", "edge_source", "fan_0.75", "fan_1.0", "fan_1.25", "fan_1.75"]


def _r(v, n=4):
    if v is None or (isinstance(v, float) and not math.isfinite(v)):
        return None
    return round(float(v), n)


def compact_row(r: dict, run_names: list[str]) -> list:
    e, se = r["edge"] or {}, r["segment_edge"] or {}
    return [r["action_type"], run_names.index(r["run"]), r["selected"], r["source"], r["target"],
            r["normal"][:2], r["segment"][0], r["segment"][1], r["c0"], r["c4"], r["c8"],
            r["scalars_reproduced"], _r(r["source_ground_dz"]), r["profile"],
            _r(e.get("distance")), e.get("class"), _r(e.get("top_nz"), 3), _r(e.get("other_nz"), 3),
            _r(e.get("angle_to_segment_deg"), 2), se.get("on_edge"), se.get("ends_on_vertices"),
            se.get("target_midpoint"), e.get("shape"), e.get("body"), e.get("source")] +         [_r(r["fan"].get(str(h))) if r["fan"] else None for h in FAN_HEIGHTS]


def _finite(value):
    """Replace non-finite floats (e.g. an open drop) with None, recursively."""
    if isinstance(value, float):
        return value if math.isfinite(value) else None
    if isinstance(value, dict):
        return {k: _finite(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_finite(v) for v in value]
    return value


def _json_default(value):
    if isinstance(value, float) and not math.isfinite(value):
        return None
    if isinstance(value, (np.floating, np.integer)):
        return value.item()
    raise TypeError(type(value))


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="command", required=True)
    f = sub.add_parser("fields")
    a = sub.add_parser("analyze")
    for p in (f, a):
        p.add_argument("--runs", nargs="*", help="run directory names under local/native-probe/runs")
    a.add_argument("--archive", default="DataPC_ACU_Versailles.forge")
    a.add_argument("--write")
    args = ap.parse_args(argv)
    {"fields": cmd_fields, "analyze": cmd_analyze}[args.command](args)


if __name__ == "__main__":
    main()
