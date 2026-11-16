# ACU-RE-0098 — Which code runs when a co-op message arrives, and who may apply it?

Date / investigator / related task: 2026-11-16 / Claude (Opus 5.5), static (capstone and IDA) / #85, coverage ledger
"receive behaviour per family"
State: partially established.
- Established:
  - the execute handler of all 963 `*MSG_NetProtocol` messages;
  - the three ways a handler hands a message on;
  - the role test each family applies on receive, where it sits in the handler or one call below it.
- Not established:
  - guards deeper than one call (340 handlers show none at that depth);
  - what each queued director event does when it is drained.

## Provenance

Binary path / SHA-256 / image base / VA / RVA:
- Captured mapped main image, SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, image base
  `0x140000000`.
- Decompilation used the private IDB `local/85-coop-ida/ACU.runtime-analysis.pe.i64`, opened without
  auto-analysis.

Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.

Stored and decoded lengths / SHA-256: `local/coop/message_handlers.json`, SHA-256
`901d58abd054898006fea13dd52066051a5253fc69f874c0cdc2fdee7f8302bd`.

Inputs:
- `local/coop/net_protocol_atlas.json` (ACU-RE-0072);
- `local/coop/message_payloads.json` (ACU-RE-0097, SHA-256 `6d9f832f…`).

Tool/provider/version / evidence IDs or saved output:
- `tools/net_message_handlers.py`, SHA-256 `aeed13f02f3276d9019b7646c964f60ee019bda28f0dc086a37da16a76fd9d58`,
  capstone 5.0.3. Its 6 synthetic tests pass (`tools/test_net_message_handlers.py`).
- IDA Pro (idalib, Hex-Rays) through ida-pro-mcp: decompile and disassembly reads only.

Exact commands and search boundary:

```text
python tools/net_message_handlers.py --image <main-image.mapped.bin> --atlas local/coop/net_protocol_atlas.json \
  --payloads local/coop/message_payloads.json --expect-sha256 9a1124f1… --output local/coop/message_handlers.json
```

Searched: every RIP-relative `lea` in `.text` that loads a `*MSG_NetProtocol_ID[_UNICAST]` string, including
short names the linker shares with the end of a longer string.

## Result

### Observations

1. **Registration tables.** Each message family has a registration function. For every message it loads the
   short ID string, then the code pointer of the handler.
   - The family's first entry also loads the send builder, which ties the table to a family.
   - Some families split one registration function over two `.pdata` chunks. For example, `GcLInteractNetData`
     registers its first 12 messages at `0x140AE9B45` and `InteractionAvailable` and `InteractionConfirmed` at
     `0x140AEAFFB`.

2. **All 963 handlers found.** The table below shows how each message's name was matched to its handler.

   | Match | Messages | How |
   | --- | ---: | --- |
   | builder | 410 | the table also loads the builder that contains the message's block |
   | unique short name | 452 | the short name occurs in one family only |
   | table family | 39 | the table's majority family; a split chunk takes the family of the chunk before it |
   | static RFC, 5 callbacks | 36 | `[receive, 0x1401525E0, send, 0x14022EC80, execute]`: the last one runs |
   | static RFC, 4 callbacks | 26 | the execute slot is null; the register function passes the handler to a separate call (for example `sub_140561E50` in `NetAIUIHelper`), and its first non-callback code `lea` is the handler |

   Earlier passes missed some names. The four short names `Start`, `Stop`, `Sync` and `Rest` are under 6 characters;
   the scan now takes 3. `GcLInteractNetData` was the split table.

3. **Handler signature.** Handlers of builder-registered messages receive `(component, message)`.
   - The message dword at `+8` holds the sender's peer slot in bits 2–6, read as `(dword >> 2) & 0x1F`. For example,
     `GcLDoorInteractNetData_SetInteractionStateDone` `0x140AA1EC0` stores it at `+172` and `+132`.
   - Many handlers end by clearing byte `+17` of the object at `message+24`. Examples: `NetBehaviorAI_ParkourLeap`
     `0x1406CE512`, `AlarmBellNetDirector_RequestRing` `0x140A425D7`, and the shared player-leave handler
     `0x14033AA69`. That byte's meaning is not established.

4. **Three dispatch styles** (tool field `dispatch`):
   - **direct (749):** the handler applies the message itself.
   - **event fan-out (185):**
     - The handler calls `sub_14275C670(component, hash, 0x32, list)`. This collects the listeners subscribed to an
       event hash, then calls each listener with the message.
     - Example: `NetHumanIntention_AssassinateTarget` `0x140718760` → `sub_140709890`, hash `0xC13A7AEA`.
     - Used by 79 of 94 `NetHumanIntention` and 103 of 171 `NetBehaviorAI` messages.
   - **queued (29):**
     - The handler builds an event object (pool `off_1452585D0`, allocated by `sub_1426F3760` and
       `sub_1426D9A10`). It then pushes the event into a director queue under a lock: `sub_14211F620`, then
       `sub_140D50F50`, then `sub_14212CBC0`.
     - Examples: `FightDirector_ConfirmMeleeAttack` `0x140497070` and `FighterTargetChanged` `0x14049D2C0`, both
       with the queue at `+832` and the lock at `+848`.
     - `FightDirectorMasterChanged` `0x14049B9D0` queues at `+708`. When its first byte is set, it instead posts
       the event to a global dispatcher.
     - Covers 27 `combat` messages.

5. **Role guards.** The tool searches each handler and the functions it calls directly. The result is in the tool
   field `role_guard`. It looks for four tests:
   - the entity master test `(flags & 0xC0000) == 0x40000` (ACU-RE-0092);
   - the entity replica test `(flags & 0xC0000) == 0x80000`;
   - `sub_141C50B40`, the net-object master test;
   - `sub_141C53E20`, which returns true when the net object at `+440` is missing or its dword `+56` is not 1.

   | System | Messages | Master | Replica | Both | Net object | None at depth 1 | Event fan-out | Queued |
   | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
   | mission | 260 | 242 | 0 | 0 | 2 | 16 | 0 | 0 |
   | behaviour | 171 | 2 | 161 | 0 | 0 | 8 | 103 | 0 |
   | combat | 104 | 2 | 0 | 1 | 0 | 101 | 0 | 27 |
   | intention | 94 | 9 | 16 | 2 | 0 | 67 | 79 | 1 |
   | crowd | 88 | 27 | 0 | 0 | 14 | 47 | 1 | 0 |
   | awareness | 83 | 7 | 0 | 0 | 49 | 27 | 2 | 0 |
   | interaction | 40 | 36 | 0 | 0 | 0 | 4 | 0 | 0 |
   | rift | 39 | 27 | 0 | 0 | 0 | 12 | 0 | 0 |
   | npc_ai | 25 | 1 | 0 | 0 | 2 | 22 | 0 | 0 |
   | target_health | 15 | 12 | 0 | 0 | 0 | 3 | 0 | 0 |
   | pvp | 13 | 5 | 0 | 0 | 0 | 8 | 0 | 0 |
   | player_data | 12 | 0 | 0 | 0 | 0 | 12 | 0 | 1 |
   | health | 10 | 2 | 4 | 0 | 0 | 4 | 0 | 0 |
   | test | 9 | 0 | 0 | 0 | 0 | 9 | 0 | 0 |

6. **Mission coordinators (`GcL*NetData`) keep two copies of their state.**
   - `GcLDoorInteractNetData_SetInteractionStateDone` `0x140AA1EC0` always writes the received view: it resets the
     timer at `+144`, sets the state at `+168` to 3 and stores the sender at `+172`.
   - Only when the entity is master, or there is no entity, does it also write the authoritative copy: it sets
     `+128` to 3, resets the timer at `+104` and stores the sender at `+132`.
   - 242 of 260 mission handlers carry that master test.

7. **The `NetBehaviorAI` replica gate.**
   - `NetBehaviorAI_ParkourLeap` `0x1406CE400` first records the message with `sub_1406D4710` (queue at `+1048`).
   - It then acts only if the entity at `+1032` has kind `flags & 7 == 1` and is a replica
     (`(flags & 0xC0000) == 0x80000`).
   - If the message's first byte is 0, it calls `sub_140695380` with the entity's position. This is the landing
     move measured in ACU-RE-0094.
   - 161 of 171 behaviour handlers test replica: the master sends behaviour and replicas apply it.

8. **Damage goes to the master only.** `NetHumanIntention_SendFightDamageEventToMaster` `0x140734650` resolves the
   target through `sub_1401720E0`. It applies the hit (`sub_1407346F0`) only if that entity is master
   (`0x1407346BF`).

9. **Directors guard on the net object, not the entity.**
   - `AwarenessDirector_GuardMigratedNet` `0x140A50190` runs `sub_140A570E0` only if
     `sub_141C53E20(director[+208] + 56)` is true.
   - `AlarmBellNetDirector_RequestRing` `0x140A42580` applies the same test to `[+16] + 56` before it updates the
     ringer record (72-byte entries at `+104`, count at `+114`).
   - Awareness has 49 such guards, crowd 14.

10. **Twelve handlers do nothing.** Their execute handler is a bare `ret`, such as `nullsub_7` `0x141E6D4E0`. They
    are:
    - `GcLRiftVoxelNetData` `InitVoxelizer`, `Start` and `Stop`;
    - `NetAIUIHelper` `AddDetectiveLoopIcon`, `RemoveDetectiveLoopIcon` and `UpdateInvestigationIcon`;
    - `NetHumanIntention_FlagForPossession`;
    - `FightDirector_TriggerBroadcastFightTargetUpdate_RFC`;
    - `GcLPVPGameStartFlowNetData` `CabalsAssignmentReplicated` and `WarmUpSyncFinished`;
    - two `UnitTest` RFCs.

    A port need not apply them on receive.

11. **Shared handlers.**
    - Seven `NetFightManager_RFC_AllFightInput*` sizes (plain, 10 to 60) share `0x1406EDD60`, which forwards to
      `sub_1406E2330` with the size byte.
    - Six coordinators share the player-leave handler `0x14033AA30`, which removes the leaving player only on the
      master: `GcLHealth`, `GcLLocateTarget`, `GcLRiftThreatSoldiers`, `GcLPlayerEscape`,
      `GcLCEResolutionChase` and `GcLCEResolutionManipulation`.

### Inferences

- **Receive rules by family** (from observations 5–9):
  - Mission coordinators accept requests from any peer. Every peer updates its local view; only the master
    updates the authoritative state.
  - Behaviour messages go from master to replicas.
  - Intention messages are fanned out to listeners, which apply their own tests.
  - Directors queue or guard on their net object.
- **Net-object state 1 likely means "remote".** In `sub_141C53E20`, dword `+56 == 1` is the state in which
  directors refuse to act. This fits `sub_141C50B40`, which also returns false for it. The numeric meaning is
  inferred, not traced.
- **Most "none" handlers probably test deeper.** The 340 handlers with no test found at depth 1 include tail-call
  wrappers such as `FightDirector_EnterConflict` `0x14049A060` → `sub_140494460`. A port should not read "none" as
  "applied unconditionally".

### Implementation choice

The portable library keeps, per message:
- its handler identity;
- its dispatch style (direct, fan-out, queued);
- the role it requires on receive, where that is established.

Families take a default from the table above. A message without an established guard is marked "unverified", not
"open".

### Unknowns and next checks

- Guards below depth 1. Searching to depth 2 and 3 finds only 2 more (338 remain "none"), so these guards are
  not in nearby callees. They are in the director queue drains, the event listeners, or behind virtual calls;
  read them by family.
- The drain side of the director queues (`+832` and `+708` in `FightDirector`).
- The listeners subscribed to each `NetHumanIntention` event hash.
- The meaning of byte `+17` at `message+24`, and the full net-object state enum.

## Reproduction status

Static. Re-running the command above reproduces `local/coop/message_handlers.json` from the same capture and
inputs.
