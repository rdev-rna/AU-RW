# ACU co-op reverse-engineering coverage

Status: ledger, 2026-11-16, issue #85. This file tracks how much of ACU's co-op is understood, system by system.
Use it to decide what to reverse-engineer next. The portable library (`coop/core`, branch `acu-85-coop-core`) waits
on this work. It implements only the rules marked *covered* below, and is not extended until each system it touches
is.

## Where the inventory comes from

`local/coop/net_protocol_atlas.json` (ACU-RE-0072) lists every reflected type record in ACU's replication layer:
1,508 records, image SHA-256 `9a1124f1…` (analysis view). Each record is grouped by its name prefix into a gameplay
system. The classification is saved to `local/coop/coverage_systems.json`. Every record falls into a system; the
only exception is the base class `NetObjectRemoteFunctionCallDiffBase`.

- **Message**: a `NetObjectRemoteFunctionCall` (or `…DiffBase`) subclass. There are 965, all one-shot RFCs or
  intentions.
- **State**: a `NetObjectMemento` subclass. There are 118, each a snapshot replicated by dirty flags (ACU-RE-0084).
- **Object**: a `NetObject` / `NetObjectDescriptor` type. There are 33, the network object kinds.

Grouping by name prefix is a lead, not proof of behaviour. A system counts as covered only when a finding shows its
behaviour.

## Coverage by system

Columns:
- **Msg / State / Obj**: messages, states and objects counted in the atlas.
- **Fields decoded**: message or state payloads whose fields are decoded.
- **Behaviour known**: what findings show.
- **Gap**: what is missing.

| System | Msg | State | Obj | Fields decoded | Behaviour known | Gap |
|---|---|---|---|---|---|---|
| **Player behaviour** (`NetBehaviorAI`): parkour (≈45 states: climb, leap, beam, swing, wall run, slide, freefall, land), cover (≈25), assassinations (air, ledge, haystack, hidespot, cover, parkour), haystack/hidespot, lockpick, ladder, swimming, elevator, carry object, loot, revive and get-revived, healing, weapons (sheath, select, aim, fire, reload, reflex shot), lantern DLC, morphing, respawn | 171 | 1 | 0 | 0 | Players use this path for damage (0092 obs 15, 17). Send and receive mechanics are shared with intentions (0080–0083). Leap messages are a jump contract sent at takeoff: position, takeoff point, predicted landing point and directions; the replica lands on the same point (0094). | Every other payload; what each message makes the replica do; whether the replica moves by the leap contract or a position stream; hang, void and swing leaps; animation and root-motion ownership; revive flow |
| **Human intentions** (`NetHumanIntention`): navigation (to entity, patrol point, danger source, in fight, in formation; pause/resume; teleport), look-at/orientation, cover, combat actions (aim, fire, throw bomb, tackle, shove, push, berserk, smoke), damage lanes, **carriage reins** (`FlickReins`, `PullReins`, `NavigateOnVehicleToPatrolPoint`), synced group behaviours, Rift weapons | 94 | 2 | 0 | 94 (0079) | Payload layouts (0079), receive (0080), send (0081), flush (0082), join resync (0083), damage execute and routing (0092 obs 14–15) | What each intention does on the receiver; what the `Activated_Navigation` state's 58 fields mean (layout in 0095); carriage driving |
| **Avatar, position, animation**: `HumanAvatar`, `ProxiesProtocol`, `NetPlayerProxy`/`NetProxyManager`, `AnimComponentNetData`, `CSrvBlendingNetData`, `FollowTrajectoryComponent`, `NetAnimalIntention` | 0 | 6 | 4 | `HumanAvatar` raw layout only (0075) | Position and heading replication (0076), proxy receive (0077), session clock (0078), snapshot push (0084) | Field meanings of every state; interpolation and extrapolation parameters; animation sync; crowd blending; animals |
| **Combat**: `NetFightManager_RFC` (join/leave/kill fight, per-frame fight input `AllFightInput*` diffs, master position check), `FightDirector` (≈70 RFCs: melee and ranged aiming *locks*, attack requests and confirms, bombs, firearms, fight positions, taunt, reactions, director master change), `FightHostilityManager`, `FighterNPC`, `NetFight_RFC` (CRC, kick unresponsive, master changed), `NetFightJIPManager` | 105 | 3 | 2 | 0 | Hit routine and damage request to the victim's owner (0092 obs 10, 14–15) | How the fight director's master is chosen and migrated; the lock/token protocol that limits attackers per target; the fight-input diff stream; desync detection by CRC; fight join-in-progress |
| **Health**: `ZenHealthReplication` (master life, max life, god mode, damage dealer, revive timer, KO, dead-body state, death witness), `ImmunityDataItem` | 10 | 2 | 0 | 0 | `RFCSetMasterLife` applies the owner's life on replicas, live in both directions (0092 obs 11, 13, 16) | KO and revive (co-op downed state); max life; god mode; death witnesses; heal FX |
| **NPC AI state and navigation**: `CharacterAI`, `CharacterBehavior`, `AIComponent`, `EntityAINetData`, `BhvGenericNPC`, `NetEntityNavigation{Flee,Flow,Formation,Wander}`, `AILockSettings` (`AddOwner`/`RemoveOwner`), group formations, reaction handler | 25 | 13 | 4 | 0 | Ownership migrates with hysteresis (claim about 50 m, release 55.5–88.8 m, unload hand-off) (0092 obs 17–20); Storm role callbacks | The migration rule's code and constants; `AILockSettings` owners (possibly ownership locks); navigation states |
| **Awareness and stealth**: `AwarenessDirector` (≈30 RFCs: guard state, LKP, high-alert zone leader, detective areas, suspects, *migration sub-states*, assassination seen), `StimulusDirector` (witness LOS, interrogation, faction witnessed death), `ConflictLoopDirector` (tickets), `AlarmBellNetDirector` (ringers), `NetEagleVision`, line of sight, ambiance awareness, danger events, faction interaction | 83 | 7 | 4 | 0 | Nothing beyond the inventory | **Everything.** This is how guards see and hunt two players; it answers AC4's measured negative that guards don't perceive the second player (P6). |
| **Missions and objectives**: Domino operator lists and JIP snapshot, `GameplayCoordinatorPersistenceManager`, coordinator conditions, ≈40 `GcL*` coordinators (bodyguard, follow, defend, chase, tail, eavesdrop, locate target, guard post, patrol, ambush, entice, escape, military trap, puzzles, player goto) | 260 | 35 | 1 | 0 | Mission coordinators are owned by one peer and do not migrate with distance (0092 obs 18). Private hosting by mission id (0092 obs 1–5). | How objective progress, mission start and end, failure, checkpoints and restart replicate; the Domino JIP snapshot; who owns the mission logic and why |
| **Mission targets** (`GcLHealthNetData`: targets, health ratio, fleeing, in range, died, lost, player join/leave) | 15 | 1 | 0 | 0 | — | Assassination target sync |
| **Interactions**: doors, lockpicking, chests and collectables, elevators, activate actions, locked entities | 40 | 6 | 0 | 0 | — | Who opens a chest or door and how both see it; loot sharing |
| **Crowd and world ambience**: crowd events (riots, executions), crowd life zones, fake crowd, bulk (`NetBulkManager` client maps, attractor/danger/victim events), crowd stations, herder migration, living-breathing manager, world ambiance | 88 | 8 | 12 | 0 | Crowd humans are local masters in every game and are not replicated (0092 obs 17) | Which crowd *events* do replicate; bulk-entity client maps; `CrowdHerderGroupMigration` |
| **Rift weapons and mounted guns** (machine gun, fighter-plane gun, searchlight, crossbow, ballistic weapon, hack spots, voxels, hail, lightning) | 39 | 22 | 0 | 0 | — | The only mounted or vehicle-like weapons in ACU: how a player-operated turret replicates |
| **Player data**: character data, inventory, online data, player info, skills, co-op healing, morphing, booster, comms (`OnLetsGo`, `OnThanks`, countdown), refill points, trophies, gameplay settings | 12 | 9 | 4 | 0 | — | What each player sends about itself on join; gear and skills that change gameplay |
| **World objects**: `NetStaticEntitySynchronisation`, `NetEntityObject`, abstract coordinator data | 0 | 1 | 2 | 0 | Pre-placed entities are matched by data id (0076/0077) | Static entity sync; object creation and deletion as players stream the world |
| **PvP remnants** (start flow, PvP eavesdrop) | 13 | 2 | 0 | 0 | — | Low priority |

Totals: 965 messages, 118 states and 33 object types.

**Phase 1 done (ACU-RE-0093, completed in ACU-RE-0097):** field layouts (slot, type, source) for all 963 `*MSG_NetProtocol` messages
(4,657 fields, 4,310 traced to an owner offset), in `local/coop/message_payloads.json`. `TransferGroup` was decoded by hand. The "Fields decoded" column above predates it.
No state type is decoded yet. Behaviour has been shown, at least in part, for parts of 6 of the 15 systems.

**Phase 2 done except one type (ACU-RE-0095, ACU-RE-0099):** codec binding, field layout and newest-wins stamp
for 115 of the 118 state types (1,424 fields), in `local/coop/memento_codecs.json`.
- `GcLCrowdLifeProtocolContainer` and `ProxiesProtocol` are bound once per slot, under keys hashed at run time
  (40 × 30 crowd entries; 30 × 10 proxies per peer).
- `AIComponent_NetProtocol` is registered but has no codec bind.

The "State" counts above are type counts, not decoded ones.

**Receive side mapped (ACU-RE-0098):** the execute handler of all 963 messages, in
`local/coop/message_handlers.json`, with each handler's dispatch style and role test:
- dispatch: 749 direct, 185 event fan-out (most intention and behaviour messages), 29 queued in a director;
- role test: 242 of 260 mission handlers test master, 161 of 171 behaviour handlers test replica, and directors
  test their net object;
- 12 handlers are empty;
- 340 show no test within one call.

**Phase 3a started (ACU-RE-0094):** `ParkourLeap` and `ParkourClimbLeap` decoded live on the testbed (52 leaps):
three points and their directions plus a leap type, sent once at takeoff. The replica's copy of the avatar lands
on the sent landing point.

## Cross-cutting mechanisms

| Mechanism | Known | Gap |
|---|---|---|
| Session, LAN, hosting, joining | LAN path and network-mode flag (0087–0090), private hosting without UI and auto-join (0092 obs 1–5), backend boundary (BACKEND_BOUNDARY.md) | Leave and rejoin; disconnect handling; **host migration** (`HostMigrationStart/End`, `MissionHostMigratedEarly_ReturningToLobby`) |
| Ownership and migration | Role bits, Storm role callbacks, the measured hysteresis (0092 obs 17–20); the hand-over primitive `Give Master Role` and its callers: distance (release R − 40, claim R − 45, planar, R per entity), fight request, interaction, crowd-group transfer (0097) | The sender of the fight load-balance request; the per-class radius table; Storm's acceptance of a hand-over; what state moves with ownership |
| Join in progress | Intention re-send (0083) | Fight JIP (`NetFightJIPManager`), Domino JIP snapshot, memento full-state sends |
| Relevance and world streaming | An owner keeps an entity until it unloads (100–111 m), then hands it over (0092 obs 19) | Which objects replicate to which peer (interest management); `DeleteReplicaEvent`; what each peer loads when players are far apart |
| Time | Session clock (0078) | Timestamps on intentions and states; lag compensation |
| Bandwidth and priority | Per-thread batches and flush (0082) | Rates per system; `AllFightInput10…60` bucketing; memento dirty frequencies |
| Desync detection | `NetFight_RFC_SendCRC` exists | What the CRC covers and the recovery on mismatch |

## Beyond ACU: vehicles, ships and other titles

- **Carriages.** ACU replicates NPC-driven carriages through three `NetHumanIntention` messages (`FlickReins`,
  `PullReins`, `NavigateOnVehicleToPatrolPoint`). Players cannot drive in ACU, so player-driven vehicles have no
  direct reference here. Studying these messages shows how a vehicle and its driver replicate as one owned
  entity.
- **Mounted weapons.** The Rift turrets (machine gun, fighter-plane gun, searchlight) are player-operated mounted
  weapons with state. They are the closest ACU model for a cannon or a ship's swivel gun.
- **Black Flag.** `AC4BFMP.exe`, Black Flag's own multiplayer, runs on a different stack, HackNet
  (`MasterStation`, `ChangeMasterStation`, `CREATE_DUPLICA`; ACU-RE-0074). It has the same master/duplica idea as
  ACU, in AC4's own engine build, so it is the closest reference for the AC4 port. It has no ships. Ship sync
  (sails, cannons, boarding, crew) has no reference in either stack. It has to be designed from the ACU model plus
  AC4's own naval code.
- **Syndicate** has no multiplayer stack (not surveyed yet). Its carriages and trains would be designed the same way.

## Work plan

Static work comes first: it covers every type at once, then live hooks confirm behaviour on the testbed
(`tools/coop_lan`). Each phase ends with a finding and an update to this ledger.

1. **Payload fields for all 965 messages.** Extend `tools/net_intention_payloads.py` from `NetHumanIntention` to
   every RFC family. It reads each message's serializer, which is the same shape for every
   `NetObjectRemoteFunctionCall`. Start with `NetBehaviorAI` (171), then `FightDirector`/`NetFightManager`,
   `AwarenessDirector`/`StimulusDirector`, Domino and GcL. Output: field lists like
   `intention_payloads.json` for every family.
2. **State fields for all 118 mementos.** Decode each memento's write and read functions (ACU-RE-0084 found the
   registrations), giving every snapshot's fields, types and dirty groups.
3. **Behaviour per system, in priority order.** For each system: receive handler → what it changes on the
   replica, plus the master-side decision code. Then a live testbed scenario to confirm it.
   - **a. Player behaviour:** movement, parkour, assassinations, cover, revive. This is the core of "looks
     right".
   - **b. Combat:** fight director master and migration, attacker locks, fight input stream, CRC, JIP.
   - **c. Awareness and stealth:** guard state, alarms, how guards perceive two players.
   - **d. Missions and objectives:** Domino, coordinators, JIP snapshot, checkpoints, failure.
   - **e. Ownership:** the migration decision code and constants; what state moves on hand-over.
   - **f. Interactions, mission targets, health KO/revive, player data.**
   - **g. Crowd events, relevance and streaming, host migration.**
   - **h. Carriages and Rift mounted weapons (the vehicle model).**
4. **Library.** Only after a system is covered does its rule go into `coop/core`, with tests that replay the live
   evidence.
