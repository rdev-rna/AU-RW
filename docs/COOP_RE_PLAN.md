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
- **Status 2026-11-15 (LAN path, static):** [ACU-RE-0087](findings/ACU-RE-0087-lan-path-static.md).
  - No direct-call path from game code reaches Storm's LAN events. The four LAN type getters are requested
    only by Storm's own registration functions, which have no direct callers.
  - `LanSessionsPage` ships and is listed in `MenuManagerGame` beside `PlayTogetherPage` and the two join
    pages. It borrows the `Menu_Online_Invite_Friends_New` layout and adds no code of its own.
  - Two local games can now run at once (ACU-RE-0086, PR #142), so a live two-instance test is possible.
- **Status 2026-11-15 (LAN path, live):** ACU-RE-0087 dynamic addendum.
  - 0 hits on the four getters, the three type checks and the `LanSessionsPage` factory across 1,342 s,
    covering menus, Go Online, and co-op mission markers (Private and Public).
  - Go Online and Play in Public fail with `0xb0040001` (Ubisoft services unreachable). Play in Private runs
    solo. No flow started a Storm session (`S+0x30` stayed `-1`).
  - Matchmaking is down on every platform (user report). Until a direct friend join can be tested, work is
    static analysis plus offline observation of the live game. The friend-join test is read-only on the
    user's game: session fields and execute counters, no input blocking.
  - Next (static): what the backend hands Storm when a join succeeds (peer addresses, session ID), so our own
    lobby can supply it. Trace from the `0xb0040001` error back to the connect step.
- **Status 2026-11-15 (error code, static plus live):** [ACU-RE-0088](findings/ACU-RE-0088-online-error-code.md).
  - The popup code is built at runtime from an error record: `(category << 28) | code`, appended by
    `0x141C551C0`. The category is ORed in without masking the code.
  - The error-category registry has IDs -1, 0, 1, 2, 3, 6, 7, 8 and 9 (OSDK 3, UbiServices 7, Hermes 9).
  - Live (hook on the formatter, user pressed Play in Public twice): the record is `{3 OSDK, 0x80040001}`, which
    maps to message `544321`. The static guess of "category 11" was wrong.
  - Path: OSDK result → async job finished by `0x1425712A0` → completion callback `0x1425045A0` → message
    table → popup. No Storm session is involved.
  - Live run 2 (job path): the failing job is the connect of online service **selector 7** (`qword_145196D18`,
    constructor `0x1424A3F40`). Its connect handler `0x1424FFFF0` gets the OSDK error and so never sets the
    service's state `+152` to 3 ("connected"). The menus and the UI callback test that field.
  - Seam for our own backend: bring selector-7 to state 3 (or satisfy the menus' checks) without OSDK, then
    find what Storm needs next.
  - Static follow-up: selector 7 is a general online service. Eleven menu functions test its state (player
    or friends lists, invite actions), and so do the ghost and online world features, together with selector 1.
    Requests to it go through `0x1424DBD50` and `0x1424DBF00`. Nothing here reaches Storm. Co-op needs the
    matchmaking or join step after it.
  - Next: find the co-op Public handler that first needs selector 7, then the step after the connect
    (matchmaking or join) that would hand Storm its peers. Live, read-only: hook the service's vf0, the
    connect start.
- **Status 2026-11-15 (Storm session create, static):** [ACU-RE-0089](findings/ACU-RE-0089-storm-session-create.md).
  - Game code creates a Storm session only through `NetSessionController` (`qword_145212ED0`): its Storm
    interface slot 17 posts `session::inEvent::Create`. Storm has no `Join` input event.
  - Three game callers: `0x141C46B10` (descriptor `"default"`, reached from `OnlineUniverseComponents` and an
    online statechart), `0x141C46E30`, and `0x141C470A0` on S (`qword_145212F48 + 32`).
  - `Create` takes a session handler and a descriptor, with no peer addresses. It refuses (`-1000000`)
    unless the Storm product state is 3 and the descriptor's `+72` property is set.
  - Our own lobby therefore has to supply three things: the descriptor contents (likely a session ID at
    `+72`), Storm in state 3, and peer addresses for Storm's peer layer. Next: what fills `+72` on a client,
    and how remote peers enter Storm (`punch::ConnectEndPoints` or the peer manager).
  - Follow-up: the `+72` ID is an 8-byte string. The controller's start-up (`0x141C61EE0`) generates one from
    the game's seeded random generator, then posts `player::inEvent::RequestAddPlayer`. Remote endpoints
    (`punch::ConnectEndPoints`, session peer messages) are reached only inside Storm. No game function hands
    Storm a remote address. The first remote contact must come from the joiner's descriptor or ticket, or
    from the RDV matchmaking layer (a backend boundary). Next: the live, offline Play in Private test from
    ACU-RE-0089.
  - Live, offline (user's game, probe only): Play in Private hit none of the start-up, add-player or Create
    functions. The controller stayed idle (state 1, empty ID, peer -1). The solo path never starts Storm, so
    offline play can't show session start. Next is static: how the matchmaking or invite result is turned
    into the joiner's descriptor or ticket, to find what our own lobby must provide.
- **Status 2026-11-15 (network mode, static):** [ACU-RE-0090](findings/ACU-RE-0090-network-mode-flag.md).
  This revises ACU-RE-0087.
  - A global `dword_144E52380` selects the network mode: 0 = offline or local, 1 = LAN, 2 = online (the retail
    value). It sets the session controller's network type (interface `+676`). In mode 1, Storm's `Create`
    generates the session ID with LAN `GenerateSessionId` instead of refusing an empty one.
  - So the LAN session path exists in code, selected by one global that no menu sets (no writer found yet).
    Plan option A has a concrete entry.
  - Next (live, reversible): launch suspended, set the flag to 1 before net start, check `+676 == 0`, and hook
    LAN `GenerateSessionId` while starting a co-op mission. If that works: two local games in mode 1 (two
    games at once already work, ACU-RE-0086).
  - Live (LAN mode): writing `1` after unpacking holds. The controller switches to network type 0, the game
    and party Storm sessions start at boot (peers 1 and 2), and starting a co-op mission creates a mission
    session (LAN `GenerateSessionId`, peer 3). The map hides the co-op markers in this mode, and nothing shows
    in the UI. Next: find the reader that hides co-op markers in mode 1, then run two mode-1 games and see
    whether the second discovers the first.
  - Live run 2: the co-op mission start in LAN mode enters the private-match host flow
    (`PRIVATE_MATCH_HOST`, start-alone off), creates the mission session and waits for a partner. The map
    hides other missions meanwhile. Next: two mode-1 games, the second trying to join.
  - Live run 3 (two games, one PC): the second game gets no Storm sockets (UDP 3074/6250/6300 held by the
    first), so its Private loads the mission solo. Storm bound to the ZeroTier adapter. Next: a second
    machine in mode 1 on the same LAN or ZeroTier network (the friend test).

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
- **Status 2026-11-15 (intentions):** [ACU-RE-0079](findings/ACU-RE-0079-intention-payloads.md). Slot layouts of
  all 94 `NetHumanIntention` messages come from the builder, using the new tool
  `tools/net_intention_payloads.py` (output in `local/coop/`). Still open: field meanings and bit widths.
- **Status 2026-11-15 (intention receive):** [ACU-RE-0080](findings/ACU-RE-0080-intention-receive.md). Messages are
  `NetNpc` protocol entries. One handler decodes all 94 and starts the same intention locally through
  StartIntention (91 of 94; 3 call send primitives). This corrects ACU-RE-0075's "resync".
- **Status 2026-11-15 (intention send):** [ACU-RE-0081](findings/ACU-RE-0081-intention-send.md). StartIntention
  runs the intention locally, then (if asked to replicate) gets a message from the net dispatcher, which
  is inferred to call the builder, and queues it per thread. Still open: the meanings of the mode and
  context fields.
- **Status 2026-11-15 (flush):** [ACU-RE-0082](findings/ACU-RE-0082-intention-flush.md). Batches per thread and
  per manager go onto dirty lists, which the network tick flushes. Kind 1/2/3 records become replica
  message, master broadcast, master unicast, or forward-to-all-but-target. State 1 = replica (this
  corrects ACU-RE-0081). Intention replication is now traced from game action to Storm send.
- **Status 2026-11-15 (second dirty list):** [ACU-RE-0083](findings/ACU-RE-0083-intention-cleanup-and-join-resync.md).
  The second list deletes released intention instances at the tick; it is not a snapshot flush. When a peer
  is added, the master re-sends every active intention to it (join-in-progress). This corrects ACU-RE-0080's
  "context time". The snapshot send path is still untraced.
- **Status 2026-11-15 (snapshot send):** [ACU-RE-0084](findings/ACU-RE-0084-snapshot-send-path.md). Mementos are
  registered per Storm object type (11 entries; `HumanAvatar` is on `NetPlayer`). Each tick, after the
  message flush, the game stamps every local memento with the session time, runs its writer, and posts
  `SynchronizeMemento`. Storm handles serialization and pacing (not traced). This corrects ACU-RE-0075
  (`+0x50` is a time stamp). Bit widths are still open.
- **Status 2026-11-16 (AI authority for the AC4 mod, static):** [ACU-RE-0091](findings/ACU-RE-0091-coop-ai-authority-for-ac4.md).
  The 1,175 message names are grouped by family and mapped onto the AC4 co-op mod's open problems. The
  ConflictLoopDirector ticket builder `0x140A89750` (request fields and wire IDs) is observed. Remote players use
  their own class, `HumanReplica`. Damage on a non-owned body is inferred to go to the owner through
  `SendFightDamageEventToMaster`. Next: a live hook on that message's builder block in the two-game testbed.
- **Status 2026-11-16 (UI-free hosting, live NPC ownership):** [ACU-RE-0092](findings/ACU-RE-0092-private-host-emulation-and-live-npc-ownership.md).
  Play in Private is emulated by `CALLAT 0x14250CD00` (`tools/coop_lan/privhost.py`); discovery alone then brings the
  joiner in. In one live fight the joiner, not the host, ran the enemy's AI and broadcast its intentions;
  `SendFightDamageEventToMaster` did not run, because the owner hit its own NPC.
- **Status 2026-11-15 (solo live probe, Phase 7 start):** [ACU-RE-0085](findings/ACU-RE-0085-solo-live-probe.md).
  First live run (read-only, lock #30, saves put back). In solo, typed intention starts reach the replicate
  path (30/s), but the builder never runs. The memento push runs every frame and stops at "session active".
  Session-object and global-sender virtuals are resolved; the `+0x80` → builder delegate is observed for the
  global sender.

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

## LAN join plan (2026-11-16, after ACU-RE-0090)

Where things stand:
- LAN mode works for Storm: the host has free-roam game and party sessions, and Private hosts a co-op lobby.
- Nothing in the UI finds a LAN host. The hidden `LanSessionsPage` is empty, and Public fails on a UbiServices
  check (`0x70000106`) before it starts LAN search.
- Storm's LAN discovery is built in. The LAN matchmaker's slot 4 (`0x1425406D0`) turns it on, and a discovery
  handler is installed at startup.

Steps, in order. Each one is a small stride with its own finding update:
1. **Done (ACU-RE-0090 run 7): the gate is a backend request.** Public waits on an async request to service
   selector 7, which fails with `{7, 0x106}`; there is no local flag to bypass. Original step:
   **Find the UbiServices gate in front of Public.** Hook the error formatter `0x141C551C0` and the Public path
   while the user presses Public, then follow the return chain to the check that raises `{7, 0x106}`.
   - If the check is a flag or state read, a probe `REDIRECT` or a data write may let Public reach slot 4. No
     code patches: code patches kill the game (ACU-RE-0090 run 4).
2. **Done (ACU-RE-0090 run 8):** `CALLAT` built; calling LAN matchmaker slot 4 turns on discovery, and
   Storm posts DiscoverSession about twice a second. Original step:
   **Probe call feature (only if step 1 has no data-level bypass).** Add `CALLAT <slot> <target> <rcx> <rdx> <r8>
   <r9>` to `probe.cpp`:
   - it runs once, on the game thread, at the next hit of an armed execute hook (for example Storm slot 12, which
     runs every frame);
   - it uses a trampoline that saves the registers, calls the target and resumes;
   - it runs only with an explicit command, and the hook is removed afterwards.

   First use: call LAN matchmaker slot 4 on `*(qword_145137930 + 1792)`, then confirm that `+692` = 1 and that
   `0x141B35E10` fires.
3. **Done (ACU-RE-0090 run 13):** discovery stores the host's mission session as a record in the joiner's
   `+152` map. ACU's `JoinSession` `0x141C54B30(mission_session, *(rec+640), rec+24, 0)`, called with `CALLAT`,
   moved the joiner's game session onto the host's (same id, synced clock, game traffic). The user saw the other player in the game windows.
   Original step:
   **Follow the discovery result.** Read the handler that `0x141B34FC0` installs. Find what a
   DiscoverSession reply produces (a session list, or a join request) and what starts the join:
   - `PRIVATE_MATCH_JOIN`, `JOIN_PARTY_INVITE` or `GHOST_JIP` in the matchmaking statechart;
   - Storm `RequestAddPlayer` (interface slot 31).
4. **Done (ACU-RE-0090 run 9):** the built-in port offset `byte_145218C33`, rewritten during startup, gives the
   second game 3084/6310 (6250 shared); both games run Storm. Original step:
   **Two games on one PC.** The second game needs its own ports, set as data (not code). Find the Storm config
   object built by `0x141B46C90`, and the time between its construction and the socket bind. Then write `+8`,
   `+412` and `+416` in that window.
   - Open question: discovery may need a shared broadcast port. If so, the one-PC test may need only the main
     port moved.
5. **Friend test.** Two machines, both in LAN mode on the same local network or with direct reachability. ZeroTier
   is excluded by the user. Attach to the user's game read-only, and use the friend's game only as they allow.

Ruled out:
- the hidden LAN page;
- patching code bytes;
- one-PC tests without moved ports;
- ZeroTier.

## Immediate next steps

1. Claim #85, or open a new slice "coop: protocol atlas", and reserve an ACU-RE
   number for the census finding.
2. Take `lock:ida-rea-session`, bind the matching IDB and check its identity.
3. Phase 1, first three owners. Phase 2 LAN-path test, done statically.
