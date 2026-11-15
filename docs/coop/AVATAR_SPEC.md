# Avatar replication spec (working draft)

This draft is engine-neutral. It is what a port has to reproduce to replicate a human character the way ACU
does. Evidence: [ACU-RE-0075](../findings/ACU-RE-0075-avatar-replication.md), with background in
ACU-RE-0072 (registry and IDs) and ACU-RE-0073 (Storm seam). Each row is marked observed (**O**) or
inferred (**I**).

## 1. Roles

| | Rule | Evidence |
|---|---|---|
| R1 | Every replicated character has one **master** peer, which simulates it. The other peers hold **replicas**. Master role can be acquired, given away, or replaced when the master leaves (Storm `object::inEvent` master-role events). | O (names), I (behaviour) |
| R2 | A replica never changes authoritative state directly. It sends a request RFC to the master (`RFCSetMaster*`, `SendFightDamageEventToMaster`). | I (names) |
| R3 | With no network handle, the same call runs locally. Single-player is the degenerate case of this design. | O (several send sites) |

## 2. Channels

| Channel | What | Delivery | Evidence |
|---|---|---|---|
| Snapshot (`NetObjectMemento`) | Continuous state such as `HumanAvatar`, `AnimComponentNetData`, `PlayerCharacterData` and `ZenHealthReplication` | Broadcast. Changed flags per field, applied only if the sequence number is not older. | O |
| Intention RFC (`NetHumanIntention`, 94 messages) | High-level actions the replica re-enacts: navigate, follow, assassinate, fire, sheathe, cover, look-at | Broadcast or unicast per call | O (names, send path), I (re-enact) |
| Owner RFCs | Discrete events per data type, such as `UpdatePlayerCoopHealing`, `SetIsKO_RFC` and `RespawnReplica` | Broadcast or unicast | O (names) |
| Resync | Re-sends every active intention with its current parameters | Probably when a peer joins or the master changes | O (code), I (trigger) |

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
| — | `+0x50` | u32 | sequence number. An update applies only if its sequence ≥ the local one. | — |

The per-field bit widths and the field meanings are still open (ACU-RE-0075 unknowns).

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
- The owning entity class. Position is now covered in section 5 (ACU-RE-0076).
- Whether the player avatar holds a proxy slot, and whether animation smooths proxy corrections.
- The payload of each intention (2 of 94 partly read).
- The bit widths. (The registry key is the wire ID `0xA92DBEA6`; see ACU-RE-0076.)
- A live check: a one-instance single-player probe to name the owner and the heap singletons. A
  two-instance co-op capture only after a session can be started without the backend.
