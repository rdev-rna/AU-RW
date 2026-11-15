# ACU-RE-0071 — How avatar gear colours are set

Date: 2026-11-15 / Claude Code session / user request "decode how colours are set and use the
default assassin blue". Related: [ACU-RE-0070](ACU-RE-0070-arnos-tailored-visual-pieces.md),
[ACU-RE-0016](ACU-RE-0016-build-table-rig-references.md), [ACU-RE-0013](ACU-RE-0013-material-texture-chain.md).
State: partially established. The table, the parameter-to-register map and the shader albedo math
are observed. The default-row choice and colour-item → row order are inferences. Lighting is
synthetic.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`. No
  native code was run or read for this finding.
- Colour table `CN_P_Avatars_ColorSets` `0000001e7c52e192`, `DataPC_ACU_LGS_BelleEpoque.forge`
  entry 334 resource 127. Payload is 11,013 bytes; it decodes to the exact end with the new
  `acu_assets::build_table`. The Python diagnostic and the Rust decoder agree.
- Material template `CN_Cloth_Tile_ColorMasked_AlphaTest` `000000358c3a2f08`, `DataPC.forge`
  entry 110 ordinal 14088, payload 670,867 bytes. It embeds 96 DXBC programs (68 vs_5_0,
  28 ps_5_0), disassembled with the system `d3dcompiler_47.dll` `D3DDisassemble`.
  Disassembly is kept in ignored `local/tailored/shaders/`.
- Gear materials (all on that template, `match_mask` byte 9 = 1): Upperbody `00000011f1ca8cf2`,
  Hood `00000011f1ca8d08`, Pants `00000011f1ca8d17`, Belts `000000131c3bdd7b`, ThrowBlade
  `00000013c1e4eb48`.
- Layout leads, all re-checked by exact-end decoding:
  - AnvilToolkit.dll SHA-256 `ba8db223…3a96` (a different build from ACU-RE-0016's), decompiled
    locally with ilspycmd into ignored `local/anviltk-20261115/`. It supplied the PropertyPath,
    PropertyPathNode, MaterialPropertyPathNodeSolver (Unity branch adds
    `MatchUsingMaterialOverrider`), Mask16, Handle and the DynamicProperty kind registry
    (`0x000a0000` Float, `0x000d0000` Vector4).
  - The mod "Gear Visual Customization 365" 1.2.5 `VisualCustomization.ini` (text only) names the
    colour fields Hue1–4 / Saturation1–4 / Brightness1–4 / VelvetShine. Its DLL was not inspected.
  - No third-party code was copied.

## Result

**Colour table (observed).** It has 8 columns and 28 rows. Every column's PropertyPath starts with
a `MaterialPropertyPathNodeSolver` with mode 1 and a match mask where only byte 9 = 1. That equals
the gear materials' `match_mask`, so the solver selects those materials. The path then names the
material property to write:

| Column | Path target | Value | Mod name |
|---|---|---|---|
| 1 | `b000af3e` | Vector4 | Hue1/Sat1/Bright1 |
| 2 | `dddd4bd5` | Vector4 | Hue2/… |
| 3 | `adb7bf5a` | Vector4 | Hue3/… |
| 10 | `34beeee0` | Vector4 | Hue4/… |
| 9 | `84f72d37` | Float | VelvetShine |
| 4, 6, 7 | `25ec2112` → the three above | Vector4 | unresolved sub-object; not on these materials |

Row 0 is a sentinel (10, 10, 1). Rows 1–27 match the 27 colour items present in the
AvatarGearManager (`ACU_ColorSet_01`–`_27`, internal names "Color 1"…). Row 1, tag `84f0c283`,
is the only weight-1 row. **Inference:** row 1 is the default colour, item "Color 1"; this build
uses it as the default assassin blue. Row 1 values: Hue1 (1.02, 0.5, 1.0); Hue2 (0.62, 1.2, 1.0);
Hue3 (0, 0, 1.1); Hue4 (1.43, 0.74, 0.36); VelvetShine 0.05.

**Parameter → shader register (observed).** The template carries 36 records of class `92d220dd`,
each holding (parameter hash, id, register, kind). Two of them independently confirm the map:
register 23 holds `8f6d6273` = (9, 18), which the shader uses as a detail-UV scale, and register 28
holds the UV-transform object, which the shader reads as two rows.

| `cb3` register | Parameter | Shader use |
|---|---|---|
| 19 | `b000af3e` | HSB when mask R ≥ 0.9 |
| 20 | `8ca98c4d` | HSB base, R ≤ 0.1 (the table does not write it) |
| 21 | `dddd4bd5` | HSB, 0.6 ≤ R < 0.9 |
| 22 | `adb7bf5a` | HSB, 0.4 < R < 0.6 |
| 25 | `34beeee0` | HSB, 0.1 < R ≤ 0.4 |
| 24 / 26 | `e9367dfe` / `227c0a72` | RGB tint pair, mixed by the t35 map (LIB_White here, so a no-op) |
| 10 | `fc594e9e` | desaturation k |
| 5 | `84f72d37` | velvet term in the specular output only |

Textures: t32 = diffuse (map 0), t33 = normal, t34 = mask (map 7, **linear**, gamma 0), t35 = tint.

**Albedo math (observed, `s68` disassembly; identical in all 20 colour-writing ps variants).**
1. Choose the HSB vector by mask R zone (the floor tests above).
2. With hue *h* (turns, × 6.28), saturation *s* and brightness *v*:
   u = (1−k)·s·v·cos, w = (1−k)·s·v·sin. Then apply the YIQ hue-rotation matrix to the linear
   diffuse. (1, 1, 1) is the identity.
3. Scale by (1.217638 − metal − cloth), where metal = sat((0.247059 − G)·4.047619) and
   cloth = sat((G − 0.752941)·4.047619) from mask G.
4. Multiply by lerp(1, tint, cloth), and by a distance-dissolve factor that is about 1 away from
   its centre (not modelled).

The hue value is therefore a **rotation of the authored diffuse**, not an absolute hue.

**Implementation.**
- `acu_assets::build_table` decodes the table.
- `acu_assets::bc1` decodes BC1 textures.
- `acu_assets::color_mask` implements the albedo math. Its tests check identity, grey
  invariance, zone thresholds, the G bands and that the prepared path matches the reference.
- `acu_bevy` applies the selected row (`--color-set`, default 1) to template materials whose
  match mask intersects the solver mask. It bakes an RGBA8 sRGB texture with a mip chain, at the
  larger of the diffuse and mask resolutions.
- Head and hair materials are on other templates and stay untouched.

**Visual check (synthetic arena).**
- Row 1: slate-blue coat and hood, red sash/cravat accents, grey breeches, leather unchanged.
- Row 3: pale coat, khaki breeches, blue cravat. Distinct from row 1, which confirms the row
  drives the colour.
- Not compared against an in-game capture. On 2026-11-15 the user, who knows the game, looked at
  the row 1 renders and confirmed they match the in-game default colours. That is a visual
  judgement, not a pixel comparison.

**Unknowns.**
- Solver match semantics (mode 1, mask intersection assumed).
- Sub-object `25ec2112`.
- The velvet/specular path.
- BC1 decode rounding versus GPU.
- Whether ACU presents row 1 as "assassin blue" in the colour menu.
- The colour item → row order.
- Other templates (non-AlphaTest ColorMasked) are not decoded.

## Reproduction

```powershell
$env:ACU_GAME_DIR = 'D:/Games/Assassin''s Creed Unity'
cargo test --locked -p acu_assets --test build_table -- --ignored   # from acu-rs/
cargo test --locked -p acu_assets --lib color_mask
powershell -File tools/run_arno.ps1 -Capture local/captures/tailored/cs1.png   # default row 1
```

The scene report's `color_set` and per-material `color_mask` record the row, the applied
targets, the parameters before and after, the mask and tint maps and the baked size.
