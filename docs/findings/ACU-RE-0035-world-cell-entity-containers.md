# ACU-RE-0035 - World cell and entity containers decoded

Date: 2026-10-22. Related: #118 (reserved here), #117 (dumper), #4 world-data, ACU-RE-0034.
State: partially established. Two container layouts are decoded and verified on samples;
semantics of several fields remain unknown.

## Provenance

- Archive `DataPC_ACU_Paris.forge`, 3,850,403,840 B, 24,677 reachable entries.
  Census: `local/scan-paris/summary.json`, resource index
  `local/paris-resources-index.json` (2,199,415 resources, 134 types).
- Installed `ACU.exe` `748abdc6..`; no process was read for this finding.
- Tool: `acu-resource` (`acu-rs/crates/acu_assets/src/bin/acu-resource.rs`, new) -
  opens an entry, decodes the package, selects a resource by file id and prints or
  writes its payload. Read-only.
- Payload dumps and checks: `local/cell05201.bin`, `local/entity_sample.bin`,
  `local/entity-layout-check/`.

## GridCellDataBlock (`ac2bbf68`)

```text
u8   version            = 1
u64  self file id
u32  type id            = ac2bbf68
u32  child count
count x { u16 flags = 1, u64 child file id }        (10 bytes each)
u32  ?                  (114 for a 116-child cell, 0 for a 1-child cell)
u32  cell index         (0, 1, 100, 5201 in the samples)
u32  0
```

Verified arithmetically: `17 + count*10 + 12 == payload_length` on every sample.

| sample | payload | count | children | tail |
| --- | ---: | ---: | --- | --- |
| `Cell00000_DataBlock` | 39 | 1 | `00000012b0858881` (`ACU_Paris_0_0_Map`) | 0, 0, 0 |
| `Cell05201_DataBlock` | 1,189 | 116 | shadow bodies, `NavMeshManager_05201`, entities | 114, 5201, 0 |

The 71-byte *record* seen in the index is this same payload behind a 32-byte envelope.

**Two uses of one type.** 21,160 cells exist; 12,781 are exactly 71 bytes (one child, the
map tile texture `ACU_Paris_<n>_0_Map`, a `a2b7e917` TextureMap of 8,321 bytes), while
2,168 exceed 1,000 bytes and carry the 3D contents. So `GridCellDataBlock` names both a
minimap tile and a world streaming cell. Treating all cells as world cells would be wrong.

## Entity (`0984415e`)

```text
u8      version = 1
u64     self file id
u32     type id = 0984415e
u8      body flag = 3 in the sample
f32[16] affine matrix, row-major
u32     child count (3 in the sample)
...
```

Sample `FRV_CanalKit_RiverKit_26_008` (`00000010aea08064`, 4,016 B payload):

```text
row0   0.68087  -0.73241   0.0   0.0
row1   0.73241   0.68087   0.0   0.0
row2  -0.0       0.0       1.0   0.0
row3 553.87689 -747.87231   4.68882  1.0
```

Rows 0-2 are a rotation (about Z, approximately 47.1 degrees); row 3 is the position.
The last component is exactly `1.0`, so the block is affine in this sample. The numbers
are plausible Paris coordinates in metres.

## Observations vs inferences

- Observation: the field order and sizes above; the arithmetic check on cell payloads;
  the exact `1.0` in the last matrix slot; the map-tile/child naming
  (`Cell00052..` -> `ACU_Paris_52_0_Map` pattern held for 0, 1, 100).
- Inference: rows 0-2 are rotation and row 3 is translation, from their values and the
  `1.0`. The axis convention (whether row- or column-vector, Z-up or Y-up) is **not**
  established; do not transform a camera with it yet.
- Unknown: the `u8 body flag` (3), the `u16 flags` per cell child (always 1 so far), the
  first tail `u32` (114 vs 0), whether the entity's trailing `u32 3` is a child count, and
  what the following records reference. None of these are guessed at in code.

## Why it matters

The map is now addressable end to end at the container level:

`cell -> child ids -> Entity -> affine transform -> MeshShape/ReferenceListShape`

with entity names that are real placement names (`FRV_CanalKit_RiverKit_26_008`,
`FRV_Wall_WL_09_042`, `GEN_ALL_WallRock_Gate_01_011`). Cell 05201 alone references 116
children, of which 372 resources in that entry include shadow bodies, one navmesh manager
and dozens of entities.

## Reproduction

```powershell
.\acu-rs\target\debug\acu-resource.exe $env:ACU_GAME_DIR\DataPC_ACU_Paris.forge `
  --entry 5311 --fid 0000000f18ec6055 --len 220 --out local\cell05201.bin
python tools/paris_resource_index.py    # cell/type index from the census
python tools/verify_world_layout.py     # parses the three samples above
```

## Next step

Emit a cell as `{entities: [{file_id, name, position, rotation, children}]}` and spawn that
one cell in Bevy with the existing mesh pipeline, so the first Paris geometry is on screen
with its original identity. Nothing in that step may claim shading parity - the viewer
lighting is still synthetic (`shading_deltas.md:13-14`).
