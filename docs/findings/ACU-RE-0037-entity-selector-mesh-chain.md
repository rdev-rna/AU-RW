# ACU-RE-0037 - Entity -> LODSelector -> LOD mesh: the record table and typed reference resolution

Date: 2026-10-22. Related: #135 (reserved here), ACU-RE-0035, ACU-RE-0036, #134.
State: observed. One cell now places 27 distinct meshes where it placed 2; a second,
unrelated cell resolves 18.

## Result

The placement chain is a decoded record walk, not an id search:

```text
GridCellDataBlock  (cell children; heterogeneous: Entity, nav, largemesh, other)
  -> Entity        (0984415e; 78-byte header, u32 count at 78, typed reference slots)
  -> LODSelector   (51dc6b80; 19-byte header + 5 x 35-byte records + 5-byte trailer)
  -> mesh id       (415d9568; resolved in the entity's entry, an _assets entry, or Paris)
```

### LODSelector payload

199 bytes, verified on twelve selectors in three entries (5311, 9544, 5155):

| offset | size | field |
| --- | --- | --- |
| 0 | 1 | version = 1 |
| 1 | 8 | self file id |
| 9 | 4 | type = `51dc6b80` |
| 13 | 6 | zero |
| 19 | 5 x 35 | records |
| 194 | 5 | trailer; `01 00 00 00 01` or `00 00 00 00 01`, not interpreted |

Record, 35 bytes:

| offset | size | field |
| --- | --- | --- |
| +0 | 8 | id, inside the selector's id block: strictly increasing, first is self+1..self+21 |
| +8 | 4 | tag = `a360319a` (bytes `9a 31 60 a3`) |
| +12 | 1 | flag_a: 1 = this level carries a mesh, 3 = empty level |
| +13 | 1 | flag_b: 1 on the first levels, 0 on a deeper mesh level; meaning unknown |
| +14 | 8 | mesh id (see resolution below) |
| +22 | 4 | f32 switch distance; observed 20, 40, 65, 80, 85, 86, 90, 90.1, 91, 120, 160, 320 |
| +26 | 9 | zero |

Record position is the LOD level: record 0 resolved `FRV_Street_Row_BievreRiver_26` to
`..._LOD0` and record 1 to `..._LOD1`. A selector may have one level, two, or three
(`..._LOD0`/`_LOD1`/`_LOD2`); empty levels are flagged `3/0` with no mesh id.

### Entity reference slots

An Entity payload stores the file ids of the components it uses. The first slot (offset
100) is only the first one, and its type depends on the entity: a light entity puts a
`344780d6` light there, an FX entity a `755ace14` component, a mesh entity a
`51dc6b80` selector. Multi-component entities keep their selector deeper in the payload
(`VIL_GP_Altair_01_001`: selector at offset 2990, meshes at 3034 and 3149, material at
3109). The selector is therefore located by resolving each 8-byte window against the
entity's *own entry* resource table and keeping the match whose type is `51dc6b80`.

### Mesh id resolution

A selector record's mesh id is a resource id that may live in three places. All branches
require the target resource's type to be `415d9568`:

1. a resource in the entity's own entry - `VIL_GP_Altair_LOD0` (entry 9544), used by
   self-contained cells;
2. an `_assets` entry whose single mesh carries that id - `000000102d232f68` is entry 1993,
   `FRV_Street_Row_BievreRiver_26_LOD0`;
3. a `415d9568` resource anywhere in Paris (mesh index), or a Paris entry whose single
   mesh carries the id.

## Measurements

Cell 05201 (entry 5311, 116 children): **45 pieces, 27 distinct meshes, 27 distinct
selectors**, LOD0. 35 children are not entities; 36 entities reference no selector
(lights, light cookies, FX, a largemesh object). Bounds `542.4..579.5 x -767.7..-735.1 x
-4.6..14.0` m. Before this change: 75 pieces from **2** distinct meshes. Evidence:
`local/map-cell-5311-lod0.png`, `local/map-cell-before.png`.

Cell 07896 (entry 9544, 621 children): **26 pieces, 18 distinct meshes, 18 distinct
selectors**. 291 children are not entities (69 x `1cbde084`, 170 x `3f742d26`, nav and
other cell payloads); 304 entities reference no selector (lights, FakeGI, AI). Evidence:
`local/map-cell-9544-lod1.png`.

## What this replaces

The previous resolver (ACU-RE-0036) took the selector at fixed offset 100, scanned selector
payloads for ids sharing the selector's high 32 bits, applied a name-prefix filter, and
fell back to the first selector resource in the package. All three rules are wrong against
the decoded table: mesh ids need not share the selector's 32-bit prefix (`0x1086ca995d` ->
`0x10bcb1c738`), the first reference slot is not always a selector, and the fallback made
unresolved entities inherit one object (73 copies of one street row).

## Unknowns

- `flag_b` semantics, and the five trailer bytes.
- The entity reference table is currently matched by type over 8-byte windows; its own
  record header (tag `ec658d29` at offset 92 on three-component entities) is not decoded.
- Entities that reference more than one selector: the first in payload order is used.
- 36 of 81 entities in cell 05201 and 304 of 337 in entry 9544 have no selector; most are
  lights/FX/AI, but the largemesh-backed objects among them still need a geometry path.
