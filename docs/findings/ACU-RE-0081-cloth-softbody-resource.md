# ACU-RE-0081 — Coat cloth: the `Cloth` soft-body resource and its native library

Date: 2026-11-15. Claude Code session. User request: "Port the cloth physics on the coat" (Arno's
Tailored outfit, [ACU-RE-0070](ACU-RE-0070-arnos-tailored-visual-pieces.md)).

State: **partially established.**
- Observed: the resource layout prefix, the record framing, three record meanings, and the class
  and field names.
- Observed live: the cloth solver's functions and their per-frame order (section "Where the
  solver is"). They are in the corpus.
- Ported: the solver program and every kernel the flap schedule uses, particle skinning, the
  render-vertex mapping, and swept collision against the flap's leg capsules. The viewer drives
  the Tailored flap with them (sections "Viewer" and "Collision").
- Compared live (2026-11-16, sections "Live comparison" and "Toward parity"): one replayed
  step lands within 0.035 mm (mean) of the game's, idle, walking, jogging and turning; a 450-step
  free-run drifts 0.1–2 mm (mean) and returns to the game's rest state. The live settings come from the
  external `SoftBodySettings` resource, now decoded and used by the viewer (section "External
  `SoftBodySettings`").

## Provenance

- Resource `Softbody_CN_P_Flap_ArnaudV2` `0000001460e45330`, type `e33044ba`:
  - Asset index label `Cloth` (`crc-name`). CRC-32("Cloth") = `e33044ba`, confirmed by the
    reflection descriptor below.
  - `DataPC_ACU_LGS_BelleEpoque_assets.forge` entry 5882, resource 0 of 3.
  - Stored entry SHA-256 `03d8dd36b66fb1f2c1d6a719676cd67b52ae12c60e7aafc08a7ea42dbedf87e8`.
  - Payload 315,093 bytes, SHA-256
    `5fdcc55f709c1875f2b00b7194798bc75e118ae3a21c583701732d00502d4587`.
  - The same file ID appears in eight other archives. Load precedence is not established (#35).
- The flap mesh `CN_P_FR_Flap_Arnaud_V2_LOD0` (`00000013c1e4eb9a`, BelleEpoque entry 364) is
  referenced through its entry object `00000013c1e4eb52` (the GraphicObject pointer below).
- Binary: `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
  image base `0x140000000`. The disk image has packed code: it does not contain the class hashes
  or the code below. Evidence comes from:
  - the captured runtime image
    `Unity/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`
    (125,980,672 bytes, laid out by RVA);
  - the address-named corpus `ACU - Decompiled/` (pseudocode, not proof of instruction identity).
- Reflection: `tools/acu_reflection.py --image <mapped> --rdata 0x144e00000 0x144f00000 …`.
  Output is kept in ignored `local/tailored/softbody-reflection.json` and
  `local/tailored/motioncloth-reflection.json`. Class names come from CRC-32 matches against the
  `.rdata` string pool near `0x143027180`.
- No third-party material. The For Honor rewrite has no cloth or soft-body decoder (searched).

## Result

### The skinned coat is two systems

The flap mesh is skinned to 17 joints. All of them resolve in the existing skeleton library.
Joints 7–16 are "dress" bones from `ACU_FR_Avatar_Dress` and related skeletons, driven by
Reflex, HingeVector and OrientationConstraint bone modifiers. These are the leg-follow modifiers
gated in [ACU-RE-0028](ACU-RE-0028-model-pose-bone-modifiers.md). They are not cloth.

The Tailored skeletons have no coat-chain (BallJoint) bones on the flap. The cloth motion is the
separate `Cloth` resource. UpperBodies table row 2, columns 6/7 (ACU-RE-0016) reference the
`Softbody_CN_P_Flap_ArnaudV2` and `Softbody_CN_P_Scarf_ArnaudV2` resources.

### Class family (observed, reflection)

```
Cloth (e33044ba) : SoftBody (4b54c698)
  SoftBody.99ae01fa : [SoftBodyState]          -> MotionClothState (6119d4ce) : ClothState : SoftBodyState
  SoftBodyState.GraphicObject, .04567b12 : [SoftBodyLOD], .Ragdoll : LiteRagdoll
  SoftBodyLOD (823777d3, 304 bytes) -> ClothLOD (fa94ba7d) -> MotionClothLOD (295583ef)
    Settings : SoftBodySettings (f7a7e4df, 144 bytes; "Damping" named, 44 others unresolved)
    six u8 arrays e3135e5c..69c9a576, blob 64f250cd, u16/float arrays,
    SoftBodyVertexMapping (62ba80e1: ?, u16, Offset float, u16[], ?[]), DuplicateVertex, Mesh
```

The string pool also names SoftBodyCollisionShape, SoftBodyAnchor, LiteRagdoll* and
SoftBodySettingsOverride*.

### Serialized prefix (observed bytes; decoded to the first blob's end)

Each pointer is shown as its tag, kind byte and file ID.

| Offset | Bytes | Meaning |
|---|---|---|
| 0 | `01`, id, `e33044ba` | root `Cloth` |
| 13 | u32 1, `04 00`, id …331, `6119d4ce` | one owned `MotionClothState` |
| 31 | `01 04` + `00000013c1e4eb52` | GraphicObject pointer. Tag 1, kind byte 4 (meaning unknown). |
| 41 | u32 4 | four `MotionClothLOD` elements: …332 and …333 are full, …334 and …335 have a null Settings (`03`) |
| 45 | `00`, id …332, `295583ef` | LOD0 |
| 58 | `01 00` + `00000016c61d53a1` | Settings pointer |
| 68 | 6 × (u32 587 + 587 bytes) | per-particle u8 arrays (meanings unknown) |
| 3614 | u32 42195 + bytes | simulation blob, field `64f250cd` |
| 45813 | … | remaining LOD0 fields; not decoded |

The fields after the blob were sized from their counts but not decoded:
- u16 ×1039;
- float3 ×587 (equal to the blob's particle xyz);
- float3 ×587 (unit vectors);
- u16 ×3117;
- 1192 `SoftBodyVertexMapping` elements, each starting with u64 0 and class `62ba80e1`;
- …

1192 is the flap mesh's vertex count.

### Simulation blob (observed)

- Layout: `u32 version (1), u32 body length, records`.
- Record framing: `u32 tag, u32 total length` (≥ 8, including the header), then data. Natively,
  `0x1423aee50` (corpus `1423aee50.c`):
  - rejects `(tag & 0xffff0000) != 0xecd70000`, lengths below 8 and records past the end;
  - finds the next record whose low 16 bits equal the requested tag, within a bounded count.
- `0x1423aef00` is the string variant: it requires exactly one terminating NUL.
- The LOD0 blob holds 47 records; they decode to the exact end.
- 69 native loader functions call these two readers. Each loads one record group
  (map: ignored `local/tailored/cloth_tag_owners.json`).

Records whose meaning is established from the loader code:

| Tag | Reader | Meaning | Flap value |
|---|---|---|---|
| `02.01` | string reader `0x1423aef00` | settings name | `ClothSettings_Standard_Avatars` |
| `11.01` | `0x1423cda50` | body class: 0 → 736-byte object (`0x1423f03d0`); 1 → 824-byte object (`0x1423f0bc0`); then load via vtable +56 | 1 |
| `11.02` | `0x1423b0860` | particle count (`+12`) | 587 |
| `11.0b` | `0x1423b0860` | float4 per particle, for count rounded up to 16 (`+16`) | 592 × 16 bytes; xyz in flap mesh space (median 1.5 mm from the nearest render vertex); w ∈ {0, −1} (meaning unknown) |

Other records are sized but have unknown meanings:
- `11.0d`, `11.24`, `11.13`: u16 lists, 6,304, 13,328 and 6,234 bytes;
- `11.2a`, `11.b3`: 1192-entry records;
- `11.23` and `11.11`: three counts each.

The `11.0b` loader also allocates sub-objects according to flag bytes in the `11.04` record
(`+108` … `+129`), which `0x1423cdc40` loads.

### Where the solver is (observed live, 2026-11-15)

**Correction:** an earlier version of this finding said the solver is absent from the corpus. That
was a listing error: the corpus names files in upper-case hex, and a lower-case filter missed them.
The range holds 1,792 corpus functions, including every function below.

**Body object (static).** Kind 1 is built by `0x1423f0bc0` with vtable `0x1435f4de0`, which has
10 slots:

| Slot | Function | Role |
|---|---|---|
| 0 | `0x1423f0ce0` | destructor |
| 5 | `0x1423f1190` | pushes particles to the `+488` object |
| 6 | `0x1423f0920` | forwards to `+488` slot 5 |
| 7 | `0x1423f0f80` | loader (records `11.12`/`11.13`/`11.20`/`11.21` after `0x1423b0860`) |
| 8 | `0x1423f0d20` | clone |
| 9 | `0x1423f0ed0` | size |

The kind-0 table `0x1435f4d90` sits directly before it.
- `+480` and `+488` are 288-byte spatial-query objects (vtables `0x1435f56c0`/`0x1435f5710`; box
  and ray traversal).
- Live: body fields are `+8` kind (1), `+12` particle count, `+16` the positions pointer, `+24`
  capacity (count rounded up to 16).

**Live run.**
- Game: PID 128144, `capture_run.py --save-pin capture-start-1 --until world --fps-cap 60`. Arno
  in a long-coated outfit (`local/native-probe/runs/cloth-live-1/screenshots/arno-now.png`).
- Instrumentation: `acu_probe.dll` SHA-256 `d7b69cca…3781f`, and `acu_tap13.dll` SHA-256
  `1d6ccbc0…39d2`, attached with `acu_probe_launcher --attach`.

1. **The vtable is not used per frame.** Hardware execution hooks were set on slots 5, 6 and 7,
   and on the container class's methods (`0x1423be7b0`/`be800`/`be850`/`be8a0`, `0x1423f1300`/
   `f1350`). None recorded a hit in 3 seconds. A control hook on the coat apply at `0x140045240`
   (ACU-RE-0028) recorded 4,270 hits in 2 seconds.
2. **Bodies exist and move.**
   - A read-only scan of 3.6 GiB of private RW memory found 708 qwords equal to the vtable
     (`body-scan.json`, SHA-256 `8f5624ea…42bb`).
   - 615 of them are consistent bodies: kind 1, with capacity equal to the count rounded up to 16.
     Particle counts include 144, 45, 84, 15, 69, 70, …
   - 32 of those had particle positions that changed between two reads 50 ms apart.
3. **Write trace.**
   - Method: `WWATCH` page guard on the 144-particle buffer `0x1380ad880` (2,304 bytes) of body
     `0x1380ad4f0`, for 0.6 s.
   - Result: 101,897 write hits (`wtrace-1380ad880.bin`, 376-byte WHit records, SHA-256
     `b706c5ee…356f`) from six worker threads.
   - Writer RIPs map to these corpus functions:
     - `0x1423e51f0` (62,985 hits, 2,746 bytes);
     - `0x142419cc0` (10,979);
     - `0x1423d7f20` (6,120);
     - `0x1423d8f20` (6,003);
     - `0x1423da5b0` (6,000);
     - `0x1423bd180` (3,570);
     - `0x1423b10d0` (3,120);
     - `0x1423d7720` (3,120).
4. **Per-frame order of writers (observed, by QPC).**
   ```
   b10d0, d7720, { (e51f0, bd180) x3, 419cc0, d7f20, d8f20, da5b0 } x2, then b10d0 again
   ```
   Calls nested beneath a writer can appear as separate writers.
   - Stack copies place `0x1423b10d0` under `0x1423af520` ← `0x1423c2c00` ← `0x1423aa040`.
     `0x1423af520` sets the instance's world matrix (`+64…+112`) and passes the old-to-new delta
     to `b10d0`.
   - `0x1423d7f20`/`0x1423d8f20` run under the collision pass `0x1423d68d0`. That pass iterates
     160-byte shape records of types 1–5 with per-shape matrices.
   - `0x1423bd180` runs under `0x1423e51f0`.
   - The top-level driver is deeper than the 40-qword stack copy.

**Inference (to verify by porting):**
- `b10d0` carries particles with the moving frame.
- The `{…} x2` block is two substeps per frame (60 fps cap). Each substep runs three iterations of
  a constraint sweep (`e51f0`, with `bd180`), then the other writers. `d7f20`/`d8f20` are
  collision.

Static support for "constraints in batches": record `11.0c` holds six counts (71, 62, 76, 37, 37,
37). `11.1d` holds 592. `11.0d` holds 16 × (sum + 2·592/16) = 394 sixteen-byte blocks, exactly its
6,304 bytes. This suggests six independently solvable batches packed four-wide (an inference).

The game was closed with `capture_run.py --close` (graceful, 2.9 s), and the saves were put back.

### The solver program (observed, static; agents' specs checked against the trace)

The step entry `0x1423d4b60(solver, body, dt, wind vec3, shapes)` calls solver vtable
(`0x1435f44e0`) slot 1, stores dt and the vec3, runs the body's schedule (records `11.09` count,
`11.0a` opcode/count pairs) through the interpreter `0x1423d47e0`, then calls slots 12 and 2.

| Op | Native | Port (`acu_sim::cloth`) |
|---|---|---|
| 0, 1, 2 | slot 3 → `0x1423e5180` → `0x1423e51f0`, distance sets 0–2 | `solve_distance` |
| 3 | slot 3 → `0x1423d8e80` → `0x1423d7f20` (backstop, settings+96) | `solve_backstop` |
| 4 | slot 3 → `0x1423d8e80` → `0x1423d8f20` (max distance, settings+128) | `solve_max_distance` |
| 13 | slot 3 → `0x1423da5b0` (tether, set 5) | `solve_tether` |
| 5 | slot 3 → `0x1423d8990` (blend to targets, set 6) | `blend_to_targets` |
| 6 | slot 4 → `0x1423d7720` (Verlet integrate; `0x14241dfb0` when body+124) | `integrate` |
| 7 | `0x1423d4700` → `0x1423d8e50` → `0x1423d5510` → `0x1423d68d0` / swept `0x1423d58a0` | **not ported** (counted, skipped) |
| 8 | slot 6 → `0x1423e4cf0` (normals) | `compute_normals` |
| 9 | slot 8 → `0x1423d72b0` (drag/wind) | `apply_drag` |
| 10 n / 11 | repeat block; n LOD-reduced by `0x14240c090` | interpreter (full-LOD count) |
| 12 | slot 7 per sub-object → `0x1423e2130` | rejected (flap does not use it) |

The flap schedule is `6, 9, 5, 10(4){2, 1, 0, 7, 3, 4, 13}11, 8`. This corrects the inference
above: `0x1423d7f20`/`0x1423d8f20` are the backstop and max-distance constraints (ops 3/4), and
the collision writer is `0x142419cc0` (op 7). The trace's `{…} x2` is consistent with a repeat
count of 4 reduced to 2 by the LOD rule; the live substep mode is still unconfirmed.

Kernel arithmetic (observed in the pseudocode and disassembly):
- Integrate: `d = keep·(p − prev) + keep·g·dt²` (or `keep·(g·dt²·gs[i] + (p − prev))` with a
  per-particle gravity map); the step is scaled by `min(1, rsqrt(max(|d|², 1e-14))·max_speed)`;
  pinned particles (w = −1) snap to their target; `prev` takes the old position.
- Distance sets: SIMD groups of four solved Jacobi-style from a snapshot, with
  `k = clamp((1/r − 1)·stiffness·r)` where `r = rsqrt(L²)·rest`, using SSE min/max NaN rules; a
  scalar tail uses `len = max(sqrt(L²), 1e-10)`. Each free end takes half; a free end paired with a
  pinned one takes all of it.
- Backstop `v = d − dot(P − T, N)`, push along N when `v ≥ 0`; max distance pulls P toward T by
  the excess over the radius; tether pulls toward an anchor past its length (`1e9` when disabled).
- Drag: `c = (a·0.5·dt)/b`, `P += N·dot(wind·dt − (P − prev), N)·c`, free particles only.
- Normals: unit face normals summed per particle, a zero sum becomes +X, then normalised.
- The native code uses `rsqrtps`/`rcpps` estimates. The port uses exact maths by default; the
  `acu_sim` feature `hw-rsqrt` uses this CPU's estimate instead (validation only).

## Implementation

`acu_assets::cloth` provides:
- `blob()`: record walk with the native framing checks; requires the version and exact end.
- `Blob::{find, settings_name, body_kind, particle_count, particles}`.
- `Blob::{distance_sets, particle_constraints, particle_maps, flags, settings, triangles,
  schedule}`: every input the flap's program reads (records `11.04`, `11.05`, `11.08`, `11.0c`–
  `11.0e`, `11.11`, `11.12`/`11.13`, `11.1a`, `11.1d`, `11.23`/`11.24`, `11.2d`, `11.2e`,
  `11.b1`–`11.b4`). Legacy drag record `11.06` is rejected.
- `parse()`: the prefix above, up to LOD0's blob. It rejects unknown pointer tags, class mismatches,
  count disagreements and nonfinite positions.

`acu_sim::cloth` (no Bevy) provides `State`, the kernels above, the frame `carry`, and `Cloth`,
which validates a program and runs it with `step(dt, target_normals)`.

Tests:
- `acu_assets`: 3 unit tests on synthetic blobs (framing, magic, length, version, padding).
- `acu_assets/tests/cloth.rs`, `#[ignore]`: the installed flap. It checks the hash, IDs, counts,
  the three distance sets' sizes and clamps, rest lengths against the particles, 180 pinned
  anchors, flags, settings (gravity (0, 0, −10), damping 0, max speed 1000, drag 1.3/0.7), 1,039
  triangles and the schedule.
- `acu_sim`: 22 unit tests with independent expected values (7 for swept collision: resting
  push-out, no tunnelling through a capsule, a moving sphere pushing a particle, box snap,
  pinned and distant particles, friction, and pose interpolation), and before that 15 (free fall, pinned snap, speed cap and
  damping, stretched pair, one-sided clamps, SIMD vs scalar agreement, backstop, max distance,
  tether, normals, drag, carry, the repeat program and opcode rejection, particle skinning, the
  render mapping and surface frames).
- The installed-flap test also checks the post-blob fields: mesh id, 1,192 mappings (234
  skinned), 203 duplicates, 8/8 influences, 18 bones, weight sums and the end offset.

### Particle skinning and the render mapping (observed, static)

The LOD0 fields after the blob decode to the payload's byte 182,018, in reflection order
(serializer `0x141d83740`; `acu_assets::cloth::lod0_mesh`):
- u16 ×1039 (`77b91c50`, meaning unknown);
- rest particle positions and normals (float3 ×587; the positions equal `11.0b` exactly);
- particle triangles (identical to `11.13`);
- 1,192 `SoftBodyVertexMapping` elements (37 bytes: inline header, u8 skinned flag, u16 triangle,
  f32 `Offset`, u16[3] particles equal to the triangle's, f32[3] `(h, u, v)`);
- 203 `DuplicateVertex` pairs, u16 ×1192 canonical map, u16 ×5385 index buffer, float2 ×1192 UVs;
- three empty LOD-transfer arrays, the Mesh id `00000013c1e4eb9a` (tag byte 0);
- u16 LOD index 0, u16 particle influences 8, u16 render influences 8, two bytes (0, 1);
- particle bones u16 ×4696 and weights f32 ×4696;
- 234 skinned render rest positions with 8 bones/weights each;
- 18 `MeshBone` elements (81 bytes each, the mesh layout).

Native use:
- **Targets.** `0x141d7a4a0` skins each rest particle and normal with up to 8 bone matrices,
  skipping bone 0xffff or weight ≤ `0x3727c5ac` (about 1e-5), and divides by the kept weight sum.
  `0x141d97aa0` copies the results to sim+504/+520, which `0x1423aa760` binds as body+64/+80.
- **Render vertices.** `0x141d70840`: `X = Q0 + u(Q1 − Q0) + h·TN + v(Q2 − Q0)` with
  `TN = nz(cross(nz(Q1 − Q0), nz(Q2 − Q1)))`; flagged vertices are bone-skinned without dividing
  by the weight sum.
- **Frames.** `0x141d98430`: area-weighted normals and UV tangents over the canonical index
  buffer, copied to duplicates, Gram-Schmidt with degenerate fallbacks; the bitangent is stored
  negated.

### Viewer (implementation, 2026-11-15)

`acu_bevy::cloth` binds a `Cloth` resource from `options::ARNO_CLOTHS` to the loaded piece whose
mesh id its LOD0 names. It looks in the piece's archive and that archive's `_assets` sibling;
this is a lookup choice, not load precedence. Load checks, all observed on the installed flap:
- the 18 cloth bones resolve to skin joints, and their matrices match the mesh's inverse binds
  to 3.6e-7;
- mapping the rest particles reproduces all 1,192 mesh vertices, in order, within 0.075 mm;
- the native frame rule gives the mesh's tangent handedness on 1,186 of 1,192 vertices.

Per 60 Hz tick:
1. skin the targets with the pose;
2. carry free particles by the character's translation change;
3. one solver step;
4. rebuild positions, normals and tangents.

The simulation frame uses native world axes (gravity `(0, 0, −10)`) relative to the character's
position. In capture mode the replay's last 240 ticks are stepped before the still frame.
`--no-cloth` restores the skinned flap.

Implementation choices (not native evidence):
- one solver step per tick, with the full repeat count of 4;
- no ambient wind (the authored drag wind vector is zero);
- scale 1;
- unported branches (body +118, +125, 4-wide tether) refuse to load rather than approximate.

Fresh captures (`local/cloth/`, ignored): `idle-cloth.png`, `run-cloth.png` and `run-skinned.png`
(the same frame, skinned), plus `run-cloth-side.png` and `walk-cloth-side.png`. The coat hangs at
idle and trails behind the legs in the run.

### Collision (opcode 7; static analysis 2026-11-15, ported)

**Settings (observed).** Record `11.05` (loader `0x1423ce370`) serializes S+7 at byte 0, S+192
at 43, friction S+196 at 44, S+8 at 48, S+9 at 49 and the priority S+195 at 50. Flap values:
- S+7 = 1 (rigid colliders), S+192 = 1 (swept path), S+8 = 1, S+9 = 0;
- friction 0, thickness S+48 = 0, priority 128.

`0x1423d4700` therefore builds mask 0x15, and `0x1423d5510` routes it to the swept path
`0x1423d58a0`, not `0x1423d68d0`. Record `11.b1` sets body+126 = 1 (per-particle thickness
table, map slot 2) and body+127 = 0 (no per-particle friction table).

**The flap's colliders (observed).** The cloth payload ends with `SoftBodyState.Ragdoll`, a tag-1
pointer to `00000027d187b440`. That is `LiteRagdoll_SkirtHV_Leg_ArnoBrigand` (type `891043d5`,
resource 2 of the same BelleEpoque_assets entry 5882; payload 973 bytes, SHA-256
`f4fa2bd7720e516ecaae23cda9742bd462c44e32caf03ee653691f90b8536179`).
- Reflection: `LiteRagdoll` (size 40) holds `Capsules` (`LiteRagdollShape`, `22225754`, size 112,
  deserializer `0x141d82960`) and `ExternalCapsules` (empty here). The names are `crc-name` guesses
  matched against the reflection hashes.
- Each shape serializes, in stream order, `ShapeType`, `BoneID`, `HalfExtent` (float4), `Height`,
  `Radius`, `LocalMatrix` (rows X, Y, Z, T) and `GroupFlags` (an inline `13e073f6` object, 16
  bytes, which the collision path does not read).
- The flap has seven capsules (`ShapeType` 0). Their bones are `3952b745`, `961763c6`, `88565c0b`,
  `f1602d3a`, `ded10611` (`Hips`), `9b14362c` (`RightFoot`) and `58988870` (`LeftFoot`); the three
  names are CRC-32 matches.

**Runtime path (observed, except where marked).**
- Each shape becomes a small rigid body that joins the cloth body's explicit collider list at
  body+408 (`0x1423aa600`). It then follows `0x1423b24f0` → `0x1423b26e0` → `0x1423d26f0`.
- World frame = `LocalMatrix × bone` (row vectors, `0x141d98be0`).
- The capsule's shape-local frame (`0x141d7aaf0`) maps rows to (−Y, X, Z), so the record's axis
  (local Y) is the element's X axis.
- `Radius` is the record radius and `Height` its half-length, with no halving.
- Unresolved: whether these bodies are static (previous frame = current) or dynamic (rewound by
  velocity). They are created by `0x1423acb60`, which jumps into protected code.

**Swept path `0x1423d58a0` (observed).** For each record, in order:
1. Query the particle BVH (record `11.1e`) with the union of the shape's world box at the previous
   and current frames.
2. For each candidate, sample the particle's motion in shape-local space:
   - start `L(M0, prev)`, end `L(M1, P)`;
   - `0x1424389b0` bisects a 1/32 step grid while samples are farther apart than the spacing
     (capsule and sphere r/2, box 0.8·min(e)), giving 2–33 samples;
   - intermediate poses interpolate rotation by axis-angle and translation linearly
     (`0x1424382b0`, `0x1424386b0`).
3. Push each sample out of the shape, carrying the push forward:
   - capsule: nearest point on the segment;
   - sphere: from the origin;
   - box: one axis snapped to 1e-4 inside.

   Friction uses `min(L, μ·pen)`; capsules also resolve the start sample.
4. Write back with the current frame, free particles only (w = +0). `prev` is never written.

Arithmetic is exact `sqrtps`/`divps`, with thresholds 2^-23 and 1e-8.

**Port (`acu_sim::cloth::collision`, `acu_bevy::cloth`).**
- The kernel follows the steps above.
- Per tick the viewer rebuilds the seven capsule frames from the pose. The previous frame is last
  tick's, shifted by the character's translation change; this is an implementation choice
  pending the rigid-body type.
- Bind-pose placement, with capsule centres in mesh space:
  - thighs at about 0.70 m, shins at 0.28 m;
  - the pelvis capsule across the hips at 0.96 m;
  - feet near the ground, pointing forward;
  - 4 and 2 rest particles start inside the two thigh capsules.
- Port differences:
  - every particle's sweep box is tested instead of the native BVH. Rounding can differ, and the
    native tail-leaf box bug (three flap particles) is not reproduced;
  - Rust's `sin`/`cos`/`acos` replace the MSVC CRT's, with `acos` clamped;
  - mesh (type 4) and cloth (type 5) colliders are not ported, and the flap has none.

Captures (`local/cloth/`, ignored): `idle-coll-160.png`, `walk-coll-90.png`, `run-coll-90.png`
and `run-coll-160.png`. With collision the coat still hangs at idle. In the run, the raised knee
lifts the flap and the back panels part around the legs.

**Live comparison:** see the next section.

### Live comparison (2026-11-16)

**Run.**
- Game: PID 131236, `capture_run.py --save-pin capture-start-1 --until world --fps-cap 60`, exe
  SHA-256 `748abdc6…7e40`, image base `0x140000000`. Closed with `capture_run.py --close`
  (graceful, 1.9 s); saves put back.
- Instrumentation: `acu_tap13.dll` rebuilt from the current `tools/native_tap/tap.cpp` (SHA-256
  `96f6ff56…c18e`), attached with `acu_probe_launcher --attach`; focus shim `acu_focus.dll`
  `1277cf80…54de`.
- Outfit: `tools/native_probe/outfit.py` (focus modes 6 and 7, after the equip/refresh routines of
  [ACU-RE-0048](ACU-RE-0048-outfit-equip.md) and the visual-selection lead from the ACUFixes
  "Gear Visual Customization 365" 1.2.5 plugin, read statically; none of its code is used). Both
  modes returned success with all five `_02` items found (types 3, 1, 2, 0, 4); the coat did not
  visibly change. Inference: this save already wears the flap's outfit. A 587-particle body
  stepped before any change was visible, and the 11-15 default-save census also had 587-particle
  bodies. Arno's coat in the game has the same shape as the viewer's Tailored render; only the dye
  differs (`local/native-probe/runs/cloth-live-2/screenshots/`).

**Census** (`tools/native_tap/cloth_census.py`, hardware hook on the step entry `0x1423d4b60`,
guard `48895c240848896c2410488974241857`, 0.5 s): 19 kind-1 bodies, each stepped once per
rendered frame (gaps 16.4–17.2 ms at the 60 fps cap), all with dt = 1/30. The flap is body
`0x173c50b70` (587 particles, 180 pinned).

**Observed body layout** (`+0x10` positions, `+0x20` previous, `+0x30` particle normals, which
the drag kernel `0x1423d72b0` reads, `+0x40` targets, `+0x50` target normals). Particles,
targets and collider records are in a character-local, z-up frame (flap z 0.12–1.12 m), not
world space.

**Live settings differ from the blob.** The body's settings block S = body+132 holds:

| Field | Body offset | Live | Blob (`11.05`/`112d`) |
|---|---|---|---|
| gravity | +164 | (0, 0, −20) | (0, 0, −10) after halving |
| thickness S+48 | +180 | 0.05 | 0 |
| S+60 | +192 | 1.1 | 1.0 |
| friction S+196 | +328 | 0.2 | 0 |
| drag a, b (read by `0x1423d72b0`) | +252, +256 | 30, 15 | 1.3, 0.7 |

Damping (0), max speed (1000) and blend (0) agree. Inference: the values come from the external
settings resource the LOD0 settings pointer names, `ClothSettings_Standard_Avatars`
`00000016c61d53a1` (type `f7a7e4df`, label confidence `unknown`, in BelleEpoque), which is not
decoded yet. Per-particle tables (body+384 block): map 0 holds 592 bytes of value 254, and the
thickness map at +80 has 144 distinct values (`body-tables-131236.json`, SHA-256
`4d8dbde8…43c4`).

**Collider records** (step argument 5 = `{records, count}`, count 7, 160 bytes each): type 3
(capsule) at +8, radius +144 and half-length +148 equal the LiteRagdoll's, current frame at
+16, previous frame at +80. At the next step, `+80`'s rotation rows equal this step's `+16`
exactly, and its translation is moved by the character's motion (about 2.3 cm per step while
walking). **This resolves the previous frame:** last step's frame carried into the new local
space, which is the rule the viewer already used.

**Captures** (`tools/native_tap/cloth_capture.py`, entry and exit snapshots filtered on the
flap body, `local/native-probe/cloth-live/`):
- `cloth-idle-131236.bin`: 60 steps standing (SHA-256 `70baa15b…9eae`);
- `cloth-move-131236.bin`: 450 steps; settle, walk, jog and stop through the in-process pad
  (SHA-256 `6cd6b57d…b65a`).

**Replay** (`acu_tap_bridge` `cloth_gate`): each captured step's inputs (positions, previous,
normals, targets, target normals, dt, wind, records) are loaded into the solver built from the
resource (`acu_sim::cloth::asset`, shared with the viewer), stepped once, and compared with the
game's exit. Free-particle position error:

| Variant | Idle mean / max | Moving mean / max |
|---|---|---|
| blob settings | 30 mm / 62 mm | 30 mm / 70 mm |
| live settings, tether × S+60, repeat 4, collision | **0.69 mm / 2.9 mm** | **0.81 mm / 9.7 mm** |
| same, repeat 2 | 4.0 mm / 10 mm | 4.2 mm / 26 mm |
| same, repeat 2, no collision | 4.7 mm / 19 mm | 4.8 mm / 73 mm |

The game itself moves the flap's free particles 0.11 mm per step at idle and 17 mm per step
(max 95 mm) in motion. Pinned particles and `prev` match exactly. Using the `rsqrtps` estimate
(`hw-rsqrt`) changes nothing measurable.

Findings from the replay:
- **Repeat count:** the live step runs the full 4 repeats. Two repeats are clearly worse, so the
  LOD rule does not reduce it here. The earlier trace's "x2" was a different body or LOD.
- **Tether stretch (inference):** at the game's exit states, particles sit at up to 1.1005×
  their decoded tether length (183,150 samples), never more. Scaling the tether lengths by S+60
  removes most of the error; where the native code applies it is not located.
- **Substep (observed):** one step per rendered frame with dt = 1/30 at a 60 fps cap. The
  viewer now uses this dt (section "External `SoftBodySettings`").
- The op-by-op trace (`--trace K`) shows the remaining gap still shrinking at the last repeat:
  the game converges slightly faster per step. The cause (another live override, or a kernel
  difference) is open.
- Particle normals mostly agree (mean 0.011), but a few are reversed (max distance 2.0). The
  cause is open.

### External `SoftBodySettings` (decoded 2026-11-16)

The LOD0 settings pointer names `ClothSettings_Standard_Avatars` `00000016c61d53a1`
(type `f7a7e4df`, asset-index label confidence `unknown`). The flap's entry 5882 carries it as
resource 1. All 629 indexed copies are byte-identical (141 bytes, SHA-256
`13efc992ab82da050e34ec64fc87f6a306f543347c77fb98f5bd8aedb9b42764`). All 109 variants of the type
(108 names) are 141 bytes.

**Layout (observed, with one inference).** Root byte 1, file id, type, then the 45 fields of the
reflected class `SoftBodySettings` (`local/tailored/softbody-reflection.json`, descriptor
`0x144e8cf90`, 144-byte runtime object), with sizes from their type codes:
- `0a` float (4 bytes), `00` and `03` byte, `07` int32, `0c` vec3 (12 bytes); `c0` is 1 byte
  (inference);
- order: fields 3 and 4, then the three `c0` fields 0–2, then fields 5–44. Fields 5–44 align
  exactly when walked back from the end. The position of fields 0–2 is an inference: they hold
  0, 0, 1 in every variant;
- check: on all 79 settings resources in BelleEpoque_assets, every bool is 0 or 1, every float
  is finite, and gravity is (0, 0, −20) or (0, 0, −9.8).

**Field meanings (each matched to the live body, PID 131236).** Names are CRC-32 hashes; only
`Damping` is named by reflection.

| Field | Hash | Flap value | Live body | Evidence |
|---|---|---|---|---|
| 39 (vec3) | `e08712ff` | (0, 0, −20) | gravity S+32 (+164) | unique match |
| 37 | `8bb303f6` `Damping` | 0 | S+44 (+176) | name |
| 34 | `cf5065b0` | 0.05 | thickness S+48 (+180) | unique match |
| 20 | `ee6c1935` | 1.1 | S+60 (+192) | unique match; bool 19 before it mirrors blob bytes 34/35 |
| 24 | `1664baeb` | 0.2 | friction S+196 (+328) | inference: fields 33 and 36 also hold 0.2; 23/24 mirror blob bytes 43/44 |
| 40, 41 | `d2bb63f9`, `6a9969e1` | 30, 15 | drag (+252, +256) | unique match |
| 16 | `d47dfb66` | 1.5 | body +160 | unique match; role unknown |
| 31 (u8) | `00d15395` | 128 | priority S+195 | same value as the blob |

The replay confirms the decode: the decoded gravity, damping, thickness, S+60, friction and
drag equal the captured body's exactly, and the replay with decoded settings reproduces the
live-body result (idle 0.69 mm, moving 0.81 mm mean).

**Port.**
- `acu_assets::cloth::soft_body_settings` decodes the resource (fixed length, root, id and type
  checks, bools 0/1, finite floats).
- `acu_sim::cloth::asset::{solver, collision_params}` take it as `external`. It overrides gravity
  (used as is, not halved), damping, drag, collision thickness and friction, and scales tether
  lengths by S+60 (inference).
- The viewer loads it from the cloth's package by the LOD's pointer, and reports it under
  `settings`.
- The viewer steps with dt = 1/30 once per 60 Hz tick (observed live). Fresh captures
  (`local/cloth/`, ignored): `idle-settings-160.png`, `walk-settings-160.png`,
  `run-settings-160.png`. The coat hangs at idle and trails the legs in the run.

### Toward parity: stiffness, the opcode gate and free-running (2026-11-16)

**Per-set distance stiffness (observed).** The distance dispatcher `0x1423e5180` passes
`*(solver+496) + 152 + 4·set` as the set's stiffness. On the live flap those bytes (body
+152/+156/+160) hold 1.0, 1.0 and **1.5**; the blob has 1.0 for all three. External settings
fields 10–18 are three (stiffness, k_min, k_max) triples, (1, −2, 2), (1, 0, 2) and (1.5, −2, 0):
the clamps equal the blob's and the stiffnesses equal the live body's
(`SoftBodySettings::distance`). With them, the idle per-step error fell from 0.69 mm to 0.04 mm
mean.

**The opcode gate (observed).** The interpreter `0x1423d47e0` runs an opcode only if
`0x14240bf70(ctx, op)` is true, where ctx = (body, S = body+132):

| Opcode | Enable |
|---|---|
| 0–4 | S[0]–S[4] |
| 13 (tether) | S[5] |
| 5 (blend) | S[6] |
| 7 (collision), 14 | S[7] ∨ S[8] ∨ S[9]; S[7] |
| 8 (normals) | S[10] |
| 9 (drag) | S[64] |
| 15, 19, 20 | S[194], S[128], S[11] |
| 18 | S[193] and LOD distance test |
| 6, 10, 11, 12 | always |

The blob loader `0x1423ce370` writes S[0]–S[6] from record `1107`, S[7]/S[10] from `1105`
bytes 0/1, S[8]/S[9] from bytes 48/49, and S[64] from the first byte of `112d` (loader
`0x1423ce820`).

**Live (PID 129560).** Hardware hooks on the drag kernel `0x1423d72b0` recorded 140 calls in
0.5 s for four bodies (45, 213, 213 and 15 particles; dt = 1/30), and none for the flap.
Solver vtable slot 8 is `0x1423d72b0` for every body, the flap included, and the flap's
runtime program contains opcode 9. Reading the S bytes of all 19 stepping bodies
(`bodies-129560.json`; some addresses were stale and are ignored):

| Bodies | Settings source (by their values) | S[6] | S[64] |
|---|---|---|---|
| 4 dragged bodies | blob (gravity −10 or −8 after halving; drag (1, 0.4), (1, 1), (50, 5)) | 0 | 1 |
| flap + 5 others | external `SoftBodySettings` (gravity −9.8 or −20, friction 0.2, drag (1, 10) or (30, 15)) | 0 | **0** |

The flap's blob has S[6] = 1 and S[64] = 1. **Port:** under external settings, blend (op 5)
and drag (op 9) are gated off, as observed; the external fields that carry them are not
identified. Blend is 0 on the flap, so only the drag change is visible. Disabling drag took the
moving per-step error from 0.25 mm to 0.035 mm mean.

**Carry (observed).** Each step's entry positions and previous equal the last exit moved by
the rigid transform that maps the collider records' current frames to the next step's
previous frames (zero error at every checked step). The transform has **no rotation, even
while turning** (largest 0.000° per step over a 450-step turning capture): the simulation
frame follows the character's position, not its heading. The viewer already carries this way.

**Each body is stepped by per-thread solver objects** (`0x4244630 + n·0x100000`, six seen),
so solver-keyed filters do not work; filter on the body (step argument 1 or `solver+8`).

**Results** (`acu_tap_bridge` `cloth_gate`, decoded settings, gate, no overrides):

| Capture | Per step, mean / max | Game motion per step | 450-step free-run, mean / max |
|---|---|---|---|
| idle (131236) | 0.031 / 0.81 mm | 0.11 mm | 0.08 / 4.6 mm |
| walk and jog (131236) | 0.035 / 7.2 mm | 17.4 mm | 0.89 / 43 mm |
| jog with turns (129560, `cloth-turn-129560.bin`) | 0.035 / 9.1 mm | 22.9 mm | 2.0 / 88 mm |

The free-run (`--free-run`) steps our own state from the first entry through every captured
step, carried by the collider-derived transform. Its drift grows while colliders sweep the
cloth and decays back to 0.08 mm once Arno stops: the coat reaches the game's rest state again.
Particle normals agree (mean 0.0003); one normal at a fold (step 248, particle 27) has the
opposite sign.

**Remaining residual.** The worst per-step particles sit on a capsule's surface in both
results, a few millimetres apart along it. The native capsule kernel `0x14240fb60` matches our
arithmetic (push, friction `min(L, μ·pen)` against the previous sample, radius
`factor·thickness + r`, spacing r/2, query box), and thickness and friction are at a sharp
error minimum. Open candidates: the axis-angle conversion (`glam`'s `Quat::from_mat3` against
`0x1424382b0`), and the candidate set of the particle BVH.

## Unknowns

- `SoftBodySettings`: the meaning of the 37 fields not matched live, the order of fields 0–2,
  and which of fields 24/33/36 is friction (all 0.2 in the flap's settings).
- Where S+60 scales the tether lengths; which external fields carry the blend and drag
  enables (S+6, S+64); the few-millimetre contact residual (section "Toward parity").
- The four unnamed leg bone hashes.
- Whether the per-step error budget meets visual parity (no side-by-side frame comparison).
- Why the engine steps with dt = 1/30 every frame at 60 fps (engine `0x141d96fb0`, mode c+92,
  rate c+108, default 30).
- The per-frame writer of the pinned targets (sim+504); when the LOD rule reduces the repeats.
- Parity of the `rsqrtps` estimate; float determinism is unresolved.
- The 4-wide tether mode (unused by the flap); op 12 sub-objects.
- The pointer kind byte; the 736-byte body class; the scarf resource; load precedence.
- Field `77b91c50`, the mapping `Offset`, and the two bytes after the influence counts.
- Visual parity with the game's coat (a per-step numeric comparison exists; no side-by-side
  frame comparison).

## Reproduction

```powershell
$g = $env:ACU_GAME_DIR
acu-rs/target/debug/acu-mat.exe "$g/DataPC_ACU_LGS_BelleEpoque_assets.forge" 5882 0 --hex --out=local/tailored/sb_flap.json
cd acu-rs; cargo test --locked -p acu_assets --test cloth -- --ignored
cargo test --locked -p acu_assets --lib cloth
cargo test --locked -p acu_sim cloth
cargo test --locked -p acu_tap_bridge cloth
# live (ACU in the world, acu_tap13 attached):
python tools/native_tap/cloth_census.py <pid> 0.5
python tools/native_tap/cloth_capture.py <pid> <body hex> idle 1.0
cargo run --release -p acu_tap_bridge --bin cloth_gate -- local/native-probe/cloth-live/cloth-idle-<pid>.bin
```
