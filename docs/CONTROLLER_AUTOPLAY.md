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
