# ACU-RE-0080: How does a peer apply a received intention message?

Date / investigator / related task: 2026-11-15 / Claude (Opus 5.5), static only / #85, co-op plan Phases 3/4 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md))
State: partially established. Handler registration, the shared receive handler, the slot decoding, and the
hand-off to the intention system are observed for all 94 messages. Three messages end in a send primitive
instead, and their role is not established. This corrects two statements in ACU-RE-0075.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: same capture as ACU-RE-0072 to 0079: mapped image
`9a1124f1…0520`, image base `0x140000000`.
Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable.
Tool/provider/version / evidence IDs or saved output:
- Capstone 5.0.3 over the mapped image, with the `.pdata` function bounds from
  `tools/net_protocol_atlas.py`'s `Image` class. IDA was not used (it would not open this session).
- The per-message function lists come from `local/coop/net_protocol_atlas.json` (`object_ids`).

Exact commands and search boundary:
- Every function that loads a `…MSG_NetProtocol_ID` literal of the 94 intention messages, from the atlas.
- Linear disassembly of the chunks that reference all 94 literals, and of the final callees of each
  receive block.

## Result

### Observations

1. **Per-message registration with Storm.** For each message, a small function (for example
   `0x14077BAE0` for `ThrowProjectile`) makes three calls on the registry `qword_145212EB8`:
   `sub_141C42310` with the broadcast wire ID, `sub_141C42370` with the unicast wire ID, and
   `sub_141C42550` with the broadcast ID again. Each call carries two more keys:
   - the object type `CRC("NetNpc")` = `0xB09FEC97`;
   - the message type `CRC("NetHumanIntention_<Msg>MSG_NetProtocol")` (for example `0x79544324`).

   The three helpers build Storm registry events (`sub_141B554F0`, `sub_141B556C0`, `sub_141B56790`).
   Intention messages are therefore entries on the **`NetNpc`** object type.
2. **Per-object handler binding.** The function whose main chunk starts at `0x140746F13` (`r15` = the
   intention component, `r14` = a dispatcher object, `r12` = the intention system) does the following for
   each of the 94 messages:
   - It builds the `<prefix><Msg>_NetProtocol_ID` and `…_UNICAST` hashes.
   - It binds a delegate `{r15, sub_1407269B0}` and registers it for both hashes through dispatcher
     virtual `+0x38`. The delegate is made by `sub_140229430`.
   - It registers a second delegate through virtual `+0x40` (`sub_1402294C0`; its target was not
     resolved).
   - It registers a per-message callback with the intention system through `sub_1405F8F70`, for example
     `0x140737A40` for `ThrowProjectile`. These callbacks return early when the entity's flags `+0xD4`
     masked with `0xC0000` equal `0x40000`; otherwise they call into the component at `+0x528`.

   The matching function at `0x1407583BF` removes the same hashes through dispatcher virtual `+0x48`.
3. **One receive handler for all intentions: `sub_1407269B0`.** Its arguments are this (the component),
   the incoming message, the intention manager, and a start context. Its chunks are `0x1407269B0`,
   `0x140726E68`, `0x140728539` and `0x140728B1C`, holding 2, 15, 6 and 71 message blocks: 94 in all.
   Each block does the following:
   - It checks the message's type record (virtual `+0x08`): the depth must be ≥ 5, and the hash at
     `+0x50` must equal that message type's hash, which is cached in a global. Dispatch is by type, not by
     wire ID.
   - It reads the slot values from the message at value offset `slot + 0x18`, using the layouts of
     ACU-RE-0079.
   - It turns u64 entity handles into local entities with `sub_141C4FB90` (handle tag `& 3 == 2`, lookups
     `sub_141CD0D80` and `sub_141CD0D30`).
   - It packs the values into a parameter struct and computes the intention ID
     (`sub_1426D10D0` of `<prefix><Msg>_NetProtocol_ID`).
   - It hands the result to the intention manager, then returns 1.

   Example: the `AssassinateTarget` block reads `u32 @+0xF0`, `bool (@+0x110 == 1)`, `u8 @+0x130` and
   the entity at `@+0x90`, then calls `sub_1405D17F0(manager, out, id, params, ctx)`.
4. **The hand-off is StartIntention.**
   - **`sub_1405D17F0`:**
     - It first calls the gate `sub_141C45720`, which refuses when the manager's net component (`+0x38`)
       is 1 and the start context's source field (`ctx+4`) is 2.
     - It looks up the intention ID in the manager's map (`+0x10`, `sub_14277AA00`) and allocates an
       instance ID from the atomic counter `0x145212F3C`.
     - If the context has no origin (`ctx+0xC` = −1), it fills the instance's time from the network clock
       and its origin from `qword_145212F48 + 0x20`, virtual `+0xD8` (the local index). *(Corrected by
       ACU-RE-0083; an earlier draft called the virtual `+0xD8` value a time.)*
     - It then creates the intention through the class's virtual `+0x78`.
   - **Typed variants with the same prologue:** they call `sub_141C45720` and then the same map lookup
     (for example `0x1405DFAC0` NavigateToEntity, `0x1405E1670` FireRangedWeapon, `0x1405E5540`
     EnterCover, `0x1405E8EC0` Orientate, `0x140223060` Sheathe/Unsheathe).
   - **`sub_141C44920`:** a variant without parameters. It uses the same default result, map lookup and
     instance counter, and is used by the 24 header-only messages.

   In total, 91 of the 94 blocks end in this family.
5. **Three blocks call send primitives instead:** `DropCrowdLifeObject` and `ChangeGuardActionTarget`
   call `sub_14044B4C0`, and `Wander` calls `sub_140221C70`.
6. **The owner's path ends in the same call.** The `AssassinateTarget` entry point `sub_1407187A0` runs
   the local body `sub_140709890` when the net handle at `+0x718` is null. Otherwise it gets the
   dispatcher from the handle (virtual `+0xC0`), builds the ID, and calls `sub_1405D17F0`.

### Corrections to ACU-RE-0075

- **Observation 7** ("resync function"): the function containing `0x140728B1C` and `0x140729376` is the
  receive handler's main chunk (observation 3 above). It is not a resend loop. Its "predicates"
  `sub_1405F49D0` and `sub_1405F59C0` take the incoming message. The fields it reads (`+0xF0`, `+0x110`,
  `+0x130`) are slot values of the received message.
- **Observation 6** ("otherwise it sends through `sub_1405D17F0`"): `sub_1405D17F0` is StartIntention on
  the network-aware intention manager. How the started intention is replicated (the builder of
  ACU-RE-0079) is not yet traced.

### Inferences and alternatives

- **Re-enactment is "start the same intention locally".** A replica decodes the message and starts the
  same intention class with the same parameters through the same StartIntention entry point the owning
  peer uses. Behaviour then runs locally. This fits the master-simulates, replica-re-enacts model of
  ACU-RE-0075, now with the mechanism observed.
- **The gate keeps replicas from starting intentions on their own.** It refuses source 2 when the net
  component's state is 1. The likely reading is: a replica refuses intentions from its own AI or input
  and accepts the network source. The meaning of the values 1 and 2 is not established.
- *(Revised by ACU-RE-0081: the send starts inside StartIntention, and the second delegate is the
  builder.)* **The per-message callbacks (observation 2) are probably the send hook.** They act only when the
  entity is not marked local-only (`0x40000`), and call into `+0x528`, which would lead to the builder.
  This is not traced.
- The three send-primitive blocks may relay requests: a received request is re-issued as a master
  action.

### Unknowns / incomplete coverage

- The target of the second delegate (virtual `+0x40`; ACU-RE-0081: the message builder) and what `0x1419F3240` (`+0x528` path) does.
- The meaning of the gate values and of the start-context fields (`+4`, `+8`, `+0xC` to `+0x18`).
- Where the builder (ACU-RE-0079) is called from. Covered in ACU-RE-0081.
- Whether the start time (network clock, ACU-RE-0083) is used for latency compensation.

### Independent comparison / falsification test

The receive blocks read exactly the slots the builder writes. For example, the `AssassinateTarget`
receive offsets (`+0x90`, `+0xF0`, `+0x110`, `+0x130`) equal the builder slots `0x78`, `0xD8`, `0xF8` and
`0x118` plus `0x18`. A receive path that ignored the builder's layout would not line up this way.
91 of 94 blocks end in functions with StartIntention's prologue and gate, and the remaining three are
named in observation 5.

### Reproduction steps

Disassemble the following. Read only.
- `0x14077BAE0` and `0x141C42310`, `0x141C42370`, `0x141C42550`.
- `0x1407478C7` to `0x1407479F2` (inside `0x140746F13`).
- `0x1407269B0` and `0x1407292F0` to `0x1407293DA`.
- `0x1405D17F0`, `0x141C45720`, `0x141C44920`, `0x1405DFAC0` and `0x1407187A0`.

### Implementation implications and finding-linked constants

- **Receive:** route by message type. Decode the slots, resolve entity handles to local entities, and
  call the target game's equivalent of StartIntention(intention ID, params). The replica runs the
  behaviour itself.
- **Gate:** stop a replica from starting intentions from its own AI or input. The equivalent of the
  `+0x38` / `ctx+4` gate is required in a port.
