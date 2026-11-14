# ACU-RE-0070 — Which meshes make up "Arno's Tailored"

Date: 2026-11-15 / Claude Code session / user request "swap the Fearless outfit for Arno's Tailored".
Related: [ACU-RE-0016](ACU-RE-0016-build-table-rig-references.md) (avatar BuildTables),
[ACU-RE-0048](ACU-RE-0048-outfit-equip.md) (Tailored item IDs), [ACU-RE-0011](ACU-RE-0011-clustered-vertex-decode.md).
State: partially established. Table rows and mesh identities are observed. The step from item
`_03` to table row 3 is an inference.

## Provenance

- No native analysis in this pass. Installed `ACU.exe` SHA-256
  `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40` (re-hashed), recorded only to
  identify the install.
- Archive `DataPC_ACU_LGS_BelleEpoque.forge`, entry 334 (avatar package; stored identity in
  ACU-RE-0016). Tables decoded to exact payload end with `tools/build_table_probe.py` over
  `acu-mat --hex` output. Their payload SHA-256 values match ACU-RE-0016:
  122 Hoods, 123 UpperBodies, 124 Belts, 125 Pants. Table 126 `CN_P_ArnoAvatar_ThrowBlade` is
  rejected by the probe at payload offset 211 (nonempty PropertyPath), so it was not decoded.
- Asset index v2026-10-22.1. Names come from the resource name field. Type labels: `51dc6b80`
  LODSelector (`crc-name`), `415d9568` Mesh (`observed-decode`), `c69075ab`
  InventoryItemSettings (`crc-name`).
- Item record `ACU_AvatarGear_Head_03` (`00000011a8f258c7`, `DataPC.forge` entry 110 ordinal 1183)
  has an 89-byte payload, SHA-256 `93e97e7a…96f4`. It holds no LODSelector, mesh or row-tag
  reference.

## Result

**Observed (table rows).** Each of the four decoded tables has 43 rows. Row 0 has no components.
Rows 1–4 reference the `Arnaud_V{row}` LODSelectors:

| Row | Hoods (col 2 / 16) | UpperBodies (col 1, 2, 4) | Belts (col 2, 6) | Pants (col 1) | weight |
|---|---|---|---|---|---|
| 1 | Hood_Arnaud_V1 / Hood_Down_V1 | Upperbody, Flap, Scarf V1 | Belts V1 | Pants V1 | 0 |
| 2 | Hood_Arnaud_V2 / Hood_Down_V2 | Upperbody, Flap, Scarf V2 | Belts V2 | Pants V2 | **1** |
| 3 | Hood_Arnaud_V3 / Hood_Down_V3 | Upperbody, Flap V3 (no scarf) | Belts V3, Strap V3 | Pants V3 | 0 |
| 4 | Hood_Arnaud_V4 / Hood_Down_V4, ShoulderCover V4 | Upperbody, Flap V4 | Belts V4, Strap V4 | Pants V4 | 0 |

Row tags (u32): Hoods row 3 `fb07ec5e`, UpperBodies `6488e3a0`, Belts `b1c16126`, Pants `dbe8ba4b`.

**Inference (row = item index).** Each table has 42 non-empty rows. ACU-RE-0048 found 42
`ACU_AvatarGear_<Slot>_NN` items per slot. In every table, row 2 is the only row with weight 1, and
item `_02` is the starting "Arno's Original" set (it is linked from the loadout preset manager).
This fits row *i* ↔ item `_i`. Under it, Tailored (`_03`) → row 3 → `Arnaud_V3`.
Not established: how the game maps an item to a row. No row tag was found in the item record or
in the AvatarGearManager payload (346,949 bytes; no u32 match for any of the 172 row tags). The
ThrowBlade (bracers) row order is undecoded. The table byte-links `CN_P_FR_ThrowBlade_Arnaud_V3`
as a candidate, and V3 is assigned by name consistency.

**Mesh identities used** (`DataPC_ACU_LGS_BelleEpoque.forge`, resource ordinal 1 of each entry,
payload SHA-256 from the scene log):

| Entry | LOD0 mesh | File ID | Payload SHA-256 |
|---|---|---|---|
| 426 | CN_P_FR_Upperbody_Arnaud_V3_LOD0 | `0000001bf62cc280` | `ed2e8a15f9b60d5aece4775d24b5f548b8b0293300b42cfe202e8bf952d2568c` |
| 425 | CN_P_FR_Flap_Arnaud_V3_LOD0 | `0000001bf62cc289` | `ebc2c62668d1bf37a64ae97c2f5dd1b14f2e0ebb4cf5455f415fc0b06e31a52e` |
| 427 | CN_P_FR_Hood_Arnaud_V3_LOD0 | `0000001bf62cc28e` | `aa96d5d59bee189e5c2adfe05d0b242aa2a9401cbe28dee42b5ad4dfcc38db90` |
| 428 | CN_P_FR_Pants_Arnaud_V3_LOD0 | `0000001bf62cc26f` | `e521956b29292e2ecab5efb433986b9b0b18eefc7ba99763c1eae3e895c83393` |
| 429 | CN_P_FR_Belts_Arnaud_V3_LOD0 | `0000001bf62cc265` | `aa4162353116abf15da93c93f59142b08d4e80491b2eae131ac0e5dd64389746` |
| 447 | CN_P_FR_Strap_Arnaud_V3_LOD0 | `0000001e0ecc1b6a` | `7ed2e3c6ae51a29128c1f6b8c9c584cf5f56e58027e61bce88e8f9e14e10117d` |
| 436 | CN_P_FR_ThrowBlade_Arnaud_V3_LOD0 | `0000001bf62cc434` | `ae8895fdd7f49fd2f48cd6979f6bf42b33a9d1cacc79b464bc2cadc34b6ab253` |

`DataPC_ACU_LGS_BelleEpoque_patch_03.forge` holds copies of all seven with the same file IDs and
equal payload SHA-256. Issue #35 (which duplicate loads) therefore does not change these bytes.
The LODSelector-to-LOD0 step follows the resource name, as for the earlier Fearless set.

**Skinning check.** All seven pieces bind to the composed rig with 0 unresolved joints. The
bind-identity error is at most 6.7e-7. The mesh bind differs from the composed skeleton bind by
at most 0.08° and 0.0141 m (new report field `max_bind_vs_skeleton_*`). Idle, walk (120 ticks) and
run (90 ticks) captures deform without visible tearing. These are captures of the synthetic arena;
they make no claim about in-game appearance.

**Not covered.** Colour sets (`CN_P_Avatars_ColorSets`, entry 334/127) are not applied; the
default diffuse is rendered. Hood-down variants, the Hood_Position table and LOD1+ are not used.

## Reproduction

```powershell
$a = Join-Path $env:ACU_GAME_DIR 'DataPC_ACU_LGS_BelleEpoque.forge'
foreach ($r in 122,123,124,125) {
  & acu-rs/target/debug/acu-mat.exe $a 334 $r --hex --out=local/tailored/t$r.json
  python tools/build_table_probe.py local/tailored/t$r.json --output local/tailored/t$r-parsed.json
}
powershell -File tools/run_arno.ps1 -Capture local/captures/tailored/idle-35.png
```

Resolve row component file IDs against `resource.file_id` in the asset index.

## Development handoff
- **Confidence:** partial. Rows → meshes are observed. Item `_03` → row 3 is inferred.
- **Interface:** `acu_bevy::options::ARNO_PIECES` now lists the seven V3 pieces plus head and the
  hood-up hair plane. The Fearless set stays loadable with `--piece 679:<n>`.
- **Follow-up:** decode the item → row mapping (native, or the gear entries in the AvatarGearManager).
  Support PropertyPath tables (ThrowBlade, Hood_Position). Apply colour sets.
