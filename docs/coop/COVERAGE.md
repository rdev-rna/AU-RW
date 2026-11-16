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

## Definition of done

The goal is robust, portable, feature-complete shared-world co-op. Nothing is skipped: every item is explained,
then classified. ACU co-op is reverse-engineered completely when all five criteria hold.

- **A. Every item explained.** Each of the 965 messages, 118 states and 33 object types has:
  - its fields;
  - its handler;
  - a behaviour finding;
  - a verdict: **port**, **engine-local**, **diagnostic** or **ACU-only**.

  The verdict records what the item *is*, not permission to skip it. A diagnostic or ACU-only item still needs its
  finding.
- **B. Every gap closed.** Every gap in "Cross-cutting mechanisms" and every easy-to-miss checklist item is closed
  by a finding, even when the answer is "local by design".
- **C. Live evidence.** Every system with a port item has a passing two-game testbed scenario with a saved trace.
- **D. Reproduction.**
  - `coop/core` replays the saved traces and makes the same decisions as ACU.
  - Then shadow mode: hook ACU and compare the library's decisions with the game's, live.
- **E. Closed adapter.** The game-adapter interface (every call the library makes into a game) is closed, and
  each call has an ACU reference meaning backed by a finding.

**Checker.** `tools/coop_coverage_check.py` enforces these criteria. It reads:
- the `local/coop/*.json` outputs: the item universe, payload fields, handlers and memento codecs;
- the verdict file `docs/coop/verdicts.json`: verdicts, behaviour findings, traces, the cross-cutting and checklist
  items, and the adapter calls;
- `docs/findings/`: every finding ID it is given must exist there.

It fails while any item is unknown, lacks a finding, or (if port) lacks a live trace. Refresh the block below
each stride:

```text
python tools/coop_coverage_check.py --update-ledger docs/coop/COVERAGE.md --no-fail
```

<!-- coverage-check:begin -->

| Criterion | Status |
|---|---|
| A. Items done (fields, handler, behaviour finding, verdict; port items traced) | 1.5% of 1116 |
| A. messages (965) | fields 964, handler 965, behaviour 17, verdict 28, done 16 |
| A. states (118) | fields 117, handler 117, behaviour 1, verdict 1, done 1 |
| A. objects (33) | fields 0, handler 0, behaviour 0, verdict 0, done 0 |
| A. Verdicts given | port 17, engine-local 2, diagnostic 10, acu-only 0 |
| B. Cross-cutting gaps closed | 10 of 30 (33.3%) |
| B. Easy-to-miss checklist closed | 0 of 16 (0.0%) |
| C. Port systems with a live trace | 3 of 4 (75.0%): behaviour, combat, health, player_data |
| D. Trace replay in coop/core / shadow mode | 0.0% / 0.0% |
| E. Adapter calls with an ACU reference | 7 of 12 (58.3%); interface open |

Complete: no. Errors: 0.

<!-- coverage-check:end -->

## Coverage by system

Columns:
- **Msg / State / Obj**: messages, states and objects counted in the atlas.
- **Fields decoded**: message or state payloads whose fields are decoded.
- **Behaviour known**: what findings show.
- **Gap**: what is missing.

| System | Msg | State | Obj | Fields decoded | Behaviour known | Gap |
|---|---|---|---|---|---|---|
| **Player behaviour** (`NetBehaviorAI`): parkour (≈45 states: climb, leap, beam, swing, wall run, slide, freefall, land), cover (≈25), assassinations (air, ledge, haystack, hidespot, cover, parkour), haystack/hidespot, lockpick, ladder, swimming, elevator, carry object, loot, revive and get-revived, healing, weapons (sheath, select, aim, fire, reload, reflex shot), lantern DLC, morphing, respawn | 171 | 1 | 0 | 0 | Players use this path for damage (0092 obs 15, 17). Send and receive mechanics are shared with intentions (0080–0083). Leap messages are a jump contract sent at takeoff: position, takeoff point, predicted landing point and directions; the replica lands on the same point (0094). | Every other payload; what each message makes the replica do; whether the replica moves by the leap contract or a position stream; hang, void and swing leaps; animation and root-motion ownership; revive flow |
| **Human intentions** (`NetHumanIntention`): navigation (to entity, patrol point, danger source, in fight, in formation; pause/resume; teleport), look-at/orientation, cover, combat actions (aim, fire, throw bomb, tackle, shove, push, berserk, smoke), damage lanes, **carriage reins** (`FlickReins`, `PullReins`, `NavigateOnVehicleToPatrolPoint`), synced group behaviours, Rift weapons | 94 | 2 | 0 | 94 (0079) | Payload layouts (0079), receive (0080), send (0081), flush (0082), join resync (0083), damage execute and routing (0092 obs 14–15) | What each intention does on the receiver (listeners and the replace-intention path in 0101; per-handler detail open); what the `Activated_Navigation` state's 58 fields mean (layout in 0095); carriage driving |
| **Avatar, position, animation**: `HumanAvatar`, `ProxiesProtocol`, `NetPlayerProxy`/`NetProxyManager`, `AnimComponentNetData`, `CSrvBlendingNetData`, `FollowTrajectoryComponent`, `NetAnimalIntention` | 0 | 6 | 4 | `HumanAvatar` raw layout only (0075) | Position and heading replication (0076), proxy receive (0077), session clock (0078), snapshot push (0084) | Field meanings of every state; interpolation and extrapolation parameters; animation sync; crowd blending; animals |
| **Combat**: `NetFightManager_RFC` (join/leave/kill fight, per-frame fight input `AllFightInput*` diffs, master position check), `FightDirector` (≈70 RFCs: melee and ranged aiming *locks*, attack requests and confirms, bombs, firearms, fight positions, taunt, reactions, director master change), `FightHostilityManager`, `FighterNPC`, `NetFight_RFC` (CRC, kick unresponsive, master changed), `NetFightJIPManager` | 105 | 3 | 2 | 0 | Hit routine and damage request to the victim's owner (0092 obs 10, 14–15). `NetFight` is deterministic lockstep (0102): per-member, per-frame inputs (`FightInput`, `AllFightInput*`), a 33.33 frame period, a step that runs only to the last frame every member has input for, join and leave commands that expire after 1000 frames, a debug-only CRC whose mismatch does nothing, and a developer `.nfr` recorder. Live (0105): both games apply the same frame-stamped input records (member ids, action bits, directions) with no move, weapon or animation id; each game picks the moves from the weapon its loadout data names | How the fight director's master is chosen and migrated; the lock/token protocol that limits attackers per target; the remaining input-record field names and the clock unit; the join/leave routines, kick protocol and fight join-in-progress (behind a decompiler limit, 0102) |
| **Health**: `ZenHealthReplication` (master life, max life, god mode, damage dealer, revive timer, KO, dead-body state, death witness), `ImmunityDataItem` | 10 | 2 | 0 | 0 | `RFCSetMasterLife` applies the owner's life on replicas, live in both directions (0092 obs 11, 13, 16) | KO and revive (co-op downed state); max life; god mode; death witnesses; heal FX |
| **NPC AI state and navigation**: `CharacterAI`, `CharacterBehavior`, `AIComponent`, `EntityAINetData`, `BhvGenericNPC`, `NetEntityNavigation{Flee,Flow,Formation,Wander}`, `AILockSettings` (`AddOwner`/`RemoveOwner`), group formations, reaction handler | 25 | 13 | 4 | 0 | Ownership migrates with hysteresis (claim about 50 m, release 55.5–88.8 m, unload hand-off) (0092 obs 17–20); Storm role callbacks | The migration rule's code and constants; `AILockSettings` owners (possibly ownership locks); navigation states |
| **Awareness and stealth**: `AwarenessDirector` (≈30 RFCs: guard state, LKP, high-alert zone leader, detective areas, suspects, *migration sub-states*, assassination seen), `StimulusDirector` (witness LOS, interrogation, faction witnessed death), `ConflictLoopDirector` (tickets), `AlarmBellNetDirector` (ringers), `NetEagleVision`, line of sight, ambiance awareness, danger events, faction interaction | 83 | 7 | 4 | 0 | Nothing beyond the inventory | **Everything.** This is how guards see and hunt two players; it answers AC4's measured negative that guards don't perceive the second player (P6). |
| **Missions and objectives**: Domino operator lists and JIP snapshot, `GameplayCoordinatorPersistenceManager`, coordinator conditions, ≈40 `GcL*` coordinators (bodyguard, follow, defend, chase, tail, eavesdrop, locate target, guard post, patrol, ambush, entice, escape, military trap, puzzles, player goto) | 260 | 35 | 1 | 0 | Mission coordinators are owned by one peer and do not migrate with distance (0092 obs 18). Private hosting by mission id (0092 obs 1–5). | How objective progress, mission start and end, failure, checkpoints and restart replicate; the Domino JIP snapshot; who owns the mission logic and why |
| **Mission targets** (`GcLHealthNetData`: targets, health ratio, fleeing, in range, died, lost, player join/leave) | 15 | 1 | 0 | 0 | — | Assassination target sync |
| **Interactions**: doors, lockpicking, chests and collectables, elevators, activate actions, locked entities | 40 | 6 | 0 | 0 | — | Who opens a chest or door and how both see it; loot sharing |
| **Crowd and world ambience**: crowd events (riots, executions), crowd life zones, fake crowd, bulk (`NetBulkManager` client maps, attractor/danger/victim events), crowd stations, herder migration, living-breathing manager, world ambiance | 88 | 8 | 12 | 0 | Crowd humans are local masters in every game and are not replicated (0092 obs 17) | Which crowd *events* do replicate; bulk-entity client maps; `CrowdHerderGroupMigration` |
| **Rift weapons and mounted guns** (machine gun, fighter-plane gun, searchlight, crossbow, ballistic weapon, hack spots, voxels, hail, lightning) | 39 | 22 | 0 | 0 | — | The only mounted or vehicle-like weapons in ACU: how a player-operated turret replicates |
| **Player data**: character data, inventory, online data, player info, skills, co-op healing, morphing, booster, comms (`OnLetsGo`, `OnThanks`, countdown), refill points, trophies, gameplay settings | 12 | 9 | 4 | 0 | **Gear and weapons (0105, live).** The loadout is one owner-authored blob of `InventoryItemSettings` IDs, sent on change and on re-creation; the replica rebuilds the look from its own data. The selected weapon slot is enforced per frame (`SetSelected*`). | The blob consumer on the replica; skills, healing, morphing, booster, comms; `RespawnReplica` |
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

**Fan-out listeners mapped (ACU-RE-0101):** each of the 185 fan-out messages raises its own event hash. That
event goes to the current AI state's table, in `local/coop/event_listeners.json` (626 tables: 89 intention, 85
behaviour, 448 NPC). The answers: 8,825 act, 53 accept only, 842 refuse, 122 ignore. A message can be dropped
because the receiver's AI state refuses it.

**Phase 3a started (ACU-RE-0094):** `ParkourLeap` and `ParkourClimbLeap` decoded live on the testbed (52 leaps):
three points and their directions plus a leap type, sent once at takeoff. The replica's copy of the avatar lands
on the sent landing point.

## Cross-cutting mechanisms

| Mechanism | Known | Gap |
|---|---|---|
| Session, LAN, hosting, joining | LAN path and network-mode flag (0087–0090), private hosting without UI and auto-join (0092 obs 1–5), backend boundary (BACKEND_BOUNDARY.md) | Leave and rejoin; disconnect handling; host migration (0100): outcome codes, completion (peers not fully joined are dropped), the game returns to the lobby if the host leaves before the mission starts, and otherwise the new host queues per-peer object work. Still open: the consumer of that queue |
| Ownership and migration | Role bits, Storm role callbacks, the measured hysteresis (0092 obs 17–20); the hand-over primitive `Give Master Role` and its callers: distance (release R − 40, claim R − 45, planar, R per entity), fight request, interaction, crowd-group transfer (0097) ; the acquisition protocol (request, give or fail; a per-object latch) and its "ask, then act" callers; three object migration modes (0100) ; R is a world load-distance grid at the owner's focus, capped by the load-square diagonal, with mission and config overrides; Storm, not game code, accepts or refuses a hand-over; the game posts the request with a pending flag and learns the result from the role callbacks; a fallback gives an object to the first valid peer; no state is sent at hand-over (0103) | The sender of the fight load-balance request; the grid values (live); the fallback's trigger; where the pending flag clears; whether Storm forwards pending state |
| Join in progress | Intention re-send (0083) | Fight JIP (`NetFightJIPManager`), Domino JIP snapshot, memento full-state sends |
| Relevance and world streaming | An owner keeps an entity until it unloads (100–111 m), then hands it over (0092 obs 19). Each net object gets a Storm replication mode at creation: every session peer by default, or the simulation camera when its descriptor asks; Storm alone adds and removes replicas; the game registers one camera shape `NetSimCamera_Default` (80, 80, 40, 60, 30, 20, 5) and mirrors other peers' camera states; ownership needs a replica in scope (0104) | Which classes are camera-scoped; what the shape numbers mean (live); where the local camera sits; what each peer loads when players are far apart |
| Time | Session clock (0078) | Timestamps on intentions and states; lag compensation |
| Bandwidth and priority | Per-thread batches and flush (0082) | Rates per system; `AllFightInput10…60` share one receive handler (0102), the send-side bucketing is unread; memento dirty frequencies |
| Desync detection | Fight CRC over each frame's member inputs, compared when every peer has reported; only with a debug flag, which also delays the fight by 5 × a setting; a mismatch reaches an empty stub (0102) | Any desync detection outside fights; a recovery path (none exists in ACU) |

## Beyond ACU: vehicles, ships and other titles

- **Carriages.** ACU replicates NPC-driven carriages through three `NetHumanIntention` messages (`FlickReins`,
  `PullReins`, `NavigateOnVehicleToPatrolPoint`). Players cannot drive in ACU, so player-driven vehicles have no
  direct reference here. Studying these messages shows how a vehicle and its driver replicate as one owned
  entity.
- **Mounted weapons.** The Rift turrets (machine gun, fighter-plane gun, searchlight) are player-operated mounted
  weapons with state. They are the closest ACU model for a cannon or a ship's swivel gun.
- **Black Flag.** `AC4BFMP.exe`, Black Flag's own multiplayer, runs on a different stack, HackNet
  (`MasterStation`, `ChangeMasterStation`, `CREATE_DUPLICA`; ACU-RE-0074). It has the same master/duplica idea as
  ACU, in AC4's own engine build, so it is the closest reference for the AC4 port. It has no ships.
- **AC4 ships are already decoded in AC4Coop.** Its `docs/STATUS.md` lists a live "ship mirror": a
  duplicated local ship carries the peer's hull HP and transform. ACU supplies the **sync model** for ships, not
  the ship mechanics. The mapping is in "Portability" below.
- **Syndicate** has no multiplayer stack (not surveyed yet). Its carriages and trains would be designed the same way.

## Portability: the game adapter and per-title mapping

The library knows nothing about the game. Each title implements a small adapter, and the library calls only that
adapter. The calls, each with its ACU reference meaning, are tracked under `adapter` in `docs/coop/verdicts.json`
(criterion E):

| Adapter call | ACU reference meaning | Finding |
|---|---|---|
| Spawn a proxy | The proxy receiver creates or updates a remote player proxy | 0077 |
| Remove a proxy | Open | — |
| Apply a pose | Position and heading replication onto the replica | 0076 |
| Apply an animation | Open | — |
| Apply an intention | Intention receive, then the AI-state listener offer | 0080, 0101 |
| Deliver damage | Damage request to the victim's owner, then `RFCSetMasterLife` | 0092 |
| Fight input | Per-member, per-frame input into the lockstep step | 0102 |
| Apply a loadout | Resolve the owner's item references locally and rebuild the look; enforce the selected slot | 0105 |
| Mission events | Open | — |
| Awareness events | Open | — |
| Ownership distance | Planar distance against R per entity (release R − 40, claim R − 45) | 0097 |
| Save hooks | Open | — |

**Per-title mapping.** Each title keeps a table from its own systems to ACU patterns. For AC4:

| AC4 system | ACU pattern |
|---|---|
| Ship | Owned vehicle (the carriage pattern: vehicle and driver replicate as one owned entity) |
| Cannons and swivel guns | Mounted weapons (the Rift turrets) |
| Crew | Migrating NPCs (ownership hysteresis and hand-over, 0092, 0097) |
| Boarding | Fight-director hand-over |
| Naval combat | Fight director plus attacker locks |

## Robustness requirements for `coop/core`

These are the "modern improvements" over ACU. Each one is a library requirement with tests, not an RE item:
- **Versioned wire format.** Capability negotiation on join.
- **Bounded decode.** Length and offset checks, no unbounded allocation, and a fuzz target for every codec.
- **Clear authority.**
  - One owner per object.
  - An explicit hand-over handshake (ACU's request / give / fail, ACU-RE-0100) with latch timeouts.
- **No silent drops.** Every dropped or refused message is reported. ACU drops a message when the receiving AI
  state refuses it (ACU-RE-0101).
- **Desync recovery.**
  - Checksums per system, then a resync from the owner.
  - ACU's fight CRC is debug-only and its mismatch path is empty (ACU-RE-0102).
- **Session resilience.**
  - Reconnect and rejoin without restarting the mission.
  - Host migration that keeps mission progress.
- **Deterministic tests.**
  - Seeded RNG, fixed ticks and recorded-trace replay.
  - Unresolved float determinism is recorded, not assumed away.
- **Backend boundary kept.** Record the Ubisoft backend, never decode it.

## Priority from here (steer of 2026-11-16)

Shared-world sync comes first, because it is the hard part. Systems follow. Every item listed is tracked in
`docs/coop/verdicts.json`.

1. **Cross-cutting:**
   - ownership: the per-class radius table, what state moves on hand-over, and Storm's acceptance of it;
   - interest management;
   - join in progress: fight JIP, the Domino snapshot, full-state memento sends;
   - leave, rejoin and disconnect mid-mission;
   - the consumer of the host-migration queue;
   - time, timestamps and lag compensation;
   - bandwidth: rates per system and dirty frequencies;
   - desync detection outside fights.
2. **Awareness and stealth:** why guards don't perceive player 2 (AC4 P6).
3. **Combat:** who owns the fight director and how it migrates; attacker locks and tokens; fight JIP; kick
   unresponsive.
4. **Missions:** start, end, fail, checkpoint, restart, objective progress, and who owns the logic.
5. **Health:** KO, revive, death, respawn.
6. **Then:** interactions, mission targets, player data, crowd events, Rift / mounted weapons, carriages.
7. **NPC AI-state names:** a live run logging state `+36` and each event's outcome.

**Easy-to-miss checklist.** Each item gets a finding, even if the answer is "local by design":
- saves and progression;
- loot and economy split;
- notoriety;
- pause and menus;
- cutscenes and synced scenes;
- fast travel and loading;
- time of day and weather;
- RNG seeds;
- physics objects, ragdolls and destruction;
- partner HUD, map markers, pings and emotes;
- audio cues;
- difficulty scaling with two players;
- achievements;
- a version or mod handshake on join;
- what ACU deliberately does not replicate (crowds, etc.), and why.

**Batched live checks:**
- the LoadBalance sender;
- the R values;
- ZenHealth +1;
- the AIComponent registry;
- migration modes on leave;
- AI-state names;
- the fight clock unit;
- decoded fight inputs.

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
