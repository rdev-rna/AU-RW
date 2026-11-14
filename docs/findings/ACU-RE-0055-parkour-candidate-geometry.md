# ACU-RE-0055 — What world geometry produces a parkour candidate

Date: 2026-11-14 / investigator: Claude (subagent, offline only) / related: backlog item 5 of
`docs/REWRITE_READINESS.md`, [ACU-RE-0042](ACU-RE-0042-parkour-candidate-trace.md) (traces),
[ACU-RE-0043](ACU-RE-0043-parkour-action-classes.md) (types), [ACU-RE-0045](ACU-RE-0045-decision-clips-and-window-sweep.md),
[ACU-RE-0046](ACU-RE-0046-player-teleport-and-vault-scan.md), [ACU-RE-0050](ACU-RE-0050-world-collision-ground.md) (collision).
State: partially established. The traces already held each candidate's world positions. Their
targets lie on decoded collision triangle edges. Those edges are convex, with a walkable top and
a steep side. The routine that fills the candidate is identified. The routine that picks the
edges, and the per-type inclusion rules, are not.

## Provenance

- No game process was launched, attached to or read. All live data comes from runs captured
  earlier (listed below) and read from `local/native-probe/runs/`.
- Installed `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
  image base `0x140000000` (the live base was the same in every run). Mapped image
  `E:/ReverseEngineering/Projects/Unity/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`,
  SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520` (rehashed for this
  finding). VA = 0x140000000 + offset.
- Corpus (`ACU - Decompiled/`, 217,041 files):
  - `1401B1770.c` (110 lines, SHA-256
    `e8fbb7bbf6c8b1445a1ba9cdfc6ddca3e057ff778203b61204d836fa9d3075f3`), lines 22–30 and 49–89;
  - `14019F920.c` (SHA-256 `e6568ae1e17cc98c53f2b15066a3a7c4bc12fc205e63263f1a42a813162b1d86`).
  - Search: a Python scan of every file for writes to `+ 192)`, `+ 196)` and `+ 200)` in the
    same function together with `sqrt` found 8 files. `14017BAA0`, `14019F920`, `1401B1770`,
    `1401B4120`, `1401D43C0`, `140FDEA20`, `141CFAA00`, `1426152D0`. Only the two named above
    were read. `rg` is not on this machine's bash PATH.
- Disassembly of `0x1401B1770` came from the mapped image (Capstone, x64); it agrees with the corpus.
- Trace inputs: the 17 runs whose `native-session.jsonl` holds `parkour_select_entry` events
  (profile `tools/native_probe/parkour_selection.profile.json`, 16 × 672-byte candidates gathered
  per call). Each run name is followed by the SHA-256 of its session file:
  `catalog-1/scenario` `0b9846a5…7946`, `nav-1/pair-1` `a256f23d…7f15`, `parkour-1` `c8705d61…6c5a`,
  `parkour-2-preset` `b2d4693d…fd`, `parkour-5-runner/scenario` `143ce084…6e2f`,
  `pinned-1a-putback-blocked/scenario` `1e42ff8f…bf`, `pinned-2a/scenario` `bd915b67…9c29`,
  `pinned-2b/scenario` `eca4f3d6…ea6ea`, `pinned-3a/scenario` `1477df25…a7665`,
  `pinned-3b/scenario` `7aac2f45…c09`, `pinned-5/scenario` `ebac42e2…e83`,
  `prop-probe-1` `d941ebc3…987`, `vault-scan-1` `a2387b5e…63`, `vault-scan-2` `8d2cb9db…97e8`,
  `vault-scan-3` `cbae13a3…94c7a`, `window-sweep-1` `95e490ef…44103`, `window-sweep-2`
  `8392bcd4…49e`. The full hashes are in the fixture's `runs` array.
- World: every run is in **Versailles**. All sources fall inside x −71.5..158.8 and
  y −331.5..−130.3. Paris starts at x ≈ 370 (ACU-RE-0050). 3,836 of the 5,242 distinct rows
  stand on decoded Versailles ground within 5 cm; most of the rest are climbing holds. The
  pinned save is the Versailles one (ACU-RE-0046).
- Collision: `tools/world_collision.py` dumps under `local/world-collision/DataPC_ACU_Versailles/`.
  The archives are `DataPC_ACU_Versailles.forge`, `DataPC_ACU_Versailles_assets.forge` and
  `DataPC_ACU_Versailles_assets_patch_01.forge`. Asset index 2026-10-22.1; type labels
  `crc-name`. Example shapes and bodies (16-hex file IDs):
  - MeshShape `0000001211859e1e` `FRV_House_D_1F_CL01` under EntityGroup `0000001a74b341f9`
    `FRV_House_Row_D_07x13x2_Open01` (Versailles entry 1858);
  - MeshShape `000000101d7c1c94` `GEN_ALL_GardensOrnateKitG_02` under Entity `0000001d3fa56d3b`
    `GEN_ALL_GardensOrnateKitG_02_096` (Versailles entry 1998);
  - MeshShape `0000000ee6436192` `FRV_House_D_1F_WD_02`.
- New tools: `tools/parkour_geometry.py` (SHA-256 `ec747299…7938`) and
  `tools/test_parkour_geometry.py` (30 tests, synthetic data, `4f4f5fdf…04f4`). Fixture
  `data/oracles/parkour_geometry.json` (schema `acu-oracle/parkour-geometry/1`, 5,242 rows,
  1,699,937 B, SHA-256 `1fc45c1e3c033a04922f59e7b34269b4de7ff8bc749d98250fe4d6e2442c2f29`).
- Commands (project root):

  ```powershell
  python tools/parkour_geometry.py fields                 # field identities, no collision
  python tools/parkour_geometry.py analyze --write data/oracles/parkour_geometry.json   # ~90 s
  python -m unittest tools/test_parkour_geometry.py
  ```

## Result

### Observations: the candidate carries world geometry

The probe gathered 10,804 candidate snapshots at entry (4,163 entry events, 4,161 exit events,
807 selections). There are 10,918 distinct 672-byte snapshots, counting the selected candidate
captured at exit. Each identity below was checked on every snapshot (`fields`).

| Offset | Content | Holds on |
| --- | --- | --- |
| `+0x10` | source point (f32×3) | — (see below) |
| `+0x30` | target point | — |
| `+0x40` | unit vector, horizontal | unit 10,918/10,918; z = 0 on 9,605 |
| `+0x50`, `+0x60` | segment A, B; target lies on A..B | 9,603 (1,314 degenerate A = B) |
| `+0x40` ⟂ A..B | | 9,604 |
| `+0xC8` = hypot(`+0xC0`, `+0xC4`) | | 10,918/10,918 exactly |
| `+0xC0` = target.z − source.z | | 10,112 within 2 mm; p95 3.5 mm, p99 7 cm |
| `+0xC4` = horizontal \|target − source\| | | 10,541 within 2 mm |
| `+0x190`, `+0x1A0` = target | | 10,673 / 10,761 |
| `+0x180` = target | | 3,963 (never for types 53, 54, 39, 68) |

- **The source is usually the player.** 6,432 snapshots could be paired with a live-state
  sample within 50 ms. Of those, `+0x10` is within 0.15 m of the player position in 4,778
  (median 2.6 cm). In the rest, the source is a hold. Example (vault-scan-3): while climbing,
  the source is (72.294, −280.716, 3.103), the previous call's type-53 target.
- **Writer identified.** Slot 1 (id `0x1`) of every one of the 43 live fancy tables points
  at `0x1401B1770`. Types 14, 53 and 64 also have `0x14019F920` at slot id `0x47`. The table
  entry is the 0x20-byte layout of ACU-RE-0041/0042, with the function at +0x10. No direct
  `E8`/`E9` callers exist; the slot is reached through the table. `0x1401B1770(cand, ref, a3,
  rec)` (`1401B1770.c:22–30`; disassembly `0x1401B177F..0x1401B1853`) copies parts of a record
  into the candidate:
  - rec+0x00 → +0x10, rec+0x10 → +0x20, rec+0x20 → +0x30, rec+0x30 → +0x40;
  - rec+0x60, +0x70, +0x80 → +0x180, +0x190, +0x1A0;
  - rec+0x90 → +0x70, rec+0xA0 → +0x80.

  It then computes `d = cand+0x30 − *ref` (`subps xmm6,[rdi]` at `0x1401B18E9`) and stores
  d.z → `+0xC0`, the xy length → `+0xC4` and the 3D length → `+0xC8` (lines 49–89).
  **The reference is an argument, not `+0x10`.** That explains the mm-size per-call residuals:
  they are constant within a call (for example −6 mm for every candidate when the source z is
  4.415). `+0x50` and `+0x60` are **not** written by this routine.
- Type 52 (18 rows) reproduces its scalars from `+0x180`, not `+0x30`. `0x14019F920` reads its
  target from `+0x300` (`14019F920.c`, `a1 + 768`), outside the 672 gathered bytes.

### Observations: candidate geometry against decoded collision

There are 5,242 rows, one per distinct (action type, source, target) at 1 cm. Each target was
joined to the triangles of `tools/world_collision.py` (MeshShape only; primitives not decoded).

| Measure | Rows |
| --- | --- |
| a decoded triangle edge within 5 cm of the target | 4,830 (92.1 %); 10 cm: 4,959; none within 0.3 m: 0 |
| ... and that edge is a **convex ledge** (top face \|nz\| ≥ 0.7, other face \|nz\| ≤ 0.3) | 4,283 |
| both segment ends on that edge (within 5 mm) | 4,534 |
| segment ends that coincide with the edge's mesh vertices (2 / 1 / 0 ends) | 2,485 / 1,940 / 235 |
| target at the segment midpoint (within 1 cm) | 2,641 |
| profile `ledge_minus`: a walkable top within 10 cm of target z, 5 cm on the **−`+0x40`** side, and a drop of at least 0.5 m 5 cm on the + side | 3,735 (the reverse, `ledge_plus`: 2) |
| selected candidates: edge within 5 cm / convex ledge / `ledge_minus` | 749 / 677 / 627 of 799 |
| rows with a non-degenerate segment (A ≠ B) that have an edge within 5 cm **and** parallel to A..B within 10° | 4,660 of 4,660 |
| rows with no edge within 5 cm | 412, every one with a degenerate segment (A = B) |

Per type (rows, from `analyze`). c0 = height above the source; c4 = horizontal distance:

| Type | Rows (selected) | Edge ≤ 5 cm | Convex ledge | c0 p10/p50/p90 (m) | c4 p50 (m) | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| 53 | 1,917 (194) | 1,917 | 1,823 | 2.19 / 3.42 / 4.59 | 1.23 | holds 2–5 m up a facade, often stacked at one xy |
| 8 | 758 (55) | 554 | 396 | −6.90 / −1.99 / 0.32 | 4.32 | 1,004 snapshots with a degenerate segment; long and downward |
| 65 | 731 (107) | 731 | 680 | −0.47 / 0.65 / 2.44 | 1.01 | |
| 3 | 277 (27) | 124 | 71 | −0.99 / −0.51 / −0.09 | 1.37 | mostly degenerate segment, `top_both` (flat ground) |
| 9 | 268 (185) | 268 | 242 | 0.53 / 1.44 / 2.41 | 1.31 | step-ups (`Run_JumpOn*`, ACU-RE-0046) |
| 35 | 267 (17) | 267 | 247 | −2.08 / 0.81 / 2.33 | 6.05 | |
| 54 | 159 (10) | 159 | 142 | 3.45 / 4.50 / 5.01 | 1.53 | |
| 73 | 104 (51) | 104 | 93 | ≈ 0 | 0.00 | target = source: the ledge Arno is on |
| 14 | 67 (9) | 67 | 39 (+28 convex, other face \|nz\| ≈ 0.33) | 1.00 / 1.08 / 2.08 | 0.66 | vault (`Sprint_SkipOver`, ACU-RE-0046) |
| 32, 44 | 32, 12 | all | 29, 11 | ≈ 0 | ≈ 0 | target at the source |
| 69 | 25 (5) | 8 | 0 | −4.9 / −2.8 / −2.0 | 1.66 | flat ground below (`top_both`), not an edge |
| 52 | 18 (17) | 3 | 0 | 0.66 / 1.00 / 1.27 | 0.70 | target is not on an edge (`+0x180` is the scalar target) |

All other types (fewer than 90 rows each) are 90–100 % "edge within 5 cm, convex ledge". In the
fixture, the matched edges come from Entity.RigidBody shapes in 2,565 rows and from
ShadowRigidBody shapes in 2,266. Building facades and props both produce candidates.

Horizontal fan rays (inference test, sources on the ground). For type 53 (1,746 rays per
height), a ray cast toward the target from heights +0.75 / 1.0 / 1.25 / 1.75 m hits a decoded
face within 0.3 m of c4 in 998 / 1,049 / 1,044 / 1,093 cases. The median hit is at c4 − 0.0 m:
the wall under the ledge. For type 9 at +0.75 m: 193 of 252. For type 54 the rays mostly miss,
because its ledge is 3.4–5 m up.

### Inferences and alternatives

- A candidate is built from a **convex collision edge whose top face is walkable and whose
  side is steep**: a ledge lip. The data:
  - its segment `+0x50..+0x60` is a span of a decoded MeshShape triangle edge, often the whole
    edge (both ends on mesh vertices);
  - `+0x40` is the horizontal normal pointing out of the ledge, away from the top face;
  - the target `+0x30` is a point on the span, often its midpoint.

  The zero-millimetre match means the scanner's edges derive from the same MeshShape vertices
  the port decodes. It does not show whether the game reads the triangles at run time or a
  precomputed edge set built from them; `ClimbAnalysisData` and `ParkourAnalysisData` are named
  in ACU-RE-0030.
- Types with a degenerate segment and a target on flat ground (3, 8 in part, 69, 48, 15 in
  part) look like landing points (cf. the landing score in ACU-RE-0030), not edges.
- 2,019 of the 4,660 matched spans do not have the target at the midpoint. Over all matched
  rows, the target's position along the span (u) has p10..p90 of about 0.26..0.73, depending on
  type. The rule that places it (projection of the input direction? clipping?) is not
  established. Segments with 0–1
  ends on mesh vertices suggest clipping, for example to a scan window or a neighbouring
  obstacle.
- The fan rays of ACU-RE-0050 hit the wall under the ledge at the target's horizontal distance.
  That fits their being wall probes for the scanner. It does not prove they generate the
  candidates.

### Unknowns / incomplete coverage

- The caller of fancy slot 1 (who builds the record), how the edge set is enumerated, and the
  inclusion rules per type: height window, range, facing, input direction. Negative examples
  (convex ledges near the player that did *not* become candidates) are not analysed. Only calls
  with 16 or fewer candidates are complete, because the probe gathers 16.
- Who writes `+0x50`/`+0x60`, and the meaning of `+0x20`, `+0x70` (equal to the horizontal unit
  source→target on 3,866), `+0x80`, `+0x180` (when not the target) and `+0xCC`.
- 412 rows (7.9 %) have no decoded edge within 5 cm. All of them have degenerate segments:
  types 8 (203), 3 (153), 69, 52, 48, 15, 39. They look like point targets (landing points),
  not edges. Whether some edge candidates sit on primitive shapes (not decoded, ACU-RE-0050)
  cannot be told from these runs, since every non-degenerate row matched a mesh edge.
- Versailles only. Paris needs a Paris save (ACU-RE-0046).
- The thresholds (2 mm, 5 cm, 10 cm, |nz| 0.7 / 0.3, 0.5 m drop, 5 cm side offset) are analysis
  choices, recorded in the fixture's `choices`.

### Independent comparison / falsification

Positions came from the game's own candidate objects. Collision came from the archives through
a decoder validated only against ground heights (ACU-RE-0050), never fitted to candidates.
Errors in either would scatter the targets by metres. Instead, 92 % sit within 5 cm of a
triangle edge and the p90 distance is 0.0 mm. Every non-degenerate candidate segment (4,660
rows) has a decoded edge within 5 cm that is parallel to it within 10°. A wrong `+0x40` sign would put the top on the
+ side; that happened in 2 rows against 3,735.

### Reproduction

1. `python tools/index_status.py` (READY); `python tools/world_collision.py extract` if the
   Versailles dumps are missing.
2. `python tools/parkour_geometry.py fields`, then `analyze --write data/oracles/parkour_geometry.json`.
3. `python -m unittest tools/test_parkour_geometry.py`.

## Live probe that would close the generator gap (plan only, not run)

The probe uses `tools/native_probe/probe.cpp` hardware breakpoints. Its register indices are
0 rcx, 1 rdx, 3 r9, 6 rsi, 7 rdi, 9 rsp. The return address is logged on every hook.

| Slot | VA (RVA) | Guard (16 bytes) | Capture |
| --- | --- | --- | --- |
| 0 | `0x1401B1770` (`1b1770`), entry of fancy slot 1 | `48895c24084889742410574883ec4041` | snapshot r9 (index 3), 256 bytes: the record. The return address names the **generator**; rcx is the candidate. |
| 1 | `0x1401B1783` (`1b1783`), before rcx changes | `488bfa488bf10f294110410f28491049` | snapshot rcx (0) + 0x50, 32 bytes: whether `+0x50`/`+0x60` are already set when the record is copied |
| 2 | `0x1401B18E9` (`1b18e9`), `subps xmm6,[rdi]` | `0f5c370f28c60f28ce0fc6c6aaf30f11` | snapshot rdi (7), 16 bytes: the reference point used for `+0xC0..+0xC8`; rsi = candidate |
| 3 | `0x140133B00` (`133b00`) as today | as in `parkour_selection.profile.json` | the candidate lists, to join on the candidate pointer |

Run it for about 30 s against Versailles with the scripted RT+A moves (`capture_run.py`). The
result gives the generator VA, which is the next corpus/disassembly target for the
edge-enumeration query. It also gives the record layout and the reference point. A second pass
can probe the generator itself (its edge query and the clip of A..B). A hardware execute
breakpoint on a hot routine serialises hits; expect drops, as in ACU-RE-0042.

## Implementation implications and finding-linked constants

- Candidate fields: `+0x10` source, `+0x30` target, `+0x40` outward horizontal normal, `+0x50` /
  `+0x60` span, `+0xC0` = target.z − ref.z, `+0xC4` = |xy|, `+0xC8` = |3D| (from `0x1401B1770`,
  where ref is an argument).
- Port recipe (implementation choice backed by the counts above). Find candidate edges among
  MeshShape triangle edges (ACU-RE-0050 composition): convex, top face |nz| ≥ 0.7, other face
  |nz| ≤ 0.3. Target = a point on the edge span; normal = horizontal, pointing away from the top
  face. Per-type windows: the c0/c4 quantiles above, as starting values only (observed ranges,
  not limits).

## Development handoff

- **Confidence:** observed for the geometry join (5,242 rows; 92.1 % on a decoded triangle edge
  within 5 cm; 4,283 convex ledges) and for the writer `0x1401B1770` (corpus, disassembly and
  43 live fancy tables). Inferred: candidates are produced from convex collision edges.
  Unknown: the generator, its enumeration and its per-type inclusion rules.
- **Established:**
  - The 672-byte candidate holds the source `+0x10`, target `+0x30`, outward horizontal normal
    `+0x40` and span `+0x50..+0x60`. `+0xC8` = hypot(`+0xC0`, `+0xC4`) on 10,918/10,918.
  - Fancy slot 1 = `0x1401B1770` copies a record (rec+0x00/0x10/0x20/0x30/0x60/0x70/0x80/0x90/0xA0
    → +0x10/0x20/0x30/0x40/0x180/0x190/0x1A0/0x70/0x80). It computes `+0xC0..+0xC8` from `+0x30`
    minus an argument point. Slot id 0x47 = `0x14019F920` for types 14, 53 and 64.
  - Edge targets sit on decoded MeshShape triangle edges. The edge is a convex ledge with a
    walkable top on the −`+0x40` side. The span is part of the mesh edge (both ends on vertices
    in 2,485 rows).
  - All 17 candidate runs are Versailles.
- **Not established:** the generator (caller of slot 1) and how it enumerates edges; who writes
  `+0x50`/`+0x60`; the target placement rule off the midpoint; per-type inclusion (height,
  range, facing, input); `+0x20`, `+0x70`, `+0x80`, `+0x180`, `+0xCC`; landing-point types (3, 8,
  69, …); candidates on primitive shapes; Paris.
- **Interface:** in `acu_sim` (Bevy-free), `ledge_edges(collision, region) -> Vec<LedgeEdge { a,
  b, outward_normal_xy, top_nz, side_nz, shape: FileId, body: FileId }>` over the ACU-RE-0050
  `CollisionInstance` triangles. Then `Candidate { action_type, source, target, normal, span,
  height: f32 (C0), horizontal: f32 (C4), distance: f32 (C8) }` with the scalars computed from
  an explicit reference point. Keep file IDs as 16-hex strings in fixtures. Per-type selection
  windows stay explicit parameters until the generator is decoded.
- **Oracle:** `data/oracles/parkour_geometry.json` (`acu-oracle/parkour-geometry/1`), regenerated
  by `python tools/parkour_geometry.py analyze --write …`. A Rust `ledge_edges` passes when it
  meets two conditions:
  - every row with a non-degenerate segment (4,660) has an emitted edge within 5 cm of
    `target` that is parallel to `segment_a..segment_b` within 10°, and is classified as a
    convex ledge wherever the fixture's `edge_class` says `convex_ledge`;
  - `+0xC0..+0xC8` recomputed from `source` match `c0`/`c4`/`c8` within 2 mm on the
    4,534 `scalars_reproduced` rows.
- **Spawned / follow-up:**
  - Run the four-slot live probe above. Then decode the generator from its VA.
  - An offline negative-example pass. For complete calls (16 or fewer candidates), list the
    decoded convex ledges near the source that did not become candidates, to bound range and
    height per type.
  - Decode BoxShape and the other primitives (ACU-RE-0050), then rerun `analyze`.
