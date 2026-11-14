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
- **Reuse:** the code is MIT-licensed. Reuse needs attribution, and none is planned: its patches
  target another game.
