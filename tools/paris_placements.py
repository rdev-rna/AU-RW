#!/usr/bin/env python3
"""Offline whole-Paris placement map from the Forge archives (ACU-RE-0047).

No game process is read. The archive is decoded by two read-only Rust bins
(`acu-type-dump`, `acu-mesh-bounds`, in `acu-rs/crates/acu_assets/src/bin/`); this
script interprets their output with the layouts recorded in ACU-RE-0035 and ACU-RE-0047.

Subcommands (run in order, or `all`):

  extract      dump every GridCellDataBlock / Entity / LODSelector / EntityGroup payload
               of DataPC_ACU_Paris.forge into local/paris/*.bin (entry lists come from
               the asset index)
  placements   walk every cell, decode each placed Entity / EntityGroup, write
               local/paris/placements.jsonl and local/paris/placements-summary.json
  meshes       decode the first (LOD0) mesh of every prop type with acu-mesh-bounds
  types        group placements into prop types, attach sizes, flag parkour candidates
               (size/name INFERENCES), write local/paris/prop-types.json

Layouts used (observed on payloads; see the finding for counts):

  common   u8 version=1, u64 self id, u32 type id, u8 transform flag
  flag 3   f32[16] world matrix at 14 (row-major, rows 0-2 basis, row 3 translation,
           column 3 = 0,0,0,1); u32 component count at 78
  flag 0   embedded object at 14: u64 id, u32 tag 788baa0d, f32[16] local matrix at 26,
           25 bytes, f32[16] world matrix at 115; u32 component count at 179
  Entity tail  u32 tag 4aec3476 + f32[6] local AABB (min xyz, max xyz), followed 40
           bytes later by tag 60121a9e; exactly one such block per Entity
  references (tag-anchored, not a full component walk)
           u32 ec658d29, 4 bytes, u64 reference   -> the visual's object (selector/mesh/...)
           u32 536e963b, u8 1, u64 mesh id        -> a 415d9568 world mesh (one per LOD)

Anything that does not match these layouts is reported with a reason, never skipped.
Axis convention (Z up) and units (metres) are NOT assumed by this script; the finding
records the evidence for them.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import hashlib
import json
import math
from pathlib import Path
import re
import sqlite3
import statistics
import struct
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "local/paris"
INDEX = ROOT / "local/asset_index.sqlite"
ARCHIVE = "DataPC_ACU_Paris.forge"

TYPE_CELL = 0xAC2BBF68
TYPE_ENTITY = 0x0984415E
TYPE_SELECTOR = 0x51DC6B80
TYPE_GROUP = 0x3F742D26
TYPE_MESH = "415d9568"
TYPE_NAMES = {
    "ac2bbf68": "GridCellDataBlock", "0984415e": "Entity", "51dc6b80": "LODSelector",
    "3f742d26": "EntityGroup", "415d9568": "WorldMesh(unnamed 415d9568)",
    "344780d6": "OmniLight", "1cbde084": "ShadowRigidBody", "b22b3e61": "MeshShape",
    "2d675ba2": "ReferenceListShape", "a2b7e917": "TextureMap", "0b4ce0e0": "NavMeshManager",
    "85c817c3": "Material",
}

TAG_TRANSFORM_OBJECT = 0x788BAA0D
TAG_VISUAL = struct.pack("<I", 0xEC658D29)
TAG_MESH_INSTANCE = struct.pack("<I", 0x536E963B)
TAG_BBOX = struct.pack("<I", 0x4AEC3476)
TAG_TAIL = struct.pack("<I", 0x60121A9E)

DUMP_MAGIC = b"ACUTD001"
MAX_CELL_CHILDREN = 100_000
AFFINE_TOLERANCE = 1e-4


class FormatError(ValueError):
    """Payload does not match a supported layout."""


# ---------------------------------------------------------------- parsers (pure) ---

def read_dump(data: bytes):
    """Yield (entry, ordinal, file_id, type_id, name, payload_offset, payload_length)."""
    if data[:8] != DUMP_MAGIC:
        raise FormatError("not an ACUTD001 dump")
    pos, end = 8, len(data)
    while pos < end:
        if end - pos < 22:
            raise FormatError(f"truncated dump record header at {pos}")
        entry, ordinal, file_id, type_id, name_length = struct.unpack_from("<IIQIH", data, pos)
        pos += 22
        if end - pos < name_length + 4:
            raise FormatError(f"truncated dump name at {pos}")
        name = data[pos:pos + name_length].decode("utf-8")
        pos += name_length
        (length,) = struct.unpack_from("<I", data, pos)
        pos += 4
        if end - pos < length:
            raise FormatError(f"truncated dump payload at {pos} ({length} B declared)")
        yield entry, ordinal, file_id, type_id, name, pos, length
        pos += length


def parse_header(payload: bytes, expected_type: int | None = None):
    if len(payload) < 13:
        raise FormatError(f"payload {len(payload)} B shorter than the 13-byte header")
    version = payload[0]
    self_id, type_id = struct.unpack_from("<QI", payload, 1)
    if version != 1:
        raise FormatError(f"unsupported version {version}")
    if expected_type is not None and type_id != expected_type:
        raise FormatError(f"type {type_id:08x}, expected {expected_type:08x}")
    return self_id, type_id


def parse_cell(payload: bytes):
    """GridCellDataBlock: u32 count, count x (u16 flags, u64 child), u32 x3 tail."""
    self_id, _ = parse_header(payload, TYPE_CELL)
    if len(payload) < 17:
        raise FormatError("cell shorter than its count field")
    (count,) = struct.unpack_from("<I", payload, 13)
    if count > MAX_CELL_CHILDREN:
        raise FormatError(f"cell child count {count} over limit")
    if 17 + count * 10 + 12 != len(payload):
        raise FormatError(f"cell child count {count} does not fit {len(payload)} B")
    children = [struct.unpack_from("<HQ", payload, 17 + i * 10) for i in range(count)]
    tail = struct.unpack_from("<III", payload, 17 + count * 10)
    return dict(self_id=self_id, children=children, tail=tail)


def _matrix(payload: bytes, offset: int, label: str):
    if len(payload) < offset + 64:
        raise FormatError(f"{label} matrix at {offset} truncated")
    m = struct.unpack_from("<16f", payload, offset)
    if not all(math.isfinite(v) for v in m):
        raise FormatError(f"{label} matrix at {offset} not finite")
    column = (m[3], m[7], m[11], m[15])
    if any(abs(a - b) > AFFINE_TOLERANCE for a, b in zip(column, (0.0, 0.0, 0.0, 1.0))):
        raise FormatError(f"{label} matrix at {offset} not affine (column 3 = {column})")
    return m


def parse_transform(payload: bytes):
    """World (and, for flag 0, local) matrix plus the component-count offset."""
    if len(payload) < 14:
        raise FormatError("payload has no transform flag")
    flag = payload[13]
    if flag == 3:
        world = _matrix(payload, 14, "world")
        count_at = 78
        layout, local = "plain-matrix@14", None
    elif flag == 0:
        if len(payload) < 26:
            raise FormatError("transform object header truncated")
        (tag,) = struct.unpack_from("<I", payload, 22)
        if tag != TAG_TRANSFORM_OBJECT:
            raise FormatError(f"flag-0 transform object tag {tag:08x}, expected 788baa0d")
        local = _matrix(payload, 26, "local")
        world = _matrix(payload, 115, "world")
        count_at = 179
        layout = "object-788baa0d@14(local@26,world@115)"
    else:
        raise FormatError(f"unsupported transform flag {flag}")
    if len(payload) < count_at + 4:
        raise FormatError(f"component count at {count_at} truncated")
    (components,) = struct.unpack_from("<I", payload, count_at)
    return dict(flag=flag, layout=layout, world=world, local=local, components=components)


def tag_references(payload: bytes):
    """Tag-anchored references: visual objects and per-LOD mesh instances, in order."""
    visuals, meshes = [], []
    for match in re.finditer(re.escape(TAG_VISUAL), payload):
        at = match.start()
        if at >= 8 and at + 16 <= len(payload):
            visuals.append(struct.unpack_from("<Q", payload, at + 8)[0])
    for match in re.finditer(re.escape(TAG_MESH_INSTANCE), payload):
        at = match.start()
        if at >= 8 and at + 13 <= len(payload) and payload[at + 4] == 1:
            meshes.append(struct.unpack_from("<Q", payload, at + 5)[0])
    return visuals, meshes


def own_bbox(payload: bytes):
    """The Entity's local AABB: the bbox tag followed 40 bytes later by the tail tag."""
    blocks = [m.start() for m in re.finditer(re.escape(TAG_BBOX), payload)
              if payload[m.start() + 40:m.start() + 44] == TAG_TAIL]
    if not blocks:
        return None, 0
    values = struct.unpack_from("<6f", payload, blocks[-1] + 4)
    return (values[:3], values[3:]), len(blocks)


def decompose(m):
    """Rows 0-2 basis (scaled), row 3 translation, as stored (row-vector convention)."""
    rows = [m[0:3], m[4:7], m[8:11]]
    scale = [math.sqrt(sum(v * v for v in row)) for row in rows]
    unit = [[v / s if s else 0.0 for v in row] for row, s in zip(rows, scale)]
    yaw = math.degrees(math.atan2(unit[0][1], unit[0][0]))
    tilt = max(abs(unit[2][0]), abs(unit[2][1]), abs(1.0 - unit[2][2]))
    return dict(position=list(m[12:15]), rotation_rows=unit, scale=scale,
                yaw_deg_about_row2=yaw, row2_deviation=tilt)


def world_aabb(m, box):
    """Transform the 8 corners of a local box with p' = x*row0 + y*row1 + z*row2 + row3."""
    lo, hi = box
    xs, ys, zs = [], [], []
    for x in (lo[0], hi[0]):
        for y in (lo[1], hi[1]):
            for z in (lo[2], hi[2]):
                xs.append(x * m[0] + y * m[4] + z * m[8] + m[12])
                ys.append(x * m[1] + y * m[5] + z * m[9] + m[13])
                zs.append(x * m[2] + y * m[6] + z * m[10] + m[14])
    return [min(xs), min(ys), min(zs)], [max(xs), max(ys), max(zs)]


def parse_placed(payload: bytes, expected_type: int):
    """Decode one Entity or EntityGroup payload into a placement record."""
    self_id, _ = parse_header(payload, expected_type)
    transform = parse_transform(payload)
    visuals, meshes = tag_references(payload)
    box, box_blocks = own_bbox(payload)
    if expected_type == TYPE_ENTITY and box_blocks != 1:
        raise FormatError(f"entity has {box_blocks} bbox+tail blocks, expected exactly 1")
    record = dict(self_id=self_id, flag=transform["flag"], layout=transform["layout"],
                  components=transform["components"], world=transform["world"],
                  local_translation=list(transform["local"][12:15]) if transform["local"] else None,
                  visual_refs=visuals, mesh_refs=meshes,
                  bbox_local=box if expected_type == TYPE_ENTITY else None,
                  bbox_blocks=box_blocks)
    record.update(decompose(transform["world"]))
    return record


def name_stem(name: str) -> str:
    """Strip one trailing instance suffix of three or more digits (`_008`)."""
    return re.sub(r"_\d{3,}$", "", name) if name else name


# ---------------------------------------------------------------- environment ---

def select_archive(name: str):
    """Point the module at one world archive and its output folder."""
    global ARCHIVE, OUT
    if Path(name).name != name or not name.endswith(".forge"):
        raise SystemExit("--archive must be a .forge basename")
    ARCHIVE = name
    OUT = ROOT / ("local/paris" if name == "DataPC_ACU_Paris.forge"
                  else f"local/world-{Path(name).stem}")


def game_dir() -> Path:
    sys.path.insert(0, str(ROOT / "tools"))
    from acu import settings  # noqa: E402
    return Path(settings()[1]["game_dir"])


def find_bin(name: str) -> Path:
    for folder in (ROOT / "acu-rs/target/release", ROOT / "local/paris/target/release",
                   ROOT / "acu-rs/target/debug"):
        path = folder / f"{name}.exe"
        if path.exists():
            return path
        if (folder / name).exists():
            return folder / name
    raise SystemExit(f"{name} not built; run in acu-rs/: cargo build --locked --release "
                     f"-p acu_assets --bin {name}")


def index_db():
    if not INDEX.exists():
        raise SystemExit("asset index missing; run python tools/index_status.py --fetch")
    return sqlite3.connect(f"file:{INDEX.as_posix()}?mode=ro", uri=True)


def run(command):
    print("+", " ".join(f'"{c}"' if " " in str(c) else str(c) for c in command), flush=True)
    result = subprocess.run([str(c) for c in command], capture_output=True, text=True)
    if result.returncode != 0:
        raise SystemExit(f"command failed ({result.returncode}): {result.stderr.strip()[:500]}")
    return json.loads(result.stdout)


# ---------------------------------------------------------------- extract ---

def entries_with(db, archive, types):
    marks = ",".join("?" * len(types))
    rows = db.execute(f"""SELECT DISTINCT e.idx FROM resource r JOIN entry e ON e.id = r.entry_id
        JOIN archive a ON a.id = e.archive_id WHERE a.name = ? AND r.type_id IN ({marks})
        ORDER BY e.idx""", (archive, *types)).fetchall()
    return [r[0] for r in rows]


def cmd_extract(args):
    OUT.mkdir(parents=True, exist_ok=True)
    db = index_db()
    exe = find_bin("acu-type-dump")
    archive = game_dir() / ARCHIVE
    jobs = (("world", ["ac2bbf68", "0984415e", "51dc6b80"]), ("group", ["3f742d26"]))
    for tag, types in jobs:
        entries = entries_with(db, ARCHIVE, types)
        listing = OUT / f"entries-{tag}.txt"
        listing.write_text("\n".join(map(str, entries)) + "\n", encoding="utf-8")
        report = run([exe, archive, "--types", ",".join(types), "--entries", listing,
                      "--out", OUT / f"{tag}-dump.bin"])
        (OUT / f"{tag}-dump.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(f"{tag}: {report['resources_written']} resources from {report['entries_read']}"
              f" entries; failed entries {len(report['failed_entries'])}")
        if report["failed_entries"]:
            raise SystemExit(f"{tag}: entries failed to decode; see {tag}-dump.json")


# ---------------------------------------------------------------- placements ---

def lookup_types(db, ids):
    """id -> (type_id, name, archive) preferring a Paris-archive copy."""
    db.execute("CREATE TEMP TABLE IF NOT EXISTS want(file_id TEXT PRIMARY KEY)")
    db.execute("DELETE FROM want")
    db.executemany("INSERT OR IGNORE INTO want VALUES (?)", ((f"{i:016x}",) for i in ids))
    out = {}
    for file_id, type_id, name, archive in db.execute("""SELECT r.file_id, r.type_id, r.name,
            a.name FROM want w JOIN resource r ON r.file_id = w.file_id
            JOIN entry e ON e.id = r.entry_id JOIN archive a ON a.id = e.archive_id"""):
        key = int(file_id, 16)
        rank = 0 if archive.startswith(Path(ARCHIVE).stem) else 1
        if key not in out or rank < out[key][3]:
            out[key] = (type_id, name, archive, rank)
    return {k: v[:3] for k, v in out.items()}


def load(tag):
    path = OUT / f"{tag}-dump.bin"
    if not path.exists():
        raise SystemExit(f"{path} missing; run: python tools/paris_placements.py extract")
    data = path.read_bytes()
    return data, list(read_dump(data))


def choose_copy(copies, cell_entries, data):
    """Pick one payload for a file id that the archive stores more than once."""
    if len(copies) == 1:
        return copies[0], "single"
    payloads = {data[c[5]:c[5] + c[6]] for c in copies}
    if len(payloads) == 1:
        return copies[0], "identical-copies"
    local = [c for c in copies if c[0] in cell_entries]
    if len(local) == 1 or (local and len({data[c[5]:c[5] + c[6]] for c in local}) == 1):
        return local[0], "copy-in-referencing-cell-entry"
    return min(copies, key=lambda c: c[0]), "AMBIGUOUS-lowest-entry"


def cmd_placements(args):
    data, records = load("world")
    gdata, grecords = load("group")
    db = index_db()
    archive_size = (game_dir() / ARCHIVE).stat().st_size

    by_type = defaultdict(lambda: defaultdict(list))
    for rec in records:
        by_type[rec[3]][rec[2]].append(rec)
    for rec in grecords:
        by_type[rec[3]][rec[2]].append(rec)
    entities, groups = by_type[TYPE_ENTITY], by_type[TYPE_GROUP]
    selectors = by_type[TYPE_SELECTOR]

    failures = []
    cells_ok, cell_children = 0, defaultdict(list)
    child_kind = Counter()
    cell_kinds = Counter()
    unknown_children = set()
    for copies in by_type[TYPE_CELL].values():
        for rec in copies:
            payload = data[rec[5]:rec[5] + rec[6]]
            try:
                cell = parse_cell(payload)
            except FormatError as error:
                failures.append(dict(kind="cell", file_id=f"{rec[2]:016x}", name=rec[4],
                                     entry=rec[0], reason=str(error)))
                continue
            cells_ok += 1
            has_placed = False
            for flags, child in cell["children"]:
                cell_children[child].append((rec[2], rec[4], rec[0], cell["tail"][1], flags))
                if child in entities:
                    child_kind["Entity"] += 1
                    has_placed = True
                elif child in groups:
                    child_kind["EntityGroup"] += 1
                    has_placed = True
                else:
                    unknown_children.add(child)
            cell_kinds["with-placements" if has_placed else "no-placements"] += 1
    other = lookup_types(db, unknown_children)
    for child in unknown_children:
        type_id = other.get(child, (None,))[0]
        child_kind[TYPE_NAMES.get(type_id, type_id or "not-in-index")] += len(cell_children[child])

    rows, choice = [], Counter()
    ref_ids = set()
    placed_ids = [(i, TYPE_ENTITY) for i in entities] + [(i, TYPE_GROUP) for i in groups]
    parsed = {}
    for file_id, expected in placed_ids:
        copies = (entities if expected == TYPE_ENTITY else groups)[file_id]
        buf = data if expected == TYPE_ENTITY else gdata
        cells = cell_children.get(file_id, [])
        rec, rule = choose_copy(copies, {c[2] for c in cells}, buf)
        choice[rule] += 1
        payload = buf[rec[5]:rec[5] + rec[6]]
        try:
            placed = parse_placed(payload, expected)
        except FormatError as error:
            failures.append(dict(kind=TYPE_NAMES[f"{expected:08x}"], file_id=f"{file_id:016x}",
                                 name=rec[4], entry=rec[0], reason=str(error)))
            continue
        parsed[file_id] = (rec, rule, len(copies), placed, payload, expected)
        ref_ids.update(placed["visual_refs"])
        ref_ids.update(placed["mesh_refs"])
    refs = lookup_types(db, ref_ids)

    def describe(ids):
        out = []
        for i in ids:
            type_id, name, archive = refs.get(i, (None, None, None))
            out.append(dict(id=f"{i:016x}", type=type_id, type_name=TYPE_NAMES.get(type_id),
                            name=name, archive=archive))
        return out

    bbox_min = [math.inf] * 3
    bbox_max = [-math.inf] * 3
    stats = Counter()
    out_path = OUT / "placements.jsonl"
    with out_path.open("w", encoding="utf-8") as stream:
        for file_id, (rec, rule, ncopies, placed, payload, expected) in parsed.items():
            cells = cell_children.get(file_id, [])
            visuals = describe(placed["visual_refs"])
            meshes = describe(placed["mesh_refs"])
            unresolved_mesh = [m["id"] for m in meshes if m["type"] != TYPE_MESH]
            row = dict(
                file_id=f"{file_id:016x}", name=rec[4], stem=name_stem(rec[4]),
                resource_type=TYPE_NAMES[f"{expected:08x}"],
                archive=ARCHIVE, entry=rec[0], ordinal=rec[1], payload_length=len(payload),
                payload_sha256=hashlib.sha256(payload).hexdigest(),
                copies=ncopies, copy_rule=rule,
                cells=[dict(file_id=f"{c[0]:016x}", name=c[1], entry=c[2], cell_index=c[3])
                       for c in cells],
                transform_flag=placed["flag"], transform_layout=placed["layout"],
                position=placed["position"], rotation_rows=placed["rotation_rows"],
                scale=placed["scale"], yaw_deg_about_row2=placed["yaw_deg_about_row2"],
                row2_deviation=placed["row2_deviation"],
                local_translation=placed["local_translation"],
                component_count=placed["components"],
                visual_refs=visuals, mesh_refs=meshes, mesh_refs_not_world_mesh=unresolved_mesh,
                bbox_local=None, bbox_world=None, bbox_blocks=placed["bbox_blocks"],
            )
            if placed["bbox_local"]:
                lo, hi = placed["bbox_local"]
                row["bbox_local"] = dict(min=list(lo), max=list(hi))
                wlo, whi = world_aabb(placed["world"], placed["bbox_local"])
                row["bbox_world"] = dict(min=wlo, max=whi)
            stream.write(json.dumps(row, separators=(",", ":")) + "\n")
            stats[row["resource_type"]] += 1
            stats["in-a-cell" if cells else "NOT-referenced-by-any-cell"] += 1
            stats[f"flag-{placed['flag']}"] += 1
            stats["with-visual-ref" if visuals else "without-visual-ref"] += 1
            stats["with-mesh-ref" if meshes else "without-mesh-ref"] += 1
            if unresolved_mesh:
                stats["mesh-ref-not-a-415d9568-resource"] += 1
            if cells:
                for i in range(3):
                    bbox_min[i] = min(bbox_min[i], placed["position"][i])
                    bbox_max[i] = max(bbox_max[i], placed["position"][i])

    visual_types = Counter()
    for _, (_, _, _, placed, _, _) in parsed.items():
        for v in placed["visual_refs"]:
            t = refs.get(v, (None,))[0]
            visual_types[TYPE_NAMES.get(t, t or "not-in-index")] += 1
    summary = dict(
        schema_version=1,
        archive=dict(name=ARCHIVE, size_bytes=archive_size,
                     world_dump=json.loads((OUT / "world-dump.json").read_text()),
                     group_dump=json.loads((OUT / "group-dump.json").read_text())),
        cells=dict(resources=sum(len(v) for v in by_type[TYPE_CELL].values()),
                   distinct_ids=len(by_type[TYPE_CELL]), parsed=cells_ok, kinds=cell_kinds,
                   child_references_by_type=child_kind),
        entities=dict(resources=sum(len(v) for v in entities.values()),
                      distinct_ids=len(entities)),
        groups=dict(resources=sum(len(v) for v in groups.values()), distinct_ids=len(groups)),
        selectors_distinct=len(selectors),
        placements_written=len(parsed), placement_stats=stats, copy_rules=choice,
        visual_ref_types=visual_types,
        position_bbox_of_cell_referenced=dict(min=bbox_min, max=bbox_max),
        failures=failures, failure_reasons=Counter(f["reason"].split(" at ")[0] for f in failures),
        output=str(out_path.relative_to(ROOT)),
    )
    (OUT / "placements-summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps({k: summary[k] for k in ("cells", "entities", "groups", "placements_written",
                                               "placement_stats", "copy_rules",
                                               "position_bbox_of_cell_referenced",
                                               "failure_reasons")}, indent=1))


# ---------------------------------------------------------------- meshes ---

def iter_placements():
    path = OUT / "placements.jsonl"
    if not path.exists():
        raise SystemExit("placements.jsonl missing; run the placements step")
    with path.open(encoding="utf-8") as stream:
        for line in stream:
            yield json.loads(line)


def lod0(row):
    meshes = [m for m in row["mesh_refs"] if m["type"] == TYPE_MESH]
    for mesh in meshes:
        if mesh["name"] and mesh["name"].endswith("_LOD0"):
            return mesh
    return meshes[0] if meshes else None


def cmd_meshes(args):
    wanted = {}
    for row in iter_placements():
        mesh = lod0(row)
        if mesh:
            wanted[mesh["id"]] = mesh["name"]
    db = index_db()
    db.execute("CREATE TEMP TABLE want(file_id TEXT PRIMARY KEY)")
    db.executemany("INSERT INTO want VALUES (?)", ((i,) for i in wanted))
    base = Path(ARCHIVE).stem
    order = {ARCHIVE: 0, f"{base}_assets.forge": 1}
    where = {}
    for file_id, archive, idx in db.execute("""SELECT r.file_id, a.name, e.idx FROM want w
            JOIN resource r ON r.file_id = w.file_id JOIN entry e ON e.id = r.entry_id
            JOIN archive a ON a.id = e.archive_id WHERE r.type_id = ?""", (TYPE_MESH,)):
        rank = order.get(archive, 2 if archive.startswith(base) else 9)
        if rank < 9 and (file_id not in where or rank < where[file_id][0]):
            where[file_id] = (rank, archive, idx)
    jobs = defaultdict(set)
    for file_id, (_, archive, idx) in where.items():
        jobs[archive].add((idx, file_id))
    exe = find_bin("acu-mesh-bounds")
    reports = []
    for archive, items in sorted(jobs.items()):
        stem = Path(archive).stem
        (OUT / f"mesh-entries-{stem}.txt").write_text(
            "\n".join(str(i) for i in sorted({i for i, _ in items})) + "\n", encoding="utf-8")
        (OUT / f"mesh-ids-{stem}.txt").write_text(
            "\n".join(sorted({f for _, f in items})) + "\n", encoding="utf-8")
        report = run([exe, game_dir() / archive, "--entries", OUT / f"mesh-entries-{stem}.txt",
                      "--ids", OUT / f"mesh-ids-{stem}.txt", "--out", OUT / f"mesh-bounds-{stem}.jsonl"])
        report["requested"] = len(items)
        reports.append(report)
        print(archive, report)
    missing = sorted(set(wanted) - set(where))
    (OUT / "mesh-bounds-summary.json").write_text(json.dumps(dict(
        requested=len(wanted), located=len(where), not_located_in_paris_archives=missing,
        reports=reports), indent=2), encoding="utf-8")
    print(f"meshes requested {len(wanted)}, located {len(where)}, not located {len(missing)}")


# ---------------------------------------------------------------- types / flags ---

VAULT_HEIGHT = (0.4, 1.6)      # metres, from the task's clip-name reading (Up50..Up200 cm)
VAULT_DEPTH_MAX = 3.0          # metres, Fd100..Fd300 cm
BEAM_LENGTH_MIN = 2.0
BEAM_CROSS_MAX = 0.4
NAME_HINTS = {
    "vault": re.compile(r"(?i)(fence|railing|rail|balust|bench|wall|cart|crate|barrel|table|"
                        r"stall|chest|box|planter|hedge|bush|low|parapet|ledge|bollard|"
                        r"trough|cask|sack|bale|counter|log|well|fountain|tomb|bin)"),
    "beam": re.compile(r"(?i)(beam|pole|plank|rope|mast|bar|rail|pipe|pillar|joist|rafter|"
                       r"spar|board|lantern|sign|flag)"),
    "parkour": re.compile(r"(?i)parkour"),
}


# Explicit traversal kit pieces, by NAME (observed placement names from the live
# Versailles inventory lead, 2026-11-14). A name match is an observation of the name only;
# it does not decode traversal behaviour.
TRAVERSAL_KIT = [
    ("anchor-climbing", re.compile(r"(?i)^GEN_ALL_AnchorClimbing_\d+")),
    ("beam", re.compile(r"(?i)^GEN_ALL_Beam_\d+m_\d+")),
    ("pole", re.compile(r"(?i)^GEN_ALL_Pole_\d+m_\d+")),
    ("window-ledge", re.compile(r"(?i)^GEN_ALL_WindowLedge_\d+m_\d+")),
    ("window-planter-ledge", re.compile(r"(?i)^GEN_ALL_WindowPlanter_Ledge_\d+m_\d+")),
    ("store-ledge", re.compile(r"(?i)^GEN_Store_Ledge.*_\d+m")),
    ("rope-climbing", re.compile(r"(?i)^GEN_ALL_RopeClimbing_\d+")),
    ("awning-ledge", re.compile(r"(?i)^GEN_ALL_AwningLedge\d+m_\d+")),
    ("balcony-wood-beam", re.compile(r"(?i)^GEN_ALL_BalconyWood_Beam_\d+m_\d+")),
]
PARKOUR_NAME = re.compile(r"(?i)_Parkour_")


def kit_family(stem):
    for family, pattern in TRAVERSAL_KIT:
        if pattern.search(stem or ""):
            return family
    return None


def size_of(box, scale):
    lo, hi = box["min"], box["max"]
    ext = [(hi[i] - lo[i]) * scale[i] for i in range(3)]
    return dict(x=ext[0], y=ext[1], z=ext[2])


def cmd_types(args):
    mesh_bounds = {}
    for path in OUT.glob("mesh-bounds-*.jsonl"):
        for line in path.read_text(encoding="utf-8").splitlines():
            row = json.loads(line)
            if "file_id" in row:
                mesh_bounds[row["file_id"]] = row

    types = {}
    kit = defaultdict(lambda: dict(instances=0, stems=Counter(), resource_types=Counter(),
                                   visual_types=Counter(), examples=[]))
    parkour = defaultdict(lambda: dict(instances=0, stems=Counter(), visual_types=Counter(),
                                       mesh_ref_counts=Counter(), components=Counter(),
                                       examples=[]))
    for row in iter_placements():
        family = kit_family(row["stem"])
        if family:
            k = kit[family]
            k["instances"] += 1
            k["stems"][row["stem"]] += 1
            k["resource_types"][row["resource_type"]] += 1
            for v in row["visual_refs"]:
                k["visual_types"][v["type_name"] or v["type"] or "not-in-index"] += 1
            if len(k["examples"]) < 5:
                k["examples"].append(dict(file_id=row["file_id"], name=row["name"],
                                          position=[round(v, 3) for v in row["position"]]))
        if PARKOUR_NAME.search(row["name"] or ""):
            q = parkour[row["resource_type"]]
            q["instances"] += 1
            q["stems"][row["stem"]] += 1
            for v in row["visual_refs"]:
                q["visual_types"][v["type_name"] or v["type"] or "not-in-index"] += 1
            q["mesh_ref_counts"][len(row["mesh_refs"])] += 1
            q["components"][row["component_count"]] += 1
            if len(q["examples"]) < 5:
                q["examples"].append(dict(file_id=row["file_id"], name=row["name"],
                                          position=[round(v, 3) for v in row["position"]],
                                          visual_refs=row["visual_refs"][:2],
                                          mesh_refs=[m["name"] for m in row["mesh_refs"][:3]]))
        mesh = lod0(row)
        visual = row["visual_refs"][0]["id"] if row["visual_refs"] else None
        if mesh or visual:
            key = f"{row['resource_type']}|visual={visual}|lod0={mesh['id'] if mesh else None}"
        else:
            key = f"{row['resource_type']}|stem={row['stem']}"
        t = types.get(key)
        if t is None:
            t = types[key] = dict(key=key, resource_type=row["resource_type"],
                                  visual_ref=row["visual_refs"][0] if row["visual_refs"] else None,
                                  lod0_mesh=mesh, stems=Counter(), instances=0,
                                  scales=[], entity_boxes=Counter(), rows2=[], example=None,
                                  positions=[])
        t["instances"] += 1
        t["stems"][row["stem"]] += 1
        t["scales"].append(row["scale"])
        t["rows2"].append(row["row2_deviation"])
        if row["bbox_local"]:
            box = row["bbox_local"]
            t["entity_boxes"][json.dumps([round(v, 3) for v in box["min"] + box["max"]])] += 1
        if t["example"] is None:
            t["example"] = dict(file_id=row["file_id"], name=row["name"], position=row["position"])
        if len(t["positions"]) < 5:
            t["positions"].append([round(v, 2) for v in row["position"]])

    out, flagged = [], Counter()
    for t in types.values():
        scales = t.pop("scales")
        median_scale = [statistics.median(s[i] for s in scales) for i in range(3)]
        tilted = sum(1 for d in t.pop("rows2") if d > 1e-3)
        entity_box = None
        if t["entity_boxes"]:
            text, n = t["entity_boxes"].most_common(1)[0]
            v = json.loads(text)
            entity_box = dict(min=v[:3], max=v[3:], instances_with_this_box=n,
                              distinct_boxes=len(t["entity_boxes"]))
        t.pop("entity_boxes")
        mesh_box = None
        if t["lod0_mesh"]:
            mb = mesh_bounds.get(t["lod0_mesh"]["id"])
            if mb and "bbox_min" in mb:
                mesh_box = dict(min=mb["bbox_min"], max=mb["bbox_max"], vertices=mb["vertices"],
                                archive_entry=mb["entry"], payload_sha256=mb["payload_sha256"])
            elif mb:
                mesh_box = dict(error=mb.get("error"))
        source = None
        box = None
        if mesh_box and "min" in mesh_box:
            box, source = mesh_box, "lod0-mesh-vertices"
        elif entity_box:
            box, source = entity_box, "entity-local-aabb"
        size = size_of(box, median_scale) if box else None
        flags = []
        stem_text = " ".join(t["stems"])
        families = sorted({kit_family(stem) for stem in t["stems"]} - {None})
        if families:
            flags += [f"traversal-kit-name:{f}" for f in families]
        if size:
            height = size["z"]
            depth, length = sorted((size["x"], size["y"]))
            size.update(height=height, depth=depth, length=length)
            if (VAULT_HEIGHT[0] <= height <= VAULT_HEIGHT[1] and depth <= VAULT_DEPTH_MAX
                    and t["resource_type"] == "Entity"):
                flags.append("vault-size")
            if length >= BEAM_LENGTH_MIN and depth <= BEAM_CROSS_MAX and height <= BEAM_CROSS_MAX:
                flags.append("beam-size")
        for hint, pattern in NAME_HINTS.items():
            if pattern.search(stem_text):
                flags.append(f"name:{hint}")
        for f in flags:
            flagged[f] += 1
        t.update(stems=dict(t["stems"].most_common(8)), median_scale=median_scale,
                 instances_tilted=tilted, entity_local_aabb=entity_box, lod0_mesh_aabb=mesh_box,
                 size_source=source, size_m_if_metres=size, inference_flags=flags)
        out.append(t)
    out.sort(key=lambda t: -t["instances"])
    vault = [t for t in out if "vault-size" in t["inference_flags"]]
    beam = [t for t in out if "beam-size" in t["inference_flags"]]
    doc = dict(
        schema_version=1,
        note=("Inference flags are size/name guesses for parkour obstacles, not decoded "
              "traversal data; confirm with the live scan before use."),
        thresholds=dict(vault_height_m=VAULT_HEIGHT, vault_depth_max_m=VAULT_DEPTH_MAX,
                        beam_length_min_m=BEAM_LENGTH_MIN, beam_cross_max_m=BEAM_CROSS_MAX),
        prop_types=len(out), flag_counts=flagged,
        vault_size_types=len(vault), vault_size_instances=sum(t["instances"] for t in vault),
        beam_size_types=len(beam), beam_size_instances=sum(t["instances"] for t in beam),
        size_sources=Counter(t["size_source"] for t in out),
        traversal_kit_by_name=kit_summary(kit),
        parkour_named_placements=kit_summary(parkour, stems=40),
        parkour_named_resources_in_index=parkour_index(),
        types=out,
    )
    (OUT / "prop-types.json").write_text(json.dumps(doc, indent=1), encoding="utf-8")
    print(json.dumps({k: doc[k] for k in ("prop_types", "flag_counts", "vault_size_types",
                                           "vault_size_instances", "beam_size_types",
                                           "beam_size_instances", "size_sources")}, indent=1))
    for t in vault[:25]:
        s = t["size_m_if_metres"]
        print(f"  {t['instances']:6d}  {list(t['stems'])[0]:50s} h={s['height']:.2f} "
              f"d={s['depth']:.2f} l={s['length']:.2f} {t['size_source']}")


def kit_summary(table, stems=12):
    out = {}
    for key, value in sorted(table.items()):
        value = dict(value)
        value["distinct_stems"] = len(value["stems"])
        for field in ("stems",):
            value[field] = dict(value[field].most_common(stems))
        out[key] = value
    return out


def parkour_index():
    """Every resource of this archive whose name contains `_Parkour_`, by type."""
    db = index_db()
    rows = db.execute("""SELECT r.type_id, COUNT(*), COUNT(DISTINCT r.file_id),
            COUNT(DISTINCT r.name), MIN(r.name) FROM resource r JOIN entry e ON e.id = r.entry_id
            JOIN archive a ON a.id = e.archive_id WHERE a.name = ? AND r.name LIKE '%!_Parkour!_%' ESCAPE '!'
            GROUP BY r.type_id ORDER BY 2 DESC""", (ARCHIVE,)).fetchall()
    return [dict(type=t, type_name=TYPE_NAMES.get(t), resources=n, distinct_ids=d,
                 distinct_names=dn, example=ex) for t, n, d, dn, ex in rows]


def cmd_near(args):
    """Placements within a radius of a point (for the live-frame comparison)."""
    rows = []
    for row in iter_placements():
        p = row["position"]
        d = math.dist(p, args.point)
        if d <= args.radius:
            rows.append((d, row))
    rows.sort(key=lambda r: r[0])
    for d, row in rows[:args.limit]:
        print(f"{d:7.2f}  {row['file_id']}  {row['name'][:56]:56s} "
              f"({row['position'][0]:.2f}, {row['position'][1]:.2f}, {row['position'][2]:.2f})")
    print(f"{len(rows)} placements within {args.radius} of {args.point}")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--archive", default="DataPC_ACU_Paris.forge",
                        help="world archive basename (default Paris; outputs go to "
                             "local/paris, other archives to local/world-<stem>)")
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("extract", "placements", "meshes", "types", "all"):
        sub.add_parser(name)
    near = sub.add_parser("near")
    near.add_argument("point", type=float, nargs=3)
    near.add_argument("--radius", type=float, default=20.0)
    near.add_argument("--limit", type=int, default=40)
    args = parser.parse_args(argv)
    select_archive(args.archive)
    steps = {"extract": [cmd_extract], "placements": [cmd_placements], "meshes": [cmd_meshes],
             "types": [cmd_types], "near": [cmd_near],
             "all": [cmd_extract, cmd_placements, cmd_meshes, cmd_types]}[args.command]
    for step in steps:
        step(args)


if __name__ == "__main__":
    main()
