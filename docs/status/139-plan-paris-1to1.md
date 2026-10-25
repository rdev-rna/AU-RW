# Plan v2 — Paris 1:1 in Bevy, reviewed against AC1's verified implementation

Supersedes v1 (§§ 1–8 of the previous revision). Corrected 2026-10-25 after reading
`Banned445/AC1-Movement-Rewritten`, a clean-room AC1 in Bevy that streams all of Damascus from the
user's install, whose `src/city/*` modules carry exe addresses and archive observations. Those are
engine rules; our v1 plan was built on guesses about them.

## 1. Definition of done

| # | Criterion | Oracle |
|---|---|---|
| D1 | Standing at a fixed street pose, the frame shows a continuous city | Deterministic capture (§6); no cell-level gaps inside the loading radius; components smaller than N px either absent or on a declared prop list |
| D2 | The player cannot reach unloaded ground | Scripted walk; position is held whenever a wanted cell in the half-cell box is loading; zero frames with no surface under the player |
| D3 | 100 m of walking at >= 30 fps | Automated keys; distance logged, fps logged, no panic |
| D4 | The ring stays textured | `parts [bound, textured]` >= 99 % textured in the loaded set |
| D5 | Distance reads as a city, not as an edge | Fake spans present beyond the loaded radius; horizon never empty |

A whole-city 4K still is **not** a criterion. The reference implementation never draws the whole
city either: distance is covered by fake meshes with per-cell index spans.

## 2. The engine's model (verified, with addresses)

### 2.1 Grid and streaming
- `GridLayout` (`0x7DB083ED`): cell size, origin, finest width, plus a byte at `+20` = loading radius
  outside the grid. `GridPartition` (`0x0E2F4444`): `levels`, `cells`, each
  `GridCell { col, row, level, datablock, dependencies }`. Cell `k` of level `L` covers
  `origin + size·2^L·(col,row)` .. `+ size·2^L`; `col,row = k % (width>>L), k / (width>>L)`.
- `GridPartition__LoadingRadiusAt` (`0x68AE10`): a per-finest-cell byte map (64 m in Damascus).
- `GridStreamer__Update` (`0x55D810`): wanted = cells under `p ± radius` **plus all parents**
  (`0x68C3E0`, `0x55CC70`); priority `0.6·(1−cos view) + 0.4·d²/cell²` (`0x55D210`,
  `VIEW_WEIGHT` = `dword_18D6780`); everything unwanted unloads each plan, no hysteresis.
- **Hold**: while any wanted cell overlapping the half-cell box around the player is unloaded, the
  player is held at the last position.
- Plan refresh 0.25 s (a port choice); decoding on loader threads.

### 2.2 Distance
- `FakeEntities`: one per 4·cell-size block (128 m in Damascus), drawn at the block centre as a
  merged low-detail mesh, with `SubMeshFakeIndexSpans` — 16 `{first, count}` records per mesh
  section, one per finest cell, in Z-order. A span is hidden while its cell's real objects are
  loaded (`GridPartition__OnCellLoaded` `0x68B620`, `FakeEntities__SetCellLoaded` `0x5E4AB0`).
  The toggle is a hypothesis in the reference; the data fixes the mapping.

### 2.3 Objects, LOD, materials, collision
- An entity is `{u32 id, u32 class, 64-byte placement, components…}`; an `EntityGroup` keeps members
  inline, and **every member is decoded with its own world placement** (matches our group fix).
- `Visual` → `Mesh` or `LODSelector` of 5 `LODDescriptor {graphic, f32 distance, f32, u8}`; one
  `MeshInstanceData` per drawn mesh, each followed by `CompiledMeshInstance` colours.
- **Per-placement vertex colours**: nearly every placement carries its own baked colours, which is
  why placements of one mesh could not be instanced in the reference; they batch per material
  instead — and measured that batching saved only ~0.15 ms/frame standing still and cost more while
  moving, because Bevy's GPU batching already handles the draw count.
- Cull distances are per object size, from the settings divided by the graphics options
  (`Renderer__SetupCullDistances` `0xAC6E90`): small 12.5 m draw / 3.75 m shadow, medium 35 / 15.
- Ground is **layered material**: `Env_MultiBlender*` with up to three diffuse/normal layers blended
  by the vertex's layer weights, sampled at UV or world X,Y × tile factor.
- Textures stay **BC1/BC3 compressed on the GPU**; CPU decode is the fallback.
- Collision is shapes: `MeshShape`, `BoxShape`, `BarrelShape` (convex hull, `0x97CD8890`),
  `CapsuleShape` (`0xB8599052`), `ListShape` (`0x86EBFD8D`), attached by `InertComponent` /
  `RigidBodyComponent` with a matrix. Movement sees only objects within `PHYSICS_RADIUS` (40 m).
- Per-frame budgets: texture bytes (6 MB), LOD mesh vertices (15 000), physics triangles (40 000).

## 3. Our implementation, judged against that model

| Subsystem | Engine | Ours | Verdict |
|---|---|---|---|
| Cell set | hierarchical grid, box + parents, per-cell radius byte | flat centroid list, one CLI radius | **wrong** |
| Priority | 0.6·(1−cos) + 0.4·d²/cell² | none | missing |
| Hold | player held while ground cell loads | crude: refuse a step with no collider under it | **approximation**, not the rule |
| Distance | fake spans per cell | nothing | missing |
| LOD | selector distances + per-size cull distances + cross-fade + vertex budget | selector distance per placement only | partial |
| Materials | layered MultiBlender ground, per-placement colours, BC on GPU | single diffuse + normal, BC on GPU | partial |
| Collision | shapes + rigid bodies, 40 m physics window | AABB boxes from render bounds | **wrong** for walking |
| Draw model | batches per material (measured as break-even) | instancing per (piece, part, cluster) + frustum culling | **fine** — and better than batching, provided ACU placements carry no per-placement colours (to verify) |
| Textures | 6 MB/frame budget | pool + slice + cap, mirrors the ACU manager | good |
| Determinism | grid order is deterministic | fixed today with `BTreeSet` (0.131 % residual) | good |

Banked this session (all pushed on `acu-137-whole-map`): instanced renderer with cluster culling;
texture residency mirroring the ACU manager (`RVA 0x2222780`, 100 MiB, slice, cap); lighting from the
real `GlobalAmbientLight`/`DirectionalLight`s; vertex format 2 decoded from data; per-placement LOD;
EntityGroup placement rule; terrain containers expanded before the radius test; assets archive in the
streaming catalog (untextured 71 → 8); deterministic ring.

## 4. Work, in dependency order

### M1 — Decode ACU's grid (blocks everything)
Find the ACU equivalent of `GridLayout`/`GridPartition` in the type registry we located
(`0x4e9c800`, typed reader `sub_270FF90`). Signature to look for: a payload beginning
`{i32 cell size, i32 origin x, i32 origin y, ..., i32 width, u8 radius}`, and a partition of
`{u32 levels, u32 cells, cells × {col,row,level,datablock}}`.
Acceptance: for every cell in `paris-world-cells.json`, the grid computes a rect that contains the
cell's placements (checked against a sample of decoded instances); report the worst deviation.

### M2 — Replace streaming with the grid rule
Wanted = box ± **per-cell radius**, plus parents; priority 0.6/0.4; unload the unwanted each plan;
plan every 0.25 s; decode off-thread.
Acceptance: same deterministic capture as today (checksum printable), plus a report of cells wanted vs
loaded per plan and the priority order of the first ten.

### M3 — Hold, properly
While any wanted cell in the half-cell box around the player is unloaded, keep the player at the last
position; release when it loads.
Acceptance: instrumented walk records zero frames in an unloaded half-cell box.

### M4 — Fake spans
ACU's `FakeEntities` equivalent: merged low-detail meshes per block with per-finest-cell index spans,
hidden when the cell loads.
Acceptance: with the ring at 64 m, a capture at the same pose shows city geometry beyond the loaded
radius; count of visible fake spans vs loaded cells reported.

### M5 — Collision from shapes
Parse ACU's shape components (hull/box/capsule/list) into a collision world; feed the player and the
camera from it; keep a physics window around the player as the reference does (40 m).
Acceptance: the player stands on streets and roofs, cannot pass through walls, and the scripted walk
finishes 100 m without falling.

### M6 — Materials to match
Layered ground materials (3 layers blended by vertex weights, world-UV layers with tile factors),
per-placement colours **if ACU placements carry them** (verify from `CompiledMeshInstance`
equivalents before implementing), water, and the BC-upload path kept.
Acceptance: a street frame's ground is cobbled/blended rather than a flat tone; the same pose compared
against the game's own screenshot when one can be captured.

### M7 — LOD and budgets
Per-size cull distances from the graphics options, a per-frame LOD vertex budget, a physics triangle
budget, and (optionally) the cross-fade.
Acceptance: no frame over 33 ms while walking the 100 m route; LOD switches reported per frame.

## 5. Explicit non-goals

- A single frame containing all of Paris (the engine renders fakes instead).
- Byte-identical output to the game (different renderer); the oracle is the frame comparison in D1 plus
  the movement test.
- Re-deriving ACU's movement system: that is the AC1 reference repo's subject, not ours.

## 6. Harness

- Captures: fixed command, `capture-state` line (cells, checksum, player) for determinism; image stats
  via `py` (Pillow); SHA-256 for equality.
- Walk: windowed run plus `um win drive` keys; parse positions from the stream log; assert the hold.
- Never rebuild while a render holds the exe; stop by PID.
- Long decodes run in the background.
- Every result lands in this file's tables, not only in chat.

## 7. Risks and stop conditions

- If M1 finds no grid in ACU's data, fall back to reconstructing one from cell ids (`Cell04814` is
  spatial) and say so; do not fake per-cell radii.
- If M5's shapes are unavailable, AABB collision stays and is labelled as an approximation.
- If M4's fake data is absent for Paris, the horizon stays empty and D5 is dropped explicitly.
