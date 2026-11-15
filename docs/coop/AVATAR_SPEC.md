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

## 5. Open items for this spec
- The owning entity class, and where position and transform are carried.
- The payload of each intention (2 of 94 partly read).
- The bit widths, and the `0xA92E6CA6` registry key.
- A live check: a one-instance single-player probe to name the owner and the heap singletons. A
  two-instance co-op capture only after a session can be started without the backend.
