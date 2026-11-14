# ACU-RE-0049 — World change and world travel (partial; first live attempt crashed)

Date: 2026-11-14. Related: [ACU-RE-0044](ACU-RE-0044-player-state-and-repeatable-start.md),
[ACU-RE-0046](ACU-RE-0046-player-teleport-and-vault-scan.md).
State: partially established. Fast travel to Paris through the in-game map works and is traced
(observation 14). The Paris World also loads on request through the game's own
load-world command (observation 10); the player is not yet brought into it. Calling the travel
manager's last step alone crashed the game (observation 7).

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, base
  `0x140000000`; mapped image SHA-256 `9a1124f1…0520` for table reads.
- Corpus: `141CCA250.c` (lines 142, 299–330, 605–610), `141CEB520.c`, `141CE69F0.c` (lines
  ~90–95), `141CCB460.c`, `141CE1840.c`, `141CE1820.c`, `141CCF930.c`, `141D60C90.c`,
  `141D57AD0.c`, `141E42C00.c`, `141C5D3C0.c` (line 169), `140F94AD0.c`, `140F16B30.c`.
- Asset index (crc-name): World resources, type `fbb63e47`.
- Live: run `travel-1-launch` (pid 120456, pinned save `capture-start-1`); crash dump
  `ACU.exe.120456.dmp` (Windows CrashDumps, 118,321,273 bytes; not copied into the project).

## Observations

1. World resources (`fbb63e47`): `ACU_Paris` `0000000f18b7e1cf`, `ACU_Versailles`
   `0000000e84b48064`, `ACU_Prologue` `00000010a04d134d`, `ACU_RebirthRitual`
   `0000000fc9a64e04`, `ACU-DLC_Saint-Denis` `000000135dce49f2`, `ACU_LGS_BelleEpoque`
   `000000102ad65759`, `ACU_LGS_Medieval` `000000102ad678f3`, `ACU_LGS_WorldWar2`
   `000000102b0edb35`, `TitleScreenFPP` `0000000829bb2de8`. Also `Universe_WorldPortalManager`
   (`03091e55`) and `FastTravelManager` (`125cff33`) in `DataPC.forge`. `0x140F16B30` only checks
   that the `_ACU_Paris` and `_ACU_Versailles` data packs are present.
2. World holder `[0x145217368]`: ring-buffer command queue (56-byte entries at `+48`, count
   `+56`, write index `+60`, read index `+64`, capacity `+68`, pending `+80`/`+84`). Per-frame
   `0x141CE69F0` drains it via `0x141CCB460` with a functor (vtable `0x14301A660`, slot 2
   `0x141CEB520`) that calls `0x141CCA250(command)`.
3. Command 5 = set world: `0x141CE1840(holder, &worldHandle, flag, delay)` writes `{5, handle +8,
   flag byte +48}`; `0x141CCA250` case 5 swaps the refcounted world handle at holder `+0x90`
   and stores the resolved World at `+0x98` (the World of ACU-RE-0044). `0x141CE1820(&h, flag)`
   is the wrapper; command 9 (`0x141CCF7D0`) can chain into 5.
4. Travel manager `[0x145219860]`: `0x141D60C90(mgr, &worldHandle, &destHandle)` stores target world
   `+592`, destination `+600`, state `+644 = 1`, requests the transition world
   `0x141CE1820(&off_1450A9600, 1)`, then `0x141CD8A00(world id, 1, 0, 6, destination id, 1)`.
   Handle layout as used: `+0` object, `+8`/`+12` counts, `+16` file ID.
5. Its only caller is `0x141E42C00(request)` (world handle `+72`, destination `+80`, plus `+16`,
   `+24`/`+34` list, `+60`, `+88`, `+96`, `+104`). Before the travel call it prepares the game:
   `0x141E5D5A0`, `0x141E5F130`, `[0x145219C88]->vtbl+64`, `0x141E69F80(…)`,
   `0x141D901E0([0x145219A80], 1, 4, 0)`, guard byte `0x14521A2D0`. Its only caller,
   `0x141C5D3C0`, reassembles a multi-part message and deserializes the request object — the
   travel request is a message object, not static data.
6. Live, the heap holds 11 handles with file ID `ACU_Paris` and 143 places that reference one, among
   them 31 records with the Paris world handle at `+72` and a `PlayerPortal` (type `7c00f6af`) at
   `+80` (mission/rift start points, e.g. `PlayerPortal_SQ02_M01_f012d4ed` `0000000fc7f96295`)
   and district destinations (type `3306cd8e`: Marais, Cite, Invalides, …).
7. **Crash.** Calling only `0x141D60C90([0x145219860], rec+72, rec+80)` from the window thread
   (focus shim mode 3) returned, then the game crashed: access violation reading `0xD8` at
   `0x140F94AEB` in `0x140F94AD0`, which reads `[0x141CD0D30() + 216]` (World → player) without a
   null check after only `0x140D69E20(1)`; the World had become the transition world (null).
   Saves were restored from the run snapshot (match the pin again).

Inference: a real fast travel first puts the game in a state where gameplay systems such as
`0x140F94AD0` stop (the preparation in observation 5) before the world handle is swapped.

8. `0x141C5D3C0` (the only caller of `0x141E42C00`) handles messages on the channel registered by
   `0x142DF1330` as `"NetMissionChannel.MissionHistory"` (`dword_145213530`): `0x141E42C00`
   applies mission history (co-op/resume), it is not the fast-travel button.
   `0x140007C70` and `0x141CCF930` have no call, jump or pointer reference in the mapped image.
9. **Load world by ID.** `0x141CD8A00(worldId u64, flag u8, u64, delay int, destinationId u64,
   chain u8)` (disassembly: `mov rdx, rcx` passes the full ID) → `0x141CD8A40(holder, …)` builds a
   world handle `0x1426EB640(&unk_14525BB70, id, 0)` and a destination handle
   `0x1426EAF00(&unk_14525BB70, id, 0)` and queues command 8 (push under the holder's lock);
   `chain` (byte +49) makes command 9 queue command 5 (switch). The travel manager calls it as
   `(world id, 1, 0, 6, destination id, 1)`.
10. Live (focus shim mode 4, run `travel-2-launch`, pid 116176): `0x141CD8A00(0000000f18b7e1cf, 1,
    0, 6, 0000000fc7f96295 PlayerPortal_SQ02_M01, 1)` → 2.2 s later holder `+0x90` names
    `ACU_Paris` and `+0x98` is a new World (`0x15F4E3C50`); the game kept running (60 fps, 5+ min)
    and rendered Paris geometry at low detail from a fixed camera. **No player:** World `+0xD8`
    stayed 0, the ACU-RE-0044 chain fails.
11. `0x141D5E520(world, &start)` selects the start (a handle in the World's list `+312`, u16 count
    `+322` — 765 in Paris — else default `+304`), stores it at `+328` and calls `0x141D68080`,
    which passes the start's transform (vtable `+136`) to the World's spawner `[+1976]`
    (`0x141DFA910`, `0x141DFB080`); guard byte `+2656`. Called live (shim mode 5, null handle →
    default start `0000000f18b7e1d4`): returned, no crash, still no player. The manager update
    `0x141D57AD0` also calls the destination's vtable `+152` first; for the default start object
    (vtable `0x143024080`) that slot is the shared stub `0x141E6D4E0`.
12. Saves were restored from the run snapshot after closing the game.
13. **In-game map, pinned save (run `map-travel-1`, pid 98004).**
    - Launched in the background, the game takes gameplay pad input but ignores Start and Back.
    - Posting `WM_ACTIVATEAPP(1)`, `WM_ACTIVATE(WA_ACTIVE)` and `WM_SETFOCUS` to the game window only
      (`focus.py activate PID`) made Start open the in-game menu and Back open the map.
    - The map shows the Saint-Louis and Recollets districts unfogged, the rest fogged. I missed
      the black **P** pin inside Saint-Louis; the user pointed it out. It is the Paris route (14).
    - The filter tabs and legend did not respond to RB or the left-stick click.
    - Probe entry hooks on `0x141D60C90`, `0x141CD8A40`, `0x141CE1840` and `0x141D5E520`
      (`tools/native_probe/world_travel.profile.json`, expected bytes checked) recorded 0 hits
      while browsing the map.
    - Saves were put back and match the pin.
14. **Fast travel to Paris through the map (run `map-travel-2`, pid 115844, pinned save).**
    - Hovering the P pin shows "Île Saint-Louis — [F] Fast Travel". Pad A, X, Y and a held A did
      nothing; the prompts are keyboard glyphs. F through the shim's new in-process keyboard
      (build t7, `focus.InProcessKeys`) opened "Confirm Fast Travel"; Enter confirmed.
    - Sampler every 50 ms (`world-samples.jsonl`); times from the confirm (s):

      | t | Change |
      | --- | --- |
      | 0.0 | travel manager `[0x145219860]`: `+592` = ACU_Paris handle, `+600` = `0000001770d432be` `PlayerPortal to Paris` (type `7c00f6af` WorldTransitionPortal, crc-name), `+644` = 1 |
      | 1.5 | holder queue count 2 |
      | 1.6 | holder `+0x90` = null handle `0x14525BB58`, `+0x98` World = 0 (transition) |
      | 4.5 | holder `+0x90` = ACU_Paris, World = `0x15CD7C850`, World `+0xD8` = 0 |
      | 23.8 | World `+0xD8` = `0x171947710`, a **new** player entity (was `0x160FD3A70`), `[E+0x60]` = new World, position (768.55, −46.65, −0.44) |
      | 28.3 | manager `+592` = null handle, `+644` = 0 (travel finished) |

    - Probe hits (return address → containing function, corpus file boundaries), in order:
      1. `0x141CE1840` (command 5) from `0x141D57C09` in `0x141D57AD0` (manager update), with
         `rdx = 0x1450A9600` (the transition world).
      2. `0x141CD8A40` (command 8) from `0x141CD8A32` in `0x141CD8A00`: `rdx` = ACU_Paris
         `0000000f18b7e1cf`, `r9`-side destination `0000001770d432be`.
      3. +2.73 s: `0x141D5E520(newWorld, …)` from `0x141D52391` in `0x141D522C0` (which calls it at
         corpus line 236).
      4. +2.98 s: `0x141D5E520` from `0x141CCF98A` and `0x141CE1840` from `0x141CCF9BE`, both in
         `0x141CCF930` (command 9 chaining into the switch).
      5. +4.40 s: `0x141D5E520(newWorld, …)` from `0x1413EAF8E` in `0x1413EAEF0`. That function
         gets a start handle (`0x1413E2A40`), then calls `0x141D5D320(0x141D4EC80(), owner, &start)`.
    - **`0x141D60C90` was not hit.** Yet the manager's `+592`/`+600`/`+644 = 1` were set, and the
      update `0x141D57AD0` did the transition and load (its `+644 == 1` branch). The image's only
      reference to `0x141D60C90` is the call at `0x141E42DC8` (in `0x141E42C00`); `0x141D57AD0` is
      reached by a jump at `0x141D54560`. Who set the state is unresolved: an unhooked thread, or
      a writer the `+ 644) = 1` search misses.
    - Inference: the player is not moved between Worlds. The new World gets a new player entity
      about 19 s after `0x1413EAEF0` asks for the start through the travel manager. This explains
      observation 10: a bare command 8 loads Paris but sets no manager state, so no player is made.
    - Saves were put back and match the pin.
15. **Round trip Paris → Versailles → Paris in one process (run `map-travel-3`, pid 122172).**
    - The launch used the pin but loaded **Paris** (holder `+0x90` = ACU_Paris, position
      496.66, −7.31). The save files had the pin hashes before launch; `.upload` files appeared at
      launch. Inference: Ubisoft Connect cloud sync replaced the pinned save with the Paris
      autosave from run `map-travel-2`. The pin alone does not decide the start world.
    - Paris → Versailles: the Paris map's "Versailles — [F] Fast Travel" pin is on Île Saint-Louis,
      next to the Café Théâtre (the user's hint). Versailles → Paris used the P pin as before;
      Arno had to walk ~30 m off the arrival point first, because his own marker covered the pin
      and the cursor could not select it.
    - Player entity per arrival (World `+0xD8`; "ID" is `[[E+0x130]+0x10]`, not in the asset index,
      so it is a runtime ID):

      | Arrival | World | Player | ID | Position |
      | --- | --- | --- | --- | --- |
      | Paris (load) | `0x11989F170` | `0x179577A10` | `F000494A` | 496.66, −7.31, −0.25 |
      | Versailles | `0x17A761D30` | `0x186EF6A50` | `F0019960` | 156.01, −172.59, −0.39 |
      | Paris again | `0x12535B5B0` | `0x12E470950` | `F00270DC` | 768.55, −46.65, −0.46 |

    - **Every arrival creates a new World object and a new player entity with a new runtime ID**;
      nothing reverts to the earlier address. What persists in the process: the world resource
      handles (Paris `0x66CFFA0`, Versailles `0x74066B10`), the holder `0xF743420` and the travel
      manager `0x740C2010`. Each trip follows the same sequence as observation 14: state 1,
      transition world, new World after ~3–4 s, player after another ~14–19 s, state back to 0.
    - Hooks on `0x141E42C00`, `0x141C5D3C0` and `0x141D60C90`
      (`world_travel_setter.profile.json`): **0 hits** across both trips, so the map route does not
      use the mission-history path. `0x1413EAEF0` was hit at each confirm, before the World change,
      from `0x141EBD513` in `0x141EBD4E0`. That function loops over a listener array
      (`qword_14521A830`, u16 count `word_14521A83A`) and calls each listener's vtable `+16` with no
      arguments; `rdx` held the target world handle (`0x74066B10`, then `0x66CFFA0`) as a leftover.
      Inference: confirming broadcasts a travel event to listeners, and the setter of `+644` runs
      just before or inside that broadcast.
    - Saves were put back and match the pin.
16. **Travel guard verified live (run `travel-guard-1`, pid 120672, shim t8 SHA-256 `1883f99e…272d`).**
    - Fast travel Paris → Versailles via the Café Théâtre pin. Before: player `0x1747A7BE0`,
      Human `0x174EBA350`.
    - The travel state was 1 at 2.0 s. `teleport.ready()` raised NotReady twice. A raw shim
      request with the old player (mode 0, its own matrix, so no move) returned −5 twice.
      `set_flag(God)` was refused without being sent.
    - Arrival at 23.8 s: new player `0x12D0833F0`, World `0xF77BA8A0`, position (156.01, −172.59,
      −0.39), the same arrival point as observation 15. The same raw request with the old player
      returned −6 and the game kept running.
    - `teleport.py keep-safe` re-armed Invisible and God Mode on the new player by itself (both
      status 1). A normal teleport afterwards: status 1, error 0.148 m.
    - Closing afterwards hung, and it took over the user's input and stalled their PC. The close
      had removed the shim first. Closing is now `capture_run.py --close` (README). Saves were put
      back and match the pin.

## Next

Find how the player entity is handed to the new World (who writes World `+0xD8`, and what the
travel manager's state machine `+644` does between the transition world and the target). The
debug manager wiring (ACU-RE-0046, `0x140ED6770` via vtable `+192`) shows the player component is
re-bound per world, so a world-level "player created/attached" event is the likely missing step.
Do not repeat the bare `0x141D60C90` call (observation 7).
Route that works (observation 14): `focus.py activate PID`, Back opens the map, move to the P pin
in Île Saint-Louis, then keyboard F and Enter through `focus.InProcessKeys` (shim t7).
Still open:
- Find who sets the manager's `+644 = 1` on the map path. Not `0x141E42C00`/`0x141C5D3C0`
  (observation 15). Next: the listeners in `qword_14521A830`, and whoever calls `0x141EBD4E0`.
- Paris → Versailles route: the "Versailles" pin on Île Saint-Louis by the Café Théâtre.
- Decode `0x1413EAEF0` / `0x141D5D320` (what creates the new player entity).
