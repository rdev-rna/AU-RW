# ACU-RE-0093 — What does every co-op message carry?

Date / investigator / related task: 2026-11-16 / Claude (Opus 5.5), static, offline with capstone / #85, coverage
ledger phase 1 ([../coop/COVERAGE.md](../coop/COVERAGE.md))
State: partially established. Every message's field layout (slot, type, source offset) is decoded. What each field
means is not.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: captured mapped main image
`motion-link-20261020/runtime-view/capture/main-image.mapped.bin`, SHA-256
`9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, image base `0x140000000` (the same capture as
ACU-RE-0072/0079).
Corpus path and lines / export identity status: not used; disassembly only.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: output `local/coop/message_payloads.json`, SHA-256
`9e688df7fda2d9477203fceaabbc2b512d0a32b88a18fe61ffb94b44e228882c`. Two runs were byte-identical.
Tool/provider/version / evidence IDs or saved output: `tools/net_intention_payloads.py`, SHA-256
`d136f945bb02a0d47d7a23de9da4b40146773cd9ed8446cc01e07dabc40b2e91`, capstone 5.0.3, with the new `--all` and
`--systems` options. The tool's 7 unit tests pass. With no options it reproduces ACU-RE-0079's 94
`NetHumanIntention` messages exactly.
Exact commands and search boundary: every `*MSG_NetProtocol` type record in `local/coop/net_protocol_atlas.json`
(ACU-RE-0072); the search space is the image's `.text`.

```powershell
python tools/net_intention_payloads.py --image <capture> --atlas local/coop/net_protocol_atlas.json `
  --expect-sha256 9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520 `
  --all --systems local/coop/coverage_systems.json --output local/coop/message_payloads.json
```

## Result

### Observations

1. **One builder form covers every message family.**
   - ACU-RE-0079 described the `NetHumanIntention` builder: each block stores the message's broadcast wire ID,
     then copies the source object's fields into `{flag, changed, value}` slots.
   - The same search finds exactly one block for each of the 963 `*MSG_NetProtocol` types, in every family:
     `NetBehaviorAI`, `FightDirector`, `NetFightManager`, `AwarenessDirector`, `StimulusDirector`, Domino, every
     `GcL*` coordinator, `ZenHealthReplication`, crowd, Rift and player data.
   - All 963 blocks are distinct: 709 of form A and 254 of form B.
   - The `NetBehaviorAI` blocks all lie between `0x14069F2EF` and `0x1406ADDAA`, inside the builder `0x14069F200`
     that ACU-RE-0092 hooked live.
2. **4,617 fields in all.**

   | Type | Fields |
   |---|---|
   | u32 | 1,586 |
   | u8 | 1,035 |
   | u64 | 603 |
   | vec3_f32 | 564 |
   | bool | 543 |
   | f32 | 191 |
   | entity handle | 47 |
   | u16 | 27 |
   | vec3_q4000 | 14 |
   | bool_bit | 6 |
   | f32_q7 | 1 |

   - Only 2 fields have no source.
   - 949 messages start with the same two slots: `0x38 u32 ← +0x8` and `0x58 u8 ← +0x14`. For intentions, `+0x8`
     is the intention id that the builder compares (ACU-RE-0079).
   - 271 messages carry only those two slots plus nothing else; they are pure events.
3. **Fields per system:**

   | System | Messages | Fields | Header-only |
   |---|---|---|---|
   | missions | 260 | 824 | 84 |
   | player behaviour | 171 | 1,457 | 45 |
   | combat | 104 | 477 | 33 |
   | intentions | 94 | 549 | 25 |
   | crowd | 88 | 394 | 17 |
   | awareness | 83 | 386 | 4 |
   | interactions | 40 | 116 | 18 |
   | Rift | 39 | 143 | 13 |
   | NPC AI | 25 | 91 | 4 |
   | mission targets | 15 | 53 | 5 |
   | PvP | 13 | 33 | 10 |
   | player data | 12 | 45 | 4 |
   | health | 10 | 29 | 1 |

4. **A cross-check against live evidence.** `ZenHealthReplication_RFCSetMasterLife` carries one value field,
   `0x78 u16 ← +0x28`. That matches the 16-bit local life the master keeps (ACU-RE-0092 obs 11), and the values
   seen live (30, 22, 14, 6, −1, 0, 15, 60).
5. **Examples that show what the payloads look like:**
   - `NetBehaviorAI_ParkourLeap` and `ParkourClimbLeap` carry 14 `vec3_f32` fields (`+0x30`…`+0x110`), seven
     bools, five u32 and four derived u64. (An earlier version of this finding said 15; ACU-RE-0094 corrects it.)
   - `NetBehaviorAI_NetStartMovement` carries one `vec3_f32` and a bool.
   - `NetBehaviorAI_EnterRevive` carries a u64 (`+0x28`) and an f32 (`+0x30`).
   - `NetBehaviorAI_AssassinateNet` / `…FromHayStack` / `…FromHidespot` share one layout: three derived u64, a u32,
     a bool and a u8.
   - `NetHumanIntention_NavigateOnVehicleToPatrolPoint` (carriages) carries six `vec3_f32`, two derived u64, five
     f32, four u32 and three bools.
   - `NetHumanIntention_FlickReins` is header-only.
   - `ZenHealthReplication_SetIsKO_RFC` carries one bool.
6. **Eleven messages decode with no fields at all,** not even the header:
   `CrowdHerderGroupMigration_TransferGroup`, `FightDirector_RFC_RequestLoadBalanceMigration` and nine
   `FightDirector_*_RFC` lock and target messages (`RequestFightTargetMeleeLock`,
   `FinishedUsingFightTargetMeleeLock`, `ReleaseFightTargetMeleeLockBy`, the two ranged-aiming lock messages,
   `ConfirmMeleeAttack`, `ResetFightMemberCurrentAction`, `SetTargetForAutoJoinFight`, `FighterTargetChanged`).
   Their blocks were found, but the copies use a pattern the decoder does not parse.

### Inferences and alternatives

- **Parkour sends its trajectory.** The 14 vectors in the parkour leap messages suggested that the master sends the
  points of the jump. ACU-RE-0094 tested it live: the message carries the position at send, the takeoff point and the
  predicted landing point, with a direction for each. It is not a sampled path.
- **Ownership migration has its own messages.** `FightDirector_RFC_RequestLoadBalanceMigration` and
  `CrowdHerderGroupMigration_TransferGroup` are candidates for the code behind the hand-over measured in
  ACU-RE-0092 obs 18–20. They decode empty, which may mean their payload is built differently. They are first in line
  for manual reading.
- **The `+0x8` / `+0x14` header is generic.** In intention messages it is the intention id and its byte. For other
  RFC families the decoder still sees the same source offsets, so it is likely a shared base (an object id and a
  flag). Not verified.

### Unknowns / incomplete coverage

- What every field means. The decoder gives slot, type and source offset, not a name.
- The 11 empty messages.
- How the replica *applies* each message. That needs each family's receive handler. For intentions it is
  `0x1407269B0` (ACU-RE-0080); other families are not traced.
- "Derived" sources (603 u64 and others) are computed values, often entity handles. Their origin needs per-block
  reading.
- The 118 state types (mementos) are not covered here. That is phase 2.

### Independent comparison / falsification test

- With no options, the tool reproduces ACU-RE-0079's output exactly. Diffing `nhi_check.json` against
  `intention_payloads.json` gave identical `messages`.
- `RFCSetMasterLife`'s single u16 matches the live life values from ACU-RE-0092.
- Next live check: hook a few `NetBehaviorAI` blocks (`ParkourLeap`, `AssassinateNet`, `EnterRevive`) on the testbed,
  then compare the decoded slots with what the master does.

### Reproduction steps

Run the command above with the capture image and the atlas from ACU-RE-0072. `coverage_systems.json` comes from the
classification in `docs/coop/COVERAGE.md`.

### Implementation implications and finding-linked constants

- A port can now size and serialise every ACU message: 963 layouts with field types and the quantisations
  `vec3_q4000` and `f32_q7` from ACU-RE-0079.
- The next phases give the layouts meaning: the receive handlers per family, the mementos, then live scenarios.
- Builders: `0x14071F090` (`NetHumanIntention`) and `0x14069F200` (`NetBehaviorAI`). The other families' blocks are
  listed per message in the output (`block`).
