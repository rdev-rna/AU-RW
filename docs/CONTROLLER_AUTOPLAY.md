# Controller-first gameplay capture

`tools/native_probe/controller_autoplay.py` drives an external virtual Xbox 360
controller through a local ViGEmClient DLL. For the checkbox interface, run
`python tools/native_probe/capture_launcher.py`. It finds an already running ACU
process, attaches the native probe if needed, and offers menu entry or a control
sweep, memory checkpoints, and optional live-image/decompile inputs. The 14
category checkboxes are recorded in each capture plan. Only `input` currently
has a verified event hook; the others are requested categories awaiting mapped
sites. Memory snapshots remain process-wide and do not filter by category. The
launcher targets an existing ACU process. Before any input it checks the
foreground PID and the exact executable SHA-256. It releases all controls when
steps end, focus changes, F12 is pressed, or the runner exits. It never writes
game memory.

## Enter the world

`tools/native_probe/menu_then_world.example.json` waits four seconds, presses A
to leave the title screen, waits five seconds, presses A on the selected menu
item, then waits 45 seconds for loading. On this installation, the two A presses
were followed by a live view of Paris's crowd scene and gameplay HUD about 30
seconds later. Other title/update prompts can require a different profile.
Steps marked `checkpoint` can take read-only process snapshots.

Example after the repaired game has reached the menu and the probe launcher
reports a sustained ready PID:

```powershell
$pidAcu = 1234
$exeHash = (Get-FileHash "D:/Games/Assassin's Creed Unity/ACU.exe" -Algorithm SHA256).Hash
python tools/native_probe/controller_autoplay.py $pidAcu tools/native_probe/menu_then_world.example.json local/native-probe/menu-run --expected-sha256 $exeHash --snapshot main
```

Run this with the ACU window focused. After attaching a ViGEm target, the
runner briefly sends a distinctive left-stick state, checks all four XInput
slots, releases the probe state, and records both the ViGEm-reported index and
the observed XInput slot in `run.json`. `--expected-user-index` guards the
observed slot. On this host ViGEm reports 0 while the state is visible at XInput
slot 3. F12 stops replay immediately; the timeline records controller state
and timings after the calibration pulse has been released.

## One-command capture

```powershell
python tools/capture_preset.py apply                 # once, ACU closed
python tools/save_pin.py pin <name>                  # once, ACU closed: the capture start
python tools/native_probe/capture_run.py local/native-probe/runs/<run> --save-pin <name> --image <mapped image>
```

`capture_run.py` needs ACU closed (or `--attach <pid> --keep-game` for a game already in the
world). Stages, each recorded with timings in `capture.json`:

1. **save_pin**: snapshot the current saves, copy the pin in (hash-verified).
2. **launch**: Ubisoft Connect `uplay://launch/720/0`; wait for the engine window; check the
   executable SHA-256.
3. **attach**: focus shim (`--fps-cap`, default 60; user input blocked, see below) and probe.
4. **world**: tap A every 5 s (title, Continue, any post-load prompt) until the player entity
   (`player_state.py`, ACU-RE-0044) leaves the loading placeholder's origin, then wait until
   its position holds still. Arno is never moved. Records position, forward and client size.
5. **scenario**: `controller_autoplay.py --background --in-process-pad --probe-profile`.
6. **decode**: `tools/parkour_trace.py`.
7. **close**, then **save_putback**: the saves found before the run are restored, so your own
   progress is unchanged (`--keep-run-save` keeps the run's autosave instead).

`--until world` stops after stage 4 and leaves the game running with shim and probe attached,
for live RE; the put-back command is then printed in `capture.json`. If the world is not
reached within `--load-timeout` seconds the game window is saved as `not-in-world.png` and the
game is closed. `--screenshot-step` takes step names from the moves file (default: the last).

**Your mouse and keyboard never reach the game** (default; `--allow-user-input` turns it off).
ACU takes the foreground a few seconds after its window appears. The shim (`block_user_input`)
then hands the foreground back to the window you had before the launch (during its first 90 s),
releases any cursor clip, drops the game's `ClipCursor`/`SetCursorPos`/cursor hiding, freezes
`GetCursorPos`, blanks DirectInput and sends raw-input/mouse/keyboard messages to the default
window procedure, even if you click the game window. The scripted controller is unaffected.
Counters are in the shim status (`blocked`, `neutralised`, `focus_returns`).

Repeatability, 2026-11-14 (ACU-RE-0044): with the pinned save and this world check, three runs
started at the same position and facing. The same timed move script still produced different
traversal from there (233 vs 34 selection calls), so targeted captures need closed-loop control
(`navigate.py`), not longer scripts.

History: a fixed "A, wait, A" once stopped on the menu; free-run nudges used as a world check
moved Arno by load-time-dependent amounts (6 vs 2 nudges, different streets). Ubisoft Connect
writes `<slot>.z`/`<slot>.upload` beside the saves while uploading after exit; `save_pin.py`
waits for them to go before copying.

## Clips, decisions and staged sweeps

Every `capture_run.py` scenario also runs `tools/native_probe/live_state.py` (player transform
and playing clips, about 30 Hz, reads only) and pairs decisions with clip starts in `clips/`
(`tools/parkour_clip_pairs.py`, names from the asset index). `--no-state` turns it off.

To capture one action from several angles:

```powershell
python tools/native_probe/capture_run.py local/native-probe/runs/<live> --save-pin <pin> --until world
python tools/native_probe/staged_sweep.py <pid> local/native-probe/runs/<sweep> --spot X Y --heading DEG --angles -40 -20 0 20 40 --route local/native-probe/runs/<earlier>/live-state.jsonl --image <mapped image>
python tools/save_pin.py putback <snapshot from the first command's capture.json>   # after closing the game
```

Find spots in earlier runs' `live-state.jsonl` + `clips/summary.json` (clip names such as
`xx_Window_*`, `xx_Beam_*`, `xx_Parkour_*Vault*`). `--route` follows the path that run took to
the spot (straight lines get stuck on street furniture). Each trial returns along its own
path, so traversals through windows or over obstacles are undone by the game's own movement.
`sweep-summary.json` lists, per trial: run start, height range, parkour decisions and every
non-locomotion clip that started. The angle reported is Arno's measured travel direction.

## Background runs (no focus, no ViGEm)

With the focus shim installed (`tools/native_probe/focus.py install <pid>`), `--background`
replaces the foreground guard with a check that the shim is live, and `--in-process-pad`
serves the scripted controller from inside the game instead of a ViGEm device. Background
screenshots render only the game window (`PrintWindow`); the desktop is never captured.
F12 still stops a replay.

```powershell
python tools/native_probe/controller_autoplay.py <pid> tools/native_probe/parkour_moves.example.json local/native-probe/runs/<name> --expected-sha256 <exe-sha256> --background --in-process-pad --probe-profile tools/native_probe/parkour_selection.profile.json
```

`continue_from_menu.example.json` presses A on the highlighted Continue item and waits for
the world. On 2026-11-14 the ViGEm virtual pad device (`USB\VID_045E&PID_028E\01`) was
disabled in Device Manager, so XInput never saw it; the in-process pad does not depend on it.

### Startup videos (game-folder change, reversible)

On 2026-11-14 the user supplied `FastUnity-391-1-0-1759610364.rar` (SHA-256
`86c7da1c3f123244914ec7f7d3c34de93354a67de1d50c65eef39e920e2b0c03`; from its name, mod
391 v1.0 on Nexus Mods). It contains 32 identical 480-byte Bink 2 placeholders
(SHA-256 `6b2d145f…327cc`) for `videos/AbstergoLogo.bk2`, `UbisoftLogo.bk2`,
`pc_TWIMTBP.bk2`, `ACU_TitleScreen.bk2` and the 14 per-language `pc_WarningSaving.bk2` /
`warning_disclaimer.bk2` clips, plus a ReadMe. No executable content. The archive is not
redistributed here.

```powershell
python tools/intro_skip.py install <extracted-archive-folder>   # backs up, then replaces
python tools/intro_skip.py status
python tools/intro_skip.py restore                               # originals back, hash-verified
```

`intro_skip.py` accepts only small `KB2` files whose paths already exist under the game's
`videos/`. Originals (95 MB) and their hashes are kept in ignored
`local/game-backups/intro-skip/`. After installing, a Ubisoft Connect launch goes from the
launcher splash straight to "Press any key"; the menu still needs title A + Continue A.

### Low-GPU capture preset and 60 fps cap

```powershell
python tools/capture_preset.py apply      # ACU closed; backs up both settings files first
python tools/native_probe/focus.py install <pid> --fps-cap 60
python tools/capture_preset.py restore    # ACU closed; original files, hash-verified
```

The preset sets every `[Graphics]` quality key in `Documents/Assassin's Creed Unity/ACU.ini`
to its lowest value, windowed 1280x720 and VSync off (`Stretching=1` scales the render to the
window), and the same values in the NVIDIA GSA SDK file `GFXSettings.ACU.xml` next to it
(UTF-16LE; `REFLECTIONS`, which has no ACU.ini key, also goes to 0). Changing only ACU.ini
does not hold: on 2026-11-14 a launch rewrote it from the xml's 1080p values about 11 s
after the Ubisoft Connect launch, and the game ran at a 1920x1061 client area. With both files
set, the game rewrote both at startup with the preset values kept, and the client area was
1280x720. The xml/ini name pairing is an implementation choice from matching values, not a
decoded mapping. ACU has no frame-rate option, and VSync in a window follows the desktop refresh
(75 Hz here), so the 60 fps cap is the shim's `Present` limiter. Keep this preset fixed for
all captures: quality settings may change what the game simulates.

Measured 2026-11-14 on an RTX 2070 SUPER (ACU's own 3D-engine utilisation, Windows GPU
counters): title screen 72 % at 61 fps uncapped at 1080p before, 49 % at 60 fps after; in
the world with the preset, 40 % at 60 fps. An in-world before value was not captured. These
were taken before the xml was covered and the window size was not read back then, so whether
that launch actually ran at 1280x720 is not established; treat them as indicative only.

## Input sweep and native correlation

After reaching gameplay, `controller_sweep.example.json` sends every ordinary
X360 gameplay input independently: four D-pad directions, Start/Back, thumb
clicks, bumpers, A/B/X/Y, both triggers at half/full, and cardinal/diagonal
positions for both sticks. It deliberately excludes the system Guide button.
This is an input exercise, not a claim about ACU's action mapping. Use the
timeline and a native session trace to see which verified sites respond.

```powershell
python tools/native_probe/xinput_profile.py $pidAcu local/native-probe/live-xinput-profile.json
python tools/native_probe/controller_autoplay.py $pidAcu tools/native_probe/controller_sweep.example.json local/native-probe/controller-sweep --expected-sha256 $exeHash --snapshot all --max-bytes 4294967296 --probe-profile local/native-probe/live-xinput-profile.json
```

Probe profiles must contain independently verified addresses and expected bytes;
the example subsystem profile intentionally contains no active offsets. Hook
sessions run alongside controller input and are unhooked on normal stop. When a
mapped image inventory exists, pass all three options to automatically join
captured event RIPs to candidate names from the local decompile corpus:

```powershell
--xref-functions local/native-probe/live-inventory/functions.jsonl --xref-summary local/native-probe/live-inventory/summary.json --corpus "ACU - Decompiled"
```

The join is address discovery only. It does not prove export-to-binary identity
or function meaning. `--snapshot all` captures sequential readable committed
regions up to the explicit byte budget, records holes, and is not a coherent
whole-process snapshot. Keep generated memory captures under ignored `local/`.

## Controller dependency provenance

The host has a ViGEm virtual bus and an Xbox-compatible controller. The official
ViGEmClient SDK was built locally from upstream tag `v1.16.18.0`, commit
`9e91a124d179bf26a878a952153042ac871da243`; its DLL and source remain in ignored
`local/native-probe/ViGEmClient-src/`. On the current host,
`vigem_target_x360_get_user_index` reports 0 while the state from the virtual
pad is visible through XInput slot 3; slots 0-2 expose a persistent controller
state. The runner now calibrates the observable slot instead of treating the
C API's reported index as the XInput slot. The upstream project is an archived,
retired SDK under MIT license; this setup does not install or change the driver.
The runner finds the locally built DLL automatically, or accepts an explicit
path through `--vigem-client` / `VIGEMCLIENT_DLL`.

An earlier launch attempt exited before menu; Ubisoft Connect's logs showed the
game child disconnecting, and a prior verification reported 37 corrupt files
(36 required). The game subsequently launched through Ubisoft Connect as PID
25068, and the native probe attached successfully. The two virtual A presses
reached the world, but a screenshot command changed foreground during the
loading wait, so the bounded replay stopped and released the pad. The subsequent
29-step full sweep completed. A concurrent byte-guarded trace recorded 4,243
XInputGetState entries across XINPUT1_3.dll and xinput1_4.dll, including game
polls for user indices 0 and 1; 84 events were dropped or overwritten during
high-frequency capture. This proves API polling and virtual report submission,
not which ACU action each report caused. The after-sweep screenshot shows the
Skills UI. Only `input` currently has a verified native capture category; the
remaining GUI categories report “No verified native probe mapped yet.”

The all-readable-memory capture is being rerun after the first pass hit the
single-client named-pipe limit while an XInput session was active. It must finish
without other probe clients connected; its manifest records holes and the result
is sequential, not coherent.
