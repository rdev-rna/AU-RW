# ACU-RE-0054 — Who sets the travel state on the map route, and what the arrival "spawner" is (static)

Date: 2026-11-14. Related: [ACU-RE-0049](ACU-RE-0049-world-travel.md) (observations 4, 11, 14–16),
[ACU-RE-0044](ACU-RE-0044-player-state-and-repeatable-start.md).
State: partially established. A second writer of travel state `+644` is found and its request
chain is traced statically; it is unverified live. The new player entity's creator is **not**
found; the earlier "spawner" at `[World+1976]` is a streaming-focus object, not an entity factory.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, base
  `0x140000000`; mapped image SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`
  (`Unity/local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`).
- Corpus (`Unity/ACU - Decompiled/`): `141D55D30.c` 14–39, `141D60A90.c` 22–76, `141D60C90.c`
  34, `1420A2E90.c` 4, `1420A2EE0.c`, `1420A2F10.c`, `14209CB00.c` 8–17, `140F98AB0.c` 17–37,
  `140F95EC0.c` 7–21, `1420BFF80.c` 6–13, `141D55E80.c` 19–88, `141D5BEC0.c`, `141D5C9C0.c`,
  `141DF83D0.c` 6–23, `141DF56E0.c` 6–10, `141DFBDF0.c` 19–68, `141DFB680.c`, `141DFB4A0.c`,
  `141DFBD00.c`, `141DFBCD0.c`, `1413EAAF0.c` 18–35, `1413E5610.c` 24, 67–73, `141C62320.c`
  13–35, `141C39EB0.c` 17–18, 51, `14209C390.c` 12–30, `140F75390.c`, `141EC1E10.c` 31–34,
  `141EBD4E0.c` 14, `141EBD490.c`, `141EBE630.c` 10, `142E646F0.c`, `141ECBEC0.c`, `141CDC410.c`
  121–128, `141CE69F0.c` 25–27, `1413CA5B0.c`, `1413EAEF0.c` 14–30, `141D5E520.c` 13–46,
  `141D68080.c` 16–41, `141DFA910.c` 8–21, `141DFB080.c` 6–15, `141D36870.c` 151, 170–176,
  `141D522C0.c` 221–236.
- Mapped-image scans: every rel32 and 8-byte pointer to the cited VAs; capstone 5.0.3 for
  instruction boundaries. Type name by CRC-32: `0x575C803B` = `SaveGameDataManager` (crc-name,
  string at `0x142FC0478`).
- No live run in this finding.

## Result

### (a) Travel state `+644` on the travel manager `[0x145219860]`

Observations:

1. Writers of `+644` in the travel-manager code besides `0x141D60C90`:
   - `0x141D55D30(mgr, &world, &dest, extra)`: `+584` = handle of the current World
     (`0x141CD0D30`), `+592` = world, `+600` = dest, `+648` = 0, **`+644 = (+584 == +592) + 1`**
     — 1 when the target differs from the current world — then `0x141D60F80(mgr, extra, 1)`,
     `0x141CEB2D0(+592)`, timer `0x14275DD60(mgr+416, dword_145219884)` (`141D55D30.c` 14–39).
     The `+ 644) = 1` search in ACU-RE-0049 missed it because the value is an expression.
   - `0x141D60A90(mgr)`: consumes a pending record at `+624` ({world, dest, third, byte +24}),
     sets `+584`, `+592`, `+600`, **`+644 = (+592 == +584) + 1`**, frees `+624`
     (`141D60A90.c` 22–76). Only reference: `0x141D30980`, a pointer in data at `0x144E7C420`.
   - `0x141D5BC10` (`+644 = 2`) and `0x141D5C890` (`+644 = 3`).
2. Each of `0x141D55D30`, `0x141D5BC10`, `0x141D5C890` has exactly one caller, a command object's
   execute method: `0x1420A2E90`, `0x1420A2EE0`, `0x1420A2F10` (slot `+8` of vtables `0x14306DE38`,
   `0x14306DE58`, `0x14306DE78`). The objects are built by `0x14209CB00`, `0x14209DCC0`,
   `0x14209DCF0`, called only from `0x140F98AB0`, `0x140F98BE0`, `0x140F98CE0`, which copy
   `[0x145219D08]+24/+40/+48` into the command and queue it with `0x1420A2620(peer, cmd, 1, 0)`.
3. `0x140F98AB0`/`BE0`/`CE0` are slots `+192`/`+200`/`+208` of vtable `0x142F664A0`.
   `0x1420BFF80(obj, peer, kind)` calls `obj->+192/+200/+208` for kind 0/1/2. It is called by
   `0x140F95EC0`, `0x140F9B380`, `0x140F9DBB0` (kinds 0, 1, 2) with `obj = *qword_1451C6160`; these
   are slots `+216/+224/+232` of vtable `0x142F663A0`, installed by `0x140F75390` on the session
   object `qword_1452217C8` (constructed in `0x14209C390`, which also allocates `qword_1451C6160`).
4. The travel-request object `qword_145219D08` (set in `0x141DF56E0`; functor `+64` =
   `0x141DF83D0`) runs `0x141DF83D0` as a job: state `+60` 1 → `0x141D55E80(mgr)`, 2 →
   `0x141D5BEC0`, 3 → `0x141D5C9C0`, then `+60 = 0`. `0x141D55E80`, when
   `0x1420B4450()` (= `0x14209B420(qword_1452217C8)`) is false, calls
   `[qword_1452217C8]->+216(…, *qword_1451C6160)` = `0x140F95EC0` (kind 0 → `0x140F98AB0` →
   `+644` writer `0x141D55D30`). When it is true and `+644 == 3` with no pending `+624`, it fills
   the pending record instead (consumed later by `0x141D60A90`).
5. State 1 is set by `0x141DFBDF0(req, &world, &a3, &a4, a5)` when `0x141C9D720(qword_1452172D8,
   worldId)` accepts the world: `+60 = 1`, `+24` = world, `+40` = a3, `+48` = a4, `+56` = a5, then
   submits the functor to `qword_1452217D0` (`141DFBDF0.c` 19–45). Otherwise it stores the
   request at `+96..+120` and calls `0x141C9F360(qword_145217280, req, 0)`. State 2 is set by
   `0x141DFB680` (handles `+24`, `+48`), state 3 by `0x141DFBA40`.
6. Callers of `0x141DFBDF0` (rel32; return address in brackets for live matching):
   `0x141DFBD00` (tail jump) ← `0x1413EAAF0` [`0x1413EAB90`], `0x141E1EFD0` [`0x141E1F012`],
   `0x141E54030` [`0x141E54349`]; `0x141DFBCD0` [`0x141DFBCEC`] ← `0x14146E330`, `0x141D01730`;
   `0x141DFB4A0` [`0x141DFB522`] ← `0x141C62320` [`0x141C6240E`]; `0x141DFBD10` [`0x141DFBD5C`].
   - `0x141C62320` is slot `+32` of vtable `0x14300F0B0`, the `+192` sub-object of
     `qword_145212F48` (`0x141C39EB0`). It stores three handles at `+1456/+1464/+1472`, a mode at
     `+376/+380`, starts a timer and calls `0x141DFB4A0` (cross-world → `0x141DFBDF0`, else
     `0x141DFB860`). `0x141DFBDF0` itself calls `0x141C57BE0(qword_145212F48)` on a busy request.
   - `0x1413EAAF0` is an event handler of `SaveGameDataManager` (`qword_1451E3498`, created by
     `0x1420F25F0(3003, 0x575C803B)`, `1413E5610.c` 24), registered on event
     `qword_145219A80 + 896` (`1413E5610.c` 67–73). It reads saved world/start/third handles
     (`0x1413E2ED0`, `0x1413E2A40`, `0x1413E2CB0`): null world → `0x141DFB8F0`; world loaded →
     `0x141D5D320` start; else → `0x141DFBD00`.
   - `0x141E54030` uses `+240` (world), `+248`, `0x141E69F80(qword_14521A210, …)` (the same
     preparation call as ACU-RE-0049 obs. 5) and, when the target is the current world with flag
     `0x1000`, `0x141C6BE00(qword_145212F48, …)` instead.
7. The listener array `qword_14521A830` (u16 count `word_14521A83A`, capacity `0x14521A838`):
   - append `0x141EC1E10(listener)` (`141EC1E10.c` 31–34); remove `0x141ECBEC0`; init in
     `0x141EBE630` (inside the `"Ai::AIEngineBase::Init"` scope); free `0x142E646F0`.
   - Registrants (rel32 to `0x141EC1E10`): `0x140DA8FB0`+0x498, `0x140F34BC0` (after
     constructing a 1328-byte object, `0x140F01E00`), `0x140F935C0` (5168 bytes,
     `0x140F78C80`), `0x141E59610` (`qword_14521A2C8`, 1880 bytes, vtable `0x14303AFB8`) and the
     `SaveGameDataManager` constructor chunk at `0x1413CA851` with `rcx = object + 16` (the
     sub-object with vtable `0x142FC1960`, slot `+16` = `0x1413EAEF0`).
   - Broadcasts: `0x141EBD4E0` (slot `+16`) and `0x141EBD490` (slot `+24`), from `0x141CDC410`
     (lines 128 and 252, right after an `"::Engine::ExecuteCommands"` pass) and `0x141E7AE90`.
     `0x141CDC410` runs only while `byte_14521738B` is set (`141CE69F0.c` 25–27) or from
     `0x141CC19A0`.
8. No code references to the `FastTravel*` strings were found: `FastTravelCommand`
   `0x142F93160`, `ActionFastTravel` `0x142F93D18`, `FastTravelPanelDataContext` `0x142EDE1D0`,
   `FastTravelManager` `0x142F3C420` are only pointed to from reflection descriptors
   (`0x143F77558`, `0x143F774A8`, `0x143C73A08`, `0x143E45E98`). `FastTravelPanelDataContext`
   (CRC `0xA9DA47CF`) is registered by `0x142D95450` with factory `0x14088F1A0`.

Inferences and alternatives:

- **Top candidate (medium):** on the map route `+644 = 1` is written by `0x141D55D30`, reached
  via request `0x141DFBDF0` (state 1) → job `0x141DF83D0` → `0x141D55E80` → session send
  `0x140F95EC0` → `0x140F98AB0` builds command `0x14306DE38` → execute `0x1420A2E90` →
  `0x141D55D30`. This explains why `0x141D60C90` and `0x141E42C00` were never hit (obs. 15): the
  map route uses a separate, command-queued setter. The session indirection looks like a
  co-op-replicated travel command.
- Alternative: `0x141D60A90` writes the same three fields when `0x1420B4450()` is true; it needs
  `+644 == 3` first, so it is less likely for a first trip.
- Which caller of `0x141DFBDF0` the map confirm uses is open; `0x141C62320` (three handles, mode,
  timer) and `0x141E54030` are the most specific candidates.
- `qword_14521A830` is a general per-tick listener list (AI-engine init scope, called after
  command execution in a blocking engine tick), not a travel event list. Inference: the hit on
  `0x1413EAEF0` at confirm (ACU-RE-0049 obs. 15) shows that tick running (`byte_14521738B` set at
  confirm), not that the broadcast carries the travel request.

### (b) What the arrival code does with the start

Observations:

9. `0x141D5D320(mgr, world, &start)` only forwards to `0x141D5E520(world, &start)`
   (`141D5D320.c` 4). `0x141D5E520` selects the start (`+312` list / `+304` default), stores it at
   `+328`, then calls `0x141D68080(world)` (`141D5E520.c` 13–46).
10. `0x141D68080` takes the start's transform (vtable `+136`) and calls
    `0x141DFA910([World+1976], &xy)`, then `0x141DFB080([World+1976], value)` where value is
    `[0x145219860]+640` unless a flag bit is set (`141D68080.c` 16–41).
11. `[World+1976]` is a 352-byte object created in the World constructor `0x141D36870` by
    `0x141DDD690` (`141D36870.c` 170–176). `0x141DFA910` walks a linked list (next at `+0`) and
    stores `(x, y, 0, 0)` at `+192` with flags `|= 0x60` at byte `+228` in every node;
    `0x141DFB080` stores a dword at `+340` in every node (`141DFA910.c` 8–21, `141DFB080.c` 6–15).
    `0x141D522C0` also stores `**(World+16)` at `[World+1976]+288` (line 228) before calling
    `0x141D5E520` (line 236).
12. The World constructor sets `World+216` (`+0xD8`, the player in ACU-RE-0044) to 0
    (`141D36870.c` 151). No other store to a World's `+216` was found: decompiled `+ 216) =`
    writes in `0x141C*`–`0x141E*`, indexed `[27] =` writes, and a byte scan for
    `mov [reg+0xD8], r64` in `0x141C00000`–`0x141E80000` (43 functions) gave only other classes
    or stack/frame stores. `0x141D02E00`/`0x141D038E0` write `+216` on an entity, not a World.

Inferences and alternatives:

- **Correction (medium-high):** `[World+1976]` is a streaming-focus list that receives the start
  position (2D) and a value from the travel manager; it is not an entity spawner. `0x141D5E520`
  points world streaming at the arrival point. This fits ACU-RE-0049: the player appears ~14–19 s
  later, after streaming.
- The player entity is created elsewhere, likely when streaming around the focus completes;
  candidates not established. `**(World+16)` (stored at focus `+288` and called through vtable
  `+144` with the World) is a lead to check live.

Unknowns / incomplete coverage:

- No live evidence for any (a) or (b) inference. Thread of the job (`qword_1452217D0`) unknown.
- The `0x145xxxxxx` thunks and reflection-driven UI code were not traced.
- World `+0xD8` writer: not found statically (may be a store through a register/struct the scans
  did not match, or code outside the scanned range).

Independent comparison / falsification test: the live plan below; each probe has a stated
expected result.

Reproduction steps: read the corpus lines cited; for references, scan the mapped image for
rel32 (`target − (site + 4)`) and 8-byte pointers to each VA.

Implementation implications: none for `acu_sim` yet beyond ACU-RE-0049's lifecycle; if confirmed,
the travel request is a queued command with states 1/2/3, and arrival first sets a streaming
focus, then later creates the player.

## Live verification plan

Use the ACU-RE-0049 route (pinned save, `focus.py activate PID`, Back → map, P pin, keyboard F,
Enter). Max 4 hardware entry hooks per session; two sessions.

Session 1 — `tools/native_probe/world_travel_setter2.profile.json`:

| Probe | VA (RVA) | First 16 bytes | Snapshot | Confirms / falsifies |
| --- | --- | --- | --- | --- |
| `travel_request` | `0x141DFBDF0` (`1dfbdf0`) | `48895c241856415641574883ec208379` | `rdx` +0, 8 bytes (world handle ptr); stack return address names the caller | Hit at Enter; caller return address picks one of obs. 6 (e.g. `0x141C6240E` via `0x141DFB522`, or `0x1413EAB90`) |
| `travel_job_state1` | `0x141D55E80` (`1d55e80`) | `40535741564883ec204c8b35783e4c03` | `rcx` +584, 72 bytes (`+584…+655`) | Hit after the request; `+644` still 0 |
| `travel_cmd_execute` | `0x1420A2E90` (`20a2e90`) | `4883ec284c8d49184c8d4110488d5108` | `rcx` +0, 32 bytes (vtable `0x14306DE38`, world, dest, third) | `[+8]` = ACU_Paris handle, `[+16]` = destination portal handle |
| `travel_state_setter` | `0x141D55D30` (`1d55d30`) | `48895c241048896c2418488974242057` | `rcx` +584, 72 bytes | **Confirms** if hit with `rcx = [0x145219860]` and `+644` reads 1 at the next sample; **falsifies** if `+644` becomes 1 without this hit |

If `travel_state_setter` is not hit, session 1b (one probe per line):

- `0x141D60A90` guard `48895c24184889742420574883ec2048`, snapshot `rcx` +584, 72 bytes.
- `0x140F95EC0` guard `4885d2745c574883ec20488b0df7b828`, snapshot `rdx` +0, 16 bytes.
- `0x140F98AB0` guard `48895c24084889742410574883ec304c`, snapshot `rdx` +0, 16 bytes.
- `0x141DF83D0` guard `40534883ec208b513c488bd9ffca7447`, snapshot `rcx` +24, 40 bytes.

Session 2 — `tools/native_probe/player_spawn.profile.json`:

| Probe | VA (RVA) | First 16 bytes | Snapshot | Confirms / falsifies |
| --- | --- | --- | --- | --- |
| `world_init` | `0x141D522C0` (`1d522c0`) | `405355565741554883ec40488bb90001` | `rcx` +16, 16 bytes (World `+16`, `+24` handles); `edx` = mode | Records the new World and `**(World+16)` lead |
| `start_focus` | `0x141D68080` (`1d68080`) | `48895c2408574883ec40488b99480100` | `rcx` +304, 32 bytes (`+304` default, `+312` list, `+322` count, `+328` start) | `+328` = arrival portal handle |
| `stream_focus_set` | `0x141DFA910` (`1dfa910`) | `f30f105204f30f101a8089e400000060` | `rdx` +0, 8 bytes (x, y) | **Confirms** the correction if (x, y) ≈ the arrival position (768.55, −46.65 for the P pin) and World `+0xD8` is still 0 then |
| `savegame_travel_event` | `0x1413EAAF0` (`13eaaf0`) | `405356574883ec30488d99801a000048` | `rcx` +6840, 24 bytes (saved handles) | Hit or not at confirm decides whether the SaveGameDataManager route is used |

With them, `WATCH` the new World's `+0xD8` (8 bytes) as soon as holder `[0x145217368]+0x98`
changes, to time the player creation against these hits. A next static step is to find the code
that runs when that watch changes (the creator), starting from `**(World+16)` vtable `+144`.

## Development handoff

- **Confidence:** static only. (a) medium for `0x141D55D30` as the map-route setter; (b)
  medium-high that `[World+1976]` is a streaming focus; the player creator is unknown.
- **Established (static):** the command chain in observations 2–5; listener-array registrants
  and per-tick broadcast (7); `0x141D5E520` → `0x141D68080` → streaming focus (9–11).
- **Not established:** any of it live; which caller starts the map request; who writes World
  `+0xD8`.
- **Interface:** none new; keep ACU-RE-0049's lifecycle states. If confirmed, add a
  `TravelRequest { world, destination, mode }` queued command between `request_travel` and
  `Transition`.
- **Follow-up:** run sessions 1 and 2; then trace the World `+0xD8` writer from the live
  return address.
