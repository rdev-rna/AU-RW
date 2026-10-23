# 137 - Whole-map pass: 8,330 cells, instanced

Issue: #137 (`type:slice`, `area:world-data`). Branch: `acu-137-whole-map`, stacked on
`acu-135-selector-lod-chain` (#136, ACU-RE-0037).

## How the game loads the map (measured through the asset index)

`DataPC_ACU_Paris.forge` entry 1 holds one `World` resource, `ACU_Paris`
(`0000000f18b7e1cf`, type `fbb63e47` = CRC-32 of `World`). Its 50,083 candidate links
resolve to:

| destination | count | meaning |
| --- | --- | --- |
| `GridCellDataBlock` (`ac2bbf68`) | 21,160 | every cell package, including the minimap cells |
| `LocalCubeMap_Cell_N_<hash>` (`eebb2443`) | 23,019 | per-cell reflection cubemaps |
| `CellID=NNNN` (`5730d30e`) | 2,811 | per-cell streaming metadata |
| seasonal sets (`41813744`: Summer/Autumn/Winter/Spring/Night) | 2,955 | lighting/climate variants |
| `WorldTransitionPortal` (`7c00f6af`) | 986 | inter-world transitions |
| `InventoryItemSettings`, `Entity`, `EntityGroup`, `BulkMesh` | 944 / 422 / 316 / 37 | gameplay and bulk geometry |

So the world is one manifest plus 21,160 self-contained cell packages; a cell package
carries its entities, their selectors, materials and (for largemesh cells) the meshes
themselves. The viewer reproduces that by iterating the cell packages, which is why the
21,160 count matches exactly.

## Cell filter correction

`local/paris-cells.json` has 21,160 cells. 12,830 of them are 71 bytes and hold exactly one
child, an `a2b7e917` texture named `ACU_Paris_<x>_<y>_Map` - minimap tiles, not geometry.
The earlier `record_length > 1000` filter was wrong: it also dropped **6,162 real cells**
(81-1000 bytes, 14-95 children each) that carry props, house corners, lanterns and bushes.
The list is now every cell with `record_length > 80`: **8,330 cells**, 530,850 child refs,
337,672 entities.

## Result

```text
8,330 cells -> 207,867 placements, 5,770 cells with geometry
337,672 entities scanned, 530,850 child refs
14,928 unique meshes, 15,155 direct-mesh entities (ground/largemesh chunks)
LODs 0/1/2, framed extent 2.4 km across
```

Evidence: `local/paris-map-complete.png` (2560x1440, LOD2, no textures). The Seine, both
arms, the island, the street grid and the building blocks read correctly at city scale.

## What changed

- `--map-cells FILE` (JSON `[{ "entry_index": N }, ...]`); `tools/world_cells.py` writes the
  list.
- A cell's entities come from the cell package's own `0984415e` resources, not from the
  child-id list (the list mixes entity, nav, physics and group payloads).
- **Entry reuse**: `load_piece` re-read and re-inflated the whole entry per mesh. A largemesh
  cell decodes to tens of megabytes and holds hundreds of meshes, so cell 21267 needed
  >10 min; reusing the cached package through `piece_from` takes **17 s** for 2,177 pieces.
- **Direct-mesh entities**: largemesh/terrain chunks carry no `51dc6b80` selector; they
  reference a `415d9568` mesh themselves (e.g. `Largemesh_MR_GroundMesh_Large_LOD2_Chunk_0005`).
  The loader now resolves a selector first, then a direct mesh, both by type in the entity's
  own entry. This is what restores the ground.
- `b22b3e61` is **Havok collision** (`hk_2013.3.0-r1`, `hknpExternMeshShapeData`), not render
  geometry: entities that reference only that are skipped, correctly.
- Mesh instancing: one decode per unique mesh, one `MapInstance` per placement.
- **`FAKE_*` backdrops are excluded**: fake ground/coast/city-silhouette meshes span the
  whole map and hid the city from above. They are counted (9 in the largest 200 cells) and
  named in the summary, so the exclusion is measured, not silent.
- Guards: out-of-range matrices and vertices, undecodable meshes are named and counted.
- `--map-dry-run`, `--map-piece-limit`, `--orbit-elevation`, `ACU_MAP_TRACE=1`.
- `--map-player` spawns the Arno rig inside the map with the existing walk controls
  (`--spawn X,Y,Z` in Bevy coordinates); the map meshes and the rig are separated by
  `arno_start` so the skin binder only sees rig pieces.
- **Why walking did nothing at first**: `acu_sim::blocked` hard-coded the synthetic 24x24 m
  arena (`|x| > 11.5 || |z| > 11.5`), so at map coordinates every axis step was rejected and
  only the yaw changed. `Player` now carries `bounds` and `floor`: a map scene sets
  `bounds = 100_000` and `floor = spawn.y`. Movement is still flat - there is no runtime
  collision, so he walks through walls and over the river.
- **Largemesh containers are expanded**: a largemesh building is an Entity whose payload
  lists one Entity per chunk (LargeMesh_IC_NotreDameCemetary_01 -> 67 chunks). Those chunks
  were never visited, which is why whole buildings were missing. One level is expanded
  through the entity census index; chunk matrices are absolute (containers are identity
  grouping nodes, measured on two buildings), so the chunk matrix is used as-is.
  Whole map 207,867 -> 224,310 placements (15,392 containers).
- Framing: a multi-cell scene uses the 2nd..98th percentile of placement origins per axis
  (the river arms otherwise stretch the frame) and a 1.1x distance multiplier; one cell
  frames its meshes directly and reports outliers.

### The blank-capture bug

`inspect_camera` clamped the orbit distance to `0.4..20.0` every frame, so any scene larger
than 20 units had its camera snapped to 20 units from the centre on the first frame - a city
capture came out as uniform sky. The clamp is now relative to the framing distance
(`initial * 0.02 .. initial * 4.0`). Multi-cell scenes are framed by their **placement
origins** (terrain and water chunks span kilometres and would otherwise set the camera
distance), single cells by their mesh bounds. The far plane is explicit (100 km).

## Commands and results

```powershell
$env:RUSTUP_TOOLCHAIN='stable'
cargo build --bin acu_bevy
acu_bevy --game-dir "<install>" --map-cells local\paris-world-cells.json --map-lod 2 `
         --no-textures --map-piece-limit 500000 --size 2560x1440 --capture local\paris-map-full.png
```

Dry run over 8,330 cells: 208,472 resolvable placements in 265 s, 14,952 distinct mesh ids.

## Remaining uncertainty

- 2,557 of 8,330 cells contribute no geometry: their entities are lights, FX, FakeGI and AI
  markers, or reference nothing the loader can type.
- 123,291 entities reference no selector and no direct mesh.
- Water surfaces are still absent; the Seine reads as a hole.
- Textures are off in this pass. 235 meshes fail to decode (vertex format), counted per id.
- The `World` resource's payload is 1.6 MB of id blocks; the link table gives its targets but
  the streaming order and load radius are engine behaviour, not decoded.

### The remaining gap: EntityGroup carries the building rows

Measured, not guessed:

- `DataPC_ACU_Paris.forge` holds **9,759** `FRV_House*` `415d9568` meshes, but the whole-map
  load references only **241** distinct house meshes. The rest are never referenced by an
  `Entity` we visit.
- Those house meshes are referenced by `51dc6b80` selectors and by **`3f742d26` EntityGroup**
  resources (e.g. `FRV_House_Row_B_13x13x3_02`), which live in entries such as 36/38/45 and are
  listed as children of the cells (`Cell05073_DataBlock` has 64 children including two
  `3f742d26`).
- An EntityGroup payload is a 30-50 KB record table (version 1, self id, type `3f742d26`, then
  ~1,020 id-like records with embedded floats), not a simple matrix + id. Its member records are
  the building's parts and their transforms.
- Consequence: monuments, quays, props, ground, streets and trees render; residential building
  rows do not, because their placement lives in the group table we do not decode yet.

### Index completeness fix

`tools/build_indexes_from_db.py` rebuilds the ACUSEL01 indices from the SQLite asset index:
`paris-world-meshes.bin` 24,717 -> **116,526** rows, `paris-selectors.bin` 13,823 -> **414,002**,
`paris-entities.bin` 325,841 -> **340,346**. The earlier files were partial census exports.

Next: decode the EntityGroup record table (member id + transform), then spawn group members as
instances under the cell, which should lift the house-mesh count from 241 towards the 9,759 that
exist in Paris.

### EntityGroup decode: first pass (experimental)

- `local/paris-groups.bin` (72,597 rows `3f742d26`) is now built alongside the other indices.
- A group payload contains a sequence of 4x4 matrices; in
  `FRV_House_Corner_F_0_03x13x2_01_002` (31,018 B) a scanner finds **73 matrix records**, each
  followed within a few hundred bytes by a `51dc6b80` selector. The first matrix with a large
  translation (474.8, -864.3, 0.5) is the building's world position; the rest are local offsets
  (0-8.5 m), i.e. the parts of one row.
- The loader now spawns group members this way (matrix + nearest following selector), both for
  groups inside the cell package and for groups named in the cell's child list (resolved through
  the group index). On `Cell05073_DataBlock` that yields 4 groups / 14 members, far below the 73
  records of the sample group, so the scan is missing members - most likely because the
  selector look-ahead is too short or the record boundary is different for other group kinds.

Next: decode the group record boundary rather than the matrix/selector adjacency, then re-check
`Cell05073` - the target is that a normal street cell references hundreds of house parts.

### EntityGroup object decode (validated, placement still wrong)

Object boundaries are delimited by the reference tag `ec658d29`. In
`FRV_House_Corner_F_0_03x13x2_01_002` that gives **28 objects**, **27 with both a 4x4 matrix and a
`51dc6b80` selector** inside the object, and the group carries a count of 29 at offset 176, so the
layout is confirmed. Each selector has one mesh record (e.g. `0x114e089b9f` -> `0x10170c7e49`), i.e.
one part of the row.

The loader now spawns per object: first matrix + first selector inside the object, base = the object
with the largest translation, other members relative to it, members more than 150 m from the group
median dropped. A 500 m radius yields **54,487 groups / 389,539 members**.

Still not right: `Cell04814` (4 groups, 66 members) frames to about 988 m, so some records carry
absolute transforms and mixing the two rules scatters parts; the district render still shows the
ground carpet without building shells. Next: validate one group against a known building position
before trusting the rule.


### Group base rule: validated on one group, not all

FRV_House_Corner_F_0_03x13x2_01_002 has exactly one object with a large translation
(474.8, -864.3, 0.5) and 27 objects within +-10 m, so base + local is correct there, and the
cell's own entities sit at 458-468, -835..-864: the building belongs at that corner.

Cell04814 still frames to ~980 m centred between the origin and (474, -864), i.e. some of its
four groups are placed near the origin: their objects never yield a large-translation matrix, so
the base falls back to identity. That is the remaining question - one group kind carries its
world position outside the object records.

### Group base rule: solved

The world transform lives in the **group header**, before the first object: in
`FRV_House_Corner_F_0_03x13x2_01_002` the header holds identity at 26 and `(474.8, -864.3, 0.5)`
at 115, and all 28 objects are within +-10 m. Objects are local offsets and are now always
placed as `header * object`.

Result: `Cell04814` frames to **45 m** at `(470, -3, 854)` instead of 980 m, and
`local/cell-4884-final.png` shows a complete half-timbered house - walls, timber framing,
windows, roof, textures. The same rule also fixes the roof group
(`FRV_Roof_Corner_F_0_03x13_12_002`, header at (474.8, -864.3, 10.0)).

### Scale consequence

With buildings included, a 300 m radius already needs **~400,000 placements** (55,988 groups,
397,417 members) versus 19,000 without them. A whole-map building render is therefore not a
single scene: streaming cells around the player is required, which is what the game's `World`
manifest does. The current whole-map capture (224,310 placements) remains the buildings-light
aerial.


## Streaming fixes (measured)

- **Buildings were skipped entirely.** --map-detail-radius tested members[0] before the
  header transform was applied. EntityGroup object matrices are local (+-10 m), so comparing them
  with the player position rejected every group. The test now runs on the resolved median world
  position. Measured at the quay, radius 250 / detail 150 / LOD2: 283 cells, 19,835 pieces,
  4,381 buildings (11,654 parts) - previously 0 buildings.
- **Cells with only building groups had no centroid.** The centroid pass counted entity matrices
  only, so those cells were never streamed. Group header transforms are now included as a
  fallback: 6,361 -> 6,537 centres in 21 s.
- **Third-person camera sat inside walls** (6 m behind at street height). --map-camera-distance
  now sets it, default 12 m.
- **Frame rate** (1,600x900, release): 11-26 fps while the ring fills, 49-52 fps steady at
  radius 200 / detail 80; 124 fps on a light ring at radius 250 / detail 150.

## Open

- Large untextured surfaces (grey masses) intersect the camera in the quay area; not yet
  identified as material failure or oversized largemesh chunks. Needs a named cell and a mesh id
  before any change.
- Ground reads as sky past the streamed ring, because the ring is the only terrain loaded.
- Cell decode is still synchronous on the main thread (the fill-time frame drops).


## Material coverage (measured, 110 streamed cells at the quay)

| Archive set | Parts with material | Parts textured |
| --- | --- | --- |
| default (SharedGroup_00, DataPC) | 14,298 / 14,298 | 11,773 (82%) |
| four SharedGroup archives only | 10,306 / 14,298 | 9,921 (69%) |
| SharedGroup_00..03 + DataPC | 14,298 / 14,298 | 14,004 (98%) |

- The 2,525 untextured parts were not materials without a diffuse: 64 named a diffuse id that
  sset_query.py where locates in DataPC_SharedGroup_01.forge, which the loader never opened.
- --texture-archive **replaces** the default list, so passing the shared archives alone dropped
  DataPC.forge and 528 materials stopped resolving. SharedGroup_01..03 are now part of
  DEFAULT_TEXTURE_ARCHIVES.
- Untextured materials remaining: 7 (materials with no diffuse by design).
