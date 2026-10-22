# 135 - Entity -> LODSelector -> mesh: record table and typed resolution

Issue: #135 (`type:re`, `area:world-data`). Branch: `acu-135-selector-lod-chain`.
Findings: `docs/findings/ACU-RE-0037-entity-selector-mesh-chain.md`.

## Result

Cell 05201 (entry 5311) now places **45 pieces from 27 distinct selectors with 27 distinct
meshes**; before the change it placed 75 pieces from 2 distinct meshes. Cell 07896 (entry
9544), a different area, resolves 26 pieces / 18 distinct meshes.

## What changed

`acu-rs/crates/acu_bevy/src/load.rs`, `load_map`:

- `LODSelector` is decoded as a record table: 19-byte header, five 35-byte records at tag
  `a360319a`, five-byte trailer. `flag_a == 1` marks a level with a mesh; record position is
  the LOD level; the record's mesh id is a file id, and its f32 is a switch distance.
- The selector is located by resolving each 8-byte window of the entity payload against the
  **entity's own entry** resource table and keeping the match whose type is `51dc6b80`.
  Fixed offset 100, high-32-bit matching, name prefixes and the "first selector in the
  package" fallback are all gone.
- Mesh ids resolve by type in four measured places: the entity's entry, an `_assets` entry,
  the Paris mesh index, or a Paris entry. Nothing is substituted.
- New `--map-lod N` (0..4, default 0) picks the preferred level; when a selector has no such
  level the finest available is used, and the chosen level is reported.
- Per-selector LOD tables and every unresolved link are printed on stderr; the summary
  reports pieces, distinct meshes, distinct selectors, levels used and skip counts.

New tool `acu-entries` (`acu-rs/crates/acu_assets/src/bin/acu-entries.rs`) lists archive
entries, which is how `_assets` entry ids were resolved; `tools/selector_assets.py`,
`tools/cell_entities.py`, `tools/entity_refs.py`, `tools/check_selector_local.py`,
`tools/find_resource.py`, `tools/find_named_resources.py`, `tools/world_cells.py` and
`tools/cell_children_audit.py` are the analysis helpers behind the measurements.

## Commands and results

```powershell
$env:RUSTUP_TOOLCHAIN='stable'
cargo build --bin acu_bevy
# from the repo root:
acu_bevy --game-dir "<install>" --map-cell 5311 --capture local\map-cell-5311-lod0.png
#   map: cell 5311 with 116 children
#   map: 45 pieces, 27 distinct meshes, 27 distinct selectors, LODs [0];
#        skipped: 0 without a matrix, 35 without an entity, 36 without a selector,
#        0 without a LOD record, 0 without a mesh
#   native bounds 542.4..579.5 x -767.7..-735.1 x -4.6..14.0

acu_bevy --game-dir "<install>" --map-cell 9544 --map-lod 1 --capture local\map-cell-9544-lod1.png
#   map: 26 pieces, 18 distinct meshes, 18 distinct selectors, LODs [0,1];
#        skipped: 0 without a matrix, 291 without an entity, 304 without a selector, ...
```

`acu-entries.exe <assets.forge> > local/paris-assets-entries.tsv` produced 48,070 entry ids;
`local/paris-world-meshes.bin` has 24,717 Paris mesh ids; `local/paris-selectors.bin` has
13,823 selector rows (regenerate with `tools/build_resource_index.py`).

## Remaining uncertainty

- `flag_b` and the five trailer bytes are uninterpreted; nothing depends on them.
- The entity's own reference header (tag `ec658d29` at offset 92 on three-component
  entities) is not decoded; the selector is matched by component type.
- 36 of 81 entities in cell 05201 and 304 of 337 in entry 9544 carry no selector (lights,
  light cookies, FX, FakeGI, AI markers). The largemesh-backed objects among them still need
  their own geometry path before the whole-map pass can claim completeness.
