# Plan — Paris 1:1 in Bevy, Arno walkable

Written 2026-10-25 after a session of hypothesis-testing. This document is the plan of
record: what is proven, what is unknown, how each unknown gets resolved, and in what
order. No step below is "try something and see"; each has a measurement and an
acceptance criterion.

## 1. Definition of done

| # | Criterion | Measurement |
|---|---|---|
| D1 | A fixed street-level capture contains no detached building parts | Two captures of the same pose, 24 h apart and after a rebuild, agree; every geometric connected component in the frame is smaller than N px **or** belongs to a declared prop list |
| D2 | Captures are deterministic | Two runs of the identical command produce byte-identical PNGs (SHA-256 equal) |
| D3 | Arno walks 100 m in the real window | Automated key input, 100 m of travel logged, **0** frames below 30 fps, no fall-through (y never drops below the ground cell), no panic |
| D4 | Streaming keeps the city textured | `parts [bound, textured]` shows >= 99% textured and <= 2% unbound in the loaded ring |
| D5 | (stretch) A city-scale still exists | A single frame containing the whole map, at a setting that fits 8 GB, with the draw/LOD numbers recorded |

D5 is explicitly a stretch: a whole-city single frame is a workload the original engine
does not produce either (see §3).

## 2. Verified state (evidence on disk)

| Area | Status | Evidence |
|---|---|---|
| Instanced renderer | one entity per (piece, part, cluster); instance matrix buffer | `6f193e6`; cell 4884 IoU 1.00000 vs per-placement |
| Cluster culling | frustum test on explicit per-cluster AABB | `cull: parts=2956 clusters=3568 submitted=2549 culled=633` (`local/cull-street.log`) |
| Texture residency | pool/slice/cap queue, generation-correct rebinds, mirrors engine policy | `d91b68d`; engine policy at `RVA 0x2222780` (pool 100 MiB, slice 2000, cap 5) |
| Lighting | from `GlobalAmbientLight` + both `DirectionalLight`s | `92f8a49`; street means 127.2/154.5/185.3 vs reference 135.7/153.9/174.0 |
| Vertex format 2 (stride 40) | decoded from dumped data | `8909182`; 0 unsupported / 0 undecoded in a streamed run |
| Per-placement LOD | selector switch distance per placement | `8ec2937` |
| EntityGroup placement | absolute vs relative per object, row/column normalisation | `b752f07`; rejects 34-39 -> 0 |
| Terrain containers | expanded before the radius test | `8ec2937` |
| Streaming catalog | assets archive included for `--map-stream` | `5f21e5a`; untextured 71 -> 8, 73,891/74,097 parts textured (99.7%) |
| Capture gate | waits for the ring to finish (`pending == 0`, written after the load loop) | `ff5f686` + uncommitted fix |
| Whole-map 4K still | **fails** | `Validation Error` / GPU resource exhaustion at 1.08M instances, 3 GB pool, 20.6 min |

## 3. What the original engine does (so we stop aiming at the wrong target)

Recovered from the runtime dump (`ACU-RE-0039`):

- draws **compiled mesh instances** — one mesh, N matrices (`CompiledMeshInstanceStream`,
  `MeshInstanceData`);
- culls **clusters** every frame: `Graphic::VisualsCulling`, `VisualsCullingQT` (quadtree,
  `QTVisualsCullingTrivialAccept`), `UpdateEntitiesCullingFlags`, `BatchCullingSettings`;
- occlusion via `GfxHierarchicalVisibility::VisibilityDepthSurface` (HiZ), with indirect
  draw variants named `ClusterCullingNoOcclusion` / `ClusteredInstanceCullingNoOcclusion`;
- **texture streaming as a scheduled frame stage** with a 100 MiB pool and a per-frame
  budget (`RVA 0x2222780`);
- renders at 900p (console) / ~1080p (PC), never a whole-city 4K frame.

Consequence: D5 is a stretch goal, and D1/D3 must be judged from a player viewpoint.

## 4. Unknowns, and the exact method for each

### U1 — the ring sometimes loads 64 cells and sometimes 186 at the same spawn and radius
Evidence: `local/det4-p.log` = `total 64 cells`; `local/ring-full.log` = `total 186 cells`;
both `--map-stream 150 --spawn 100,20,300`.

Method:
1. The `capture-state:` line (added, unverified) prints `cells`, a sorted checksum of the
   loaded entries, and the player position at capture.
2. Run the identical command twice; if the player positions differ, the freeze is not
   holding (investigate `player::State::capture`); if they agree but the checksums differ,
   the target set itself is unstable (investigate `stream.centres` population and the
   `Vec2::new(x, -y)` mapping).
3. Acceptance: two runs print the same `cells` and `checksum`.

### U2 — captures are not deterministic (27-37% pixels)
Depends entirely on U1. No further work until U1 is answered; anything else is measuring
noise.

### U3 — detached props and building fragments in the street view
Only meaningful once U1/U2 are fixed, because a half-loaded ring photographs as fragments.

Method when U1/U2 are clean:
1. Capture the fixed pose; list the geometric components that are isolated (a small
   Python connected-component pass over the non-sky mask).
2. For the largest offenders, get their world position from the image ray and match it to
   the nearest spawned instance (dump instances with labels to a file behind an env flag).
3. Decide per case: wrong transform (fix the placement rule), missing neighbours (load
   policy), or legitimate prop (declare it).
4. Acceptance: D1.

### U4 — no occlusion culling
The engine has HiZ; we have frustum only. Needed only if D3's fps fails or D1 shows
overdraw artefacts.

Method:
1. Decompile the `GfxHierarchicalVisibility` path and the `ClusterCullingNoOcclusion`
   pipeline variants to learn the depth-pyramid construction and the test.
2. Implement as a low-res depth prepass + CPU/GPU cluster rejection against it.
3. Acceptance: same frame, same pixels (minus sub-pixel), fewer submitted clusters.

### U5 — movement has never been measured
Method: automated windowed run with scripted `hold W`, log player position each second,
compute distance, fps and `y` relative to the ground; assert D3.

## 5. Harness (so this runs unattended)

- Every experiment: fixed command, log to `local/<name>.log`, image stats via `py`
  (Pillow) — mean/stddev/distinct/modal, plus SHA-256 for determinism.
- Never rebuild while a render holds `target\release\acu_bevy.exe` (Windows locks it);
  stop the process first by PID.
- Long runs (whole map) go to the background; nothing else builds meanwhile.
- Each result lands in this file's status table, not in chat only.

## 6. The map system, corrected against AC1's verified implementation

Reference: `Banned445/AC1-Movement-Rewritten` (clean-room AC1 in Bevy, streams all of Damascus from the
install). Its `src/city/grid.rs` and `src/city/stream.rs` carry exe addresses and archive observations,
so these are engine rules, not preferences.

### The real model

1. **The world is a hierarchical grid, not a list of centroids.**
   `GridLayout` (class `0x7DB083ED`): cell size, origin, and the finest level's width in cells.
   `GridPartition` (`0x0E2F4444`): `levels`, `cells`, each `GridCell { col, row, level, datablock, dependencies }`.
   Cell `k` of level `L` covers `origin + size·2^L·(col,row)` .. `+ size·2^L`, with
   `col,row = k % (width>>L), k / (width>>L)`. Each level halves the width and doubles the cell size, so
   large objects live in coarse cells.
2. **The loading radius comes from the data, per cell.**
   `GridPartition__LoadingRadiusAt` (`0x68AE10`) reads a byte map indexed by the finest cell under the
   player; the `GridLayout` byte at `+20` is the radius outside the grid. In Damascus it is **64 m**.
3. **Wanted = the box, plus parents.**
   `GridPartition__IterCellsInBox` (`0x68C3E0`) + `GridStreamer__RequestCellAndParents` (`0x55CC70`):
   every cell of any level overlapping the square box `p ± r`, and all their parents.
4. **Priority is view-weighted, not pure distance.**
   `GridStreamer__RequestCellsInBox` (`0x55D210`):
   `score = 0.6·(1 − cos(view angle)) + 0.4·d²/cell²`, sorted ascending (`VIEW_WEIGHT` = `dword_18D6780`).
5. **Everything not wanted unloads, every plan, no hysteresis** (`GridStreamer__Update` `0x55D810`).
6. **The player is held while the ground under him is still loading** — the single most important rule we
   were missing. While any wanted cell overlapping the half-cell box around the player is not loaded,
   `hold` keeps the player at the last position (`stream.rs:375-379`).
7. **Distance is covered by fake meshes, not by drawing the city.** A loaded finest cell hides its fake
   spans, unloading shows them (`GridPartition__OnCellLoaded` `0x68B620`,
   `FakeEntities__SetCellLoaded` `0x5E4AB0`) — that is what fills the horizon.
8. **Budgets per frame, not per load**: texture bytes (`6 MB`), LOD mesh vertices (`15 000`), physics
   triangles (`40 000`), and shadow/cull distances by object size (`SMALL_CULL` 12.5 m,
   `MEDIUM_CULL` 35 m).

### What our implementation does instead

| Rule | Ours |
|---|---|
| hierarchical grid with parents | flat list of cell centroids from `paris-cell-centres.json` |
| per-cell loading radius from data | one CLI `--map-stream` radius for everything |
| wanted = box + parents | distance from centroid ≤ radius |
| view-weighted priority | none (set iteration order, now `BTreeSet`) |
| hold while the ground loads | **absent** — the player walks off into unloaded space |
| fake spans for the distance | absent — the horizon is empty |
| per-frame budgets | texture only; geometry/LOD unbudgeted |

### Consequence

Point 6 explains the walk test exactly: Arno walked 152 m into a region whose cells were still loading and
there was nothing under him, because nothing holds him and no fake spans cover the gap. Points 1–2 explain
why spawn points looked empty: I was picking a coordinate and hoping geometry existed at it, instead of
asking the grid which cell contains the point and what its radius is.

### Next steps, in order

1. **RE ACU's grid.** In the ACU dump, find the equivalent of `GridLayout`/`GridPartition`: a payload with
   cell size, origin, width, level count, and a per-finest-cell radius byte map. Our cells already carry
   spatial ids (`Cell04814`), so the grid exists in the data; we have simply never decoded it. Search the
   type registry for a class whose payload is `{i32 size, i32 origin_x, i32 origin_y, i32, i32 width, u8 radius}`
   and for a partition `{u32 levels, u32 cells, cells × {col,row,level,datablock}}`.
2. **Replace centroid-radius streaming with grid streaming**: cells from the box ± their own radius, plus
   parents, ordered by the 0.6/0.4 priority.
3. **Implement the hold.** While any wanted cell under the player's half-cell box is unloaded, keep the
   player at the last position. This alone removes the walk-into-the-void failure.
4. **Fake spans** for the horizon (ACU equivalent of `FakeEntities__SetCellLoaded`).
5. Only then re-test determinism, the walk, and the visuals.

## 7. Order of work (superseded by section 6)


1. U1 measurement (one command, two runs, compare `capture-state`). **Blocks everything.**
2. U2 fix, then re-verify determinism to the byte.
3. U3 floating geometry with a clean, reproducible baseline.
4. U5 walk test and fps.
5. U4 occlusion, if 3/4 demand it.
6. D5 city-scale still, last, at a fitting setting.

## 8. Stop conditions

- If U1 shows the target set is genuinely unstable across identical runs, stop feature
  work and fix the streaming scheduler first; every visual judgement depends on it.
- If U3 turns out to be missing cells rather than bad transforms, the fix is load policy
  (ring margin), not placement — say so and change the policy.
- If D3 fails on fps only, the next step is U4, not more geometry work.

