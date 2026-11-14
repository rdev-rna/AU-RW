# ADR-0005 — Root-motion ownership: simulation owns the player transform; animation root motion is an input

Date: 2026-11-14
Status: accepted (implementation choice; the native owner is unknown)
Related task / findings: ACU-RE-0022, 0026, 0027 (pose, graph binding; root motion open),
ACU-RE-0032 (translation correction kernel), ACU-RE-0044 (player transform), ACU-RE-0046
(teleport), ACU-RE-0052 (camera-relative heading), ADR-0004 (tick).

Context and constraints:
- **Not established natively:** whether the player's position (E+0x50) is written from animation
  root motion, by a movement or physics controller, or both. No finding identifies the writer.
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
- Until the native owner is known, no parity claim is made for foot sliding, speed curves or
  stopping distances; they are compared with the oracles as measurements only.

Validation (the probe that settles it): a hardware write watch on the live player's `E+0x50` while
walking and during one vault. The writing routine, its caller and whether the value equals a clip
root delta settle the native owner per state. Record the result in a finding and revisit this
ADR.

Revisit trigger: that probe's result, or a finding that names the root-motion consumer.

The native facts above are labelled as observed or not established; the ownership model is an
implementation choice.
