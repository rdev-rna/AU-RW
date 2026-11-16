# ACU-RE-0100 — What happens when the host leaves, and how does a peer ask for an object?

Date / investigator / related task: 2026-11-16 / Claude (Opus 5.5), static (capstone and IDA) / #85, coverage ledger
"host migration", "relevance and streaming"
State: partially established.
- Established:
  - the session-level host-migration outcome;
  - the game's reaction to the end of a host migration;
  - the object acquisition request and failure protocol and five of its game callers;
  - the object migration-mode values;
  - Storm's object-event vocabulary.
- Not established:
  - what each of the four migration-mode fields controls;
  - who in the game adds or removes replicas;
  - the consumer of the new host's per-peer queue.

## Provenance

Binary path / SHA-256 / image base / VA / RVA:
- Captured mapped main image, SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, image base
  `0x140000000`.
- Decompilation used the private IDB `local/85-coop-ida/ACU.runtime-analysis.pe.i64`, opened without
  auto-analysis.

Corpus path and lines / export identity status: not used.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: no output file. This finding comes from IDA reads.
Tool/provider/version / evidence IDs or saved output: IDA Pro (idalib, Hex-Rays) through ida-pro-mcp, with
decompile, disassembly and xref reads only, plus capstone 5.0.3 scans written to the session scratchpad.

Exact commands and search boundary:
- every string containing `Migrat`, `migrat` or `HostMigrat` in the image;
- every `storm::echo::` event name about objects, replicas, groups or the master role;
- references to those strings, their type functions and the callers of those functions.

The Ubisoft backend's `MatchmakingProtocol::MigrateHost_V1`, `TaskRDV_SSMigrateHost` and `onHostMigrated` RDV
notification are recorded here by name only and not decoded (backend boundary B1).

## Result

### Observations

1. **Session host migration (Storm).**
   - The result-code table `sub_1426B9520` names the failure cases:
     - `-999992`, "leave host migration is running";
     - `-999948`, "Host migration disabled";
     - `-999947`, "Invalid topology for host migration";
     - `-999943`, "Session host gone".
   - The completion routine is `sub_141C068A0`:
     - It returns without acting if the session state at `+1836` is 3 or bit 1 of `+8` is clear.
     - With no host elected (`+1144 == -1`), it reports "Host migration failed." (`0x141C06BB3`) through
       `sub_141C01720`.
     - With a host elected, and if the local peer is that host (`sub_141B84B80`), it walks the peer list. It
       removes each peer whose join state (`+648`) is not 3, using `sub_141BB2040`; these are peers that had not
       finished joining.
     - Peers in state 3 other than the old host, with bit `0x40` of `+660` clear, get bit 8 set in `+661` and a
       notification (`sub_141C023F0`).
   - `sub_141C18F80` raises `session::outEvent::HostMigrationEnd`. It copies the new session data (`+352` to
     `+392`, and the host at `+212`).

2. **The game's reaction: `MissionBeaconManager` `OnHostMigrationEnd` (`sub_140F41160`).**
   - **Before the mission starts:** if a mission beacon is active and the current mission record has bit
     `0x400000000000` set at `+0x40`, the game:
     - leaves the beacon (`sub_142558410`);
     - shows "The mission host has left before we could start the mission: returning to lobby.";
     - sends the telemetry event `MissionHostMigratedEarly_ReturningToLobby`.
   - **Otherwise, when the session is active and the local peer is host:** it calls
     `sub_141C6EB40(storm, session, peer)` for every other peer. That function walks the session's object table
     (768-byte entries, session virtual `+304`). For every object whose peer field `+760` equals that peer, it
     queues a 64-byte request `{session, peer, object id, -1}` at `storm+1648`.

3. **Object migration modes.** These are three name hashes, set at start-up:
   - `ObjectMigrationMode.Disable` → `dword_145203FA0` (`sub_142DEFD40`);
   - `ObjectMigrationMode.Active` → `dword_145203EB0` (`sub_142DEFB00`);
   - `ObjectMigrationMode.SessionHostOnly` → `dword_145203F00` (`sub_142DEFEC0`).

   An object configuration holds four mode fields at `+0`, `+16`, `+32` and `+48`, plus flags at `+64` and
   `+65`. The default constructor `sub_141B42120` sets all four to Disable.

   On object creation, `sub_141C5B010` sets field `+0` to Active (flag `+64` = 1) and field `+16` to Disable. It
   sets field `+32` to Active, or to Disable when the descriptor's byte `+9` is set, and field `+48` to Disable.
   Changing the mode at run time uses `object::inEvent::SetMigrationMode`.

4. **Acquisition protocol (`NetEntityObject`).**
   - **Request:** `sub_141C6BC10(object)` guards and latches the send:
     - it does nothing if the Storm object's state `+56` is 0 or its pending byte `+195` is set;
     - otherwise, if `+265` is clear, it sends `AcquisitionMigrationRequest` through `sub_141C6E600` and stores
       the result in `+265`.
     - The request is sent as an object message (virtual `+304`, key `unk_145213070`).
   - **Master side:** `sub_141C5E860` receives the request. It reads the requester from payload dword 20 and calls
     `sub_141C55F80(object, requester)`, the variant of "Give Master Role" (ACU-RE-0097). If that fails, it
     replies `AcquisitionMigrationFailure` through `sub_141C6E510`.
   - **Callers.** The game asks for ownership before acting on an entity it does not master:
     - `sub_14181B8E0`: if the target is a replica and the actor is not, it requests the target and returns
       "not yet". Otherwise it performs the action (`sub_140EC5A60`).
     - `sub_1413AED40`: when the object's user (`+360`) is the local player (`sub_141F884A0`) and the object is a
       replica.
     - `sub_1411D7750`: when its entity is a replica.
     - `sub_140D3ADA0`: before linking two entities (the first is master, the second is requested), then it queues
       the link.
     - `sub_1416A2660`: one call at `0x1416A2F29` (not read).

5. **Storm's object-event vocabulary.** These are the replica lifecycle and ownership operations the game drives:
   - **in-events:** `AddReplica`, `RemoveReplica`, `SetReplicationMode`, `EnableReplication`,
     `SetMigrationMode`, `AcquireMasterRole`, `GiveMasterRole`, `Accept`/`RefuseGiveMasterRole`,
     `Accept`/`RefuseReplaceGoneMasterCandidacy`, and `objectObserver` `AddGroupObserver`/`RemoveGroupObserver`;
   - **out-events:** `MasterResolved`, `ReplicaResolved`, `ResolvedReplicaReady`, `MasterGone`, `ReplicaGone`,
     `MasterRoleAcquired`/`Migrated`/`Lost`, `GiveMasterRole`/`GiveMasterRoleFail`, `ReplaceGoneMaster`,
     `AcquireMasterRoleFail`, and `objectObserver` `Discovered`/`Gone`.

   All in-events pass through one processor, `sub_141B8ABF0`, which tests the event type with the predicates
   `sub_141B3xxxx`. Each type has a type function (for example `AddReplica` `0x141B745F0`, `ReplaceGoneMaster`
   `0x141B79070`).

6. **No network relevance setting by name.**
   - The only "relevance" strings are camera settings.
   - Replication scope is explicit: replicas are added or removed per object, and group observers discover the
     objects of a group.
   - The per-entity streaming distance (ACU-RE-0097) decides when ownership moves between peers.

7. **Fight leave reasons** (`sub_140703400`): None, Dead, Flee, Cleanup, Involuntary, Ended, JoinFail, Kick and
   QuitDuringMigration (0–8).

### Inferences

- **Host loss before the mission starts ends the session; after it starts, the mission continues.**
  - Before the mission starts, the joiners return to the lobby.
  - After it starts, the new host keeps the mission and queues work for each peer's objects. These are most
    likely state resyncs; the queue's consumer is not read.
- **"Ask, then act" is the rule for player actions on entities owned elsewhere.** The latch at `+265` stops the
  request from repeating every frame. The action proceeds on a later frame, once the master role arrives
  (ACU-RE-0097's `Give Master Role`), or the attempt ends on `AcquisitionMigrationFailure`.
- **SessionHostOnly exists for objects that must stay with the host.** Which game objects use it is not traced.

### Implementation choice

The portable library models these operations as explicit, testable rules:
- the acquire-request/give/fail protocol, with a per-object latch;
- three migration modes per object;
- host migration with an "early" exit to the lobby and a per-peer resync after takeover.

It does not copy Storm's internals.

### Unknowns and next checks

- What the four configuration fields control. A live check: set each one and watch `MasterRoleMigrated` when a
  peer leaves.
- Who in the game posts `AddReplica`, `RemoveReplica` and `AddGroupObserver`, and with which groups.
- The consumer of `storm+1648`, the new host's per-peer queue.
- What `sub_1416A2660` does with the acquisition request.

## Reproduction status

Static. Open the IDB above and decompile the listed addresses.
