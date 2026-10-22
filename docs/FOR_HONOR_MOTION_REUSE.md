# For Honor motion matcher reuse

> **Status (2026-10-22): scaffolding, not ACU behaviour.** ACU does not use For Honor-style
> motion matching per the maintainer. These modules drive the current viewer only; do not
> extend them. Retirement is tracked in issue #99.


User-directed reuse, 2026-10-19. Source revision `e77a747fd88b81a185e438039fd5fe3c41b41725`.
Read-only source: `E:/ReverseEngineering/Projects/ForHonorRustRewrite/fh-rs/crates/fh_combat/src/`.

The seven modules were imported byte-for-byte, then formatted for this workspace's
Rust 2024 toolchain; original headers and algorithm bodies are retained. Narrow
module-level Clippy allowances retain the reviewed call interfaces, indexed loop,
sample alignment check and nested search conditions without a cosmetic refactor.
Their native
addresses, image identities, codec/sample layouts, numeric settings and tests
refer to For Honor only. They do not establish Unity behavior or compatibility.
The modules contain the matcher, cost/search/query/request and smoothing kernels;
they have no Bevy, install or native-math dependencies. No live sibling path dependency.
Existing notices are preserved; no project distribution license is assigned.

| Module | Source SHA-256 |
| --- | --- |
| `amo.rs` | `7fa599d6c7ec268dbde9de0a3f9f79d131ae8f0261e96f88b8e4441f1c15163c` |
| `amo_prediction.rs` | `eefb4f7196fbe1f19c45f09789850b1edb1ff5a3fbe21395cc64c92d4923654c` |
| `amo_request.rs` | `cfe9f6ebe19b5b229dd41af730c9ec3eff9a7688c2455888068d0a4d5e26aafd` |
| `amo_samples.rs` | `09d6f67583afc5029fef911f0bbd62f7ef42c2164957afad284b5e58b4a3a3c5` |
| `amo_search.rs` | `399ccd9173d3e2611be175cb0c7ce44c0d134e8a47140952611b9c06632fc7d6` |
| `amo_velocity_smoother.rs` | `7d87d884e3cc3fdb3f524de031214e7d2c7816ac772a00e611e3357d9b015726` |
| `loco_match.rs` | `bd8bd74c1fd17ee552e0efaea0c86a831af465603cd7254c3dec30673be52d69` |

Integration gate: decode original Unity clips with explicit track-to-bone mapping,
timing and root channels; build features in an explicitly documented frame; drive
both pose and root from one selected clip/time and blend transitions. Do not feed
arbitrary Unity bytes into the For Honor 0x150-byte sample format. Any generated
features/settings are adapter choices until Unity evidence establishes them.

Imported matcher tests validate the preserved For Honor reconstruction algorithms.
Unity decoding, playable motion quality and clothing selection require separate gates.

The isolated test-only `fh_root_test.rs` retains `motion.rs::to_world` to support
one imported matcher test. Whole original `motion.rs` SHA-256:
`25d886806168be775b3205b8614b43876d51959a9d885ebe7550a3cd05e78e5e`.
This helper supplies no runtime Unity axis conversion. The initial isolated port
passes 44 tests.

The Unity adapter in `acu_motion/src/unity.rs` now uses both imported
`amo::primary_pose_cost` and `amo_prediction::prediction_geometry_cost`. Unity
joint features and original navigation-root future trajectories are cached at
30 Hz; no Unity bytes enter FH packed sample decoding. Requests stay relative
to actor facing. Search runs within a profile on direction changes and at 10 Hz;
continuing a take retains its phase. Switch cost is 0.12; pose blend is 160 ms.
Prediction horizons are 0.05/0.5/1.1 seconds, with position/velocity weights
1/0.5 and zero auxiliary/heading weights because this bank has identity root
rotation. These settings, translation-loop extension and gradual facing rates
(180/270 degrees/s for walk/sprint) are explicit adapter choices.

`fh_bevy/src/install_loco.rs::continuity_preserves_eight_direction_steering`
provides the request/facing separation reference. The Unity player consumes
selected root distance, warps residual direction to requested travel and turns
facing independently; it does not overwrite facing from velocity each tick.
Original lateral/backward walk and lateral sprint vectors are documented in
[finding0019](findings/ACU-RE-0019-directional-navigation.md).
Native Unity graph rules, root-yaw turns and actor ownership remain unresolved.

Cloth reference reviewed read-only: sibling `fh_bevy/src/cloth.rs`,
`cloth/kernels.rs`, `cloth_runtime.rs`, `fh_assets/src/cloth_def.rs` and
`docs/re/CLOTH_SOLVER.md`. This gives an authored-operation particle solver,
skinned-target preparation, bone-linked collision deflectors and render mapping.
No Unity cloth bytes are passed to it. Unity op/record schemas and garment
bindings are the next compatibility gate, rather than inventing coat dynamics.

Read-only directional/cloth reference source identities (2026-10-19):

| File | SHA-256 |
| --- | --- |
| `fh-rs/crates/fh_bevy/src/install_loco.rs` | `d2d628317e574bcd956717b9509a4711c7d9e46fa12892fcd718eab1d8d48253` |
| `fh-rs/crates/fh_bevy/src/cloth.rs` | `60c06b68ca7a558266affe55b09b1278482b36274b72362b3a77c26e34852e58` |
| `fh-rs/crates/fh_bevy/src/cloth/kernels.rs` | `faef9df052366c83f8058c824212136cdbf0fbb3a63002ac1153015a4c383afd` |
| `fh-rs/crates/fh_bevy/src/cloth_runtime.rs` | `5d2ba7f52f14788e124ed3c0b55ff3210a72607def4068c8bf71de0aeb9e321b` |
| `fh-rs/crates/fh_assets/src/cloth_def.rs` | `74c49009460be225161cce26436024243d526e000319bd554bbe93d2eb759b67` |
| `docs/re/CLOTH_SOLVER.md` | `1d2436fa2a0e93c2f92678284159772f466fbbcb4c4d78377704aa1339039e53` |
