# ACU-RE-0013 — Material and diffuse texture chain

Date: 2026-10-19. Task: ACU-004 / ACU-005.
State: partially established.

## Provenance

Asset-byte investigation for installed ACU.exe SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
image base `0x140000000`. No native VA/RVA or pseudocode-export correspondence
is established here. Original archives are read only.

Layout reference: AnvilToolkit.dll 1.3.7.0 SHA-256
`940374775553172b06d55537c4db78048bc3206b9b2f72bcc61bacfdcdb57636`,
decompiled with ilspycmd 8.2.0.7535. The `Game.Unity` branches of Material,
TextureSet, Mask, Mask16, TextureSelector, UVTransform, FileReference and dynamic
base parameter types are saved under ignored `local/reference-atk-materials/`.
This is third-party layout corroboration, not native execution evidence.
Byte probes and intermediate reports are in `local/material-re-20261018/`.

`DataPC_ACU_LGS_BelleEpoque.forge` entry 679 contains these material/set pairs:

| Material ID | Material payload SHA-256 | Texture set ID |
| --- | --- | --- |
| `0000000b1b911534` | `29f787cffa2cb89fc6ec9827cf544369b119939f6e6dfeab6e5114d608970988` | `0000000b1b911510` |
| `0000000b1b911533` | `2e4fc62575dfeb364eadd24c6bac540f4c26d1ea16f061c8a2ee1f3a0d052bc2` | `0000000b1b911503` |
| `0000000b1b911530` | `35e9a2d8eabf78fb1fc727ed0140336c987767688f222b4b6af480dc975861f5` | `0000000b1b9114ea` |

For the first material, diffuse map `0000000b1b911514` is in
`DataPC_SharedGroup_00.forge` entry 628, resource 0. Stored-entry SHA-256:
`61397e1117d66b19ba58ecd48d3ba294de2666c5121471e549a9e9857f58106b`.
Decoded payload range `[39,699232)` (699193 bytes), SHA-256:
`dab4b9bc3f8bbc8ad63be99f1abe463bf75f1f0cc4f11c2eb766a568c011f2ae`.
All native mip pixels SHA-256:
`51aae5ca2510697727a45644631058def6e41ade4d0bba841fc9ed66564ddba7`.
The texture is 1024×1024, DXGI 71 (BC1), gamma 1, eleven stored mip levels.
Exact entry offsets and remaining selected payload identities are reproducible
through the scene report and the archive reader; the stored-range hashes above
identify the bytes independently of archive relocation.

## Result

The throwing-blade material `00000013c1e4eb48`, `DataPC.forge` entry 110,
resource 12269, exposes a nonboolean reference scope: its TextureSet reference
at payload byte 23 has tag 1, scope byte 4, ID `000000131c3be1b0`.
Payload range `[50840397,50841700)`, length 1303, SHA-256
`ca3e68b3c6dc8355b2eee16777b6e07c8a37686a8775ac8f89c7f039341b92e1`;
stored-entry SHA-256
`7d5f500049eeb22158005bbd74fb8c22b84b84501d226aa6bfc66c4b0aa2334f`.
The reference reader also reads `IsGlobal` as a byte. The parser preserves that
byte without asserting a boolean range; the meaning of scope 4 remains unknown.
This fixes an overly restrictive check exposed by the first scene load, rather
than guessing reference semantics. `mat-13c1-hex.json` records the probe.

The bounded parser consumes complete Material and 142-byte TextureSet payloads.
Dynamic parameters retain names, type/class hashes and typed values. Unsupported
parameter variants fail explicitly. Selector map type 0 and the set's diffuse
reference agree for the three outfit vectors above. Map type 1 agrees with the
normal reference. Numeric type 7 names the set's Mask1 texture in these samples;
the reference-tool enum calls 7 MapOcclusion, so role names alone must not replace
serialized IDs. Authored fixtures cover values, truncation, trailing bytes,
unsupported types and allocation limits.

Draw-part material binding uses the corresponding compiled instancing record
only when draw counts and per-part vertex counts match. This is a bounded
correspondence for the inspected assets, not a universal cluster/submesh rule.
The scene resolves local package resources before a catalog over explicitly
selected archives. Duplicate catalog candidates use the first selected candidate
and are reported; this does not establish native patch precedence.

The presentation uploads the stored diffuse mip chain directly as GPU BC data,
uses decoded UV0, displays diffuse colors in sRGB, and applies serialized alpha
test flags/thresholds. Roughness, lighting, two-sided rendering and the grey arena
are presentation choices. Original template shaders, skinning, normal/specular
maps, tint/mask layering and native material parity remain unimplemented.

Reproduce from `acu-rs/` with `ACU_GAME_DIR` set:

```powershell
cargo test --locked -p acu_assets --test material
cargo test --locked -p acu_assets --test material -- --ignored
cargo run --locked -p acu_bevy -- --capture ../local/arno-textured.png --report ../local/arno-textured.json
```

The JSON report retains mesh, material and texture IDs, source locations and
payload/pixel hashes without extracting game payloads to disk. It records missing
references and material selector/set disagreements explicitly.

## Visible validation

Fresh build: `cargo build --locked --workspace`, Rust 1.99.0 MSVC, Bevy 0.19.1.
Eleven pieces / nineteen draw parts resolve eight materials and seven resident
diffuse textures. No part is unbound; no chain report notes remain. Captures at
35°, 145° and 260° in `local/captures/acu-arno-textured-20261019/` were inspected:
blue cloth, leather, red scarf and skin align coherently on the assembled static
model, with no exploded geometry or obvious missing texture. This supports UV0
as-is for this set, without establishing universal native UV conventions.

An actual DX12 window capture (`window-dx12.png`) was also inspected successfully.
Vulkan offscreen captures work; the Vulkan window instead reports invalid
present-image layout and semaphore state on this machine. The launcher selects
DX12 for interactive inspection and Vulkan for headless capture. Backend selection
is a local presentation workaround, not a game-data inference.

Cargo fmt/clippy/workspace tests/build and both explicit material install tests
pass. Five authored material tests include the scope-byte regression; the native
blade payload is also checked by exact hash and scope in the install test.
The capture manifest records executable/source digests, arguments, selected
assets and output hashes. No original-game runtime comparison was performed.
