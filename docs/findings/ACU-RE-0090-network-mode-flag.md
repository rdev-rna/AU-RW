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

## Static follow-up — LAN-aware callers (2026-11-15)

Corpus files under `ACU - Decompiled/`, cross-checked with E8/E9 call scans of the hash-bound mapped image.
Names come from the local symbol index (`method`/`name` tables).

Observations:
11. `0x14254BB10` returns `dword_144E52380 == 1` (an "is LAN" helper; the argument `qword_145137930` is unused).
    It has six call sites in four functions:
    - `0x1407A14B0`, `0x1407A6C20` and `0x1407BB790` (HUD code; classes not recovered) all test
      `IsLAN() || *(qword_145137930 + 152) == 3`. So the menus treat LAN as equivalent to the "connected"
      state of that online object. In `0x1407A6C20`, failing this test calls `sub_142519A40(7)` and shows the
      not-connected popup (feature id 177).
    - `0x140F5EF90` (`OnlineUniverseComponents::helper_140F5EF90`, reached from `vf24` `0x140F5F270`) calls
      mission create `0x141C46B10(qword_145212EC8, IsLAN())`.
12. Mission create has two further callers, and both pass `dword_144E52380 == 1` as the same argument:
    `0x142569B50` (a tail `jmp`; it registers a callback first) and `0x142550900`, which creates the party
    session when its selector is 0 and the mission session when it is 1. Inside `0x141C46B10` that argument is
    written into the session descriptor before the Storm call at interface `+136` (slot 17, `Create`).
    Create then sets `NetMissionManager + 196 = 1`. Live it read `3`, so the session moved two states further.
13. `0x142540630` (called from `0x142552DD0`) builds a different object at `qword_145137930 + 1792` per mode:
    136 bytes from `0x142530580` (vtable `off_14360D7E0`, registers with the game session) in LAN mode, else
    328 bytes from `0x142530E50` (vtable `off_14360D590`, registers with the party session).

Inference: LAN co-op is a supported code path: the session descriptor carries a LAN flag, and the menus
accept LAN where they would require a connection. The user later corrected observation 10: after starting
the co-op mission, **all** map missions were hidden, not only co-op ones. Whether that happened at load or
only after the mission start is not yet known. If it was after, the most likely reading is that the co-op
launch flow is waiting (in `0x140F5EF90`, the launch fires only once its flags `+106` and `+107` are set,
and their writers are not found), and the map hides other missions while a launch is pending. This is
not verified.

## Follow-up — the co-op launch gate (2026-11-15)

The user confirmed the map missions vanished only **after** starting the co-op mission, not at load.

Static observations (corpus files, vtables read from the hash-bound mapped image, class name from the
symbol index `vtable` table):
14. The object `0x140F5EF90` runs on is `qword_1451C58E8`, a **`MissionBeaconManager`** (primary vtable
    `0x142F5FE30`, constructor `0x140F03E10`, deleted in `OnlineUniverseComponents::vf7`). `vf24` of
    `OnlineUniverseComponents` (`0x140F5F270`) ticks it every update.
15. The gate in `0x140F5EF90`, with `M` = the manager:
    - **Stage 1.** When `M+112 == 1` and `M+104` is set but `M+105` and `M+106` are not, it creates the
      mission session; `M+105` stores the result.
    - **Stage 2.** When `M+112 == 1` and `M+104`, `M+106` and `M+107` are set but `M+108` is not, it sets
      `M+112 = 0`, copies the mission descriptor at `M+92`, and calls `0x140F5AC80` (the launch).
16. `M+106` and `M+107` are set by the session-listener interface at `M+16` (vtable `0x142F5FE90`): slots 0
    (`0x140F43290`) and 2 (`0x140F432E0`) write `+90` = 1 and `+91` = mission session vf22. Relative to the
    object these are `M+106` and `M+107`. Mission session vf22 is `0x141C53CC0`: true when there is no peer
    list at `+144`, or when its two counts at `+16` and `+20` are equal (an inference: all expected peers
    are present). Slot 2 also starts a further flow (`sub_1425580C0`, state 11) when its event id equals
    `dword_145203C30`.
17. The writers of `M+104`, `M+105` (outside stage 1) and `M+112 = 1` were not found. Only three
    RIP-relative references to `qword_1451C58E8` exist, so the request probably reaches the manager through
    a stored pointer.

Live read of the running LAN game (PID 128320, after the attempt; `ReadProcessMemory`, read-only;
`local/native-probe/runs/lan-mode-20261115/beacon.json`):
18. `M+104` = 0, `M+105` = 0, `M+106` = 1, `M+107` = 1, `M+112` = 0, descriptor pointer `M+92` = null. The
    flag still reads 1, and `NetMissionManager + 196` reads 3. Bytes `M+108`–`M+111` read `f4 7f 00 00`;
    the constructor does not initialise them.

Inference: the mission session reported back and its peer check passed (`M+106`, `M+107`). But the request
fields (`M+104`, `M+112`, `M+92`) were cleared afterwards. So either the launch fired and the next step
failed, or the request was cancelled. It is not established which. Note that stage 2 tests only the byte
at `M+108`, and that byte was nonzero in this read. If it is never cleared, stage 2 can never fire.
Falsification test (live, reversible): poll `M+88`–`M+127` every few ms and hook `0x140F5AC80` while the user
starts a co-op mission. Restart the game first, because mission create refuses while `NetMissionManager +
196` is nonzero.

## Live run 2 — watching the launch (2026-11-15)

Fresh LAN-mode game, launched and flagged as in the first live run: PID 106876. The flag poller wrote `1` at
1.578 s (`lan-launch-20261115/lanflag.json` `63013b8c…`, `run/capture.json` `f2537dc7…`). Before the
attempt, `beacon_watch.py` armed four execute hooks with matching entry bytes:
- `0x140F5AC80` (beacon launch);
- `0x141C46B10` (mission create);
- `0x140F43290` and `0x140F432E0` (listener slots 0 and 2).

It also polled `M+88`–`M+127`, `NetMissionManager + 196` and the flag every 2 ms. Log:
`watch.jsonl` `4b44a363…`. All hooks were removed at the end. The user started a co-op mission, and again
nothing visible happened.

Observations:
19. Before the attempt, every `MissionBeaconManager` launch field was zero, including `M+108`. So the
    nonzero bytes in the first run were not what blocked anything here.
20. Order of events:
    1. `t = 49.219 s`: mission create was hit once, with return address `0x142755AFB` and `RDX` low byte
       `1` (is-LAN). `R9` still held `0x14252AC10`, the callback that `0x142569B50` stores just before
       its tail `jmp` into mission create. So the caller was `0x142569B50`, run by the statechart
       dispatcher `0x1427556F0`.
    2. `NetMissionManager + 196` went 0 → 1 → 3.
    3. About 1 s later, listener slot 0 was hit (return address `0x141C5EEBD`), setting `M+106` = `M+107` = 1.
    4. Beacon launch and listener slot 2: 0 hits.
    5. `M+104`, `M+105` and `M+112` stayed 0 throughout.

    So the `MissionBeaconManager` launch request is not the path the map uses here. Its stage 1 never ran.
21. `0x142569B50` is entered from `0x142552BC0`, `0x142552D20` and `0x14255C3E0`. `0x142552BC0` first labels the
    matchmaking context `PRIVATE_MATCH_HOST_START_ALONE` when byte `+236` of the statechart object is
    set, and `PRIVATE_MATCH_HOST` when it is not. `0x14256F380` writes that label as a string at `+224`.
    State changes go through `0x14256A000` with ids such as `dword_14525370C` = 20078.
22. Live read after the attempt (read-only):
    - matchmaking manager `qword_145137930` (vtable `0x14360D4E0`): `+152` = 1;
    - its statechart at `+1472`: context string **`PRIVATE_MATCH_HOST`**;
    - `+236` (start alone) = 0; `+232` = 0.

    In the corpus, `+236` is only ever cleared (`0x14255A7A0`); its setter was not found.

Inference: in LAN mode, choosing the co-op mission puts the game into the **private-match host** flow. It
creates a live Storm mission session and then waits for a second player. It does not start alone,
because the start-alone flag is off. The map hides the other missions while this lobby is open. The
"nothing happens" is a host waiting with no lobby UI. That fits the co-op path being built to need a
partner here, and it is the state a second LAN game would have to join (`PRIVATE_MATCH_JOIN`). This is not
verified.

## Live run 3 — two LAN games on one PC (2026-11-15)

Setup:
- Game 1 (host, PID 106876) stayed in `PRIVATE_MATCH_HOST` from live run 2.
- Game 2 (PID 122388) was launched beside it with `capture_run.py ... --direct --until world --fps-cap 30
  --cpu-cores 4-5 --allow-user-input`. The flag was written to `1` at 1.485 s (`lan-two-20261115/lanflag2.json`
  `7a9d98c8…`, `run2/capture.json` `6a37e1c8…`).
- Before the launch, the user's two save files were copied and hash-checked to `local/game-backups/`, since
  both games share one save folder.
- Both games got the four LAN event-type getter hooks (`coop_lan_hooks` set `getters`) for 830 s:
  `host-lan.jsonl` `f30f6be3…`, `join-lan.jsonl` `45600474…`.

In game 2 the user chose the same co-op mission. **Public** did nothing visible. **Private** loaded game 2
straight into the mission, while game 1 stayed in its lobby.

Observations:
23. LAN getter hooks: 0 hits in both games over the whole run.
24. State after the join attempt (read-only; `state-after-join.jsonl` `867a2504…`).

    Game 1:
    - matchmaking context `PRIVATE_MATCH_HOST`;
    - Storm `+688` = 2;
    - game, party and mission sessions with local peers 1, 2 and 3;
    - mission peer list counts (0, 0).

    Game 2:
    - no Storm session at all: game, party and mission peers `-1`, IDs empty;
    - Storm `+688` = **1** (start-up did not complete);
    - mission state `+196` = 0;
    - no `PRIVATE_MATCH_*` context string.
25. UDP sockets (`netstat -ano -p udp`, `udp-ports.txt` `ce42bc65…`). Game 1 holds `0.0.0.0:3074`, plus
    ports 6250 and 6300 bound to the address of a **ZeroTier One** virtual adapter. Game 2 holds no UDP
    socket.

Inferences:
- On one PC only the first game gets Storm networking. The second probably cannot bind the fixed port
  3074 (or 6250/6300), so its Storm start-up stops at state 1. With Storm down, Private takes the solo
  path and loads the mission alone. That matches what online-mode Private does (ACU-RE-0089). Game 2 never
  saw game 1's session. The bind failure itself was not observed (no socket hooks), and whether the
  ports can be configured is unknown.
- Storm bound its session ports to the ZeroTier adapter, not the physical LAN address. So a real second
  machine on the same ZeroTier network, such as the user's friend, should be reachable by these sockets.
  That is a two-machine LAN test. Not tested.

Follow-up (same day):
26. The ZeroTier adapter was live even though the user had not knowingly turned ZeroTier on.
    `Get-NetAdapter` showed three ZeroTier virtual ports Up/Connected, the `ZeroTierOneService` service
    was running, and the address Storm used was preferred and manually assigned. The user does not
    want ZeroTier as the friend route. Nothing was changed on the machine.
27. The default ports are set in one Storm configuration constructor, `0x141B46C90` (corpus file
    `141B46C90.c`, lines 47–56): `WORD +8 = 3074`, `WORD +412 = 6250`, `WORD +416 = 6300`. This is the only
    file in the `141B*`/`141C*` corpus range with those constants. Inference: changing these words in the
    second game before Storm starts would let it bind its own sockets beside the first game. Untested.
    Also unknown is whether LAN discovery then still reaches between the two, since discovery may need a
    shared port.

## Live run 4 — moving the second game's ports (2026-11-16)

After the user uninstalled ZeroTier and Tailscale, both games were restarted. The host (PID 130944) has the
flag only, and now binds `0.0.0.0:3074` plus 6250 and 6300 on the Wi-Fi LAN address. In free roam it
has game and party sessions (peers 1 and 2) and no mission session. The user explains that ACU has
**multiplayer free roam**, where players join each other's worlds, separate from co-op missions, and
that co-op missions can be launched from it. This matches the game/party sessions existing before any
mission session.

The second game was launched with `lanports.py` (scratchpad). It writes the flag, then replaces the three
`mov eax, imm32` port immediates in `0x141B46C90` (VAs `0x141B46E1E`, `0x141B46E29`, `0x141B46E5D`) with
3084/6260/6310. Each write checked the original bytes first and read back the new value.

Observations:
28. Patched right at unpack (1.468 s, `lanports-join.json`): the game exited with `0xC0000005` before
    opening a window.
29. Patched 5 s after the flag write (6.765 s, `lanports-join2.json`): the game opened its window, was
    attached at 9.7 s, then lost its window during the load, and the process was gone.
    `capture_run` stopped with "game window has no client area". No Application event log entry
    named ACU.exe.
30. The flag-only second game in live run 3 loaded fine. So the code patch is the difference.

Inference: changing those code bytes kills the game. This probably comes from an integrity or anti-tamper
check, or from the unpacker re-touching those pages; the cause is not established. Moving the ports needs
a data-level change instead: the config object's `+8`/`+412`/`+416` after construction and before the
sockets are bound. That object's address and timing are not yet known.

Unknowns / coverage:
- Who sets the start-alone flag (statechart `+236`), and what the private-match host waits for before it
  launches (a joined peer, a host confirm, or both).
- Whether a second mode-1 game on another machine (same LAN or ZeroTier network) discovers and joins the
  `PRIVATE_MATCH_HOST` session; on one PC the second game gets no Storm sockets.
- How Storm picks the adapter for 6250/6300, and whether its ports can be changed.
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
