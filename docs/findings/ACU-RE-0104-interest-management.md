# ACU-RE-0104 — Which objects replicate to which peer (interest management)?

Date / investigator / related task: 2026-11-16 / Claude (Opus 5.5), static (capstone and IDA) / #85, cross-cutting
items `interest.relevance` and `interest.delete_replica` in `docs/coop/verdicts.json`
State: partially established.
- Established:
  - every net object gets a Storm replication mode when it is created, and the game chooses between two of the five;
  - the simulation-camera model Storm uses for scope, and the one camera shape the game registers;
  - how the game learns other peers' camera states;
  - the object groups and their observers;
  - the scope test the ownership rule uses.
- Not established:
  - which object classes use which mode (a per-descriptor byte, read at runtime);
  - what the seven shape numbers mean inside Storm;
  - where the local camera is placed.

## Provenance

Binary path / SHA-256 / image base / VA / RVA:
- Captured mapped main image, SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, image base
  `0x140000000`.
- The private IDB `local/85-coop-ida/ACU.runtime-analysis.pe.i64`.

Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.

Stored and decoded lengths / SHA-256: the shape constants were read from the capture at `0x144E52CD8`–`0x144E52D0C`.
These are initialised data in the mapped image. A live check should confirm them.

Tool/provider/version / evidence IDs or saved output:
- IDA Pro (idalib, Hex-Rays) through ida-pro-mcp.
- capstone 5.0.3, for the shape registration's `xmm` arguments.
- A scan of `.text` for RIP-relative `lea`s of each Storm event's vtable, which locates every place that builds that
  event.

Backend boundary: Storm's own scope computation was located but not decoded (`docs/coop/BACKEND_BOUNDARY.md`). This
finding records the game-to-Storm interface.

## Result

### Observations

1. **Five replication modes exist; the game uses two.**
   - Storm names five modes, as hashes of these strings: `ObjectReplicationMode.ManualMode`,
     `ExcludeSessionPeerMode`, `UniqueSessionPeerMode`, `EverySessionPeerMode` and `SimulationCameraMode`.
   - The initialisers are `sub_142DEFDA0`, `sub_142DEFD80`, `sub_142DEFFE0`, `sub_142DEFD60` and `sub_142DEFEE0`.
   - At creation, `sub_141C5B010` (ACU-RE-0100's setup routine) sets the mode with `SetReplicationMode`
     (`sub_141BBDF10`):
     - **`EverySessionPeerMode`** by default (`dword_145204110`): every session peer gets a replica;
     - **`SimulationCameraMode`** (`dword_1452040B0`) when byte `+208` of the creation descriptor (`a3+64`) is 1 or 3,
       tested as `((b − 1) & 0xFD) == 0`.
   - It then sets the migration modes (ACU-RE-0100) and calls two object virtuals (`+264`, `+352`).
2. **Every `AddReplica` and `RemoveReplica` is built inside Storm.**
   - The vtable scan finds them only in Storm code:
     - `AddReplica` at `0x141B5D55B`, `0x141B9E24C` and `0x141BBA8D8`/`…AA8`/`…C2A`;
     - `RemoveReplica` at `0x141B644CB`, `0x141B8AE4C`, `0x141B9E1F4` and `0x141BBA8D1`/`…AA1`/`…C23`.
   - In `sub_141BBA730`, removes and adds sit in pairs, consistent with recomputing scope.
   - No game function builds either event. The game controls scope only through the mode and the camera.
3. **The simulation camera.**
   - The game registers a Storm object type `NetObject.NetSimulationCamera` (`sub_141C1FC30`):
     - its memento is `NetObject.NetSimulationCamera.Memento`, the Storm type `storm::echo::SimulationCameraMemento`,
       with parameters 200, 200;
     - its group is `NetObjectGroup.NetSimulationCamera`.
   - Storm's simulation interface has:
     - inbound: `CreateCamera`, `DeleteCamera`, `SetObjectShape` and `SetCameraStateHandler`;
     - registry: `AddSimulationCameraShape`;
     - outbound: `CameraState` and `CameraReady`, plus a function `object::outFunc::WriteSimulationInfo` (Storm asks
       each object for its simulation data).
4. **One camera shape: `NetSimCamera_Default`.**
   - `sub_141C682E0` posts `AddSimulationCameraShape` (`sub_141B59340`) once:
     - its name is `Register:NetSimCamera_Default`;
     - its shape-type hash is `0x0A7A76F7` (at `0x144E52CE0`);
     - it has seven floats.
   - The floats, in argument order from the capstone read:

     | Argument | Address | Value |
     |---|---|---|
     | `xmm2` | `0x144E52CF0` | 80.0 |
     | `xmm3` | `0x144E52CF4` | 80.0 |
     | arg 5 | `0x144E52CF8` | 40.0 |
     | arg 6 | `0x144E52CFC` | 60.0 |
     | arg 7 | `0x144E52D00` | 30.0 |
     | arg 8 | `0x144E52D04` | 20.0 |
     | arg 9 | `0x144E52D08` | 5.0 |

   - Storm copies them unchanged into the event, so their meaning is defined inside Storm.
5. **Other peers' cameras are mirrored into the game.**
   - The game's `CameraState` handler (`sub_141C56690` → `sub_141C57F50`) looks up the peer by id
     (`sub_1420B1E90(qword_1452217B8, peer << 32 | 0xFF)`).
   - It copies the camera state into that peer's record at `+40`…`+96`: thirteen dwords, a byte and a qword. A
     position and orientation are probable, but the field names are unconfirmed.
6. **Object groups and observers.**
   - The game names three object groups: `NetObjectGroup.Default`, `NetObjectGroup.NetSimulationCamera` and
     `NetObjectGroup.SessionChatter_Main`.
   - `AddGroupObserver` (`sub_141B54C90`) has two game callers:
     - `sub_141C5F3D0`: for every group in a map at `+8`, make the given peer an observer. This is consistent with a
       new peer being subscribed to every group. The routine also rerolls an 8-bit id with the session LCG
       (`1664525·x + 1013904223`).
     - `sub_141C15FD0`: make a peer an observer of `SessionChatter_Main`.
7. **The scope test used by ownership.**
   - `sub_141C51730(netobj, peer)` asks the Storm object (`+440`) for its per-peer record (`sub_141B2F900`) and
     returns record byte `+12`.
   - The distance rule (ACU-RE-0097) gives an object only to a peer for which this is true, that is, a peer that
     already holds a replica.

### Inferences

- **Interest is mostly "everyone".**
  - Objects created with the default mode replicate to every session peer whatever the distance. ACU-RE-0092
    matches this: both games held replicas of mission coordinators and world objects.
  - Only objects whose descriptor sets byte `+208` to 1 or 3 are scoped by the simulation camera. These are
    probably the many streamed AI humans and props.
- **Storm owns scope, so the game cannot add a replica by hand.** It picks a mode and feeds camera data. Storm
  creates and deletes replicas (`CreateReplicaEvent`, `DeleteReplicaEvent`) as cameras and objects move.
- **Ownership needs scope.** The new owner must already hold a replica, so a peer's camera must cover an object
  before that peer can own it. This links the 80/60/40 m-scale shape to the ownership radius in ACU-RE-0103.

### Implementation choice

The library models interest as two modes per object class: **broadcast** (every peer) and **camera** (peers whose
interest volume covers the object). The volume is adapter data. ACU's shape is recorded as reference values, without
an interpretation.

Modern improvements over ACU:
- an explicit interest volume per peer, with a hysteresis band;
- a replica-create event that carries the full state at once, so a peer entering scope never sees a partial
  object (see the join-in-progress items).

### Unknowns and next checks

- **Which classes are camera-scoped.**
  - Live: log `sub_141C5B010`'s descriptor byte `+208` and the object class at each creation.
  - Or decode the descriptor tables statically (the objects' type records, ACU-RE-0072).
- **The shape numbers.** Live: move a peer away from a camera-scoped object and record when the replica is deleted
  and created (`DeleteReplicaEvent` / `ReplicaCreatedEvent`). Compare with 80, 60, 40, 30, 20 and 5.
- **The local camera.** Find who posts `CreateCamera` from the game side (`sub_141B2CDE0`, a virtual), and what
  `WriteSimulationInfo` writes for an entity.

## Reproduction status

Static. Each observation names its addresses. The vtable scan and the capstone reads use the same capture.
