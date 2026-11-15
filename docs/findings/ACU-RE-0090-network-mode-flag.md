# ACU-RE-0090 — ACU's network-mode flag selects a LAN session path

Date / investigator / related task: 2026-11-15 / claude-session, static plus one live field read / #85, co-op plan
Phase 2 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md)); revises the conclusion of
[ACU-RE-0087](ACU-RE-0087-lan-path-static.md), follows [ACU-RE-0089](ACU-RE-0089-storm-session-create.md)
State: partially established. The flag, its three values, and the places where it switches the session
code are observed. Its meaning per value is inferred. Who sets it is not found (see coverage).

## Provenance

Binary path / SHA-256 / image base: installed `ACU.exe` SHA-256 `748abdc6…27e40`. Addresses come from the
hash-bound runtime view: mapped image SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`,
base `0x140000000`. Class names come from `local/symbol_index.sqlite`, event type names from
`local/coop/net_protocol_atlas.json`.
Corpus path and lines: `ACU - Decompiled/<VA>.c`, whole file, for each function named below. The export
identity follows ACU-RE-0031.
Tool/provider: Python scans of the mapped image:
- RIP-relative `disp32` references;
- writes `C7 05 disp32 imm32` and `89 /r` with RIP-relative addressing;
- absolute 8-byte pointers.

Also the symbol-index `call` table, and a native-probe `READ` in the user's game (ACU-RE-0089 live run,
PID 55780). No IDA or REA session.

## Result

Observations:
1. **`dword_144E52380` holds a three-valued mode.** Five functions read it (RIP-relative):

   | Function | Behaviour by value |
   |---|---|
   | `0x141C519B0` (net start, called by `0x141C51EC0`) | calls `0x141B366B0(NetSessionController + 8, …, v)` with `v = 2` for mode 0, `v = 0` for mode 1, `v = 1` for mode 2 |
   | `0x141C506B0` | mode 0: returns the fixed player name `L"Arno Victor Dorian"`; modes 1–2: looks the name up in a player list |
   | `0x1420B2700` | modes 1 and 2 build a name from `a3 + 96` (`0x141C222F0`); mode 0 returns nothing |
   | `0x141C397E0`, `0x141C3B5A0` (net constructors) | set a field (`+588 = 1` and `+132 = 0`) only in mode 0 |
   | `0x141C51EC0` | sets net root `+2468 = 1` when the mode is not 0 or 2 (`(mode & ~2) != 0`) |

   The captured image holds `2` at `0x144E52380` (a runtime capture, not the file's initial value).
2. **The mode reaches Storm as the controller's network type.** `0x141B366B0` stores its value at
   interface `+676` (refused with `-1000000` if interface `+24` or `+25` is set). In the user's online-capable
   game the field read `1` (ACU-RE-0089 live run). That matches mode 2 → `1`.
3. **Storm's `Create` branches on that field.** In `0x141B2CFF0` (interface slot 17), a non-zero `+676` takes
   the direct path through `0x141B69150`, which posts `session::inEvent::Create` (ACU-RE-0089). Zero (the
   mode-1 value) takes a second path:
   1. it gets a Storm-side descriptor object (`0x141B69550`, from the product's allocator) and copies the
      caller's descriptor into it (`0x1426B5340`);
   2. if the copy's ID at `+72` is empty, it generates one with `0x141B71A40` (LAN `GenerateSessionId`,
      observation 4) and stores it at `+48` and `+144`;
   3. if the copy has no name, it names the session after the ID (`0x1426BFB20` → `+8`);
   4. it calls the same builder `0x141B69150`.

   So in mode 1 `Create` fills an empty session ID itself instead of refusing (ACU-RE-0089 observation 6).
   No address argument appears on this path either.
4. **In mode 1 the session ID comes from Storm's LAN product.** `0x141C72710(session)` generates a session's ID
   (object `+8`, ACU-RE-0089) just before the create:
   - mode 1: it calls `0x141B71A40`, which runs a synchronous Storm function whose type vtable
     `0x143005670` has the getter `0x141B76450` (`storm::echo::lan::GenerateSessionId`, ACU-RE-0087) at vf1,
     and copies the result;
   - other modes: it steps the seeded LCG at `qword_14525FAE0 + 4` and builds an 8-byte ID (`0x1426A9900`).
5. **The three Storm session objects.** All are instances of one session class (base vtable `0x14300EA50`;
   ID at `+8`, local peer at `+48`, vf4 returns the ID). Each registers with the controller (`0x141C688D0`):
   - game session at `NetGameManager + 0x20` (`qword_145212F48`, vtable `0x14300F140`): the S sampled live;
   - mission session at `NetMissionManager + 32` (`qword_145212EC8`, constructor `0x141C3A5F0`), created by
     `0x141C46B10` with the name `"default"`;
   - party session at `NetPartyManager + 24` (`qword_145212AA0`, constructor `0x141C3AB20`), created by
     `0x141C46E30` with the name `"Party_"` + ID.

   The job step `0x142550900` picks party (type 0) or mission (type 1), creates if needed, and copies the
   session ID and handle into the job.
6. **No writer found.** No `C7 05` immediate store, no `89` register store, and no absolute pointer to
   `0x144E52380` exists in the scanned code range (`0x140000000`–`0x143000000`). A whole-corpus text search for
   `dword_144E52380 =` was still running when this was written; its result will be added here.

Inferences:
- The values most likely mean 0 = offline or local (fixed name, no player lookup), 1 = LAN (Storm LAN
  session ID, the non-direct `Create` path) and 2 = online (the retail value). The LAN getters' 0 hits in
  ACU-RE-0087 fit mode 2.
- **This revises ACU-RE-0087's conclusion.** The game does have a LAN session path inside its own session
  code, selected by one global, even though no menu reaches it. Plan option A ("use ACU's LAN mode") has a
  concrete entry: run the game with `dword_144E52380 = 1` before net start (`0x141C51EC0`).
- In mode 1, peers would most likely find each other through Storm's LAN discovery
  (`lan::DiscoverSession`, `BroadcastLanDiscovery2`, ACU-RE-0073) instead of an address from matchmaking. On
  one network that needs no server at all. Across the internet, our own lobby would still have to relay or
  bridge the discovery. This is not observed.

Unknowns / coverage:
- Who sets the flag: possibly a data initializer, startup configuration or a write the scans miss (for
  example through a computed address).
- Whether mode 1 needs anything else that is absent in retail (LAN discovery menus, `LanSessionsPage`).
- What mode 1 does to the online services (selector 7, ACU-RE-0088) and to the menus.
- Whether mode 1 is stable at all in this build.

Independent comparison / falsification test (live, reversible):
- Launch suspended (as `capture_run --direct` already does), write `1` to `0x144E52380` before resume, then
  read interface `+676` after net start (expect `0`).
- Then hook `0x141B76450` (LAN `GenerateSessionId`) and `0x141C72710`, and start a co-op mission.
- `+676` staying `1` would show the value is overwritten after load.
- Restore: none needed. The write is in process memory only.

Reproduction steps:
```text
RIP-relative readers of 0x144E52380: 0x141C397E0, 0x141C3B5A0, 0x141C506B0, 0x141C519B0, 0x1420B2700
ACU - Decompiled/141C519B0.c lines 32-47 (mode -> 0x141B366B0), 141B366B0.c (stores +676),
141B2CFF0.c (branch on +676), 141C72710.c (session ID by mode), 141B71A40.c (LAN GenerateSessionId call)
event vtable 0x143005670 vf1 = 0x141B76450 (atlas: storm::echo::lan::GenerateSessionId via ACU-RE-0087)
```

Implementation implications and finding-linked constants:
- Mode flag `0x144E52380` (retail value 2); controller network type at interface `+676` (`NetSessionController + 684`).
- Mode-1 session ID: `0x141B71A40` → LAN `GenerateSessionId`; mode 0/2 ID: LCG `qword_14525FAE0 + 4`.
- Session objects: game `qword_145212F48 + 0x20`, mission `qword_145212EC8 + 32`, party `qword_145212AA0 + 24`.
