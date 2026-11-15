# ACU-RE-0089 — How ACU's game code asks Storm to create a session

Date / investigator / related task: 2026-11-15 / claude-session, static only / #85, co-op plan Phase 2
([COOP_RE_PLAN.md](../COOP_RE_PLAN.md)); follows [ACU-RE-0073](ACU-RE-0073-session-transport-seam.md) and
[ACU-RE-0088](ACU-RE-0088-online-error-code.md)
State: partially established. The path from game code to Storm's session `Create` event is observed, with
its arguments, along with the local start-up (a random 8-byte ID, then `RequestAddPlayer`). Remote addresses
are handled inside Storm, and their entry point is not found.

## Provenance

Binary path / SHA-256 / image base: installed `ACU.exe` SHA-256 `748abdc6…27e40`. Addresses come from the
hash-bound runtime view: mapped image SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`,
base `0x140000000`. Class and method names come from `local/symbol_index.sqlite` (tool SHA-256 `9d6ef9d9…`).
Event type names come from `local/coop/net_protocol_atlas.json` (ACU-RE-0072).
Corpus path and lines: `ACU - Decompiled/<VA>.c`, whole file, for each function named below. The export
identity follows ACU-RE-0031.
Tool/provider: the same Python scans as ACU-RE-0088 (RIP-relative `disp32` references, absolute 8-byte
pointers, vtable walks) and the symbol-index `call` table. No IDA or REA session. Indirect calls are found
only through vtable pointers and decompiled `*(vtable + offset)` text.

## Result

Observations:
1. **Storm handles session input events itself.** The type records for `storm::echo::session::inEvent::*`
   (`Create` `0x145205440`, `AcceptJoinRequest` `0x145205600`, `RefuseJoinRequest` `0x14520D070`,
   `ChangeSessionDescriptor` `0x145205400`, `RequestTicketSessionId` `0x145205640`, `DistributeTicket`
   `0x145205740`, `SetSessionHandler` `0x1452053C0`) are each referenced only by their own type getter.
   - Those getters are called only from Storm's session-event dispatcher `0x141BFD4F0` and its sibling
     `0x141BFD850`, plus small type tests such as `0x141BD0EC0`.
   - The dispatcher tells events apart by asking each event for its type record (vf1) and comparing the
     record's ID.
   - The event list has no `Join` event.
2. **Each event's vtable has its getter at vf1.** That gives the code that builds each event:

   | Event | Vtable | Built in | Reached from |
   |---|---|---|---|
   | `Create` | `0x1430014A0` | `0x141B69150` | `0x141B2CFF0` |
   | `ChangeSessionDescriptor` | `0x143001448` | `0x141B5B000` | `0x141B2C870` |
   | `AcceptJoinRequest` | `0x1430016A8` | `0x141B975F0` | `0x141B8C7B0` |
   | `RequestTicketSessionId` | `0x143001700` | `0x141B469F0`, `0x141B9FE80` | `0x141BFD850`, `0x141B8C7B0`, `0x141BA0860` |
   | `DistributeTicket` | `0x143001848` | `0x141B487D0`, `0x141BA1BE0` | `0x141B4EDE0`, `0x141B8C7B0` |

   Everything in this table lies in Storm's code range (`0x141B2xxxx`–`0x141BFxxxx`).
3. **`NetSessionController` is a Storm session controller.** Its constructor `0x141C3AC50` stores three vtables:
   - `0x14300ED80` at `+0`;
   - `0x14300EE40` at `+8`;
   - `0x14300F098` at `+688`.

   It then stores the object in `qword_145212ED0`. The `+8` vtable mirrors Storm's base controller vtable
   `0x142FFE5B0`, constructed by `0x141B29320`, which this constructor calls on `+8`. In both vtables:
   - slot 17 (`+0x88`) is `0x141B2CFF0`, which builds and posts `Create`;
   - slot 27 (`+0xD8`) is `0x141B2C870`, which builds and posts `ChangeSessionDescriptor`.
4. **Three game functions call `Create`.** Each calls `(*(qword_145212ED0 + 8))->vf17`:

   | Caller | What it passes | Reached from |
   |---|---|---|
   | `0x141C46B10` | a new descriptor named `"default"` | `OnlineUniverseComponents::helper_140F5EF90`, `0x142550900`, `0x142569B50` |
   | `0x141C46E30` | a new descriptor | `0x142550900` |
   | `0x141C470A0(obj, descriptor, n)` | the caller's descriptor | `0x141C350E0`, on the session object S = `qword_145212F48 + 32` (ACU-RE-0073, sampled live in ACU-RE-0087) |

   `0x142569B50` is called from two `StatechartState::vf8` functions (`0x142552BC0`, `0x142552D20`) and from
   `0x14255C3E0`.
5. **The `Create` arguments.** The call is `Create(controller+8, &result, handler, &descriptor, n, &context)`:
   - `handler` comes from `0x141C42780(qword_1451C5F78 + 1256)`. That is a new 128-byte session handler with
     vtable `0x14300E918`, built by `0x141B29A40` from `qword_145212EE0`, with callback `0x141C6DAD0` bound to
     the net root's `+1256` object.
   - `descriptor` is built by `0x141B29AD0`, with vtable `0x142FFDFE8` and more than 400 bytes of typed
     property fields.
   - There is no peer-address argument.
6. **`Create` refuses in some states.** `0x141B69150` writes `-1000000` to the result and posts nothing when
   any of these holds:
   - a global product slot is missing;
   - the Storm product state (`*(*(0x141B7B5A0()+16)+8)`) is not `3`;
   - the descriptor's property at `+72` is equal to an empty value;
   - the `n` argument is `-1`.

   Otherwise it builds the event (`0x141BF7B30`) and posts it through the product's queue at `+72`, vf2.

Inferences:
- Game code creates (and probably also joins) Storm sessions only through `NetSessionController`'s `Create`.
  The statechart callers in `0x14255xxxx` suggest the online or matchmaking state machine starts it, near
  the selector-7 service code (ACU-RE-0088). That is an inference from address and naming.
- With no `Join` event and no address argument, a client most likely joins by creating with a descriptor
  that names an existing session, through the descriptor's `+72` property (an ID?). Peers would then be found
  through Storm's own peer layer (`peerManager` / `punch`, ACU-RE-0073), with addresses that come from outside
  `Create`. This is not observed.
- For our own lobby, the inputs are: a descriptor (with the `+72` property set), the Storm product in state
  3, and a source of peer addresses for the peer layer. Matchmaking (RDV / Hermes) would normally supply
  the descriptor contents and the addresses.

Unknowns:
- What the descriptor's `+72` property is, and who fills it on the client side.
- What Storm product state 3 means, and what brings it there (it may be the transport or network start;
  compare `peerManager::internal::NetworkStarted`).
- How remote peers are given to Storm (`punch::ConnectEndPoints`, peer manager or a ticket).
- Which statechart drives `0x142569B50`, and whether `0x141C46E30` is the join variant.

Independent comparison / falsification test (live, read-only, offline is enough): hook `0x141C46B10`,
`0x141C46E30`, `0x141C470A0` and `0x141B69150` while starting a co-op mission with Play in Private (which runs
solo).
- A hit with result `-1000000` would show which refusal applies offline.
- No hit would show that the solo path doesn't create a Storm session at all, which ACU-RE-0087 suggests
  (`S+0x30` stayed `-1`).

## Follow-up — the session ID, local start-up, and where remote addresses enter (2026-11-15, static)

Method: the same scans as above. Event vtables were named by their vf1 getter's RIP-relative reference to an
atlas type record.

Observations:
7. **The descriptor's `+72` property is a byte-string ID.** Its helpers:
   - `0x1426A98E0` builds an empty one (length `+8` = 0, data `+16` = null), the "empty value" in
     observation 6;
   - `0x1426A9810(id, a, b)` builds an 8-byte one from two 32-bit values, stored big-endian;
   - `0x1426AE410` compares two IDs byte by byte.

   `Create` therefore refuses a descriptor whose ID has length 0.
8. **`NetSessionController` start-up generates its own ID.** Main vtable slot 39 (`0x141C61EE0`, vtable
   `0x14300ED80 + 0x138`) does this, in order:
   1. sets `+688 = 2`;
   2. calls Storm's base slot 15 (`0x141B34FC0`, from `defaultsessioncontrollerbase.cpp`). That registers the
      controller and builds the **local** `storm::PeerDescriptor` (vtable `0x142FFD960`, 840-byte object from
      `0x141B29170`, with a fresh descriptor);
   3. on success, draws two values from the game's seeded LCG at `qword_14525FAE0 + 4` (multiplier `1664525`,
      increment `1013904223`) and stores an 8-byte ID built from them (each value shifted right by 8) at `+728`;
   4. builds a request with that ID and the string at `+768`, and calls base slot 31 (`0x141B2C540`), which
      posts `storm::echo::player::inEvent::RequestAddPlayer` (event vtable `0x143005E48`).
9. **`ConnectEndPoints` is reached only inside Storm.** The event is built in `0x142674A20`, called from
   `0x141BBF920`, which matches an address against a list of type-151 entries at `+240`. That function is
   called only by `0x141C2BD90`, slot 3 of a Storm connector vtable (`0x143008A38`, built by `0x141BA7320`).
   The session-level peer messages are also built only by Storm-range code: `SessionReachPeerMessage` in
   `0x141BD9CE0` and `0x141C26360`, `PlayerJoinAttachment` in `0x141B8C7B0` and others.
10. **The game is told about connections.** The game-side type `PeerToPeerConnectionNotification` (record
    `0x145214BF8`) is built by `NetSessionController` main slot 80 (`0x141C61040`) and by `0x141C577A0`
    (another vtable, `0x143013070`). The game receives these notifications; it does not send addresses.

Inferences:
- The local player and peer come from the game: a start-up that generates a random 8-byte ID, then
  `RequestAddPlayer`. Remote peers are connected by Storm itself (connector → `ConnectEndPoints`, session
  peer messages), from data it already holds.
- No traced game function hands Storm a remote address. So the first remote contact must enter through an
  input this pass didn't decode. The candidates are:
  - a joiner's descriptor (the ID at `+72`, or other descriptor properties);
  - a ticket (`RequestTicketSessionId` / `DistributeTicket`);
  - the RDV / ubiservices matchmaking layer writing peer data into Storm through its own path
    (`rdv::GameSessionClient`, `prudp:/`, ACU-RE-0073 observation 5).

  The third would put the backend on the address path. Under B1 it is recorded, not decoded.

Unknowns: which descriptor properties besides `+72` a joiner fills, and from where; whether Storm resolves a
session ID to the host's address on its own (only possible through a server); and the class built by
`0x141BA7320`.

Next test (live, read-only, offline): hook `0x141C61EE0` (start-up), `0x141B2C540` (`RequestAddPlayer`) and
`0x141B69150` (`Create`) while starting a co-op mission with Play in Private. That shows whether the solo
path runs the start-up and creates a session, and with which ID.

Reproduction steps:
```text
python tools/asset_query.py is not needed; use the atlas type records and the symbol index:
  type records: local/coop/net_protocol_atlas.json, names "storm::echo::session::inEvent::*"
  RIP-relative references to each record -> its getter; absolute pointers to the getter -> event vtable (vf1)
  lea references to each vtable -> builders; symbol-index callers -> posters
  ACU - Decompiled/141C3AC50.c (controller vtables), 141C46B10.c / 141C46E30.c / 141C470A0.c (vf17 calls),
  141B69150.c (Create builder and refusals)
```

Implementation implications and finding-linked constants:
- Seam: `qword_145212ED0` (`NetSessionController`); Storm interface at `+8` (vtable `0x14300EE40`): slot 17
  is `Create` (`0x141B2CFF0`), slot 27 is `ChangeSessionDescriptor` (`0x141B2C870`).
- Builders: descriptor `0x141B29AD0`, handler `0x141C42780`; the `Create` refusal code is `-1000000`.
- Game-side starts: `0x141C46B10`, `0x141C46E30`, `0x141C470A0` (S = `qword_145212F48 + 32`).
