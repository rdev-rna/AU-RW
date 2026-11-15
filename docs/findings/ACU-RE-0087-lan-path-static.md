# ACU-RE-0087 — Does any ACU code path start Storm's LAN session product?

Date / investigator / related task: 2026-11-15 / claude-session, static only / #85, co-op plan Phase 2 LAN-path test
([COOP_RE_PLAN.md](../COOP_RE_PLAN.md)); follows [ACU-RE-0073](ACU-RE-0073-session-transport-seam.md) observation 7
State: partially established. No direct-call path from game code to the LAN events. The LAN menu page ships
and is registered, but adds no behavior of its own. Indirect (virtual or data) paths are not covered.
Dynamic addendum (2026-11-15, below): the four LAN type getters got **0 hits** in every live flow tested,
including two co-op mission starts. Every online entry failed with `0xb0040001` (Ubisoft services
unreachable), and no Storm session started.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: installed `ACU.exe` SHA-256 `748abdc6…27e40`. Addresses are
from the hash-bound runtime view (mapped image SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`,
base `0x140000000`) through `local/symbol_index.sqlite` (tool SHA-256 `9d6ef9d9…`, 585,043 direct call edges).
Corpus path and lines: `ACU - Decompiled/1408EDAB0.c`, `1408EDC10.c`, `140901990.c`, `1408CBB10.c`
(each whole file, 6 to 15 lines). The export identity follows ACU-RE-0031: header and resource binding only.
Archive identity: asset index `local/asset_index.sqlite` (version 2026-10-22.1). Resources in `DataPC.forge`
entry 110 and `DataPC_patch_01.forge` entry 0. File IDs are listed below.
Tool/provider: sqlite queries on the symbol index (`class`, `method`, `call`) and `tools/asset_query.py`
(`find`, `walk`, `where`, `sql`). No IDA or REA session. Link evidence is **candidate** (equal bytes), never
a decoded reference. The page type `8755212e` has confidence `unknown`.
Search boundary: the call graph covers direct calls only. A corpus-wide `rg` for the page class globals
timed out after 900 s on this disk, so it gives no result. That route is not covered.

## Result

Observations:
1. **LAN event types are requested only by Storm's own registration code.** The type getters, followed by
   their direct callers:
   - `0x141B752A0` (DiscoverSession) is called by `0x141B3BF10` and `0x141B8A4F0`.
   - `0x141B7A500` (SetDiscoveryHandler) is called by `0x141B3D4C0` and `0x141B8A4F0`.
   - `0x141B76450` (GenerateSessionId) is called by `0x141B8A600`.
   - `0x141B748A0` (BroadcastLanDiscovery2) is called by `0x141B3BB60`.

   The wrappers `0x141B3BF10`, `0x141B3D4C0` and `0x141B3BB60` each have one caller, `0x141B8A680`.
   `0x141B8A4F0`, `0x141B8A600` and `0x141B8A680` have no direct callers. These are Storm-internal
   factory/registration functions (ACU-RE-0073), so no game function calls into the LAN events directly.
2. **`LanSessionsPage` is a shipped menu page.** Reflected class `LanSessionsPage` (class info `0x143C94DD8`,
   getter `0x1408EDAB0`, factory `0x1408B0E50`, CRC `A886AF69`, size 224) derives from `AutoFeedDataPage`.
   Its siblings are `PlayTogetherPage`, `JoinAnySessionPage`, `JoinFriendSessionPage`, `FriendsPage`,
   `InvitationsPage`, `CustomizeAvatarPage` and `PopupPlayerActionsPage`. A data resource
   `LanSessionsPage` (`0000001299c151e4`, type `8755212e`, ordinal 3301) ships in both archives, as do
   `PlayTogetherPage` (`0000001d6934e79a`), `JoinAnySessionPage` (`0000001288940d16`) and
   `JoinFriendSessionPage` (`0000001288940d17`).
3. **All four are listed by the menu manager.** `MenuManagerGame_0XA9AF241CF` (`0000000a9af241cf`, type
   `3dda14ee`) holds candidate links to them at offsets 251 (JoinAny), 261 (JoinFriend), 271 (Lan) and 641
   (PlayTogether). No other resource links to any of the four.
4. **The LAN page reuses another menu's layout.** At offset 15, `LanSessionsPage` links to
   `Menu_Online_Invite_Friends_New` (`00000012733d8a82`). `PlayTogetherPage` instead links to its own
   `UI_Menu_PlayTogether_Page` (`0000002a24aab0fb`).
5. **No behavior of its own.** Of its 17 vtable slots (`0x142EE9FD0`), `LanSessionsPage` defines only these:
   - slot 0: the destructor (`0x1408CBB10`, resets the vtable and frees);
   - slots 2 and 3: property serialization (ACU-RE-0073);
   - slot 5: the getter;
   - slot 7: `0x140901990`, a thunk to the base `AutoFeedDataPage` slot 7 `0x14088FAE0`.

   `JoinAnySessionPage` uses the same thunk for slot 7. `PlayTogetherPage` overrides slot 7 with its own
   `0x140901EA0`. No getter or factory of any of these pages has a direct caller, so pages are built from data.

Inferences and alternatives:
- The game can still reach Storm's LAN product only indirectly: a virtual call through a type record handed
  out by the registration tables, or a data-driven page binder. No direct path exists. The LAN page looks
  like an unfinished or retired feature: it is registered, but has a borrowed layout and no code. It may also
  be reachable through menu data that this pass did not decode (page IDs inside `MenuManagerGame`).
- `JoinAnySessionPage` also lacks its own code but is a shipped feature, so missing code alone does not prove
  the LAN page is dead. The borrowed layout and the missing direct LAN calls are the stronger signals.

Unknowns / incomplete coverage:
- Indirect calls into the four getters (not in the direct-call graph).
- How menus navigate to a page (by ID or by index in `MenuManagerGame`), and whether any button or action
  data names `LanSessionsPage`.
- What the page's two properties hold.

Independent comparison / falsification test (dynamic, next): in a running game, hardware execute hooks on
the four LAN type getters (`0x141B752A0`, `0x141B7A500`, `0x141B76450`, `0x141B748A0`). Count hits through
startup, the main menu, the online/co-op menus and a co-op lobby attempt. Hits after registration would show
a runtime path the direct-call graph misses. No hits means the LAN product is never used in these flows.
`tools/coop_live_probe.py` already arms four execute hooks per round.

Reproduction steps:
```powershell
python tools/asset_query.py walk LanSessionsPage
python tools/asset_query.py sql "SELECT r.file_id, r.name, l.offset, l.dst_file_id, l.kind FROM link l JOIN resource r ON r.id=l.src_resource_id WHERE l.dst_file_id='0000001299c151e4'"
# symbol index: callers of the getters above in table `call`; slots in table `method` for each page class
```

## Dynamic addendum — live LAN-getter hooks and co-op entry attempts (2026-11-15)

Provenance: installed `ACU.exe` SHA-256 `748abdc6…27e40` (checked at attach in every run), loaded at its
preferred base `0x140000000`. Original Ubisoft-signed `uplay_r1_loader64.dll` active (SHA-256
`0e06fee6…3291f`, swapped in at 19:55:33 local time, `local/game-backups/loader-swap/log.jsonl`), launched through
Ubisoft Connect. Hooks: four hardware execute breakpoints per round via the native probe (`HOOK`, expected
entry bytes checked before arming), driven by `tools/coop_lan_hooks.py` (commit `1de73b3`). Session fields
read once per second from S = `[0x145212F48]+0x20` (ACU-RE-0073). The input came from the focus shim's
in-process pad. Raw data is machine-local under `local/native-probe/runs/` (not distributed):

| Run (PID) | Window (local) | Hooks | Flow | Hits |
|---|---|---|---|---|
| `lan-hooks-20261115` (119068) `getters.jsonl` `eebe2290…` | 19:56:53–20:03:02 (369 s) | getters | title, world load, pause, Play Together, **Go Online** → error | 0 |
| same, `checks.jsonl` `988935a6…` | 20:03:05–20:05:04 (120 s) | 3 LAN type checks + `LanSessionsPage` factory `0x1408B0E50` | Go Online again → error | 0 |
| same, `getters-coop.jsonl` `a225bd09…` | 20:05:18–20:12:49 (452 s) | getters | map, Selective filter, hover a co-op mission marker, **Play in Private**, mission loads | 0 |
| `coop-online-20261115` (124004) `getters.jsonl` `b568e648…` | 20:27:42–20:31:25 (223 s) | getters | map, hover a co-op marker, **Play in Public** → error | 0 |
| `coop-manual-20261115` (122532, launched by the user) `getters.jsonl` `2140ab25…` | 20:49:26–20:52:23 (178 s) | getters | co-op mission start screen, **Play in Public** → error | 0 |

Observations:
1. **No LAN code ran.** None of the four getters, the three type checks or the `LanSessionsPage` factory
   was hit in any round (1,342 s armed in total). The co-op flows covered here therefore don't use Storm's
   LAN product, at least up to the point where they fail.
2. **Every online entry fails before a session exists.** Go Online (Play Together menu) and Play in Public
   (co-op mission marker and mission screen) each showed "Connection failed … required Ubisoft services are
   currently unreachable … [0xb0040001]". Screenshots: `lan-hooks-20261115/shots/09-online-a.png`
   (`635c69c4…`), `coop-online-20261115/shots/04-public.png` (`e94d022f…`), `coop-manual-20261115/shots/02-public.png`
   (`8d14c611…`). The Ubisoft Connect overlay being on or off made no difference (user toggled it between runs).
3. **No Storm session started in any flow.** `S+0x30` (local peer) stayed `-1`, `S+0x90` (peer pointer) `0`,
   `S+0x98` (network time) `0` and `S+0x9D` (suspended) `0` throughout: `lan-hooks` 584 samples
   (20:15:53–20:25:36: in the private mission, Quit Mission, free roam), `coop-online` 216 samples, `coop-manual`
   183 samples (`session-fields.jsonl` in each run).
4. **Play in Private with one player runs the mission solo** (`shots/22-private.png` `b2544f9a…`, loaded at 20:12:21).
   In that mission, a probe round (`local/coop/live_probe_private_mission_20261115.json` `9e809227…`, 20 s per round)
   saw these hits:

   | Hook | Hits | Per second |
   |---|---|---|
   | `0x1C55BB0` (memento push entry) | 1,208 | 60.35 |
   | `0x1C66360` (replicate-object path) | 61 | 3.05 |
   | `0x1C46880` / `0x1C4CB50` (batch close / record flush) | 481 each | 23.92 |
   | gated memento push `0x1C55BEC`, memento write `0x1C77FE0`, start intention `0x5D17F0`, intention builder `0x71F090` | 0 | 0 |

   The intention counter at `0x145212F3C` rose from 1,033 to 1,114 over 41.1 s. The labels come from earlier
   co-op notes and are not re-derived here. Without a session, the replication bookkeeping still runs every
   frame, but the gated network path does not.

Context, not evidence: the user reports that ACU matchmaking broke on every platform after Ubisoft's 2026
console 60 fps update (community report, not verified here). A direct join with a friend is the one route the
user still expects to work. It has not been tested.

Inferences:
- The co-op entries tested here go to the Ubisoft backend first and stop at its error. They never reach Storm
  session creation, so these flows can't show whether Storm would run without that backend. The LAN product's
  0 hits fit observation 1 of the static pass: nothing in these flows requests it.
- A working friend join is the next dynamic test. It would be the first live Storm session (expect `S+0x30 ≥ 0`,
  `S+0x90 ≠ 0`). Hooks for that test should be read-only (session fields plus execute counters) with no input
  blocking on the user's game.

Unknowns: whether any menu or data path opens `LanSessionsPage` at all (still static-only); what the backend
step hands Storm when it succeeds (peer addresses, session ID), which is what our own lobby would have to supply.

Implementation implications and finding-linked constants:
- Plan option A ("use ACU's LAN mode") has no supported entry point so far. Supplying peer addresses ourselves
  (our own lobby, posting Storm session events with known addresses) stays the likely seam (ACU-RE-0073).
- Constants: the LAN getter VAs above, `LanSessionsPage` class info `0x143C94DD8`, resource `0000001299c151e4`.
