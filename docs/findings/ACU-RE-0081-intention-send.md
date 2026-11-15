# ACU-RE-0081: What triggers sending an intention message?

Date / investigator / related task: 2026-11-15 / Claude (Opus 5.5), static only / #85, co-op plan Phases 3/4 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md))
State: partially established. The chain from a game action to the queued message is observed except for
one virtual hop. That hop is inferred from matching signatures and from the registration. How the queue
is flushed to the wire is not traced.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: same capture as ACU-RE-0072 to 0080: mapped image
`9a1124f1…0520`, image base `0x140000000`.
Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable.
Tool/provider/version / evidence IDs or saved output: Capstone 5.0.3 over the mapped image, with
`.pdata` bounds from `tools/net_protocol_atlas.py`. Reference search covers rel32 calls and jumps,
absolute pointers and RIP-relative LEAs. IDA was not used.
Exact commands and search boundary: references to the builder entry `0x14071F090` and its two other
chunks, then linear disassembly of `sub_1405D17F0` (StartIntention) and its net callees.

## Result

### Observations

1. **The builder is registered as a per-message delegate.** The only references to the builder entry
   `0x14071F090` are RIP-relative LEAs inside the per-object binding function (`0x140746F13`,
   ACU-RE-0080 observation 2). The first is `lea rbx, [0x14071F090]` at `0x140747164`, and one more
   follows for each later message block. For each message, the binding function does two things:
   - it builds the delegate `{component, builder}` with `sub_1402294C0`;
   - it registers that delegate under the message's broadcast hash through dispatcher virtual `+0x40`.

   So the second delegate of ACU-RE-0080 is the message builder.
2. **Builder signature.** The prologue at `0x14071F0AC` to `0x14071F0B3` keeps:
   - `rcx` as this (the component);
   - `r8` as the source intention (`rbx`, compared at `+0x8`);
   - `r9b` as the unicast flag (`r12`), where `test r12b` selects the unicast wire ID.
3. **StartIntention replicates when asked to.** In `sub_1405D17F0`, after the new intention instance is
   filled in (id at `+0x8`, header byte at `+0x10`, context words at `+0x14` to `+0x24`):
   - A replicate flag is set when the start context byte `+0x11` is nonzero **and** the manager byte
     `+0x6C` is nonzero.
   - If the flag is set, it calls one of two paths with `(manager, out, intention, ctx+0x1C)`. The choice
     depends on the intention class entry's byte `+0x18` (from the manager's ID map):
     - `sub_141C66660` if the byte is set (global path);
     - `sub_141C66360` if it is clear (object path).
   - The returned record goes to `sub_141C6DC50(manager, slot, record)`. The slot comes from
     `sub_141C633D0`.
4. **Object path `sub_141C66360`.** It takes the manager's net component (`*manager − 0x88`; interface at
   `+0x88`) and returns 0 if the interface or entity checks fail (`+0x10`, `sub_140682520`,
   `sub_142100760`). It then picks a case from the context mode `ctx+0x14` and the component state `+0x38`:

   | Mode | State | Case |
   |---|---|---|
   | 4 | 0 | case 2 if the target `ctx+0x1C` is −1, else case 1 |
   | 4 | not 0 | case 1 |
   | 5 | 0 | case 3 |
   | 5 | not 0 | nothing |
   | 1, 2, 3 | any | that case |

   Each case gets the message by calling dispatcher virtual `+0x80(out, intention, unicast_flag)`. The
   dispatcher is reached through interface virtual `+0x08`. Each case then sets the message's field
   `+0x30` to `intention+0x24` (with its changed flag), and allocates a 24-byte record
   `{kind, message, target, flag}`:

   | Case | Delivery | Record |
   |---|---|---|
   | 1 | broadcast if state is 1, otherwise unicast to `ctx+0x1C` | kind 1 |
   | 2 | broadcast | kind 2, flag = `ctx+0x18 ≠ 0` |
   | 3 | unicast to `ctx+0x1C` | kind 3, flag = `ctx+0x18 ≠ 0` |

5. **The global path `sub_141C66660` has the same case structure.** It applies only when `sub_141F89160`
   returns true and `sub_1407D8470` is not 1. It gets the message from the static helper object
   `qword_145139D00 + 0x110` (the global sender of ACU-RE-0073 observation 6), through virtual `+0x80`.
6. **The records are kept per thread and per manager.** `sub_141C6DC50` uses the TLS slot
   `0x145260F3C` and a map keyed by the manager to find an array, and stores the record at
   `array[slot]`. No send happens inside StartIntention.

### Inferences and alternatives

- **Virtual `+0x80` calls the delegate registered through `+0x40`.** Its arguments
  `(out, intention, unicast flag)` match the builder's `(…, r8 = intention, r9b = unicast)` after the bound
  this. The builder is the only per-message creator registered with that dispatcher, and both sides key on
  the intention's message ID. The dispatcher's vtable is on the heap, so a live probe or IDA would
  confirm it.
- **Full send chain:**
  1. A game action (for example `sub_1407187A0`) on a networked entity calls StartIntention.
  2. StartIntention creates and runs the intention locally.
  3. If replication is requested, it asks the net component's dispatcher for a message.
  4. The dispatcher invokes the per-message builder (ACU-RE-0079), which copies the intention's fields
     into slots and picks broadcast or unicast.
  5. A `{kind, message, target}` record is queued for the intention's slot.
  6. A later flush (not traced) sends it through Storm.
- **Case 1 suggests that a replica sends to its master and the master broadcasts.** With state 1 the
  message is broadcast; otherwise it is unicast to the context's target peer. Taken with the receive gate
  (ACU-RE-0080), state 1 is probably the master role. That is not established.
- The per-message callbacks of ACU-RE-0080 observation 2 are therefore not the send hook. The send
  starts inside StartIntention. Their role (they skip entities flagged `0x40000`) remains open.

### Unknowns / incomplete coverage

- Proof of the virtual `+0x80` → builder hop (inferred).
- The flush: who reads the per-thread record table and calls the Storm send.
- The meanings of the context fields `+0x11`, `+0x14` (mode), `+0x18` and `+0x1C`, and of the manager
  byte `+0x6C` and component state `+0x38`.
- What the global path is for, as opposed to the object path.

### Independent comparison / falsification test

- The builder's unicast test (`test r12b`, from `r9b`) and the callers' `r9b = 1` / `r9 = 0` choices line
  up with the unicast and broadcast cases in observation 4.
- The builder is referenced nowhere except the binding function, so a direct call path would have shown
  up as a rel32 reference; none exists.
- A live check: hook `0x14071F090` during a solo session and expect no calls, because no net component
  exists. In a session, expect one call per replicated StartIntention.

### Reproduction steps

Disassemble the following. Read only.
- `0x140747164` (the builder LEA in the binding function).
- `0x14071F090` to `0x14071F0B3`.
- `0x1405D1A08` to `0x1405D1BF8` (the replicate branch).
- `0x141C66360` to `0x141C66640`, `0x141C66660` to `0x141C66830`, and `0x141C6DC50`.

### Implementation implications and finding-linked constants

- **Port design:** replicate at the point where the target game starts an action. Start it locally, then,
  if the entity is networked, build `{header, typed slots}` from the action's parameters and queue it as
  broadcast (master) or unicast to the master (replica).
- **Per-intention flag:** a per-intention-class flag selects a global sender instead of the per-object
  one.
