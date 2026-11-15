# ACU-RE-0073 — Baked GI containers and WorldAmbiance lighting settings

Date: 2026-11-15 / Claude Code side session / task "port ACU's lighting (light and sun/sky data, baked
GI, AO) starting from the For Honor rewrite". Related: [LIGHTING_RESEARCH.md](../LIGHTING_RESEARCH.md),
[ACU-RE-0034](ACU-RE-0034-paris-type-vocabulary.md) (four GILoader / WorldAmbiance / LayeredSky /
Climate), [ACU-RE-0060](ACU-RE-0060-reflection-symbol-index.md) (reflection symbol index, on branch
`claude/lucid-dirac-c0ec4f`), [ACU-RE-0071](ACU-RE-0071-avatar-color-sets.md) (DXBC disassembly
method).
State: partially established.
- The GI containers are observed: they decode to their exact end.
- The GI irradiance values are NOT decoded, because the CPU codec for the GICP blobs is unknown.
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
- The GICP body is sparse records (8 bytes plus a u32 index) followed by a high-entropy region.
  Header words `+20..+28` look like stale pointer halves.
- No CPU routine for GICP was found:
  - The immediate `GICP` does not occur in the image's code. Its one byte match is reflection data.
  - A full `grep -rlE "0x50434947|1346587975|GICP"` and `grep -rlF "'PCIG'"` over all 217,041
    corpus files (44 min) returned no file.
  - The xrefs to the GIData descriptor are only loaders and the type-registration table
    `0x14223CD60`.
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

### Inferences (labelled; not proven)

- GILoader `+16` / `+32` are the world-space minimum and size of the GI domain (Paris:
  x −960..608, y −864..512, z −112..16). Tree `min` / `size` are cell corners.
- The four GIData of a result are SH components (one plausible reading: L0 then three L1 axes). The
  four blobs are colour channels plus an extra channel, possibly in YCbCr, given the shader. The
  GIData `+0` / `+16` vectors feed the shader's `Scale` / `Bias`.
  - Evidence: GIData 3 has `+0.w + +16.w = 1.0` (0.7109 + 0.2891). Its x and y are sign-symmetric.
  - Alternative readings (a min/range pair, or another basis) are not excluded.
- HDR `+120` / `+124` are aperture (f/5.6) and reciprocal shutter (1/250 s), and `+136` is middle
  grey 18%. EV100 = log2(5.6² · 250 · 100/100) = 12.94.
- Tail `+3356` is the shadow map resolution. `+3396` are per-cascade far distances (4 cascades).
  `+3376..` are per-cascade depth biases.

### Unknowns

- **The GICP codec.** No CPU decoder was found yet. Next leads:
  - consumers of `GIData +32..+88`;
  - the code that binds `t_g_GIDecodeInput*`;
  - a live hook on the 3D-texture upload (authorized in AGENTS.md).

  Until the codec is known, no ACU irradiance can be rendered.
- Which shader variant each GIData uses (`+96`, `+104..+106` flags?), and the cell → world-volume
  indirection.
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

### Reproduction

```powershell
cd acu-rs
cargo test --locked -p acu_assets --lib -- gi:: ambiance::
$env:ACU_GAME_DIR = 'D:/Games/Assassin''s Creed Unity'
cargo test --locked -p acu_assets --test gi -- --ignored
cd ..
powershell -File tools/run_arno.ps1 -Capture outputs/lighting/arno-paris-noon.png   # default --lighting paris-noon
```

### Implementation

- `acu_assets::gi`: `decode_loader` and `decode_datablock`. GICP blobs are kept as bytes, with
  their dims.
- `acu_assets::ambiance::decode`: the SSAO / HDR prefix and the shadow tail. `HdrLighting::ev100`
  computes EV100.
- `acu_bevy::lighting`, selected by `--lighting synthetic|paris-dawn|paris-noon|paris-sunset|paris-night`
  (default `paris-noon`), applies:
  - camera `Exposure` = native EV100;
  - `DirectionalLightShadowMap` 1024;
  - `CascadeShadowConfig.bounds` = (5, 14, 38, 72).

  Sun direction, sun illuminance (Bevy `DIRECT_SUNLIGHT`), ambient fill and clear colour are
  synthetic. They are listed in the scene report's `lighting.synthetic`.
