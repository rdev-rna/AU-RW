# 137 - Whole-map pass: 2,168 cells, instanced

Issue: #137 (`type:slice`, `area:world-data`). Branch: `acu-137-whole-map`, stacked on
`acu-135-selector-lod-chain` (#136, ACU-RE-0037).

## Result

One scene holds the whole Paris world at LOD2, read from the installed archives:

```text
143,641 placements from 2,150/2,168 cells
2,128 unique meshes, 2,158 selectors, LODs 0/1/2
2.25 km x 2.0 km x 0.5 km, capture 1920x1080
```

Evidence: `local/paris-map-lod2.png`.

## What changed

- `--map-cells FILE` (JSON `[{ "entry_index": N }, ...]`) spawns many cells in one scene;
  `tools/world_cells.py` writes `local/paris-world-cells.json` (2,168 rows with
  `record_length > 1000`).
- A cell's entities come from the cell package's own `0984415e` resources, not from the
  child-id list. The child list is heterogeneous (nav, largemesh, other payloads), and the
  index walk read one package per child: cell 21267 (3,406 entities) needed more than 300 s
  of CPU that way and under 3 s from the package.
- `Loaded` now carries `instances: Vec<MapInstance { piece, matrix }>`; `pieces` holds one
  entry per unique mesh, in local space. `main.rs` builds one mesh handle per part and
  spawns one entity per part per instance under a Y-up root.
- `--map-dry-run`, `--map-piece-limit N` (400 single cell, 200,000 list). Per-selector LOD
  tables, chosen LOD and every unresolved link go to stderr.
- Guards that keep one bad record from poisoning the scene: out-of-range entity matrices,
  out-of-range vertices and undecodable meshes are named and counted.

## Commands and results

```powershell
$env:RUSTUP_TOOLCHAIN='stable'
cargo build --bin acu_bevy
# from the repo root:
acu_bevy --game-dir "<install>" --map-cells local\paris-world-cells.json --map-lod 2 `
         --no-textures --map-piece-limit 400000 --size 1920x1080 --capture local\paris-map-lod2.png

# map: 143640 pieces from 2150/2168 cells (250076 entities, 375609 child refs),
#      2128 distinct mesh ids, 2158 distinct selectors, LODs [0,1,2];
#      skipped: 4724 without a matrix, 101452 without a selector, 0 without a LOD record,
#      24 without a mesh, 235 undecodable meshes, 1 out-of-range matrix, 0 out-of-range
#      meshes, 0 cells without a cell block
# native bounds min=[-962.33344, -985.2814, -104.1986] max=[1288.1412, 1024.6433, 396.0409]
# elapsed 240 s including the capture
```

Dry run over the same list (no mesh decode): 135 s, 143,876 resolvable placements,
2,133 distinct mesh ids.

## Remaining uncertainty

- 235 meshes fail to decode (`unsupported vertex format 2 stride 40` and similar); they are
  counted per mesh id, not yet diagnosed.
- 4,724 entities have no accepted affine (matrix row 3 not 1.0, or values out of range).
- 101,452 entities reference no `51dc6b80` selector: lights, light cookies, FX, FakeGI and
  AI/path markers, plus largemesh-backed objects whose geometry path is still unknown.
- Water surfaces are absent; the Seine reads as a hole in the aerial.
- Textures are off in this pass.
