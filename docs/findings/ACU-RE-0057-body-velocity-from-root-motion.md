# ACU-RE-0057 — What drives the player's physics body: animation root motion, divided by dt

Date: 2026-11-14. Related: [ACU-RE-0056](ACU-RE-0056-player-transform-writer.md) (the entity
transform is a copy of the physics body pose), [ADR-0005](../decisions/0005-root-motion-ownership.md),
[ACU-RE-0051](ACU-RE-0051-tick-and-time.md) (World clock: `+0x18` = dt s, `+0x1C` = 1/dt),
[ACU-RE-0025](ACU-RE-0025-player-root-full-rig.md) (AtomAnimComponent, controller `+0xD0`).
State: **observed live (runs body-chain-1, root-motion-1/2/3) plus static chain.** The physics body's
per-frame motion equals the skeleton's root delta in walking, strafing, free-running, climbing and
turning. The source of that delta inside the animation graph (`0x1400FA080`) is located but not
decoded.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, base
  `0x140000000`. Disassembly from the mapped image (Unity project)
  `local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`, SHA-256
  `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520` (capstone). Corpus files
  `1400EF1B0.c`, `14003E870.c`, `140099980.c`, `140037CB0.c`, `14007C840.c`, `14262AF90.c`,
  `14262AEE0.c`, `141A51F70.c`, `1416E4840.c`, `142611C80.c`, `142611DD0.c`, `1425FA760.c`,
  `142625230.c` (`E:/ReverseEngineering/Projects/Unity/ACU - Decompiled/`). Export identity is
  unverified (ACU-RE-0031). Direct-call sites were found by scanning the mapped image for `E8/E9`
  rel32 targets.
- Tool: native probe `HOOKDATA` (hardware data-write watches, ACU-RE-0056), build
  `0e3fb5d3fc5153b0…`. Script `tools/native_probe/root_motion_link.py` (SHA-256 prefix
  `809c646d54a55f94`): watches the motion object's delta, the velocity request slot and, with
  `--body`, the position target and the body translation; pad phases idle, walk, stop, strafe,
  free-run (RT+A), idle.
- Live: Paris, pinned save `capture-start-1`, pid 16980, **60 fps cap** (World clock 1/dt =
  60.19), focus shim t8, Invisible + God. Raw data (local, ignored):
  `local/native-probe/runs/root-motion-1/writes.jsonl` (`9f430d1a73c5739f…`, Arno at a wall: the
  walk became a climb), `root-motion-2` (`7eea147e8cfe4045…`, stick backward, away from the wall),
  `root-motion-3` (`1a4ff5f9faf68f21…`, `--body`), plus `body-chain-1` (solver and request slots).

## Observations

1. **Chain (static).** The AtomAnimComponent update `0x1400EF1B0` (player instance: vtable
   `0x142E7F780`) does, in order:
   - `0x1400FA080(controller = [anim+0xD0], …, r8 = &out)` at `0x1400EF692`; `out` holds a
     translation (`rbp-0x40`) and a rotation quaternion (`rbp-0x30`);
   - removes a pending offset: translation −= `[anim+0x2F0]`, rotation = conj(`[anim+0x300]`)/|q|² ⊗
     rotation, then resets `+0x2F0`/`+0x300` to constants (`0x1400EF6C6`–`0x1400EF789`);
     `+0x2F0` is accumulated earlier in the same routine through `0x140058080`;
   - `0x14003E870(skeleton = [anim+0xD8], &out)` → `0x140099980([skeleton+0x280], &out)`, which
     stores translation at `M+0x90` and rotation at `M+0xA0` (M = the skeleton's motion object);
   - `0x140037CB0(skeleton, biped = [anim+0xE0])`. With skeleton flag word `+0xE8` bit `0x200` and
     a biped present it calls `0x14007C840(out, entity, biped, scale, &M[+0x90], &M[+0xA0])`.
     `scale` = 1.0, or `entity+0xC0 × skeleton+0x190 × skeleton+0x194` when skeleton byte `+0xCD0`
     bit `0x10` is set. `+0xCD0` bit `0x20` selects `0x14007CD80` instead.
   - `0x14007C840`: world delta = rotation part of the entity matrix · (scale · translation);
     velocity = world delta × `f32 [clock+0x1C]` (1/dt; clock = `World+0x830` or `+0x7D8` by entity
     flag bit 58). `0x14262AF90(biped, &velocity, 1)` stores it in request slot `+0x130` and sets
     flag `+0x79D`. If |velocity·dt|² > 900, it moves the body to the target directly instead. The new
     orientation is the body rotation ⊗ the rotation delta, passed to `0x14262DA70(biped, &q, 1)`.
   - The solver `0x1425FA760` takes the slot as the body velocity (`0x1425FABE5`, body-chain-1),
     and the consumer clears it (`0x1425FB14B`). `0x142625230` sometimes projects the slot onto the
     plane normal `+0x660`, keeping its magnitude (`0x142625326`).
2. **Live player objects (pid 16980).** Entity components with owner `+0x10` = the entity:
   - AtomAnimComponent `+0xD8` = SkeletonComponent (vtable `0x142E76630`), and `+0xE0` =
     BipedComponent (vtable `0x143051E60`, the physics proxy of ACU-RE-0056).
   - Skeleton flags: `+0xE8` = `0xF36D` (bit `0x200` set); `+0xCD0` = `0x13` (bit `0x10` set, bit
     `0x20` clear).
   - Scale: `entity+0xC0`, `skeleton+0x190` and `skeleton+0x194` are all 1.0.
   - Motion object `[skeleton+0x280]` has no class name through the reflection read (vtable slot 5).
   - Idle: delta (0, 0, 0) and rotation (0, 0, 0, 1).
3. **One delta → one request per frame.** In every run the write order per frame is: root delta
   (`0x14009999B`), request (`0x14262B095`), optional projection (`0x142625326`), consumer clear
   (`0x1425FB14B`). The request follows the delta by a median 0.017 ms.
4. **Velocity = |delta| × 1/dt** (magnitude ratio for frames with a non-zero delta):

   | Run / phase | frames moving | \|delta\|/frame (median) | \|v\| (median) | v / (\|delta\|/dt), median (range) |
   | --- | --- | --- | --- | --- |
   | 1 climb (stick into the wall) | 27 | 0.0824 m, local z | 5.00 m/s vertical | 0.9995 (0.987–1.011) |
   | 1 free-run (climbing) | 98 | 0.0723 m | 4.34 m/s | 1.0003 (0.987–1.020) |
   | 2 strafe on flat ground | 117 | 0.0254 m, horizontal | 1.52 m/s, vz 0 | 1.0004 (0.984–1.093) |
   | 2 free-run on flat ground | 132 | 0.0580 m (0.052 horizontal) | 3.47 m/s | 1.0000 (0.941–1.033) |

   In run 1's strafe phase the request was 0.84 × |delta|/dt. Arno was on the wall there, so the
   pairing or the projection differs; that phase is excluded.
5. **The body moves by the delta** (run 3, body translation `[biped+0x768]+0x260` watched): the
   integrator writes at `0x142628691` and a re-sync at `0x14262DCE0`. The per-frame body step over
   |delta|:
   - walk: 74 frames, median 1.003;
   - strafe: 93 frames, median 1.000;
   - every frame with a zero delta (idle, stop, walk start while turning): step 0.0000.
6. **Local axes.** On flat ground the delta lies on local **+y** (strafe median (0.000, 0.0243,
   0.000); free-run (0.000, 0.0519, 0.0004)). Climbing deltas lie on local **z**. This matches
   ACU-RE-0044/ADR-0003 (y forward, z up).
7. **Turning comes from the rotation delta.** At the start of the strafe phase (run 1), the delta
   quaternion was (0, 0, −0.072, 0.997): about 8.3° of yaw per frame with zero translation, while
   Arno turned in place.
8. **A second mode exists.** `0x141A51F70` clears all four request slots and flags (`+0x120..+0x150`,
   `+0x79C..+0x79F`) and `+0x7A0`/`+0x7A2`. It then sets a position target through
   `0x14262AEE0(biped, &target, 0)` (target `+0x1D0`, flag `+0x7A2`, i.e. `+1954` = 1). Its
   entity is `[[a1+0x28]+8]+0x1C88`, the same path as ACU-RE-0056's path A.
   - Seen in run 2 in 57 of 123 strafe frames and 4 of 182 free-run frames: the root-motion
     request was written, then cleared before the solver.
   - Not seen in run 3. Its trigger and the source of `target` are not traced.
   - `0x1416E4840` clears the same slots without a target (run 1: walk and stop frames with a zero
     delta).

## Inferences

- **On the observed states, animation root motion drives the player.** The animation graph's root
  delta becomes the body's velocity and rotation. The body is stepped and collided, and the entity
  copies the body (ACU-RE-0056). No speed or input model writes the body in those states. Input
  reaches motion only through what the graph plays.
- The walking-speed oscillation within a gait (body-chain-1: 0.55 → 2.5 → 1.1 m/s) is the clip's
  root curve. That explains the speed ramps seen in ACU-RE-0052 (camera-2).
- `anim+0x2F0`/`+0x300` look like corrections already applied to the entity this frame, for
  example a warp. They are removed so they are not applied twice. This is not checked.
- The position-target mode (obs. 8) is probably a constrained or corrective placement, for example
  stairs, a slope or snapping during a transition. Its input may itself be derived from the root
  delta; that is unknown.

## Unknowns

- How `0x1400FA080` extracts the root delta from the playing clips (which bone or track, how blend
  weights combine, the ACU-RE-0032 correction toward targets). This is the next step for parkour
  warps.
- The trigger and target of the position-target mode `0x141A51F70`, and `0x14007CD80` (skeleton
  flag `0x20`).
- Combat, vaults and scripted actions were not isolated. Climbing and free-run phases were.

## Development handoff

- **Consumer:** ADR-0005, `acu_sim` movement and `acu_motion` sampling.
- **Model for the port:**
  - Per tick, the animation graph produces a root delta (translation, local y-forward/z-up, plus
    a rotation quaternion).
  - The simulation rotates the translation into world space with the body's current orientation,
    converts it to a velocity (`delta / dt`), and lets the body step and collide.
  - The new orientation is the body rotation ⊗ the rotation delta. The entity copies the body.
  - A per-tick displacement above 30 m (`|v·dt|² > 900`) teleports the body instead.
  - In `acu_sim` terms, locomotion's velocity source is `clip_root`, not `controller`.
- **Oracle numbers (60 fps cap):**
  - strafe 1.52 m/s and free-run 3.47 m/s on flat ground;
  - climb 5.0 m/s vertical;
  - velocity / (delta/dt) = 1.000 ± 0.01 in most frames.
  These are run-specific samples, not per-clip constants.
- **Safety:** these watches cost about 130–200 traps per second on one entity. Capped launch only.
