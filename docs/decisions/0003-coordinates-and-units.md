# ADR-0003 — Coordinates and units: native Z-up metres in data and simulation, Y-up only in Bevy

Date: 2026-11-14
Status: accepted
Related task / findings: ACU-RE-0044 (player transform), ACU-RE-0047 (placements), ACU-RE-0050
(collision/ground), ACU-RE-0052 (camera), `docs/REWRITE_READINESS.md`.

Context and constraints:
- Native observation, up axis: Z is up. Gravity and the ground are along Z (ACU-RE-0044). Decoded
  ground heights match live player z to 3.5 mm (ACU-RE-0050). Camera pitch = asin(forward.z), and
  the rest pitch is −3.3° (ACU-RE-0052).
- Native observation, forward: the player matrix row 1 (E+0x30) is forward (ACU-RE-0044). The
  camera's forward is its quaternion applied to local +Y (ACU-RE-0052 obs. 5, 12).
- Native observation, right: with the camera facing compass yaw ψ (atan2(x, y)), the left stick
  pushed right moves Arno along ψ + 89…93° (ACU-RE-0052 obs. 13). With forward +Y, that direction
  is +X.
- Native observation, units: placements match live positions to under 1 mm (ACU-RE-0047). The
  numbers have human scale:
  - camera 3.9 m behind and 1.6 m above the feet;
  - ground-probe rays from +0.5 m;
  - wall-scan rays at +0.75…+1.75 m (ACU-RE-0050);
  - parkour step-ups 0.5–2.4 m (ACU-RE-0055).
- AGENTS.md: explicit coordinate/unit conversions; never guess native axes.
- Today: `acu_bevy` already rotates native data with `Quat::from_rotation_x(-π/2)`
  (`acu-rs/crates/acu_bevy/src/main.rs` `native_z_up_to_bevy_y_up`). `acu_sim` is a synthetic arena
  in a Y-up frame (gravity on `.y`).

Options considered:
1. Keep native coordinates (Z up, metres) in `acu_assets` and `acu_sim`, and convert once at the
   Bevy boundary.
2. Convert everything to Bevy's Y-up at decode time.

Chosen approach and reason: option 1. Oracles, decoders and live comparisons are all in native
coordinates. Converting once at presentation keeps every diff against the original a plain
number comparison and keeps simulation independent of Bevy (AGENTS.md).

Convention (implementation contract):
- Native frame: X right, Y forward, Z up, metres. X × Y = Z, so it is right-handed.
- Native → Bevy: (x, y, z) → (x, z, −y), the existing `rotation_x(−π/2)`. It is a proper rotation
  (determinant +1), so handedness is kept. Native forward +Y becomes Bevy forward −Z, and native up
  +Z becomes Bevy +Y. Quaternions convert by conjugating with the same rotation.
- Headings in data and tests: compass degrees ψ = atan2(x, y), with 0 = +Y and 90 = +X, as in
  `teleport.py`, `camera_state.py` and `data/oracles/camera.json`.
- Quaternions keep the native (x, y, z, w) order.

Consequences / limitations:
- `acu_sim` moves to Z-up native metres when the first non-synthetic slice (M2) starts. The
  synthetic arena may stay Y-up only while it is labelled synthetic; the change gets its own
  tests.
- **Handedness is inferred, not proven.** It relies on the stick's "right" being screen right and on
  the rendered image not being mirrored. Both are normal, but neither was measured against a
  rendered frame.
- Metres are consistent with every observation; no native constant naming the unit has been seen.

Validation:
- `data/oracles/ground_*.json`: z values compare directly with decoded collision.
- `data/oracles/camera.json` `move_right`: early move heading relative to the camera is +77…+93°,
  so +X is right of +Y.
- A future check: render a decoded placement in Bevy next to a game-window capture of the same
  view.

Revisit trigger: a mirrored comparison image, or a native constant or conversion showing
centimetres or another unit.

Native observations and implementation choices are labelled above; the Bevy mapping and the
heading convention are implementation choices.
