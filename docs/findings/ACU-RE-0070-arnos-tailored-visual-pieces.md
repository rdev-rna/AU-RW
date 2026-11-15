# ACU-RE-0070 — Which meshes make up "Arno's Tailored"

Date: 2026-11-15 / Claude Code session / user request "swap the Fearless outfit for Arno's Tailored".
Related: [ACU-RE-0016](ACU-RE-0016-build-table-rig-references.md) (avatar BuildTables),
[ACU-RE-0048](ACU-RE-0048-outfit-equip.md) (item IDs; corrected here), [ACU-RE-0011](ACU-RE-0011-clustered-vertex-decode.md).
State: partially established. Tier codes, internal names, table rows and mesh identities are
observed. The final step, from the item's internal name to a table row, matches names and is not
decoded.

**Correction (same day).** The first version of this finding chose `Arnaud_V3`. It assumed table
row *i* = item `_0i` and took ACU-RE-0048's claim that Tailored is `_03`. The user identified the
rendered V3 set as Arno's **Master** set. The tier codes below confirm that: `_03` is Master and
`_02` is Tailored.

## Provenance

- No native analysis. Installed `ACU.exe` SHA-256
  `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40` (identifies the install only).
- `AvatarGearManager_0X110741B51F` (`000000110741b51f`, `DataPC.forge` entry 110, ordinal 791;
  identical copy in `DataPC_patch_01.forge` entry 0). Payload 346,949 bytes, read with
  `acu-mat --hex`. The layout used is observed bytes only:
  - every `ACU_AvatarGear_*` item ID appears once;
  - the u32 at item-ID offset −5 is a small code;
  - a UTF-16 internal name such as `Arno_Head_2` follows the item ID.
- Avatar BuildTables: `DataPC_ACU_LGS_BelleEpoque.forge` entry 334, resources 122 Hoods,
  123 UpperBodies, 124 Belts, 125 Pants. Each decodes to its exact payload end with
  `tools/build_table_probe.py`; payload hashes are in ACU-RE-0016. Table 126 ThrowBlade is rejected
  at payload offset 211 (nonempty PropertyPath), so it was not decoded.
- Localized names: the gear-name table of the third-party mod "Gear Visual Customization 365"
  1.2.5, as in ACU-RE-0048. Only its text files were read, into a scratch directory; no text is
  reproduced. Asset index v2026-10-22.1 supplied names and type labels (`51dc6b80` LODSelector
  `crc-name`, `415d9568` Mesh `observed-decode`, `c69075ab` InventoryItemSettings `crc-name`).

## Result

**Observed: the AGM tier code.** The u32 code before each item ID agrees with the mod's tier word
for all 170 checked items (slots Head, Chest, Forearms, Waist and Legs, items `_05`–`_40`):
Handmade = 2, Tailored = 3, Improved = 4, Master = 5, Legendary = 6.

**Observed: Arno's family.**

| Item | Internal name | Code | Localized (by code) |
|---|---|---|---|
| `_01` | `Arno_<Slot>_1` | 1 | Arno's Original |
| `_02` | `Arno_<Slot>_2` | **3** | **Arno's Tailored** |
| `_03` | `Arno_<Slot>_3` | 5 | Arno's Master |
| `_04` | `Arno_<Slot>_4` | 6 | Arno's Master Assassin (the mod lists it last) |

The mod's list starts with Original, Tailored and Master, and moves the Master Assassin item to the
end. Its position *i* → `_{i+2}` rule therefore holds for every other family, but is off by one
for Arno's three. That is ACU-RE-0048's error.

Tailored item IDs (`_02`): Head `00000011a8f2593f`, Chest `00000011a8f25aae`, Forearms
`00000011a8f25bf1`, Waist `00000011a8f25b51`, Legs `00000011a8f25c94`.

**Observed: table rows.** Each table has 43 rows. Row 0 is empty. Rows 1–40 hold 10 families of
4 variants, in this order: Arnaud, Rodeur, Revolutionnaire, Napoleonian, Phantom, Musketeers,
Medieval, Military, Brigands, Noble. Rows 41–42 are the two SDN armor sets. Row 2 (`Arnaud_V2`) is
the only row with weight 1. The item order is different: the AGM has Medieval at `_17`–`_20` and
Phantom at `_21`–`_24`. So a row index is **not** an item index.

**Inference: item → row.** `Arno_<Slot>_N` ↔ `Arnaud_VN` matches by name. It agrees with two
independent checks: `_03`/V3 is the set the user recognises as Master, and `_02` carries the
default weight. Tailored is therefore row 2 of each table, the Arnaud V2 pieces. The ThrowBlade
(bracers) table is undecoded, and V2 is assigned there by the same naming.

**Mesh identities used** (`DataPC_ACU_LGS_BelleEpoque.forge`, resource ordinal 1 of each entry).
Each mesh has a single distinct payload across BelleEpoque and its patch_03:

| Entry | LOD0 mesh | File ID | Payload SHA-256 |
|---|---|---|---|
| 370 | CN_P_FR_Upperbody_Arnaud_V2_LOD0 | `00000013c1e4eb5a` | `dda8f6815868d16eccb32582c05fc56726d4423979228a622bec7ecbb9c95145` |
| 364 | CN_P_FR_Flap_Arnaud_V2_LOD0 | `00000013c1e4eb9a` | `fb917ea9bb28f714ba5cc0e91fb413d7c975314974aedf9ca328bd615f2adfd3` |
| 368 | CN_P_FR_Scarf_Arnaud_V2_LOD0 | `00000013c1e4eba4` | `f3efc492f6a6c8b8c889c53b3317638ac2becf0fd5abda9ddee7769a4a549598` |
| 367 | CN_P_FR_Hood_Arnaud_V2_LOD0 | `00000013c1e4eb79` | `5aa46fdbd5893204dc9f27447c42477010c975ae332c611df06c77c57a7d84fe` |
| 366 | CN_P_FR_Pants_Arnaud_V2_LOD0 | `00000013c1e4eb65` | `6601c9834ce5e504707eb66fa940da26ce3decacf9610146b7090878c56e4866` |
| 365 | CN_P_FR_Belts_Arnaud_V2_LOD0 | `00000013c1e4eb6f` | `0b60073e5b84dd365835e9cec446d08ce450061c7d311d1fb137ebf3ee74588c` |
| 369 | CN_P_FR_ThrowBlade_Arnaud_V2_LOD0 | `00000013c1e4eb86` | `d7d15c38715102c83732b47375c8dd04416f6b4fa42d9828ca2b0752253740a9` |

For reference, Master (`Arnaud_V3`) is BelleEpoque entries 425–429, 436 and 447. Those are Flap,
Upperbody, Hood, Pants, Belts, ThrowBlade and Strap; V3 has no scarf.

**Skinning check.**
- All seven V2 pieces bind with a bind-identity error of at most 6.7e-7.
- The belts have one joint, `b62122f9`, that is unresolved and unweighted. It is kept parentless
  and frozen under the existing policy.
- Mesh bind versus composed skeleton bind is at most 0.09° and 0.0315 m (report field
  `max_bind_vs_skeleton_*`).
- Bind, walk (120 ticks) and run (90 ticks) captures deform without visible tearing.
- These are captures of the synthetic arena, not an in-game appearance claim.

**Colour.** Colour sets are decoded and applied in [ACU-RE-0071](ACU-RE-0071-avatar-color-sets.md).

**Not covered.** Hood-down, Hood_Position, the IconicWeapon V2 mesh and LOD1+ are not used.

## Reproduction

```powershell
$d = $env:ACU_GAME_DIR
& acu-rs/target/debug/acu-mat.exe (Join-Path $d 'DataPC.forge') 110 791 --hex --out=local/tailored/agm.json
foreach ($r in 122,123,124,125) {
  & acu-rs/target/debug/acu-mat.exe (Join-Path $d 'DataPC_ACU_LGS_BelleEpoque.forge') 334 $r --hex --out=local/tailored/t$r.json
  python tools/build_table_probe.py local/tailored/t$r.json --output local/tailored/t$r-parsed.json
}
powershell -File tools/run_arno.ps1 -Capture local/captures/tailored/v2-idle.png
```

For each item, find its little-endian file ID in the AGM payload. Read the u32 at offset −5 and
the UTF-16 name after the ID.

## Development handoff
- **Confidence:** partial. The tier code and the rows are observed. Item → row matches by name,
  and the user and the default weight corroborate it.
- **Interface:** `acu_bevy::options::ARNO_PIECES` lists the seven V2 pieces plus the head and the
  hood-up hair plane. The Fearless set (`--piece 679:<n>`) and the Master V3 set (`--piece 425:1` …)
  stay loadable.
- **Follow-up:** decode the AvatarGear record and how it selects a row. Support PropertyPath tables
  (ThrowBlade, Hood_Position). Apply colour sets.
