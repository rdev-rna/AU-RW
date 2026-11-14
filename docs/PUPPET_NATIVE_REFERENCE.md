# puppet-native / "Unity Mod" for AC Mirage: reference note

Date: 2026-11-14. This is an external reference, not ACU evidence.

## What it is

- **Repository:** `github.com/venvyl/puppet-native` (author Venvyl, MIT licence). It is a mod
  platform injected as an ASI for Black Flag Resynced, Origins, Mirage, Valhalla and Shadows.
  - Release V1.6 notes say it "ported unity parkour for ac mirage".
- **Nexus mod:** "Unity Mod", AC Mirage mod 253, v1.0, uploaded 2026-10-27, marked beta. The page
  describes it as natively ported Unity parkour, combat and assassinations.
- **Mechanism, as the page describes it:** the ASI is a proxy between Unity and Mirage files. It
  loads data at run time and patches it in a layer on top of Mirage's packed data, which Mirage
  then plays.
  - The page says patching takes one to five minutes after load.
  - Leaps of faith are disabled because they crash.
- **Moves listed on the page:**
  - vault down and controlled descent (on a hotkey, instead of crouch);
  - manual vault, from hang and from the ground; manual slide vault;
  - swing to sidewall;
  - diagonal wall run (a placeholder) and wall-run eject;
  - reverse lache; roll; interrupt grab;
  - "faith impulse": swings in more places, plus side swing;
  - neutral jump and manual jump; vault-down-only; reverse wall grind.

## What the public source shows

This was reviewed read-only in a scratch copy and never built or run.

- `ACMirage.cpp` targets Mirage v1.1.1 and uses MinHook and XInput. Each move is a slot of patches
  on **Mirage's own executable** at Mirage RVAs:
  - forced branches (`jcc` → `jmp`);
  - forced flags (`sete al` → `mov al,1`);
  - redirected calls and code caves;
  - per-move hotkeys (for example ParkourDown on LB or C).
- No code in the public files reads Unity's forge archives or animation data. The data proxy the
  Nexus page describes is presumably in the distributed binary or the launcher build, which was
  not downloaded or inspected.

## What the Nexus package contains (inspected 2026-11-14)

- **Package:** `Unity-Mod 253 1.1` zip, SHA-256 `cfe73ff33c064283…72a779`, 9,730,701 bytes. It was
  listed in memory. Only the JSON, ini and forge files were copied out to a scratch folder; the
  DLL and ASI were never extracted or run.
  - 93 clip JSON files, named `<n>_-_<ACU animation name>.json`: parkour, climb, hang-free,
    swing, wall eject, assassination and throw clips.
  - `DataPC_extra_patch_02.forge` (851,968 bytes).
  - `winmm.dll` (7.4 MB; presumably an ASI loader, not inspected).
  - `Unity.asi` (185,856 bytes, SHA-256 `912f12e94b1642d9…11038ec`).
  - `mod_config.ini`: the hotkeys for each move.
- **Clip JSON:** per clip, the length plus every track with its ID, a `codec` byte, an `infotype`
  and keyframes (frame plus position, rotation, raw or int values). These are the clips' own
  tracks:
  - Frames are on the 60/s key grid.
  - `codec` equals our per-track type byte `t`, with codec = t / 3 (ACU-RE-0018).
  - Spot check: the root translation of `xx_Parkour_Leaping_Fd350cm-Dn200cm` (ACU file ID
    `000000083293199c`) matches `animation::parse_root` key for key (frames 0, 8, 24, 28 and
    38 = 0, 0.133, 0.4, 0.467 and 0.633 s; end at (0, 3.4961, −1.9978)).
- **The JSON decodes codecs our parser rejects:**
  - codec 7 (t = 21–23): 159 tracks;
  - codec 8 (t = 24, 26): 277 tracks;
  - plus codecs 6, 11, 13 (raw), 16 (frame-only) and 19 (int).
- **What `Unity.asi` does (string scan only, no disassembly):** it embeds the clip file names and
  logs lines such as `anim worker start base=…` and `leap[%d] tracks=%zu max=%u` for named slot
  groups (leap, eject, vault, vault down 300/800, hang-free…). The likely mechanism is that it
  reads the JSON and overwrites the tracks of existing Mirage clips in memory, while the patches
  above force Mirage's branches. So the "port" is Unity clips running on Mirage's parkour logic.
- **Not decompiled.** It is a JSON reader and a Mirage memory writer. The codec decoding happened
  wherever the JSON was exported, so decompiling the ASI would teach us nothing about `ACU.exe`.

## Relevance to AU-RW

- **Different goal.** It makes Mirage's engine play Unity-style moves, and possibly Unity data. It
  does not reimplement Unity's logic outside an engine, so it does not replace this project.
- **No ACU binary knowledge.** Its addresses and patches are for Mirage. Nothing in it is evidence
  about `ACU.exe`, and nothing in it is cited as such.
- **Inference (unverified):** if Mirage can play Unity parkour once Unity data is layered in, much
  of Unity's parkour behaviour lives in data (graphs, clip sets, parkour tables). The executable
  would then supply the machinery. This fits ACU-RE-0059, where parkour is clip root motion plus a
  target correction. It is not evidence for it.
- **Usable as a checklist.** The move list above names Unity actions to cover in variant capture
  and in the controls map. Check each one against ACU's own clips and input handling before using
  it.
- **A cross-check for codecs 7 and 8.** The JSON holds decoded values for codec 7/8 tracks of
  ACU clips. These are the codecs that block full pose decoding of 339 of the 590 played clips
  (ACU-RE-0059). Derive the decoder from `ACU.exe` (its samplers), then compare its output with
  these values as an independent expected result.
  - The JSON is Ubisoft-derived data from a third party. It stays local, is passed to a tool by
    path, and is never committed or quoted beyond a few spot values.
- **Reuse:** the code is MIT-licensed. Reuse needs attribution, and none is planned: its patches
  target another game.
