# ACU-RE-0081 — Coat cloth: the `Cloth` soft-body resource and its native library

Date: 2026-11-15. Claude Code session. User request: "Port the cloth physics on the coat" (Arno's
Tailored outfit, [ACU-RE-0070](ACU-RE-0070-arnos-tailored-visual-pieces.md)).

State: **partially established.**
- Observed: the resource layout prefix, the record framing, three record meanings, and the class
  and field names.
- Not established: the cloth solver. It is not in the decompiled corpus and has not been ported.
  No simulation runs in the viewer yet.

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

### Where the solver is (not established)

The corpus has only 135 functions in `0x1423a0000`–`0x142450000`, and 69 of them are these
loaders. The library's other functions, including the step function, are absent from the corpus.
Opening the existing IDA database `ACU.runtime-analysis.pe.i64` headless did not respond within
30 minutes in this session.

The next step is one of two routes:
- disassemble the step function from the mapped image (Capstone, as in ACU-RE-0028);
- locate it live through the acu_tap pipe: trace the vtable of the 824-byte body object built by
  `0x1423f0bc0`.

## Implementation

`acu_assets::cloth` provides:
- `blob()`: record walk with the native framing checks; requires the version and exact end.
- `Blob::{find, settings_name, body_kind, particle_count, particles}`.
- `parse()`: the prefix above, up to LOD0's blob. It rejects unknown pointer tags, class mismatches,
  count disagreements and nonfinite positions.

Tests:
- 3 unit tests on synthetic blobs: framing, magic, length, version and padding errors.
- `tests/cloth.rs`, `#[ignore]`: the installed flap, checking the hash, IDs, counts, settings name,
  first particle and offset.

**Not implemented:** no simulation and no Bevy integration. Deliberately, the viewer runs no
substitute cloth, because the solver's behaviour is not recovered.

## Unknowns

- The solver, integration and constraints.
- The meanings of the six u8 arrays and the w flag.
- The pointer kind byte.
- The undecoded LOD fields.
- How render vertices follow particles (`SoftBodyVertexMapping`).
- Collision shapes.
- The 736-byte body class.
- The scarf resource.
- Load precedence.

## Reproduction

```powershell
$g = $env:ACU_GAME_DIR
acu-rs/target/debug/acu-mat.exe "$g/DataPC_ACU_LGS_BelleEpoque_assets.forge" 5882 0 --hex --out=local/tailored/sb_flap.json
cd acu-rs; cargo test --locked -p acu_assets --test cloth -- --ignored
cargo test --locked -p acu_assets --lib cloth
```
