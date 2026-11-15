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

## Live run 5 — opening the hidden `LanSessionsPage` (2026-11-16)

Host only (PID 130944, LAN mode, free roam).

Static observations:
31. Menu pages are opened by `sub_1407EAB20(menu, page_id, flag)` on `*(qword_1451EACE8 + 48)`. It calls
    `sub_140801C10(page_id)`, which resolves the id through the page registry `qword_14521E120`
    (`sub_14088CF40`). Each registry entry's page object reports its class info through vf5. The compared
    value is `classinfo + 36`, read by `sub_1426F3D40`. The pause/main menu handler `0x1408B5C70` passes
    ids as immediates in a switch. `PushPageOperator::vf19` (`0x141486B30`) passes `operator + 248`.

Live observations:
32. Hooking the push routine and the lookup while the user opened menu pages
    (`lan-page-20261116/pagehook.jsonl` `f27620e1…`) captured these ids:
    `0x92601886`, `0xCD70751B`, `0x41E34E2E`, `0x0187908F`, `0x88F14FA9`, `0xDE9BFE2F` and `0xED260607`.
    Reading `classinfo + 36` of the page classes in the live game matched each one to a class:
    | id | class |
    |---|---|
    | `0x92601886` | `HomePage` |
    | `0xCD70751B` | `BrotherhoodPage` |
    | `0x41E34E2E` | `QuestLogPage` |
    | `0x0187908F` | `CharacterPage` |
    | `0x88F14FA9` | `SkillPage` |
    | `0xDE9BFE2F` | `OptionsPage` |
    | `0xED260607` | `MsgBoxPage` |

    So **a page id is the page class CRC**, the symbol index's `crc_own`. For `LanSessionsPage` it is
    `0xA886AF69`; for `PlayTogetherPage` it is `0x0810D9C6`.
33. The probe hooked the lookup and set a conditional RCX `REDIRECT` from `0xCD70751B` (Brotherhood) to
    `0xA886AF69`, active only for return address `0x1407EAB58`. It ran alongside the LAN getter hooks
    (`lanpage.jsonl` `a1186f3f…`). When the user opened Brotherhood, the redirect fired
    (`rcx_after` = `0xA886AF69`). The screen showed only the menu background and the currency bar, with
    no page content (user screenshot `screens/lanpage-user.webp` `1c75dda6…`). The LAN getters got 0
    hits. The redirect and hooks were removed afterwards, and no game memory was written.

Inference: `LanSessionsPage` is registered and opens, but it is an empty shell. It neither draws content
nor starts LAN discovery, which matches ACU-RE-0087's finding that it has no code of its own. It is not
an in-game way to find a LAN lobby. Storm's LAN discovery has not run in any test, so a join needs it
called directly, or the invite path driven.

## Static follow-up and live run 6 — Storm's built-in LAN discovery (2026-11-16)

Static observations (corpus files; vtables read from the hash-bound mapped image):
34. Storm interface slot 12 (`0x141C626C0`) is a periodic update. When interface `+688 == 2`, byte `+692` is set,
    and `+696` (interval) + `+704` (last time) has passed, it calls `0x141B35E10` (slot 16). That function builds a
    `storm::echo::lan` DiscoverSession event (constructor `0x141B6E5E0`). The controller constructor `0x141C3AC50`
    initialises the controller fields `+700` = 0 (interface `+692`) and `+704` = 5,000,000 (interface `+696`).
    The time units are not established.
35. Writers of the flag (controller `+700`):
    - `0x141C41360`, a plain setter;
    - a `StatechartState` pair: `0x141C52BA0` (slot 8, sets 1 and records a start time) and `0x141C52CF0`
      (slot 9, sets 0). Absolute pointers at `0x14300E900` and `0x14300E908`.
36. The setter's only caller is `0x1425406D0`, which is **slot 4 of the LAN matchmaker**: vtable `0x14360D7E0`, the
    136-byte object that `0x142540630` installs at `qword_145137930 + 1792` in mode 1 (observation 13). The
    online matchmaker (vtable `0x14360D590`) has `0x1425407B0` in the same slot. Slot 4 behaves like "start search":
    in LAN mode it turns discovery on and stamps `+112`/`+128`.
37. Startup (interface slot 15, `0x141C61EE0`) calls `0x141B34FC0`. That builds the SetDiscoveryHandler event
    (`0x141BBBCB0`) in `defaultsessioncontrollerbase.cpp`, so a discovery handler is installed at startup.
38. Correction to the earlier "no LAN code ran" readings (ACU-RE-0087, observations 9 and 23). Those hooks sat on
    the event-type *getters*. Storm code also tests the cached type pointers inline (for example `0x141B8A680`
    checks `off_1452059A0` itself), so a getter hook can miss real use. Zero getter hits do not prove that
    no LAN event was built.

Live (host PID 130944, LAN mode, free roam; read-only):
39. Before the test, the host's interface showed `+688` = 2, `+692` = 0 (discovery off), `+696` = 5,000,000 and
    `+704` = 0.
40. Hooks on `0x1425406D0` (LAN start search), `0x141B35E10` (post DiscoverSession), `0x141C52BA0` (discovery state
    enter) and `0x141B6E5E0` (DiscoverSession constructor) were armed with matching entry bytes
    (`lan-search-20261116/lansearch.jsonl` `f0602b11…`). The user then chose **Play in Public** on a co-op mission.
    The game showed "Connection failed … Ubisoft online service … [0x70000106]" (`public-error.png` `7b76fb80…`),
    and all four hooks had 0 hits.

Inference: `0x70000106` decodes as category 7 (UbiServices, ACU-RE-0088) with code `0x106`. Public checks the
Ubisoft services before it ever reaches the LAN matchmaker's start search, so LAN discovery stays off. Where that
check sits is not found yet.

## Live run 7 — the check in front of Public (2026-11-16)

Host only (PID 130944, LAN mode, free roam). These are the same four hooks as ACU-RE-0088 live run 2:
- formatter `0x141C551C0` and job finish `0x1425712A0`, each with 256 bytes at RSP;
- job done `0x142737E50`, with 8 bytes at RDX (the result record);
- job lookup `0x14250C8A0`, with 128 bytes at RCX.

Log: `lan-public-err-20261116/errchain.jsonl`. The user pressed Play in Public and got `0x70000106` again.

Observations:
41. The result records stored by `0x142737E50` were one `{8 OnlineInternal, 0}`, then twice `{7 UbiServices, 0x106}`.
    The job seen by `0x14250C8A0` holds state 2 (done), selector **7** at `+8` and code `0x106` at `+12`. That is
    the same online service selector 7 that fails to connect in ACU-RE-0088.
42. Two failing paths (frames from the RSP snapshots), each ending in `0x142514CD0` → `0x142514D60` → `0x1425712A0`:
    - `0x142503F30`;
    - `0x1424F0E10`.

    Both are completion callbacks of an inner asynchronous request. Each reads that request's result record
    (`sub_141D6E590`) and tests it for success (`sub_14275DF60`). On failure they pass the record up unchanged.
    The popup then comes from `0x1425045A0` → `0x14275AA20` → the formatter.

Inference: Public issues a real request through the Ubisoft service layer, selector 7. That service never
connects with Ubisoft's servers gone (and has no backend at all under the custom loader). Its failure, `0x106`,
ends the Public flow before the LAN matchmaker's start search. The gate is a backend boundary, not a local flag.
Per the backend rule (B1) the request itself is only recorded, not decoded. No data-level bypass is apparent.
The remaining route is calling the LAN matchmaker's slot 4 directly (LAN join plan, step 2).

## Live run 8 — starting LAN discovery with the probe call feature (2026-11-16)

Tooling: the probe gained a one-shot `CALLAT` (commit `ff09afb` on `acu-multi-launch-direct`; DLL SHA-256 prefix
`a06d245b`). At the next hit of an armed execute hook it calls a function on the trapping thread and then
restores the trap context. The fixture regression passed.

Run: a fresh LAN-mode host (PID 131964); the flag was written at 1.391 s (`lan-call-20261116/lanflag.json`).
`lancall.py` (scratchpad) did the following:
- checked that `*(qword_145137930 + 1792)` held the LAN matchmaker vtable `0x14360D7E0`;
- hooked Storm interface slot 12 (`0x141C626C0`), the DiscoverSession poster `0x141B35E10`, its constructor
  `0x141B6E5E0`, and `0x1425406D0`;
- issued `CALLAT 0 1425406d0 <LAN matchmaker> 0 0 0`.

Log: `lancall.jsonl`.

Observations:
43. The call ran once, on thread 131304, at site `0x141C626C0`, and returned. The game kept running. Within
    0.1 s the interface showed `+692` = 1 (it read 0 just before) and `+704` = a fresh timestamp.
44. From then on, Storm slot 12 posted DiscoverSession on its own: `0x141B35E10`, called from `0x141C62720`,
    followed by the constructor. That was **59 posts in 30 s**, about two per second, on several worker threads.
    `+704` advanced by about 10,000,000 per second, so the `+696` interval of 5,000,000 is about 0.5 s. That fits
    100 ns time units, which is an inference.
45. The UDP sockets were unchanged: 3074 on all addresses, 6250 and 6300 on the LAN address. No other game was
    running, so nothing could answer.

Inference: calling the LAN matchmaker's start search turns on Storm's own periodic LAN discovery, with no
Ubisoft service involved. The open questions are what the discovery handler (installed at startup by
`0x141B34FC0`) does when a host answers, and whether that leads into a join. Answering them needs a second LAN
game that can open sockets (moved ports, plan step 4) or a second machine (step 5).

## Live run 9 — a built-in port offset; two LAN games with Storm on one PC (2026-11-16)

Static observations:
46. The live Storm port config sits at `*(qword_1451C5F78) + 1536`; `qword_1451C5F78` is set by constructor
    `0x141C39890`. The host's live read gave 3074 / 6250 / 6300 at `+8` / `+412` / `+416`, the defaults from
    `0x141B46C90`. Before start-up, `0x141C72980` adjusts that config. It **adds the byte `byte_145218C33` to the
    main port (`+8`)**, and in non-online mode it also adds it to `+416`. `+412` (6250) is never offset. The byte's
    other readers are two developer login paths (`0x1424D0830`, `0x1424E5C50`, near a `@ubisoft.com` string). This
    reads like a developer instance index. No RIP-relative write to it was found.

Live observations:
47. Writing the byte once at unpack did not hold: the second game later read 0, and its ports stayed 3074 etc.
    (`lan-offset-20261116/lanoffset.json`). `lanoffset2.py` (scratchpad) instead rewrote it every ~1 ms. Startup
    reset it twice (at 2.1 s and 3.4 s). The config object appeared at 4.6 s with 3074/6250/6300, and at 9.5 s the
    config read **3084 / 6250 / 6310** (`lanoffset2.json`).
48. With that, the second game (PID 114608) bound `0.0.0.0:3084`, LAN `:6250` (shared with the host, which also
    holds LAN `:6250`) and LAN `:6310`. Storm `+688` reached 2, and the game and party sessions came up with local
    peers 1 and 2. This makes **two LAN games running Storm side by side on one PC**, with no code changes.
49. The host (PID 131964) still had discovery on from run 8. Over 20 s, hooks in both games
    (`host-lanev.jsonl`, `join-lanev.jsonl`) showed the following. The host built the BroadcastLanDiscovery2 event
    40 times (constructor `0x141B95F20`, from the LAN dispatcher at `0x141B8A8BB`), so each DiscoverSession
    becomes a broadcast. The second game hit none of the three dispatcher branches (`0x141BA2950`,
    `0x141BA2E40`, `0x141BA33E0`) and built no broadcast.

Inferences:
- Port `+412` (6250) stays the same on purpose, so that LAN instances share it. It is probably the discovery
  port. The second game binds it beside the host, which implies address reuse; that is not checked.
- The second game is in plain free roam and not advertising anything, so it may simply ignore discovery.
  Untested. The next test is to make it a host (Private on a co-op mission, `PRIVATE_MATCH_HOST`) while game 1
  searches.

## Live run 10 — discovery request and reply between two LAN games (2026-11-16)

Setup: game 1 (PID 131964) is discovering (run 8). Game 2 (PID 114608) uses port offset 10 (run 9). To tell the
windows apart, the user saw their titles renamed through `SetWindowTextW`; only the title text changed. Logs are
in `lan-host2-20261116/`.

Observations:
50. The user chose Play in Private in game 2. Mission create was hit once (return `0x142755AFB`), together with
    one hit of LAN dispatcher branch `0x141BA2950` (from `0x141B8A70E`) in game 2. Game 2 then read
    `PRIVATE_MATCH_HOST`, with a mission session (local peer 3, state `+196` = 3). Nothing visible happened in either
    game. Game 1's LAN dispatcher branches had 0 hits while it built 415 broadcasts (`host.jsonl`, `join.jsonl`).
51. Socket calls. The probe hooked `WS2_32.dll` exports with `HOOKVA` (expected entry bytes checked live; logs
    `host-sock.jsonl` and `join-sock.jsonl`). Both games use only `sendto` and `recvfrom`, called from Storm at
    return sites `0x1426D7A70` and `0x1426D7777`, and never `WSASendTo` or `WSARecvFrom`. Game 1 sends on one socket
    and polls two. Game 2 sends and polls on one socket.
52. Results, from hooks on the two return sites that read RAX (`host-ret.jsonl`, `join-ret.jsonl`, 20 s):
    - Game 1 sent 39 packets of **73 bytes** (about 2/s). Each cycle it received 2 packets: a 73-byte one
      (its own broadcast, on the sending thread) and a **103-byte** one.
    - Game 2 received 73-byte packets and, each time, on the same thread, at once sent a **103-byte** packet
      (39 receives against 35 sends over the run).

Inference: Storm's LAN discovery works between two ACU games on one PC. Game 2 answers each 73-byte
DiscoverSession broadcast with a 103-byte reply, and game 1 receives that reply. Payloads were not captured or
decoded. Game 2's reply is triggered by its receive, not by its UI state. Whether it also answered before it
hosted is not known, because socket hooks were first armed after hosting. The reply reaches game 1 but leads to
nothing visible. Nothing in the UI consumes the discovered session, since the search was started by the probe
and not by a menu flow. Next: trace what game 1 does with the 103-byte reply (the discovery handler, and the LAN
matchmaker's other slots 6, 7 and 12), and what would turn a discovered session into a join.

## Live run 11 and static follow-up — where the discovery reply goes (2026-11-16)

Live (game 1, PID 131964): a hook on the `recvfrom` return site `0x1426D7777` kept 256 stack bytes for 6 s
(`lan-reply-20261116/recvstack.jsonl`). It caught 12 receives of 103 bytes, all on one stack:
`0x141BACC70` (return `0x141BACD8C`) ← `0x141B71B50` ← `0x141BB5B50` ← `0x1426BBF70` (a thread procedure).

Static observations:
53. `0x141BACC70` is the LAN socket reader. It reads up to `0x2000` bytes per packet, deserialises the packet
    (`0x141BFCBE0`), and passes it with the sender address to `0x141B7E5B0`, the LAN packet handler.
54. `0x141B7E5B0` branches on the packet's first byte:
    - **Request side:** reads 4 bytes, decodes an event, and for a discovery request checks for a matching
      session and answers through `0x141BBA1B0`. This fits game 2's receive-then-send in run 10.
    - **Response side:** decodes an event, builds a 672-byte `storm::echo::lan::inEvent::Event` (constructor
      `0x141B423D0`, vtable `0x143005548`, filled by `0x141B81040` with the inner event at `+48` and the sender at
      `+56`), and posts it.
55. The controller base handler `0x141B31020` recognises that type through `0x141B28170` and calls interface slot
    48 (vtable `+384`; thunk `0x141C588C0` → `0x141B32630`). In LAN mode (`+676 == 0`), `0x141B32630` creates an
    840-byte session record (`0x141B29170`) from the inner event and the sender. It inserts the record into the
    controller's session map at interface `+152`, keyed by the inner event's `+72`.
56. The same map is read by interface slots 38 (find by session id at `record + 12`), 40 (find by a key at
    `record + 640`), 41 (remove by id) and 42 (clear).
57. Storm's join operations sit next to Leave/Secede/Kick/Dissolve (slots 23–26):
    - slot 19 (`0x141B2C2A0`) builds `storm::echo::player::inEvent::AcceptTicket`;
    - slot 21 (`0x141B2C220`) builds `storm::echo::player::inEvent::AcceptFollowHost`, which carries a
      `storm::PeerDescriptor`.

    Slots 20 and 22 have the same shapes. Event names were read from the strings after each vtable.

Live read:
58. Game 1's map at interface `+152` holds **one record**: tree header with all three links to one node, count 1.
    Game 2's map is empty (its header points to itself).

Inference: game 1 has discovered game 2's session and stored it in its Storm session list. Nothing joins it
because no game flow asked for a search. The next step is to find ACU's own caller of slot 21
(AcceptFollowHost) or slot 19 (AcceptTicket), likely the matchmaking join states (`PRIVATE_MATCH_JOIN`,
`JOIN_PARTY_INVITE`). Then drive that caller with `CALLAT`, using the discovered record, rather than building
Storm's arguments by hand.

## Static follow-up — ACU's join path (2026-11-16)

Method: RIP-relative references to `qword_145212ED0` in the hash-bound mapped image, then indirect calls through
interface slots 17–22 (displacements `0x88`–`0xB0`) within 260 bytes of each reference.

Observations:
59. Callers found per slot:
    - slot 17 (Create): `0x141C46B10`, `0x141C46E30`, `0x141C470A0`, `0x1425406D0` (the first two are known from
      run 1);
    - slot 19 (AcceptTicket): **`0x141C40A60`**;
    - slot 20: `0x141C6E810` and `0x141C6E860`;
    - slot 22: `0x141C450C0` and `0x141C59D60`;
    - slot 21 (AcceptFollowHost): none found by this pattern.
60. `0x141C40A60(session)` is ACU's join request:
    1. it copies the descriptor at `session + 56` into `session + 8` (`sub_1426ADF40`, the same copy that mission
       create uses for its session descriptor);
    2. it builds a request from it (`0x141B47830`, `0x141B2F960`);
    3. it calls slot 19 with a handler named `"AcceptTicketRequest"` (`sub_141C42780(qword_1451C5F78 + 1256, ...)`)
       and the request's id.
61. Its only caller is `0x141C52C90`, slot 8 (on-enter) of a `StatechartState` (vtable pointer at `0x14300E780`). It
    calls `0x141C40A60(qword_145212F48 + 32)`, which is the **game (free-roam) session**, then fires a statechart
    event (`dword_145212EF8`). The LAN discovery on/off pair (`0x141C52BA0` and `0x141C52CF0`, observation 35)
    belongs to a sibling `StatechartState` (pointers at `0x14300E900` and `0x14300E908`). The states are
    created by small factories (`0x141C35480` → constructor `0x141C38F50`, and `0x141C354F0`), chosen by an index
    at `+96`. So the state machine is built from data. Its graph (state names and transitions) is not read yet.

Inference: the free-roam game session's state machine has its own LAN discovery state and an "accept ticket"
join state. Joining means putting the target session's descriptor at `game session + 56` and entering that
state. Directly, that is `0x141C40A60(game session)`, which `CALLAT` can do. Open before trying it:
- the layout of the session descriptor (it is copied with `sub_1426ADF40` and may hold pointers);
- how a discovered record (observation 55) maps to that descriptor;
- what normally writes `+56` and drives the state machine into the join state.

## Live run 12 — first join attempt through `AcceptTicket` (2026-11-16)

Setup: game 1 (PID 131964) is the joiner; game 2 (PID 114608) is the host in `PRIVATE_MATCH_HOST`. Script:
`lanjoin.py` (scratchpad). Log: `lan-join-20261116/join.jsonl`.

Static observations:
62. A session descriptor is a plain byte buffer, copied by `sub_1426ADA30`: capacity `+0`, length `+8`, data
    pointer `+16`. Live, every session's descriptor at `session + 8` is the **8-byte session id**:
    - game 1: game `00000cc3a4f52d47`, party `00000cc3a4fc7419`;
    - game 2: game `00000cc4e4398d9e`, party `00000cc4e4456c4a`, mission `00000cc5bad93d2d`.

    `+56` was empty (`{0,0,0}`) in all of them.
63. The request builder `0x141B2F960` looks the descriptor up in a second controller map at interface `+280`
    (a ticket store) and fails if there is no entry. Slot 19 removes an entry from that map. No producer of
    `+280` entries was identified in this pass.

Live, in order:
64. Game 1's controller session map (`+152`) was empty when re-read (header pointing to itself, count 0), although
    it held one record during run 11. Discovered records do not stay.
65. The script did the following:
    1. allocated 8 bytes in game 1 holding game 2's game-session id;
    2. pointed game 1's `game session + 56` at it (`{8, 8, ptr}`);
    3. ran `CALLAT 0 141c40a60 <game session>` on Storm slot 12;
    4. restored `+56` to `{0,0,0}` right after the call finished.

    The join routine ran on thread 99952. It called slot 19 (return `0x141C40AF9`) with **R9 = `0xFFFFFFFF`**, the
    request id from `0x141B7CAE0`. So the ticket lookup found nothing. Over the next 20 s neither game showed new
    traffic, only discovery packets (73 and 103 bytes in game 1, 73 in game 2). No peer list changed: counts (0,0)
    in every session of both games.
66. As designed, the routine copied the target id into game 1's own `game session + 8`. It was put back
    afterwards (`00000cc4e4398d9e` → `00000cc3a4f52d47`, written in place, same length).

Inference: `AcceptTicket` joins only a session for which the joiner holds a ticket (interface `+280`). In
matchmaking, that ticket presumably comes from the matchmaker or the host. A bare session id is not enough. The
LAN routes still open:
- find who fills `+280`; the discovery reply might, if kept;
- use slot 21, `AcceptFollowHost`, which takes a `storm::PeerDescriptor` (the host's address) and so fits a
  LAN join better. It has no caller found in ACU yet.

## Static follow-up — what `AcceptFollowHost` needs (2026-11-16)

Event names were read from the string after each event vtable in the mapped image. The session interface slots
are:

| Slot | Function | Event |
|---|---|---|
| 17 | `0x141B2CFF0` | `session::inEvent::Create` |
| 19 | `0x141B2C2A0` | `player::inEvent::AcceptTicket` |
| 20 | `0x141B358A0` | `player::inEvent::RefuseTicket` |
| 21 | `0x141B2C220` | `player::inEvent::AcceptFollowHost` |
| 22 | `0x141B35820` | `player::inEvent::RefuseFollowHost` (also builds a `storm::PeerDescriptor`) |
| 27 | `0x141B2C870` | `session::inEvent::ChangeSessionDescriptor` |
| 28 | `0x141B2CCD0` | `router::inEvent::Connect` |
| 29 | `0x141B2D650` | `router::inEvent::Disconnect` |
| 31 | `0x141B2C540` | `player::inEvent::RequestAddPlayer` (builder `0x141BB33D0`, event vtable `0x143005E48`) |

Observations:
67. Slot 21 (`0x141B2C220(iface, out, handler, int a4, PeerDescriptor* a5)`) calls the handler's vf3 and takes the
    handler id at `handler[2]`. It passes `a4` into the event as its own field (`0x141B52100`, `v27 = a2`). Every
    Accept has a matching Refuse (Ticket and FollowHost). So `a4` is most likely the id of an *incoming offer* that
    is being accepted or refused, which is an inference.
68. Address layout (`0x1426A9AA0`, `0x1426D1950`, `0x1426D1770`):
    - vtable `0x14300D638`;
    - kind `151` at `+8`;
    - a `sockaddr_in` at `+16`: family `2`, port big-endian at `+18`, IPv4 at `+20`;
    - the port again at `+144`.

    `0x1426AAB40` builds the PeerDescriptor base. A `storm::PeerDescriptor` is that base with vtable `0x142FFD960`
    set afterwards, as `0x141B423D0` does, and it carries an address object at `+112`. The LAN request handler
    (`0x141B7E5B0`) builds such an address from the sender's IP and a 2-byte port read from the packet.

Inference: `AcceptTicket` and `AcceptFollowHost` are both **answers to offers that arrive from the network**: a
Ticket, or a FollowHost proposal, each with a request id. That fits run 12, where there was no ticket and no
request. A joiner cannot start either one alone. The offer most likely comes from the host side:
- `RequestAddPlayer` (slot 31), the host adding a player, which presumably sends that player a Ticket;
- or a router `Connect` (slot 28).

So calling `AcceptFollowHost` without an incoming offer was not tried. The next experiment is host-side: in
game 2, find ACU's caller of slot 31 (or 28) and call it with game 1's address. Game 2 already sees that address,
because each discovery request arrives from it.

## Run 13 — game 1 joins game 2 through `JoinSession` (2026-11-16)

Static path (mapped image, binary SHA-256 `748abdc6…8b27e40`, image base `0x140000000`):
69. `RequestAddPlayer` (slot 31) is **not** a host invite. Its only direct caller is Storm startup (slot 15,
    `0x141C61EE0`, call at `0x141C620B2`). That caller builds a random peer id and adds the **local** player
    (handler from `sub_141C42780`).
70. The image's event names (`storm::echo::session::…`) describe a ticket handshake:
    - `RequestTicketSessionId`;
    - `TicketRequest` → `AcceptTicketRequest`;
    - `TicketReceived` → `AcceptTicket`;
    - `JoinRequest` → `AcceptJoinRequest`;
    - `FollowHost`.

    ACU names its Storm requests with strings at `0x143012690`–`0x1430127E8`, among them `JoinSession`,
    `SendFollowHost` and `AcceptTicketRequest`.
71. `JoinSession` is ACU's `0x141C54B30(session, info, peer, bool)`, the only user of the `JoinSession` string
    (at `0x141C54B6C`):
    - it copies `info+72` (the session id descriptor) into `session+56`;
    - it builds player data from `qword_145219C80`;
    - it calls Storm controller interface slot 18, `Join` (`0x141C54E60` → `0x141B30900` → builder
      `0x141BB3580`, which uses `info+72`, `info+208` and the peer).

    It has three callers:
    - `0x141C350A0`, a statechart action, which joins the game session with `bool = 1`;
    - `0x141C60210` (vtable `0x1430119A8`), which joins the mission session with `0` for type 1, and calls
      `0x141C6BCB0` for type 2;
    - `0x142560C20` (vtable `0x14360D900`, slot 54), which joins party or mission with `obj+96` / `obj+504`.
72. LAN record layout (`0x141B29170`, record size 840, vtable `0x142FFE588`):
    - ints at `+12`, `+16` and `+20`;
    - a `storm::PeerDescriptor` at `+24`, copied from the discovery event `+56`, with its Address at `+24+112`;
    - a handle at `+640` to the session info (vtable `0x142FFDFE8`), whose id descriptor is at `+72`.

    The `+152` map is a tree: root at `+16`, node `[0]`/`[1]` children, key at `node+32`, record pointer at
    `node+72`. Live, game 1 had one record: game 2's **mission** session (`PRIVATE_MATCH_HOST`) at game 2's
    LAN address, port 3084. The info id was equal to game 2's mission-session id.

Live (both games as in runs 10–12; offline LAN mode; scratchpad `lanjoin2.py` and `lanrec.py`; data in
`local/native-probe/runs/lan-joinsession-20261116/`):
73. One `CALLAT 0 141c54b30 <mission_session> <record+640 value> <record+24> 0` on Storm slot 12, in game 1.
    This is the call `0x141C60210` makes for type 1.
    - It completed on the game thread.
    - Storm `Join` (slot 18 / `0x141B30900`) fired once from it (return `0x141C54C2F`).
    - About 5 s later **Join fired again on another game thread**, with the info at `iface+0x850`. That is ACU
      itself following into a second session.
74. After 40 s:

    | Field | Game 1 before | Game 1 after | Game 2 |
    |---|---|---|---|
    | game-session id | `…a4f52d47` (its own) | `…e4398d9e` | `…e4398d9e` |
    | local handle | 1 | 4 | — |
    | session clock (`+0x98`) | 3,206,723 | 2,713,460 | 2,713,442 |
    | mission handle | −1 | 3 | — |

    - Game 1's game-session id became **game 2's** id.
    - Its session clock jumped to game 2's value, within 18 units.
    - Its mission session gained handle 3, but no id at `+8`.
    - The party sessions did not change.
    - Both games now read and send steady full-size datagrams (1,080–1,464 B) as well as discovery.
75. Correction to runs 10–12 and observation 16: `session+144` is not a peer list. It points to a Storm session
    record of the same class as the LAN records (vtable `0x142FFE588`), and its `+16` equals the session handle
    at `session+48`. So the "counts" read at `+16`/`+20` were the record's constructor ints (`+20` is 1 in game 1's
    joined sessions and 0 in game 2's), not peer counts. Each record also holds maps at `+712` and `+776`, each
    of size 2 in both games after the join. These were not read before the join, so they are not yet evidence
    of membership.

76. Visual check (user, same session, after run 13): **each game shows the other player.** The user reported
    seeing them after looking at both game windows. No screenshot was kept.

77. The user's screenshot, taken before the drop:
    - each game's HUD lists **two players**, its own and the other game's;
    - game 2's view shows **two assassins side by side** on a roof;
    - game 2 shows a co-op objective (`DEFEND Danton`).

    So the players were together in game 2's co-op content, not only in free roam. Shortly after, the user saw
    the other player **disconnect**. My tools only read memory after the join call, and all hooks were removed
    43 s after it.
78. State after the drop (read only):
    - Game 1's game session has a **new id of its own** (`…a2a23b18`, handle 5), and its LAN map at `+152` is
      empty.
    - Game 1's mission session still has handle 3 and no id.
    - Game 2 is unchanged: game session `…e4398d9e`, mission `…bad93d2d`, context `PRIVATE_MATCH_HOST`.

    Why the link dropped is not known. Candidates, none tested:
    - game 1's mission session never got an id at `+8`, so the join may have been half done;
    - game 2's statechart never left `PRIVATE_MATCH_HOST`;
    - a timeout.

Inference: game 1's free-roam game session joined game 2's. The id was taken over, the clock was synced,
game traffic flowed and the players saw each other in game 2's co-op content (observations 76–77). The
link did not last (observation 78). This is the LAN route the UI never offers: discovery (slot 4 via `CALLAT`), then
`JoinSession` with the stored record. Not established:
- whether game 2's statechart or UI noticed the join;
- whether mission launch works.

## Run 14 — catching the drop: the rejoin crashed game 1 (2026-11-16)

79. After the drop (observation 78), game 1 still got game 2's 103 B discovery replies. Both games also kept
    exchanging small datagrams: 30 B both ways, and 26 B into game 2. But no record reached the `+152` map:
    - the LAN packet handler `0x141B7E5B0` ran;
    - the reply-event constructor `0x141B423D0` and the record store `0x141B32630` did not.

    The reply branch skips a session that `0x141B85600` finds in the handler's list at `+64`, matching on
    `info+72`. Game 1's mission session was still handle 3 with base `+196` = 3. So **only the free-roam game
    session had dropped; the mission-session link stayed up**, which is an inference from the filter plus the
    traffic.
80. ACU's `LeaveSession` for the mission session is `0x141C55040(mission_base)`. It runs only when
    `base+196 == 3`, calls Storm slot 23 `Leave` with the handle at `base+80` (= `session+48`), and sets `+196`
    to 0. The party variant is `0x141C550B0`, using `+188` and `+72`. The game-session form is `0x141C355F0`.

    Live, one `CALLAT` of `0x141C55040` in game 1 completed. Game 1's mission session became handle −1 with state
    0, and game 2's record was back in the `+152` map at once.
81. One second later, a second `JoinSession` (same call as run 13, with the fresh record) **ended game 1**:
    - the probe pipe closed while `CALLSTATUS` was being polled;
    - there was no Windows Application-log entry and no new crash dump;
    - the game's save index shows a save written by game 1 at the moment of the call (local time 01:40:29).

    Game 2 kept running. The launcher save folder still matches its backup hashes. The live save folder was
    copied to `local/game-backups/save3dm-after-crash-20261116/`.

    Inference, not tested: Storm was still leaving that same session when the join arrived. Slot 18 queues a
    join when controller `+1016` is set (`0x141C41370`) and otherwise starts it.

Run data: `local/native-probe/runs/lan-drop-20261116/` (scratchpad `dropwatch.py`). The drop-hook set was armed
but never reached the watch phase:
- joiner: `Leave` 23, `Create` 17, router `Disconnect` 29, `Secede` 24;
- host: `Kick` 25, `Leave` 23, router `Disconnect` 29, `Dissolve` 26;
- all with 256-byte stack snapshots.

The cause of the drop in run 13 is still unknown. Next: a fresh game 1 (LAN flag, then discovery), then one
join with the drop hooks armed from the start, and no leave and rejoin.

## Run 15 — relaunched game 1 rejoins on its own (2026-11-16)

82. Game 1 was relaunched (`capture_run.py --direct --until world --fps-cap 60 --cpu-cores 0-3`, plus `lanflag.py`).
    It came up on the original ports (3074) with its own sessions: game-session handle 1, mission −1. Game 2
    was unchanged since run 14: `PRIVATE_MATCH_HOST`, port offset 10.

    After discovery was turned on (`CALLAT` LAN matchmaker slot 4), game 1 received game 2's 103 B replies but
    stored no record. Instruction hooks in `0x141B7E5B0` locate the rejection:
    - the request-type check at `0x141B7E6A1` gives `al` = 1;
    - the reply check at `0x141B7E768` gives `al` = 1;
    - the address comparison `0x141B4BF80`, called at `0x141B7E7C1` and read at `0x141B7E808`, gives `bl` = 1,
      so the reply is discarded.

    The two compared address objects (vtable `0x143002270`, kind 151) were equal: same IPv4, port 3074.
    **Game 2's reply carried game 1's own address.** That is the address the crashed game 1 had.
83. Within minutes, with **no join call from the tools**, game 1 was in game 2's sessions:
    - game-session id = game 2's `…e4398d9e`, handle 4;
    - mission session handle 3, base `+196` = 3;
    - both session records (`session+144`) hold game 2's PeerDescriptor, port 3084, and the record ints at
      `+12`/`+16` are (4, 2) and (3, 2).

    Inference: game 2 never removed the crashed peer. A new game at the same IP and port was taken back into
    the session as that peer. The mechanism was not traced.
84. User visual check: **both games show the other player.**
85. Watch run (scratchpad `dropwatch.py --watch-only`, data in `local/native-probe/runs/lan-drop2-20261116/`):
    - session state of both games polled every 0.25 s;
    - hooks on game 1's `Secede` 24, `Leave` 23, `Create` 17 and router `Disconnect` 29;
    - hooks on game 2's `Kick` 25, `Leave` 23, router `Disconnect` 29 and `Dissolve` 26.

    During it, the co-op objective failed (the protected NPC died) and **both games reloaded**, as reported by
    the user. No session id or handle changed in either game, and none of the eight hooks fired. **The link
    survived a mission-fail reload.**

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
