# #85 Co-op reverse engineering (area: coop)

Branch `acu-85-coop-re-plan`. Plan: [../COOP_RE_PLAN.md](../COOP_RE_PLAN.md). Files: `docs/COOP_RE_PLAN.md`,
`docs/areas/coop.md`, `docs/coop/BACKEND_BOUNDARY.md`, `docs/coop/AVATAR_SPEC.md`, `docs/findings/ACU-RE-0075-avatar-replication.md`, `docs/findings/ACU-RE-0076-position-replication.md`, `docs/findings/ACU-RE-0077-proxy-receiver.md`, `docs/findings/ACU-RE-0078-session-clock.md`, `docs/findings/ACU-RE-0079-intention-payloads.md`, `docs/findings/ACU-RE-0080-intention-receive.md`, `docs/findings/ACU-RE-0081-intention-send.md`, `docs/findings/ACU-RE-0082-intention-flush.md`, `docs/findings/ACU-RE-0083-intention-cleanup-and-join-resync.md`, `docs/findings/ACU-RE-0084-snapshot-send-path.md`, `docs/findings/ACU-RE-0085-solo-live-probe.md`, `tools/coop_live_probe.py`, `tools/net_intention_payloads.py`, `tools/test_net_intention_payloads.py`, `docs/findings/ACU-RE-0073-session-transport-seam.md`, `tools/net_census.py`, `tools/net_protocol_atlas.py`,
`tools/test_net_protocol_atlas.py`, `tools/net_title_survey.py`, `tools/test_net_title_survey.py`,
`docs/findings/ACU-RE-0072-net-protocol-registry.md`, `docs/findings/ACU-RE-0074-target-title-net-stacks.md`.

## Done (2026-11-15)
- **Phase 0:** IDB identity bound. The analysis view `47a81dc9…` maps to the mapped image `9a1124f1…`. I used
  a private IDB copy in `local/85-coop-ida/`, read-only, under lock #31.
- **Phase 1 (registry format):** [ACU-RE-0072](../findings/ACU-RE-0072-net-protocol-registry.md).
  - Message IDs are CRC-32 of name strings. A unicast ID appends `_UNICAST` before hashing.
  - The image has 1,508 self-checked net type records, all rooted in `storm::*` or `NetObject`. Of these,
    958 are RFC messages and 118 are mementos.
  - 62 static registrars map one-to-one to the `zenitar_` messages.
  - 1,639 object-scoped ID literals are each traced to the functions that use them.
  - There are 217 direct send sites. When a call site has no net handle, the call runs locally.
  - Acceptance owners `NetFightJIPManager`, `MissionMatchmakingNetHelper` and `NetHumanIntention` resolve by
    two independent routes.
- `tools/net_protocol_atlas.py` writes `local/coop/net_protocol_atlas.json` deterministically.
  `tools/test_net_protocol_atlas.py` has 11 synthetic tests (CRC values, record self-check, list cycle,
  missing `.pdata`, function bounds, bad PE). All pass.

## Choice
The full atlas, about 810 KB of identifiers, stays in ignored `local/` and is regenerated from the capture.
The repo is public, so only the tool and the finding are committed. This overrides the plan's earlier
`data/coop/` location.

## Done (Phase 2, 2026-11-15)
- [ACU-RE-0073](../findings/ACU-RE-0073-session-transport-seam.md) maps the net subsystem root and its 13
  singletons (4 named). It shows `NetSessionController` is an event relay, lists the Storm/Echo
  session/object/LAN/punch API from the type records, and finds that the NAT-traversal URLs come from
  ubiservices.
- Backend register started: [../coop/BACKEND_BOUNDARY.md](../coop/BACKEND_BOUNDARY.md) (8 rows).
- LAN-path test: **unresolved**. No game code calling Storm's LAN product was found statically.

## Done (Phase 6 survey, 2026-11-15)
- [ACU-RE-0074](../findings/ACU-RE-0074-target-title-net-stacks.md): `tools/net_title_survey.py` (4 tests)
  over AC1, AC2, Black Flag (campaign and multiplayer), Freedom Cry and ACU (packed and unpacked controls).
  Only ACU has Storm/Echo or NetProtocol. Black Flag multiplayer has HackNetZ. No campaign executable has
  a replication framework, so porting uses option C.

## Done (Phase 3/4 avatar, 2026-11-15)
- [ACU-RE-0075](../findings/ACU-RE-0075-avatar-replication.md) and [../coop/AVATAR_SPEC.md](../coop/AVATAR_SPEC.md).
  - The universal wire-ID rule is verified for 963 of 963 RFC types; the atlas now emits every ID, with
    2 new tests.
  - The `HumanAvatar` snapshot codec is read: 5 raw fields, 8 quantized floats, sequence-gated apply.
  - The 94 `NetHumanIntention` messages are classified, and a resync function was found.
- This resolves the Phase 1 open item: the object-scoped prefix is `zenitar_<Owner>_`.
- The solo live probe was deferred: other linked sessions are active, and lock #30 has a stale ACQUIRE
  with no release.

## Done (position, 2026-11-15)
- [ACU-RE-0076](../findings/ACU-RE-0076-position-replication.md): position, velocity and heading travel in
  the dead-reckoned `NetProxyManager`/`ProxiesProtocol` pool (30×10). All thresholds and encodings are read
  from the sender. `NetPlayer` is a container object, and entity replicas bind by entity ID. This corrects
  the ACU-RE-0075 key (`0xA92DBEA6` = wire ID).

## Done (proxy receiver, 2026-11-15)
- [ACU-RE-0077](../findings/ACU-RE-0077-proxy-receiver.md): receiver `sub_141C66C10`, the per-entity
  proxy state, the extrapolation query `sub_141C4F340` with its step caps, three consumers that apply the
  prediction directly, the heading decode (z-up), and the shared-clock requirement.

## Done (session clock, 2026-11-15)
- [ACU-RE-0078](../findings/ACU-RE-0078-session-clock.md): the game clock family and its fallback latch,
  the Storm time module records, the per-session time record, slewing and monotonic reads, the
  host-origin ping protocol with its wire layout, and client averaging. No backend crossing.

## Done (intention payloads, 2026-11-15)
- [ACU-RE-0079](../findings/ACU-RE-0079-intention-payloads.md): the new tool `tools/net_intention_payloads.py`
  (with 7 synthetic tests) extracts the layouts of all 94 intention messages (549 fields) from the
  builder, written to `local/coop/intention_payloads.json`. IDA would not open (timed out twice), so this
  was done offline with capstone.

## Done (intention receive, 2026-11-15)
- [ACU-RE-0080](../findings/ACU-RE-0080-intention-receive.md): `NetNpc` registration, per-object handler
  binding, the shared receive handler `sub_1407269B0`, the StartIntention family and its gate. This
  corrects ACU-RE-0075 observations 6 and 7. Done offline with capstone.

## Done (intention send, 2026-11-15)
- [ACU-RE-0081](../findings/ACU-RE-0081-intention-send.md): the builder is the second per-message delegate.
  StartIntention's replicate branch, the object and global paths with their case tables, and the
  per-thread record queue. One virtual hop is inferred. Done offline with capstone.

## Done (intention flush, 2026-11-15)
- [ACU-RE-0082](../findings/ACU-RE-0082-intention-flush.md): per-thread batches, manager dirty lists on
  `qword_145213880`, the network-tick flush, and the mapping from record kind to Storm object send. This
  corrects ACU-RE-0081 (state 1 = replica). Done offline with capstone.

## Done (second dirty list, 2026-11-15)
- [ACU-RE-0083](../findings/ACU-RE-0083-intention-cleanup-and-join-resync.md): the `+0x3C` list is deferred
  deletion of released intention instances. The join-in-progress re-send (stored messages re-broadcast,
  active intentions unicast to the new peer) is observed, with the trigger inferred to be AddReplica.
  Corrects ACU-RE-0080 (virtual `+0xD8` is the local index) and ACU-RE-0082. Done offline with capstone.

## Done (snapshot send, 2026-11-15)
- [ACU-RE-0084](../findings/ACU-RE-0084-snapshot-send-path.md): all 11 memento registrations, decoded by
  object type, memento type and wire ID. Also the per-tick push (stamp, write, `SynchronizeMemento`) and
  the `SetMementoDirty` / `SetMementoAllDirty` virtuals. Corrects ACU-RE-0075 (owner `NetPlayer`; `+0x50`
  is a time stamp). Not found: bit widths, Storm-side serialization, the read handler. Done offline with
  capstone.

## Done (solo live probe, 2026-11-15)
- [ACU-RE-0085](../findings/ACU-RE-0085-solo-live-probe.md): first live run (PID 16008, capture_run.py
  --until world, pin capture-start-1, fps cap 60; closed with --close; saves put back). New tool
  `tools/coop_live_probe.py`. It gives hit rates for eight co-op routines and resolves the session-object
  and global-sender virtuals. Solo stops at the network-component and session checks, as predicted.

## Not done / next
- Phase 1 remainder: confirm the decoder, encoder and body roles by tracing the receive path from Storm to
  the decoder. (The vf0 prefix question is resolved by ACU-RE-0075.)
- Phase 2 remainder: who posts `lan::inEvent::*`, whether tickets are backend-signed, the mapping from
  Storm callbacks to `NetSessionController` slots. A live capture (Phase 7) would name the heap singletons.
