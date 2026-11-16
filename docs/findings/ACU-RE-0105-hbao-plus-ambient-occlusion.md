# ACU-RE-0105 — How ACU's max-quality ambient occlusion (NVIDIA HBAO+) is parameterised and computed

Date / investigator / related task: 2026-11-16 / Claude (lighting session) / lighting port, max
settings target (`docs/TASKS.md`)
State: partially established (parameters and kernel observed; some field names and the jitter
table inferred)

## Provenance

- Binary: `GFSDK_SSAO.win64.dll` from the ACU install, SHA-256
  `3194ccc5afe00ceed7fa0eb827bd44be82c991ce2409436c1ee8c88cb859b1f3`, image base `0x180000000`.
  It was analysed in IDA (idalib, Hex-Rays) on a byte-identical copy in `local/hbao/`.
- Exports: `GFSDK_SSAO_CreateContext_D3D11` and `GFSDK_SSAO_CreateContext_GL`.
- Functions:
  - Renderer vtable `GFSDK::SSAO::D3D11::Renderer` at RVA `0x9D4B8`:
    - slot 2: `RenderAO`, `0x180010880`;
    - slot 3: `PreCreateRTs`, `0x18000CA00`;
    - slot 4: `Release`, `0x18000CC00`.
  - `SetAOParameters`: `0x18000CA80`.
  - Constant derivation: `0x1800085A0`.
- Live capture:
  - Probe: `tools/native_probe/hbao_probe.cpp`, built by `build_shader_probe.ps1` as
    `acu_hbao_probe.dll`, SHA-256 `abe60d89…89c7`.
  - It is a MinHook detour on `RenderAO`, installed only after the first 10 bytes match the
    analysed copy.
  - Injected with `capture_run.py --extra-dll` into ACU pid 8324 at the max preset
    (`shader_quality.py set high`), pinned save `capture-start-1`. The Low preset was restored
    afterwards.
  - Output: `local/native-probe/hbao-max-20261116/acu_hbao_probe-8324.json` (status `hooked`; four
    captures, all identical; 5,851 calls by the last dump, 7,533 at close).
- Shader comparison:
  - Tool: `tools/native_probe/hbao_shader_compare.py`. It disassembles the DLL's 31 DXBC blobs
    (29 ps, 1 vs, 1 gs) with D3DDisassemble into `local/hbao/asm/<file offset>.asm`.
  - It matches each blob's opcode sequence against For Honor's read-only HBAO+ dumps
    (`ForHonorRustRewrite/local-extracted/hbao/*.asm`, 3.x SDK statically linked in For Honor).
  - Command: `ACU_GAME_DIR=… python tools/native_probe/hbao_shader_compare.py`.
- For Honor reference (read only):
  - `fh-rs/crates/fh_assets/src/hbao.rs`;
  - `fh-rs/crates/fh_bevy/src/fh_hbao.{rs,wgsl}`, commit `546a3776`.

## Result

### Observations

- **`Parameters` passed to `RenderAO`** (r9). Offsets are in bytes; the values are identical on
  every logged call; mask 3.

  | Offset | Value | How the DLL uses it (`0x1800085A0`) |
  | --- | --- | --- |
  | +0x00 | 0.8 | × MetersToViewSpaceUnits → R; cb[9] = R², cb[10] = −1/R², cb[8] = R·0.5 / a[13] · a[8] |
  | +0x04 | 0.01 | clamped to [0, 1] → cb[11] (N·V bias) |
  | +0x08 | 1.34 | clamped to [0, 2], × 1/(1 − bias) → cb[12] |
  | +0x0C | 1.09 | clamped to [0, 2], × 1/(1 − bias) → cb[13] |
  | +0x10 | 2.4 | floored at 0 → cb[14] |
  | +0x14 | 0 | enable flag: cb[26] = −1/max(+0x18, ε) and cb[27] = max(+0x1C, 0) when set; else 0 and 1 |
  | +0x18, +0x1C | 60.0, 100.0 | (unused, the flag is 0) |
  | +0x20, +0x24, +0x28 | u32 1, u32 2, 8.0 | not read by `0x1800085A0` |
  | +0x2C..+0x38 | 0, 4.0, 0.5, 1.0 | not read by `0x1800085A0` |

- **Shader families in the DLL** (file offsets), and how they compare with For Honor's 3.x SDK:
  - **Opcode for opcode identical (similarity 1.000):**
    - the full-screen vertex shader and the layer geometry shader;
    - the reinterleave pass (`0x73750`, `0x739B0`);
    - the radius-2 and radius-4 cross-bilateral blurs (`0x73C00`..`0x751D0`) and their
      sharpness-profile variants (`0x775D0`..`0x78D30`).
  - **CoarseAO** (`0x7B410` and 5 siblings, about 495 instructions each), opcode similarity
    0.85–0.875. It is the same kernel:
    - quarter-resolution deinterleaved depth;
    - 8 directions 45° apart, 4 steps each of `radius_px · 0.05` quarter-resolution texels,
      starting one texel out plus jitter;
    - the falloff `sat(N·V/|V| − cb[11]) · sat(1 + |V|²·cb[10])`;
    - an early AO = 1 when `cb[8] / z < 1`;
    - the result `sat(1 − Σ·k / 16)`.
  - **Where ACU's CoarseAO differs:**
    - all 32 samples are weighted by cb[13];
    - the second term is `normal.w · cb[12] · 8`, a literal 8 where For Honor reads a constant;
    - there is no foreground or background radius clamp.
  - **ACU only: the normal pass** (`0x717B0`, and `0x72490` / `0x72DE0` with input normals).
    - It rebuilds the view-space normal from depth with the nearer neighbour on each axis.
    - When cb[12] ≠ 0 it also writes, to alpha, the mean of 4 full-resolution falloff taps at
      ±3 px in x and y (else 0).
  - **ACU only: the radius-8 blur** (`0x75AF0`, `0x76830`, `0x796C0`, `0x7A420`).
    - Weights: `exp2(−r²·2/81 − ((z_r − z_c)·cb0[4].w)²)`, with no slope term.
    - Point taps at r = 1..4; bilinear taps at 5.5 and 7.5, weighted as r = 5 and 7.
    - The radius-2 and radius-4 blurs carry `2/9` and `2/25`.
- **`InputData`** (r8):
  - The f32 at +0x14 reads 1.0.
  - The first 0xC0 bytes hold pointers and handles, not an inline projection matrix.

### Inferences and alternatives

- **Field names** follow the clamps and the public HBAO+ header order. They are not taken from
  symbols:
  - +0x08 DetailAO, +0x0C CoarseAO, +0x10 PowerExponent;
  - +0x14..+0x1C DepthThreshold {Enable, MaxViewDepth, Sharpness};
  - +0x20..+0x28 Blur {Enable, Radius, Sharpness};
  - +0x2C..+0x38 Blur.SharpnessProfile {Enable, ForegroundSharpnessScale, ForegroundViewDepth,
    BackgroundViewDepth}.
- **Blur radius:** enum 2 is radius 8.
  - The DLL holds exactly three blur families (2, 4 and 8, from their falloff literals).
  - The header order puts 8 last.
  - An alternative order is not excluded.
- **cb[12] / cb[13]:** cb[12] is the detail weight, because the normal pass tests it before
  computing the 4-tap term, and CoarseAO multiplies `normal.w` by it.
- **`InputData` +0x14 = MetersToViewSpaceUnits 1.0:** the port uses 1 with native units as
  metres.
- **a[13] and a[8]** in the radius formula are taken to be tan(fovy/2) and the viewport height,
  as in For Honor's SDK.
- **Lineage:** this DLL is an older HBAO+ revision than For Honor's 3.x. The CoarseAO / blur /
  reinterleave code is shared; the detail-AO normal pass and the radius-8 blur were later
  replaced by SmallScale / LargeScale AO and the slope blur.

### Unknowns / incomplete coverage

- The per-layer jitter table (CoarseAO cb2) was not read. The port uses a per-pixel hash.
- How ACU composites the AO result into lighting (which terms it darkens) is not traced. Bevy
  applies it to diffuse ambient through `diffuse_occlusion`.
- Only Paris at the pinned save was captured; the parameters may differ per world or interior.
- The ShadowLib path for max shadows is separate and not covered.

### Independent comparison / falsification test

- For Honor's separately extracted shader dumps match 14 ACU shaders opcode for opcode. A wrong
  DXBC extraction or disassembly would not produce that.
- The live parameters satisfy the DLL's clamps (all values inside their ranges).
- Port check: Arno, `--lighting paris-day`, at 1280x720:

  | Mode | Pixels darker than `--ao off` by more than 2 levels | Lighter | Max |
  | --- | --- | --- | --- |
  | `--ao hbao` | 29,316 | 0 | 28.3 levels |
  | Bevy GTAO | 25,545 | 0 | 31 levels |

  - HBAO+ concentrates on the inner coat, the boot contact and the box bases; sky and open
    ground are unchanged.
  - MSAA is off in all three modes, so the diff is AO only.
  - Captures: `outputs/lighting/ao/arno-paris-day-ao-off-hbao-bevy.png`,
    `arno-paris-day-ao-hbao-diff.png` (×4), `arno-legs-off-vs-hbao.png`.
  - Scene reports: `local/captures/ao/scene-*.json`.

### Reproduction steps

1. `python tools/board.py lock acquire game-process --name <session>`, then
   `python tools/native_probe/shader_quality.py set high`.
2. `powershell tools/native_probe/build_shader_probe.ps1`.
3. Run `capture_run.py` with the save pin, `--until world`, an explicit `--fps-cap` ≤ 300 and
   `--extra-dll local/native-probe/acu_hbao_probe.dll`.
4. Read `acu_hbao_probe-<pid>.json` and parse `params` with `acu_assets::hbao::HbaoParams::parse`.
5. Close with `capture_run.py --close`, release the save pin, restore `shader_quality.py set low`
   and release the lock.
6. `python tools/native_probe/hbao_shader_compare.py`.

### Implementation implications and finding-linked constants

- `acu_assets::hbao`:
  - `HbaoParams::ACU_MAX` (the table above) and `HbaoParams::parse`;
  - `HbaoParams::constants`, following `0x1800085A0`;
  - `blur_radius` (enum 0, 1, 2 → 2, 4, 8);
  - `DIRECTIONS` 8, `STEPS` 4, `DETAIL_TAP` 3, `DETAIL_WEIGHT` 8, `NORMALISE` 1/16.
- `acu_bevy::hbao` + `shaders/hbao.wgsl`:
  - a reviewed import of For Honor's Bevy pass with ACU's detail term, single coarse amount and
    radius-8 blur;
  - default on (`--ao hbao`), with `--ao bevy` and `--ao off` as controls;
  - evaluated at full resolution instead of 16 deinterleaved layers (UNVERIFIED equivalence).
