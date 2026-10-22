# 122 - Map scene path (cell -> placed meshes in Bevy)

Issue: #122 (`type:slice`, `area:world-data`). Branch: `acu-122-map-scene`.

## What works

`acu_bevy --map-cell <entry|file id>` spawns a world cell's contents and captures a frame:

- opens `DataPC_ACU_Paris.forge` (cells, entities, selectors, world meshes) and
  `DataPC_ACU_Paris_assets.forge` (fallback for mesh entries);
- reads the `GridCellDataBlock`, walks its children;
- resolves child -> `Entity` through `local/paris-entities.bin`, reads the affine matrix
  at offset 14 (ACU-RE-0035) and bakes it into the vertices, so the existing spawn,
  camera and capture paths work unchanged;
- resolves `Entity` -> `LODSelector` -> `415d9568` world mesh through
  `local/paris-selectors.bin` and `local/paris-world-meshes.bin`;
- caps at 400 meshes, reports placed / skipped counts, and never guesses.

Measured on cell 05201 (116 children): **75 meshes placed, 0 entities without a matrix,
6 without a resolvable mesh**, 9 s to build and capture. Baked bounds
`11.3 x 18.9 x 14.0 m` around `(544.7 .. 556.0, -747.9 .. -729.0, 4.7 .. 18.7)`, i.e. a
street-sized cluster at the entity's real Paris coordinates.

## What is wrong

**The entity -> selector resolution is not trustworthy.** 73 of the 75 placed pieces are
the same mesh (`FRV_BV_TanneryFence_03_LOD0`), because the resolver scans the entity
payload for any 8-byte window that maps to a selector and takes the first match. Shared
ids inside the payload (materials, shapes, sub-objects) collide with unrelated selectors.
A name-prefix filter was tried and is also wrong: composite entities legitimately
reference a parent object with a different name (`FRV_CanalKit_RiverKit_26_008` ->
`FRV_Street_Row_BievreRiver_26`), so it rejected everything.

Consequently the captured frame shows one correct placed tree and nothing else useful.
The scene path is real; the reference it follows is not.

## Next, precisely

Re-derive the entity's reference structure by RE rather than scanning:

1. Compare several entity payloads with known selectors and find the record that holds
   the selector id. The sample shows the id at offsets 100 and 122 (22-byte stride), and
   material ids at 219/229, 260/270, 301/311 (10-byte stride) - so the payload appears to
   be a list of typed records, not a flat id list.
2. Decode that record table (tag, id, maybe sub-index) and use it instead of the scan.
3. Then re-run `--map-cell`; every piece should be a distinct building, and 116 children
   should resolve to dozens of distinct meshes.

## Files

- `acu-rs/crates/acu_bevy/src/options.rs`: `--map-cell`, `--map-paris`, `--map-assets`;
  map mode disables the Arno rig.
- `acu-rs/crates/acu_bevy/src/load.rs`: `load_map`, index reader, cell/entity parsers,
  transform baking.
- `tools/build_resource_index.py`: census -> compact `id -> entry` binary per type.
- `tools/bc1_tile.py`, `tools/paris_sheet.py`: evidence helpers (map tile decode, capture
  sheet).

## Commands

```powershell
python tools/build_resource_index.py 0984415e local/paris-entities.bin
python tools/build_resource_index.py 51dc6b80 local/paris-selectors.bin
python tools/build_resource_index.py 415d9568 local/paris-world-meshes.bin
$env:WGPU_BACKEND='vulkan'
.\acu-rs\target\debug\acu_bevy.exe --game-dir "<install>" --map-cell 5311 --capture ..\local\map-cell.png
```
