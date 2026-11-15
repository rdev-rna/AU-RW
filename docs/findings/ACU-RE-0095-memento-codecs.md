# ACU-RE-0095 — What does every co-op state type (memento) carry?

Date / investigator / related task: 2026-11-16 / Claude (Opus 5.5), static, offline with capstone / #85, coverage
ledger phase 2 ([../coop/COVERAGE.md](../coop/COVERAGE.md))
State: partially established. The codec binding, field layout and newest-wins stamp are decoded for 112 of the
118 memento types. What each field means is not; 6 types have no binding found.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: captured mapped main image
`motion-link-20261020/runtime-view/capture/main-image.mapped.bin`, SHA-256
`9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, image base `0x140000000` (the capture used by
ACU-RE-0072 to 0084 and 0093).
Corpus path and lines / export identity status: not used; disassembly only.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: output `local/coop/memento_codecs.json`, SHA-256
`bae4ccc2e410c5348a74e127f34ed1f5e32040f1d2e5c683c9e40c32cebc1ace`. Two runs were byte-identical.
Tool/provider/version / evidence IDs or saved output: `tools/net_memento_codecs.py`, SHA-256
`2966abe7f252e00caf6c3e391632654d9d34746070ed8827f86b01108a8039a9`, capstone 5.0.3. Synthetic tests:
`tools/test_net_memento_codecs.py` (8 tests, `377c0f42…`), run with `python -m unittest tools.test_net_memento_codecs`.
Exact commands and search boundary: every type record whose ancestors include `NetObjectMemento` in
`local/coop/net_protocol_atlas.json` (ACU-RE-0072): 118 types. The search space is the image's `.text`.

```powershell
python tools/net_memento_codecs.py --image <capture> --atlas local/coop/net_protocol_atlas.json `
  --expect-sha256 9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520 `
  --output local/coop/memento_codecs.json
```

## Method

1. **Binding.** ACU-RE-0084 showed how `HumanAvatar` binds its codec. The same shape holds for every type:
   - a bind function stores the broadcast wire ID as an immediate;
   - before the ID it loads the read delegate (`lea rdx, [rip+read]`);
   - after the ID, before the registry's virtual call, it loads the write delegate (`lea r8, [rip+write]`).

   The tool finds the ID's code sites, then the bind function among them. Many delegates are leaf functions with no
   `.pdata` entry, so the delegate addresses are taken from the `lea`s, not from the function table.
2. **Write delegate** (owner in RCX, memento in RDX). Every slot is marked changed the same way:
   `mov rax, [m+K]; mov byte [m+K+8], 1; mov byte [rax], 1`.
   - **Slots.** K is the slot. The value store next to it, `[m+V]`, gives the value offset (`V-K`) and the type. The
     register it stores gives the owner field.
   - **Setter calls.** Other slots are filled by setter calls: `lea rcx, [m+K]`, with the value or its address in
     RDX.
   - **Nested writers.** Some writers call a nested writer for an embedded struct, or loop over an element writer
     for an array of sub-mementos. These are decoded recursively. Element fields are written as
     `+0x80[]/0x60`: slot `0x60` of each element of the sub-memento array whose pointer is at memento `+0x80`.
   - **Slot objects with a vtable.** Some slots have a vtable at +0 and their flag pointer at +8. They are reported
     from their start, with `"vtable": true`.
   - **Function ends.** Decoding follows jump targets past a `ret`, because shrink-wrapped writers have several
     `.pdata` chunks.
3. **Read delegate, as an independent check.** The read delegate copies memento values back into the owner. The
   tool decodes its inline copies separately and compares each write-side inline field (`K + value offset` → owner
   field) with the read side. It also finds the stamp rule: `mov e, [m+0x50]; cmp e, [o+T]; jb` (ACU-RE-0084).

## Result

### Observations

1. **112 of 118 types decode; 1,293 fields.**

   | System | Types | Decoded | Fields | In sub-memento arrays |
   |---|---|---|---|---|
   | Mission coordinators (`GcL*`) | 32 | 32 | 666 | 213 |
   | Rift | 22 | 22 | 217 | 48 |
   | NPC AI, navigation and intentions | 13 | 12 | 133 | 0 |
   | Player and NPC character (`NetPlayer`/`NetNpc` set) | 7 | 6 | 60 | 0 |
   | Awareness | 8 | 8 | 55 | 43 |
   | Player data | 9 | 8 | 52 | 0 |
   | Crowd | 7 | 4 | 46 | 14 |
   | Missions (Domino, events) | 11 | 11 | 27 | 7 |
   | Combat | 5 | 5 | 21 | 13 |
   | World objects and other | 4 | 4 | 16 | 0 |

   Field types:

   | Type | Count | Notes |
   |---|---|---|
   | u32 | 375 | |
   | bool | 373 | |
   | vslot_u32 | 218 | Virtual slot (`0x141BB38D0`): get `+0x38`, compare, set `+0x50`; value in EDX. Some hold element counts: `GcLDefendNetData` `0x58` is the count its writer loops over for `+0x80[]`. |
   | u64 | 99 | |
   | f32 | 74 | Raw float. Inline float slots keep the value at `+0x14`, or at `+0x1C` in vtable slots. |
   | u16 | 53 | |
   | vec3_f32 | 46 | |
   | u8 | 19 | |
   | array | 10 | `0x1426D07D0` (slot, pointer, count) |
   | f32_q1 | 8 | Quantised floats (ACU-RE-0075) |
   | quat | 6 | `0x141C70970` normalises a quaternion, then quantises it |
   | f32_q2 | 1 | Quantised float (ACU-RE-0075) |
   | Other | 11 | 3 unpaired slots, 8 calls to unclassified helpers |

2. **The read side agrees.**
   - Of the write-side inline fields, 550 have the same owner field in the read delegate.
   - 5 differ, all in Domino operator lists, where the read delegate writes through an adjusted owner pointer
     (`-0x8`).
   - 97 are not read back inline: the read side uses a call or a converted value.
3. **Every memento applies newest-wins against an owner field.**
   - 109 read delegates start by comparing the memento's stamp (`+0x50`, the session time, ACU-RE-0084) with an
     owner field, and skip the memento if it is older.
   - The owner field is `+0x50` for 38 types; for the others it varies (`+0x28`, `+0x20`, `+0x244` …).
   - It is listed per type as `read_check.stamp_owner_field`.
4. **Known layouts reproduced.**
   - `HumanAvatar` gives the 13 slots of ACU-RE-0075 exactly: same slots, types, sources and quantisers.
   - `ZenHealthReplication` (`NetPlayer` and `NetNpc`) has 8 slots:
     - `0x60` and `0x80`: u16 from `+0x18` and `+0x1A`, each stored **plus one**; the read delegate subtracts one;
     - `0xA0`: bool `+0x1C`;
     - `0xC0`: u64 `+0x28` (the read side turns it into a reference-counted handle);
     - `0xE0`: bool `+0x38`;
     - `0x100`: u16 `+0x3A`;
     - `0x120`: u32 `+0x3C`;
     - `0x140`: bool `+0x40`.
   - The +1 storage fits a life value of −1, seen live in ACU-RE-0092, being carried as 0.
5. **Mission coordinators carry most of the state.**
   - The 32 `GcL*` coordinator mementos hold 666 fields. 213 of them sit in arrays of sub-mementos, each written by
     an element writer.
   - Example: `GcLDefendNetData` has 19 top-level slots plus three element arrays: `+0x80[]` (6 fields from a
     0x30-byte owner element), `+0x170[]` and `+0x2B0[]`.
   - Their read stamps are mostly `+0x50`.
6. **Gaps in the ledger that this fills:**
   - `NetHumanIntention_Activated_Navigation` (58 fields) and `NetHumanIntention` (24 fields) are the intention
     system's state;
   - `AwarenessDirector` (24 fields, 22 in arrays), `AlarmBellNetDirector`, `AwarenessPostConflict` and
     `CSLineOfSightNetData` cover awareness;
   - `FightDirector` (12) and `FightHostilityManager` (3) cover combat;
   - `NetEntityNavigation{Flee,Flow,Wander,Formation}` (9–12 each) cover NPC movement modes;
   - `RiftBallisticWeaponComponent` (14) and seven Rift weapon components cover turrets;
   - `UIPlayerInfo` (33), `LocalPlayerData`, `PlayerOnlineData`, `PlayerCharacterData` and `PlayersNetData` cover
     player data.
7. **Small mementos are genuinely small.**
   - `FighterNPC` writes one byte (`+0x2F0`).
   - `NetAnimalIntention` writes one bool.
   - `CharacterAI` writes two computed u64.
   - Their behaviour travels in messages instead (ACU-RE-0093).
8. **Not decoded (6):**
   - `NetBehaviorAI_NetProtocol`: the wire ID only occurs inside the large NBAI functions, with no bind shape;
   - `AIComponent_NetProtocol`: sites in two registration helpers only;
   - `LivingBreathingManager`, `VillaTrophyManager`, `GcLCrowdLifeProtocolContainer` and `ProxiesProtocol`: the
     wire ID is not in `.text`.

   `BhvGenericNPC`, `CharacterAI` and `CharacterBehavior` decode only under the wire IDs that ACU-RE-0084 found at
   their registration, not the `zenitar_` CRC.

### Inferences and alternatives

- **A peer's world state is the sum of its mastered mementos.** Players, NPCs, coordinators, Rift objects, awareness
  and fight directors each keep their state in one memento. Each memento is refreshed every network tick and
  applied newest-wins. This is the state a late joiner, or a peer taking ownership, needs. Messages (ACU-RE-0093)
  carry the events between snapshots.
- **Arrays of sub-mementos are how coordinators scale.** A coordinator sends its per-NPC or per-objective entries
  as an element array whose count is a virtual slot. A port needs variable-length arrays with per-element change
  flags.
- **The +1 life encoding** probably lets the wire's unsigned u16 carry −1 ("dead" or "unset") as 0. An alternative
  is a serializer that reserves 0. Not verified.

### Unknowns / incomplete coverage

- Field meanings. The tool gives slot, type and owner offset only.
- The 6 undecoded types and the 11 unclassified fields (helpers `0x1403FE450`, `0x1402B8820`, `0x14059E9D0`,
  `0x14059EAC0` and `0x14059EBB0`; three unpaired slots).
- Bit widths and quantisation ranges, which are set when each memento is constructed (ACU-RE-0084).
- How Storm serialises the changed fields, and their refresh rate and thresholds (inside Storm).
- Element-array sizes and their bounds.

### Independent comparison / falsification test

- `HumanAvatar` matches ACU-RE-0075, which was decoded by hand, slot for slot.
- 550 inline fields agree between write and read delegates. The read side is decoded by a separate pass that shares
  only the disassembler.
- The +1/−1 pair in `ZenHealthReplication` appears in both delegates.
- Synthetic tests cover:
  - the compare-and-set form;
  - register-compare bools;
  - increments;
  - setter tail-jumps through saved registers;
  - code after `ret`;
  - nested element writers;
  - the read decoder's stamp and pairs, and the cross-check counts.
- Live check still to do: hook a memento write on the testbed (for example `ZenHealthReplication` on a damaged NPC)
  and compare the slot values with the owner's fields.

### Reproduction steps

Run the command above with the capture image and the atlas from ACU-RE-0072. Look up a type in
`local/coop/memento_codecs.json` → `mementos[]`, which gives its `bind`, `read`, `write`, `fields` and
`read_check`.

### Implementation implications and finding-linked constants

- **For a port:** each replicated object type gets a state record. Its fields are typed scalars, vectors,
  quaternions, arrays, and arrays of sub-records. Each field carries a changed flag. The record is stamped with the
  session time at each refresh, and the receiver applies it only if the stamp is newer. The layouts here size every
  record: 1,293 fields over 112 types.
- **Modern improvements over ACU's scheme:**
  - delta-compress per field against the last acknowledged state, not the last sent state;
  - send element arrays as keyed adds, removes and changes, instead of positional slots;
  - carry −1 in a signed or optional field instead of the +1 trick;
  - prioritise records by distance and relevance instead of refreshing all of them every tick.
- Constants:
  - setters `0x141BB38D0` (vslot), `0x1426D07D0` (array), `0x141C70970` (quat), `0x14018BE90` and `0x14018BF70`
    (quantised floats), plus the message setters of ACU-RE-0079;
  - stamp at memento value `+0x50`;
  - invoker thunk `0x1401B78D0`.
