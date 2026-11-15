# ACU-RE-0084: How do snapshots (mementos) reach Storm, and which object types carry them?

Date / investigator / related task: 2026-11-15 / Claude (Opus 5.5), static only / #85, co-op plan Phases 2–4 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md))
State: partially established.
- Observed: memento registration on Storm object types, the per-tick push loop, and the call into each
  memento's write function.
- Not traced: how Storm serializes and paces mementos, and the receive (read) path.
- Corrects ACU-RE-0075 on the owning class and on the meaning of `+0x50`.
- The bit widths are still not found.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: same capture as ACU-RE-0072 to 0083: mapped image
`9a1124f1…0520`, image base `0x140000000`.
Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable.
Tool/provider/version / evidence IDs or saved output: Capstone 5.0.3 over the mapped image, with
`.pdata` bounds from `tools/net_protocol_atlas.py`. Names come from a CRC-32 (zlib) match against every
NUL-terminated printable string in the image. IDA was not used.
Exact commands and search boundary:
- References to the Storm strings `inEvent::SynchronizeMemento`, `SetMementoDirty %s`,
  `SetMementoAllDirty %s`, `MementoActivation %s`, `UpdateAllPushMementosAndSynchronize %s` and
  `registry::inEvent::AddObjectMementoEntry`.
- The immediate `0xA92DBEA6` across `.text`.
- All 17 call sites of `sub_141C42490`, decoding the stack arguments stored within 0x80 bytes before each
  call.

## Result

### Observations

1. **Mementos are registered per Storm object type.** `sub_141C42490` builds Storm's
   `registry::inEvent::AddObjectMementoEntry` (its type descriptor at `0x143005AA8` precedes that name).
   Its 17 call sites register these entries. All use mode 4; "wire ID" is the third key.

   | Object type | Memento type | Wire ID |
   |---|---|---|
   | `NetPlayer` | `HumanAvatar_NetProtocol` | `0xA92DBEA6` |
   | `NetPlayer` | `CSrvBlendingNetData_NetProtocol` | `0x5CA1CE99` |
   | `NetPlayer`, `NetNpc` | `CharacterAI_NetProtocol` | `0x9862798E` |
   | `NetPlayer`, `NetNpc` | `ImmunityDataItem_NetProtocol` | `0xE7FD650F` |
   | `NetPlayer`, `NetNpc` | `ZenHealthReplication_NetProtocol` | `0x037158A1` |
   | `NetPlayer`, `NetNpc` | `AIComponent_NetProtocol` | `0x57218EFC` |
   | `NetPlayer`, `NetNpc` | `CharacterBehavior_NetProtocol` | `0x02FECC66` |
   | `NetNpc` | `BhvGenericNPC_NetProtocol` | `0xA837A90F` |
   | `NetNpc` | `EntityAINetData_NetProtocol` | `0xCBBF14E1` |
   | `NetGclEntity` | `GcLSrvPatrolTrackerNetData_NetProtocol` | `0xCD5A0213` |
   | `NetGclEntity`, `NetStaticEntity` | `AnimComponentNetData_NetProtocol` | `0x1A11C194` |

   Seven of the eleven wire IDs equal `CRC-32("zenitar_" + type + "_ID")`. The four exceptions are
   `BhvGenericNPC`, `CharacterAI`, `AIComponent` and `CharacterBehavior`. A prefix search over every
   image string found no match for them.
2. **Per-object codec binding.** `sub_140176C60(owner, registry, skip)`:
   - builds two delegates, `{owner, sub_140172F20}` (read) and `{owner, sub_140173180}` (write), with the
     invoker thunk `0x1401B78D0`, and registers them through registry virtual `+0x00` under
     `0xA92DBEA6`;
   - registers the record hash through registry virtual `+0x88`;
   - stores the registry at `owner + 0x58`.

   `sub_140136080` reverses this through registry virtual `+0x08` and clears `owner + 0x58`.
3. **The push runs every network tick.** The network update `sub_141C630C0` calls these in order:
   1. `sub_141C77700`, the message flush (ACU-RE-0082);
   2. later, `sub_141C55BB0`.

   `sub_141C55BB0` returns unless the session is active (`sub_141F89160`) and `sub_141F891A0` returns
   false. It then gets a key from virtual `+0x30` of `qword_145212F48 + 0x20`, finds that key in a tree at
   `+0x160`, and calls `sub_141C77400` for each object in the matching list.
4. **"Update all push mementos and synchronize"** (`sub_141C77400(object)`). It walks the object's
   memento map (from virtual `+0x90` of the sub-object at `+0x88`). For each memento:
   - it calls the object's virtual `+0x170(memento)` (below);
   - it posts Storm's `object::inEvent::SynchronizeMemento` through `sub_141B36D00`, with the cookie
     `UpdateAllPushMementosAndSynchronize %s` and the object's handle (`+0x2C`). The builder
     `sub_141BC1270` uses the event descriptor at `0x143003988`, right before that event name.
5. **Virtual `+0x170` runs the write function** (`sub_141C77FE0(object, memento)`, slot 46 of the
   vtable at `0x142F698D0`):
   - It reads the network time (`sub_141F89950`).
   - It finds the memento's registered delegate pair through registry virtual `+0x20` (registry at
     object `+0xE0`; key at memento `+0x8`).
   - If a delegate is found, it sets two header slots:
     - the bool slot (value `+0x30`) to 1;
     - the u32 slot (value `+0x50`) to the network time.

     It then clears that slot's changed byte and its flag, so the time stamp alone does not mark the
     memento changed.
   - It then calls the write delegate with `(owner, memento)`. For `HumanAvatar` that is
     `sub_140173180`, which sets the changed flags of the fields that moved (ACU-RE-0075).
   - Without a delegate, it sets the bool slot to 0.
6. **Dirty marking also goes through Storm.** In the same vtable:
   - slot 63 (`sub_141C722C0`) posts `SetMementoDirty` with the memento key formatted as `0x%08X`;
   - slot 64 (`sub_141C72200`) posts `SetMementoAllDirty`.

   Both go through `sub_141B36610`, and both do nothing when the object's field at `−0x50` is 0. No
   game-side caller of either slot was found. `0xA92DBEA6` occurs as an immediate only at registration.
7. **The `HumanAvatar` pool.** The factory `sub_140151A20` creates a pool (`sub_140152860`) whose
   elements are `0x238` bytes, for the memento objects. The constructor that sets the slots (and the
   quantization bit widths at slot `+0x20`) was not located.

### Corrections to ACU-RE-0075

- **Owning class:** `HumanAvatar_NetProtocol` is a memento of Storm object type **`NetPlayer`**.
- **`+0x50` is a network-time stamp,** not a sequence number. It is set to `sub_141F89950` (the
  session clock, ACU-RE-0078) every time the memento is written. The receive rule "apply if the incoming
  `+0x50` ≥ the local one" is therefore a newest-wins check on the session clock.

### Inferences and alternatives

- **Storm decides what goes on the wire.** The game refreshes every memento of its local objects each
  tick and posts `SynchronizeMemento`. Storm then sends the changed fields at its own rate. This follows
  from the strings `outFunc::WriteMemento`, `PushMemento`, `SetRefreshRate` and the
  `MementoTransmissionThreshold*` events. Their implementations are inside Storm and are not traced.
- **The list keyed by virtual `+0x30` holds the objects this peer masters.** Only the master would push
  mementos. Whether virtual `+0x30` returns the local peer id (as virtual `+0xD8` does, ACU-RE-0083) is
  not established.
- **The receive side is Storm's `outEvent::ReadMemento` calling the read delegate** (`sub_140172F20`),
  which applies the memento only when its time stamp is not older. That handler was not traced.
- **Messages before snapshots.** Within a tick, intention messages are flushed before mementos are
  pushed. A replica can therefore receive an action before the state that follows it. Storm's delivery
  order across the two channels is unknown.

### Unknowns / incomplete coverage

- The quantization bit widths (memento constructor not located).
- How Storm serializes mementos (changed-flag bitmap, refresh rate, thresholds). This is inside Storm's
  replication layer, not a Ubisoft backend service. A port replaces it with its own memento transport.
- The four wire IDs that do not follow the `zenitar_` rule.
- The game-side `ReadMemento` handler, and the callers of `SetMementoDirty` and `SetMementoAllDirty`.
- What `sub_141F891A0` (session virtual `+0x10`) gates.

### Independent comparison / falsification test

- The CRC-32 names of all 17 call sites resolve to existing type strings. Each object type is one of the
  Storm object types (`NetPlayer`, `NetNpc`, `NetGclEntity`, `NetStaticEntity`), and each memento type
  has a `_NetProtocol` record. A misread argument slot would give unnamed hashes.
- The memento header slot that `sub_141C77FE0` stamps (value `+0x50`) is the field the `HumanAvatar`
  read function compares (ACU-RE-0075 observation 3).
- A live check: hook `sub_141C77FE0` in a solo session. If the push runs without a co-op session, expect
  calls only when `sub_141F89160` is true. In a session, expect one call per memento per tick on the
  master for `NetPlayer`.

### Reproduction steps

Disassemble the following. Read only.
- `0x141C42490` and `0x141B55F50`; the 17 call sites listed by a reference search for `0x141C42490`.
- `0x140176C60`, `0x140136080` and `0x14018B650`.
- `0x141C630C0`, `0x141C55BB0`, `0x141C77400`, `0x141B36D00` and `0x141BC1270`.
- `0x141C77FE0`, `0x141C722C0` and `0x141C72200`.
- `0x140151A20` and `0x140152860`.

### Implementation implications and finding-linked constants

- **Snapshot transport:** for each object the local peer masters, refresh every memento once per network
  tick:
  - stamp it with the session time;
  - run the writer, which sets changed flags;
  - send the changed fields.

  On receive, apply a memento only if its stamp is not older than the local one.
- **Port scope:** a player character needs seven mementos (table above). `HumanAvatar` is the only one
  decoded so far.
- **Ordering:** flush actions (intentions) before snapshots within a tick, as ACU does.
