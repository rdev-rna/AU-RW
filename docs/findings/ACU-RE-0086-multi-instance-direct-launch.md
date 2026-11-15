# ACU-RE-0086 — Can several ACU games run at once on one machine?

Date / investigator / related task: 2026-11-15 / claude-session / #85 (co-op RE, two-instance capture)
State: observed (workaround established; the game's own check is not located)

## Provenance

Binary path / SHA-256 / image base / VA / RVA: `D:/Games/Assassin's Creed Unity/ACU.exe`,
SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40` (checked by
`acu_probe_launcher` and `capture_run` for every game). No VA/RVA: the creating code is not located.
Install state: active `uplay_r1_loader64.dll` is not Ubisoft's (156,672 bytes, SHA-256
`532faf5284334a654073b5e65fbb51caf00d3d2e8d994dc539625564a5e8743d`, see ACU-RE-0085). The
Ubisoft-signed loader is kept beside it as `.dll.bak`. This finding holds for that install state only.
Corpus path and lines / export identity status: the name strings `scimitar_semaphore` and
`CDdf212806D6EmB31yE0c` are in neither the on-disk image (protected; `grep -a` found 0 hits in ACU.exe,
the loader, GDF.dll, NvGsa.x64.dll) nor the decompiled corpus (`rg -F` over all 217,041 files, 0 hits).
Archive identity: not applicable.
Tool/provider/version: Windows 10 19045; ctypes `NtQuerySystemInformation(SystemExtendedHandleInformation)`,
`NtQueryObject`, `NtQuerySemaphore`, `NtQueryMutant`, `DuplicateHandle` (`tools/native_probe/instance_guard.py`);
`capture_run.py` and `multi_launch.py` capture.json records under ignored `local/native-probe/runs/`.

## Result

Observations:
1. `uplay://launch/720/0` with one ACU running started no second game within 120 s (#30 comment of
   2026-11-15, run `multi-B-20261115`).
2. `ACU.exe` started directly (CreateProcess from the game folder, no arguments) with no other game
   running: its own PID opened a `ScimitarEngineWindowClass` window after 3.8 s and reached the title
   screen. The PID did not relaunch through Ubisoft Connect.
3. A second direct `ACU.exe` while the first ran exited with code 0 without opening a window.
4. Named objects in the running game (types Semaphore/Mutant/Event/Section) include a semaphore
   `\Sessions\1\BaseNamedObjects\scimitar_semaphore` with current count 1, maximum 1. Nobody holds
   it, so it is not used as a lock. There is also an unowned mutex
   `\Sessions\1\BaseNamedObjects\CDdf212806D6EmB31yE0c` (count 1, not owned by the caller).
5. After only the semaphore handle was closed in the first game (`DuplicateHandle(...,
   DUPLICATE_CLOSE_SOURCE)` from outside), a second direct launch opened its own window in 3.3 s. The
   mutex was left alone. Both games kept rendering their own title screens and both focus shims
   stayed live. The second game created its own `scimitar_semaphore` (`instance_guard.py list`
   showed only it as holder). Both closed gracefully with `capture_run.py --close` (1.8 s and 1.7 s).
6. End to end with the tools (`multi_launch.py 2 --until world --save-pin capture-start-1
   --fps-cap 60`): see "Two-game world run" below.

Inferences and alternatives:
- ACU's single-instance check depends on the existence of `scimitar_semaphore`: probably
  CreateSemaphore then ERROR_ALREADY_EXISTS leads to exit 0. That the check is a semaphore existence
  test is inferred from 3 and 5. The call site is not seen. The mutex `CDdf...` may belong to the
  non-Ubisoft loader or another module; it does not block a second game.
- With the signed Ubisoft loader, a direct start may relaunch through Ubisoft Connect and exit, as
  the earlier `capture_run` note "ACU.exe started directly exits" says. Not retested here.

Unknowns / incomplete coverage:
- Where the semaphore is created and checked (needs the runtime-view IDB or a hook on
  CreateSemaphoreExW in a new process).
- Whether the game uses the closed handle value later (wait, release or CloseHandle at exit). If it
  does, the value may by then name another object. No fault has been seen in the runs here.
- Whether two games share any other per-machine state safely: the one save folder (handled by the
  save-pin lease), the shader cache, logs and Ubisoft Connect overlay state.
- Whether two games can see each other through the co-op/LAN layer (the next #85 step).

Independent comparison / falsification test: closing the mutex alone (not done) would show it is
not the guard. Holding a semaphore of that name from a third process with no ACU running, then
starting ACU.exe directly, should also make the game exit 0.

Reproduction steps:
```powershell
python tools/native_probe/multi_launch.py 2 --until title --fps-cap 60
python tools/native_probe/instance_guard.py list
python tools/native_probe/capture_run.py --close <pid>   # each game
```

Implementation implications and finding-linked constants:
- `instance_guard.GUARD_NAME = "scimitar_semaphore"`, type Semaphore. Match on the last path
  component so either session namespace is covered.
- `capture_run --direct` releases the guard only in the game it launched, inside the launch lock, so
  the next launch is never blocked by its own games. Another holder is refused by PID, not touched.

## Two-game world run

`python tools/native_probe/multi_launch.py 2 --until world --save-pin capture-start-1 --fps-cap 60`
(summary `local/native-probe/runs/multi-20261115-190751.json`):

| run | PID | guard handle closed | save lease | world after first A | position | shim |
|---|---|---|---|---|---|---|
| 1 | 129608 | 0x3b8 | swapped (pin in) | 51.0 s | 803.723, -59.708, 6.997 | live, 60 fps cap, 61 fps |
| 2 | 121808 | 0x26c | joined, `live_matches_pin` true | 61.4 s | 803.723, -59.708, 6.997 | live, 60 fps cap, 61 fps |

- Client area 1280x720 in both. Afterwards `instance_guard.py list` showed no holder.
- Fresh screenshots of both windows show Arno in the same pinned interior and the HUD, each window
  rendering its own frame.
- Both HUDs show the profile name `3DMGAME`. That is the name the installed loader supplies, not a
  Ubisoft account, so both games run as the same player identity. This is unexamined, and may
  matter for co-op peer/session IDs.
- The lease held both games (`save_pin.py lease`: two live holders). Each game is normally closed
  with `capture_run.py --close <pid>`, then its printed `save_pin.py release` line is run. The last
  release puts the saves back.
- Load: on this 6-logical-processor machine, the shims' last status files showed 27 and 48 fps
  under the 60 fps cap (37,393 and 36,921 presents, about 10 minutes). Two games at 60 fps is more
  than this machine sustains. Lighter instances (lower caps or settings for a second game) are a
  follow-up.
- Both games had exited by about 10 minutes, not through `capture_run.py --close`. How they ended was
  not observed. `save_pin.py settle` pruned the two dead holders and put the saves back (snapshot
  `20261115-190752`).
