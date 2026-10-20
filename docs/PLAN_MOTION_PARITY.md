# Motion parity plan (opened 2026-10-20, replaces the capture-polling loop)

Goal: Arno's Rust motion (final bone pose **and** root/facing trajectory) matches
the original under identical input, with every claim backed by a numeric gate.

2026-10-20 user steering: investigate predictive traversal scanning feeding
dynamic root-motion target matching. ACU-RE-0030 establishes the landing-score
kernel and a scan-to-target-storage call chain. Prioritize tracing that target
into native action/animation displacement before introducing a scanner or warper
into the viewer. The terms describe the investigation hypothesis; the complete
native architecture and connection are not yet established. This does not erase
the reversal acceptance case or the outstanding pose-stage gates below.

## Where we really are (offline re-analysis of run 68, no game needed)

Script: `local/native-probe/motion-link-20261020/blendlab.py` equivalent (scratch).
Fixture: `run-68/blend-regression-fixture.json`, 14 coherent multi-leaf samples.

- Single-leaf evaluation is exact (0.003 deg). Multi-leaf **blend is also right**:
  hemisphere-aligned weighted sum of the bound leaf rotations gives a per-frame
  *median* error of 0.017-0.034 deg over ~53 bound joints (frames 28, 31).
- The blend formula is not the problem: weighted-sum vs sequential nlerp vs
  sequential slerp, any ordering, move the mean by < 0.02 deg. A global time shift
  (-60..+60 ms) does not help. Sign/hemisphere choice per leaf does not help.
- The whole residual (mean 0.4-1.7 deg, single bones 5-29 deg) sits on a handful
  of **low-index native joints** (native idx 2,3,4,5,12,13,14: the pelvis/spine/
  upper-chain roots) even where every leaf agrees on that bone (leaf dot ~1.0,
  native differs 11 deg). So something **after the blend** edits those bones:
  root-motion/turn extraction, or a procedural spine/lean/look layer.
- Frame 35 is a 32 deg whole-pose outlier: an incoherent sample, not a model error.

Conclusion: Codex's "blend regression failed" was a misdiagnosis. Do not tune the
blend. Find the post-blend stage by observing it, not by fitting around it.

## Why the earlier loop was slow

1. Polling process memory from outside; reads are never atomic, so every
   comparison carried unknown skew, and crowds added unrelated modifiers.
2. Debug-register hooks (4 slots, single-step exceptions) cannot sit on a hot
   path (the `0x988e3` site faulted with 0x80000004) and cannot copy buffers.
3. No stage ladder: the pose was compared at an unknown stage, then the
   hypothesis was rejected instead of locating where the stages diverge.
4. No ability to *control* the game, only watch it.

## Phases (each has a gate; stop and report at a failed gate, do not loop)

### P1 - in-thread tap with inline detours (new `acu_tap.dll`)
- Inline detour with own trampoline (length-disassembled prologue, RIP-relative
  fixups, byte guards against SHA `748abdc6...`), no debug registers.
- Hook entry/exit of the stage functions already guarded: `0x1401040a0`
  (anim eval, graph `+0x20e0`), `0x14009a7a0` (composition, clears `0x0c000000`),
  `0x1400e9cc0` (component method), plus the callers found from xrefs.
- On each exit copy, **on the game thread**, into a lock-free ring: controller
  graph leaves (id/time/offset/weight/binding ptr), every pose context (+flags),
  owner transform, QPC, frame/seq. Atomic by construction.
- Gate: 300 consecutive owner-matched snapshots, zero skew, game stays up 10 min.
  If a hook site is unsafe, fall back to hooking the caller one level up; never
  reinstall `0x988e3`.

### P2 - stage ladder
- Same snapshot at pre-blend, post-blend, post-each-later-stage. Diff buffers in
  the DLL to name the exact function that edits joints 2-5/12-14.
- Reproduce that stage in Rust (root/turn extraction or procedural layer), then
  gate: full 23 primary joints, held-out frames, median < 0.05 deg, p95 < 0.5 deg,
  plus shifted-time and mode-zero negative controls.

### P3 - oracle / live modification (fast, exhaustive coverage)
- DLL commands to **write**: pin the player graph to one clip or a chosen
  (clip, time, weights) set, freeze/step time, pause AI. Rust sends a test vector,
  the game returns its pose, compared immediately. Covers every clip and blend
  pair without crowds, sprint circles or luck.
- Also write Rust's pose into the live buffer for side-by-side visual A/B.
- Gate: all decodable player clips (29/34 now) pass single-leaf; 20 random blend
  pairs pass; undecodable codec-7/8 clips listed explicitly.

### P4 - root motion and locomotion controller
- Tap velocity/yaw/root per tick plus clip root deltas, with XInput injection
  at fixed schedule (existing runner). Fit nothing blindly: derive start/stop,
  turn-rate and accel from the logged controller state and clip root channels.
- Rust headless replay of the same input script; gate: root position error
  < 2 cm and yaw error < 1 deg at every tick of walk/sprint/turn/stop routes.

### P5 - ship into Bevy
- Graph-selected playback driving `PoseRig` with the verified stages; regression
  fixtures from P1-P4 kept in `acu-rs`, install-dependent tests marked explicit.
- Fresh visual capture beside a native screenshot for the same moment.

## Operating rules (to avoid the previous failure modes)

- One hypothesis per capture; decide pass/fail from a pre-stated gate.
- Offline first: mine existing runs (run 55/64/66/68) before launching the game.
- No new external-poll captures for pose; use the tap once it exists.
- Do not reopen IDA sessions that vanished; use saved assembly/runtime image.
- Record every failed gate honestly; update STATUS/TASKS once per phase, not per run.
- Preserve restore notes for each hook deployment (`local/native-probe/RESTORE.md`).

## Status update (2026-10-20, later)

- P1 tooling exists: `tools/native_tap/` (`acu_tap.dll`, MinHook inline detours via a
  generic thunk, game-thread snapshots, write tracer, POKE). `build.ps1` builds it and
  `acu_tap_selftest.exe`; the self-test passes 12/12 (args in regs/stack/xmm, return,
  snapshots, write tracing, unhook). Static helper: `tools/acu_static/callgraph.py`.
- Game launch needs a manual Play in Ubisoft Connect (`uplay://launch/720/0` did nothing).
  User launches; then attach with `acu_probe_launcher.exe --attach <pid> --dll acu_tap.dll`
  after checking the executable SHA-256.
- First hooks: component update `0x1400e9cc0` (rcx = component, +0xd0 = controller),
  graph update `0x1401040a0`. Then write-trace the pose joints that the blend model cannot
  explain (native idx 2,3,4,5,12,13,14) to name the post-blend editor.

## Methodology: identifying Arno (no heuristics-only matching)

1. Take component/controller/pose pointers from hook arguments; never scan heap addresses.
2. Player-only graph content: Arno's leaf IDs (sprint `000000016737c2c4`, idle
   `00000003836e1d49`, from run 63) must be present; crowd graphs lack them.
3. Input causality: a virtual-stick input changes only the true controller's leaf weights
   on the next frame.
4. Intervention test (decisive): make the hook skip the graph update for the candidate
   for a few frames; Arno must visibly freeze in a screenshot while the crowd continues.
   In-memory only; release afterwards.
5. Re-verify and record the evidence every session (addresses change on restart).

## Methodology: keeping data clean (walls, climbing)

- Low profile only (no RT) for pose/blend data; sprint (RT) triggers auto-climb.
- Every snapshot logs active leaf IDs; frames with climb/vault/collision leaves are
  excluded automatically from an allow-list, not by operator care.
- Compare root displacement per tick against the clip root delta and a max speed;
  violations flag and drop the interval and abort the run.
- Open plaza, short straight/arc legs; reset Arno's root with POKE between runs.
- Pose parity uses oracle mode (graph pinned to a chosen clip/time): no navigation at all.
- Pose-vs-graph consistency does not depend on position, so wall contact only affects P4.

## Methodology: fluid, non-robotic input

- Slew-rate limit on every profile (full deflection >= ~150 ms, reversals slower);
  violating profiles are rejected before the run, never silently clipped. Direction changes
  are polar arcs, not (x,y) -> (-x,-y) jumps.
- Frame-locked delivery: hook the game's `XInputGetState` and supply the value for each
  game frame from the profile at the game's tick (no wall-clock jitter); ViGEm is fallback.
- Log the stick value the game actually read each poll; report max per-frame step and peak
  jerk; reject runs over a human-scale bound.
- Teleport detector: per-tick root displacement vs clip root delta and max speed.
- Two profile classes, kept apart:
  - Natural (feel/visual): minimum-jerk easing, faster release with slight overshoot,
    magnitude/direction coupling on arcs, +/-2-3% drift above the deadzone, human-scale
    reaction gaps; all variation from a recorded seed so Rust replays it exactly.
  - Clean (parity measurement): plain slew-limited ramps, no noise.
- Optional human trace: record the user's real stick stream on a short walking route
  (laps and turns), replay it in the game, feed the identical stream to Rust.
- Compare against real play with `/watch` on an ACU parkour/walking video (user-suggested).
