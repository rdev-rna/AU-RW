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

## 6. Order of work

1. U1 measurement (one command, two runs, compare `capture-state`). **Blocks everything.**
2. U2 fix, then re-verify determinism to the byte.
3. U3 floating geometry with a clean, reproducible baseline.
4. U5 walk test and fps.
5. U4 occlusion, if 3/4 demand it.
6. D5 city-scale still, last, at a fitting setting.

## 7. Stop conditions

- If U1 shows the target set is genuinely unstable across identical runs, stop feature
  work and fix the streaming scheduler first; every visual judgement depends on it.
- If U3 turns out to be missing cells rather than bad transforms, the fix is load policy
  (ring margin), not placement — say so and change the policy.
- If D3 fails on fps only, the next step is U4, not more geometry work.
