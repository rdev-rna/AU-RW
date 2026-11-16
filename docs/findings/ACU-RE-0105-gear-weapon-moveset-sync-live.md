# ACU-RE-0105 — How does co-op sync outfits, gear, weapons and movesets?

Date / investigator / related task: 2026-11-16 / Claude (Opus 5.5), live two-game testbed plus static (IDA, capstone)
/ #85, items `PlayerInventoryData_NetProtocol`, `NetBehaviorAI_SetSelected*MSG`, `NetBehaviorAI_UnsheathWeaponMSG`
and the fight-input messages in `docs/coop/verdicts.json`
State: established for appearance and weapon choice; the moveset conclusion is established for what is sent (no
move or animation data). The meaning of each fight-input field is partly open.
- Established:
  - the whole loadout replicates as one compressed blob of asset references, sent when it changes;
  - the replica builds the look from its own game data;
  - the weapon slot in hand replicates as a small number, enforced on the replica each frame;
  - drawing a weapon is replayed on the other game;
  - attacks travel only as frame-stamped fight inputs, and both games apply the same records.
- Not established:
  - what consumes the blob on the replica (the routine that rebuilds the avatar);
  - the name and meaning of each fight-input field;
  - whether the replica's swing animation matched the owner's frame by frame. Game B's camera never framed player A
    mid-swing.

## Provenance

Binary path / SHA-256 / image base / VA / RVA:
- Running `ACU.exe`, SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40` (reported by
  `capture_run.py`).
- Expected hook bytes and static reads come from the captured mapped image, SHA-256
  `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, image base `0x140000000`. The probe checked
  16 bytes at every hooked site.
- IDB: `local/85-coop-ida/ACU.runtime-analysis.pe.i64`.

Corpus path and lines / export identity status: not used.

Archive identity / byte range / range hash / file ID (hex string):
- Every item named below is an `InventoryItemSettings` resource. They come from the same entry of `DataPC.forge`:
  offset 2438262, stored length 48507518, stored SHA-256 prefix `7d5f500049eeb221` (asset index).
- The type label is confidence `crc-name`: the CRC-32 of the name equals type ID `c69075ab`. Item names are the
  index's resource names.

Stored and decoded lengths / SHA-256 (ignored outputs in `local/coop/gear-20261116/`):

| File | Contents | SHA-256 |
|---|---|---|
| `gear_join.jsonl` | Join and object re-creation | `bfb6338e…c8f8` |
| `gear_change.jsonl` | Colour and hood changes | `b1151d3e…d195` |
| `weapon.jsonl` | Weapon change and draw | `20983a81…b3ec` |
| `fight.jsonl` | Fight-input apply counts | `91c124fc…c103` |
| `fightinput.jsonl` | 566 fight-input records with snapshots | `b4a6cf8b…a672` |
| `inv_blob_A.bin` | Blob before, 380 B stored, 1038 B decoded | `ab584bef…ffc1` |
| `inv_blob_A_after.bin` | Blob after the colour change | `1a2713a7…f138` |

Screenshots (`nav*.png`, `wB*.png`, `fA_*.png`, `fB_*.png`) are kept beside the logs.

Tool/provider/version / evidence IDs or saved output:
- The native probe (execution hooks, `READ`) and `capture_run.py` from branch `acu-multi-launch-direct`.
- `tools/coop_lan/gearwatch.py` (SHA-256 `1cc2dade…cb3b`).
- `tools/coop_lan/padnav.py` (`ae91cca2…ad40`): menu and fight input through the in-process pad.
- `tools/coop_lan/inventory_blob.py` (`4dd4b763…86a5`), with tests in `tools/test_inventory_blob.py`.
- IDA Pro (idalib, Hex-Rays) through ida-pro-mcp; capstone 5.0.3.

Exact commands and search boundary:
- Testbed per `tools/coop_lan/README.md`:
  - game A (the joiner) on cores 0–3 at 60 fps;
  - game B (the host, ports + 10) on cores 3–5 at 30 fps. On cores 4–5 alone it hung at "Checking for additional
    content" for over 10 minutes.
  - `privhost.py` on B, then `lancall.py` on A. Both loaded DEFEND Danton (`COOP_RB050`) with two players on the
    HUD.
- `privhost.py` and `lancall.py` re-use probe hook slots 0–3, which replaced gearwatch's hooks; these were re-armed
  by hand.
- Both games started from the same save, so both loadouts were identical before the changes.
- Changes were made on A through the pause menu: Character Customization → Gear Loadout → Equipment or Weapons.

Backend boundary: Storm's transport was not decoded (`docs/coop/BACKEND_BOUNDARY.md`).

## Result

### Observations

1. **The loadout replicates as the `PlayerInventoryData` memento** (wire id `06DF0B10`; bind `sub_1404F87A0`).
   - It has one field, a byte array at object `+0x28` with its count at `+0x30`. The memento stamp is at `+0x70`.
   - **Write** (`sub_140506020`, owner): copies the object's bytes into the memento through `sub_1426D07D0`. That
     routine marks the field dirty only when the length or the bytes differ, so an unchanged loadout is never
     re-sent.
     - Live, the owner ran the write about three times a second (for example, 383 hits on B in the first 120 s).
     - Only changes were sent.
   - **Read** (`sub_140505FF0`, replica): if the incoming stamp (`+0x50`) is at least the stored one (`+0x70`), it
     stores the stamp and replaces the bytes (`sub_1401BF220`). Nothing else happens in the read.
   - The same object registers `RespawnReplicaMSG` (handler `sub_140506040`). It was not seen in this run.
2. **The blob format.** The blob is a u32 decoded size followed by ACU's compressed frame, the same as Forge data:
   - magic `0x1004fa9957fbaa33`, version 1, algorithm 0, flags `0x8000`;
   - one LZO1X block, with an Adler-32 checksum whose sums start at 0.
   - **Example:** 380 bytes stored, a 348-byte block, 1038 bytes decoded.
   - The decoded object holds records with the type hash `0x157E5282`. Each record carries an
     `InventoryItemSettings` file ID. In loadout order:
     - `ACU_ColorSet_01`;
     - `ACU_AvatarGear_Forearms_02`, `_Waist_02`, `_Legs_02`, `_Chest_02` and `_Head_02`;
     - `ACU_WristCrossbow` and `ACU_WristCrossbow_Advanced`;
     - `ACU_BeserkDart_Weapon` and `ACU_BeserkDart_Advanced`;
     - `ACU_WM_Gun_50` and `ACU_WM_1Handed_23_SwordOfEden`.
3. **Live changes, A to B.** Each change was sent as the menu closed, not when the item was picked.

   | Change | Blob difference | Seen on B |
   |---|---|---|
   | Colour set "Assassin Blue" → "Peaceful Tales" | One reference, decoded `+0xCF`: `ACU_ColorSet_01` → `ACU_ColorSet_07` | The new stamp in the same 0.2 s probe poll; byte-identical; A's replica in white |
   | Head: Arno's Tailored Hood → a red hood | `+0x1E2`: `ACU_AvatarGear_Head_02` → `ACU_AvatarGear_Head_10` | 16 ms later; red hood on the replica |
   | Heavy weapon: Pickaxe | Weapon list `[Gun_50, 1Handed_23_SwordOfEden]` → `[2Handed_01, Gun_50]`; blob 380 → 383 B stored | 16 ms later; Pickaxe on the replica's back |

   - Heavy weapons are named `2Handed` in the data.
4. **The weapon slot in hand: `sub_1406D9170`**, run per frame for an entity of kind 1 (player avatar) at object
   `+1032`.
   - It reads three values from the entity's weapon component (`sub_141CCFAC0(+184)`):
     - the selected weapon (`sub_140EC5530`);
     - the selected equipment (`sub_140EC5470`);
     - the selected ranged weapon (`sub_140EC54D0`).
   - **On the owner:** when a value differs from the cached copy (`+1120`, `+1124`, `+1128`), it calls the matching
     `SetSelected*` handler: weapon `sub_1406D4E10`, equipment `sub_1406D4A90`, ranged `sub_1406D4C50`.
     `sub_1406D4E10` stores the new value and applies it through the component's listeners (`sub_140EC6460`).
   - **On the replica** (role bits `(+212 & 0xC0000) == 0x80000`): when the component disagrees with the cached
     copy, it calls the same functions with the cached value, the one received from the owner. The replica cannot
     drift from the owner's choice.
   - **Live:** equipping the Pickaxe ran `sub_1406D4E10(…, 3)` on A, then on B 16 ms later.
     - B ran it again about 1.0 s after the new blob arrived.
     - **3 is the selected-weapon value for the heavy slot.**
5. **Drawing the weapon is replayed.**
   - Holding X on A, out of combat, drew the Pickaxe. `sub_1406D8F50(…, 3)`, the `UnsheathWeaponMSG` handler, ran
     on A, then twice on B 31 ms later. Both calls returned to the same caller, `0x140222D92`.
   - B's replica held the Pickaxe in the two-handed grip.
   - Sixteen more draws happened in the fight on A, and 66 handler calls on B.
6. **Fight inputs carry no move or animation.**
   - `sub_1406EDB20` queues one fight-input record:
     - frame number = clock / 33.33 ms, or the message's own frame at `+92` when received;
     - payload at `+104`, passed to `sub_1406F2D70`.
   - During the fight both games logged the same records:
     - A: 284, flag `r8b` = 1;
     - B: 282, flag 0;
     - identical frame numbers 58945–59095 and identical field values per frame.
   - Fields seen (offsets from the message):

     | Offset | Seen |
     |---|---|
     | `+0x44`, `+0x48` | One id pair per member: (3803, 1084) and (3804, 1086), plus close values |
     | `+0x50` | A packed word: low byte `0x32` and one changing byte (`0x00`, `0x01`, `0x0A`, `0x0E`, `0xFB`, …) |
     | `+0x58` | 1 or `0x101` |
     | `+0x5C` | The frame |
     | `+0x70` | Two flag words, `0x4508385` and `0x4500385`, one per member |
     | `+0x74` … | Two copies of a unit 2-D direction, e.g. (0.208, −0.978) and (0.743, 0.669) |

   - No field holds an item ID, a weapon type, a move ID or an animation. The member-apply call
     (`sub_1406E2D20`, ACU-RE-0102) ran about 5,860 times on each game, at the same rate.
7. **`HumanAvatar_NetProtocol` is not appearance.**
   - Its u64 at `+0x18` toggled between `01fbfdfffffffffe` and `001b29fae7a18eea`, with no gear change. The other
     fields stayed constant: u32 5, byte 2, and eight floats −0.4, 0.11, 0.11, 0.18, 0.13, 0, 0, 0.
   - None of its fields changed with the colour, hood or weapon. It was re-received about 1 s after each gear change:
     correlation only.
8. **Side observations.**
   - About 404 s into the run, both games re-created their player objects. The replica first read an empty blob,
     stamp 0. The owner then re-sent the full blob. B's own blob briefly read a different value (`c0ef2c2c…`), then
     returned. This is a full-state send on re-creation (join in progress).
   - When A fought, B's objective changed to "KILL the Austrian spies (0/10)" and A's health bar dropped on B's HUD.

### Inferences

- **Appearance is data-driven from references.** The wire carries only file IDs of `InventoryItemSettings`. Each
  game resolves them in its own Forge data, so both games need identical data: the same build and patches. A
  missing ID on the receiver cannot be rendered. ACU has no fallback for this on the wire.
- **The moveset follows from the weapon, locally.**
  - The replica knows which weapon the owner holds from two things: the loadout blob (which item) and the selected
    slot (which one is drawn).
  - Fight moves are chosen by each game's own lockstep simulation from the shared inputs (ACU-RE-0102). The weapon
    class comes from local item data, so each game picks the heavy-weapon moves itself.
  - No move or animation crosses the wire. A desync in weapon data would therefore play different moves, with no
    detection; the fight CRC is debug-only.
- **The selection value is enforced, not just applied once.** The replica re-applies the owner's value every frame
  its component disagrees, so local logic cannot keep a different weapon drawn.

### Implementation choice

The library models a loadout as an owner-authored, versioned list of `(slot, item reference)`. The adapter maps
item references to the title's own items:
- AC4: outfits, weapons and the ship's upgrades;
- ACU: `InventoryItemSettings` file IDs.

It sends the whole list on change and on replica creation. The receiver applies it through an adapter call
`apply_loadout`. The selected slot is a separate small state with owner authority. Moves are never sent: fights use
the lockstep input model, and the adapter maps inputs to the title's combat.

Modern improvements over ACU:
- **Validate references on receipt.** Report unknown items instead of silently failing, and fall back to a default
  item.
- **Send a delta per slot** instead of a compressed copy of the whole loadout. ACU's blob is about 380 bytes, and
  one change touches 2 bytes.
- **Checksum the resolved weapon class** in the fight input stream, so a data mismatch is detected.

### Unknowns and next checks

- **The blob consumer on the replica.** Find who reads object `+0x28` after `sub_140505FF0` (a read watch, or
  xrefs to the object's class methods). Then decode the record layout beyond the item ID (the slot index field).
- **Fight-input fields.** Name `+0x44`/`+0x48` (member ids), the `+0x50` action byte and the `+0x70` flags. Press
  each button on the owner while logging, and compare.
- **Frame-accurate animation on the replica.** Frame A from B's camera during a fight and log the clip played on
  both games. Expect identical moves on the same frames.
- **The value of `SetSelected*` for each slot.** Change to one-handed, long, pistol and rifle. 3 was seen for heavy.
- **`RespawnReplicaMSG`.** When it is sent (respawn, or re-creation).

## Reproduction status

Live, one session, both directions of the testbed (A is the owner of its avatar; B holds the replica).
- The blob decode and item names reproduce from the saved logs: `inventory_blob.py items|diff`.
- The static addresses come from the same capture.

### Reproduction steps

1. Set up the testbed (`tools/coop_lan/README.md` steps 1–4, custom loader), with both games in a co-op mission.
2. Run `gearwatch.py out.jsonl <pidA> <pidB> --seconds 900`. Re-arm the hooks after `privhost.py`/`lancall.py`.
3. On A: `padnav.py <pidA> s.png START DPAD_DOWN A …` into Gear Loadout. Equip an item, then leave the menu.
4. `inventory_blob.py diff` on the blobs before and after. Check B's screen with `padnav.py <pidB> b.png wait:0.2`.
5. For weapons, add `--extra <pidB>:0:setsel_weapon:6D4E10:1:0:16 --extra <pidB>:1:unsheath_weapon:6D8F50:1:0:16`.
   For fight inputs, hook `0x6EDB20` with a snapshot at `rdx+0x40` on both games during a fight.
