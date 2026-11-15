# ACU-RE-0082: How are queued intention messages sent to Storm?

Date / investigator / related task: 2026-11-15 / Claude (Opus 5.5), static only / #85, co-op plan Phases 2–4 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md))
State: partially established. Batching, dirty lists, the per-tick flush, and the mapping from record kind
to send call are observed. That the three send virtuals emit Storm's `object::outEvent` messages is
inferred from names. This corrects ACU-RE-0081's reading of component state 1.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: same capture as ACU-RE-0072 to 0081: mapped image
`9a1124f1…0520`, image base `0x140000000`.
Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable.
Tool/provider/version / evidence IDs or saved output: Capstone 5.0.3 over the mapped image, with
`.pdata` bounds from `tools/net_protocol_atlas.py`. RIP-relative reference search over `.text`. IDA was
not used.
Exact commands and search boundary:
- Functions in `0x141C40000` to `0x141C80000` that use TLS index `0x145260F3C` with slot offset `0x20`.
- RIP-relative references to `qword_145213880`.
- Linear disassembly of the flush chain.

## Result

### Observations

1. **Per-thread batches.** Four functions use the same per-thread map (TLS index `0x145260F3C`,
   offset `0x20`, keyed by manager):
   - `sub_141C633D0` opens a batch slot (StartIntention gets its slot index from it).
   - `sub_141C6DC50` stores a record into the slot (ACU-RE-0081).
   - `sub_141C46880` closes the batch.
   - `sub_141C41BD0` and `sub_141C41D20` open and close their own batches around other sends.
2. **Closing a batch** (`sub_141C46880`, called by StartIntention right after queueing):
   - If manager `+0x6C` is 0, it only marks the manager dirty (`sub_141C564F0`).
   - Otherwise it removes the batch from the TLS map, then calls `sub_141C56560` and `sub_141C564F0`, and
     appends the batch to the manager's list at `+0x60` (`sub_142075DD0`).
   - `sub_141C56560` sets manager byte `+0x79` and adds the manager to the list at
     `qword_145213880 + 0x30`. `sub_141C564F0` sets byte `+0x7A` and adds it to the list at `+0x3C`.
     Each manager is added once until the flags are cleared.
3. **Per-tick flush.** The network session update `sub_141C450C0` (ACU-RE-0078 observation 2) calls
   `sub_141C630C0`, which calls `sub_141C77700(qword_145213880)`. That function:
   - walks the `+0x3C` list, calling `sub_141C4CA70` for each manager;
   - walks the `+0x30` list, calling `sub_141C4CB50` for each manager;
   - then resets the list counts.
4. **Record flush** (`sub_141C4CB50`). It clears `+0x79` and swaps the manager's `+0x60` records into a
   local array (`sub_141C371D0`). For each record it checks byte 0:
   - if the byte is nonzero, it calls `sub_141C6FAE0`, the peer remote-call sender;
   - if it is 0, it calls `sub_141C6F6A0`, the object sender.

   Intention records are allocated with byte 0 = 0 (`sub_141C66360`), so they take the object sender.
5. **Object sender** (`sub_141C6F6A0`). It returns unless the manager's net component (`*manager − 0x88`)
   is enabled (`+0x3C`). The message is record `+8`. Each case calls a virtual on the net component and
   passes a cookie string to `sub_141C42780` (`qword_1451C5F78 + 0x4E8`):

   | Record kind | Component state `+0x38` | Virtual | Cookie string |
   |---|---|---|---|
   | 1 | 1 | `+0x130` | `FunctionCallContext::CallCookie::SendReplicaMessage` |
   | 1 | not 1 | (unicast to record `+0x10`) | `…::SendMasterUnicast` |
   | 2 | any | `+0x120` | `…::SendMasterBroadcast` |
   | 3 | any | `+0x128` | `…::SendMasterUnicast-Foward` |

   - For kind 2, the reliability flag is record `+0x14`, which was `ctx+0x18`.
   - For kind 3, the sender gets the peer list (`sub_1420B1FD0(qword_1452217B8, 2)`) and sends to every
     peer whose id (`+0x14`) differs from the record's target `+0x10`.
6. **Peer remote-call sender** (`sub_141C6FAE0`), for comparison. It uses `qword_1452136C0` virtuals
   `+0x90`, `+0x88` and `+0x80`, with the cookies `SendPeerBroadcastExclude-Foward`, `SendPeerBroadcast`
   and `SendPeerUnicast`. This is the path for non-object remote calls.

### Correction to ACU-RE-0081

Component state 1 is the **replica**. With state 1, a kind-1 record goes to the master as a replica
message. Otherwise it is a master unicast to the target. ACU-RE-0081's inference that state 1 was the
master is wrong. This also settles ACU-RE-0080's gate: StartIntention refuses source 2 on a replica.

### Inferences and alternatives

- **The three object virtuals emit Storm's object messages.** `+0x120`, `+0x128` and `+0x130` produce
  `storm::echo::object::outEvent::MasterBroadcast`, `MasterUnicast` and `ReplicaMessage` (ACU-RE-0073
  observation 3). The cookie names match the event names exactly. The component's vtable is on the heap,
  so this is not proven.
- **End-to-end routing:**
  - A master broadcasts intentions, or unicasts them to one peer.
  - A replica's intentions go only to the master, as replica messages.
  - Kind 3 is a master forwarding a message to everyone except its origin. This is likely how a
    replica's request reaches the other peers.
- **Batching gives one send per manager per network tick.** All intentions started during a frame are
  sent together at the next tick, in the order they were queued.

### Unknowns / incomplete coverage

- What `sub_141C4CA70` (the `+0x3C` list) sends. It probably flushes snapshots: it is reached through
  `sub_141C527D0`, which is 11 bytes and was not followed.
- The exact Storm event each virtual produces (inferred from names).
- The target set used by kind-1 unicast on the master, and the peer list `qword_1452217B8`.

### Independent comparison / falsification test

- The record layout written by `sub_141C66360` (byte 0, kind `+4`, message `+8`, target `+0x10`, flag
  `+0x14`) is read field for field by `sub_141C6F6A0`.
- The kind values 1, 2 and 3 in ACU-RE-0081 observation 4 select exactly the three send branches here.
- A live check: hook `sub_141C6F6A0` in a two-peer session. Expect kind 2 on the host for its NPCs, and
  kind 1 with state 1 on the client.

### Reproduction steps

Disassemble the following. Read only.
- `0x141C46880`, `0x141C564F0` and `0x141C56560`.
- `0x141C630C0` and `0x141C77700`.
- `0x141C4CB50`, `0x141C6F6A0` to `0x141C6F960`, and `0x141C6FAE0`.

### Implementation implications and finding-linked constants

- **Port transport:** collect replicated actions per entity during a frame, then send once per network
  tick.
  - Master: broadcast (reliability flag per action), unicast to one peer, or forward to all but the
    origin.
  - Replica: send only to the master.
- **Gate on the replica:** the replica must refuse locally sourced intention starts.
