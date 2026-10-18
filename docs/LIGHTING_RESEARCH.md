# Lighting target and research

Primary developer reference and attribution boundaries:
[rendering sources](RENDERING_SOURCES.md). The user designates its ACU architecture
as ground truth. Its culling cubemap masks must not be mistaken for lighting data.

The user-selected default is **Day, Paris** (2026-10-19). This is the target
environment for the eventual native lighting implementation. Current synthetic
directional/ambient lighting does not reproduce that environment and must remain
labelled synthetic until its data and shader path are resolved.

Reference requested by the user: https://youtu.be/4babdW9O6y4?t=759 (12:39 onward).
Browser attempts failed with cache misses. The user then identified
[claude-watch](https://github.com/taoufik123-collab/claude-watch). Its skill and
scripts were read, fetched to ignored `local/claude-watch/`, pinned at commit
`7711231e4c47e5d4e06bcf5326c4abf5b70ab4a9`, and run with Whisper disabled.
All 12 frames from 12:39-16:39 and native English captions were reviewed.
Source title: *Assassin's Creed Unity PS5 Gameplay - NEW 60fps Update*, GameRiot,
27:24. Platform/update claims are uploader metadata, not verified here.

Reproduce from this project:

```powershell
python local/claude-watch/scripts/watch.py 'https://www.youtube.com/watch?v=4babdW9O6y4' --start 12:39 --end 16:39 --max-frames 12 --resolution 1024 --no-whisper --no-hook-microscope --out-dir local/watch-acu-lighting-20261019 --intent 'Analyze AC Unity lighting; Day Paris reconstruction target'
```

Local `report.md`, frames, video and captions are under
`local/watch-acu-lighting-20261019/`; generated log is adjacent. No game data or
audio was uploaded, no external account was connected, and no vault was changed.
This project-only report follows the authorized project scope. Retain it for
follow-up comparisons. Twelve frames are sparse observations, not continuous
video analysis. Script timestamps are nominal at 20-second intervals; exact
decoded-frame PTS was not audited.

## Visual observations from the requested section

- Approximately 12:39-13:39 (frames1-4): warm stone walls/arches, window-shaped
  bright patches on carpet/floor, broad fill in shadow and dark foreground cloth.
- Approximately 14:19 (frame6): dark cool interior beside a very bright doorway.
- Approximately 14:39-15:39 (frames7/9/10): warmer sunlit masonry, cooler courtyard
  shade, intricate ornamental shadows and darkening at geometric contacts.
- Approximately 15:59-16:19 (frames11/12): broad facial shading and muted stone/
  cloth response; material-specific response must be investigated separately.
- Frame5 is a menu and frame8 has a tutorial overlay; exclude these from matched
  image comparisons. Gameplay/cutscene camera changes also prevent pixel parity.

These are appearance observations. Broad colored fill could combine authored
albedo, grading and indirect lighting; dark contacts do not identify an AO method.
The captions explain no GI/probe implementation. Neither spatial cubemap
selection nor temporal exposure behavior can be established from this sample.
The sequence depicts young Arno in a palace introduction; exact world identity
has not been verified through game data. Do not adopt it as the Paris environment
preset without independent evidence.

The user's baked-GI lead now has primary historical support: Ubisoft's
SIGGRAPH2025 retrospective pp5-6 describes Unity's open-world irradiance volumes
and four baked time-of-day keyframes with0.5m probe spacing (RENDERING_SOURCES.md).
Recover the installed variant rather than treating these as decoded fields.
Location/time-of-day lighting cubemap selection is still unresolved. Do not equate a cube
texture with diffuse GI: reflection/environment captures, sky illumination and
irradiance representations can have different roles and selection rules.

Research acceptance:

- Trace Paris daytime environment settings to stable archive/resource IDs and
  hashes, including any variants, transition/blend rules and spatial selection.
- Identify baked diffuse/lightmap/irradiance data independently of specular
  environment cubemaps; establish encoding, units, orientation, mip selection,
  interpolation and interaction with dynamic/skinned characters.
- Recover direct sun/sky, shadows, ambient occlusion, fog, exposure, tone mapping
  and grading, and determine which are authored/baked versus evaluated at runtime.
- Compare against read-only ForHonorRustRewrite lighting/shading research under
  E:/ReverseEngineering/Projects/, treating it as another game, not ACU evidence.
- Verify Day, Paris with matched original/rewrite camera, pose and conditions;
  record differences in shading_deltas.md. Keep the grey arena synthetic even
  when using recovered Paris lighting.

No native lighting asset selection or visual change has been implemented here.

## Sibling Anvil reference reviewed

Read-only references under
`E:/ReverseEngineering/Projects/ForHonorRustRewrite/docs/re/`:
`LIGHTING_PRESETS.md`, `EK_SHADER_PROGRAMS.md`, `EKSHADERS_SDB34.md`.
They describe that project's preset-to-cubemap chain, unresolved GI stream,
shader-database decoding and character/lighting shader analysis. Reuse the
investigation method: preserve resource identities, distinguish irradiance and
specular inputs, recover shader bindings and verify HDR orientation/exposure.
Do not transfer FH IDs, codec dispatch, face orientation, template math, probe
semantics or its explicitly unverified brightness/exposure constants to ACU.
That project's last-cubemap-mip irradiance fallback is a stand-in, not evidence
for Unity's native diffuse GI. Exact ACU shader/container compatibility is open.
