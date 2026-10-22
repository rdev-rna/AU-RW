# ACU-RE-0036 - World geometry chain and the first Paris mesh rendered

Date: 2026-10-22. Related: #120 (reserved here), ACU-RE-0035, #117, #4 world-data.
State: observed. One building LOD renders from original data; a cell does not yet.

## Result

`415d9568` - the largest unnamed type in the Paris census (116,526 resources,
1,681 MB, 42 % of all payload bytes) - is the **world LOD mesh object**:

- Name pattern: `FRV_Street_Row_BievreRiver_26_LOD1`.
- `acu-mesh` parses it without modification: `typed_status: "parsed"`,
  `clustered_vertices: 25,260`, `clustered_indices: 6,528`.
- `acu_bevy --piece DataPC_ACU_Paris_assets.forge:1992:0` renders it:
  1,263 vertices, 1,088 triangles, four materials resolved with diffuse textures.

The full chain is therefore:

```text
GridCellDataBlock  (cell -> child file ids)
  -> Entity         (affine matrix + LODSelector id + material ids)
  -> LODSelector    (51dc6b80; references LOD objects whose ids sit near its own)
  -> 415d9568       (clustered world mesh, decoded by the existing pipeline)
  -> Materials + TextureSets (85c817c3 / d70e6670, diffuse resolves today)
```

## Evidence

- Entry 1992 in `DataPC_ACU_Paris_assets.forge` contains eight resources:
  `FRV_Street_Row_BievreRiver_26_LOD1` (`415d9568`) plus three `Material` and four
  `TextureSet` records.
- `acu-mesh` report for that entry: `parsed`, clustered 25,260 vertices / 6,528 indices,
  zero bones, zero submeshes.
- Viewer report: `piece FRV_Street_Row_BievreRiver_26_LOD1 ... qf=8 src=clustered/clustered
  verts=1263 tris=1088`, and four `material <id> diffuse=Some(<id>)` lines.
- Capture `local/paris-lod1.png` (1280x720, 553,077 bytes): balcony railings, a
  sandstone-paved section and masonry blocks, i.e. a recognisable street-row elevation,
  not a placeholder.
- Entity `FRV_CanalKit_RiverKit_26_008` at (553.877, -747.872, 4.689) references the
  selector for that row, which is how the object was reached.

## Limits - important

- This is **one mesh object at the origin**. The entity's affine transform is *not*
  applied by the viewer; the piece path places at local origin. No claim of correct
  placement is made.
- LOD**1**, not LOD0. 1,263 triangles is a coarse level.
- Lighting is the viewer's synthetic daylight (`shading_deltas.md:13-14`); no GI, no
  native shader, no lightmap. The visible textures are diffuse only.
- One object is not a cell. A cell is 116 children and a street is many entities.
- The renderer's `--piece` path was written for character parts and accepts this only
  because the mesh decoder is type-driven; a real world scene still needs a scene path
  that consumes cells and applies transforms.

## Unresolved

- Where LOD0 lives for this row. The selector references ids `...da`, `...db`, `...df`;
  only `...df` is an entry in `_assets` (the LOD1 object). The others are not entries in
  `_assets` or `_Paris`, so either they are in a patch archive or the reference is not a
  direct entry id.
- The `415d9568` payload header beyond `version/self/type` (35,259 bytes for LOD1) is not
  decoded; the mesh decoder reaches the clustered buffers through the compiled-mesh
  structure, which is enough to render but not yet to explain every field.

## Reproduction

```powershell
.\acu-rs\target\debug\acu-mesh.exe "<install>\DataPC_ACU_Paris_assets.forge" 1992
$env:WGPU_BACKEND='vulkan'
.\acu-rs\target\debug\acu_bevy.exe --game-dir "<install>" `
  --piece "DataPC_ACU_Paris_assets.forge:1992:0" `
  --capture local\paris-lod1.png --report local\paris-lod1.json
```

## Next

Consume a cell: walk `GridCellDataBlock` children, read each `Entity` matrix, resolve its
LOD object, and spawn the set at their transforms. That needs a scene path in `acu_bevy`
that the character-piece loader cannot provide, plus the `_assets` census for bulk
resolution.
