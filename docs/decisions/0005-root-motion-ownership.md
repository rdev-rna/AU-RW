# ADR-0005 — Root-motion ownership: simulation owns the player transform; animation root motion is an input

Date: 2026-11-14
Status: accepted. Revisited 2026-11-14: the native transform owner is observed (ACU-RE-0056), and so is
the body's velocity source on walking, strafing, free-run, climbing and turning: animation root
motion (ACU-RE-0057).
Related task / findings: ACU-RE-0022, 0026, 0027 (pose, graph binding; root motion open),
ACU-RE-0032 (translation correction kernel), ACU-RE-0044 (player transform), ACU-RE-0046
(teleport), ACU-RE-0052 (camera-relative heading), ACU-RE-0056 (transform writer), ACU-RE-0057
(body velocity from root motion), ACU-RE-0058 (root-delta extraction), ADR-0004 (tick).

Context and constraints:
- **Native observation (ACU-RE-0056, 2026-11-14):** the player's matrix has a single writer, the
  entity set-world-matrix `0x141CE1FF0`, reached by two paths per frame:
  - the physics sync `0x142608180`, which copies the physics body's world matrix minus a local
    offset and carries **all** displacement and heading change (30 mm per frame walking, 63 mm
    free-running, at 60 fps);
  - character code (`0x141A87F1C` → `0x1420F7AB0`), which re-writes the same values (0 mm, 0°).
  The entity transform is therefore a copy of the physics body's pose after the step.
- **Native observation (ACU-RE-0057, 2026-11-14):** the physics body is driven by the animation
  graph's root delta.
  - The AtomAnimComponent update gets a translation and rotation delta from the graph
    (`0x1400FA080`) and stores them on the skeleton's motion object.
  - The physics proxy's velocity request becomes R(entity) · delta · (1/dt), and its orientation
    becomes q_body ⊗ q_delta.
  - Measured: velocity / (|delta|/dt) = 1.000 ± 0.01; body step per frame / |delta| = 1.000–1.003;
    a zero delta gives zero motion.
  - A second, rarer mode (`0x141A51F70`) cancels the request and sets a position target instead.
    Its trigger is not traced.
- Earlier native observations that bear on it:
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
  target warp).
  - Since ACU-RE-0057, **locomotion is `clip_root`** (walk, strafe, free-run, climb, turning in
    place): velocity = R(body) · root delta / dt, and orientation = q_body ⊗ q_delta.
  - Speed curves, stops and turns therefore come from which clips the graph plays and how it
    blends them, not from a tuned speed model.
  - `controller` remains available for synthetic tests and for states not yet observed (combat,
    scripted actions).
  - Parkour actions are `clip_root` with a warp to the candidate target, because their endpoints
    are fixed geometry (ACU-RE-0055). The warp itself (ACU-RE-0032) is still an inference.
- Parity for foot sliding, speed curves and stopping distances now depends on native clip selection
  and on root-delta extraction (`0x1400FA080`, not decoded). Until those are reproduced, these
  are compared with the oracles as measurements only.

Validation:
- Done: the write watch on `E+0x50` (ACU-RE-0056) shows the physics body owns the transform. The
  ADR-0005 model holds, with the character body as the single authoritative transform.
- Done: write watches on the skeleton's root delta, the velocity request and the body
  translation (ACU-RE-0057) show `clip_root` for locomotion.
- Done: the root-delta extraction (ACU-RE-0058).
  - Per clip: delta_t = R(Q(A))⁻¹ (T(B) − T(A)) and delta_q = Q(A)⁻¹ Q(B), from tracks 0/1. This
    matches live to within 0.02 mm.
  - In a clip-to-clip transition, root motion comes from the incoming clip, not the pose blend.
- Next: implement it in `acu_motion` and check it with `tools/root_delta_compare.py`. Then test
  rotating roots and directional blends live.

Revisit trigger: a state found to be driven by `0x141A51F70`'s position target or by
`0x14007CD80` (skeleton flag `0x20`), or combat or scripted actions showing a different source.

The native facts above are labelled as observed or not established; the ownership model is an
implementation choice.
