# 120 - World geometry chain and first Paris render

Issue: #120 (`type:re`, `area:world-data`). Branch: `acu-120-world-geometry`.
Finding: `docs/findings/ACU-RE-0036-world-geometry-render.md`.

## Result

`415d9568` (the largest unnamed type: 116,526 resources, 1,681 MB) is the world LOD mesh
object. `acu-mesh` parses it unmodified (25,260 clustered vertices, 6,528 indices) and
`acu_bevy --piece` renders it with four materials and their diffuse textures.

Chain confirmed: `cell -> Entity (transform + selector id) -> LODSelector -> 415d9568 ->
Materials/TextureSets`.

Evidence: `local/paris-lod1.png` shows a street-row elevation - balcony railings,
sandstone paving, masonry - from `FRV_Street_Row_BievreRiver_26_LOD1` (entry 1992,
`DataPC_ACU_Paris_assets.forge`), 1,263 verts / 1,088 tris.

## Explicitly not claimed

One object, at the origin, LOD1, synthetic lighting, no GI, no shader parity, no
placement transform applied. A cell is 116 children and this is a single mesh.

## Tools added

- `tools/entity_references.py` - membership test of 8-byte windows against the archive id
  set; how the entity's selector and material references were found.
- `tools/resolve_entity_chain.py` - entity -> selector resolution from the census.
- `tools/resolve_lod_chain.py` - selector neighbourhood probe into `_assets` without a
  4.9 GB scan.

## Commands

```powershell
.\acu-rs\target\debug\acu-mesh.exe "<install>\DataPC_ACU_Paris_assets.forge" 1992
$env:WGPU_BACKEND='vulkan'
.\acu-rs\target\debug\acu_bevy.exe --game-dir "<install>" --piece "DataPC_ACU_Paris_assets.forge:1992:0" --capture local\paris-lod1.png --report local\paris-lod1.json
```
