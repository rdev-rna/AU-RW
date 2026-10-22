# 137 - Whole-map pass: 8,330 cells, instanced

Issue: #137 (`type:slice`, `area:world-data`). Branch: `acu-137-whole-map`, stacked on
`acu-135-selector-lod-chain` (#136, ACU-RE-0037).

## How the game loads the map (measured through the asset index)

`DataPC_ACU_Paris.forge` entry 1 holds one `World` resource, `ACU_Paris`
(`0000000f18b7e1cf`, type `fbb63e47` = CRC-32 of `World`). Its 50,083 candidate links
resolve to:

| destination | count | meaning |
| --- | --- | --- |
| `GridCellDataBlock` (`ac2bbf68`) | 21,160 | every cell package, including the minimap cells |
| `LocalCubeMap_Cell_N_<hash>` (`eebb2443`) | 23,019 | per-cell reflection cubemaps |
| `CellID=NNNN` (`5730d30e`) | 2,811 | per-cell streaming metadata |
| seasonal sets (`41813744`: Summer/Autumn/Winter/Spring/Night) | 2,955 | lighting/climate variants |
| `WorldTransitionPortal` (`7c00f6af`) | 986 | inter-world transitions |
| `InventoryItemSettings`, `Entity`, `EntityGroup`, `BulkMesh` | 944 / 422 / 316 / 37 | gameplay and bulk geometry |

So the world is one manifest plus 21,160 self-contained cell packages; a cell package
carries its entities, their selectors, materials and (for largemesh cells) the meshes
themselves. The viewer reproduces that by iterating the cell packages, which is why the
21,160 count matches exactly.

## Cell filter correction

`local/paris-cells.json` has 21,160 cells. 12,830 of them are 71 bytes and hold exactly one
child, an `a2b7e917` texture named `ACU_Paris_<x>_<y>_Map` - minimap tiles, not geometry.
The earlier `record_length > 1000` filter was wrong: it also dropped **6,162 real cells**
(81-1000 bytes, 14-95 children each) that carry props, house corners, lanterns and bushes.
The list is now every cell with `record_length > 80`: **8,330 cells**, 530,850 child refs,
337,672 entities.

## Result

```text
8,330 cells -> 207,867 placements, 5,770 cells with geometry
337,672 entities scanned, 530,850 child refs
14,928 unique meshes, 15,155 direct-mesh entities (ground/largemesh chunks)
LODs 0/1/2, framed extent 2.4 km across
```

Evidence: `local/paris-map-complete.png` (2560x1440, LOD2, no textures). The Seine, both
arms, the island, the street grid and the building blocks read correctly at city scale.

## What changed

- `--map-cells FILE` (JSON `[{ "entry_index": N }, ...]`); `tools/world_cells.py` writes the
  list.
- A cell's entities come from the cell package's own `0984415e` resources, not from the
  child-id list (the list mixes entity, nav, physics and group payloads).
- **Entry reuse**: `load_piece` re-read and re-inflated the whole entry per mesh. A largemesh
  cell decodes to tens of megabytes and holds hundreds of meshes, so cell 21267 needed
  >10 min; reusing the cached package through `piece_from` takes **17 s** for 2,177 pieces.
- **Direct-mesh entities**: largemesh/terrain chunks carry no `51dc6b80` selector; they
  reference a `415d9568` mesh themselves (e.g. `Largemesh_MR_GroundMesh_Large_LOD2_Chunk_0005`).
  The loader now resolves a selector first, then a direct mesh, both by type in the entity's
  own entry. This is what restores the ground.
- `b22b3e61` is **Havok collision** (`hk_2013.3.0-r1`, `hknpExternMeshShapeData`), not render
  geometry: entities that reference only that are skipped, correctly.
- Mesh instancing: one decode per unique mesh, one `MapInstance` per placement.
- **`FAKE_*` backdrops are excluded**: fake ground/coast/city-silhouette meshes span the
  whole map and hid the city from above. They are counted (9 in the largest 200 cells) and
  named in the summary, so the exclusion is measured, not silent.
- Guards: out-of-range matrices and vertices, undecodable meshes are named and counted.
- `--map-dry-run`, `--map-piece-limit`, `--orbit-elevation`, `ACU_MAP_TRACE=1`.
- `--map-player` spawns the Arno rig inside the map with the existing walk controls
  (`--spawn X,Y,Z` in Bevy coordinates); the map meshes and the rig are separated by
  `arno_start` so the skin binder only sees rig pieces.
- **Why walking did nothing at first**: `acu_sim::blocked` hard-coded the synthetic 24x24 m
  arena (`|x| > 11.5 || |z| > 11.5`), so at map coordinates every axis step was rejected and
  only the yaw changed. `Player` now carries `bounds` and `floor`: a map scene sets
  `bounds = 100_000` and `floor = spawn.y`. Movement is still flat - there is no runtime
  collision, so he walks through walls and over the river.
- Framing: a multi-cell scene uses the 2nd..98th percentile of placement origins per axis
  (the river arms otherwise stretch the frame) and a 1.1x distance multiplier; one cell
  frames its meshes directly and reports outliers.

### The blank-capture bug

`inspect_camera` clamped the orbit distance to `0.4..20.0` every frame, so any scene larger
than 20 units had its camera snapped to 20 units from the centre on the first frame - a city
capture came out as uniform sky. The clamp is now relative to the framing distance
(`initial * 0.02 .. initial * 4.0`). Multi-cell scenes are framed by their **placement
origins** (terrain and water chunks span kilometres and would otherwise set the camera
distance), single cells by their mesh bounds. The far plane is explicit (100 km).

## Commands and results

```powershell
$env:RUSTUP_TOOLCHAIN='stable'
cargo build --bin acu_bevy
acu_bevy --game-dir "<install>" --map-cells local\paris-world-cells.json --map-lod 2 `
         --no-textures --map-piece-limit 500000 --size 2560x1440 --capture local\paris-map-full.png
```

Dry run over 8,330 cells: 208,472 resolvable placements in 265 s, 14,952 distinct mesh ids.

## Remaining uncertainty

- 2,557 of 8,330 cells contribute no geometry: their entities are lights, FX, FakeGI and AI
  markers, or reference nothing the loader can type.
- 123,291 entities reference no selector and no direct mesh.
- Water surfaces are still absent; the Seine reads as a hole.
- Textures are off in this pass. 235 meshes fail to decode (vertex format), counted per id.
- The `World` resource's payload is 1.6 MB of id blocks; the link table gives its targets but
  the streaming order and load radius are engine behaviour, not decoded.

