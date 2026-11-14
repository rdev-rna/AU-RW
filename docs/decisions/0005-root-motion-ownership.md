# ADR-0005 — Root-motion ownership: simulation owns the player transform; animation root motion is an input

Date: 2026-11-14
Status: accepted. The native transform owner is now observed (ACU-RE-0056); the body's velocity source is still unknown.
Related task / findings: ACU-RE-0022, 0026, 0027 (pose, graph binding; root motion open),
ACU-RE-0032 (translation correction kernel), ACU-RE-0044 (player transform), ACU-RE-0046
(teleport), ACU-RE-0052 (camera-relative heading), ADR-0004 (tick).

Context and constraints:
- **Native observation (ACU-RE-0056, 2026-11-14):** the player's matrix has a single writer, the
  entity set-world-matrix `0x141CE1FF0`, reached by two paths per frame:
  - the physics sync `0x142608180`, which copies the physics body's world matrix minus a local
    offset and carries **all** displacement and heading change (30 mm per frame walking, 63 mm
    free-running, at 60 fps);
  - character code (`0x141A87F1C` → `0x1420F7AB0`), which re-writes the same values (0 mm, 0°).
  The entity transform is therefore a copy of the physics body's pose after the step.
- **Not established natively:** what drives the physics body: a movement-logic velocity, or one
  derived from animation root motion.
- Native observations that bear on it:
  - Heading follows the camera yaw plus the stick angle to within 0.1° (ACU-RE-0052 obs. 13).
  - Speed ramps up over about 0.6 s (0.9 → 1.4 → 1.6 → 2.7 m/s in run camera-2).
  - Teleporting means writing the matrix and resetting the Human behaviour; movement then
    restarts from the new matrix (ACU-RE-0046).
  - A per-frame correction of the animation step toward a target exists (ACU-RE-0032, an
    inference from arithmetic).
  - Parkour targets lie on decoded collision edges (ACU-RE-0055).
- AGENTS.md: do not guess root-motion ownership; keep simulation independent of Bevy.
- Today `acu_sim` has both paths: `step` (speed-driven) and `step_with_root` (displacement from a
  clip).

Options considered:
1. The simulation owns the transform. A kinematic controller integrates a velocity on the fixed
   tick. Animation root motion is one possible source of that velocity per state (locomotion: a
   speed or input model; parkour and actions: clip root displacement, warped to the target).
2. Animation owns the transform: the entity moves by sampled root motion everywhere.
3. Wait for the native owner before writing any movement.

Chosen approach and reason: option 1.
- It keeps one authoritative transform in `acu_sim`, deterministic on the ADR-0004 tick, with
  collision (ACU-RE-0050) applied in one place.
- It can express both native possibilities: per state, the velocity source is either the
  controller or the clip, so whatever the native owner turns out to be is a configuration choice,
  not a rewrite.
- Option 3 would block M2 on research that a cheap probe can settle later (see Validation).

Consequences / limitations:
- Each movement state declares its velocity source: `controller` or `clip_root` (+ optional
  target warp). Locomotion starts as `controller`, tuned against `data/oracles/locomotion.json`
  and `camera.json`. Parkour actions start as `clip_root` with a warp to the candidate target,
  because their endpoints are fixed geometry (ACU-RE-0055).
- Until the body's velocity source is known, no parity claim is made for foot sliding, speed curves or
  stopping distances; they are compared with the oracles as measurements only.

Validation:
- Done: the write watch on `E+0x50` (ACU-RE-0056) shows the physics body owns the transform. The
  ADR-0005 model holds, with the character body as the single authoritative transform.
- Next: a write watch on the body's translation and the set-velocity input, compared with the
  playing clip's root delta per frame. That decides `controller` versus `clip_root` per state.

Revisit trigger: the velocity-source probe's result, or a finding that names the root-motion consumer.

The native facts above are labelled as observed or not established; the ownership model is an
implementation choice.
