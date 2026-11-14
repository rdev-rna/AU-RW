# ACU-RE-0050 — World collision and ground height (offline decode)

Date: 2026-11-14 / investigator: Claude (subagent, offline only) / related: backlog item 1 of
`docs/REWRITE_READINESS.md`, ACU-RE-0034 (type names), ACU-RE-0035 (cells), ACU-RE-0047
(placements), ACU-RE-0046 (hop scan, teleport), #35 (patch precedence).
State: partially established. The static collision triangles of a world are decoded and placed;
ground height from them matches the live player's settled height to millimetres on Versailles
and Paris (Versailles: 96.8 % of 5,299 on-foot points within 5 cm; Paris hop scan: 61 of 65
within 5 cm, heights from -8.7 m to 10.0 m). Primitive shapes, the navmesh, data-layer switching and the runtime query
routine are not decoded.

## Provenance

- No game process was launched, attached or read for this finding. Live numbers come from
  runs captured earlier by the coordinator (listed below), read from `local/native-probe/runs/`.
- Installed `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
  image base `0x140000000`. Mapped image used for strings:
  `E:/ReverseEngineering/Projects/Unity/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`
  (VA = 0x140000000 + offset).
- Archives (read-only, `D:/Games/Assassin's Creed Unity/`): `DataPC_ACU_Versailles.forge`
  (bodies, reference lists, primitives, entities, groups), `DataPC_ACU_Versailles_assets.forge`
  and `DataPC_ACU_Versailles_assets_patch_01.forge` (MeshShape); the Paris counterparts
  `DataPC_ACU_Paris.forge`, `DataPC_ACU_Paris_assets.forge`, `_assets_patch_01/02/03`.
- Asset index `local/asset_index.sqlite` version 2026-10-22.1 for entry lists and names. Type
  labels used are `crc-name` (ACU-RE-0034). The component tags below were named the same way
  (CRC-32 of an ASCII string found in the mapped image, string VA given).
- Reference payloads (archive, entry, entry stored SHA-256, ordinal, payload length, payload SHA-256):
  - MeshShape `00000034b4c3a4a3` `Largemesh_VRS_Village_Farm_LOD0_Chunk_0003_MeshShape`:
    Versailles_assets entry 3209 `d80f9f22..b411e8`, ordinal 0, 8,624 B `df63e4b8..c68bd7`.
  - MeshShape `00000034cbf62439` `Largemesh_VRS_Village_FarmPart4_LOD0_Chunk_0000_MeshShape`:
    Versailles_assets entry 6108 `00a79933..864475`, 32,449 B `3b66742d..209172`.
  - ShadowRigidBody `0000003f575a63a1`: Versailles entry 1699 `d6cdcba5..9f945eef`, ordinal 1,
    143 B `93c08553..957dd1` (identical copies in entries 1554, 1570, 4829).
  - ShadowRigidBody `0000003f575a63b1` (one DataLayerAction record): entry 1570, 177 B `545c3e4e..6aca1`.
  - ReferenceListShape `000000245abcd54d` `GEN_ALL_SnapWall_04 Merged Shape`: entry 1699, 162 B
    `b66b3ee0..a66fcb` (21 identical copies).
  - Entity `00000010cbf44427` `GEN_ALL_WallRock_6m_01_294` (RigidBody component): entry 1699,
    ordinal 10, 1,657 B `f0c9e438..bf3136`.
  - Entity `0000002cc2b501ff` `Largemesh_VRS_Village_FarmPart4_LOD0_Chunk_0000`: entry 1853,
    770 B `8c0d58c1..b01e027`.
- Tools (new): `tools/world_collision.py` (extract / survey / height / compare) driving the existing
  read-only bin `acu-rs/target/debug/acu-type-dump.exe`; tests `tools/test_world_collision.py`
  (18 tests, synthetic bytes). No Rust was changed.
- Commands, from the project root:

  ```powershell
  python tools/index_status.py                                   # READY 2026-10-22.1
  python tools/world_collision.py extract                        # Versailles dumps -> local/world-collision/DataPC_ACU_Versailles/
  python tools/world_collision.py survey                         # layout counts -> summary.json
  python tools/world_collision.py height 56.0653 -270.6515 --probe -0.5487
  python tools/world_collision.py compare --write data/oracles/ground_versailles.json
  python tools/world_collision.py --archive DataPC_ACU_Paris.forge extract
  python tools/world_collision.py --archive DataPC_ACU_Paris.forge --bounds 340 -390 990 160 compare --runs hop-paris-1 hop-paris-2 --write data/oracles/ground_paris.json
  python -m unittest tools/test_world_collision.py
  ```

## Result

### Observations: where collision lives

1. **Two sources of static collision**, both ending in `MeshShape` (`b22b3e61`) triangles:
   - **`ShadowRigidBody`** (`1cbde084`) resources in the world archive's cell entries. Versailles:
     7,280 copies, 5,549 distinct ids. They carry the terrain (`Largemesh_*_Chunk_NNNN_MeshShape`)
     and merged building/roof shapes.
   - **`RigidBody` components** inside placed `Entity` and `EntityGroup` payloads, including the
     inline entities of groups. Versailles: 18,463 on top-level owners, 10,360 on inline entities.
     They carry props, walls, decals, interiors.
   Some shapes are reached by both. The terrain chunk `00000034cbf62439` is placed once by
   ShadowRigidBody `0000003f575a63f8` and once by Entity `0000002cc2b501ff` at the same height.
2. **Component and record tags, named by CRC-32** (string VA in the mapped image):
   `228f402a` = `RigidBody` (0x14361e7a8), `43ef99c2` = `CollisionFilterInfo` (0x143621af8),
   `db1d406e` = `DataLayerFilter` (0x143636e90), `e31593e1` = `DataLayerAction` (0x143636e00),
   `8f8039af` = `MeshShapeTriangleMaterialData` (0x14361e818); shape classes `4ec68e98`
   `BoxShape`, `445b37f9` `CylinderShape`, `b8599052` `CapsuleShape`, `fa3f7a18` `SphereShape`;
   `92ada771` `FloorHeightData`.
3. **MeshShape layout** (all 2,644 Versailles MeshShapes parse with no bytes left over):

   ```text
   0   u8 1, u64 self id, u32 type b22b3e61
   13  12 bytes (zero in 2,626, first byte 0x49 in 17)
   25  u32 (0x390f5602 in the sample; meaning unknown)
   29  u32 L = Havok packfile length
   33  Havok binary packfile, L bytes: magic 57e0e057 10c0c010, file version 11, layout 8/1/0/1,
       3 sections (__classnames__, __types__, __data__), contents "hk_2013.3.0-r1",
       one object of class hknpExternMeshShapeData (an AABB tree; triangles are NOT inside)
   33+L  u8 flag, u8          Versailles: flag 0 with per-triangle materials 1,312, without 17;
                               flag 1 without 1,315. Paris: flag 1 with per-triangle materials 14,
                               so the flag is not a reliable "has materials" bit
         u32 nv, nv x f32[3]   vertices, shape-local, metres
         u32 ni, ni x u16      indices, a triangle list (ni % 3 == 0, all < nv)
         u32 nm, nm x u8       per-triangle material index; nm is 0 or ni/3
         u32 k,  k x 49 bytes  MeshShapeTriangleMaterialData: 8 zero bytes, tag 8f8039af, u8 1,
                               u8 0, u64 CollisionMaterial id (+14), rest not decoded
         f32[8]               AABB (min x y z 1, max x y z 1); equals the vertex bounds within
                               1 mm on 2,642 of 2,644 shapes
   ```

   Sample `00000034b4c3a4a3`: L = 4,112 (0x1010); 183 vertices, 894 indices (298 triangles),
   298 material indices (0: 127, 1: 53, 2: 118), materials `0000000738d22e48` ACU_Dirt,
   `0000000738d22e58` ACU_Stone_Dirt, `0000000738d22e5b` ACU_Veg_Grass_Long (all CollisionMaterial
   `74f7311d`). Versailles total: 581,341 triangles. Paris: all 13,326 distinct MeshShapes parse
   the same way (4,315,854 triangles; AABB equals vertex bounds on 13,317).
4. **ShadowRigidBody layout** (all 7,280 copies parse):

   ```text
   0   u8 1, u64 self id, u32 type 1cbde084
   13  u8 1, u8 link byte (0|1; Versailles: 1 with a MeshShape 4,324, 0 with a ReferenceListShape
       1,224 or a BoxShape 1; Paris: 0 with a MeshShape 27,033 times, so NOT a shape class)
   15  u64 shape id
   23  8 zero bytes, u32 db1d406e (DataLayerFilter), ...
   35  u8 n = number of 25-byte DataLayerAction records that start at 47 (Versailles 0: 5,355,
       1: 189, 2: 5; Paris up to 12; all 85,497 Paris copies parse with this rule)
   47+25n  u32 43ef99c2 (CollisionFilterInfo), 15 bytes
   66+25n  f32[16] world matrix, row-major, row 3 = translation, column 3 = (0,0,0,1)
   130+25n u32 m, m x (u8 0, u64 Entity id); nothing follows
   ```

   The 15 CollisionFilterInfo bytes were `01 00..00` on every body that supported a live point.
   The tail lists entity ids: across the 7,280 Versailles body copies, 10,662 ids are listed;
   7,280 of them are entities with a decoded RigidBody component (exactly one per body copy),
   and 7,277 of those reference the same shape id as the body. The other 3,382 listed ids have
   no decoded RigidBody (role not established). Sample `0000003f575a63a1` lists Entity `0000002457777310`
   `Largemesh_VRS_Village_Farm_LOD0_Chunk_0003` and its matrix row 3 (80, -336, 0) equals that
   entity's placement (ACU-RE-0047 decoder).
5. **ReferenceListShape layout** (all 3,648 copies parse with no bytes left over):
   `u32 0, u32 n, n x (u8 1, u8 0|1, u64 shape id), u32 n, n x f32[16] child matrix,
   u32 n, n x 27 bytes, f32[8] AABB`. All 108,679 children in Versailles are MeshShapes with
   link byte 1. Paris (51,816 copies, all parse) also has link byte 0 on children that are
   MeshShapes: `FRB_BV_RiverWalls_004 Merged Shape` (`0000002f4a7b663d`) lists
   `000000137ffc330b` `FRV_BV_WallRiver_3M_01` (MeshShape) with byte 0. So shapes are resolved by
   the referenced resource's type, never by this byte; its meaning is unknown.
6. **RigidBody component in an Entity / EntityGroup**: `u64 component id, u32 228f402a, 8 zero
   bytes, u32 43ef99c2` then, relative to the CollisionFilterInfo tag `F`:
   `F+4` u8 variant, `F+19` u8 1, `F+20` u8 link byte (as above), `F+21` u64 shape id, `F+29` f32 x3
   + 5 bytes (10.0, 0.1, 0.1 in most), `F+46` f32[16]. Variant 2 (1,692 in Versailles) holds the
   shape id elsewhere and a zero block at `F+46`; it is reported unsupported and skipped.
   The f32[16] at `F+46` is identity on 23,383, diagonal on 5,437, and a symmetric 3x3 with a
   small row 3 on 3 (`SQ01_M010_SC120_PROP_Musket01`: diagonal 3.03/2.60/0.45, row 3
   (0.024, 0.274, 0.55)).
7. **Inline entities inside groups** keep a flag-3 world matrix right after their type tag
   (`u64 id, u32 0984415e, u8 3, f32[16]`). Of the inline entities whose position lies in the hop
   scan area, 4,180 coincide within 1 cm with a live entity origin from `hop-versailles/map.jsonl`
   when the matrix is read as world space; none coincide when it is composed with the group's
   world matrix; 3,811 match neither (not loaded or not in the scan).
8. **Primitive shapes**: Entity RigidBodies reference `BoxShape` 595, `CylinderShape`
   374, `CapsuleShape` 197, `SphereShape` 41 times in Versailles (one ShadowRigidBody references a
   BoxShape). 803 references point at ids not in the dumped archives. None of these is decoded.
9. **NavMeshManager** (`0b4ce0e0`, one per cell, e.g. `NavMeshManager_01441` 52,839 B) holds a
   packed block of its own followed by two Havok packfiles of class `hknpExternMeshShapeData`
   (payload offsets 0xe27 and 0x83c6 in that sample). Not decoded.

### Observations: ground height against the live game (Versailles)

Ground height at (x, y) = the highest decoded collision triangle crossed by the vertical line
through (x, y) at or below the reference height + 0.5 m ("support"). The live reference is the
player's position (Z up, metres; ACU-RE-0047) at moments when it is settled:

- hop scan `at` rows (`hop-versailles/hops.jsonl`), and
- live-state samples whose z changes by at most 5 mm over three consecutive samples
  (`catalog-1`, `nav-1`, `prop-probe-1`, `vault-scan-1..3`, `window-sweep-1..2`),
  deduplicated on a 0.25 m xy / 0.05 m z grid: 6,864 points.
- "On foot" subset: the sample's dominant clip is `xx_Nav_*` / `acu_nav_*` without `WallBrace`,
  or a hop-scan row: 5,299 points.

| set | points | no surface | median abs error | p90 | max | within 5 cm | within 25 cm |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| all settled | 6,864 | 41 | 3.5 mm | 82 mm | 7.69 m | 6,033 | 6,243 |
| on foot | 5,299 | 28 | 3.5 mm | 3.9 mm | 1.71 m | 5,102 (96.8 %) | 5,225 |

Signed median (player z minus surface): -3.4 mm, i.e. the player origin sits 3-4 mm below the
decoded surface. Examples: spawn `(56.0653, -270.6515, -0.5487)` → surface -0.5635 on
`00000034cbf62439` (ACU_Dirt); hop 1 `(50.992, -296.467, 0.016)` → 0.0192. Of the on-foot points
within 5 cm, 4,334 are supported by a ShadowRigidBody shape and 768 by an Entity RigidBody shape.

The "all settled" set includes climbing and hanging (z steady on a wall), which explains most
errors above 1 m: by dominant clip, `xx_Climbing_*` and `xx_Parkour_Leaping_*` samples are mostly
outside 5 cm, `xx_Nav_*` mostly inside.

### Observations: ground height against the live game (Paris)

Same rule, live reference = the settled player position of every Paris hop (`hop-paris-1`
2 rows, `hop-paris-2` 64 rows; 65 distinct points; x 370..954, y -359..131), collision limited
to instances overlapping the box (340, -390)..(990, 160).

| set | points | no surface | median abs error | p90 | max | within 5 cm | within 25 cm |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Paris hops | 65 | 0 | 3.2 mm | 8.4 mm | 0.56 m | 61 | 64 |

The heights are not trivial: (738.509, -101.32, -8.456) → -8.4644 (river bank), (929.469,
-142.637, 10.011) → error -4 mm, (574.641, -326.308, 5.431) → 5.4341 under a column of 10
surfaces, (450.141, -63.471, -8.64) → -8.6367. Signed median -2.7 mm, the same sign and size as
Versailles. The arrival point (768.55, -46.653, -0.456) → -0.4527. Outliers: hop 19
(800.191, -354.561, 1.982) is 0.56 m above the nearest decoded surface (1.4239), hops 8, 40,
63 are 6-13 cm above an Entity RigidBody surface; a primitive or undecoded shape is the likely
support (not checked).

Paris dataset (`DataPC_ACU_Paris.forge` + `_assets*`): 85,497 ShadowRigidBody copies (58,814
distinct), 51,816 ReferenceListShape copies, 287,245 + 6,833 MeshShape resources (13,326
distinct ids; MeshShapes are in the world archive itself as well as in `_assets`), 215,927
top-level and 71,752 inline Entity RigidBodies (453 unsupported). 3 MeshShape ids have differing
copies across the Paris archives (#35; the base archive's copy was used). Paris RigidBodies
reference BoxShape 5,601, CylinderShape 3,973, CapsuleShape 2,007, SphereShape 375 times
(not decoded); 6,336 reference ids not in the dumped archives.

### Inferences and alternatives

- The f32[16] at RigidBody `F+46` is not a shape transform. Applying it put
  `GEN_ALL_DirtDecals_02` shapes at 4x their size (diagonal 4.0, while the entity's own AABB and
  the MeshShape AABB agree at about ±0.87 m) and left 646 on-foot points mis-supported; dropping
  it raised the on-foot within-5-cm count from 4,505 to 5,102. With mass 10.0 at `F+29` and the
  symmetric musket block, it is most likely an inertia tensor with a centre of mass in row 3.
  Inference, not decoded.
- ShadowRigidBody is a per-cell copy of an entity's RigidBody (same shape, the entity's world
  matrix, the entity id in its tail; observation 4). Why the copy exists (streaming the cell's
  physics before or without the entity, or a separate physics world) is not established. A
  reconstruction can take collision from either source; taking both duplicates triangles but
  does not change the support height.
- The 3-4 mm offset is probably the character controller's contact skin or a capsule offset.
  Not established.
- Remaining on-foot misses (169 points beyond 5 cm, 28 with no surface) cluster on props whose
  collision is a primitive (not decoded), on ids outside the dumped archives, and in a few places
  with no terrain (e.g. around (63.4, -331.6)). Some may be bodies switched by data layers.

### Unknowns / incomplete coverage

- Primitive shapes (Box, Cylinder, Capsule, Sphere): identified, not decoded (a 107-byte
  BoxShape holds three floats then a f32[16] at 0x21; whether the floats are half or full extents
  is not established).
- RigidBody variant 2 (1,692 Versailles components), the 60 components whose owner has no
  supported transform (flag-0 inline owners), and 803 shape references outside the dumped archives.
- CollisionFilterInfo bytes, DataLayerFilter / DataLayerAction semantics (which bodies a mission
  state enables), the 27-byte reference-list records, the 12 bytes at MeshShape 13 and the u32
  at 25, the second byte after the packfile.
- The Havok packfile contents (the hknpExternMeshShapeData AABB tree) are skipped; the
  triangles are read from the trailing Ubisoft arrays.
- NavMeshManager and FloorHeightData are not decoded.
- Which duplicate copy the game loads (#35): bodies and shapes found in several entries were
  byte-identical in Versailles. In Paris 3 MeshShape ids differ between archives; the tool takes
  the copy from the first archive in name order (world archive, then `_assets`, then patches),
  an implementation choice. None of the 65 Paris points was supported by one of those 3 ids
  (not checked individually).
- Handedness and the row-vector convention (ACU-RE-0047) are still inferred; the ground match
  is unaffected by handedness because x and y are taken from the same frame on both sides.
- The runtime query routine is not confirmed (see "Live check").

### Independent comparison / falsification

The live heights were captured by the game and never used to fit anything: the decode has no
free parameter except the 0.5 m support step. A wrong matrix offset, a wrong vertex stride, a
local/world mix-up or a wrong index width would scatter the error by metres; instead 96.8 % of
on-foot points agree within 5 cm and the median is 3.5 mm. The F+46 block was rejected by this
test (it degraded the match), not by assumption.

### Live check that would settle the remainder

- ACUFixes (NameTaken3125; cited as a lead, unverified, `docs/ACUFIXES_REFERENCE.md`) names a
  ray cast at `0x1425FD7D0`. Corpus `1425FD7D0.c` (size 63) computes
  `end = origin + length * direction` and calls `sub_1425FD830`, which (`1425FD830.c`, size 287)
  returns 0 when start and end coincide, builds a ray (origin, direction, reciprocal direction
  with sign bits and 0.5f in w) and calls `sub_1425FDE60` (size 1,027). This is consistent with a
  segment ray cast. The query object is 1,632 bytes, built by `sub_1425EC5A0` (hit array at +8
  with 96-byte entries per `sub_1425DF220`, which sorts `*(u16*)(+10)` entries).
- `sub_1425FD7D0` has 24 call sites outside itself (full corpus scan, 217,041 files; Python scan
  because `rg` is not on this machine's bash PATH). Two have fixed vertical directions, read from
  the mapped image:
  - `140D0B8D0.c:35-39`: origin = `*a2 + xmmword_142E88AB0` (0, 0, 0.1), direction =
    `xmmword_142E75BC0` (0, 0, 1) XOR `xmmword_142E76400` (sign bits) = (0, 0, -1), length
    `1050253722` = 0.3f; the function returns true when nothing is hit, and is skipped when the
    horizontal speed is at least a threshold read from `[*a1]+0xA8`. A short "is there ground
    under this point" probe from 0.1 m above to 0.2 m below.
  - `1401716A0.c:37-44`: origin = `*a2 + xmmword_142E77FF0` (0, 0, 0.5), direction (0, 0, 1),
    length `1090519040` = 8.0f; reads the hit's z at `[hit]+8` (hit from `sub_1425D4EF0`) and
    subtracts `a2.z`. An upward clearance probe.
  Which of these the character's ground contact uses is not established.
- Type-id sites (same scan): `0xB22B3E61` in `141DB6A10.c:5` (`sub_14270E560(a2, 0xB22B3E61,
  a1 + 96, ...)`, a typed-reference serializer call on an object field +96), `482205828`
  (= `0x1CBDE084`) in `142609FA0.c:37`, a reader that loads an array of `159662430`
  (= `0x0984415E` Entity) references into `+264` (u16 count at `+274`) and then one
  ShadowRigidBody reference into `+280`. The ShadowRigidBody payload has no reference after its
  entity list, so this is probably the reader of another class that points at a body (owner not
  identified); `0x74F7311D` in 12 functions around `0x1425D01D0..0x1425D1DB0` and
  `0x142608CE0`, `0x14260A650`. These are leads for a live read of the runtime objects; they were
  not followed further.
- Hook `0x1425FD830` (rcx = the 1,632-byte query object, rdx = the world/physics pointer the
  callers take from `[entity-side object]+96`, r8 = &origin, r9 = &end, then two flag bytes)
  while Arno walks; filter to end - origin = (0, 0, -d); after return, take the first hit through
  `sub_1425D4EF0(query+8, 0)` as `1401716A0.c` does (hit position at +0, z at +8), and compare
  the hit z with `python tools/world_collision.py height X Y`. Equal hits on terrain and
  on a primitive-shaped prop would confirm both the routine and the decode, and show which shapes
  the character query sees (decals, data-layer bodies).
- Read `[entity+0x130]+0x10` (ACU-RE-0047) of a RigidBody owner and its component array to check
  that the runtime body uses the entity world matrix without the `F+46` block.

### Implementation implications and finding-linked constants

- Type ids: MeshShape `b22b3e61`, ShadowRigidBody `1cbde084`, ReferenceListShape `2d675ba2`,
  CollisionMaterial `74f7311d`; tags RigidBody `228f402a`, CollisionFilterInfo `43ef99c2`,
  DataLayerFilter `db1d406e`, DataLayerAction `e31593e1`, MeshShapeTriangleMaterialData `8f8039af`.
- Offsets: body shape id 15, record count 35, matrix 66 + 25n; MeshShape packfile length 29,
  packfile 33; material record 49 bytes, material id +14; reference list record 27 bytes;
  RigidBody: CollisionFilterInfo at +12 from the tag, shape id at F+21.
- Composition: ReferenceListShape child then body; Entity RigidBody shape at the owner's world
  matrix. `p' = x*row0 + y*row1 + z*row2 + row3`.

## Live check (run raycast-1, 2026-11-14)

Setup: pid 86540, Paris, focus shim t8 at a 30 fps cap, Arno standing, then walking forward
for 3 s. Native probe profile `tools/native_probe/raycast.profile.json`: one entry probe at
`0x1425FD850` inside the ray routine `0x1425FD830`. At that point xmm5 already holds the ray end,
loaded from `[r9]` at `0x1425FD841`, and r8 points at the origin. Player positions came from a
concurrent 50 ms poll (`camera_state.py --log`). Raw data is in `local/native-probe/runs/raycast-1/`.
The ray-cast lead is credited to ACUFixes (NameTaken3125) and was re-derived from the corpus and
the mapped image.

- **Ground under the live player.** Live player z −0.3845 at (686.85, −94.99). `world_collision.py
  height` gives a support surface at −0.381 (MeshShape `0000000fdd39cd9a`, MDA_LM_CimInn_ARCH_01,
  body `0000003f575f5087`). The difference is 3.5 mm, the same offset as the oracle median.
- **The routine is live and busy.** 15,376 hooked ray casts arrived in one 10 s session, and
  13,200 of them started more than 2 m from the player (NPCs and other systems). Rays cast by
  others included vertical rays of 3.35 m starting 1.5–1.9 m above other characters.
- **Rays at the player's position**, grouped by shape (height of origin above the player origin,
  length):

  | Shape | Origin height | Length | Casts |
  | --- | --- | --- | --- |
  | Down | +0.5 m | 1.5 m | 537 |
  | Down | +0.5 m | 7.5 m | 187 |
  | Down | +3.0 m | 11.0 m | 175 |
  | Down | +3.75 m | 11.5 m | 211 |
  | Horizontal | +0.75 / +1.0 / +1.25 m | 5.0 m | about 140 each |
  | Horizontal | +0.75 / +1.25 / +1.75 m | 2.5 m | about 135 each |
  | Slanted / horizontal | about +1.42 m | 1.7 m | small counts |

  The down rays are ground probes. The horizontal fans at fixed heights are the obvious candidate
  for wall and ledge scanning (parkour scanner input, REWRITE_READINESS item 5); that is an
  inference.
- **Not recorded:** hit results (they are written after the routine returns) and callers (at a
  mid-function probe the stack top is no longer the return address). Per-second rates are
  omitted because the probe ring overwrote events. Next step: a probe at the routine's return
  path, or at the callers `0x140D0B8D0` (down 0.3 m) and `0x1401716A0` (up), reading the hit
  record through `sub_1425D4EF0(query+8, 0)`.

## Development handoff

- **Confidence:** observed for the static triangle collision of Versailles (every MeshShape,
  ShadowRigidBody and ReferenceListShape parses; the live on-foot match is 96.8 % within 5 cm);
  observed on Paris as well (65 hop points, 61 within 5 cm, all ReferenceListShape and
  ShadowRigidBody copies parse). Partial overall: primitives, data layers, navmesh and the
  runtime query are open.
- **Established:**
  - Collision = MeshShape (`b22b3e61`) triangles placed by ShadowRigidBody (`1cbde084`, world
    matrix at 66 + 25n) or by an Entity / EntityGroup / inline-entity `RigidBody` (`228f402a`)
    component at the owner's world matrix; ReferenceListShape (`2d675ba2`) children compose
    child-then-body.
  - MeshShape: Havok 2013.3 packfile (length at 29, skipped), then u8 flag, u8, f32[3] vertices,
    u16 triangle list, optional u8 per-triangle material index, 49-byte material records with the
    CollisionMaterial id at +14, f32[8] AABB.
  - Ground height = highest triangle under (x, y) at or below the reference + 0.5 m; on-foot
    median error 3.5 mm (player origin 3.4 mm below the surface), Versailles 5,299 points;
    Paris 65 hop points median 3.2 mm, heights -8.7..10.0 m.
  - Shapes are resolved by the referenced resource's type; the u8 link byte before a shape id
    (body 14, RigidBody F+20, reference-list child) is not a shape class.
  - The RigidBody f32[16] at CollisionFilterInfo + 46 must not be applied to the shape.
- **Not established:** primitive shapes (Box/Cylinder/Capsule/Sphere), RigidBody variant 2,
  CollisionFilterInfo and data-layer semantics (which bodies a mission state switches), the
  character's own ground query and contact offset, NavMeshManager and FloorHeightData, duplicate
  precedence (#35; 3 differing MeshShape copies in Paris), the Havok AABB tree.
- **Interface:** a Bevy-free `acu_assets` (or new `acu_world`) reader: `MeshShape { vertices:
  Vec<[f32;3]>, triangles: Vec<[u16;3]>, triangle_material: Option<Vec<u8>>, materials:
  Vec<FileId>, aabb }`; `CollisionInstance { source: Shadow|Entity, owner: FileId, shape: FileId,
  world: [[f32;4];4] }` per cell; and a pure `ground_height(x, y, z_ref, step) -> Option<(f32,
  FileId material)>` over a 2D grid of instance AABBs. Port the layouts from
  `tools/world_collision.py`; keep file ids as 16-hex strings in fixtures; fail on unsupported
  variants rather than skipping silently.
- **Oracle:** `data/oracles/ground_versailles.json` (schema `acu-oracle/ground/1`; per point:
  live position, clip, on-foot flag, decoded support z, error, shape / body / material ids),
  and `data/oracles/ground_paris.json` (same schema, 65 hop points), regenerated by
  `python tools/world_collision.py [--archive ...] [--bounds ...] compare --write ...`.
  A Rust port passes when it reproduces `support_z` per row within 1 mm and the on-foot summary.
- **Follow-up:** decode BoxShape and the other primitives (then re-run the oracle; the remaining
  on-foot misses are the test); live hook of `0x1425FD830` filtered to direction (0, 0, -1) to
  confirm the query and see which shapes it hits; DataLayerFilter / DataLayerAction semantics;
  NavMeshManager as a later AI/pathing slice.
