# COMPACT - ACU Paris map into Bevy (state at 2026-10-22, session 2)

Repo: `C:\Users\Administrator\AU-RW`, `main` at `98c3ff9`. Working tree clean.
This file is the restart point. Read it, then do "Next experiment" below.

## Where the work stands

| stage | state | evidence |
| --- | --- | --- |
| resource type vocabulary | done, 126/134 named | ACU-RE-0034 |
| cell/entity container layouts | done | ACU-RE-0035 |
| world mesh type `415d9568` + single render | done | ACU-RE-0036 |
| entity placement (matrices) | done, 80/81 distinct positions | `tools/check_cell_placement.py` |
| **selector -> mesh resolution** | **WRONG: 2 of 27 distinct meshes** | below |
| multi-cell / whole map | not started | |

Renders that are correct (single objects, at origin, LOD0/LOD1):
`local/paris-sheet.png` (street row, wall, gate), `local/paris-lod1*.png`.
Cell scene attempt: `local/map-cell-05201-*.png` - shows a quay platform with railings
and a staircase, but only 2 distinct meshes from 27 selectors.

## The one open bug

Cell 05201 (entry 5311): 116 children, 81 entities with affine matrices, 80 distinct
positions, 45 with a valid selector id at offset 100, **27 distinct selectors**.
The viewer places 75 pieces but only **2 distinct mesh labels**
(`FRV_Street_Row_BievreRiver_26_LOD1` x73, `GEN_ALL_Stairs_2m_01A_LOD1` x2).

Facts established:
- The `LODSelector` id is at entity offset **100** (and again at 122, 22 bytes apart);
  true for every entity sampled, but **36 of 81 have a non-selector value there**
  (floats such as `1.0f` read as `033f800000000000`).
- Selector payloads contain **no Paris mesh resource ids** (`tools/trace_selector_mesh.py`:
  0 hits against `local/paris-world-meshes.bin`). The LOD objects are `_assets` **entry**
  ids near the selector id (`...aa6d7` selector -> `...aa6df` = entry 1992, `_LOD1`).
- Probing ids at +/-64 around the selector, and scanning the payload, both land on shared
  entries: that is what collapses 27 selectors into 2 meshes.
- Name matching alone fails: composite entities cross names
  (`FRV_CanalKit_RiverKit_26_008` -> `FRV_Street_Row_BievreRiver_26`).

## Next experiment (do this first)

Decode the **selector payload record table**; stop probing.

1. Dump four selectors whose correct LOD we know by name:
   `00000015470aa6d7`, `00000010422cc115`, `0000001086ca995d`, `0000002f4e5cce91`.
2. Align their 199-byte payloads and find the record that holds the LOD id: look for the
   field that differs per selector and always equals `selector_id + delta` (sample: +8).
3. Expect a small header (`u8 version, u64 self, u32 type`), then a count, then records of
   a fixed stride; the same shape as `GridCellDataBlock` (10-byte `u16 flag + u64 id`).
4. Implement that parse in `load_map` (`acu-rs/crates/acu_bevy/src/load.rs`, function
   `load_map`, block starting at `// The selector reference sits at a fixed offset`).
5. Re-run: `--map-cell 5311` must report **~27 distinct labels**, not 2.

Then: loop the 2,168 world-size cells (`record_length > 1000` in
`local/paris-cells.json`), spawn with LOD2/LOD3, capture an aerial.

## Tools and data already built (all in `main`)

- `acu-resource` bin: dump a resource payload by entry + file id (`--type` fixed).
- `tools/build_resource_index.py` -> `local/paris-{selectors,world-meshes,entities}.bin`.
- `tools/paris-selector-names.json`: 13,705 selector names -> entry (note: most are in
  entry 1, a shared table, so read that package once and cache).
- `tools/check_cell_placement.py`, `tools/trace_selector_mesh.py`,
  `tools/entity_selector_offsets.py`.
- Census: `local/scan-paris/{summary.json,entries.jsonl}` (708 MB, per-resource rows).
- Indices are ignored by git (`local/`), regenerate with the scripts above.

## Environment gotchas (all cost time already)

- `RUSTUP_TOOLCHAIN=stable` on every cargo call; the pin is broken upstream.
- `python` resolves to the Windows Store stub unless
  `C:\Users\Administrator\AppData\Local\Python\bin` is first on PATH.
- `acu_bevy --capture` **silently writes nothing with an absolute path**; use a relative
  path (e.g. `..\local\x.png`).
- `acu-scan` on `DataPC_ACU_Paris_assets.forge` (4.9 GB) stalls (I/O bound); targeted
  entry probes are cheaper. `DataPC_ACU_Paris_gi.forge` is **14.2 GB** - untouched, and it
  holds the four baked time-of-day GI sets.
- Minimap tiles are `NoInlinePixels`: the Paris map *image* cannot be stitched from the
  cell resources; pixels are prefetch-streamed.

## Rules that still apply

- Protocol: `docs/PROJECT_BOARD.md` (claim, branch `acu-<issue#>-slug`, notes in
  `docs/status/`, shared files maintainer-owned).
- No claim without measurement; unknown fields stay unknown in the findings.
- Lighting is synthetic; no shading or GI parity is claimed anywhere.

## Addendum (same session, after the compact note)

The selector payload is a **record list**, decoded by `tools/align_selectors.py`:

```
u8   version = 1
u64  self id
u32  type = 51dc6b80
u32  0
u16  0
   record at 19, then every 35 bytes:
     u64  id          = self + 3, self + 4, self + 5 ... (one per sibling object)
     u32  tag         = 9a3160a3 (constant across selectors)
     u8   1, u8 1
     u32  varies      (2d232f68 / 3b7e8954 / ... - likely a name hash)
     u32  16 or 8
     f32  20.0 / 22.5 (LOD distance?)
     u64  0
```

Four unrelated selectors align perfectly and the ids track `self + k`, so the LOD
reference is found by reading these records, not by probing ids.

Likely cause of the 2-of-27 collapse now identified: the selector resource lookup falls
back to `first selector in the package` when the file-id match fails, and most selectors
live in entry 1 (a shared table with thousands of them) - so every entity ends up with
that entry's first selector, named `FRV_Street_Row_BievreRiver_26`. Fix: never fall back
to "first selector"; require an exact file-id or exact name match, and fail loudly.
