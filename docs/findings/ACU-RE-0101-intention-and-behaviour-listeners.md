# ACU-RE-0101 — Who receives the fanned-out intention and behaviour messages?

Date / investigator / related task: 2026-11-16 / Claude (Opus 5.5), static (capstone and IDA) / #85, the event
fan-out left open in ACU-RE-0098
State: partially established.
- Established:
  - the event hash of each of the 185 fan-out messages;
  - the AI-state listener tables that receive them;
  - how each state answers: act, accept, refuse or ignore.
- Not established:
  - the names of the AI states (this build has no RTTI, and the state hashes are not CRC-32 of any image string);
  - what most action listeners do beyond their first call;
  - the FightDirector queue drain.

## Provenance

Binary path / SHA-256 / image base / VA / RVA:
- Captured mapped main image, SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, image base
  `0x140000000`.
- Decompilation used the private IDB `local/85-coop-ida/ACU.runtime-analysis.pe.i64`, opened without
  auto-analysis.

Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.

Stored and decoded lengths / SHA-256: `local/coop/event_listeners.json`, SHA-256
`0c78767010d491b8bd7e542dd8a244410c01f899d27548e37ecb0eebc960c4f2`. The input was
`local/coop/message_handlers.json` (ACU-RE-0098).

Tool/provider/version / evidence IDs or saved output:
- `tools/net_event_listeners.py`, SHA-256 `d430a0bab6617a6aa8ec4ecbf22de92400b443dc04a9ae951160dfa0f86d3bd8`,
  capstone 5.0.3. Its 4 synthetic tests pass (`tools/test_net_event_listeners.py`), and all 28 co-op decoder tests
  pass together.
- IDA Pro (idalib, Hex-Rays) through ida-pro-mcp: decompile and xref reads only.

Exact commands and search boundary:

```text
python tools/net_event_listeners.py --image <main-image.mapped.bin> --handlers local/coop/message_handlers.json \
  --expect-sha256 9a1124f1… --output local/coop/event_listeners.json
```

Searched:
- each fan-out handler and the functions it calls directly, for `mov edx, imm32` followed by
  `call 0x14275C670`;
- all of `.data`, for 8-byte-aligned occurrences of those hashes.

## Result

### Observations

1. **Every fan-out message raises its own event.**
   - The 185 messages pass 177 distinct hashes to `sub_14275C670(component, hash, 0x32, list)`. For example,
     `NetHumanIntention_AssassinateTarget` passes `0xC13A7AEA` and `NetBehaviorAI_AssassinateNet` passes
     `0x565070EC`.
   - 18 cover messages first raise one shared event, `0x89AD9ED2`, then their own.
   - None of the hashes is CRC-32 of an image string or a type-record ID.

2. **Listener tables.**
   - `.data` holds runs of 32-byte entries `{u32 hash, u32 index, u64 flags, u64 handler, u64 0}`, with the index
     counting up from 0. 163 of the 177 hashes sit at the same index in every table, so the index is a global
     event number.
   - An AI-state constructor stores its table at state `+24` and a state hash at `+36`. For example,
     `sub_14062BD00` stores the table `0x143AFD700` and the hash `0xFB9498EB`, plus two more tables at `+264` and
     `+304`.
   - 611 of the 626 tables are loaded by such a constructor.
   - A handler of `nullsub_7` (`0x141E6D4E0`) means the state ignores the event.

3. **Table classes.** The sizes match the systems that use them:

   | Entries | Tables | Receives |
   | ---: | ---: | --- |
   | 132 | 89 | `NetHumanIntention` events (75 of 79 fan-out intentions) |
   | 178 | 85 | `NetBehaviorAI` events (player behaviour) |
   | 855 | 448 | `NetBehaviorAI` (49 messages) and `AwarenessDirector` events |
   | 68 | 3 | `CSrvAmbianceAwarenessStateMachine_SetActiveMood` |
   | 9 | 1 | `EventController_HandleBulkEntitySpawnedNetworked` |

4. **How a state answers.** A listener receives `(state, message, …, bool *accepted)`. Across all tables:
   - **8,825 entries act:**
     - The NHI listener for `AssassinateTarget` (`0x1405B3CD0` → `sub_14070BA60`, the same in 25 tables)
       allocates a 544-byte intention (`sub_140626D40`) and fills it from the message (`sub_14093C060`). It swaps
       the new intention into the human's slot (`sub_141F3C130`), frees the old one, and reports accepted.
     - NBAI listeners forward to the owning component: `AbortGettingRevive` (`0x1405A5BA0` →
       `sub_14069E100(**(state+40))`), or a second fan-out at component `+1024` (`AssassinateNet`, `0x1405BB140`).
     - The 855-entry tables forward to the NPC (`0x141A0ACA0` → `sub_141B15AE0(*(*(state+40)+16))`).
   - **53 entries only accept** (for example `0x14091F070`: `*accepted = 1`).
   - **842 entries refuse** (for example `0x1405B1090`: `*accepted = 0`). All but 3 are behaviour events. The
     shared cover event `0x89AD9ED2` is refused by 46 states and acted on by 39.
   - **122 entries ignore** the event.

5. **Breadth per message (examples):**
   - `NetHumanIntention_AssassinateTarget`, `BeShoved`, `BeginRiftThreatImpact`, `CancelAnimationRFC` and
     `CancelNavigationRFC`: handled in all 25 tables that list them.
   - `CancelLookAt`: 11.
   - `AimAtTargetWithWeapon`: 6, with three different handlers.
   - `NetBehaviorAI_BeginRiftThreatImpact`: 85 behaviour states and 94 NPC states.
   - `NetBehaviorAI_AssassinateNet`: 2 behaviour states, with different handlers.

6. **FightDirector queues.**
   - The constructor `sub_14045C990` sets up five list and lock pairs: `+608/+624`, `+672`, `+708/+720`,
     `+768/+784` and `+832/+848`.
   - The receive handlers push events into the `+832` and `+708` queues (ACU-RE-0098).
   - No function other than the handlers, the constructor (`sub_14045C990`), the destructor (`sub_140468460`) and
     the registration function (`sub_1404A4580`) uses `+0x340` together with `+0x350` with a `lea`.
   - The queued event objects carry interleaved records (`off_142EA54C8`), not plain vtables. The drain is not
     found.

### Inferences

- **A message is applied only if the receiver's current AI state wants it.**
  - The fan-out offers the event to the state machine. The current state acts, accepts, refuses or ignores it.
  - A replica that is in a different state from the master's can therefore drop a message the master expected
    to be applied. For example, cover messages are refused in 46 states.
  - This is one source of desync, and a port should make it observable.
- **Intentions replace one another.** A received intention builds a fresh 544-byte intention object and swaps it
  in. The replica's human therefore follows the latest intention only, with no queue.

### Implementation choice

The portable library keeps, per fan-out message:
- the event hash;
- the number of states that act on it, accept it, refuse it or ignore it;
- the action handlers.

When the receiving state refuses an event, the library reports it instead of dropping it silently.

### Unknowns and next checks

- **State names.** A live check could map them: log `state+36` of the active state, and the event outcome, on
  the testbed.
- **Action listeners.** What each one does beyond its first forward, read per family.
- **The FightDirector queue drain.** Decode the event-object layout behind `off_142EA54C8` and follow its slots.

## Reproduction status

Static. Re-running the command above on the same capture and handler map reproduces the output.
