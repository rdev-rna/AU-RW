# 115 - Paris resource type vocabulary

Issue: #115 (`type:re`, `area:world-data`). Branch: `acu-115-paris-type-vocabulary`.
Finding: `docs/findings/ACU-RE-0034-paris-type-vocabulary.md`.

## Result

126 of 134 types in `DataPC_ACU_Paris.forge` named by joining CRC-32(class name) against
the binary's own string table (32,712 candidate identifiers). Six known material-type
anchors reproduce exactly, which is the method's self-check.

Structural types: `GridCellDataBlock` 21,160 (`Cell00000_DataBlock`), `Entity` 340,346,
`EntityGroup` 72,597, `MeshShape` 287,245, `ReferenceListShape` 51,816, `LODSelector`
414,002, `CollisionMaterial` 69,535, `NavMeshManager` 3,329, `WorldTransitionPortal` 760.

Lighting: exactly four each of `GILoader`, `WorldAmbiance`, `LayeredSky`, `Climate`
(`PAR_WorldAmbiance_Dawn`), which matches the four baked time-of-day keyframes the repo
previously only had as a developer-talk claim.

## What it does not do

Naming is not decoding. `typed_parsed` is still 6.70 % of envelopes and that 6.70 % is
entirely texture, mesh and skeleton; every world type reports `typed_parsed: 0`. The
largest unnamed type `415d9568` (116,526 envelopes) is unresolved.

## Next

A resource-level index (entry -> type id, file id, name). `entries.jsonl` from `acu-scan`
is entry-level only, so a named type cannot yet be opened as bytes.

## Commands

```powershell
cd acu-rs; .\target\debug\acu-scan.exe ..\local\scan-paris "$env:ACU_GAME_DIR\DataPC_ACU_Paris.forge"
python tools/type_name_crc.py
python tools/world_type_candidates.py
```
