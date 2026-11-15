# Co-op networking reverse-engineering plan

Status: plan, 2026-11-15. Area `area:coop`, epic #26, scope note #85.

## Goal

Work out how Assassin's Creed Unity runs co-op: sessions, peers, authority, what gets
replicated and how, join-in-progress, host migration, and how co-op changes
gameplay. The result is an engine-level spec good enough to bring co-op to other
Assassin's Creed titles, which ship without it. **Static analysis comes first**:
the pseudocode corpus, the reflection symbol index and the IDA database. Live hooks
come later, and only to confirm what static work predicts.

This is a separate track from the Bevy reconstruction. ROADMAP.md still defers online
play for the rewrite, and this plan does not add a multiplayer requirement to it. The
user explicitly asked for this track on 2026-11-15.

## Ground rules

- **B1 - No Ubisoft backend work.** We do not decode, emulate or talk to Ubisoft
  servers (Quazal RendezVous, Hermes, ubiservices, Uplay or the matchmaking and
  NAT-traversal services). Each call into that layer is **recorded** in the backend
  boundary register (Phase 5) with its VA, caller and purpose, and then left alone. We
  will use our own networking.
- **B2 - Evidence rules from AGENTS.md apply.** Strings and class names are leads.
  Behaviour needs callers, disassembly or pseudocode plus provenance (image SHA-256,
  VA, corpus path and lines). Keep observations, inferences, choices and unknowns
  apart. Use the `docs/templates/FINDING.md` template.
- **B3 - Public repo.** Issues and PRs hold addresses, hashes and names only. No game
  bytes and no decompiled code. Credit and re-derive third-party work; don't paste it.
- **B4 - Locks.** Hold `lock:ida-rea-session` before you bind IDA or REA, and
  `lock:game-process` for anything live (docs/PROJECT_BOARD.md).

## What the image already shows (census, 2026-11-15)

Source: `tools/net_census.py` run on the captured mapped image
`motion-link-20261020/runtime-view/capture/main-image.mapped.bin`, SHA-256
`9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, image base
`0x140000000`, plus `local/symbol_index.sqlite` (ACU-RE-0060). The output is
deterministic: two runs gave byte-identical JSON. It reads only string and class
locations; **none of this establishes behaviour yet.**

```powershell
python tools/net_census.py `
  --image "E:/ReverseEngineering/Projects/Unity/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin" `
  --expect-sha256 9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520 `
  --symbols local/symbol_index.sqlite --output local/coop/net_census.json
```

The strings fall into three layers. Each layer gets a different treatment.

| Layer | What the strings show | Count | Our treatment |
|---|---|---|---|
| **Game replication** ("zenitar" NetProtocol) | `<Type>_NetProtocol`, `<Owner>_<Msg>MSG_NetProtocol`, `..._NetProtocol_ID[_UNICAST]`, `FactoryEntry<Type>` twins | 963 distinct message types and 198 distinct state types, each with a `FactoryEntry` twin (2,240 strings in all); 819 `_UNICAST` IDs | **Port.** This is the co-op logic. |
| **Transport / session** (Storm, Echo) | `storm::echo::{session,object,group,player,simulation,time,QOS,peer*,lan,voicechat}`, `storm::punch::*`, `storm::link`, `storm::emulator`, "Storm Core", "StormNetworkThread" | 689 `storm::` strings | **Understand, then replace or shim.** We need its semantics (replicas, ownership, time). We drop its NAT punch and matchmaking. |
| **Online backend** (Ubisoft) | Quazal copyright string at `0x143617EA0`, RendezVous source paths (`extern/online/tg/rdv/...`), `Hermes*`, ubiservices, `uplay_r1_loader64.dll` | about 1,600 strings | **B1: record the boundary only.** |

Leads by layer. Each entry is a reflected class (symbol index) or an image string:

- **Session and control:** `NetSessionController` (0x6B8 bytes), `NetSession`,
  `NetSessionKey`, `NetSessionControlChannel` (string), `NetGameManager` (0x6A8),
  `NetPartyManager`, `NetMissionManager`, `NetUniverseComponent`,
  `OnlineUniverseComponents`. Host migration has events on both sides of the seam:
  `storm::echo::session::outEvent::HostMigrationStart/End` (0x142FFE280) and the RDV
  notification "onHostMigrated" (0x1435D6E80). The `MissionHostMigratedEarly_ReturningToLobby`
  string suggests mission code knows about migration.
- **LAN path:** `LanSessionsPage`, `storm::echo::lan::*`. *Inference to test first:*
  a LAN session may run Storm/Echo without the RDV backend. If it does, that is the
  cheapest seam for our own networking.
- **Entity replication:** `NetComponent` (0x278) → `NetGameComponent` (0x320);
  `NetWorldComponent` (0x500) → `NetGameWorldComponent` (0x540);
  `storm::echo::object*`, `objectObserver`, `DeleteReplicaEvent`,
  `ObjectShapeMessage`, `streamPacker`; `NetDataContainerSerializeHelper`;
  `ProxyManager` / `PlayerProxyContext`.
- **Player avatar:** `HumanReplica` (0x9A0, parent `HumanBase`), `BhvAssassinReplica`
  (0xBF0), `HumanAvatar_NetProtocol`, `PlayerNetSpawnInfo`, `PlayerCharacterData`,
  `ZenHealthReplication`, `AnimComponentNetData`, `AnimSync{Die,Fall}Event[Seed]`.
  `NetHumanIntention` has 94 message types, which hints that players replicate
  *intentions* rather than poses (inference, to verify).
- **AI and NPCs:** `NetBehaviorAI` (171 message types, the largest owner),
  `EntityAINetData`, about 100 `GcL*NetData` types, `FightDirector` (78) and
  `AwarenessDirector` (34) messages. `CSrv*` (for example `CSrvPlayerHealth`) next to
  `CL*` hints at a server-side / client-side split inside AI behaviours (inference).
  `AwarenessDirector_RequestDetectiveOwnership` shows ownership requests.
- **Combat:** `NetFight2Component` and `NetFight2WorldComponent` (0x738),
  `NetFightManager`, `NetFightPeer`, `NetFightJIPManager` (RequestJIP, SendJIPReply,
  SendJIPSnapshot, JIPFailed, at 0x142EC14A8), and
  `NetFightRecording[_Tick]`, `NetFightReplayInput/Join/Leave`. These hint at
  input-replay combat sync (inference).
- **Crowds:** `GcLCrowdLifeNetContext`, `NetFakeCrowd` (0x600),
  `BulkShepherdNetComponent`, `NetBulkManager`, `CrowdHerderGroupMigration`.
- **Missions and scripting:** `MissionNetworkData`, `MissionReplicationPack`,
  `Mission{Step,Group,Condition,Constraint,DataLayer}NetworkData`,
  `MissionMatchmakingManager` / `MissionMatchmakingLogic` (a statechart),
  `MissionPlayerCountCondition`. Domino (visual scripting) has `NetworkActionOperator`,
  `NetworkFlowOperator`, `DominoJIPSnapshot`, `DominoNetworkOperatorSnapshot`, and
  `TimerNetData` / `DelayNetData` / `ForEachNetData` / `ForLoopNetData`.
- **Co-op gameplay and UI:** `CoopHealingEvent`, `CoopGatingComponent`,
  `HUDCoopBriefing*`, `HUDRemotePlayerWidgetModule`, `PeerPlayerIconComponent`,
  `AddRemotePlayerEvent` / `RemoveRemotePlayerEvent`,
  `DisableQuickSelectCoopSkillEvent`.
- **Debug aids worth finding early:** `NetworkEmulator`, `storm::emulator::*`,
  `NetworkHistory`, `NetScryMonitor[Game]`, `storm::stats::BandwidthStats*`,
  `UnitTest_*_NetProtocol`.
- **OS:** the image imports `WS2_32.dll` (`WSAStartup`, `WSASocketA`, `sendto`,
  `getaddrinfo`). This is where the lowest-level socket shim would sit.

## Phases

Each phase becomes one or more `type:re` issues under epic #26. Each phase lists its
deliverable and acceptance criterion. Phases 1-5 are static. Phase 7 is dynamic.

### Phase 0 - Setup (one session)
- Bind the IDB that matches the symbol-index image,
  `local/native-probe/motion-link-20261020/runtime-view/ACU.runtime-analysis.pe.i64`.
  Check the image SHA before trusting any VA (ACU-RE-0031 covers corpus identity).
- Reserve an ACU-RE number for the census finding. Open the Phase 1-6 issues.
- **Done when** the census finding cites the image SHA and the tool SHA, and the IDB
  identity is recorded.

### Phase 1 - Protocol atlas (static)
- Follow xrefs from each `_NetProtocol_ID` and `FactoryEntry*` string to its
  registration site. Recover the registry layout: message ID, owner type, factory,
  serializer and handler pointers. Check whether the 819 `_UNICAST` IDs are a
  separate dispatch path.
- Join the registry to the reflection classes: owner class → message list →
  handler VA.
- Deliverable: `tools/net_protocol_atlas.py` → `local/coop/net_protocol_atlas.json`
  (regenerated, not committed: public repo) and a finding on the registry format.
- **Status 2026-11-15:** registry format, ID scheme and type records established in
  [ACU-RE-0072](findings/ACU-RE-0072-net-protocol-registry.md). The three acceptance
  owners resolve by two routes. Still open: the receive path and the vf0 prefix strings.
- **Done when** at least 3 owners (`NetHumanIntention`, `NetFightJIPManager`,
  `MissionMatchmakingNetHelper`) resolve end to end, from string to handler, by two
  independent routes (xref and vtable/factory).

### Phase 2 - Session and transport model (Storm/Echo, static)
- Session life cycle: create, join, leave, kick (`MatchmakingKickPeerEvent`), and
  where `NetSessionController` calls into `storm::echo::session` versus into
  Hermes/RDV. **That call boundary is the seam for our networking.**
- Peers: peer manager and channels, logical connections, router, and the
  `storm::link` state machine.
- Host: how the host is picked, what "host" owns, and the host-migration sequence
  across Echo, the mission layer and the RDV notification.
- Time: `storm::echo::time` / `session_time`. Tick or clock sync between peers.
- LAN: how `LanSessionsPage` discovers sessions and whether the LAN path needs any
  backend call. This is the first falsification test for the cheapest seam.
- Mark NAT punch (`storm::punch::*`), QOS probes, bans and voice chat as backend-adjacent:
  record them, don't port them. Voice is out of scope.
- Deliverable: a session state diagram and a seam finding.
- **Status 2026-11-15:** [ACU-RE-0073](findings/ACU-RE-0073-session-transport-seam.md).
  The seam is Storm's event interface, and `NetSessionController` is a session-event relay. NAT traversal
  takes its server URLs from ubiservices (backend row 1). The LAN-path test is still **unresolved**.
  Backend register started: [coop/BACKEND_BOUNDARY.md](coop/BACKEND_BOUNDARY.md).

### Phase 3 - Replication model (static)
- Echo objects: how a replica is created and deleted, what an "object shape" is
  (likely a schema or property layout, unverified), groups, observers and the
  simulation manager.
- Authority and ownership: who simulates each object, how ownership moves
  (`RequestDetectiveOwnership`, `CrowdHerderGroupMigration`), and what happens to
  owned objects when a peer leaves.
- Serialization: the stream packer, `NetDataContainerSerializeHelper`, quantization,
  and delta versus full state.
- Rates and priority: update frequency, bandwidth budgets (`BandwidthStats`) and
  relevance or interest management.
- How engine entities attach: `NetComponent` / `NetGameComponent` on entities,
  `NetWorldComponent` on the world, `ProxyManager` for remote stand-ins.
- Deliverable: a replication spec, engine-neutral and written as data flow, plus
  findings with VAs.

### Phase 4 - Gameplay replication (static, per subsystem)
One issue per subsystem, in this order. Players come first because co-op is
pointless without them.
1. **Player avatar**: spawn (`PlayerNetSpawnInfo`), intention versus pose
   replication, animation sync, health, inventory, the `HumanReplica` and
   `BhvAssassinReplica` behaviour for remote players.
2. **Combat**: NetFight2, the JIP handshake, and recording or replay input (is it
   lockstep, input-delay, or authoritative?).
3. **AI and NPCs**: the `NetBehaviorAI` message set, the `CSrv`/`CL` split, and
   awareness and fight director authority.
4. **Missions and Domino**: mission state replication, `MissionReplicationPack`,
   Domino network operators, and how player-count conditions gate content.
5. **Crowds**: crowd-life context, fake crowd, bulk shepherd.
6. **Co-op mechanics**: healing and revive, per-player desync, co-op gating and
   briefing, shared rewards, remote-player HUD.
7. **Join in progress**: the JIP snapshot path across fight, Domino and mission.
- Deliverable per subsystem: the messages used, who has authority, the trigger, the
  payload fields (where recoverable), and engine dependencies.
- **Status 2026-11-15 (player avatar, first pass):** [ACU-RE-0075](findings/ACU-RE-0075-avatar-replication.md)
  and [coop/AVATAR_SPEC.md](coop/AVATAR_SPEC.md). Every wire ID is
  `CRC-32("zenitar_"+type+"_ID")`, verified for 963 of 963 RFC types. The `HumanAvatar` snapshot codec is
  read field by field. The model is "master simulates, replicas re-enact intentions". Still open: the
  owner class and the field meanings.
- **Status 2026-11-15 (position):** [ACU-RE-0076](findings/ACU-RE-0076-position-replication.md). Position,
  velocity and heading travel in a dead-reckoned proxy pool (`NetProxyManager`, 30×10 slots), and placed
  world entities bind by entity ID. Still open: the receiver side and the bit widths.
- **Status 2026-11-15 (receiver):** [ACU-RE-0077](findings/ACU-RE-0077-proxy-receiver.md). The receiver keeps
  the newest update, and game code reads an extrapolated position that it applies directly. The heading
  decode shows z-up. A shared network clock is required.
- **Status 2026-11-15 (clock):** [ACU-RE-0078](findings/ACU-RE-0078-session-clock.md). The Storm session
  clock starts at 0 on the host. Clients ping it (100 ms, then 2 s), average `host + rtt/2` per 500 ms, and
  slew toward the result with monotonic reads. The path is peer-to-peer only, with no backend crossing.

### Phase 5 - Backend boundary register (runs alongside every phase)
- `docs/coop/BACKEND_BOUNDARY.md`, one row per call that crosses into Ubisoft
  services: VA, calling function, service (RDV, Hermes, ubiservices, Uplay, punch),
  apparent purpose (login, matchmaking, invite, stats, news, commerce) and the
  replacement (stub, our own lobby or relay, or not needed).
- Never decode payloads or protocol DDLs on the far side of this line (B1).

### Phase 6 - Portability spec
- A spec that names the engine features co-op depends on: entity components, the
  event system, Domino or scripting, the animation-sync hooks, and the fixed-tick
  assumptions.
- Target-title survey (static only, separate issues, read-only installs): do other
  AnvilNext titles carry leftover `NetComponent`, Storm or NetProtocol code? Unknown
  today. Syndicate is the most likely, because it shares the engine generation.
  This decides between reusing dormant code and reimplementing it.
- Deliverable: `docs/coop/PORTING_SPEC.md` with a reuse/reimplement matrix per
  subsystem.
- **Status 2026-11-15 (installed titles):** [ACU-RE-0074](findings/ACU-RE-0074-target-title-net-stacks.md).
  No installed title carries Storm/Echo or NetProtocol. Black Flag multiplayer (`AC4BFMP.exe`) runs on
  HackNet/HackNetZ, a Net-Z-style duplicated-object library. The Black Flag campaign, Freedom Cry, AC2 and
  AC1 have no replication framework. **Porting into any installed campaign is therefore option C**: our
  own replication layer, specified by the ACU findings, plus engine glue. Syndicate is not installed yet;
  survey it first when it is (`tools/net_title_survey.py`).

### Phase 7 - Dynamic confirmation (after static work, behind the locks)
- A DLL hook in ACU logs NetProtocol dispatch (message ID, size, direction) and the
  Echo replica life cycle. Two local instances run a LAN session if Phase 2 shows
  the LAN path is backend-free.
- Compare the log with the Phase 1 atlas and the Phase 3 and 4 predictions. Record
  every mismatch as a finding.
- Follow the memories and tooling rules: close only with `capture_run.py --close`,
  never hot-swap a shim, and always set an explicit FPS cap of 300 or lower.

## Our networking: where to cut

There are three possible seams. The plan doesn't choose one until Phase 2 delivers
its evidence.

| Option | Cut point | Good | Bad |
|---|---|---|---|
| A. Socket/backend shim | Keep Storm/Echo. Replace RDV/Hermes matchmaking with our own lobby, and use LAN mode or a socket redirect for transport. | Least code. ACU co-op stays authentic. | Works only inside ACU. Doesn't carry over to other titles. |
| B. Echo API replacement | Reimplement the Storm/Echo session and replica API on our own transport. | Gives us a reference implementation for porting. | Needs the full Phase 3 spec. |
| C. Game-level protocol | Reimplement the NetProtocol message set per subsystem in the target title. | The only option for titles that lack the code. | Biggest job. Depends on the target engine. |

Working hypothesis, to be tested: A is the testbed in ACU, which validates the
spec. C, informed by B, is the porting route.

## Unknowns and risks

- What "zenitar" means as a prefix (a module name? a codegen tool?). Unknown.
- The symbol index comes from one runtime capture. Its class sizes and offsets are
  observations from that capture (ACU-RE-0060).
- ACU-RE-0031 binds the corpus to the installed executable only through headers and
  resources. The installed code is packed, so verify each VA in the IDB.
- The census counts strings, not functions. Dead or stripped code may inflate it.
- Determinism: if combat or AI uses input replay, float determinism across machines
  becomes a porting risk. Not yet established.
- Target titles may lack the engine hooks co-op needs. Phase 6 decides this.

## Immediate next steps

1. Claim #85, or open a new slice "coop: protocol atlas", and reserve an ACU-RE
   number for the census finding.
2. Take `lock:ida-rea-session`, bind the matching IDB and check its identity.
3. Phase 1, first three owners. Phase 2 LAN-path test, done statically.
