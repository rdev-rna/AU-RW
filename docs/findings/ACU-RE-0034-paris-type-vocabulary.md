# ACU-RE-0034 - Paris resource type vocabulary recovered by CRC-32

Date: 2026-10-22. Related: #115 (reserved here), #4 world-data, ACU-005.
State: observed. The type vocabulary is named; no world container is decoded.

## Method

A resource type id is the zlib CRC-32 of its class name. That rule is already in the
repo (`acu_assets/material.rs:10-12`, ACU-RE-0002) for six material types; this finding
applies it to the whole Paris archive.

1. `strings_all.txt` (89,474 strings, UTF-16LE) yielded **32,712** distinct plausible
   identifiers (C++ names, optionally namespace-qualified).
2. Each was hashed with CRC-32. All six known anchors reproduce exactly:

   ```
   Material 85c817c3   TextureSet d70e6670   Mask df5d6c0e
   Mask16 92b95f74     TextureSelector 7d08460d   UVTransform c52e2125
   ```
3. `acu-scan` on `DataPC_ACU_Paris.forge` produced a census of **134 types**.
4. The two sets were intersected: **126 of 134 types named (94%)**.

## Census

2,199,415 resource envelopes; 24,675 of 24,677 entries decoded; 0 failed entries;
0 unparsed metadata extensions. Typed parsing is unchanged at
**147,365 / 2,199,415 (6.70%)**, which is exactly texture 18,329 + mesh 116,490 +
skeleton 12,546. Every world type below is `typed_parsed: 0`.

| type id | name | count | example resource name |
| --- | --- | ---: | --- |
| `51dc6b80` | LODSelector | 414,002 | `CN_C_FR_HighClass_MA_A_Hat` |
| `0984415e` | Entity | 340,346 | `GFX_WPS_Invalides` |
| `b22b3e61` | MeshShape | 287,245 | `MDA_House_A_4F_CC_01` |
| `85c817c3` | Material | 258,135 | - |
| `d70e6670` | TextureSet | 215,882 | - |
| `415d9568` | **unnamed** | 116,526 | - |
| `1cbde084` | ShadowRigidBody | 85,497 | - |
| `3f742d26` | EntityGroup | 72,597 | `CE_Ambush_01_3Agressor_1Ally_001` |
| `74f7311d` | CollisionMaterial | 69,535 | `ACU_Stone_Dirt` |
| `0fa3067f` | Animation | 61,050 | - |
| `2d675ba2` | ReferenceListShape | 51,816 | `MDA_House_Corner_A_0_03x10x3_01 Merged Shape` |
| `ac2bbf68` | **GridCellDataBlock** | 21,160 | `Cell00000_DataBlock` |
| `344780d6` | OmniLight | 20,272 | - |
| `0b4ce0e0` | NavMeshManager | 3,329 | `NavMeshManager_03705` |
| `7c00f6af` | WorldTransitionPortal | 760 | `WorldTransitionPortal_Default` |
| `3306cd8e` | DistrictRegionCellData | 255 | - |
| `44f7c7a6` | GenericRegionCellData | 181 | - |
| `17fb5aa8` | RegionLayout | 11 | `Crowds` |
| `9ed472d4` | GILoader | **4** | - |
| `21c2d472` | WorldAmbiance | **4** | `PAR_WorldAmbiance_Dawn` |
| `24d32cf8` | LayeredSky | **4** | `PAR_LayeredSky_Dawn` |
| `f549cd6b` | Climate | **4** | `Climate` |

Other named families, all previously unknown to the repo: `AmbianceZone` 5,497,
`BuildTable` 15,046, `BulkMesh` 159, `HairMesh` 142, `AtomGraph` 1,567,
`StreamableAnimationDataBlock` 526, `TimeOfDayPropertyControllerData` 11,
`ContactTable` 7, `FloorHeightData` 1, `CrowdDensityGrid` 1, `MinimapTextures` 1.

## What this establishes

- The Paris map's structural vocabulary is **cells** (`GridCellDataBlock`, named
  `Cell00000_DataBlock`), **groups** (`EntityGroup`), **placements** (`Entity`), and
  **shapes** (`MeshShape`, `ReferenceListShape`, `BulkMesh`), with `LODSelector`,
  `CollisionMaterial` and `NavMeshManager` alongside. That is the shape of the map.
- The lighting set is **four of each**: `GILoader`, `WorldAmbiance`, `LayeredSky`,
  `Climate`, with example names `PAR_WorldAmbiance_Dawn` / `PAR_LayeredSky_Dawn`.
  This independently corroborates the "four baked time-of-day keyframes" recorded as a
  *historical* claim in `RENDERING_SOURCES.md:38-44` - now observed as four resources in
  this build, not assumed.
- Entity example names are real Paris identifiers (`GFX_WPS_Invalides`), so entity names
  are usable as labels for locating a district.

## Limits

- Naming proves the class **name hashes to the id**. It does not prove the resource
  layout, field order or semantics. A name is a vocabulary item, not a decoder.
- Example names come from the envelope's name field; they label the resource, they do
  not establish behaviour.
- `415d9568` (116,526 envelopes, the largest unnamed type) is unresolved. Its class name
  is not in the binary's string table as extracted, or is spelled differently.
- Nothing here is a world reader. `typed_parsed` is still zero for every type above.

## Reproduction

```powershell
# 1. census of the Paris archive (writes local/scan-paris)
cd acu-rs
$env:ACU_GAME_DIR = "C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity"
.\target\debug\acu-scan.exe ..\local\scan-paris "$env:ACU_GAME_DIR\DataPC_ACU_Paris.forge"

# 2. candidate identifiers -> CRC-32 (verifies the six known anchors)
python tools/type_name_crc.py

# 3. world/map shortlist
python tools/world_type_candidates.py
```

## Next step

Build a **resource-level index**: entry -> (resource type id, file id, name), so a named
type such as `GridCellDataBlock` can be reached as bytes. `entries.jsonl` is entry-level
only (`archive, control, entry, stored_sha256, typed_status`), which is why this finding
can name the cell type but cannot yet open one.
