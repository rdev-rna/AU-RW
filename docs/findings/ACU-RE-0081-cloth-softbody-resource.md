# ACU-RE-0081 — Coat cloth: the `Cloth` soft-body resource and its native library

Date: 2026-11-15. Claude Code session. User request: "Port the cloth physics on the coat" (Arno's
Tailored outfit, [ACU-RE-0070](ACU-RE-0070-arnos-tailored-visual-pieces.md)).

State: **partially established.**
- Observed: the resource layout prefix, the record framing, three record meanings, and the class
  and field names.
- Observed live: the cloth solver's functions and their per-frame order (section "Where the
  solver is"). They are in the corpus.
- Not established: the solver's semantics. It has not been ported, and no simulation runs in the
  viewer yet.

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
- `acu_sim`: 12 unit tests with independent expected values (free fall, pinned snap, speed cap and
  damping, stretched pair, one-sided clamps, SIMD vs scalar agreement, backstop, max distance,
  tether, normals, drag, carry, the repeat program and opcode rejection).

**Not implemented:** collision (op 7), so the cloth can pass through the body; no Bevy
integration yet (targets from particle skinning and the render-vertex mapping are next).

## Unknowns

- Collision: the swept path `0x1423d58a0` and the character's collider shapes.
- The live substep mode and rate (engine `0x141d96fb0`, mode c+92, rate c+108, default 30).
- The per-frame writer of the pinned targets (sim+504) and the LOD repeat reduction in play.
- Parity of the `rsqrtps` estimate; float determinism is unresolved.
- The 4-wide tether mode (unused by the flap); op 12 sub-objects.
- The pointer kind byte; the 736-byte body class; the scarf resource; load precedence.

## Reproduction

```powershell
$g = $env:ACU_GAME_DIR
acu-rs/target/debug/acu-mat.exe "$g/DataPC_ACU_LGS_BelleEpoque_assets.forge" 5882 0 --hex --out=local/tailored/sb_flap.json
cd acu-rs; cargo test --locked -p acu_assets --test cloth -- --ignored
cargo test --locked -p acu_assets --lib cloth
cargo test --locked -p acu_sim cloth
```
