# 117 - Resource payload dumper and world container decode

Issue: #117 (`type:slice`, `area:world-data`). Branch: `acu-117-resource-dumper`.
Finding: `docs/findings/ACU-RE-0035-world-cell-entity-containers.md`.

## Result

`acu-resource` (new bin) reads one resource payload by file id from an archive entry.
With it, two world containers are decoded and verified:

- **GridCellDataBlock**: `u8 version, u64 self, u32 type, u32 count,
  count x (u16 flags, u64 child), u32 x3 tail`. The arithmetic
  `17 + count*10 + 12 == payload_length` holds on every sample.
- **Entity**: `u8 version, u64 self, u32 type, u8 flag, f32[16] affine matrix, u32 count`.
  Sample `FRV_CanalKit_RiverKit_26_008` gives rotation ~47 deg about Z and position
  `(553.877, -747.872, 4.689)` with an exact `1.0` last component.

The map is now addressable as `cell -> child ids -> Entity -> transform -> shape refs`.

## Correction made during the work

The first parser read the entity matrix at offset 13 instead of 14 and produced values
around 1e-32 to 1e-44. The numbers were obviously wrong (a transform that is neither
affine nor scaled), which is what exposed the off-by-one. They are not reproduced in the
finding.

## Two uses of one type

Of 21,160 `GridCellDataBlock` records, 12,781 are exactly 71 bytes and reference a
`ACU_Paris_<n>_0_Map` texture (minimap tiles), while 2,168 exceed 1 KB and hold the 3D
contents. Treating every cell as a world cell would be a mistake; the index records both.

## Unknowns recorded, not guessed

`u8 body flag` (3), the per-child `u16` (always 1 so far), the first tail `u32` (114 vs 0),
whether the entity's trailing `u32` is a child count, and the axis convention for the
matrix. The transform must not be used for a camera or a parity claim until the convention
is established.

## Commands

```powershell
.\acu-rs\target\debug\acu-resource.exe <Paris forge> --entry 5311 --fid 0000000f18ec6055 --len 220 --out local\cell05201.bin
python tools/paris_resource_index.py
python tools/verify_world_layout.py
```
