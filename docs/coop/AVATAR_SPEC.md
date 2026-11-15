# Avatar replication spec (working draft)

This draft is engine-neutral. It is what a port has to reproduce to replicate a human character the way ACU
does. Evidence: [ACU-RE-0075](../findings/ACU-RE-0075-avatar-replication.md), with background in
ACU-RE-0072 (registry and IDs) and ACU-RE-0073 (Storm seam). Each row is marked observed (**O**) or
inferred (**I**).

## 1. Roles

| | Rule | Evidence |
|---|---|---|
| R1 | Every replicated character has one **master** peer, which simulates it. The other peers hold **replicas**. Master role can be acquired, given away, or replaced when the master leaves (Storm `object::inEvent` master-role events). | O (names), I (behaviour) |
| R2b | A gate in StartIntention (`sub_141C45720`) refuses when the net component's state is 1 (replica, ACU-RE-0082) and the start source is 2. This stops a replica from starting intentions from its own AI or input. | O (code), I (source 2 = local) |
| R2 | A replica never changes authoritative state directly. It sends a request RFC to the master (`RFCSetMaster*`, `SendFightDamageEventToMaster`). | I (names) |
| R3 | With no network handle, the same call runs locally. Single-player is the degenerate case of this design. Live in solo (ACU-RE-0085): the replicate path runs about 30 times per second and stops at the network-component checks; the memento push stops at "session active" (`S+0x30 ≠ −1`). | O (send sites, live solo probe) |

## 2. Channels

| Channel | What | Delivery | Evidence |
|---|---|---|---|
| Snapshot (`NetObjectMemento`) | Continuous state such as `HumanAvatar`, `AnimComponentNetData`, `PlayerCharacterData` and `ZenHealthReplication` | Broadcast. Changed flags per field, applied only if the sequence number is not older. | O |
| Intention RFC (`NetHumanIntention`, 94 messages) | High-level actions the replica re-enacts: navigate, follow, assassinate, fire, sheathe, cover, look-at. Each message is a snapshot-style object: header {u32 id, u8} plus typed slots with changed flags (ACU-RE-0079). | Broadcast or unicast per call | O (names, send path, layouts), I (re-enact) |
| Owner RFCs | Discrete events per data type, such as `UpdatePlayerCoopHealing`, `SetIsKO_RFC` and `RespawnReplica` | Broadcast or unicast | O (names) |
| Intention send | StartIntention starts the intention locally. If the context asks for replication (`ctx+0x11` and manager `+0x6C`), it asks the net dispatcher (virtual `+0x80`) for a message. That call is inferred to reach the per-message builder (ACU-RE-0079). The result is queued as a {kind, message, target} record: broadcast, or unicast to `ctx+0x1C` (ACU-RE-0081). | Broadcast or unicast by mode and state | O (chain), I (one virtual hop) |
| Intention flush | Records are batched per thread and per manager. At the next network tick, the dirty managers are flushed: kind 1 goes to the master if this peer is a replica (state 1), otherwise as a master unicast; kind 2 is a master broadcast; kind 3 is forwarded to every peer except the target. These are inferred to be Storm's `object` MasterBroadcast, MasterUnicast and ReplicaMessage (ACU-RE-0082). | Once per tick | O (chain), I (Storm event names) |
| Intention lifetime | Instances live in the manager's map and are released by handle. Released keys are queued and deleted at the next network tick (ACU-RE-0083). Each instance stores its origin peer and its start time on the network clock. | Local | O |
| Join resync | When a replica is added on a peer, the master re-broadcasts its stored messages, then sends each active intention again, unicast to that peer, if the intention is aimed at everyone or at that peer (ACU-RE-0083). | Unicast to the new peer | O (resend), I (trigger is AddReplica) |
| Intention receive | One handler (`sub_1407269B0`) for all 94 messages. It dispatches by message type, decodes the slots, resolves entity handles to local entities, and calls StartIntention(id, params), so the replica runs the same intention locally (ACU-RE-0080). ACU-RE-0075's "resync" is this handler. | Per message | O |

## 3. Wire IDs (O)

`id = CRC-32("zenitar_" + TypeName + "_ID")`, and `unicast_id = CRC-32("zenitar_" + TypeName + "_ID_UNICAST")`.
`TypeName` is, for example, `NetHumanIntention_AssassinateTargetMSG_NetProtocol`. This is verified for all
963 RFC types. `tools/net_protocol_atlas.py` lists every ID.

## 4. `HumanAvatar` snapshot (O, meanings unknown)

| # | Source offset | Type | Encoding | Changed when |
|---|---|---|---|---|
| 1 | `+0x10` | u8 | raw | value differs |
| 2 | `+0x18` | u64 | raw | value differs |
| 3 | `+0x20` | u64 | raw | value differs |
| 4 | `+0x28` | u32 | raw | value differs |
| 5 | `+0x2C` | u8 | raw | value differs |
| 6 | `+0x30` | f32 | signed, range [-2, 2], `code = trunc(v·0.5·(2^bits−1))` | the value moves by at least 0.001 |
| 7–13 | `+0x34` to `+0x4C` | f32 ×7 | signed, range [-1, 1], `code = trunc(v·(2^bits−1))` | the value moves by at least 0.001 |
| — | `+0x50` | u32 | network-time stamp, set when the memento is written (ACU-RE-0084). An update applies only if its stamp ≥ the local one. | — |

The per-field bit widths and the field meanings are still open (ACU-RE-0075 unknowns). `HumanAvatar` is a
memento of Storm object type `NetPlayer`, alongside `CSrvBlendingNetData`, `CharacterAI`,
`ImmunityDataItem`, `ZenHealthReplication`, `AIComponent` and `CharacterBehavior` (ACU-RE-0084).

**Snapshot cycle (ACU-RE-0084, O):**
1. Each network tick, after the message flush, the game walks the objects filed under the local peer.
2. For each memento of those objects, it sets the header bool to 1 and stamps `+0x50` with the session
   time, without marking that stamp as changed.
3. It runs the memento's write function, which sets changed flags on the fields that moved.
4. It posts Storm's `SynchronizeMemento`.

How Storm then serializes and paces them is not traced (I).

## 5. Position, velocity and heading (ACU-RE-0076)

| | Rule | Evidence |
|---|---|---|
| P1 | Position travels in a **proxy pool**, `NetProxyManager`: 30 `ProxiesProtocol` entries × 10 entities each. It is not part of the avatar snapshot. | O |
| P2 | Dead reckoning. The sender predicts `p_last + v_last·dt` and re-sends position and velocity when the prediction error² exceeds 0.0025 (precise mode) or 0.25 (normal), when the distance² since the last send exceeds 6.25 or 16, or when the entity stops. Prediction applies only after more than 100 ms. | O |
| P3 | The sender predicts from the quantized values it just sent, so both sides extrapolate identically. | O |
| P4 | Position: per-axis signed fixed point, x ∈ ±4000, y ∈ ±3000, z ∈ ±2000, re-sent per axis on a change of at least 0.01. | O (axes and units unverified) |
| P5 | Velocity: unit direction (x, z, sign of y) plus speed clamped to 340. | O |
| P6 | Heading: one scalar from matrix row 1, re-sent when it changes by more than 0.175 rad. Each entity also carries a timestamp and one flag. | O |
| P7 | Placed world entities are bound by entity ID to each peer's local copy, not spawned over the network. | O |
| P8 | The receiver keeps only the newest update per entity (u32 timestamp, wrap-around comparison) and stores the dequantized position and velocity. Each position query returns `p + v·(now − ts)·0.001`, with the squared step capped at 16 or 48 and z clamped to ±2000. Consumers set the replica's transform straight from it; no blending was found in the proxy layer (ACU-RE-0077). | O |
| P9 | Heading decode: `angle = code·4/(2^bits − 1)`. Facing = +y rotated about +z, so the proxy layer is z-up. | O |
| P10 | Timestamps are on a host-origin session clock (Storm `time` module, ACU-RE-0078). The host starts at 0. Each client pings with `{id, 1, client_ms}`, and the host replies `{id, 0, host_session_ms, echoed client_ms}` (big-endian u32). Each sample estimates host time as `host_session_ms + rtt/2`; samples are averaged and applied at most every 500 ms. Pings go every 100 ms for the first 10 replies, then every 2 s. | O |
| P11 | Clock corrections are slewed (`max(1/30, err²·4e-8)` ms per ms, no overshoot), and reads are monotonic. When a session ends, the game latches `offset = net − local` so time stays continuous. That the game's clock getter is this session time is inferred. | O / I |

## 6. Open items for this spec
- The owning entity class: Storm object type `NetPlayer` (ACU-RE-0084). The game class behind it is still
  unnamed. Position is covered in section 5 (ACU-RE-0076).
- Whether the player avatar holds a proxy slot, and whether animation smooths proxy corrections.
- The meanings of intention fields. The layouts of all 94 are extracted (ACU-RE-0079,
  `tools/net_intention_payloads.py`): a 2-slot header {u32 id, u8}, then typed slots (u32, u64, u8, bool,
  f32, vec3, quantized vec3 ±4000 or f32 ±7, entity handle).
- The bit widths. (The registry key is the wire ID `0xA92DBEA6`; see ACU-RE-0076.)
- A live check: a one-instance single-player probe was done (ACU-RE-0085). It resolved the session
  object's virtuals and the global sender's `+0x80`. A two-instance co-op capture only after a session can
  be started without the backend, and once multi-instance capture tooling (PR #141) is tested.
