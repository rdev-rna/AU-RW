# ACU-RE-0047 - Offline world placement map (Paris, Versailles) and parkour-obstacle candidates

Date: 2026-11-14 / investigator: Claude (subagent, offline only) / related: ACU-RE-0035,
ACU-RE-0036, ACU-RE-0046 (live teleport and vault scan), #35 (patch precedence).
State: partially established. Placement transforms are observed and cross-checked against
live positions in Versailles. Prop sizes come from decoded mesh vertices. Obstacle flags
are size and name inferences only.

## Provenance

- No game process was read or launched. All data comes from Forge archives in
  `D:/Games/Assassin's Creed Unity/`, opened read-only.
  - `DataPC_ACU_Paris.forge`: 3,770,744,832 B, 24,677 entries. This is the rescanned file;
    see the asset index note.
  - `DataPC_ACU_Versailles.forge`: 623,607,808 B.
  - Mesh payloads come from `DataPC_ACU_Paris.forge`, `DataPC_ACU_Paris_assets.forge` and
    `DataPC_ACU_Paris_assets_patch_03.forge` (and the Versailles counterparts).
- Asset index `local/asset_index.sqlite` version 2026-10-22.1. It supplied the entry lists,
  the type of each referenced id and the names. Type labels are `crc-name` (ACU-RE-0034),
  except `415d9568`, which is unnamed (`observed-decode`).
- New read-only Rust bins, built with
  `cargo build --locked --release -p acu_assets --bin acu-type-dump --bin acu-mesh-bounds`:
  - `acu-rs/crates/acu_assets/src/bin/acu-type-dump.rs`: bulk-dumps the resources of chosen
    types into `ACUTD001` files.
  - `acu-rs/crates/acu_assets/src/bin/acu-mesh-bounds.rs`: bulk `415d9568` decode with the
    existing `mesh::parse` / `decode`, giving a vertex AABB per mesh.
- Interpreter: `tools/paris_placements.py`. Tests: `tools/test_paris_placements.py`
  (18 tests, synthetic payloads).
- Reference payloads (file id, archive entry, entry stored SHA-256, payload length,
  payload SHA-256):
  - `00000010aea08064` `FRV_CanalKit_RiverKit_26_008`: Paris entry 5311, entry
    `3e61b371..14e5bc1e`, payload 4,016 B `31af7f4e..57492764`.
  - `0000001a5fd4c457` `GEN_All_MetalFence_01_069`: Versailles entry 1984, entry
    `2452883f..9980359c`, payload 1,076 B `ad9ef26d..8b88f4ab`.
  - `00000026fb963214` `GEN_ALL_CratesWine_01_008`: Versailles entry 1699, entry
    `d6cdcba5..9f945eef`, payload 1,272 B `ca22c798..b0759db9`.
  - `0000000e84b48068` `Player_Activator`: Versailles entries 1 and 5048 hold identical
    copies, 234 B `88cca640..2c1cbde1`.

Commands (from the project root):

```powershell
python tools/paris_placements.py extract       # Paris dumps -> local/paris/*-dump.bin
python tools/paris_placements.py placements    # -> local/paris/placements.jsonl + summary
python tools/paris_placements.py meshes        # LOD0 mesh vertex bounds
python tools/paris_placements.py types         # -> local/paris/prop-types.json
python tools/paris_placements.py --archive DataPC_ACU_Versailles.forge all   # local/world-DataPC_ACU_Versailles/
python tools/paris_placements.py --archive DataPC_ACU_Versailles.forge near 56.1 -270.7 -0.5 --radius 6
python -m unittest tools/test_paris_placements.py
```

Run times: extraction takes about 25 s for each Paris dump (670 MB of world payloads, 930 MB
of group payloads). Placements take about 45 s, mesh bounds about 40 s. Zero entries failed
to decode.

## Result

### Observations: layouts

These layouts hold on every payload in both archives. Every cell, Entity and EntityGroup
parsed without a failure.

- **GridCellDataBlock**: the layout matches ACU-RE-0035. All 21,160 Paris cells fit
  `17 + n*10 + 12`. Children carry flags 1 (520,703) or 0x0101 (22,978).
- **Entity / EntityGroup common header**: `u8 1, u64 self, u32 type, u8 transform flag`.
  The flag is either 3 or 0, with no other value seen.
  - **Flag 3** (Paris: 334,448 Entity, 52,315 EntityGroup resources): f32[16] at offset 14.
    Rows 0-2 are the basis and row 3 is the translation. Column 3 is exactly (0,0,0,1) on
    every one. A u32 component count sits at offset 78.
  - **Flag 0** (5,898 Entity, 20,282 EntityGroup resources): an embedded object at 14
    (`u64 id`, `u32 788baa0d`). It holds a *local* matrix at 26, 25 bytes (always starting
    with eight zero bytes in groups), then a *world* matrix at 115, with the component count
    at 179. Both matrices are affine on every payload.
  - Flag-0 local translations are small: about ±6.5 m, with z at 6.5 or 10.5 m (floor
    heights). Their world translations lie a median 11.4 m (p90 18.7 m) from the median of
    their flag-3 siblings in the same cell. This is why offset 115, not 26, is treated as
    the world matrix.
- **Entity own AABB**: tag `4aec3476` plus f32[6] (min xyz, max xyz). Tag `60121a9e`
  follows 40 bytes after that tag. Every Entity has exactly one such block; 1,349 also hold
  a second bare `4aec3476`. Groups hold several blocks (nested content), so the tool
  attributes none to a group.
- **References (tag-anchored scan, not a full component walk)**:
  - `ec658d29` + 4 bytes + u64 gives the visual's object. In Paris that object is an
    LODSelector 617,551 times, a 415d9568 mesh 47,287 times, an OmniLight 36,826 times,
    `755ace14` 18,767 times, `80320fb8` 1,347 times, `15da0c49` 899 times, and an id that
    is not in the index 191 times.
  - `536e963b` + u8 1 + u64 gives a world mesh id, one per LOD in order.
    `FRV_CanalKit_RiverKit_26_008` lists `..._LOD0` at payload offset 144 and `..._LOD1` at
    382. Only 4 Paris placements hold such a reference that is not a 415d9568 resource.
  - The entity therefore names its meshes directly. The selector-to-mesh hop that defeated
    #122 is not needed.

### Observations: counts (Paris)

- 6,601 cells have Entity or EntityGroup children; 14,559 have none (minimap tiles and
  similar). Cell child references by type: Entity 332,581, EntityGroup 72,524,
  ShadowRigidBody 85,497, `eebb2443` 22,978, TextureMap 16,384, others less.
- 397,205 placements are written, one per distinct file id: 325,841 Entity and 71,364
  EntityGroup. Of these, 396,561 are referenced by a cell and 644 are not (written with
  `cells: []`). 332,197 have a visual reference and 283,064 have a mesh reference.
- Duplicate ids inside the archive:
  - 391,013 are single copies.
  - 4,731 have identical copies.
  - 1,459 are resolved by taking the copy stored in the referencing cell's entry
    (implementation choice).
  - **2 are ambiguous**: the tool takes the lowest entry and marks the row
    `AMBIGUOUS-lowest-entry`.
- Position range of cell-referenced placements: x -1920.0..2000.3, y -1152.0..2525.9,
  z -127.9..700.6. The extremes are probably sky and far-scenery objects; this was not
  checked.
- Prop types (key = visual ref + LOD0 mesh id, or name stem when neither exists): 39,205
  types.
  - Size from decoded LOD0 vertices: 17,220 types.
  - Size from the entity AABB only: 20,817 types.
  - No size: 1,168 types.
  - LOD0 decode: 16,415 meshes requested, 16,372 located in Paris archives. 40 failed in
    the decoder, mostly `unsupported vertex format 2 stride 40` (14) and
    `nonfinite mesh scalar` (13).
- Versailles: 4,401 cells and 38,605 placements (32,524 Entity, 6,081 EntityGroup). Zero
  failures; duplicates were 293 identical and 67 resolved by the cell-entry rule.

### Observation: coordinate frame cross-check (Versailles, against live data)

The live values came from the coordinator, from the running game with the pinned save in
Versailles:

| live name | live position | decoded file id | decoded position | distance |
| --- | --- | --- | --- | ---: |
| `GEN_All_MetalFence_01_069` | (40.197, -250.353, 2.52) | `0000001a5fd4c457` | (40.197, -250.353, 2.520) | 0.0005 |
| `GEN_ALL_CratesWine_01_008` | (38.2, -302.593, 0.051) | `00000026fb963214` | (38.200, -302.593, 0.051) | 0.0007 |
| Arno (player) | (56.1, -270.7, -0.5) | `0000000e84b48068` `Player_Activator` | (56.07, -270.65, -0.50) | 0.06 |

Row 3 of the flag-3 matrix is therefore the position in the same frame and units as the
live entity inventory, to the precision the live values were quoted at. Live data has Z up
and metres. The player spawn coincides with the `Player_Activator` placement.

The file ids above are the ones to compare with the hypothesised live field
`[entity+0x130]+0x10`. Equal ids would confirm that field; this finding has not read it.

**Paris has not been checked against live data.** The decoder and layout are the same, so
the frame is expected to match, but no live Paris position has been compared.

### Observations: explicit traversal kit (by name), Paris

The names come from the live lead; no size is used here. Each one's visual is an
LODSelector, apart from one AnchorClimbing entity.

| family | Paris placements | stems |
| --- | ---: | --- |
| anchor-climbing (`GEN_ALL_AnchorClimbing_01..04`) | 3,818 | 01: 2,510, 02: 920, 03: 307, 04: 81 |
| beam (`GEN_ALL_Beam_2m/4m/6m/9m_01`) | 2,969 | 2m: 1,903, 4m: 546, 6m: 310, 9m: 210 |
| pole (`GEN_ALL_Pole_2m/3m_01`) | 1,580 | 2m: 1,131, 3m: 449 |
| store-ledge (`GEN_Store_LedgeNN_3m`, 12 variants) | 959 | Ledge11: 205, Ledge03: 186, Ledge13: 158, ... |
| window-planter-ledge (`GEN_ALL_WindowPlanter_Ledge_2m_01`) | 730 | |
| window-ledge (`GEN_ALL_WindowLedge_2m/5m_01`) | 629 | 2m: 367, 5m: 262 |
| rope-climbing (`GEN_ALL_RopeClimbing_01`) | 298 | |
| balcony-wood-beam (`GEN_ALL_BalconyWood_Beam_3m_01`) | 191 | |
| awning-ledge (`GEN_ALL_AwningLedge3m/7m_01`) | 96 | 3m: 75, 7m: 21 |

`FRV_ALL_AnchorClimbing_01` (948 instances) is a second family that the
`GEN_ALL_` pattern does not count. Versailles totals for comparison:
- anchor-climbing 439 (the live inventory saw 174 placements loaded)
- beam 172
- pole 120
- store-ledge 36
- window-planter-ledge 30
- window-ledge 20
- rope-climbing 20
- awning-ledge 11
- balcony-wood-beam 4

Vertex sizes of LOD0 meshes, if the units are metres:

| piece | h (m) | d (m) | length (m) |
| --- | ---: | ---: | ---: |
| `GEN_ALL_Beam_2m_01` | 0.70 | 0.20 | 2.12 |
| `GEN_ALL_Beam_4m_01` | 1.24 | 0.27 | 4.41 |
| `GEN_ALL_Pole_2m_01` | 0.22 | 0.14 | 2.04 |
| `GEN_ALL_AnchorClimbing_01` | 0.69 | 0.05 | 0.25 |
| `GEN_ALL_WindowLedge_2m_01` | 0.66 | 1.19 | 2.07 |
| `GEN_ALL_RopeClimbing_01` | 3.66 | 0.22 | 1.34 |

The nominal lengths in the names (2m, 4m, 5m) match the decoded lengths to within about
0.4 m. This independently supports metres as the mesh unit.

### Observations: `_Parkour_`-named resources (Paris)

Index query, by type:

| type | resources | distinct ids | distinct names | example |
| --- | ---: | ---: | ---: | --- |
| EntityGroup `3f742d26` | 2,966 | 2,966 | 544 | `FRV_IV_Parkour_Boat_Group_A_001` |
| Entity `0984415e` | 136 | 136 | 73 | `GEN_ALL_Parkour_Boat_02_011` |
| ReferenceListShape `2d675ba2` | 103 | 20 | 20 | `FRV_Parkour_Corner_F_160_7x13x2_01_a Merged Shape` |
| LODSelector | 84 | 2 | 2 | `GEN_ALL_Parkour_Boat_02` |
| MeshShape | 26 | 5 | 5 | |
| 415d9568 | 11 | 1 | 1 | `GEN_ALL_Parkour_Boat_B_LOD2` |
| `e802b9da` | 6 | 6 | 6 | |
| `5766a70c` | 6 | 1 | 1 | |
| `e7b83453` | 2 | 2 | 2 | |

- The 2,966 groups (`FRV_Parkour_Row_*`, `FRV_Parkour_Corner_*`, `MDA_Parkour_Row_*`) are
  all cell children.
  - 2,890 of them have no visual and no mesh reference. Their payloads are small (p50 447 B)
    and hold **no candidate link to any other resource**; they carry only a transform, a
    bbox and self-sibling ids.
  - In a sample of 300, every one is referenced by its cell, and 90 are also referenced by
    a ShadowRigidBody (`1cbde084`).
  - The other 76 groups link to LODSelector, 415d9568, Material, MeshShape and ShadowRigidBody
    resources, and 70 of them to a `... Merged Shape` ReferenceListShape. Each Merged Shape
    links down to one MeshShape and up to its same-named group, plus a ShadowRigidBody.
- The 136 Entities are mostly placed boats with meshes (`GEN_ALL_Parkour_Boat_02`, LOD0-2)
  and a few co-op/mission markers.
- Their role (for example, authored parkour collision per building row) is **not
  established**. What is observed is that they are transform-and-bbox nodes tied to
  physics bodies, with no render mesh.
- Versailles has 197 `_Parkour_` EntityGroups and no other `_Parkour_` types.

### Inferences (size-based guesses, to be confirmed by the live scan)

The thresholds are recorded in `prop-types.json`:
- vault: height 0.4-1.6 m, horizontal depth no more than 3.0 m, Entity only. The basis is
  the clip names `SkipOver_Fd{100,200,300}cm-Up{50..200}cm` and `SlideOver_Fd..-Up..`.
- beam: length at least 2 m, cross-section no more than 0.4 m.

Results in Paris:
- 1,359 vault-size types (85,738 instances).
  - Vegetation and creatures dominate the raw list: `GEN_ALL_GrassB_01` (3,466),
    `BushSmall_*`, `CN_A_Chicken`.
  - With an obvious-vegetation and effect name filter: 1,071 types and 46,843 instances.
- 94 beam-size types (3,591 instances).

Top non-vegetation vault-size types (h / d / length in m, from LOD0 vertices):

| type | instances | h | d | length | note |
| --- | ---: | ---: | ---: | ---: | --- |
| `GEN_ALL_GardensOrnateKitJ_01` | 1,986 | 1.49 | 1.18 | 2.12 | |
| `GEN_ALL_Stairs_Row_A_3x3_01` | 1,507 | 0.53 | 0.96 | 3.05 | stairs |
| `GEN_ALL_Shoring_6m_01` | 819 | 0.51 | 0.50 | 6.06 | |
| `GEN_ALL_GardensOrnateKitG_02` | 714 | 1.51 | 1.10 | 1.11 | |
| `GEN_ALL_BarrelStack_03` | 671 | 1.05 | 0.74 | 1.40 | |
| `GEN_ALL_BoxStacked_01` | 667 | 0.54 | 0.57 | 0.98 | |
| `FRV_IV_HayBale_M_01` | 639 | 1.15 | 1.24 | 2.29 | |
| `GEN_ALL_Tomb_Fresh_01` | 608 | 0.75 | 1.44 | 2.72 | |
| `GEN_ALL_CratesWood_Group_01` | 583 | 1.12 | 1.57 | 1.73 | |
| `GEN_ALL_RivBank_Fence_01` | 503 | 1.14 | 0.20 | 3.02 | |
| `GEN_ALL_CratesWine_01` | 473 | 1.16 | 0.59 | 1.05 | |
| `GEN_ALL_PostChains_01` | 460 | 1.22 | 0.37 | 3.09 | |
| `GEN_ALL_Barrel_01` | 406 | 0.94 | 0.68 | 0.71 | |
| `GEN_ALL_BarrelStack_04` | 403 | 1.45 | 0.80 | 2.79 | |

Top beam-size types: `FRV_MR_StoneTrim_3m_01` (1,690; 0.28 x 0.39 x 3.04) and
`GEN_ALL_Pole_2m_01` (1,580).

The kit pieces above (anchors, beams, window and store ledges) also pass the vault-size
test. They are wall-mounted, so this shows the size test cannot tell a ground obstacle from
a wall attachment. The name class should be preferred for those, and elevation above
ground needs adding (see Unknowns).

### Unknowns and incomplete coverage

- The **entity AABB is not the mesh vertex box**. On 208,981 single-visual entities, the
  maximum corner difference has p50 0.84 m and p90 3.0 m. z-max usually agrees and
  horizontal extents often differ, sometimes as if rotated. The tool therefore uses mesh
  vertices when they decode. Types that only have the entity AABB (20,817) are lower
  confidence.
- Mesh vertex bounds depend on the clustered decode (ACU-RE-0011/0012). The per-instance
  `536e963b` record may carry its own transform, which is not decoded; the bytes after the
  mesh id were zero in the sample.
- **Not decoded**:
  - nested inline Entity records inside EntityGroups. 2,579 of 5,000 sampled groups hold
    `0984415e` tags inside.
  - per-instance matrix lists inside groups (for example roof rows).
  - the full component layout.
  - the meaning of the 25 bytes at 90-114, `u8` children flags and tags `755ace14`,
    `80320fb8`, `15da0c49`, `eebb2443`.
  Group placements therefore give the group's own world transform and references, not its
  parts.
- Rotation is reported as stored rows plus a yaw about row 2. Handedness and the
  row-vector versus column-vector convention are not independently established. The world
  AABB uses `p' = x*row0 + y*row1 + z*row2 + row3`, the same as `acu_bevy` `load_map`.
- Ground contact and elevation are not computed. Neither navmesh nor collision is decoded.
- Which duplicate copy the game loads (#35): the cell-entry rule is an implementation
  choice. Two Paris ids remain ambiguous.
- Name hints are coarse. `bar` matches `Barrel`, for example. They are search aids, not
  evidence.

### Independent comparison / falsification

The live positions of two named props and the player in Versailles match the decoded
row-3 translations to 0.5-0.7 mm (props) and 0.06 m (player). A wrong matrix offset, a
local-frame matrix or different units would fail this test. The nominal metre lengths in
kit names also match the decoded vertex lengths.

### Joining with the live inventory

- Key on the 16-hex `file_id` in `placements.jsonl` once the live field
  `[entity+0x130]+0x10` is confirmed equal to these ids. Otherwise use name plus nearest
  position within 0.01 m. Each row also carries `archive`, `entry`, `payload_sha256` and
  `cells[]`.
- `prop-types.json` `types[].key` groups placements. The per-type `positions` hold the
  first five examples. `traversal_kit_by_name` and `parkour_named_placements` hold the
  name-class results.
- The output files are gitignored under `local/paris/` and
  `local/world-DataPC_ACU_Versailles/`: `placements.jsonl`, `placements-summary.json`,
  `prop-types.json`, `mesh-bounds-*.jsonl` and `*-dump.json`.

### Implementation implications

- Placing a cell in Bevy (#122) can read meshes straight from the `536e963b` records.
  Flag-0 entities and groups must use the world matrix at 115, not 26 and not 14.
- Constants recorded in this finding:
  - tags: `788baa0d`, `ec658d29`, `536e963b`, `4aec3476`, `60121a9e`
  - offsets: 14, 26, 78, 115, 179

## Development handoff
- **Confidence:** partial — placement layouts and transforms observed on every payload and
  cross-checked live in Versailles; obstacle classes are size/name inferences.
- **Established:**
  - Entity/EntityGroup header `u8 1, u64 self, u32 type, u8 flag`. Flag 3: f32[16] at 14
    (rows 0–2 basis, row 3 translation, column 3 = (0,0,0,1)), component count at 78. Flag 0:
    world matrix at 115 (not 26, not 14), component count at 179.
  - Entity AABB: tag `4aec3476` + f32[6]. Visual ref: `ec658d29` + 4 bytes + u64. LOD mesh refs:
    `536e963b` + u8 1 + u64, one per LOD in order.
  - Row-3 translation equals live positions to 0.5–0.7 mm (`0000001a5fd4c457`,
    `00000026fb963214`, Versailles); player spawn = `Player_Activator` `0000000e84b48068`.
  - Kit names' nominal lengths match decoded LOD0 vertex lengths (supports metres).
- **Not established:** Paris against live data; handedness and row- vs column-vector convention;
  nested inline entities and per-instance matrices in groups; the full component layout; bytes
  90–114 and tags `755ace14`, `80320fb8`, `15da0c49`, `eebb2443`; the role of `_Parkour_` groups;
  ground contact, navmesh, collision; duplicate precedence (#35; 2 Paris ids ambiguous);
  obstacle classes.
- **Interface:** `acu_assets`: decode a cell's placements → (file ID, flag, 4×4 world matrix,
  AABB, visual ref, LOD mesh IDs). `acu_bevy`: place them with
  `p' = x*row0 + y*row1 + z*row2 + row3`, as `load_map` does. Z up; metres supported by kit
  lengths and the live match.
- **Oracle:** `python tools/paris_placements.py --archive <forge> all` and
  `python -m unittest tools/test_paris_placements.py`; compare Rust-decoded transforms with
  `placements.jsonl` per file ID, and with live `tools/native_probe/world_entities.py` positions.
- **Spawned / follow-up:**
  - Decode nested entities and per-instance matrices inside groups.
  - Check a Paris position live; establish handedness.
  - Add elevation/ground contact to separate wall kit from ground obstacles.
  - Settle the cell-entry duplicate rule (#35) and the 2 ambiguous ids; the 40 LOD0 decode
    failures.
