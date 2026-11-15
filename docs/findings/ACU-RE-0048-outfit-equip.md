# ACU-RE-0048 — Equipping the "Arno's Tailored" outfit at runtime

Date: 2026-11-14 / static subagent pass for the live outfit-swap slice. Related:
[ACU-RE-0041](ACU-RE-0041-acufixes-layout-spot-checks.md) (ACUFixes review), [ACU-RE-0046](ACU-RE-0046-player-teleport-and-vault-scan.md).
State: partially established. Static only; nothing was run against the live game. The item IDs
and the loadout write are established; the step that redraws the in-world model is not.

> **Corrected by [ACU-RE-0070](ACU-RE-0070-arnos-tailored-visual-pieces.md):** Arno's Tailored is `_02`,
> not `_03`. The AGM tier code puts Tailored at 3 and Master at 5. The mod's list moves the
> Master Assassin item to the end, so the +2 offset is off by one for Arno's three items. `_03` is
> Arno's Master, `_01` is Original and `_04` is Master Assassin. The table below is kept as first
> written.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40` (re-hashed this
  pass), image base `0x140000000`. The on-disk `.text` section is packed (raw size 0). Disassembly comes
  from the mapped image `local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`,
  where file offset equals RVA (capstone 5.0.3).
- Corpus (`ACU - Decompiled/`) files read in full: `140D2B240.c`, `140D2B370.c`, `140D33320.c`,
  `140D61560.c`, `140D61890.c`, `140D2FFD0.c`, `140D68F30.c`, `140D55970.c`, `140D55C10.c`,
  `141192EF0.c`, `141193170.c`, `14119E5D0.c`, `141E55230.c`, `140EF61B0.c`, `140D020A0.c`,
  `140851DA0.c` (lines 30–90), `141103820.c`, `140D52580.c`, `140D252A0.c`, `140D2E5D0.c`,
  `140D2E490.c`, `140D2F820.c`, `140D262C0.c`, `140F41B40.c`, `140884AC0.c`, `141119DC0.c`,
  `141DF4020.c`, `141DF44C0.c` (head), `140D325B0.c`, `140EA52A0.c` lines 1110–1142 and 1670–1699.
- `140854C50.c` decompiled as `JUMPOUT(0x13FBB0120)`. That is a hook artifact in the corpus capture,
  so `0x140854C50`–`0x140854F30` was disassembled from the mapped image instead.
- Asset index v2026-10-22.1 (`python tools/asset_query.py`). The item resources are type `c69075ab`
  `InventoryItemSettings` (confidence `crc-name`), stored in `DataPC.forge` entry 110 and duplicated in
  `DataPC_patch_01.forge` entry 0. Which duplicate the game loads is not established (#35). Links are
  byte-equality candidates.
- Localized names come from a third-party lead. The mod "Gear Visual Customization 365" 1.2.5 (an
  ACUFixes plugin; Nexus Mods page linked from its Readme), archive
  `Gear Visual Customization 365 1.2.5 2026-10-03T15-23Z RVNaTSuWk.zip`. Its license is unknown. Only
  its text files (`Readme.txt`, `VisualCustomization.ini`, and the gear-name table
  `装备名称对照表.txt`) were extracted, to a scratch directory, and read. Its DLL was not extracted,
  loaded or inspected. No code or table text is reproduced here; the mod is credited for the
  name-to-position facts below.
- ACUFixes by NameTaken3125 (review clone from ACU-RE-0041) supplied these leads: `AvatarGear`,
  `AvatarLoadout`, `AvatarGearManager`, `PlayerProgressionManager` and `PlayerCustomizationWorker`
  layouts; `0x140D2B240` "EquipGear"; `0x141DF44C0` "GetString". Each fact used below was re-derived
  from the corpus or disassembly; no code was copied.
- English `LocalizationPackage_English` (`00000000176ce0f5`, `DataPC.forge` entry 1528, payload 361,506
  bytes, SHA-256 `b5d55982…2aed`) is encoded. A plain or UTF-16 search for "Tailored" found nothing,
  so localized names could not be verified offline.

## Result

### Items (deliverable 1)

| Piece (localized, per mod table) | Resource name | File ID | Type |
|---|---|---|---|
| Arno's Tailored Hood | `ACU_AvatarGear_Head_03` | `00000011a8f258c7` | `c69075ab` |
| Arno's Tailored Coat | `ACU_AvatarGear_Chest_03` | `00000011a8f25ab2` | `c69075ab` |
| Arno's Tailored Bracers | `ACU_AvatarGear_Forearms_03` | `00000011a8f25bf5` | `c69075ab` |
| Arno's Tailored Belt | `ACU_AvatarGear_Waist_03` | `00000011a8f25b55` | `c69075ab` |
| Arno's Tailored Breeches | `ACU_AvatarGear_Legs_03` | `00000011a8f25c98` | `c69075ab` |

There are 5 pieces. Each gear slot has 42 entries in both the mod table and the index (`_01`–`_40`
plus `DLC_Raider` and `DLC_StDenis`), and no other "Arno's Tailored" piece exists.
For reference, "Arno's Original" is `_02` (Head `00000011a8f2593f`, Chest `00000011a8f25aae`,
Forearms `00000011a8f25bf1`, Waist `00000011a8f25b51`, Legs `00000011a8f25c94`).

**How the IDs were identified (inference, high confidence).** The mod lists items per slot. In that
list, position *i* (0-based) matches `ACU_AvatarGear_<Slot>_{i+2}` at 16 independent checkpoints.
Each checkpoint is a "[AFS]" or "[Gear][ULC]/[UPlay]" reward resource (type `60b663fb`) that links to
exactly one item:
- Tailored Prowler Coat and Belt sit at position 4 and link to `_06`.
- Tailored Sans-Culottes Hood and Trousers sit at position 8 and link to `_10`.
- Leipzig Hood, Prussian Waistcoat, Isenburg Belt and National Guard Trousers sit at position 11 and
  link to `_13`.
- Tailored Phantom Hood, Coat, Bracers, Belt and Breeches sit at position 19 and link to `_21`.
- Siege of Maastricht Hood, Athos Shirt, La Tour d'Auvergne Bracers and Black Musketeer Breeches sit at
  position 23 and link to `_25`.
- Tailored Brigand Hood and Bracers sit at position 31 and link to `_33`.

The mapping holds at every checkpoint and is consistent with a total of 42 entries per slot. Arno's
Tailored sits at position 1, which gives `_03`. Two corroborating signals:
- Cooperative-mission resources (`d9aa11b9`) link to the `_03` items: `COOP_LB030` to `Head_03` and
  `COOP_RB060` to `Chest_03`. This fits mission-reward gear.
- `AvatarLoadoutPresetManager_0X1D11EA7285` links to `Head_02` and `Chest_02`, which fits `_02` being
  the starting set.

The position-1 step itself is unchecked. Confirm it live by reading each gear's name UIString (below)
before equipping.

### Runtime structures (observed in corpus/disassembly)

- **PPM** = `[0x1451B3C58]`.
  - `PPM+0x1B8` is the current-character key slot (`0x140D61890` compares it).
  - `PPM+0x1C0` is the shared block of the player entity (`0x140D2B240` reads `[rip→0x1451B3C58]+0x1c0`).
  - `sub_140D61560(PPM)` returns the active `AvatarLoadout*`: it is `[charData+0xF8]`, or
    `charData+0x190` when that is null.
- **Shared block**: `[+0]` object pointer, `[+8]` int refcount, `[+0xC]` flags. The object is valid
  only when bit 31 of the int at `+0xC` is set (`movsxd/shl 0x20/sar 0x3f` mask).
  `[+0x10]` holds the resource file ID. This was observed twice:
  - `0x140D68F30` compares it with `0x1F5AA2BF7A` (`ACU_WM_Mortar_05`) and `0x283C02CE36`
    (`SDN_AvatarGear_Suit_Bonaparte`).
  - `0x140D325B0` switches on the loadout `+0x48` slot against `0x2393ECA558` (`ACU_ColorSet_07`).
- **AvatarLoadout slots**: `0x140D33320(loadout, gear.type@+8, gear.subtype@+0xC)` selects a slot.
  Type 0 maps to `+0x00`, 1 to `+0x08`, 2 to `+0x10`, 4 to `+0x20`, and any other value to `+0x18`.
  Type 5 picks `+0x28`/`+0x30`/`+0x38` by subtype, 6 picks `+0x40` and 7 picks `+0x48`. ACUFixes' slot
  names are waist, chest, forearms, head, legs, then weapons, outfit and color. The mapping from
  slot to body part is unverified.
- **AGM** (AvatarGearManager) = deref of the shared block at `[0x1451B4C58]+0x1970` (rip targets at
  `0x140854D02`/`0x140854D0D`). It holds three AvatarGear* arrays:
  - pointer at `+0x10`, u16 count at `+0x1A`;
  - pointer at `+0x1C`, u16 count at `+0x26`;
  - pointer at `+0x28`, u16 count at `+0x32`.

  `0x140D2FFD0(AGM, &blockSlot)` returns the gear whose `[gear+0x10]` equals the block pointer.
- **AvatarGear**: `+8` type, `+0xC` subtype, `+0x10` InventoryItemSettings shared-block pointer.
  - Observed in `0x140D2B370` and `0x140D2FFD0`.
  - ACUFixes lead, unverified: name UIString is a u32 at `+0x68`.
  - Localization: `LocMgr = [0x145219CF0]` (`0x141DF4020`); `0x141DF44C0(LocMgr, int id, wchar_t** out)`.

### Equip routine (deliverable 2)

`0x140D2B240(rcx = AGM (unused in body), rdx = AvatarGear*)`, x64 fastcall, returns nothing useful.
The disassembly shows `rcx` is never read. The routine does this:
1. Takes a reference on the `PPM+0x1C0` block.
2. Gets the loadout with `sub_140D61560(PPM)`.
3. Calls `0x140D2B370(loadout, gear)`, which swaps `[gear+0x10]` into the slot chosen by
   `0x140D33320` with interlocked refcounts. Type 5 (weapons) first calls `0x140D2B460`, which
   (un)equips weapon entities.
4. Calls `0x140F41B40(qword_1451C5B00, playerEntity, loadout)`, a telemetry record built with
   "0x%08x" / "AC%d" formatting in `0x140F54810`. It is not visual.
5. Calls `0x140C85280(55)`.

The routine does **not** check ownership.

The game's own equip button `0x140854C50` (disassembly):
1. Requires `0x140D68F30(gear+0x10, PPM+0x1B8)` to report "owned". That query goes through the
   player's inventory event object under lock `off_1451B4B40`.
2. Calls `0x140D2B240(AGM, gear)`.
3. Calls `0x140884AC0(owner, gearUIInfo)`. That routine calls `0x141119DC0(owner+0xB8, gear)`, which
   sets the worker bytes `+0x249` (dirty) and `+0x24A` (non-weapon); these offsets match ACUFixes'
   PlayerCustomizationWorker.
4. Calls `0x14148A480([0x1451EACA8], &entityRef)` and `0x1401D5E40(0x141099C60()+0x460)`.

When the menu closes and `+0x249` is set, `0x141103820` runs:
- `0x140D52580(PPM)`: re-equips weapon slots on the player entity, then runs `0x140D252A0(loadout,
  entity)` (stats, AI components, gear-modifier stats) and `0x140D64390`.
- `0x140D402E0(AGM)`.
- `0x140D71A80(PPM, 0)`: queues a job keyed by `PPM+0x1B8`. Purpose unknown; it may be a save or sync
  request.

**Ownership / give item.** The buy-and-equip path `0x140851DA0` calls
`0x140D55970(gear+0x10, PPM+0x1B8, 1, 1, 0, 1)` before equipping. That routine dispatches event
`dword_1451D0D38` (runtime value `0x43DB`) through `0x141192EF0` on `[component+0xB8]` of the player
(`0x141E55230`), which adds the item to the inventory. The debug item "Cycle through / equip player
cape" (`0x140EF61B0`, debug manager `0x140EA52A0` line 1693) uses the same dispatch. The debug item
"Give Player All Equipment and Abilities" is `nullsub_7` (stripped).

### Call recipe (game thread)

1. Get `AGM` and check it is non-null. Walk its three arrays. For each gear, take
   `blk = [gear+0x10]`. When the bit-31 test passes and `[blk+0x10]` is one of the five IDs, keep the
   gear. Optionally log `type@+8` and `subtype@+0xC`, and resolve `UIString [gear+0x68]` with
   `0x141DF44C0`.
2. Optional ownership step. It writes inventory (persistent):
   `0x140D55970(gear+0x10, PPM+0x1B8, 1, 1, 0, 1)`.
3. For each piece, call `0x140D2B240(AGM, gear)`.
4. If the model does not change, try `0x140D52580(PPM)` (the menu-close commit, without its save
   job). Then try the UI's `0x14148A480([0x1451EACA8], &[entity+0x130]-style ref)` only after
   identifying that entity live.

### Unknowns / risks

- No path for the visual redraw was found. Clothing types 0–4 only write the loadout slot, and no
  routine read in this pass applies meshes to the in-world entity.
- `0x140C85280(55)` and `0x140D71A80` may trigger save or network work. Their behaviour is unknown.
- Loadout writes and step 2 persist in the save. `0x140D262C0` scans and clears loadout slots by an
  item flag, which may undo items that are not owned.
- `AvatarGear+0x68` comes from ACUFixes only. The position-1 to `_03` step has not been checked live.

Reproduction: the asset queries are in the sections above (`asset_query.py sql` joins of `link` to
`resource`). The disassembly helper is a throwaway capstone reader over the mapped image.

## Development handoff
- **Confidence:** partial — static only; item IDs and the loadout write are established, the
  in-world model redraw is not, and nothing was run live.
- **Established:**
  - Item file IDs (type `c69075ab`, `crc-name`), Tailored `_03`: Head `00000011a8f258c7`, Chest
    `00000011a8f25ab2`, Forearms `00000011a8f25bf5`, Waist `00000011a8f25b55`, Legs
    `00000011a8f25c98` (the position-to-ID mapping is a high-confidence inference).
  - PPM `[0x1451B3C58]`; active loadout `sub_140D61560(PPM)`; slot choice `0x140D33320` by gear
    type/subtype; AGM from `[0x1451B4C58]+0x1970` with three AvatarGear* arrays.
  - Equip `0x140D2B240(AGM, gear)` swaps the slot with no ownership check; menu-close commit
    `0x141103820`; ownership add `0x140D55970(gear+0x10, PPM+0x1B8, 1, 1, 0, 1)`.
- **Not established:** the visual redraw path; effects of `0x140C85280(55)` and `0x140D71A80`
  (possible save/network work); slot → body-part mapping; `AvatarGear+0x68` UIString (ACUFixes
  only); the position-1 → `_03` step live; which item duplicate loads (#35).
- **Interface:** live tooling only, not product code (a game-thread equip call). Rust side
  unknown: the `c69075ab` item format is not decoded, so an `acu_assets` outfit slice needs its
  own finding.
- **First live run (2026-11-16, PID 131236, ACU-RE-0081):** `tools/native_probe/outfit.py`
  drives the focus shim's game-thread modes 6 and 7 (`tools/native_probe/focus.cpp`, RVAs from
  this finding; the visual-selection call `0x140D25540(copy of the active loadout, 0, 0)` follows
  a lead from the ACUFixes "Gear Visual Customization 365" 1.2.5 plugin, read statically, none of
  its code used). All five Tailored `_02` items were found among the AvatarGearManager's gears
  (types: head 3, chest 1, forearms 2, waist 0, legs 4). Mode 6 (visual selection) and mode 7
  (equip per item plus refresh `0x140D527F0`) both returned success without an exception. The
  model did not visibly change in screenshots after 0, 3 and 15 s. Inference: the pinned save
  already wears this set (its 587-particle flap body was stepping). A real change of outfit has
  not been observed, so the redraw path stays unestablished. The saves were pinned and put back.
- **Oracle:** not yet passed. Proposed by the call recipe: on the game thread, read each gear's
  `[gear+0x10]` block file ID and name UIString before equipping, read the loadout slots after
  `0x140D2B240`, and screenshot the model.
- **Spawned / follow-up:**
  - Find the routine that applies gear meshes to the in-world entity.
  - Determine what `0x140C85280(55)` and `0x140D71A80` do before calling them.
  - Confirm the `_03` mapping and `+0x68` live.
