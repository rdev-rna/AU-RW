# HAND-OFF — Full reverse engineering of Assassin's Creed Unity

For a new agent starting an exhaustive RE of ACU with the goal of a 1:1 Bevy rewrite
("Paris, walkable, as the game is"). This document is the complete state: what exists, what is
proven, what is disproven (do not repeat those), the tooling, and the RE programme.

Read §0–§4 before touching anything. §6 is the work programme. §8 is the list of traps that have
already cost this project days.

---

## 0. Ground rules

1. **No guessing.** Every claim about code, offsets, formats or behaviour must come from a
   decompile (`ida` MCP), a byte-level read of the data, or a live probe. If it is inferred, the
   document says so and so must you.
2. **Negative results count as results.** §5 lists what has been ruled out. Re-testing them wastes
   days.
3. **The oracle comes first.** Before changing behaviour, write down what decides PASS/FAIL
   without a human opinion (byte diff, hash, IoU, fps, hold violation). §7 has the harness.
4. **The real game is the reference.** Anything ambiguous in the data can be settled by reading the
   running process, if a signed-in session is available (see §3.4).

---

## 1. Where everything is

| Thing | Path / value |
|---|---|
| Rewrite repo | `C:\Users\Administrator\AU-RW`, branch `acu-137-whole-map`, PR **#138** |
| Rust workspace | `<repo>\acu-rs` (crates: `acu_bevy` = app, `acu_assets` = forge/mesh/texture, …) |
| Release binary | `<repo>\acu-rs\target\release\acu_bevy.exe` |
| Game install | `C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity` |
| `ACU.exe` | 30,907,608 B, SHA-256 `748ABDC6456397FE7D8080F975F2A5ED3123BCE548E7BF1C68B23F47E8B27E40`, **no version resource**; patch forges `_patch_01..03` suggest 1.5.0 |
| Runtime dump / IDB | `<repo>\local\dump\ACU_dump_raw.i64` — md5 `d333718cc428c8dbf10c3fddf90c1947`, sha256 `4a6f7026a903368be405770be36470747254b3fd8aaba223f023ea80774e5c0d`, imagebase `0x0`, image size `0x7825000`, **215,244 functions**, 90,641 strings, single segment |
| Asset index (SQLite) | `<repo>\local\asset_index.sqlite` — 1.9 GB; tables `meta, archive, entry, resource(3,289,299), texture(65,479), link(12,575,691), type_label(484), loose_file` |
| Cell list | `<repo>\local\paris-world-cells.json` — 8,330 `{entry_index}` rows |
| Cell centroids | `<repo>\local\paris-cell-centres.json` — 6,537 `{entry,x,y,placements}` |
| Cell extents | `<repo>\local\cell-bounds.txt` — 6,537 `entry min_x min_y max_x max_y` (native axes) |
| Whole-map cache | `<repo>\local\paris-full.cache` — 4,373,294,576 B, `ACUCACH1` v4 |
| AC1 reference implementation | `C:\Users\Administrator\Documents\Default Project\projects\assassins-creed\AC1-Movement-Rewritten` (remote `Banned445/AC1-Movement-Rewritten`); see §4 |
| Plan of record | `<repo>\docs\status\139-plan-paris-1to1.md` |
| This session's findings | `<repo>\docs\findings\ACU-RE-0039-…streaming-inventory.md` |

IDA is driven through the `ida` MCP server (IDA Pro 9.3, `mrexodia/ida-pro-mcp`, idalib mode).
`idb_open(input_path=…)` → session id; pass `database=<session>` on every call; `idb_close` at the
end. Sessions expire when idle; re-open rather than reusing a stale id.

---

## 2. Proven engine facts

### 2.1 Type vocabulary (hex class ids, from the dump and the index)

| id | Meaning | Evidence |
|---|---|---|
| `ac2bbf68` | `GridCellDataBlock` — cell child-id manifest | reader `sub_1DEEBB0` (18-byte guard), `sub_1DEF490` (9-byte) |
| `0984415e` | Entity — 78-byte header, one row-major affine world matrix at +14 | `entity_matrix()`; 470,431 resources |
| `51dc6b80` | LODSelector — 5 × 35-byte records (level, mesh id, switch distance at +22) | `selector_records()`; 531,523 resources |
| `415d9568` | World mesh (clustered) | `resolve_mesh()`; 203,276 resources |
| `85c817c3` / `d70e6670` | Materials / TextureSets | materials resolve through these |
| `3f742d26` | `EntityGroup` — members inline, each `{u32 id, u32 Entity}` | `spawn_group()` |
| `ec658d29` | Tag marking an object boundary inside a group payload | `group_base()`, `spawn_group()` |
| `b22b3e61` | Havok collision component (not render geometry) | docs/status/137 |
| `a2b7e917` | TextureMap | 65,479 texture rows |
| `651b5aa3` | `ACU_Worlds` — 62 `{u16 flag, u64 world id}` entries (guards/interiors) | dumped, `local/ACU_Worlds.bin` |
| `6caecc37` | `GameWorld_Tree` — 111 B tree, no grid | dumped, `local/GameWorld_Tree.bin` |

The type descriptor table sits at **`0x4e9c800`** (`ac2bbf68` at `0x4e9d308`, size field `0x12`);
handlers are reached only through it — **no static callers exist** for the readers. The generic
typed reader is `sub_270FF90(ctx, …, type_id, …)`.

### 2.2 Frame task graph

`sub_21A3930` registers `Graphic::GlobalUpdates` (`sub_219BA80`), `Graphic::PreUpdateNodes`
(`sub_219F4D0`), `Graphic::UpdateNodes` (`sub_21E75B0`), `Graphic::PostUpdateNodes` (`sub_219F450`),
`Graphic::EndGraphicTasks`, and **`Graphic::TextureStreamingManager` → `sub_21A8AC0`**.

### 2.3 Texture streaming (fully recovered, implemented)

- `sub_21A8AC0`: `mov rcx, cs:qword_523C6D0; test rcx,rcx; jz; mov rax,[rcx]; jmp qword [rax+0C0h]`
  — a virtual call on the manager singleton. Live: `qword_523C6D0` held `0x6CAE6F00` (a heap
  object **outside** the dumped image), whose vtable is `0x14309F1E0` → **RVA `0x309F1E0`**, and
  slot `+0xC0` = **RVA `0x2222780`** (the update).
- Config is deserialised by `sub_22055B0`: `u8 enabled` at `+32`, then four dwords at
  `+16/+20/+24/+28`. Live values: **`104857600` = exactly 100 MiB** at `+16`, 10, 2000, 5.
- `sub_2222780` accumulates elapsed time, compares against the float at `+24`, and when the slice
  trips walks a request array (`u16` count at `+44`) **stopping when `used(+280) >= budget(+28)`**,
  dispatching each texture as an async job `sub_26E4C80(…, 3, &qword_50A9600, sub_221B490, …)`.
- Manager shutdown clears the singleton in `sub_220DCD0`.

Implementation in the rewrite: `acu-rs/crates/acu_bevy/src/texture_stream.rs` (pool / slice / cap,
nearest-first, generation-driven bind-group rebuild).

### 2.4 LOD selection

`sub_43360`: for `0x51DC6B80` records it squares the record distance and bands it against
`dword_4E7E678` = `FLT_MAX, FLT_MAX, 400.0, 144.0, 64.0, 16.0` → cut-offs at **4, 8, 12, 20 m**
(band 5 nearest). This is *a* band table, not necessarily the world LOD ladder; the 12-byte record
here (distance at `+8`) differs from the 35-byte selector record (distance at `+22`). Do not
conflate them.

### 2.5 Meshes

- Part triangle indices are **absolute** into the piece vertex array (measured: `part_base=112`,
  `part_count=5`, `tri_min=112`, `tri_max=116`).
- Vertex formats: 0,1,4,5,6 were already mapped; **format 2 stride 40 was recovered from a dumped
  buffer** (`ACU_DUMP_VERTEX=1`, 1,234 vertices): positions at +0 with packed `w` always 32767,
  unit vectors at +8/+12/+16 (normal, tangent, binormal), UV pair at +20, 16 bytes of joint
  indices/weights at +24 — i.e. the same layout as format 1.

### 2.6 EntityGroup placement

- A group's member matrix is either an **absolute world placement** or a **local offset** from the
  header base. Rule now used: absolute iff its translation is within 50 m of the base; otherwise
  `base * local`.
- Measured over 4,160 groups / 26,748 objects: **4,177 absolute** (all > 100 m from the origin),
  **22,571 relative**; **no group fell back to an identity base** (0/4,160). The 50 m threshold has
  no ambiguous cases in this data.
- Row-major matrices (translation in slots 3/7/11) are normalised by `object_affine()`.
- `local * base` (row-vector order) was tested and is **wrong** — it scatters parts.

---

## 3. Tooling in the rewrite (use these; they exist because each saved a day)

### 3.1 Build / run

```
$env:RUSTUP_TOOLCHAIN='stable'          # rust-toolchain.toml pins a broken channel string
cd C:\Users\Administrator\AU-RW\acu-rs
cargo build --locked --release -p acu_bevy
$env:ACU_GAME_DIR="C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity"
```

### 3.2 Flags that matter

| Flag | Use |
|---|---|
| `--map-cells FILE --map-centres-only --map-cell-centres OUT` | census pass (~15 s for 8,330 cells) |
| `ACU_CELL_BOUNDS=path` (env) | per-cell extents from the same pass |
| `--map-cell N --capture out.png` | single-cell still, deterministic |
| `--map-stream M --map-player --map-instanced` | the playable windowed mode |
| `--map-instanced` | one entity per (piece, part, cluster) with an instance buffer |
| `--texture-pool-mb / --texture-slice-ms / --texture-per-frame` | residency queue limits |
| `ACU_DUMP_RESOURCE="<file id>;<path>"` | resolve and write one resource through the catalog |
| `ACU_DUMP_VERTEX=1` | dump an unknown vertex layout to `local/vertex-fmtN-strideM.bin` |
| `ACU_GROUP_TRACE=1` | per-object group placement data (`local/grouptrace.log`) |
| `ACU_INDEX_TRACE=1` | part index range vs base/count |
| `ACU_GROUND_TRACE=1` | player position, ring bounds, collider boxes, nearest box |
| `ACU_STREAM_TRACE=1` | radius / centres / target / loaded |
| `ACU_MAP_TRACE=1`, `ACU_MAP_MISSING=1`, `ACU_DUMP_MISSING=dir` | decode diagnostics |
| `ACU_MAP_GREY=1` | force grey material (isolates material from geometry) |

### 3.3 Captures and the walk test

Captures are deterministic **only** if the ring is complete: `request_capture` waits for
`stream.pending == 0` and prints `capture-state: cells=… checksum=… player=…`. Two runs of the same
command now differ by ~0.13 % (they were 27 % before `target` became a `BTreeSet`).

Walk test (windowed, automated):
```
$p = Start-Process .\acu-rs\target\release\acu_bevy.exe -ArgumentList '--map-stream','150','--map-player','--map-instanced','--spawn','100,20,300','--size','1280x720' -RedirectStandardOutput local\walk.log -RedirectStandardError local\walk.err -PassThru
Start-Sleep 40
& "$env:USERPROFILE\.local\bin\um.exe" win drive --proc acu_bevy "focus" "hold 0x57 30000"
& "$env:USERPROFILE\.local\bin\um.exe" win shot --exe acu_bevy.exe local\walk.png
Stop-Process -Id $p.Id -Force
```
Positions come from `stream: at (x,z)` lines; fps from `fps:` lines (stdout). **Note:** redirected
logs are UTF-16 when written by `Start-Process` — parse with `encoding='utf-16'`.

### 3.4 Live probing (blocked without a login)

Launching `ACU.exe` spawns Ubisoft Connect and demands the account password. Do **not** work around
that (DRM). With a signed-in session, the probe is:
`qword[base+0x523C6D0]` → manager → `qword[manager]` → vtable → `qword[vtable+0xC0]` → RVA.
Read-only `ReadProcessMemory` via PowerShell `Add-Type`; `tools/scan-memory.ps1` in the repo has the
scaffolding.

---

## 4. The AC1 reference implementation (use it as the design guide)

`Banned445/AC1-Movement-Rewritten` — a clean-room AC1 in Bevy that streams all of Damascus from the
user's install. Its `src/city/*` carry exe addresses, so its rules are engine facts:

| Rule | Where |
|---|---|
| Grid: `GridLayout` `0x7DB083ED` (cell size, origin, width, +20 radius byte), `GridPartition` `0x0E2F4444` (`{col,row,level}` per cell; level L spans `size·2^L`) | `city/grid.rs` |
| Wanted = box ± **per-cell radius byte** (`0x68AE10`, 64 m in Damascus) **plus all parents** (`0x68C3E0`, `0x55CC70`) | `city/stream.rs` |
| Priority `0.6·(1−cos view) + 0.4·d²/cell²` (`0x55D210`, `dword_18D6780`) | `city/stream.rs` |
| **Hold the player** while any wanted cell in the half-cell box is loading (`0x55D810`) | `city/stream.rs:375` |
| Fake meshes per 4·cell block with per-finest-cell index spans, hidden as cells load (`0x68B620`, `0x5E4AB0`) | `city/fakes.rs` |
| LOD by camera distance; per-size cull distances (`0xAC6E90`): small 12.5/3.75 m, medium 35/15 m | `city/stream.rs` |
| Ground = `Env_MultiBlender` layered materials (3 diffuse/normal layers, vertex weights, world-UV tiling) | `city/ground.rs` |
| Collision = shapes `Mesh/Box/Barrel(0x97CD8890)/Capsule(0xB8599052)/List(0x86EBFD8D)` | `city/shapes.rs` |
| Per-frame budgets: texture 6 MB, LOD 15k vertices, physics 40k triangles | `city/stream.rs` |
| Batching per material measured as **break-even** (0.15 ms saved still, slower moving) — Bevy's GPU batching already covers draw count | `city/stream.rs` header |

**ACU does not have the grid.** Tried and failed: the three AC1 class ids have zero resources in
ACU's index; `ACU_Worlds` and `GameWorld_Tree` contain no grid; a lattice fit over 6,537 cell
extents finds nothing (cells are content blocks, centroids are means of placements). The hold,
priority, fakes and budgets still transfer; the per-cell radius is a labelled approximation
(`HOLD_RADIUS = 40 m`).

---

## 5. Disproven (do not repeat)

| Claim | Result |
|---|---|
| The cache path can render the map | It renders only the sim ground/blocks; geometry data is hash-identical, so the defect is downstream, never found. Abandoned in favour of decode. |
| A whole-city 4K still is reachable by tuning | Fails with `Validation Error`/GPU exhaustion at 1.08 M instances + 3 GB pool after 20.6 min. The engine draws fakes instead. |
| Loose entity matrices are row-major | Transposing them makes buildings vanish. |
| Groups compose as `local * base` | Scatters every building part. |
| `no selector` entities are missing buildings | Their links resolve to `Night`/`Dawn`/`Noon`/`Sunset` and `GFX_Fire_StreetLamp` — lighting/FX. |
| Part indices are part-local | They are absolute (`part_base=112, tri 112..116`). |
| Streaming nondeterminism was timing | It was `HashSet` iteration order (`target`), fixed with `BTreeSet`; 27 % → 0.13 %. |
| Fragmentation was bad geometry | Mostly a half-filled ring: the capture gate fired between load batches. Wait for `pending == 0`. |
| `--map-centres-only` works on the whole-map path | It was hardcoded false in `MapRuntime`; 33 min → 15 s after the fix. |

---

## 6. The RE programme (exhaustive, in dependency order)

Each item: **target → method → deliverable → oracle**.

### R1. Serialisation framework (the master key)
- **Target**: the descriptor table at `0x4e9c800` and `sub_270FF90`.
- **Method**: decompile the lookup path, decode the record layout (`hash, flags, size, alignment,
  handler`), then enumerate every entry. Handlers have no static xrefs.
- **Deliverable**: a full `type id → {size, alignment, reader}` table, exported to
  `docs/findings/ACU-types.csv`, plus a decoder skeleton that reads any payload generically.
- **Oracle**: decoding 1,000 random resources by table must consume exactly their `record_length`
  (index has it), with zero over/under-runs.

### R2. Archive/forge layer (mostly done)
- **Target**: `acu_assets` forge reader + `tools/*.py` index builder.
- **Deliverable**: documented forge entry/resource layout; the index rebuildable from scratch.
- **Oracle**: every indexed resource's `stored_sha256` round-trips through the reader.

### R3. World / streaming manager
- **Target**: whatever schedules cell loading in ACU. Start from the frame task graph (§2.2) and
  from `GraphicWorld::Entity::Components` (`0x30941d0`/`0x3094200`), `BatchCullingSettings`,
  `Graphic::VisualsCulling*` (`0x3096d20`…`0x3096f00`).
- **Deliverable**: the cell set rule (box? radius? parents?), the hold condition, the priority
  function, and the unload rule, with addresses.
- **Oracle**: the live game's loaded-cell set vs the rule, at three player positions.

### R4. Cell contents and components
- **Target**: `ac2bbf68` → children; `0984415e` → components by marker.
- **Deliverable**: complete component list with sizes and semantics (what our loader currently
  skips: lights, nav, FX, GI blockers, fake entities …).
- **Oracle**: for a sample cell, every resource in the package is accounted for by a component or
  an explicit "not geometry".

### R5. LOD and culling
- **Target**: `51dc6b80` records; `sub_43360`; per-size cull distances; HiZ
  (`GfxHierarchicalVisibility::VisibilityDepthSurface`, `0x30a8da0`).
- **Deliverable**: the exact level choice and cull distances; the occlusion algorithm.
- **Oracle**: a frame rendered with our culling has the same silhouette as one rendered with none.

### R6. Mesh decode
- **Target**: `415d9568`, `sub_1E176C0` (three 8-byte element streams at +64/+76/+88).
- **Deliverable**: every vertex format and index format, quantization, submesh/material binding.
- **Oracle**: decoded positions reproduce the stored extents (already true for formats 0/1/2/4/5/6).

### R7. Materials and textures
- **Target**: `85c817c3`, `d70e6670`, the texture-set graph, `Env_MultiBlender`-style layered
  templates, per-placement vertex colours (verify whether ACU's compiled instances carry them).
- **Deliverable**: the full material template graph and the blend model.
- **Oracle**: a street's ground matches a screenshot of the game at the same pose (see R11).

### R8. Collision
- **Target**: `b22b3e61` and Havok shapes.
- **Deliverable**: shape parsing + a physics world with the game's window radius.
- **Oracle**: the scripted walk finishes 100 m with no fall-through and no wall penetration.

### R9. Fakes / impostors
- **Target**: ACU's equivalent of `FakeEntities` (search the World payloads and the Paris archive
  for merged low-detail meshes with per-cell index spans).
- **Deliverable**: horizon coverage.
- **Oracle**: with a 64 m ring, the frame has geometry beyond it and no empty band.

### R10. Animation/skeleton (Arno)
- Separate track: skeleton, clips, IK, cloth. The AC1 reference has a working model to copy.

### R11. Ground truth capture
- **Target**: the running game, same pose as ours.
- **Method**: with a signed-in session, position Arno, hide HUD, screenshot; store pose + image.
- **Deliverable**: a pose→image corpus for D1.
- **Oracle**: our frame vs the game's frame, geometry-mask IoU and per-pixel delta.

---

## 7. Harness and oracles

| Check | Command | PASS |
|---|---|---|
| Build | `cargo build --locked --release -p acu_bevy` | exit 0, no new warnings |
| Determinism | same capture twice | `capture-state` checksum equal, PNGs ≤ 0.2 % apart |
| Single cell | `--map-cell 4884 --map-instanced --capture` | silhouette IoU ≥ 0.99 vs `local/tex-inst.png` |
| Ring complete | capture log | `stream: textures resident=…/…` and `pending == 0` before save |
| Hold | walk script | position stops while `pending_near > 0`; never over empty space |
| Textures | log | `parts [bound, textured]` ≥ 99 % textured |
| Decode | census | `0 unsupported vertex format`, `0 not decoded` |

Image stats: `py` with Pillow (mean/stddev/distinct/modal, IoU); never `py -3` (the launcher
resolves `py`).

---

## 8. Traps already paid for

1. **`HashSet`/`HashMap` iteration order is randomised per process.** It decides batch order,
   so identical runs loaded different cells (27 % frame difference). Use `BTreeSet`/sorted `Vec`
   wherever order affects output.
2. **The capture gate must wait for `pending == 0`, not for the count to stop changing** — batches
   make the count plateau mid-fill (64 of 186 cells photographed as "fragmentation").
3. **Windows locks the exe** while a run is alive: stop by PID before rebuilding.
4. **`Start-Process` redirects produce UTF-16 logs** — read them as such.
5. **The IDB has imagebase 0**; live addresses are `module_base + RVA`. Some singletons point into
   the heap, which the dump does not contain — those need a live probe.
6. **Anvil classes carry no RTTI**; only Havok names survive. Strings are the map, not RTTI.
7. **Type ids are stored as hex text** in `asset_index.sqlite` (`'ac2bbf68'`, not the integer).
8. **`rust-toolchain.toml` pins a channel string rustup rejects**; build with
   `RUSTUP_TOOLCHAIN=stable`.
9. **A stale `--capture` path writes a valid-looking PNG of the wrong state** — always check the
   `capture-state` line.
10. **Do not infer a class's meaning from its name alone** — `sub_2A87740` ("Cannot convert vertex
    format from") is Havok, not Anvil.

---

## 9. Current state of the rewrite (as of this hand-off)

Working, verified, pushed on `acu-137-whole-map`:

- instanced renderer (one entity per piece/part/cluster, instance matrix buffer, explicit AABBs);
- cluster frustum culling (633 of 3,568 clusters rejected in a street view; cell 4884 IoU 1.00000);
- texture residency queue mirroring the ACU manager (100 MiB, slice, cap, async-style uploads);
- lighting from `GlobalAmbientLight` + both `DirectionalLight`s (street RGB within ~7 % of the
  `StandardMaterial` path);
- vertex format 2 decoded; per-placement LOD by selector distance; EntityGroup placement rule;
  terrain containers expanded before the radius test; assets archive in the streaming catalog
  (untextured 71 → 8);
- deterministic streamed ring (BTreeSet); capture gate waits for completeness;
- **hold**: the player stops while cells within 40 m are loading (verified: 152 m of walking into
  empty sky became a 42 m walk that stops on loaded ground).

Known open defects: fake/distance coverage (M4), AABB-only collision (M5), single-diffuse materials
instead of layered ground (M6), per-size cull distances and LOD budgets (M7).

Plan of record: `docs/status/139-plan-paris-1to1.md` (M1–M7 with acceptance tests).
