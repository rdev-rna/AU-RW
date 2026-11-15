# ACU-RE-0073: Where does ACU's game code meet the Storm transport and the Ubisoft backend?

Date / investigator / related task: 2026-11-15 / Claude (Opus 5.5), static only / #85, co-op plan Phase 2 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md))
State: partially established. The net subsystem layout, the session relay, the Storm API vocabulary and the NAT-traversal backend dependency are observed. The static LAN-path test is **unresolved**.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: the same capture and analysis view as
[ACU-RE-0072](ACU-RE-0072-net-protocol-registry.md): mapped image `9a1124f1…0520` and analysis view
`47a81dc9…d8c2`, image base `0x140000000`.
Corpus path and lines / export identity status: not used. VAs were read in IDA on the analysis view.
Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable.
Tool/provider/version / evidence IDs or saved output: IDA Pro 9.3, ida-pro-mcp idalib, headless session
`acu85`, run on the private IDB copy `local/85-coop-ida/`. I used decompile, xref and string reads only:
no renames, no patches, no save. Lock #31 was held. The Storm event vocabulary comes from the type records
in `local/coop/net_protocol_atlas.json` (ACU-RE-0072 tool). The class names come from
`local/symbol_index.sqlite` (ACU-RE-0060). An offline string scan of the mapped image covered the toggles
and the URL fields.
Exact commands and search boundary: the IDA queries are listed per observation. The string scans covered
the whole mapped image.

## Result

### Observations

1. **Net subsystem root.** `sub_141C39890` stores itself in `qword_1451C5F78`. It then heap-allocates and
   constructs 13 objects in this fixed order and finishes with a virtual call on `qword_145219C88`. I named
   the first four by matching the vtable their constructor stores against the symbol index:

   | Constructor | Size | Class |
   |---|---|---|
   | `sub_141C3AC50` | 0x6B8 | `NetSessionController` |
   | `sub_141C39EB0` | 0x6A8 | `NetGameManager` (also stores a `Statechart` vtable) |
   | `sub_141C3AB20` | 0xC8 | `NetPartyManager` |
   | `sub_141C3A5F0` | 0xC8 | `NetMissionManager` |

   The other nine (`sub_141C3AFD0` 0xC0, `sub_141C3A850` 0x230, `sub_141C396E0` 0x100,
   `sub_141C3B250` 0x210, `sub_141C39CC0` 0x120, `sub_141C39E20` 0x140, `sub_141C3A3B0` 0x198,
   `sub_141C39030` 0x308, `sub_141C39190` 0x110) store no reflected vtable and are not named yet.
2. **`NetSessionController` is an event relay.** Its new virtual slots 11, 13, 15, 19 and 21
   (`0x141C60260`, `0x141C5F980`, `0x141C61890`, `0x141C5EBF0`, `0x141C588D0`) all have the same shape:
   - First, `sub_141C4C8C0(controller, id)` finds the session object whose virtual `+0x30` returns `id`.
     It searches the pointer list at `controller + 0x2D0` (u16 count at `+0x2DA`).
   - Then the function calls one session virtual, at `+0x178`, `+0x180`, `+0x188`, `+0x190` and `+0x198`
     respectively.
   - Finally it calls every listener stored under that session in the map at `controller + 0x350`, through
     listener virtuals `+0x28` to `+0x48`.

   The controller holds no socket or transport code in these slots.
3. **Storm/Echo API surface.** These names come from self-checked type records (ACU-RE-0072). They are
   names only; their behaviour is not read.
   - `storm::echo::session::inEvent` (17): `Create`, `AcceptJoinRequest`, `RefuseJoinRequest`, `Kick`,
     `Leave`, `Secede`, `Dissolve`, `TransferHost`, `ChangeSessionDescriptor`, ticket requests and removal,
     `SetSessionHandler`.
   - `storm::echo::session::outEvent` (12): `HostElected`, `HostMigrationStart/End`, `JoinRequest`,
     `JoinRequestMissingPlayers`, `PeerConnected`, `PeerDiscovered`, `PeerGone`, `PlayerJoined`, `Leave`,
     `SessionDescriptorUpdate`.
   - 32 `storm::echo::session::*Message` peer messages, including `SessionHostElectedMessage`,
     `SessionHostTransferMessage`, `SessionExitHostMigrationMessage`, `SessionPeerUnreachableMessage`,
     `SessionReJoinRequestMessage` and `SessionTicketRequestMessage`.
   - `storm::echo::object::inEvent` (24): `Create`, `AddReplica`, `RemoveReplica`, `AcquireMasterRole`,
     `GiveMasterRole`, the `AcceptReplaceGoneMasterCandidacy` / `RefuseReplaceGoneMasterCandidacy` pair,
     `SetReplicationMode`, `SetRefreshRate`, `SetMigrationMode`, `SynchronizeMemento`, and others.
     `outEvent` has `MasterBroadcast`, `MasterUnicast`, `ReplicaMessage`, `ReadMemento` and `Ready`.
   - `storm::echo::lan` has `DiscoverSession`, `SetDiscoveryHandler`, `GenerateSessionId`,
     `BroadcastLanDiscovery2` and `outEvent::DiscoveredSession` (string names; the records are not
     populated in this capture).
   - `storm::echo::time::inFunc::GetSessionTimeInMs`.
   - `storm::echo::registry::inEvent::AddObjectType`, `AddObjectProtocolEntry` and
     `AddObjectMementoEntry`.
   - `storm::punch` has `LoginServer`, `DetermineNATType`, `ConnectEndPoints`, `ResolvePublicAddress`
     and a `traversalLogin` challenge state machine.
4. **Storm components.** The source paths name pluggable default controllers under
   `storm/private/defaultcontrollers/src/`: `defaultsessioncontrollerbase`, `defaultobjectcontroller`,
   `defaultobjecthandler`, `defaultpeerchannelcontroller`, `defaultsimulationcontroller`,
   `defaultvoicechatcontroller` and `globalcontroller`. A module-name table at `0x143635308` to
   `0x143635430` lists `session`, `group`, `qos`, `punchclient`, `punchdetect`, `punchtraversal`,
   `punchserver`, `relay`, `networkmanager`, `transportmanager` and `netobjectsynchronization`.
5. **NAT traversal depends on the backend.** `sub_1424E17B0` reads the keys `detectUrl1`, `detectUrl2`
   and `traversalUrl` through `sub_142886730`. That function is the ubiservices parameter lookup that also
   logs "Empty punch url is returned". It reads from the object at `qword_145199610 + 0x138` and returns
   false if that object is missing. The key strings sit next to `rdv::GameSessionClient`, `PunchGUID` and
   `prudp:/`.
6. **Static sends use a global object.** `sub_14015BA20` sends
   `zenitar_AccomplishmentEventsNetHelper_NetBroadcastAccomplishmentEventMSG_NetProtocol_ID`. It uses the
   global `qword_145139D00` in the place an entity would take, getting the dispatcher through
   `qword_145139D00 + 0x110` and vtable `+0xC0`, and calls a second send primitive, `sub_140221C70`.
7. **LAN: no game-side wiring found.** The getters for the LAN event types (`sub_141B752A0`
   DiscoverSession, `sub_141B7A500` SetDiscoveryHandler, `sub_141B76450` GenerateSessionId,
   `sub_141B748A0` BroadcastLanDiscovery2) are referenced only from Storm-internal factory tables
   (`sub_141B8A4F0`, `sub_141B8A680`). `LanSessionsPage`'s own virtuals (`0x1408E4BB0`, `0x1408EB290`)
   only serialize page properties. Their type records are null in this capture, so nothing requested them
   during the captured single-player run.
8. **The singletons live on the heap.** `qword_145139D00`, `qword_1451C5F78`, `qword_145199610`,
   `qword_145212F48` and `qword_145219C88` all point outside the image, so this capture cannot name their
   classes.

### Inferences and alternatives

- **The P2P session needs no server once peers can reach each other.** Storm's own session protocol carries
  host election, host transfer, host migration, re-join and peer-gone messages. The backend's visible roles
  so far are: finding sessions (RDV / ubiservices matchmaking), supplying the NAT-traversal server URLs
  (observation 5) and possibly issuing tickets. The alternative is that `SessionTicket*` messages carry a
  backend-signed credential that peers check. That would put the backend inside the join path and needs
  checking next.
- **The seam for our networking is Storm's event interface.** Game code posts `inEvent`s and handles
  `outEvent`s through `NetSessionController` and its listeners. The game never talks to sockets directly.
  Plan option A becomes: supply peer addresses ourselves (LAN discovery or our own lobby) and skip
  `punch`. Option B becomes: implement the `storm::echo::{session,object,peerChannel,time}` event contract
  on our own transport.
- `NetSessionController` vf11 to vf21 are probably the Storm session-handler callbacks (the `outEvent`
  set). Matching each slot to an event needs the code that calls into these slots.

### Unknowns / incomplete coverage

- **LAN-path test (plan Phase 2): unresolved.** No static evidence shows whether ACU ever starts Storm's
  LAN product, or which setting enables it. The next steps are: find who posts `lan::inEvent::*`, which
  probably goes through a generic post-event call taking a type record, and find who creates
  `LanSessionsPage`.
- Which Storm code calls `NetSessionController`'s slots, and the mapping from slot to `outEvent`.
- What the nine unnamed net singletons are, and the classes of the heap globals. Both need a live capture.
- Whether tickets are backend-signed.
- The receive path from Storm to the per-owner decoder (carried over from ACU-RE-0072).
- Whether `relay` (observation 4) is a backend relay server.

### Independent comparison / falsification test

- The five relay slots were read separately. Each one has the same lookup, the same fan-out and
  consecutive vtable offsets. One slot breaking the pattern would have refuted the relay reading.
- Observation 5 is a direct code read. A live check would hook `sub_142886730` and expect the
  `traversalUrl` key during online start-up.

### Reproduction steps

Decompile `0x141C39890`, `0x141C60260`, `0x141C4C8C0`, `0x1424E17B0`, `0x14015BA20` and `0x141B752A0`.
Query the atlas type records for `storm::` names. Read only; don't paste their code into the repo.

### Implementation implications and finding-linked constants

- Session lookup list at `NetSessionController + 0x2D0` (u16 count at `+0x2DA`); listener map at `+0x350`.
- The punch configuration keys are `detectUrl1`, `detectUrl2` and `traversalUrl`. They are recorded in the
  backend boundary register; we don't call or emulate that service.
