# ACU-RE-0084 — Character material templates and the deferred G-buffer contract

Date: 2026-11-15 / Claude Code side session (lighting port, follow-up "port the actual character
shader, materials for all outfits, sun strength"). Related: [ACU-RE-0071](ACU-RE-0071-avatar-color-sets.md)
(colour sets, `color_mask`), [ACU-RE-0073](ACU-RE-0073-baked-gi-and-world-ambiance.md) (GI, deferred
lighting shader).
State: partially established. Shader bytecode located and disassembled; the G-buffer read side is
observed; the output -> slot order is established by consistent writes across three templates.

## Provenance

| Item | Identity |
| --- | --- |
| Executable | ACU.exe 748abdc6...7e40, base 0x140000000, mapped image `main-image.mapped.bin` (runtime view) |
| Deferred lighting PS | DXBC at VA `0x143211F50` (19,176 bytes), ps_5_0; 41 sibling permutations bind the same resources |
| Cloth template | `CN_Cloth_Tile_ColorMasked_AlphaTest` `000000358c3a2f08`, type `bcfb3c7a` (label unknown), `DataPC.forge` entry 110; payload 670,867 bytes, SHA-256 `03b613fc0ab4f901fea90f9d864ea4f16058aba27173e936106f8eaff4055327` |
| Skin template | `CN_Skin_Wrinkle` `00000006dc67dd6d`, `DataPC_ACU_LGS_BelleEpoque.forge` entry 333 (also patch_03 entry 54) |
| Hair template | `CN_Hair` `00000011565b044b`, `DataPC.forge` entry 968 ordinal 332 (also dlc_10 entry 726) |
| Tools | `d3dcompiler_47.dll` `D3DDisassemble`; `local/lighting/dxbc_file.py` (scan for `DXBC`, size at `+24`) |

The asset-index type labels above are `unknown` confidence; template identity comes from the
materials' own `template` reference (`acu-mat`, `material::Material::template`).

## Observations

- Arno's default outfit (scene report, 7 materials) uses three templates: five cloth materials on
  `CN_Cloth_Tile_ColorMasked_AlphaTest`, the head on `CN_Skin_Wrinkle`, the hair on `CN_Hair`.
- The cloth template payload holds 96 DXBC blobs: 68 vs_5_0 and 28 ps_5_0. The pixel shaders fall
  into four groups by signature: 16 with 5 render-target outputs and 9-10 textures (G-buffer
  pass), 4 with 1 output and 25 textures (a forward-lit pass), 4 with 3 outputs, 4 with none
  (depth/shadow). The payload also carries the template's node and parameter names
  (`CN_ThreeNormalBlending`, `CombineGloss`, `CN_ColorRandFiveMasking`, `CN_ThresholdFiveMask`,
  `CN_TranslucencyOcclusion`, `CN_Wetness`, `CharacterEffectMultiplier`, `Layer0_Diffuse`,
  `Layer0_Normal`, `Layer0_Mask1`, `Layer1DetailVelvet`, ...).
- Cloth G-buffer PS (blob at payload offset `0x4C391`, textures t17, t32-t39), behaviour:
  - a distance/noise fade (t37 noise at `2.5 * xy`), alpha test against `cb4[10].x` with
    `t35.z * diffuse.a * fade`;
  - albedo: t32 diffuse recoloured by the t34 mask zones and t35 tint (the ACU-RE-0071 colour-set
    math: zone thresholds 0.247 / 0.753, colours `cb3[19..22]`, `cb3[24..26]`), with an HSV/YIQ hue
    rotation; output 0 = albedo * |fade|, w = coverage;
  - normal: t33 base normal (RG, z reconstructed) combined with three detail normals t36, t38,
    t39 (UV scale `cb3[23]`) weighted by mask terms and `cb3[1]`, `cb3[3]`, `cb3[4]`, `cb3[11]`;
    tangent frame from interpolants 2-4; output 1 = normal * 0.5 + 0.5, w = coverage;
  - output 2 = a specular colour (hue from the albedo, saturation from masks) times a mask
    intensity, w = 0;
  - output 3: x = a mask-derived gloss minus a wetness/time term (`cb1[5].z * cb1[418].x +
    cb3[6].x`), y / z = translucency-like terms (view-dependent `cb2[1]` dot), w = 0;
  - output 4 = fade / `t17[0,0]` (a 1x1 texture, probably exposure), w = 0.
- Deferred lighting PS `0x143211F50` resource names (RDEF): `t_Albedo` t0, `t_Normals` t1,
  `t_DepthSurface` t2, `t_SpecularReflectance` t3, `t_SSAO` t4, `t_Emissive` t12,
  `t_g_SkyLightingCubeTexture` t13, `t_MiscProps` t15, `t_GlobalEnvMap` t25, `t_BRDFLUT` t55.
  Its reads:
  - `t_Normals.xyz * 2 - 1` = world normal; `t_Normals.w` = gloss g, specular power `2^(19 g)`,
    and `1 / (0.5 * 2^(19 g) + 1)` feeds a Beckmann-style lobe (`exp(-(1-NdotH^2)/(NdotH^2 m))`);
  - `t_MiscProps.w * 255 == 255` selects a special path (albedo `sqrt` when `t_SpecularReflectance.w
    > 1e-4`, a second normal from `t_MiscProps.xyz`, a second gloss from `t_SpecularReflectance.x`,
    reflectance `pow(t_Albedo.w, 2.2)`); otherwise reflectance = `t_SpecularReflectance.xyz` and
    emissive = `t_Emissive`;
  - sun: `cb5[3]` direction, `cb5[4]` colour (`DeferredLightingConsts.m_DirectLight`), times the
    cascaded shadow (t8) and cloud shadow (t54); diffuse wrap via `t_BRDFLUT` indexed by
    `NdotL * 0.5 + 0.5` and `t_MiscProps.y`.

- Output order across templates (smallest 5-output PS of each): skin `CN_Skin_Wrinkle` writes
  output 2 = (0.04, 0.04, 0.04) (dielectric F0) and output 3 = (0.5, 0.3, 0, 0); hair `CN_Hair`
  writes output 3 = (second normal * 0.5 + 0.5, 1.0) and output 2.x = 2 * gloss; all three write
  the same value to output 0.w and output 1.w. Against the deferred reads this gives: output 0 =
  `t_Albedo` (w = gloss copy, read as reflectance on the special path), output 1 = `t_Normals`
  (w = gloss), output 2 = `t_SpecularReflectance`, output 3 = `t_MiscProps` (w = 1 selects the
  special path: hair), output 4 = `t_Emissive`.
- Stripped cloth variant: gloss = tint-map (t35) w * diffuse alpha * fade; the same product drives
  the alpha test.
- Cloth template parameter records: 36 records of class `92d220dd`, 42 bytes each, from payload
  offset `0x4169` (texture records) and `0x42cb` (constants): class u32, parameter hash u32, id u32,
  register u32, kind u32 (0 texture, 1 constant, 3 object), type u32 (2 texture, 0 float, 1 vec4,
  5 object), u16, then a 16-byte reference. Registers 160..167 are textures t32..t39; the rest are
  `cb3` registers. Values below are Arno's upper body (`00000011f1ca8cf2`, BelleEpoque entry 370,
  `acu-mat`; the other four cloth materials were not all checked):

  | Register | Hash | Arno | Role in the G-buffer PS |
  | --- | --- | --- | --- |
  | t32 / t33 / t34 / t35 | `1371c615` / `4c671014` / `9ae6abcd` / `a64e88a4` | | diffuse / normal (A = gloss) / mask / tint |
  | t36 / t37 / t38 / t39 | `14c6a86b` / `4ddb34ee` / `e3069623` / `d7e50c4a` | | detail (metal) / dissolve noise / detail (cloth, mid) / detail (base) |
  | 1 / 3 / 4 / 11 | `4cba78d8` / `d8b0b4e3` / `50bf8584` / `ccc4a04b` | 4 / 4 / 4 / 1 | detail weights: base / mid / cloth / metal |
  | 2 / 17 | `c5416a6d` / `e640aad7` | 1 / 1 | specular hue scale: metal / cloth + mid |
  | 5 / 15 | `84f72d37` / `ebc1dd6d` | 0.0423 / 0.02 | specular intensity: cloth / mid |
  | 12 / 14 | `c9c73faf` / `d3cd1895` | 1 / 1 | specular saturation: cloth / metal |
  | 6 | `99f31a87` | 0 | wetness bias (added to a global wetness term) |
  | 7 / 8 / 9 / 27 | `802d4252` / `05e323dc` / `4a4597f8` / `3ca1e27e` | 0.001 / 0.001 / 0 / 0 | dissolve radius / noise scale / mix / centre (inactive: fade 1) |
  | 13 / 16 | `6a67feb5` / `ca0eaa2f` | 0.35 / 0.15 | back-light translucency |
  | 18 | `5f7a6196` | 0.55 | mid-band threshold on mask G |
  | 10, 19-26, 28 | see ACU-RE-0071 | | albedo recolouring, detail UV scale (23) = (9, 18), UV transform (28) |

- The cloth G-buffer pixel shaders come in two forms. The full form (e.g. payload offset `0x40655`,
  6 of the 16) samples all four mask channels and three-channel detail maps; the stripped form
  (`0x4C391` and others) ignores mask B and the detail alpha and writes the coverage product as
  gloss. Arno's normal maps are BC7 (format 98, with alpha) and his masks carry B, so the full form
  is the one that fits his data (inference: the permutation selector was not observed).
- Full form, behaviour (all re-derived from `D3DDisassemble` output; t35 is sampled with swizzle
  `.xzwy` into `.yz`, so the tint mix is t35 **z** and the coverage factor t35 **w**; mask channels
  map directly):
  - band weights from mask G: metal = sat((0.247059 - G) * 4.047619), cloth = sat((G - 0.752941)
    * 4.047619), mid = (1 - cloth) * sat(G - cb3[18]) * 8, base = 1 - min(metal + mid + cloth, 1);
  - output 0.w = mask B (read by the deferred pass as `pow(x, 2.2)`, a specular occlusion);
  - detail maps at UV * cb3[23] decode to (normal - (0, 0, 1), alpha - 1) and accumulate onto
    (0, 0, 4, 4) with weights cloth * cb3[4] + mid * cb3[3] (t38), base * cb3[1] (t39),
    metal * cb3[11] (t36); the result's xyz joins the base normal by reoriented normal mapping
    (t = base + (0, 0, 1), u = detail * (-1, -1, 1), n = t * dot(t, u) / t.z - u), so a weight of 4
    applies a detail normal at full strength;
  - gloss: the base gloss (normal-map alpha) and the accumulated detail w combine as specular
    powers, `g = sat(log2(1 / (2^(-19 g_base) + 2^(-19 w))) / 19)`, then wetness moves it towards
    `1.5 * max(g, 0.1)`; output 1.w = g;
  - output 2 (specular reflectance) = intensity * colour, intensity = 0.04 base + cb3[5] cloth +
    cb3[15] mid + 0.9 metal, colour = a value-1 HSV colour with the albedo's hue (times
    cb3[17] (cloth + mid) + cb3[2] metal) and saturation (times cb3[14] metal + cb3[12] cloth);
  - output 3.x = sat(cloth + 0.5 mid + 0.1 base - wetness); 3.y / 3.z = back-light terms.
- Deferred lighting PS `0x143211F50`, the BRDF of the non-special path: GGX distribution with
  `alpha^2 = 1 / (0.5 * 2^(19 g) + 1)` (the same quantity listed above), Schlick Fresnel from
  `t_SpecularReflectance` using the view-half angle, a height-correlated-style Smith GGX visibility,
  and, when `t_MiscProps.x > 0`, a blend by `t_MiscProps.x` towards a velvet distribution of the
  form `(1 + 4 exp(-cot^2 / alpha^2) / sin^4) / (1 + 4 alpha^2)`. The specular result is multiplied
  by `pow(t_Albedo.w, 2.2)` and by SSAO; `t_MiscProps.z * sat(-NdotL)` adds back-light diffuse.


## Inferences (labelled)

- Earlier code read the t35 tint mix from channel R; by the swizzle it is channel Z. Arno's t35 is
  `LIB_White_DiffuseMap` (all channels equal), so the bake output does not change.

- The order above is inferred from value semantics (constants 0.04, w = 1 on hair, matching
  second-normal and second-gloss reads); the RTV binding itself was not observed.
- Gloss g -> GGX `alpha^2 = 2 / (2^(19 g) + 2)` is ACU's own (deferred PS); Bevy's perceptual
  roughness is `sqrt(alpha)`.

## Bevy implementation (implementation choices)

- `acu_assets::color_mask::Surface` reads the registers above from the material; `weights`,
  `Surface::specular`, `Surface::velvet`, `combine_gloss`, `gloss_to_perceptual_roughness` are CPU
  reference versions with unit tests.
- `acu_bevy` `character.wgsl` evaluates the full form per pixel for cloth-template materials:
  band weights, detail normals with reoriented normal mapping, combined gloss -> perceptual
  roughness, specular reflectance -> Bevy `reflectance = sqrt(F0 / 0.16)` with metallic 0, and
  specular occlusion `pow(mask B, 2.2)` applied both to F0 and to Bevy's `specular_occlusion`
  (ACU multiplies the whole specular term; Bevy's field only scales indirect light). The albedo
  recolouring stays a CPU bake (ACU-RE-0071).
- Visual check: `outputs/lighting/arno-paris-noon-cloth.png` (scene report
  `local/captures/lighting/scene-cloth.json`): all five cloth materials bind their mask and three
  BC7 detail maps (`LIB_cloth_Flecks` or `LIB_Cloth_Metal`, `LIB_cloth_cotton`, `LIB_Cloth_Leather`);
  fabric weave and folds, brighter metal fittings, no invalid pixels. Not compared with the game.
- Not ported: the velvet lobe blend, the back-light translucency, the dissolve, global wetness
  (only the material's `cb3[6]`), and ACU's diffuse wrap (`t_BRDFLUT`); Bevy's BRDF replaces ACU's.

## Unknowns

- Render-target formats and blend state of the G-buffer pass.
- The `CN_Skin_Wrinkle` and `CN_Hair` G-buffer shaders (not yet read; hair probably uses the
  special `t_MiscProps.w = 255` path).
- Runtime constants: sun and lighting scale captured live (see ACU-RE-0073); `g_SkySHConstants`
  not yet.

## Reproduction

```powershell
cd acu-rs
./target/debug/acu-resource.exe "D:/Games/Assassin's Creed Unity/DataPC.forge" --entry 110 --fid 000000358c3a2f08 --out ../local/lighting/tmpl/cloth_colormasked.bin
cd ..
python local/lighting/dxbc_file.py local/lighting/tmpl/cloth_colormasked.bin local/lighting/tmpl/cloth
python local/lighting/dxbc.py local/lighting/fwd g_WorldGIIndirection   # deferred lighting shaders
```
