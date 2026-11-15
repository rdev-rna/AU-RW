# ACU-RE-0075: How does ACU replicate a human avatar, and how are all message IDs formed?

Date / investigator / related task: 2026-11-15 / Claude (Opus 5.5), static only / #85, co-op plan Phases 3/4 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md))
State: partially established. The wire-ID rule is established for all 963 RFC types. The avatar snapshot
codec and the intention-message model are observed. The field meanings and the owning entity class are
unresolved.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: same capture and analysis view as ACU-RE-0072 and 0073:
mapped image `9a1124f1…0520`, analysis view `47a81dc9…d8c2`, image base `0x140000000`.
Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable.
Tool/provider/version / evidence IDs or saved output:
- IDA Pro 9.3, ida-pro-mcp idalib, headless session `acu85`, on the private IDB copy. Read only, closed
  without saving, lock #31 held.
- Capstone 5.0.3, used offline to disassemble the encoder windows from the mapped image.
- `tools/net_protocol_atlas.py`, now with wire IDs (tool SHA-256
  `ee78206b67c0b5184d15aace77741fd497b8353db6cb26263f9ab5a8443c80c3`), writes
  `local/coop/net_protocol_atlas.json`. Two runs produced identical output.

Exact commands and search boundary: the atlas command is in ACU-RE-0072. The ID check searches the whole
`.text` section for each computed 32-bit value.

## Result

### Observations

1. **Universal wire-ID rule.** At the `NetHumanIntention` build branch (`0x140721F7A` / `0x140721F83`), the
   message object receives ID `0xDC303C6B` (broadcast) or `0x572CC457` (unicast). A brute-force CRC-32 over
   every image string, prefixed onto `AssassinateTargetMSG_NetProtocol_ID`, finds exactly one match each:
   `zenitar_NetHumanIntention_AssassinateTargetMSG_NetProtocol_ID`, and the same with `_UNICAST`. The
   runtime prefix built by the sender's virtual (ACU-RE-0072 observation 7) is therefore
   `zenitar_<Owner>_`, fixed per owner type, not per instance.
   - Applying `CRC-32("zenitar_" + <type name> + "_ID")` and the `_UNICAST` variant to every RFC type
     record: for **963 of the 963** `…MSG_NetProtocol` types, both values occur as constants in `.text`.
     A control with a wrong suffix (`_IDX`) matched 0 of 300.
   - Snapshot (`NetObjectMemento`) types: the broadcast ID occurs for 110 of 118, and the unicast ID for
     only 1.
2. **Avatar snapshot type** `HumanAvatar_NetProtocol` (record `0x145139D70`, base `NetObjectMemento`).
   It has a four-entry hash-keyed descriptor block at `0x1439E5B80`:
   - key `0x87A343CA` → thunk `0x140135FD0` → `sub_140176C60` (registers the snapshot);
   - key `0xEDDC8481` → `sub_140136080` (role unknown);
   - key `0x4F70EE5E` → `sub_1401363B0` (is-a check).

   `sub_140176C60` hands a pair `{sub_140172F20, sub_140173180}` to the Storm registry under key
   `0xA92DBEA6`, which is the snapshot's own wire ID (corrected in ACU-RE-0076; an earlier draft had a
   hand-conversion error, `0xA92E6CA6`), registers the record hash through registry virtual `+0x88`, and
   stores the registry pointer at `owner + 0x58`.
3. **The snapshot codec.** "Source" means the game-side object being replicated; "slot" means a field slot
   in the snapshot object. Each slot holds a pointer to a changed flag at `+0x00`, a changed byte at `+0x08`
   and the value at `+0x10`. Quantized slots also hold the integer code at `+0x18`, the last float at
   `+0x1C` and the bit width at `+0x20`.
   - **Write** (`sub_140173180`, source → snapshot):

     | Source offset | Type | Slot | Encoding |
     |---|---|---|---|
     | `+0x10` | u8 | `+0x60` | raw |
     | `+0x18` | u64 | `+0x80` | raw |
     | `+0x20` | u64 | `+0xA0` | raw |
     | `+0x28` | u32 | `+0xC0` | raw |
     | `+0x2C` | u8 | `+0xE0` | raw |
     | `+0x30` | f32 | `+0xF8` | quantized by `sub_14018BF70`, range [-2, 2] |
     | `+0x34` to `+0x4C` | 7 × f32 | `+0x120` to `+0x210` (stride `0x28`) | quantized by `sub_14018BE90`, range [-1, 1] |

     Raw slots are marked changed only when the value differs. Quantized slots are marked changed only when
     the value moves by at least 0.001. The [-2, 2] quantizer stores `trunc(v·0.5·mask)` and the [-1, 1]
     one stores `trunc(v·mask)`, where `mask = (1 << bits) − 1`; both saturate at the ends of their range.
   - **Read** (`sub_140172F20`, snapshot → source): runs only when the snapshot's sequence number
     (`+0x50`) is at least the source's (`+0x50`). It copies the raw fields and decodes the quantized ones as
     `code·2/mask` and `code/mask`.
4. **The avatar's neighbouring state types**, each with its own RFCs:
   - `AnimComponentNetData_NetProtocol` (`0x145139A10`, the same five-helper pattern);
   - `PlayerCharacterData` with `UpdateCharacterParams`;
   - `LocalPlayerData` with `UpdateAvatarSkills`, `UpdatePlayerMorphingState`, `UpdatePlayerCoopHealing`,
     `StopBoosterRFC` and `PlaySFXBasedOnMetaStat`;
   - `PlayerInventoryData` with `RespawnReplica`;
   - `ZenHealthReplication` with 10 RFCs, among them `RFCSetMasterLife`, `RFCSetMasterMaxLife`,
     `RFCSetMasterDamageDealer`, `RFCSetMasterGodMode`, `SetIsKO_RFC`, `PlayHealFX_RFC` and
     `RFCSetReviveTimerStartNetTime`;
   - also `PlayerOnlineData`, `UIPlayerInfo` and `PlayerInfo`.
5. **`NetHumanIntention` has 94 messages,** all high-level actions. By name: about 23 are combat (fire,
   aim, assassinate, sheathe and unsheathe, throw a bomb), 14 movement (navigate to an entity, patrol point
   or nav target, follow, wander, pause or resume navigation, flee on crowd flow), 7 synchronized custom
   actions and group behaviour, and 3 animation cancels. The rest are cover, look-at and orient, hurt,
   stumble, shove and tackle, reins, facial animation, incapacitated, berserk and smoked.
6. **Sending looks the same for the avatar's messages as elsewhere.** `sub_1407187A0` (`AssassinateTarget`)
   runs the local body `sub_140709890` when the net handle at `+0x718` is null. Otherwise it sends through
   `sub_1405D17F0` *(corrected in ACU-RE-0080: this is StartIntention on the network-aware manager, not
   a raw send)*.
7. **Resync function** *(corrected in ACU-RE-0080: this is the receive handler's main chunk, not a
   resend loop; the fields below are slot values of the received message)*. The large function containing `0x140728B1C` and `0x140729376` walks the intentions
   one at a time with predicates such as `sub_1405F59C0` and `sub_1405F49D0`, and re-sends each active one
   with its current parameters. For example, `SetRiftWeaponAimingAngle` is re-sent with the floats at
   `+0x94`, `+0xBC` and `+0xE4`. `AssassinateTarget` is re-sent with `{u32 at +0xF0, bool (byte at +0x110
   equals 1), u8 at +0x130}` plus a value built by `sub_1414682D0`.

### Inferences and alternatives

- **Model: the master simulates, replicas re-enact intentions.** The owning peer runs the character and
  broadcasts high-level intentions ("navigate to X", "assassinate Y"). Each replica re-enacts them locally,
  and snapshots carry continuous state. `NetHumanIntention` covers NPCs as well as players (patrol and
  guard messages), so the same mechanism is expected for both.
- **Authority rule from the names.** A replica asks the master through `RFCSetMaster*` and
  `SendFightDamageEventToMaster`. The master applies the change and replicates it through the snapshot.
  This matches Storm's master-role events (ACU-RE-0073 observation 3).
- *(Superseded by ACU-RE-0080.)* **The resync function is probably for join-in-progress or master migration.** It rebuilds a replica's
  intention state from scratch. The alternative is periodic reliability resends.
- The 8 quantized floats of `HumanAvatar` are probably locomotion or blend parameters (speed, direction,
  lean and similar). This is not established: no game-side writer of the source fields has been read.

### Unknowns / incomplete coverage

- The game class that owns a `HumanAvatar_NetProtocol` snapshot is not known. It is reached through the
  type-keyed factory, so no direct caller exists. A live probe would settle it.
- What the source fields mean, and the per-slot bit widths, which are set outside the codec. (The
  registry key is the wire ID `0xA92DBEA6`; see ACU-RE-0076.)
- The fields of each intention payload. Resolved for layout in ACU-RE-0079 (all 94); meanings are open.
- Position and transform replication. No position fields appear in `HumanAvatar`. Resolved in
  ACU-RE-0076: position travels in the dead-reckoned `ProxiesProtocol` pool, not in
  `AnimComponentNetData`.

### Independent comparison / falsification test

The ID rule passed a test across the whole set: 963 of 963 against 0 of 300 for the control. A single
`…MSG` type whose IDs were missing from `.text` would have broken it. The snapshot read and write
functions mirror each other field by field: the same offsets, the same order, and inverse quantizations.

### Reproduction steps

1. Run the atlas, then check `summary.wire_ids` and each record's `wire_id`.
2. Decompile `0x140176C60`, `0x140172F20`, `0x140173180`, `0x14018BF70`, `0x14018BE90` and `0x1407187A0`.
3. Disassemble `0x140721EFC` and `0x140729300`. Read only.

### Implementation implications and finding-linked constants

- Our transport can compute every message ID offline: `CRC-32("zenitar_" + type + "_ID")`, with
  `_UNICAST` appended for unicast. The atlas lists them.
- The `HumanAvatar` codec in observation 3 is the first concrete replicated-state schema to implement. It
  has 5 raw fields, 8 quantized floats, a sequence-gated apply, and changed flags with a 0.001 threshold.
