# ACU-RE-0073 — Baked GI containers and WorldAmbiance lighting settings

Date: 2026-11-15 / Claude Code side session / task "port ACU's lighting (light and sun/sky data, baked
GI, AO) starting from the For Honor rewrite". Related: [LIGHTING_RESEARCH.md](../LIGHTING_RESEARCH.md),
[ACU-RE-0034](ACU-RE-0034-paris-type-vocabulary.md) (four GILoader / WorldAmbiance / LayeredSky /
Climate), [ACU-RE-0060](ACU-RE-0060-reflection-symbol-index.md) (reflection symbol index, on branch
`claude/lucid-dirac-c0ec4f`), [ACU-RE-0071](ACU-RE-0071-avatar-color-sets.md) (DXBC disassembly
method).
State: partially established (codec observed; GI evaluation inferred).
- The GI containers are observed: they decode to their exact end.
- The GICP codec is observed and implemented (`acu_assets::gicp`): an octree-delta byte codec, found
  through a live staging-upload hook. All 192 blobs of a Paris datablock decode to their exact end.
  The `GIDecodeYCbCr` combination into RGB SH texels is implemented from its DXBC.
- WorldAmbiance is partially decoded (forward prefix plus fixed tail). The Bevy scene applies the
  exposure and shadow settings; the sun, sky and fill are still synthetic.

## Provenance

- Native code. These were read, not executed:
  - The decompiled corpus `ACU - Decompiled/` (pseudocode of the dumped image).
  - The IDA database of the runtime dump `Unity/local/native-probe/motion-link-20261020/runtime-view/
    ACU.runtime-analysis.pe`: SHA-256 `47a81dc9f29c48a709cbdc1ef60a7e7db9eb8caefaabf30940dd4e1ee755d8c2`,
    image base `0x140000000`, from `ACU.exe` SHA-256 `748abdc6…7e40`. IDA was opened through
    idalib-mcp with auto-analysis off.
  - The installed `ACU.exe.i64` is the packed image: it has no code at these VAs, so it was not used.
- Class info (name, CRC = Forge type id, object size, reflected fields) comes from the
  ACU-RE-0060 symbol index. It was rebuilt in scratch from the mapped capture `.../capture/
  main-image.mapped.bin`. Loader = vtable slot 2. Descriptors referenced by the loaders were resolved
  from the mapped image (`descriptor + 0x70` = loader).
- Native loaders (VA in the runtime image):

| Class (type id) | Loader | Notes |
| --- | --- | --- |
| `GILoader` (`9ed472d4`, 256 B) | `0x1421CE1A0` | u64 Season, 3×f32, 3×f32, u32, pointer `GITreeNode`, 2×vec4, embedded `DataLayerFilter` |
| `GITreeNode` (`4aa4037e`, 80 B) | `0x1421CE710` | 3×f32, u32, 3×f32, u8, handle, u32 count + pointer children, embedded link |
| tree link (`a8a5bb0a`, 24 B) | `0x1421CE5F0` | u64, handle `GIResult` (`0x32ED41B1`), handle `GIDatablock` (`0xF7D22B3C`) |
| `DataLayerFilter` (`db1d406e`) / item (`e31593e1`) | `0x1426F16A0` / `0x1426F1640` | u32 count; item = handle + u32 |
| `GIDatablock` (`f7d22b3c`, 40 B) | `0x1421CDF00` | u64 Season, u32 count of inline `GIResult` |
| `GIResult` (`32ed41b1`, 112 B) | `0x1421CE480` | 3×u32, `Center`, `Extents`, vec4, handle, pointer `GIRenderIndoorVolume`, u8, u32 count of `GIData` |
| `GIData` (`daf9220e`, 112 B, no reflected fields) | `0x1421CDD20` | 4 × (u32 length + bytes), vec4 `+0`, vec4 `+16`, u8, u8, u32 `+96`, u8, u32 `+100` |
| `WorldAmbiance` (`21c2d472`, 3440 B) | `0x1421D03D0` | see below |
| `PostEffects` (`c83b8907`) | `0x14225A1D0` | 2 bytes then 26 embedded effects |
| `SSAOSettings` (`d82e4804`) / `SSBCSettings` (`d52d0013`) | `0x14225A600` / `0x14225A6E0` | |
| `HDRLightingSettings` (`33b77fec`) / `TextureSelector` (`7d08460d`) | `0x142258830` / `0x142205470` | |

- Stream helpers:
  - `0x14270B920` (pointer): u8 tag, where 0 = u64 id, u32 type, body; 2 = u64 ref; 3 = null.
  - `0x14270BA90` and `0x14270BC80`: u64, u32, body.
  - `0x14270BB60`: u8, u64, u32, body.
  - `0x14270FF90` (handle): u8, u64.
  - `0x14270E560` (handle): u8 tag, u8, u64, plus type and body when the tag is 0.
  - Arrays `0x14000B2A0` and `0x1405F08E0`: u32 count.
- Assets (payload SHA-256 equal to the asset index `payload_sha256`):

| Resource | Archive / entry | File id | Bytes | SHA-256 |
| --- | --- | --- | --- | --- |
| GILoader Night | `DataPC_ACU_Paris.forge` 21267 | `0000003a66d8e3b4` | 6,101,348 | `05265fa6…5c31` |
| GILoader Dawn | same | `0000003a66d8e3b5` | 6,553,119 | `89015f4e…07a5` |
| GILoader Noon | same | `0000003a66d8e3b6` | 6,504,119 | `7dc51e49…c738` |
| GILoader Sunset | same | `0000003a66d8e3b7` | 6,270,419 | `651ec844…cbe8` |
| `GIDB_Cell18068_LOD1_sid_7f5f1bc0b` | `DataPC_ACU_Paris_gi.forge` 3338 | `0000001758716bd6` | 743,650 | `7e0c8a3d…c08c` |
| `PAR_WorldAmbiance_Dawn/Night/Sunset/Noon` | `DataPC_ACU_Paris.forge` 22301–22304 | `…f5f1bb7b/bb9d/bbbf/bbe1` | 4,037 each | `b044dd36…`, `ff675df4…`, `dca7ebd3…`, `0814f1c8…` |

- GI decode shaders: four DXBC `cs_5_0` programs embedded in the image, extracted from the mapped
  capture and disassembled with `d3dcompiler_47.dll` `D3DDisassemble`:

| Image VA | Bytes | SHA-256 | Inputs | YCbCr constants |
| --- | --- | --- | --- | --- |
| `0x143315540` | 4,188 | `6d32fe99…81f7` | 5 | yes |
| `0x143318D30` | 6,528 | `66374988…cefa` | 16 | no |
| `0x14331D630` | 3,432 | `d34835c9…aaae` | 4 | yes |
| `0x1433200B0` | 7,392 | `5d17dc2f…f7cf` | 16 | yes |

  Their names come from the registration strings `GIDecodeMerged`, `GIDecodeRGB`,
  `GIDecodeUncompressed` and `GIDecodeYCbCr`. Which name goes with which program is not mapped.
  The disassembly and the scratch Python prototypes are kept in the ignored `local/lighting/`.
- For Honor sources compared. Read-only; nothing was copied. FH repository HEAD
  `0230fdcff910e15d6d162508f14e55b8ce4fe71b`:

| File | SHA-256 |
| --- | --- |
| `fh-rs/crates/fh_assets/src/environment.rs` | `77d18a83…838a` |
| `fh-rs/crates/fh_assets/src/gi_volume.rs` | `0653c0f6…4b3e` |
| `fh-rs/crates/fh_assets/src/sky_ambient.rs` | `ce9251ae…0213` |
| `fh-rs/crates/fh_assets/src/hbao.rs` | `c81d42d3…08b0` |

## Result

### Comparison with For Honor (what transfers)

| FH layout (file) | ACU | Verdict |
| --- | --- | --- |
| `GIStream` (`0xD16E3EBE`) embedded in a LOC preset: one remap grid of u32 cells, plus 7³ RGBA16F SH bricks and an R8 normal-bias atlas, found by blob-size products (`environment.rs` `load_gi_stream`, `gi_volume.rs`) | No `GIStream`. A `GILoader` per time-of-day Season holds a ~65k-node `GITreeNode` tree. Its leaves link `GIDatablock`s in the separate `*_gi.forge` archives. Each holds 16 m `GIResult` cells with four `GIData`, each of four GICP-compressed blobs | **does not transfer**. Layout, encoding and placement all differ |
| GPU lookup: remap cell, then brick, then trilinear SH planes `xyz·dir + w` (`gi_volume.rs`) | GPU: `GIDecode*` compute shaders turn four input 3D textures into four `g_GIDecodeOutput` volumes. Forward shaders sample `t_g_WorldGIVolume0..3` through `g_WorldGIIndirection` | Same family idea (SH volume plus indirection). No FH constant applies |
| LOC preset `SkyLight` panorama / 8×8 ambient cube (`environment.rs`, `sky_ambient.rs`) | `WorldAmbiance` → `LayeredSky` (2.36 MB, separate resource). The sun comes from the time-of-day manager | Not compared at byte level; layered-sky decoding is open |
| `PostEffects` type `0xC83B8907` (`environment.rs` `TYPE_POST_EFFECTS`) | Same type id `c83b8907`, embedded in `WorldAmbiance` | **Type id matches**. FH's "first dependency = grade LUT" rule was not tested; the ACU loader starts with SSAO then HDR |
| HBAO+ AO block defaults (`hbao.rs`) | `SSAOSettings` (`d82e4804`) with `SSBCSettings`; MSSAO shaders (`MSSAO_*_PS`) in the image | **does not transfer**. ACU uses its own MSSAO path, not HBAO+ |

Only the method transferred:
- separate irradiance from specular;
- keep resource identities;
- read the native loaders and shader bindings.

No FH parser module was imported.

### Observations

- The four Paris GILoaders decode to their exact payload end. Their `+48` handles are the Season
  resources `PAR_TOD_Paris_Night/Dawn/Noon/Sunset` (`…bc03`, `…bb41`, `…bc0b`, `…bc07`).
- Each loader has these tree counts:

  | Loader | Non-null nodes | Datablock-linked nodes |
  | --- | --- | --- |
  | Night | 61,000 | 61,000 |
  | Dawn | 65,518 | 65,454 |
  | Noon | 65,028 | 65,020 |
  | Sunset | 62,691 | 62,691 |

  Child arrays contain null slots (tag 3).
- All four loaders carry the same values:
  - `+80` = (32, 32, 32);
  - `+92` = (16, 16, 16);
  - `+16` = (−960, −864, −112, 0);
  - `+32` = (1568, 1376, 128, 0).
- Leaf nodes have size 16 or 8. They link a `GIResult` id and a `GIDatablock` resource id; many
  leaves share one datablock.
- `GIDB_Cell18068_LOD1_sid_7f5f1bc0b` decodes to its exact end:
  - Season `PAR_TOD_Paris_Noon`;
  - 12 `GIResult`s, each `Extents` (16, 16, 16), e.g. `Center` (−720, −336, −48);
  - 4 `GIData` per result.
  - Every GICP blob starts `GICP`, u16 1, u16, then u32 dims (16³; the 4th blob is sometimes 32³).
  - The GIData `+100` is 32. `+96` is 7 or 15.
- Static search did not find the codec: the magic is compared through memory, not as an
  immediate. The one byte match `0x144FB3C5C` is `dword_144FB3C5C`, the constant
  `sub_1421C4040` compares against (it sits in a hash/name table).
- **Live route to the codec** (2026-11-15, ACU pid 126620 in Paris, native probe hardware hooks on
  system `d3d11.dll` SHA-256 `2359b84d...2539`; RVAs from a local hardware device:
  `CreateTexture3D` `0xE0E70`, `Map` `0x12C360`, `Unmap` `0x12C5F0`, `CopySubresourceRegion`
  `0x12F3A0`; traces `local/lighting/live/upload-2.jsonl`, `staging-1.jsonl`):
  - `CreateTexture3D` never fired (pool built at load). `UpdateSubresource` only uploads buffers.
  - `Map` / `Unmap` go through one lock wrapper `sub_1422F0E60` / `sub_1422F1150`. Its staging branch
    (`sub_1422F0E00`, `Map` WRITE, then `CopySubresourceRegion`) ran during teleports into the GI
    domain. A hook at `0x1422F1067` with a 256-byte snapshot at `[rsp+0x78]` returned 2,985 times to
    `0x1421DB931` (`sub_1421DB8C0`, which maps 16 textures), called from `sub_1421E59F0` and
    `sub_1421DB500` (cell streaming, atlas slot `x = i % +684`, `y = i / +684`).
- The decode chain (corpus pseudocode, re-derived):
  - `sub_1421DB500` schedules a job `sub_1421B0490` -> `sub_1421CB1F0` -> `sub_1421C5890(pool, ctx, slot)`.
  - `sub_1421C5890` calls `sub_1421C3D20` per 112-byte GIData. With the pool's compressed flag
    (`+900`) and `GIData +104 == 0` it uses `sub_1421C4310` when `+106` is set (each of the four
    blobs -> one 8-bit channel; GIData `k` blob `j` -> mapped texture `4k + j`), else
    `sub_1421C44F0` (blob 0 -> four interleaved channels). It copies GIData `+0` to `+768 + 16k`
    and `+16` to `+832 + 16k`, then copies (`sub_1428F5900`) the decoded bytes into the mapped
    textures.
  - `sub_1421C4040` (core): `GICP` magic; edge `+8`; per-channel offset byte `+20`; root sample
    `+24`; literal count `+28`; literals at `+32` (u32 node number + 8 corner bytes); then a u32
    bitstream byte count and the bitstream (read MSB-first in byte-swapped 64-bit words).
  - `sub_1421E6220`: pre-order octree. Each node takes the next node number. A matching literal
    writes its 8 corner bytes. Otherwise read 3 bits `w`; if `w > 0`, 3 bits min-corner index `m`
    and 7 values of `w` bits; insert 0 at `m`; corners 1..7 = `corner0 - d[0] + d[k]` (bytes,
    wrapping). Corner order (x, y, z) = 000, 100, 010, 110, 001, 101, 011, 111. Nodes larger than
    2 recurse into 8 children in the same order. The node counter runs on across channels.
  - `sub_1421E6070`: slice z = 0 gets `+= offset + 0x80`; each later slice `+= previous + 0x80`.
    `sub_1421E61B0` interleaves four channels.
- Exact end on real data: in `GIDB_Cell18068_LOD1_sid_7f5f1bc0b` all 192 blobs consume every
  literal, `ceil(bits / 8)` equals the stored byte count, and exactly 8 padding bytes follow. All
  GIData there have `+104` 0, `+105` 1, `+106` 1 (YCbCr path).
- `GIDecodeYCbCr` (`0x1433200B0`) data flow, from the DXBC: textures `t0..t15` are the 16 inputs.
  For GIData `k`: `a = t(4k)*S.x + B.x`, `b = t(4k+1)*S.y + B.y`, `c = t(4k+2)*S.z + B.z`,
  `w = |t(4k+3)|^2.2 * S.w + B.w` with `S = cb5[3+k]`, `B = cb5[7+k]`. `m = |w0|`. Coefficient
  `j < 3` has (Y, Cb, Cr) = `m * (comp_j of GIData 0, 1, 2)`; coefficient 3 has (`w0`, `m*w1`,
  `m*w2`). BT.601 rows give R, G, B, stored to `u0`, `u1`, `u2` (one texel = four coefficients).
  `u3 = (a3, b3, c3)*|w3|, w3`.
- Forward GI evaluation, from the deferred lighting pixel shader at `0x143211F50` (ps_5_0, 41 other
  shaders in the image bind the same `g_WorldGIIndirection`; DXBC disassembled with
  `D3DDisassemble`, `local/lighting/fwd/`). Reflected names from its RDEF:
  - `cb_g_Frame.m_GIConsts` (`GISamplingConfig`: `Params`, `WSToIndirectionScale`, `...Bias`,
    `InvVolumeSize`, `InvVolumeAtlasSize`), `m_SunLight` (`DirectLightConsts`: `m_Direction`,
    `m_Color`), `m_SunShadows` (`m_SplitBlendMulAddUVAndZ[4]`, `m_CascadesRangesMax/Min`...),
    `cb_g_Pass.m_GlobalLightingScale`; `g_WorldGICells` (`GIIndirectionCell`, 52 bytes:
    `WSToUnitScale`, `WSToUnitBias`, `UnitToVolumeScale`, `UnitToVolumeBias`, `NormalBiasScale`),
    `g_IndoorGIVolumes` (64-byte matrices), `g_SkySHConstants` (`SkyLightingSH`: `Values[7]`,
    `SkyLightingUp`, `SkyLightingScale`).
  - Lookup: `i = g_WorldGIIndirection.Load((p + n*Params.y*0.5)*WSToIndirectionScale +
    WSToIndirectionBias)`; `i == 0` (or `Params.x == 0`) falls back to a cube map. Otherwise cell
    `i - 1`: `unit = saturate(p*WSToUnitScale + WSToUnitBias + n*NormalBiasScale*Params.y)`,
    `uvw = unit*UnitToVolumeScale + UnitToVolumeBias`, then `t_g_WorldGIVolume0..3` at `uvw`
    (per-axis, no swizzle: atlas u, v, w = world x, y, z). Up to two indoor volumes blend in by
    8-bit weights.
  - Evaluation: `GI_c = (dot(V_c.xyz, n) + V_c.w) * s` for c = R, G, B (`V_c` = volume c,
    `n` = world normal, `s = occlusion * m_GlobalLightingScale.y`), plus
    `max(0, SkySH_L2(n)) * (dot(V3.xyz, n) + V3.w)`. So volumes 0..2 hold bounce light and volume 3
    is a sky-visibility L1 that multiplies a separate L2 sky SH. The pass output is scaled by
    `m_GlobalLightingScale.x`.
- `0x14331D630` (4 inputs `t0/t4/t8/t12`, `cb_g_DecodeConstants` = `InputScale`, `InputBias`,
  `WriteOffset`, `Scale[4]`, `Bias[4]`) does the following:
  - samples each input with `sample_l` at a half-texel-corrected coordinate;
  - multiplies three channels by `max(|t0.x|, 0)`;
  - converts with rows (1.16895, 0, 1.60229), (1.16895, −0.393299, −0.816156), (1.16895, 2.02514, 0);
  - writes `u0..u2`, then `t12.xyz * |t12.w|` to `u3`.
- `WorldAmbiance` forward decode, identical layout in all four presets:
  - The `LayeredSky` handle (Noon → `PAR_LayeredSky_Noon` `…bb4a`), then `PostEffects`.
  - `SSAOSettings`: enabled 1, `+96` 7.0, `+100` 2.5, `+104` 5.
  - `SSBCSettings`.
  - `HDRLightingSettings`:
    - 5.6, 250, `ISO` 100 and 18 in all four presets;
    - `+132` = Dawn 1.5, Noon 3.0, Sunset 1.5, Night −4.8;
    - a 32-float curve at `+160`, which differs per preset.
  - Forward decoding stops at byte 407, the start of the third post effect.
- `WorldAmbiance` tail (last 75 bytes, from the loader's fixed reads), identical in all four presets:
  - `+3356` = 1024;
  - `PSSMShadowEndDistance` 128, `PSSMSplitWeight` 0.9, `+3368` 0, `PSSMShadowNoiseSize` 3;
  - then 2, 1e-4, 2e-4, 3e-4, 5e-4;
  - a byte 1;
  - `+3396` = (5, 14, 38, 72);
  - bytes 1, 1;
  - `+3412` = (0.65, 1.3, 2.6, 5.2).
- Native consumer of the shadow tail (mapped image `main-image.mapped.bin`, base 0x140000000,
  capstone): `sub_1421F9030` (size 8787; corpus `ACU - Decompiled/1421F9030.c`) reads `+3356`,
  `+3360`, `+3364`, `+3368`, `+3372`, `+3376`, `+3380..+3392`, `+3396..+3408`, `+3412..+3424` and
  bytes `+3430..+3432` from one object (`rbx`). Same offsets as the WorldAmbiance loader writes.
  - `0x1421F92EF..0x1421F9343` copies `+3396..` into a per-cascade local array.
  - `0x1421F9A60..0x1421F9ACB` loads `+3412..+3424` into a 4-float local.
  - `0x1421FA7BE..0x1421FA7F8` copies `+3380..+3392` into a per-cascade local; `0x1421FB192` reads it
    per cascade.
- The user's own plugin `ACUFixes/plugins/HardShadowPatch.dll` (SHA-256
  `ba525fcd1cce5c7fe4e1f9465f43c4bc4ae3d66914bc16dc02c95541b095883f`; first-party, user-confirmed)
  patches exactly these three sites. Its setting names are `AdjustablePSSMSplitDistance`
  (`0x1421F9310`, 0x30 bytes), `AdjustablePSSMSplitBlendRegion` (`0x1421F9AB0`, 0x22) and
  `AdjustablePSSMCascadeBias` (`0x1421FB192`, 0xC). Its default `SplitBlendRegion1..4` equal `+3412`.
  Its `ShadowResolution` patches other objects (`0x1422D9985` `[rbp+0xDCC]` in `sub_1422D9320`;
  `0x1421E409F` / `0x1421E41E2` `[rdi+0x128]`), not `+3356`.

### Inferences (labelled; not proven)

- GILoader `+16` / `+32` are the world-space minimum and size of the GI domain (Paris:
  x −960..608, y −864..512, z −112..16). Tree `min` / `size` are cell corners.
- GIData 0..2 are the Y, Cb, Cr planes of an L1 SH (coefficients 0..2 the world x, y, z terms,
  stored relative to `|L0|`; coefficient 3 is L0). Now supported by the forward shader above,
  which evaluates `dot(xyz, n) + w`; GIData 3 is the sky visibility the shader multiplies with
  the sky SH.
- GIData `+0` / `+16` are the shader's `Scale` / `Bias` (`cb5[3..6]` / `cb5[7..10]`): copied into
  two consecutive 4 x vec4 blocks; the constant-buffer binding was not observed.
- The output sample position uses `cb5[0] = 1/(n-1)` (not observed); the implementation resamples
  every input to a 16^3 grid.
- HDR `+120` / `+124` are aperture (f/5.6) and reciprocal shutter (1/250 s), and `+136` is middle
  grey 18%. EV100 = log2(5.6² · 250 · 100/100) = 12.94.
- Tail `+3356` is the shadow map resolution (still inferred; the plugin's resolution patch is elsewhere).
- Strongly supported by the native consumer and the plugin names: `+3396` = PSSM split distances,
  `+3412` = split blend regions, `+3380..+3392` = per-cascade depth biases. That `rbx` in
  `sub_1421F9030` is the WorldAmbiance instance (not a copy with the same layout) is inferred.
  `+3376` (2) is unknown.

### Unknowns

- The runtime values of `m_GlobalLightingScale`, `m_SunLight`, `g_SkySHConstants` and the
  per-cell `UnitToVolume` inset (texel placement inside a cell). Without the sky SH the Bevy port
  shows bounce light only, at a synthetic scale.
- `GIData +96` (7 / 15), `+105`; the RGB (`sub_1421C44F0`) path is implemented only as the
  decoder's channel count (Paris data does not exercise it).
- Sun direction and intensity (the time-of-day manager), the `LayeredSky` layout, the 24 later post
  effects, `Wind` and `WorldFog`. HDR `+132` and the curve are also unknown.
- How `PSSMShadowEndDistance` (128) relates to the last cascade distance (72).
- Which duplicate of each id the game loads (#35). Other cities (Versailles, Saint-Denis) were not
  decoded.

### Independent checks

- The Python prototypes (`local/lighting/gi.py`, `gidb.py`, `wa.py`) and the Rust decoder agree on
  every count and value above.
- Synthetic unit tests use independent byte builders.
- The install tests pin payload hashes that match the asset index's independently computed
  `payload_sha256`.
- Truncation, trailing bytes, wrong type ids, unsupported tags, bad GICP magic and length overrun
  all fail.
- GICP: the Python prototype (`local/lighting/gicp.py`, `gicp_ref.py`) is a separate
  implementation. The Rust install test pins its SHA-256 of two decoded volumes and one combined
  texel (12, 0, 15). Synthetic unit tests use an independent test encoder and check delta,
  literal, flat-node, padding, unused-stream and unused-literal cases.

### Reproduction

```powershell
cd acu-rs
cargo test --locked -p acu_assets --lib -- gi:: gicp:: ambiance::
$env:ACU_GAME_DIR = 'D:/Games/Assassin''s Creed Unity'
cargo test --locked -p acu_assets --test gi -- --ignored
cd ..
powershell -File tools/run_arno.ps1 -Capture outputs/lighting/arno-paris-noon.png   # default --lighting paris-noon
```

### Implementation

- `acu_assets::gi`: `decode_loader` and `decode_datablock`. GICP blobs are kept as bytes, with
  their dims.
- `acu_assets::gicp`: `decode(blob, channels)` (exact-end checked), `combine_ycbcr` (one
  `GIDecodeYCbCr` texel) and `decode_cell` (a result's 16^3 texels).
- `acu_assets::ambiance::decode`: the SSAO / HDR prefix and the shadow tail. `HdrLighting::ev100`
  computes EV100.
- `acu_bevy::lighting`, selected by `--lighting synthetic|paris-dawn|paris-noon|paris-sunset|paris-night`
  (default `paris-noon`), applies:
  - camera `Exposure` = native EV100;
  - `DirectionalLightShadowMap` 1024;
  - `CascadeShadowConfig.bounds` = (5, 14, 38, 72).

  Sun direction, sun illuminance (Bevy `DIRECT_SUNLIGHT`) and clear colour are synthetic. They
  are listed in the scene report's `lighting.synthetic`.
- `acu_bevy::gi_volume` (with a Paris preset): gathers every leaf of the preset's `GILoader` that
  touches a 24 x 24 x 8 m region around the anchor (496.664, -7.308, -0.252), Arno's live position
  on 2026-11-15. It decodes each cell with `gicp::decode_cell` and resamples the region at 1 m,
  finest leaf first (Noon: four level-0, one each of levels 1, 2, 5, 7, 9; 0 uncovered voxels;
  `GIResult Center` = tree min checked for every cell). One Bevy `IrradianceVolume` holds the
  ambient cube faces `max(0, L0 +- L1_axis)` in Bevy axes and replaces the synthetic ambient fill.
  The intensity scales the median lit L0 luminance (0.00837 at Noon) to the former 4000 fill
  (SYNTHETIC). Observation: the fine leaves stop at the street surface; the air above the street
  is covered only by levels 5+.
- Capture: `outputs/lighting/arno-paris-noon-gi.png` (scene report
  `local/captures/lighting/scene-gi.json`).
