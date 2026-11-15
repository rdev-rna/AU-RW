# ACU-RE-0084 — Character material templates and the deferred G-buffer contract

Date: 2026-11-15 / Claude Code side session (lighting port, follow-up "port the actual character
shader, materials for all outfits, sun strength"). Related: [ACU-RE-0071](ACU-RE-0071-avatar-color-sets.md)
(colour sets, `color_mask`), [ACU-RE-0073](ACU-RE-0073-baked-gi-and-world-ambiance.md) (GI, deferred
lighting shader).
State: partially established. Shader bytecode located and disassembled; the G-buffer read side is
observed; which pixel-shader output feeds which deferred slot is not yet established.

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

## Inferences (labelled)

- Output 2 feeds `t_SpecularReflectance`, output 4 `t_Emissive`, output 3 `t_MiscProps`: the
  value ranges match (specular colour; zero emissive apart from the fade term; w = 0 = not the
  special path). Not proven: the cloth PS writes coverage into output 1's w while the deferred
  pass reads gloss from `t_Normals.w`, so either the render-target order differs or blend/write
  masks keep gloss from another pass. Resolving this needs the renderer's RTV binding or a live
  capture of the bound render targets.

## Unknowns

- Render-target order, formats and blend state of the G-buffer pass.
- The `CN_Skin_Wrinkle` and `CN_Hair` G-buffer shaders (not yet read; hair probably uses the
  special `t_MiscProps.w = 255` path).
- Runtime constants: `m_DirectLight` / `m_SunLight` colour and direction (sun strength),
  `m_GlobalLightingScale`, `g_SkySHConstants`. The game was in use by another session at the time.

## Reproduction

```powershell
cd acu-rs
./target/debug/acu-resource.exe "D:/Games/Assassin's Creed Unity/DataPC.forge" --entry 110 --fid 000000358c3a2f08 --out ../local/lighting/tmpl/cloth_colormasked.bin
cd ..
python local/lighting/dxbc_file.py local/lighting/tmpl/cloth_colormasked.bin local/lighting/tmpl/cloth
python local/lighting/dxbc.py local/lighting/fwd g_WorldGIIndirection   # deferred lighting shaders
```
