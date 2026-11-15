# ACU-RE-0083: What does the second dirty list do, and how does a joining peer get the active intentions?

Date / investigator / related task: 2026-11-15 / Claude (Opus 5.5), static only / #85, co-op plan Phases 3/4 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md))
State: partially established. The second dirty list is observed to be deferred deletion of intention
instances, not a snapshot flush. A re-send of all active intentions to one peer is observed, and so is
the handler that calls it when a peer is added. The Storm event behind that handler is inferred. This
corrects ACU-RE-0082's guess and ACU-RE-0080's reading of the start context's "time".

## Provenance

Binary path / SHA-256 / image base / VA / RVA: same capture as ACU-RE-0072 to 0082: mapped image
`9a1124f1…0520`, image base `0x140000000`.
Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable.
Tool/provider/version / evidence IDs or saved output: Capstone 5.0.3 over the mapped image, with
`.pdata` bounds from `tools/net_protocol_atlas.py`. Reference search covers rel32 calls and jumps,
absolute pointers and RIP-relative LEAs. IDA was not used. The image has no usable RTTI at the vtables
checked.
Exact commands and search boundary:
- Linear disassembly of `sub_141C4CA70` and its callee `sub_141C527D0`.
- Every instruction in `0x141C30000` to `0x141C80000` that addresses manager offset `+0x6D` or `+0x75`,
  or writes 1 to `+0x79` or `+0x7A`.
- References to each function found, followed up to the first virtual-table entry.

## Result

### Observations

1. **The `+0x3C` list deletes released intention instances.** `sub_141C4CA70(manager)`, called for each
   manager on the `qword_145213880 + 0x3C` list (ACU-RE-0082 observation 3):
   - takes the manager lock (`+0x30`), clears byte `+0x7A`, and swaps out the pending array (pointer at
     `+0x6D`, count word at `+0x77`);
   - calls `sub_141C527D0(manager, key)` for each 8-byte key, then frees the array through the allocator
     `qword_145221CF8` (virtual `+0x28`).

   `sub_141C527D0` looks the key up in the manager's instance map (`+0x18`, `sub_14277AA00`). If it is
   found, it removes the entry (`sub_14277A1E0`), calls the instance's virtual `+0x00` with 0 (destructor),
   frees it through the same allocator, and returns 1. Otherwise it returns 0. ACU-RE-0082's note that this
   function is 11 bytes was wrong; it runs from `0x141C527D0` to `0x141C528AF`.
2. **Keys are queued by intention handles.** `sub_141C458D0(manager, key)` appends the key to the pending
   array at `+0x6D` under the lock. A key of −1 instead clears the whole instance map (`sub_141C47FF0`) and
   zeroes the count `+0x2A`. Its only caller is `sub_141C75130`, which takes a two-word handle
   `{manager, key}`: if the key is not −1, it queues it and resets the handle to `{0, −1}`. Dozens of game
   functions call `sub_141C75130` (for example `sub_14042DAC0`, `sub_140496738` and `sub_140557CC0`).
3. **The pending list is processed at the next tick.** Only batch close (`sub_141C46880`) calls
   `sub_141C564F0`, which puts the manager on the `+0x3C` list. Releasing a handle does not do this
   itself, so a released instance is deleted at the network tick after the manager's next batch close.
4. **Re-send of all active intentions** (`sub_141C4B730(manager, request)`). Under the lock it walks the
   manager's instance array (`+0x20`, count `+0x2A`). For each instance:
   - it skips keys that are still in the pending-delete array (`sub_1411C27D0`);
   - it looks up the intention class by the key's upper 32 bits in the class map (`+0x10`);
   - it builds a start context from the instance (byte `+0x10`, words `+0x14` to `+0x24`), with the
     last byte set to 1;
   - if the request's word `+4` is 1, it sends the instance only when instance `+0x20` is −1 or equals the
     request's peer `+8`. In that case the context's word `+4` becomes 1, its word `+0x10` becomes the
     peer, and the last byte becomes 0;
   - it calls the class's virtual `+0x08(out, instance, context)`.

   It then marks the manager for the send list (`+0x79`, `qword_145213880 + 0x30`), the same list as
   ACU-RE-0082 observation 2.
5. **It runs after a re-broadcast of stored messages.** `sub_141C4B9C0(owner, request)`:
   - gets an index from `sub_141F884A0` (virtual `+0xD8` of `qword_145212F48 + 0x20`), bounds-checks it
     against the count word `+0xE2`, and takes the map at `owner + 0xD8 [index]`;
   - for each stored message, it creates a fresh message of the same type, copies its fields, and sends it
     through `sub_141C41BD0(owner + 0x18, message, 1)`;
   - it then jumps to `sub_141C4B730(owner + 0x18, request)`.

   `sub_141C41BD0` opens a batch, stamps the message's first slot (value `+0x30`, flag `+0x20`, changed
   `+0x28`) with the network time `sub_141F89950` (ACU-RE-0077), queues a kind-2 record (broadcast,
   ACU-RE-0082), and closes the batch.
6. **Two callers build the request `{1, 1, peer}`.**
   - `sub_141C5C060(owner, _, peer)` does it only when the peer is not the local index from
     `sub_141F884A0`. It is a virtual-table entry (pointer at `0x143010570`).
   - `sub_141C5EAF0(listener, out, event)` first calls `sub_141B33D60`, which adds a per-peer entry keyed by
     the event's peer id (`event + 0x38`) to a tree at `listener + 0x40`. If that returns 1, it does the
     re-send for that peer on the object at `listener + 0xE0`, then calls virtual `+0xB8(5, 0, peer)` on
     the sub-object at `+0x88`. `sub_141B33D60` sits at slot 10 of a base table (`0x142FFDE90`), next to a
     matching remove at slot 11 (`sub_141B33E50`). `sub_141C5EAF0` is referenced from more than ten
     vtables.
7. **StartIntention fills the instance's origin and time.** In `sub_1405D17F0` at `0x1405D18FC`:
   - If context word `+0xC` equals the sentinel `0xFFFFFFFF` (`dword_14300E064`), the instance gets
     `+0x14` = network time (`sub_141EBD390`, a jump to `sub_141F89950`) and `+0x18` = the local index
     (virtual `+0xD8`).
   - Otherwise it copies `+0x14` from context `+0x14` and `+0x18` from context `+0xC`.
   - Either way `+0x1C` comes from context `+0x10`.

### Corrections

- **ACU-RE-0082:** the `+0x3C` list is deferred deletion of intention instances, not a snapshot flush.
- **ACU-RE-0080 observation 4:** virtual `+0xD8` gives the local index, not a time. The time comes from
  the network clock. Context `+0xC` is the originating index and context `+0x14` the start time.

### Inferences and alternatives

- **The index from virtual `+0xD8` is the local peer's id in the session.** It is compared with peer ids,
  used as the origin of locally started intentions, and bounds a per-peer array. That it is a session peer
  id rather than a player slot is not established.
- **`sub_141C5EAF0` is the master's "replica added on a peer" handler,** probably for Storm's
  `object::inEvent::AddReplica` (ACU-RE-0073). It adds the peer to the object's replica set, then brings
  the new replica up to date: stored messages are re-broadcast, and each active intention aimed at
  everyone or at that peer is sent again. With mode 1 and the peer as target, this matches ACU-RE-0081's
  case 1 (unicast to the target). Join-in-progress therefore uses the normal send path.
- **Deletion is deferred so that queued records stay valid.** A record queued during a frame may refer to
  an instance that is released in the same frame. Deleting at the tick, after the send list is flushed,
  avoids that. This is not proven: the tick walks the `+0x3C` list before the `+0x30` list.
- **The class's virtual `+0x08` is "replicate this instance".** It is called with the same
  `(out, instance, context)` shape as StartIntention's replicate step. Its target is a heap vtable.

### Unknowns / incomplete coverage

- The Storm event that reaches `sub_141C5EAF0`, and what `sub_141C5C060` handles.
- The class's virtual `+0x08`, and the meaning of the stored messages at `owner + 0xD8`.
- What instance `+0x20` (per-peer restriction) and context word `+4` mean outside this path.
- `sub_141C3C229`, which also reads the pending array (probably the manager's teardown).
- The snapshot (`NetObjectMemento`) send path is still not traced; it is not on either dirty list.

### Independent comparison / falsification test

- The pending array written by `sub_141C458D0` (`+0x6D`) is the one swapped out by `sub_141C4CA70` and
  checked by the re-send loop, and no other code in the range writes it.
- The re-send's mode 1 and peer field select case 1 in ACU-RE-0081's object path. A re-send that used a
  different mechanism would not set those fields.
- A live check: hook `sub_141C4B730` in a two-peer session. Expect one call on the host when the client's
  replicas are created, with the client's peer id, and none on the client.

### Reproduction steps

Disassemble the following. Read only.
- `0x141C4CA70` and `0x141C527D0`.
- `0x141C458D0`, `0x141C75130` and `0x141C564F0`.
- `0x141C4B730` to `0x141C4B9B8` and `0x141C4B9C0` to `0x141C4BB3B`.
- `0x141C41BD0`, `0x141C5C060`, `0x141C5EAF0` and `0x141B33D60`.
- `0x1405D18E0` to `0x1405D1932`.

### Implementation implications and finding-linked constants

- **Lifetime:** intention instances belong to the manager's map and are released by handle. A port can
  delete at its network tick, after sending.
- **Join-in-progress:** when a peer gains a replica of an entity, the master re-broadcasts its stored
  state messages and sends each active intention again, unicast to that peer, with the original origin and
  start time. A port needs the start time and origin on each active action, plus a per-action peer
  restriction.
- **Message timestamp:** the first slot of these messages carries the network time (ACU-RE-0078 clock).
