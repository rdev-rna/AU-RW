# ACU-RE-0072 — Flat-ground locomotion: Rust graph driver against live captures

Date: 2026-11-15. Related: [ACU-RE-0067](ACU-RE-0067-live-locomotion-parameters-and-blend-rules.md)
(live parameters), [ACU-RE-0069](ACU-RE-0069-atomgraph-evaluation-sync-and-mirroring.md) (graph
evaluation, walking turn rate), [ACU-RE-0057](ACU-RE-0057-body-velocity-from-root-motion.md) (root
motion drives the body). Report: [LOCOMOTION_PARITY.md](../LOCOMOTION_PARITY.md).

State: **partially established**.
- **Observed:** the native measurements below, from existing live captures, and the Rust
  results from the asserting test.
- **Observed in data:** the reset defect and its cause (NewDemo rules, Rust trace).
- **Inference:** the causes of the remaining differences.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, image base
  `0x140000000`. No new binary analysis in this finding; VAs are cited from ACU-RE-0069.
- NewDemo AtomGraph: `DataPC.forge` entry 1518, file id `000000013d5bc4d8` (ACU-RE-0069). Clips:
  `DataPC.forge` entry 928 (`xx_Nav_Walk_3-Loop` `0000000fc08fd67a`, duration 2.8 s;
  `xx_Nav_WalkStop_RC` `00000014dcafc8d8`; `xx_Nav_WalkStop_RP` `00000010322cae8f`; ids from
  the asset index, `crc-name`).
- Live captures (local only, ignored; SHA-256 prefix):
  - `loco-params-1/live-state.jsonl` `f37d9d1d…` and `live-state2.jsonl` `17f8dafc…`;
  - `loco-params-1/params2.jsonl` `0b387926…`, QPC-aligned with `live-state2`;
  - `loco-turns-1/live-state.jsonl` `692a628c…`;
  - `loco-turn-delta-1/live-state.jsonl` `e9b41ca8…`;
  - `loco-turn-delta-2/live-run.jsonl` `301607ea…`.

  Each was launched with `capture_run.py … --fps-cap 60` and closed with `--close`
  (ACU-RE-0067, ACU-RE-0069 obs. 13).
- Tools:
  - `tools/native_probe/loco_segments.py`: segments, speeds and yaw from a capture;
  - Rust tests in `acu-rs/crates/acu_bevy/src/player.rs`: `installed_locomotion_segments`,
    `installed_graph_dump`, and the `ACU_LOCO_TRACE` trace;
  - Rust tests in `acu-rs/crates/acu_motion/src/atom_graph.rs`: the forced-start tests.

## Observations

1. **Native flat-ground values** (open-ground segments only):
   - Idle `IdleLow_Footr/Footl` at 0 m/s.
   - Walk start `PreNavWalkMid_Lt0/Rt0_tr_Walk` dominant for 0.60–1.12 s (7 starts).
   - Full-stick `Walk_3-Loop` at 1.75–1.99 m/s (`loco-turn-delta-1`, 0.5 s windows) and
     2.22–2.31 m/s (`loco-params-1`).
   - Half-stick `WalkSlow_3-Loop` at 0.76–0.87 m/s.
   - Walk stop `WalkStop_RC` in 5 of 5 stops (walk clip time at the stop 0.226, 0.41, 0.552,
     0.673, 0.734 s), with 0.39–0.45 m of travel.
   - Free run reaches `Sprint_3-Loop_V02` 1.00–1.82 s after input. Sprint 5.26 m/s (60 Hz) and
     5.79–5.95 m/s (30 Hz, blended).
   - `SprintStop_Fd100cm` lasts 0.74–0.78 s and travels 0.60–0.81 m. It is followed by
     `IdleHigh_tr_IdleLow_footl` (2 of 2).
   - Walking turn: 2.14–2.15° per 60 Hz frame.
   - Walking reversal: `WalkFwd_PlantAndTurnLt180_ProperRP` (0.52 s, 175.5°) or `_Improper`
     (0.78 s, 177.5°).
2. **Reset defect (fixed).**
   - Hub state 1 (walk machine 3320), rule 3, goes to hub state 4 (idle/stop machine 3677) when
     `Parkour == 0` and events `0x0040000b` and `0x00400034` are both absent (mode 2). It has a
     second branch for Parkour 1 with |TargetAngle| ≥ 110.
   - The interpreter's rules read the previous step's event list. A forced `start_in` left that
     list empty, so the first step took this rule. The stop machine then entered the
     sprint-stop state 3679 and played `SprintStop_Fd100cm` (mirrored), moving 0.68 m.
   - On later resets the stale list from the previous run decided the outcome.
3. **Walk stop selection in Rust** (sweep of the release time, 0.05 s steps):
   - Graph state 3723 holds `WalkStop_RP/RC` and `WalkSlow_Stop_LP/LC`.
   - From `Walk_3-Loop` times 0.03–1.11 s it picks RP(m) / RC(m) / RP / RC / RP(m) in bands of
     about 0.24 s.
   - At the five native release times it would pick RC in about one of five.
4. **`IdleHigh_tr_IdleLow_footl` lies outside the hub.**
   - It appears only in graph states 8951, 8952 and 8954, under machines 8950 / 8953 /
     8949 / 8947.
   - State 8947 has no parent machine. No state reachable from the navigation hub
     (payload 9,974,731, state 3296) holds the clip.
5. **Rust sprint root speed.** `Sprint_3-Loop_V02` moves the Rust body at 6.26–6.39 m/s, the
   clip's own root motion at TimeScale 1. That is about MovementSpeed 6.277.

## Inferences

- Walk stop: the native choice does not alternate with phase as Rust's does. Two places could
  differ: the SelectBestMatchingTags cost and threshold (`0x1400ECD90` / `0x1400CDE10`, threshold
  `f32::MAX` is a CHOICE in ACU-RE-0069), or the sync position handed to state 3723 at entry.
- After a sprint stop, the native high-to-low idle comes from a machine above or beside the hub;
  the Rust interpreter's root is the hub.
- Sprint: the native sprint speed is lower than the clip's root speed. A TimeScale below 1 or
  blending would explain it; neither is measured on clean ground.

## Unknowns

- Native walk-stop choice over a controlled set of phases; native open-ground sprint speed.
- The machine that owns state 8947 and how native moves between it and the hub.
- Damp-node law (could pick `WalkSlow_Stop` after a half-stick walk); TimeScale source.

## Independent comparison

- `installed_locomotion_segments` asserts 16 behaviours against the native bands and lists 5
  known differences. Any other difference fails.
- Live replay (`atom_graph_replay`, `loco-params-1`, measured parameters): dominant-clip
  agreement 674/1051 with the fix and `IDLE_START`, against 649 before. Median time error
  0.089 s. IdleLow keeps a constant 8.46 s phase offset, because the capture starts inside an
  idle cycle.
- The forced-start unit tests use a two-state synthetic graph whose expected path follows from
  the rule semantics alone. Both fail without the fix.

## Reproduction

See [LOCOMOTION_PARITY.md](../LOCOMOTION_PARITY.md#reproduction).

## Implementation implications

- `atom_graph::Player::start_in` primes the forced states' events, restarts the clock and
  resets the seed (CHOICE, labelled in code).
- `graph_driver::IDLE_START = [1, 5, 1]` (`IdleLow_Footr`).
- Next targets:
  - SelectBestMatchingTags cost and the stop handoff (difference 1);
  - the root above the hub (difference 4);
  - a capture with repeated stops and a clear sprint line.
