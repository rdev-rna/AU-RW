# ACU-RE-0090 — ACU's network-mode flag selects a LAN session path

Date / investigator / related task: 2026-11-15 / claude-session, static plus a live LAN-mode run / #85, co-op plan
Phase 2 ([COOP_RE_PLAN.md](../COOP_RE_PLAN.md)); revises the conclusion of
[ACU-RE-0087](ACU-RE-0087-lan-path-static.md), follows [ACU-RE-0089](ACU-RE-0089-storm-session-create.md)
State: partially established.
- Static: the flag, its three values, and where it switches the session code. No code writes it.
- Live: writing `1` after unpacking holds. It switches the controller to network type 0, starts Storm game
  and party sessions at boot, and creates a mission session when a co-op mission is started. The map then
  hides the co-op mission markers. Two-player play is not tested.

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
   `0x144E52380` exists in the scanned code range (`0x140000000`–`0x143000000`). A whole-corpus byte search
   for `dword_144E52380 =` (217,041 files, 432 s) matched 13 files, and **every match is a comparison**
   (`==`), none an assignment. So the value comes only from the image's data and nothing changes it at
   runtime. The search also found eight readers that the RIP scan missed, because they compare against an
   immediate: `0x141C56CC0`, `0x141C662B0`, `0x141C72980`, `0x1424E58D0`, `0x142540630`, `0x14254BB10`,
   `0x1425568D0` and `0x142576CC0`.

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

## Live run — LAN mode (2026-11-15)

Provenance: installed `ACU.exe` SHA-256 `748abdc6…27e40` (checked at attach), base `0x140000000`. Custom uplay
loader active (`tools/loader_swap.py use custom`; the original is restored afterwards).
- Launched directly: `capture_run.py local/native-probe/runs/lan-mode-20261115/run --direct --until world
  --fps-cap 60 --allow-user-input`, on the user's own save (PID 128320, `run/capture.json` `6173488d…`). The
  focus shim was installed with input blocking off.
- Flag write: a poller (`lanflag.json` `123d7a50…`) opened the new process and read `0x144E52380` every 1 ms.
  It read `0` until 1.359 s after spawn, then `2` (unpacked). At that moment it wrote `1`
  (`WriteProcessMemory`, ok), and the value stayed `1` for the rest of the 90 s watch. Process memory only;
  no file changed.
- Hooks after reaching the world: `0x141C72710`, `0x141B2CFF0`, `0x141B76450`, `0x141C46B10` (expected entry
  bytes matched live), log `hooks.jsonl` `db51e3af…`.

Observations:
7. **The write holds, and the controller switches to LAN.** In the world, probe reads showed:
   - flag `1`;
   - controller interface `+676` (network type) `0`, against `1` in online games (ACU-RE-0089);
   - interface `+688` `2` (start-up ran; it was `1` in the online game);
   - the controller ID set to 8 bytes (`00f1c4de00145714`).
8. **Storm sessions start at boot in LAN mode.** Before any mission:

   | Session | ID | Local peer (`+48`) | Peer pointer `+0x90` | Network time `+0x98` |
   |---|---|---|---|---|
   | game (S) | `00000cb367b6c6f9` | `1` | non-null | running (65,244 ms) |
   | party | `00000cb367bc01e4` | `2` | non-null | running |
   | mission | empty | `-1` | — | — |

   In every online-mode run these were all `-1` and empty (ACU-RE-0087, ACU-RE-0089).
9. **Starting a co-op mission creates a Storm mission session.** When the user started a co-op mission
   (22:41:04 local time), the hooks hit in this order, once each except the LAN getter:
   1. `0x141C46B10` (mission create), called from `0x1427556F0`;
   2. `0x141C72710` (session ID);
   3. `0x141B76450` (LAN `GenerateSessionId` type) twice, from Storm's own registration functions `0x141B8D470`
      and `0x141B8A600`;
   4. `0x141B2CFF0` (Storm `Create`).

   Afterwards the mission session had ID `00000cb3adf8a708`, local peer `3`, a peer pointer, and a network
   time that had started. `NetMissionManager + 196` was `3`.
10. **The UI does not follow.** The user reported that nothing visible happened, and that the co-op mission
    markers disappeared from the map in this mode.

Inferences:
- Mode 1 is a working LAN mode for Storm: sessions are created with LAN-generated IDs and no backend. The
  UI hides co-op missions in this mode, probably through one of the readers listed in observation 6, so
  the game was not built to offer them in LAN.
- To play co-op over LAN, the next pieces are (a) the menu or map filter that hides co-op markers in mode 1,
  and (b) a second game in mode 1 discovering or joining this session.

Unknowns / coverage:
- Which reader hides the co-op markers, and what the mission-start path does in mode 1 after `Create`.
- Whether a second mode-1 game finds the session (LAN discovery), and what a joiner needs.
- What mode 1 does to the online services (selector 7, ACU-RE-0088). The custom loader has no online.
- Stability over a longer session.

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
