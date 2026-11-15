# ACU-RE-0091 — Which ACU co-op mechanisms answer the AC4 co-op mod's open problems?

Date / investigator / related task: 2026-11-16 / claude-session, static / #85, co-op plan Phase 4 (combat, AI) and
Phase 6 (portability). Input: the AC4 co-op mod's open problems, read from its own repository (`AC4Coop`, branches
`feat/launcher`, `docs/pose-sync`): `docs/design/unity-netcode-mapping.md` and
`docs/evidence/f7-puppet-damage-2026-11-15.md`.
State: partially established. Names, registration, wire IDs and one builder are observed. No handler or send trigger
in this finding has been seen to run.

## Provenance

Binary: installed `ACU.exe` SHA-256 `748abdc6…27e40`. Mapped runtime image SHA-256
`9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, base `0x140000000`.

Corpus: `ACU - Decompiled/<VA>.c`, whole file, for every function named below. Export identity per ACU-RE-0031.

Tools:
- scans of the mapped image for strings and RIP-relative references (scratchpad `xref.py`, `calls.py`);
- Capstone 5.0.3 disassembly of `0x140A89750`;
- `local/symbol_index.sqlite` (classes, vtables, methods);
- `local/coop/net_protocol_atlas.json` and `local/coop/intention_payloads.json` (ACU-RE-0072, -0079).

No IDA or REA session.

Search boundary:
- every NUL-terminated string matching `*NetProtocol*` in the image: 4,056 strings, of which 1,175 are message or
  container class names once the `FactoryEntry` copies and the `_ID` / `_ID_UNICAST` names are removed;
- the reflection index for `%Replica%`, `%Damage%`, `%Health%` and the fight and awareness names below.

## Result

Observations:

1. **Census by family.** The 1,175 names fall into families by prefix. These are the largest, with the AC4 mod
   problem each one bears on:

   | Family | Names | AC4 mod problem |
   |---|---|---|
   | `NetBehaviorAI` | 172 | AI on replicas, guard behaviour (design note §2 item 1) |
   | `NetHumanIntention` | 96 | intention lane (proto 15, design note §2 item 3) |
   | `FightDirector` | 79 | combat parity, attack gating (F7) |
   | `AwarenessDirector` | 35 | guard perception / detection (P6, design note §2 item 7) |
   | `NetFightManager` | 21 | fight join/leave, fight input |
   | `NetBulkManager` | 17 | cheap crowd path (design note §2 item 10) |
   | `GcLHealthNetData`, `ZenHealthReplication` | 16, 11 | damage and health (F7) |
   | `StimulusDirector` | 12 | witness and reaction validation (P6) |
   | `ConflictLoopDirector` | 9 | attack tickets (F7) |
   | `NetFightJIPManager` | 4 | joining a fight in progress |
   | `AILockSettings` / `AILockOwner` | 4 | exclusive ownership of an AI (design note §2 item 8) |

   Names that state an authority rule outright:
   - `ZenHealthReplication_RFCSetMasterLife`, `RFCSetMasterMaxLife`, `RFCSetMasterDamageDealer`;
   - `NetHumanIntention_SendFightDamageEventToMaster` and `NetBehaviorAI_SendFightDamageEventToMaster`;
   - `ConflictLoopDirector_RequestTicket_Master` / `_Dispatch` and `ReleaseTicket_Master` / `_Dispatch`;
   - `AwarenessDirector_RequestDetectiveOwnership_RFC`, `GuardMigratedNetwork`, `Request_RegisterNPC`;
   - `StimulusDirector_RequestReactionValidation` / `SendReactionValidation`, `RequestWitnessHasLOS_RFC`.
2. **Registration pattern (ConflictLoopDirector).** Each message has two lazy getters:
   - a factory-entry getter, e.g. `0x140AC7B10` for `RequestTicket_Master`. It calls
     `sub_1426D1F30("FactoryEntry", name)` and fills a static entry. The entries are 0x140 bytes apart (name buffer at
     `+0x40`). The header holds the previous entry's name address, the value 2, and the CRC `0xD537DD67`, which is
     shared by every `ConflictLoopDirector` entry;
   - a type-record getter, e.g. `0x140ACF020`. It calls `sub_14012A540(&off_1451A1A30, name, sub_140172480())`. The
     record `0x1451A1A30` is the atlas's record for this message (ACU-RE-0072).
3. **Ticket message builder `0x140A89750`.** One function builds every `ConflictLoopDirector` message. For each one it:
   - forms `<prefix>` + `"<Msg>_NetProtocol_ID"`, where the prefix comes from `a1` vf0;
   - looks the type up (`sub_141B69550`);
   - writes the wire ID chosen by `a4`;
   - copies fields from the game-side request `a3`, each with its own changed flag.

   The wire IDs it writes are the atlas's:

   | Message | `a4` true (unicast) | `a4` false (broadcast) |
   |---|---|---|
   | `RequestTicket_Master` | `0xD8A81DEC` | `0x8D569876` |
   | `RequestTicket_Dispatch` | `0x5DE59D15` | `0x54763DA8` |

   Fields copied from `a3`, the same for `_Master` and `_Dispatch`: `+0x08` u32 → message `+80`; `+0x14` u8 → `+112`;
   `+0x28` u32 → `+144`; `+0x38` u64 → `+176`; `+0x50` u64 → `+208`. `_Dispatch` adds `+0x60` (copied by
   `sub_14012A9E0` into `+216`) and `+0x64` u32 → `+280`. No direct call to `0x140A89750` and no 8-byte pointer to it
   were found, so it is reached through a vtable or a computed call, which was not resolved.
4. **Remote players have their own body class.** The reflection index has `HumanReplica` (size 2,464, parent
   `HumanBase`, vtable `0x142F18FC0`, 43 slots) next to `Human` (size 4,192, parent `HumanBase`), plus
   `BhvAssassinReplica` (size 3,056, parent `CharacterBehavior`, 91 slots). `HumanReplica` overrides 14 of
   `HumanBase`'s 43 slots: 0, 2, 3, 5, 13–17, 19, 30, 32, 39 and 40.
5. **Damage classes.** `FightDamageEvent`: size 0x1C0, parent `DamageEvent` (0x30), type record `0x143DC3E20`,
   getter `0x140BDD580`. Also `FightDamageResponseEvent`, `CSrvHealth` (0x190) and `CSrvPlayerHealth` (0x278). The
   type record has three RIP references: `FightDamageEvent` vf3 and vf5, and slot 1433 of the large type table built
   by `0x140B70EA0`. None of them is an event handler. The route from a received `FightDamageEvent` to
   `SendFightDamageEventToMaster` was not found statically.

Inferences and alternatives:
- **F7's next step has an ACU precedent.** The AC4 mod sees guards' `FightDamageEvent`s posted to the puppet with
  nothing applying them. In ACU, names and payloads (ACU-RE-0079: `SendFightDamageEventToMaster` carries the damage
  event's fields from `+0x08` to `+0xac`) say a hit on a body this peer does not own becomes a request to the owner,
  who applies it and replicates health (`ZenHealthReplication`, a memento per ACU-RE-0084). This is name-level plus
  payload-level evidence. The forwarding decision is not seen.
- **Tickets are an owner-arbitrated request.** `_Master` messages go to whoever owns the conflict loop, and
  `_Dispatch` messages carry the owner's decision out, with two extra fields. This fits "one arbiter per contested
  resource" (design note §2 item 8). The field meanings (`a3+0x08` …) are unknown; `+0x38`/`+0x50` being 64-bit
  suggests entity or object IDs, an inference.
- **A replica is a different class, not a flagged Human.** For the AC4 mod, the ACU precedent favours a puppet whose
  logic and damage handling differ by type: `HumanReplica` overrides 14 virtuals. The current approach is a borrowed
  or duplicated Edward with gates. What the 14 overrides do is unknown.

Unknowns / incomplete coverage:
- the receive handlers for the ticket messages and their effect on attacker selection;
- the send trigger of `SendFightDamageEventToMaster` and the master-side apply path;
- the meaning of the 14 `HumanReplica` overrides and the `BhvAssassinReplica` behaviour;
- the awareness and detection authority model (only names so far).

Independent comparison / falsification test:
- **Live, using the ACU-RE-0090 two-game testbed.** In the host game, hook the `SendFightDamageEventToMaster` block
  of the builder (`0x140722B60`, ACU-RE-0079), with a 256-byte stack snapshot. Then have a host NPC strike the guest's
  replica. The inference predicts that the block runs on the host for each hit and that no host health setter runs for
  the replica. A hit on the host's own player is the negative control: it must not reach the block.
- The same run with a hook on `0x140A89750` would show which game action requests a ticket, and which peer.

Reproduction steps: the census is a string scan of the mapped image for `[A-Za-z0-9_:]{3,120}NetProtocol\w*\0`,
dropping `FactoryEntry*`, `*_ID` and `*_ID_UNICAST`, then grouping by the first `_`-separated token. The rest is the
VAs above in the corpus and the image.

Implementation implications and finding-linked constants (for the portable co-op layer):
- A replica forwards state changes to the owner and never applies them (AVATAR_SPEC R2, now with the damage message
  pair and the payload named).
- Contested AI resources use request (`_Master`) → owner decides → result (`_Dispatch`).
- The remote body should be its own type (or an explicitly flagged mode) with its own damage and logic overrides.
- Wire IDs as in observation 3. Ticket request fields: `+0x08` u32, `+0x14` u8, `+0x28` u32, `+0x38` u64,
  `+0x50` u64; dispatch adds `+0x60` and `+0x64` u32.
