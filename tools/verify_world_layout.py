"""Check the entity/cell container layout against real payloads.

Layout under test, from `Cell05201_DataBlock` and `FRV_CanalKit_RiverKit_26_008`:
  u8 version (=1), u64 self file id, u32 type id, then type-specific body.
  GridCellDataBlock body: u32 count, count x (u16 flags, u64 child id), 12-byte tail.
  Entity body: 16 x f32 matrix (row-major, last row is position/1.0), then children.

This script only asserts what it can see: version, self id, type id, and for entities
that the matrix is affine (last row w == 1.0).
"""
import json
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
EXE = ROOT / "acu-rs/target/debug/acu-resource.exe"
ARCHIVE = (r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Unity"
           r"\DataPC_ACU_Paris.forge")
ARCHIVE = r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity\DataPC_ACU_Paris.forge"
OUT = ROOT / "local/entity-layout-check"
OUT.mkdir(parents=True, exist_ok=True)

ENTITY_TYPE = "0984415e"
CELL_TYPE = "ac2bbf68"


def dump(entry, fid, tag):
    path = OUT / f"{tag}.json"
    result = subprocess.run([str(EXE), ARCHIVE, "--entry", str(entry), "--fid", fid,
                             "--len", "0", "--out", str(OUT / f"{tag}.bin")],
                            capture_output=True, text=True)
    if result.returncode != 0:
        return None, result.stderr.strip() or result.stdout.strip()
    return json.loads(result.stdout), None


def parse_header(payload):
    if not payload:
        return None
    version = payload[0]
    self_id = struct.unpack_from("<Q", payload, 1)[0]
    type_id = struct.unpack_from("<I", payload, 9)[0]
    return dict(version=version, self=f"{self_id:016x}", type=f"{type_id:08x}",
                payload_length=len(payload))


def parse_entity(payload):
    header = parse_header(payload)
    if header is None or header["type"] != ENTITY_TYPE:
        return header
    header["body_flag"] = payload[13]
    matrix = struct.unpack_from("<16f", payload, 14)
    header["matrix"] = [[round(matrix[row * 4 + col], 5) for col in range(4)] for row in range(4)]
    header["affine_last_row"] = (matrix[12], matrix[13], matrix[14], matrix[15])
    header["position"] = (round(matrix[12], 3), round(matrix[13], 3), round(matrix[14], 3))
    body_offset = 14 + 64
    header["body_u32_at_78"] = struct.unpack_from("<I", payload, body_offset)[0] if len(payload) >= body_offset + 4 else None
    return header


def parse_cell(payload):
    header = parse_header(payload)
    if header is None or header["type"] != CELL_TYPE:
        return header
    count = struct.unpack_from("<I", payload, 13)[0]
    expected = 17 + count * 10 + 12
    header["count"] = count
    header["fits"] = (expected == len(payload))
    children = []
    for index in range(min(count, 4)):
        base = 17 + index * 10
        flags, child = struct.unpack_from("<HQ", payload, base)
        children.append((flags, f"{child:016x}"))
    header["children"] = children
    if len(payload) >= 17 + count * 10 + 12:
        tail = struct.unpack_from("<III", payload, 17 + count * 10)
        header["tail"] = tail
    return header


CASES = [
    (5311, "0000000f18ec6055", "cell-05201"),
    (67, "0000000f18ec4bed", "cell-00000"),
    (5311, "00000010aea08064", "entity-river-008"),
]

results = []
for entry, fid, tag in CASES:
    report, error = dump(entry, fid, tag)
    if error:
        print(f"{tag}: DUMP FAILED {error[:200]}")
        continue
    binary = (OUT / f"{tag}.bin").read_bytes()
    parsed = parse_entity(binary) if report["resources"][0]["type_id"] == ENTITY_TYPE else parse_cell(binary)
    results.append(parsed)
    print(f"{tag}: {json.dumps(parsed)}")

print("\nsummary")
entities = [r for r in results if r and r["type"] == ENTITY_TYPE]
cells = [r for r in results if r and r["type"] == CELL_TYPE]
for entity in entities:
    print(f"  entity {entity['self']} position={entity['position']} "
          f"last_row={tuple(round(v, 4) for v in entity['affine_last_row'])}")
for cell in cells:
    print(f"  cell {cell['self']} count={cell['count']} fits={cell['fits']} tail={cell.get('tail')}")
