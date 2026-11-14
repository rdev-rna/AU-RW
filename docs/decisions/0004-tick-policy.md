# ADR-0004 — Tick policy: fixed 1/60 s simulation tick, with 1/30 s as a parity mode

Date: 2026-11-14
Status: accepted
Related task / findings: ACU-RE-0051 (tick and time, live at 30/60/100 fps caps),
`data/oracles/tick.json`, AGENTS.md (fixed simulation ticks, unresolved float determinism).

Context and constraints:
- Native observation: one engine frame runs per rendered frame. Game time and every gameplay
  clock (root, World A/B/C) advance by the measured frame dt (median 0.01666 s at 60 fps,
  0.03333 s at 30, 0.0100 s at 100). The clock tree updates about 10 clocks per frame.
- Native observation: physics takes one step per frame, at a rate chosen from a table:
  {120, 60, 45, 30} Hz with ±5 Hz hysteresis, a single 1/30 s step for 15–30 fps, and several
  1/30 s steps below 15 fps.
  - 60 fps: 1/60 s step.
  - 30 fps: 1/30 s step.
  - 100 fps: still 1/60 s per 0.01 s frame (1,042 of 1,052 frames).
  - The native game is therefore variable-step and frame-rate dependent.
- AGENTS.md: fixed simulation ticks, explicit input/state, seeded randomness, and record
  unresolved float determinism.

Options considered:
1. A fixed simulation tick of 1/60 s, decoupled from rendering (Bevy `FixedUpdate`), with render
   interpolation.
2. Reproduce the native variable step: dt = frame time, physics from the bucket table.
3. A fixed tick at another rate (30 or 120 Hz).

Chosen approach and reason: option 1, with the tick length as a parameter.
- At a 60 fps cap the native game runs exactly this: one 1/60 s frame with one 1/60 s physics step.
  1/60 is therefore both deterministic and a native-parity configuration.
- A 1/30 s mode reproduces the native 30 fps case for parity tests.
- Option 2 would make simulation results depend on the host frame rate, as the original's do,
  which breaks replays and oracle diffs.

Consequences / limitations:
- `acu_sim::DT = 1/60` stays, made configurable (1/60 default, 1/30 parity) when the M2 tick work
  starts.
- At 100 fps and other rates the port intentionally differs from the native game, whose physics
  then steps 1/60 s per 0.01 s frame. Parity is claimed only at 1/60 and 1/30, each compared with
  an oracle captured at that cap.
- Blend and timer durations in native code use clock ticks of 1/30000 s (ACU-RE-0051 obs. 4).
  Convert them to seconds explicitly.
- Float determinism across machines and builds is unresolved; replays are exact only on one
  build and machine until checked.
- Animation sampling and render interpolation between ticks are presentation, not simulation.

Validation:
- `data/oracles/tick.json`: the native frame dt and physics step at the 60 cap equal the port's
  default tick, and at the 30 cap the parity tick.
- Locomotion and camera oracles (captured at 30 and 60 fps caps) are compared using the
  matching tick.

Revisit trigger: evidence that a native gameplay system runs on its own fixed tick independent
of the frame (for example AI or animation at a fixed rate), or a need to match native behaviour
at frame rates other than 30/60.

Native observations are labelled above; the fixed tick is an implementation choice.
