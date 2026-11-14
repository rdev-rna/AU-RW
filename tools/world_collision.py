#!/usr/bin/env python3
"""Offline world collision and ground height from the Forge archives (ACU-RE-0050).

No game process is read. Payloads are bulk-dumped by the read-only Rust bin
`acu-type-dump`; this script interprets them with the layouts in ACU-RE-0050.

Subcommands:

  extract   dump ShadowRigidBody (1cbde084) and ReferenceListShape (2d675ba2) from the world
            archive and MeshShape (b22b3e61) from the world archive and its *_assets*.forge
            companions into local/world-collision/<archive stem>/ (entry lists from the index)
  survey    parse every dumped payload, report layout counts and failures (summary.json)
  height X Y [--probe Z]  every collision surface on the vertical line through (X, Y)
  compare   ground height against live player positions (hop scans and live-state traces),
            written to data/oracles/ground_<world>.json with --write

Layouts (observed on every Versailles payload; see ACU-RE-0050 for counts):

  common header   u8 1, u64 self id, u32 type id                          (13 bytes)
  ShadowRigidBody u8 1 at 13, u8 link byte at 14 (0 or 1; meaning unknown, NOT the shape class),
                  u64 shape id at 15, u8 n at 35 (count of 25-byte records starting at 47,
                  tag e31593e1 in the samples), u32 tag 43ef99c2 at 47 + 25n,
                  f32[16] world matrix at 66 + 25n (row-major, row 3 = translation)
  ReferenceListShape  u32 0 at 13, u32 n, n x (u8 1, u8 0|1, u64 shape id), u32 n,
                  n x f32[16] child matrix, u32 n, n x 27 bytes, f32[8] AABB (min xyz 1, max xyz 1)
  MeshShape       12 bytes at 13, u32 at 25, u32 L at 29, Havok packfile (L bytes, magic
                  57e0e057 10c0c010, "hk_2013.3.0-r1", one hknpExternMeshShapeData) at 33;
                  then u8 per-triangle-material flag, u8, u32 nv, nv x f32[3] vertices,
                  u32 ni, ni x u16 indices (triangle list), u32 nm (0 or ni/3), nm x u8
                  material index, u32 k, k x 49-byte material record (u64 CollisionMaterial
                  id at +14), f32[8] AABB

Composition used (an implementation choice checked against live positions, see the finding):
  world = child(v) then body, each p' = x*row0 + y*row1 + z*row2 + row3.

Anything that does not match the layouts raises FormatError with a reason.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import json
import math
import mmap
from pathlib import Path
import sqlite3
import statistics
import struct
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
INDEX = ROOT / "local/asset_index.sqlite"
RUNS = ROOT / "local/native-probe/runs"

TYPE_BODY = 0x1CBDE084
TYPE_REFLIST = 0x2D675BA2
TYPE_MESHSHAPE = 0xB22B3E61
TAG_BODY_RECORD = 0x43EF99C2      # crc32("CollisionFilterInfo")
TAG_RIGID_BODY_BYTES = struct.pack("<I", 0x228F402A)  # crc32("RigidBody")
TAG_ENTITY_BYTES = struct.pack("<I", 0x0984415E)      # Entity
TAG_GROUP_BYTES = struct.pack("<I", 0x3F742D26)       # EntityGroup
TYPE_ENTITY = 0x0984415E
TYPE_GROUP = 0x3F742D26
PRIMITIVE_SHAPES = {0x4EC68E98: "BoxShape", 0x445B37F9: "CylinderShape",
                    0xB8599052: "CapsuleShape", 0xFA3F7A18: "SphereShape"}
HAVOK_MAGIC = bytes.fromhex("57e0e05710c0c010")
DUMP_MAGIC = b"ACUTD001"

MAX_VERTICES = 1_000_000
MAX_INDICES = 3_000_000
MAX_CHILDREN = 100_000
MAX_MATERIALS = 4096
MAX_BODY_RECORDS = 16
BODY_RECORD = 25
REFLIST_RECORD = 27
MATERIAL_RECORD = 49
AFFINE_TOLERANCE = 1e-4
GRID = 8.0  # metres per bucket of the body AABB index (implementation choice)


class FormatError(ValueError):
    pass


class PrimitiveShape(Exception):
    """A Box/Cylinder/Capsule/Sphere shape: identified, not decoded into triangles."""


# ------------------------------------------------------------------ parsing ---

def scan_dump(buffer):
    """Yield (entry, ordinal, file_id, type_id, name, start, length) from an ACUTD001 dump."""
    if buffer[:8] != DUMP_MAGIC:
        raise FormatError("not an ACUTD001 dump")
    pos, end = 8, len(buffer)
    while pos < end:
        if end - pos < 22:
            raise FormatError(f"truncated dump record header at {pos}")
        entry, ordinal, file_id, type_id, name_length = struct.unpack_from("<IIQIH", buffer, pos)
        pos += 22
        if end - pos < name_length + 4:
            raise FormatError(f"truncated dump name at {pos}")
        name = bytes(buffer[pos:pos + name_length]).decode("utf-8", "replace")
        pos += name_length
        (length,) = struct.unpack_from("<I", buffer, pos)
        pos += 4
        if end - pos < length:
            raise FormatError(f"truncated dump payload at {pos} ({length} B declared)")
        yield entry, ordinal, file_id, type_id, name, pos, length
        pos += length


def read_dump(data):
    """Yield (entry, ordinal, file_id, type_id, name, payload bytes) from an ACUTD001 dump."""
    for entry, ordinal, file_id, type_id, name, start, length in scan_dump(data):
        yield entry, ordinal, file_id, type_id, name, bytes(data[start:start + length])


class Reader:
    def __init__(self, data: bytes, pos: int = 0):
        self.data, self.pos = data, pos

    def take(self, fmt: str):
        size = struct.calcsize(fmt)
        if self.pos + size > len(self.data):
            raise FormatError(f"read of {size} B at {self.pos} past end {len(self.data)}")
        values = struct.unpack_from(fmt, self.data, self.pos)
        self.pos += size
        return values

    def count(self, limit: int, label: str) -> int:
        (n,) = self.take("<I")
        if n > limit:
            raise FormatError(f"{label} count {n} over limit {limit}")
        return n

    def skip(self, size: int):
        if self.pos + size > len(self.data):
            raise FormatError(f"skip of {size} B at {self.pos} past end")
        self.pos += size


def header(payload: bytes, expected: int) -> int:
    if len(payload) < 13:
        raise FormatError(f"payload {len(payload)} B shorter than the 13-byte header")
    if payload[0] != 1:
        raise FormatError(f"unsupported version {payload[0]}")
    self_id, type_id = struct.unpack_from("<QI", payload, 1)
    if type_id != expected:
        raise FormatError(f"type {type_id:08x}, expected {expected:08x}")
    return self_id


def affine(m) -> tuple:
    if any(not math.isfinite(v) for v in m):
        raise FormatError("non-finite matrix value")
    if abs(m[3]) > AFFINE_TOLERANCE or abs(m[7]) > AFFINE_TOLERANCE or \
            abs(m[11]) > AFFINE_TOLERANCE or abs(m[15] - 1.0) > AFFINE_TOLERANCE:
        raise FormatError("matrix column 3 is not (0, 0, 0, 1)")
    return tuple(m)


def aabb(r: Reader):
    v = r.take("<8f")
    if abs(v[3] - 1.0) > AFFINE_TOLERANCE or abs(v[7] - 1.0) > AFFINE_TOLERANCE:
        raise FormatError("AABB w components are not 1")
    return (v[0], v[1], v[2]), (v[4], v[5], v[6])


def parse_body(payload: bytes) -> dict:
    self_id = header(payload, TYPE_BODY)
    r = Reader(payload, 13)
    one, kind, shape = r.take("<BBQ")
    if one != 1 or kind not in (0, 1):
        raise FormatError(f"body bytes 13/14 = {one}/{kind}, expected 1 and 0|1")
    if len(payload) < 36:
        raise FormatError("body shorter than its record count")
    records = payload[35]
    if records > MAX_BODY_RECORDS:
        raise FormatError(f"body record count {records} over limit")
    tag_at = 47 + BODY_RECORD * records
    if len(payload) < tag_at + 19 + 64:
        raise FormatError("body shorter than its matrix")
    (tag,) = struct.unpack_from("<I", payload, tag_at)
    if tag != TAG_BODY_RECORD:
        raise FormatError(f"tag {tag:08x} at {tag_at}, expected {TAG_BODY_RECORD:08x}")
    matrix = affine(struct.unpack_from("<16f", payload, tag_at + 19))
    # Tail: u32 n, n x (u8 0, u64 Entity id) - the entities this body mirrors.
    r = Reader(payload, tag_at + 19 + 64)
    n = r.count(MAX_CHILDREN, "body entity")
    entities = []
    for _ in range(n):
        flag, entity = r.take("<BQ")
        if flag != 0:
            raise FormatError(f"body entity flag {flag}, expected 0")
        entities.append(entity)
    if r.pos != len(payload):
        raise FormatError(f"{len(payload) - r.pos} trailing bytes after the body entity list")
    return dict(id=self_id, kind=kind, shape=shape, records=records, matrix=matrix,
                filter=payload[tag_at + 4:tag_at + 19].hex(), entities=entities)


def parse_reflist(payload: bytes) -> dict:
    self_id = header(payload, TYPE_REFLIST)
    r = Reader(payload, 13)
    (zero,) = r.take("<I")
    if zero != 0:
        raise FormatError(f"reference list field at 13 = {zero}, expected 0")
    n = r.count(MAX_CHILDREN, "child")
    children = []
    for _ in range(n):
        a, b, child = r.take("<BBQ")
        if a != 1 or b not in (0, 1):
            raise FormatError(f"child flags {a}/{b}, expected 1 and 0|1")
        children.append(child)
    if r.count(MAX_CHILDREN, "matrix") != n:
        raise FormatError("matrix count differs from child count")
    matrices = [affine(r.take("<16f")) for _ in range(n)]
    if r.count(MAX_CHILDREN, "record") != n:
        raise FormatError("record count differs from child count")
    r.skip(REFLIST_RECORD * n)
    box = aabb(r)
    if r.pos != len(payload):
        raise FormatError(f"{len(payload) - r.pos} trailing bytes")
    return dict(id=self_id, children=list(zip(children, matrices)), aabb=box)


def parse_mesh_shape(payload: bytes) -> dict:
    self_id = header(payload, TYPE_MESHSHAPE)
    if len(payload) < 41:
        raise FormatError("mesh shape shorter than its packfile header")
    havok_length = struct.unpack_from("<I", payload, 29)[0]
    if payload[33:41] != HAVOK_MAGIC:
        raise FormatError("no Havok packfile magic at 33")
    r = Reader(payload, 33)
    r.skip(havok_length)
    material_flag, second = r.take("<BB")
    nv = r.count(MAX_VERTICES, "vertex")
    vertices = [r.take("<3f") for _ in range(nv)]
    ni = r.count(MAX_INDICES, "index")
    if ni % 3:
        raise FormatError(f"index count {ni} is not a triangle list")
    indices = r.take(f"<{ni}H") if ni else ()
    if indices and max(indices) >= nv:
        raise FormatError(f"index {max(indices)} out of range ({nv} vertices)")
    nm = r.count(MAX_INDICES, "triangle material")
    if nm not in (0, ni // 3):
        raise FormatError(f"{nm} triangle materials for {ni // 3} triangles")
    tri_materials = bytes(r.take(f"<{nm}s")[0]) if nm else b""
    k = r.count(MAX_MATERIALS, "material")
    materials = []
    for _ in range(k):
        record = r.take(f"<{MATERIAL_RECORD}s")[0]
        materials.append(struct.unpack_from("<Q", record, 14)[0])
    if tri_materials and max(tri_materials) >= k:
        raise FormatError(f"triangle material {max(tri_materials)} out of range ({k})")
    box = aabb(r)
    if r.pos != len(payload):
        raise FormatError(f"{len(payload) - r.pos} trailing bytes")
    for v in vertices:
        if not all(math.isfinite(c) for c in v):
            raise FormatError("non-finite vertex")
    triangles = [indices[i:i + 3] for i in range(0, ni, 3)]
    return dict(id=self_id, havok_length=havok_length, material_flag=material_flag,
                second=second, vertices=vertices, triangles=triangles,
                tri_materials=tri_materials, materials=materials, aabb=box)


def parse_rigid_bodies(payload: bytes):
    """RigidBody components inside an Entity / EntityGroup payload (tag-anchored scan).

    Yields dicts: offset, kind (the link byte, 0|1, meaning unknown), shape,
    'local' = the f32[16] at CollisionFilterInfo + 46, owner (offset of the nearest preceding
    Entity or EntityGroup type tag) and the owner's world matrix; or 'unsupported' with a reason.
    The +46 block is NOT applied to the shape: its 3x3 part is diagonal or symmetric and its
    row 3 is zero or small (an inertia tensor and centre of mass by inference), and applying it
    puts decal shapes 4x too large (ACU-RE-0050).
    """
    pos = 0
    while (pos := payload.find(TAG_RIGID_BODY_BYTES, pos)) >= 0:
        here, pos = pos, pos + 4
        filter_at = here + 12
        if len(payload) < filter_at + 46 + 64:
            yield dict(offset=here, unsupported="truncated rigid body")
            continue
        if payload[here + 4:here + 12] != bytes(8) or \
                struct.unpack_from("<I", payload, filter_at)[0] != TAG_BODY_RECORD:
            yield dict(offset=here, unsupported="no CollisionFilterInfo 12 bytes after RigidBody")
            continue
        variant = payload[filter_at + 4]
        one, kind, shape = struct.unpack_from("<BBQ", payload, filter_at + 19)
        if one != 1 or kind not in (0, 1):
            yield dict(offset=here, unsupported=f"variant {variant}: shape flags {one}/{kind}")
            continue
        try:
            local = affine(struct.unpack_from("<16f", payload, filter_at + 46))
        except FormatError as error:
            yield dict(offset=here, unsupported=f"variant {variant}: {error}")
            continue
        owner = max(payload.rfind(TAG_ENTITY_BYTES, 0, here), payload.rfind(TAG_GROUP_BYTES, 0, here))
        world = None
        if owner >= 9 and len(payload) > owner + 4:
            flag = payload[owner + 4]
            try:
                if flag == 3:
                    world = affine(struct.unpack_from("<16f", payload, owner + 5))
                elif flag == 0 and owner == 9:
                    world = affine(struct.unpack_from("<16f", payload, 115))
            except (FormatError, struct.error):
                world = None
        if world is None:
            yield dict(offset=here, unsupported=f"owner at {owner} has no supported world transform")
            continue
        yield dict(offset=here, kind=kind, shape=shape, local=local, owner=owner, world=world,
                   inline=owner != 9, filter=payload[filter_at + 4:filter_at + 19].hex(),
                   params=payload[filter_at + 29:filter_at + 46].hex())


def apply(m, p):
    x, y, z = p
    return (x * m[0] + y * m[4] + z * m[8] + m[12],
            x * m[1] + y * m[5] + z * m[9] + m[13],
            x * m[2] + y * m[6] + z * m[10] + m[14])


def compose(child, parent):
    """Matrix of 'child then parent' for the row-vector convention p' = p*M."""
    out = []
    for i in range(4):
        row = child[4 * i:4 * i + 4]
        for j in range(4):
            out.append(sum(row[k] * parent[4 * k + j] for k in range(4)))
    return tuple(out)


def box_corners(box):
    (a, b, c), (d, e, f) = box
    return [(x, y, z) for x in (a, d) for y in (b, e) for z in (c, f)]


def vertical_hits(tri, x, y):
    """z of the vertical line through (x, y) on triangle tri, and the face normal z, or None."""
    (ax, ay, az), (bx, by, bz), (cx, cy, cz) = tri
    det = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
    if abs(det) < 1e-9:
        return None  # vertical or degenerate in plan view
    l1 = ((by - cy) * (x - cx) + (cx - bx) * (y - cy)) / det
    l2 = ((cy - ay) * (x - cx) + (ax - cx) * (y - cy)) / det
    l3 = 1.0 - l1 - l2
    eps = -1e-7
    if l1 < eps or l2 < eps or l3 < eps:
        return None
    z = l1 * az + l2 * bz + l3 * cz
    ux, uy, uz = bx - ax, by - ay, bz - az
    vx, vy, vz = cx - ax, cy - ay, cz - az
    nx, ny, nz = uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx
    length = math.sqrt(nx * nx + ny * ny + nz * nz) or 1.0
    return z, abs(nz) / length


# ---------------------------------------------------------------- archives ---

def game_dir() -> Path:
    sys.path.insert(0, str(ROOT / "tools"))
    from acu import settings  # noqa: E402
    return Path(settings()[1]["game_dir"])


def find_bin(name: str) -> Path:
    for folder in (ROOT / "acu-rs/target/release", ROOT / "acu-rs/target/debug"):
        for candidate in (folder / f"{name}.exe", folder / name):
            if candidate.exists():
                return candidate
    raise SystemExit(f"{name} not built; run in acu-rs/: cargo build --locked -p acu_assets --bin {name}")


def index_db():
    if not INDEX.exists():
        raise SystemExit("asset index missing; run python tools/index_status.py --fetch")
    return sqlite3.connect(f"file:{INDEX.as_posix()}?mode=ro", uri=True)


def out_dir(archive: str) -> Path:
    if Path(archive).name != archive or not archive.endswith(".forge"):
        raise SystemExit("--archive must be a .forge basename")
    return ROOT / "local/world-collision" / Path(archive).stem


def shape_archives(db, archive: str) -> list[str]:
    stem = Path(archive).stem
    rows = db.execute("SELECT name FROM archive WHERE name = ? OR name LIKE ? ESCAPE '\\' ORDER BY name",
                      (archive, f"{stem}\\_assets%.forge")).fetchall()
    return [r[0] for r in rows if r[0] == archive or r[0].startswith(f"{stem}_assets")]


def entries_with(db, archive, types):
    marks = ",".join("?" * len(types))
    rows = db.execute(f"""SELECT DISTINCT e.idx FROM resource r JOIN entry e ON e.id = r.entry_id
        JOIN archive a ON a.id = e.archive_id WHERE a.name = ? AND r.type_id IN ({marks})
        ORDER BY e.idx""", (archive, *types)).fetchall()
    return [r[0] for r in rows]


def dump(archive: str, types: list[str], out: Path, db) -> dict:
    entries = entries_with(db, archive, types)
    if not entries:
        return {"archive": archive, "entries": 0, "resources_written": 0}
    listing = out.with_suffix(".entries.txt")
    listing.write_text("\n".join(map(str, entries)) + "\n")
    command = [find_bin("acu-type-dump"), game_dir() / archive, "--types", ",".join(types),
               "--out", out, "--entries", listing]
    print("+", " ".join(str(c) for c in command), flush=True)
    result = subprocess.run([str(c) for c in command], capture_output=True, text=True)
    if result.returncode != 0:
        raise SystemExit(f"acu-type-dump failed: {result.stderr.strip()[:500]}")
    summary = json.loads(result.stdout)
    if summary.get("failed_entries"):
        raise SystemExit(f"failed entries in {archive}: {summary['failed_entries'][:5]}")
    return summary


def cmd_extract(args):
    out = out_dir(args.archive)
    out.mkdir(parents=True, exist_ok=True)
    db = index_db()
    report = {"world": dump(args.archive, ["1cbde084", "2d675ba2", "4ec68e98", "445b37f9",
                                            "b8599052", "fa3f7a18"], out / "world-dump.bin", db),
              "entities": dump(args.archive, ["0984415e", "3f742d26"], out / "entity-dump.bin", db)}
    for archive in shape_archives(db, args.archive):
        report[archive] = dump(archive, ["b22b3e61"], out / f"shapes-{Path(archive).stem}.bin", db)
    (out / "extract.json").write_text(json.dumps(report, indent=1))
    print(json.dumps({k: v.get("resources_written") for k, v in report.items()}, indent=1))


# ------------------------------------------------------------------- world ---

class World:
    """Bodies, reference lists and mesh shapes of one world archive (lazy shape parsing).

    Dumps are memory-mapped; mesh shapes are parsed on first use. `bounds` (xmin, ymin, xmax,
    ymax) keeps only instances whose world AABB overlaps it, to bound memory on Paris.
    """

    def __init__(self, archive: str, bounds=None):
        self.archive = archive
        self.bounds = bounds
        self.dir = out_dir(archive)
        if not (self.dir / "world-dump.bin").exists():
            raise SystemExit(f"no dump in {self.dir}; run extract first")
        self.stats = Counter()
        self.unsupported = Counter()
        self.failures = []
        self.maps = []
        self.bodies, self.reflists, self.primitives = {}, {}, {}
        for entry, ordinal, fid, tid, name, payload in read_dump(self.map("world-dump.bin")):
            try:
                if tid == TYPE_BODY:
                    body = parse_body(payload)
                    body.update(entry=entry, name=name)
                    self.stats["body copies"] += 1
                    previous = self.bodies.get(fid)
                    if previous and previous["matrix"] != body["matrix"]:
                        self.stats["body copies differing"] += 1
                    self.bodies.setdefault(fid, body)
                elif tid in PRIMITIVE_SHAPES:
                    self.primitives[fid] = PRIMITIVE_SHAPES[tid]
                elif tid == TYPE_REFLIST:
                    self.reflists.setdefault(fid, parse_reflist(payload))
                    self.stats["reference lists"] += 1
            except FormatError as error:
                self.failures.append({"file_id": f"{fid:016x}", "type": f"{tid:08x}", "error": str(error)})
        # Shapes: base archive before _assets before patches (sorted name order). Which copy
        # the game loads is not established (#35); differing copies are counted.
        self.raw_shapes = {}
        self.shape_source = {}
        for path in sorted(self.dir.glob("shapes-*.bin")):
            buffer = self.map(path.name)
            for entry, ordinal, fid, tid, name, start, length in scan_dump(buffer):
                if fid in self.raw_shapes:
                    old_buffer, old_start, old_length = self.raw_shapes[fid]
                    if old_buffer[old_start:old_start + old_length] != buffer[start:start + length]:
                        self.stats["mesh shape copies differing"] += 1
                    continue
                self.raw_shapes[fid] = (buffer, start, length)
                self.shape_source[fid] = (path.stem[len("shapes-"):], entry, name)
        # RigidBody components of placed entities and groups (inline entities included).
        self.entity_bodies = []
        if (self.dir / "entity-dump.bin").exists():
            buffer, seen = self.map("entity-dump.bin"), set()
            for entry, ordinal, fid, tid, name, start, length in scan_dump(buffer):
                if fid in seen:
                    self.stats["entity copies skipped"] += 1
                    continue
                seen.add(fid)
                payload = buffer[start:start + length]
                if TAG_RIGID_BODY_BYTES not in payload:
                    continue
                for rb in parse_rigid_bodies(payload):
                    if "unsupported" in rb:
                        self.stats["entity rigid bodies unsupported"] += 1
                        self.unsupported[rb["unsupported"].split(":")[0]] += 1
                        continue
                    rb.update(entity=fid, entity_name=name)
                    self.entity_bodies.append(rb)
                    self.stats["entity rigid bodies (inline)" if rb["inline"] else "entity rigid bodies"] += 1
        self.shapes = {}
        self.instances = None

    def map(self, name):
        with open(self.dir / name, "rb") as handle:
            buffer = mmap.mmap(handle.fileno(), 0, access=mmap.ACCESS_READ)
        self.maps.append(buffer)
        return buffer

    def raw(self, fid):
        ref = self.raw_shapes.get(fid)
        if ref is None:
            return None
        buffer, start, length = ref
        return buffer[start:start + length]

    def shape_aabb(self, fid):
        """The trailing f32[8] AABB of a MeshShape, read without parsing the whole shape."""
        buffer, start, length = self.raw_shapes[fid]
        if length < 45:
            raise FormatError("mesh shape too short for its AABB")
        return aabb(Reader(buffer[start + length - 32:start + length]))

    def shape(self, fid):
        if fid not in self.shapes:
            raw = self.raw(fid)
            self.shapes[fid] = parse_mesh_shape(raw) if raw is not None else None
        return self.shapes[fid]

    def resolve(self, kind, shape, matrix, depth=0):
        """[(MeshShape id, world matrix)] for a shape reference, by the referenced type.

        The link byte (`kind`) is not used: in Paris, children with byte 0 are MeshShapes too.
        """
        if shape in self.raw_shapes:
            return [(shape, matrix)]
        if shape in self.primitives:
            raise PrimitiveShape(self.primitives[shape])
        reflist = self.reflists.get(shape)
        if reflist is None:
            raise FormatError(f"shape {shape:016x} is not a dumped MeshShape, reference list or primitive")
        if depth >= 4:
            raise FormatError(f"reference list nesting deeper than 4 at {shape:016x}")
        parts = []
        for child, m in reflist["children"]:
            try:
                parts += self.resolve(None, child, compose(m, matrix), depth + 1)
            except PrimitiveShape as primitive:
                self.stats[f"reference-list child primitive {primitive} (not decoded)"] += 1
            except FormatError as error:
                self.stats["reference-list child unresolved"] += 1
                self.failures.append({"file_id": f"{child:016x}", "source": "ReferenceListShape child",
                                      "error": str(error)})
        return parts

    def build_index(self):
        self.instances, self.grid = [], defaultdict(list)
        sources = [("ShadowRigidBody", fid, body["kind"], body["shape"], body["matrix"], body["filter"])
                   for fid, body in self.bodies.items()]
        sources += [("Entity.RigidBody", rb["entity"], rb["kind"], rb["shape"],
                     rb["world"], rb["filter"] + "/" + rb["params"])
                    for rb in self.entity_bodies]
        for source, fid, kind, shape_ref, matrix, filter_bytes in sources:
            try:
                parts = self.resolve(kind, shape_ref, matrix)
            except PrimitiveShape as primitive:
                self.stats[f"{source} primitive {primitive} (not decoded)"] += 1
                continue
            except FormatError as error:
                self.stats[f"{source} unresolved"] += 1
                self.failures.append({"file_id": f"{fid:016x}", "source": source, "error": str(error)})
                continue
            for shape_id, world in parts:
                if shape_id not in self.raw_shapes:
                    self.stats[f"{source} shape not a dumped MeshShape"] += 1
                    continue
                try:
                    box = self.shape_aabb(shape_id)
                except FormatError as error:
                    self.stats["mesh shapes failed"] += 1
                    self.failures.append({"file_id": f"{shape_id:016x}", "type": "b22b3e61", "error": str(error)})
                    continue
                corners = [apply(world, c) for c in box_corners(box)]
                lo = [min(c[i] for c in corners) for i in range(3)]
                hi = [max(c[i] for c in corners) for i in range(3)]
                if self.bounds and (hi[0] < self.bounds[0] or lo[0] > self.bounds[2]
                                    or hi[1] < self.bounds[1] or lo[1] > self.bounds[3]):
                    self.stats[f"{source} instances outside bounds"] += 1
                    continue
                n = len(self.instances)
                self.instances.append(dict(source=source, body=fid, shape=shape_id, matrix=world,
                                           filter=filter_bytes, lo=lo, hi=hi, tris=None))
                self.stats[f"{source} instances"] += 1
                for gx in range(math.floor(lo[0] / GRID), math.floor(hi[0] / GRID) + 1):
                    for gy in range(math.floor(lo[1] / GRID), math.floor(hi[1] / GRID) + 1):
                        self.grid[(gx, gy)].append(n)

    def column(self, x, y):
        """Every collision surface crossing the vertical line through (x, y), top first."""
        if self.instances is None:
            self.build_index()
        hits = []
        for n in self.grid.get((math.floor(x / GRID), math.floor(y / GRID)), ()):
            inst = self.instances[n]
            if not (inst["lo"][0] <= x <= inst["hi"][0] and inst["lo"][1] <= y <= inst["hi"][1]):
                continue
            try:
                shape = self.shape(inst["shape"])
            except FormatError as error:
                self.stats["mesh shapes failed"] += 1
                self.failures.append({"file_id": f"{inst['shape']:016x}", "type": "b22b3e61", "error": str(error)})
                inst["lo"], inst["hi"] = [math.inf] * 3, [-math.inf] * 3
                continue
            if inst["tris"] is None:
                world = [apply(inst["matrix"], v) for v in shape["vertices"]]
                inst["tris"] = [(world[a], world[b], world[c]) for a, b, c in shape["triangles"]]
            for t, tri in enumerate(inst["tris"]):
                if not (min(p[0] for p in tri) <= x <= max(p[0] for p in tri)
                        and min(p[1] for p in tri) <= y <= max(p[1] for p in tri)):
                    continue
                hit = vertical_hits(tri, x, y)
                if hit is None:
                    continue
                mat_index = shape["tri_materials"][t] if shape["tri_materials"] else 0
                material = shape["materials"][mat_index] if shape["materials"] else None
                hits.append(dict(z=hit[0], normal_z=hit[1], source=inst["source"], filter=inst["filter"],
                                 body=f"{inst['body']:016x}",
                                 shape=f"{inst['shape']:016x}", shape_name=self.shape_source[inst["shape"]][2],
                                 material=f"{material:016x}" if material is not None else None))
        hits.sort(key=lambda h: -h["z"])
        return hits


def support(hits, probe_z, step):
    """Highest surface at or below probe_z + step (the 'settle from just above' query)."""
    below = [h for h in hits if h["z"] <= probe_z + step]
    return below[0] if below else None


# ---------------------------------------------------------------- commands ---

def cmd_survey(args):
    world = World(args.archive, args.bounds)
    shape_stats = Counter()
    for fid in world.raw_shapes:
        try:
            s = world.shape(fid)
            shape_stats["parsed"] += 1
            shape_stats[f"material flag {s['material_flag']} per-triangle {bool(s['tri_materials'])}"] += 1
            shape_stats["triangles"] += len(s["triangles"])
            if s["vertices"]:
                lo = [min(v[i] for v in s["vertices"]) for i in range(3)]
                hi = [max(v[i] for v in s["vertices"]) for i in range(3)]
                dev = max(max(abs(a - b) for a, b in zip(lo, s["aabb"][0])),
                          max(abs(a - b) for a, b in zip(hi, s["aabb"][1])))
                shape_stats["aabb equals vertex bounds (1 mm)" if dev <= 1e-3 else "aabb differs"] += 1
        except FormatError as error:
            shape_stats["failed"] += 1
            world.failures.append({"file_id": f"{fid:016x}", "type": "b22b3e61", "error": str(error)})
    world.build_index()
    shape_types = Counter()
    for rb in world.entity_bodies:
        shape_types["MeshShape" if rb["shape"] in world.raw_shapes else
                    "ReferenceListShape" if rb["shape"] in world.reflists else
                    world.primitives.get(rb["shape"], "not in the dumped archives")] += 1
    kinds = Counter(f"link byte {b['kind']}: " + ("MeshShape" if b["shape"] in world.raw_shapes else
                    "ReferenceListShape" if b["shape"] in world.reflists else
                    world.primitives.get(b["shape"], "not dumped")) for b in world.bodies.values())
    records = Counter(b["records"] for b in world.bodies.values())
    summary = dict(archive=args.archive, bodies=len(world.bodies), body_shape_kind=kinds,
                   body_records=records, reference_lists=len(world.reflists),
                   mesh_shapes=len(world.raw_shapes), mesh_shape_stats=shape_stats,
                   entity_rigid_body_shapes=shape_types, entity_rigid_body_unsupported=world.unsupported,
                   stats=world.stats, failures=world.failures[:50], failure_count=len(world.failures))
    (world.dir / "summary.json").write_text(json.dumps(summary, indent=1, default=dict))
    print(json.dumps(summary, indent=1, default=dict))


def cmd_height(args):
    world = World(args.archive, args.bounds or (args.x - 1, args.y - 1, args.x + 1, args.y + 1))
    hits = world.column(args.x, args.y)
    for h in hits:
        print(f"z={h['z']:9.3f}  nz={h['normal_z']:.3f}  body {h['body']}  shape {h['shape']} "
              f"{h['shape_name']}  material {h['material']}")
    if args.probe is not None:
        s = support(hits, args.probe, args.step)
        print("support:", None if s is None else round(s["z"], 4))


def live_points(run_names):
    """Settled player positions: hop scan rows, and live-state samples whose z is unchanged
    (within 5 mm) over three consecutive samples, deduplicated on a 0.25 m / 0.05 m grid."""
    points, seen = [], set()

    def add(source, p, clip=None):
        key = (round(p[0] * 4), round(p[1] * 4), round(p[2] * 20))
        if key not in seen:
            seen.add(key)
            points.append(dict(source=source, position=[round(c, 4) for c in p], clip=clip))

    for name in run_names:
        folder = RUNS / name
        hops = folder / "hops.jsonl"
        if hops.exists():
            for line in hops.read_text().splitlines():
                row = json.loads(line)
                if "at" in row:
                    add(f"{name}/hops.jsonl#{row.get('hop')}", row["at"])
        for trace in sorted(folder.glob("*.jsonl")):
            if trace.name in ("hops.jsonl", "map.jsonl", "keep-safe.jsonl"):
                continue
            window = []
            for line in trace.read_text().splitlines():
                try:
                    row = json.loads(line)
                except json.JSONDecodeError:
                    continue
                p = row.get("position")
                if row.get("kind") != "sample" or not p:
                    continue
                window = (window + [p])[-3:]
                if len(window) == 3 and max(q[2] for q in window) - min(q[2] for q in window) <= 0.005:
                    clips = row.get("clips") or []
                    clip = max(clips, key=lambda c: c.get("weight", 0))["file_id"] if clips else None
                    add(f"{name}/{trace.name}", p, clip)
    return points


WORLD_RUNS = {
    "DataPC_ACU_Versailles.forge": ["hop-versailles", "catalog-1", "nav-1", "prop-probe-1",
                                    "vault-scan-1", "vault-scan-2", "vault-scan-3",
                                    "window-sweep-1", "window-sweep-2"],
    "DataPC_ACU_Paris.forge": ["hop-paris-1", "hop-paris-2"],
}


def on_foot(clip_name):
    """Dominant clip is ground locomotion: xx_Nav_* / acu_nav_*, not a wall-brace move."""
    if not clip_name:
        return False
    lowered = clip_name.lower()
    return (lowered.startswith("xx_nav_") or lowered.startswith("acu_nav_")) and "wallbrace" not in lowered


def cmd_compare(args):
    world = World(args.archive, args.bounds)
    runs = args.runs or WORLD_RUNS.get(args.archive, [])
    points = live_points(runs)
    db, names = index_db(), {}
    for point in points:
        clip = point.get("clip")
        if clip and clip not in names:
            row = db.execute("SELECT name FROM resource WHERE file_id = ? AND name IS NOT NULL LIMIT 1",
                             (clip,)).fetchone()
            names[clip] = row[0] if row else None
        point["clip_name"] = names.get(clip) if clip else None
        point["on_foot"] = on_foot(point["clip_name"]) or "/hops.jsonl#" in point["source"]
    if args.limit:
        points = points[:args.limit]
    rows = []
    for i, point in enumerate(points):
        x, y, z = point["position"]
        hits = world.column(x, y)
        top = hits[0] if hits else None
        s = support(hits, z, args.step)
        nearest = min(hits, key=lambda h: abs(h["z"] - z)) if hits else None
        rows.append(dict(point, surfaces=len(hits),
                         top_z=None if top is None else round(top["z"], 4),
                         support_z=None if s is None else round(s["z"], 4),
                         support_error=None if s is None else round(z - s["z"], 4),
                         support_normal_z=None if s is None else round(s["normal_z"], 4),
                         support_shape=None if s is None else s["shape"],
                         support_shape_name=None if s is None else s["shape_name"],
                         support_body=None if s is None else s["body"],
                         support_source=None if s is None else s["source"],
                         support_filter=None if s is None else s["filter"],
                         support_material=None if s is None else s["material"],
                         nearest_error=None if nearest is None else round(z - nearest["z"], 4)))
        if (i + 1) % 500 == 0:
            print(f"{i + 1}/{len(points)}", flush=True)

    def stats(values):
        values = [abs(v) for v in values if v is not None]
        if not values:
            return dict(count=0)
        values.sort()
        return dict(count=len(values), median_abs=round(statistics.median(values), 4),
                    p90_abs=round(values[int(0.9 * (len(values) - 1))], 4), max_abs=round(values[-1], 4),
                    within_0_05=sum(v <= 0.05 for v in values), within_0_25=sum(v <= 0.25 for v in values))

    foot = [r["support_error"] for r in rows if r["on_foot"]]
    by_source = defaultdict(list)
    for r in rows:
        by_source[r["source"].split("#")[0]].append(r["support_error"])
    signed = [r["support_error"] for r in rows if r["support_error"] is not None]
    result = dict(
        schema="acu-oracle/ground/1",
        finding="ACU-RE-0050",
        binary_sha256="748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40",
        world_archive=args.archive,
        shape_archives=sorted({src[0] for src in world.shape_source.values()}),
        asset_index_version=index_version(db),
        bounds=args.bounds,
        command="python tools/world_collision.py " + " ".join(sys.argv[1:]),
        method=("vertical line through each live player position against every MeshShape triangle "
                "placed by a ShadowRigidBody or by an Entity/EntityGroup RigidBody component of the "
                "world archive (shapes from the archive and its _assets companions; primitives not "
                "decoded); support = highest surface at or below player z + step"),
        step_m=args.step,
        live_runs=runs,
        live_point_rule=("hop scan 'at' rows, and live-state samples whose z changed by at most "
                         "5 mm over three consecutive samples; deduplicated on 0.25 m xy / 0.05 m z"),
        points=len(rows),
        no_support=sum(r["support_z"] is None for r in rows),
        support_error=stats(signed),
        support_error_signed_median=round(statistics.median(signed), 4) if signed else None,
        nearest_error=stats([r["nearest_error"] for r in rows]),
        on_foot_rule=("dominant clip name starts xx_Nav_ or acu_nav_ and has no WallBrace, or the "
                      "point is a hop-scan settled position"),
        on_foot_points=len(foot),
        on_foot_no_support=sum(e is None for e in foot),
        on_foot_support_error=stats(foot),
        on_foot_signed_median=round(statistics.median([e for e in foot if e is not None]), 4)
        if any(e is not None for e in foot) else None,
        per_source={k: stats(v) for k, v in sorted(by_source.items())},
        support_materials=Counter(r["support_material"] for r in rows if r["support_material"]),
        dataset=dict(bodies=len(world.bodies), reference_lists=len(world.reflists),
                     mesh_shapes=len(world.raw_shapes), stats=world.stats,
                     failure_count=len(world.failures)),
        row_columns=ROW_COLUMNS,
        rows=[[r.get(c) for c in ROW_COLUMNS] for r in rows],
    )
    summary = {k: v for k, v in result.items() if k not in ("rows", "row_columns")}
    print(json.dumps(summary, indent=1, default=dict))
    if args.write:
        target = Path(args.write)
        target.parent.mkdir(parents=True, exist_ok=True)
        head = json.dumps({k: v for k, v in result.items() if k != "rows"}, indent=1, default=dict)
        body = ",\n".join("  " + json.dumps(row, separators=(",", ":")) for row in result["rows"])
        target.write_text(head[:-2] + ',\n "rows": [\n' + body + "\n ]\n}\n")
        json.loads(target.read_text())  # the hand-joined file must still be valid JSON
        print("wrote", target)


ROW_COLUMNS = ["source", "position", "clip", "clip_name", "on_foot", "surfaces", "top_z",
               "support_z", "support_error", "support_normal_z", "support_source", "support_shape",
               "support_body", "support_material", "nearest_error"]


def index_version(db):
    row = db.execute("SELECT value FROM meta WHERE key = 'index_version'").fetchone()
    return row[0] if row else None


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--archive", default="DataPC_ACU_Versailles.forge")
    parser.add_argument("--bounds", type=float, nargs=4, metavar=("XMIN", "YMIN", "XMAX", "YMAX"),
                        help="keep only collision instances overlapping this xy box")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("extract")
    sub.add_parser("survey")
    h = sub.add_parser("height")
    h.add_argument("x", type=float)
    h.add_argument("y", type=float)
    h.add_argument("--probe", type=float)
    h.add_argument("--step", type=float, default=0.5)
    c = sub.add_parser("compare")
    c.add_argument("--runs", nargs="*")
    c.add_argument("--step", type=float, default=0.5)
    c.add_argument("--limit", type=int, default=0)
    c.add_argument("--write")
    args = parser.parse_args(argv)
    {"extract": cmd_extract, "survey": cmd_survey, "height": cmd_height, "compare": cmd_compare}[args.command](args)


if __name__ == "__main__":
    main()
