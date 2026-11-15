# Ground locomotion parity (flat ground)

Date: 2026-11-15. Finding: [ACU-RE-0072](findings/ACU-RE-0072-flat-ground-locomotion-parity.md).
Scope: idle, walk, half-stick walk, free-run sprint, starts and stops, the walking turn and the
walking reversal on flat open ground. The Rust side is the native NewDemo graph through
`acu_motion::graph_driver` with root-delta body motion in `acu_bevy/src/player.rs`.

## How it was compared

- **Native:** the live captures already on disk (`local/native-probe/runs/`, all capped at
  60 fps and closed with `capture_run.py --close`):
  - `loco-params-1` (two runs of the same scenario; `live-state2` aligned by QPC with `params2`);
  - `loco-turns-1`;
  - `loco-turn-delta-1` and `loco-turn-delta-2` (60 Hz).

  `tools/native_probe/loco_segments.py` turns each capture into dominant-clip segments, ground
  speed and yaw. Only segments with no wall-brace, crowd-avoid or slope clip dominant are used.
  No new capture was taken (see "Unverified").
- **Rust:** `installed_locomotion_segments` (ignored test in `acu_bevy/src/player.rs`) scripts
  the same inputs from the idle start at 60 Hz. It measures the requested root displacement
  only, so the synthetic arena's obstacles cannot affect it. It asserts every behaviour below
  marked as matching, and lists the known differences. `ACU_LOCO_TRACE=1` prints each graph
  transition with the rule that fired and the entered machines' entry rules.

## Matches (asserted)

| Behaviour | Rust | Native | Evidence |
|---|---|---|---|
| Idle | `IdleLow_Footr`, 0 m/s | `IdleLow_Footr/Footl`, 0 m/s | every capture's first 2–5 s |
| Walk start from idle, stick forward | `PreNavWalkMid_Lt0_tr_Walk (m)`, 1.10 s | `PreNavWalkMid_Lt0/Rt0_tr_Walk`, 0.60–1.12 s (7 starts) | all four captures |
| Start latency | 2 ticks (0.033 s) | ≤ 1 sample (0.033 s) after MovementSpeed > 0 | `params2` + `live-state2` |
| Full-stick walk loop and speed | `Walk_3-Loop`, 2.35 m/s | `Walk_3-Loop`, 1.75–1.99 m/s (`loco-turn-delta-1`), 2.22–2.31 m/s (`loco-params-1`) | Rust is at the top of the native range; native TimeScale varies 0.9–1.3 |
| Half-stick walk loop and speed | `WalkSlow_3-Loop`, 0.75 m/s | `WalkSlow_3-Loop`, 0.76–0.87 m/s at MovementSpeed 0.621 | `live-state2` windows |
| Back to idle after a walk stop | `IdleLow_Footr` | `IdleLow_Foot*` | — |
| Free run from idle to sprint | 1.65 s to `Sprint_3-Loop_V02` | 1.00–1.82 s (3 runs) | `loco-params-1` ×2, `loco-turn-delta-2` |
| Sprint stop clip, length and travel | `SprintStop_Fd100cm`, 0.78 s, 0.73 m | same clip, 0.74–0.78 s, 0.60–0.81 m | `live-state`, `loco-turn-delta-2` |
| Walking turn, stick 90° right | −2.15° per frame, 90° in 0.7 s, inside `Walk_3-Loop` with no turn clip | 2.14–2.15° per frame, identity root rotation | `loco-turn-delta-1` (ACU-RE-0069 obs. 13) |
| Stick reversed while walking | `WalkFwd_PlantAndTurnLt180_ProperRC (m)`, 0.68 s, 180° | `…_ProperRP` 0.52 s / 175.5°, `…_Improper` 0.78 s / 177.5° | `loco-params-1` ×2 |
| Live parameter replay (graph only) | 674/1051 dominant-clip agreement | — | `atom_graph_replay` (was 649 before the fix below) |

## Fixed in this pass

**A reset played a sprint stop and moved the body with no input.** The hub's walk → idle/stop
rule (rule 3 of hub state 1) fires when Parkour is 0 and the idle events `0x0040000b` and
`0x00400034` are both absent. Rules read the previous step's event list.

- After `start_in`, that list was empty, so a fresh driver left the forced idle on its first
  step. It played `SprintStop_Fd100cm` (mirrored) and drifted 0.68 m.
- Later resets kept the previous run's events, so whether this happened depended on what had
  played before.

Fix (`atom_graph::Player::start_in`): restart the clock and the seed, then run a zero-length
graph pass so the forced states' own events exist before the first step. The forced start is
now `graph_driver::IDLE_START` = `[1, 5, 1]`, which is `IdleLow_Footr`, the clip every capture
starts in. Before, `[1, 5]` entered the idle machine's entry pose `IdleLow-FullLookDnLedge`.
Tests: `forced_start_holds_a_self_sustaining_state` and `reset_does_not_depend_on_the_previous_run`
(synthetic graph; both fail without the fix).

## Differences (listed by the test, not fixed)

1. **Walk stop variant.**
   - Native picked `WalkStop_RC` in 5 of 5 stops, at `Walk_3-Loop` times 0.23, 0.41, 0.55,
     0.67 and 0.73 s.
   - Rust picks from graph state 3723 by phase: RP and RC alternate every ~0.24 s of the 2.8 s
     loop. At those native times it would pick RP in 3–4 of 5.
   - The RP stop travels 0.64 m against native 0.39–0.45 m. Rust's RC stop travels 0.40 m,
     which matches.
   - Likely cause: SelectBestMatchingTags (`0x1400ECD90`, cost `0x1400CDE10`). Rust uses a
     `f32::MAX` cost threshold (a CHOICE in ACU-RE-0069), or the sync handoff into the stop.
     Needs binary work.
2. **Half-stick stop.** Rust plays `WalkStop_RC`; native played `WalkSlow_Stop_LP` (one stop).
   The two share state 3723. Damp nodes are passed through in Rust, so MovementSpeed reaches 0
   at once. This is a possible cause, not checked.
3. **Sprint speed.** Rust's `Sprint_3-Loop_V02` moves at 6.26–6.39 m/s, which is the clip's own
   root speed and equals MovementSpeed 6.277. Native measured 5.26 m/s (60 Hz, 0.76 s) and
   5.79–5.95 m/s (30 Hz windows, the sprint clip blended at weight 0.4–0.7 with avoid clips),
   so Rust is 7–20 % faster. No clean open-ground native sprint exists.
4. **After a sprint stop.**
   - Native plays `IdleHigh_tr_IdleLow_footl` (1.1 s) in 2 of 2 runs; Rust goes straight to
     `IdleLow_Footr`.
   - In NewDemo that clip lives only under state 8947 (states 8951, 8952 and 8954). No machine
     reachable from the navigation hub (3296) contains it.
   - The interpreter is rooted at the hub, so it cannot play this clip. Native reaches it
     through a machine above or beside the hub, which is not decoded.
5. **Minor, not asserted:**
   - On release, Rust shows `WalkSlow_3-Loop` dominant for 0.10 s (hub idle/stop state 5,
     which leaves after 0.05 s) before the stop clip; native keeps `Walk_3-Loop` dominant
     until the stop.
   - Rust enters the stop about 0.1 s after release; native about 0.2 s after MovementSpeed
     reaches 0. The replay `WalkStop_RC` time error is 0.16 s.

## Unverified

- **Native variation.** The native bands come from 2–7 samples per behaviour, all in the
  pinned save's street. A sprint there runs into walls within about 5 m. No capture has a
  clean open-ground sprint or several stops at controlled phases. A new capture (repeated walk
  stops at varied hold times; a sprint along a clear line) would settle differences 1 and 3.
  It was not run in this pass.
- **Bank and avoid clips during turns.** Native blends `WalkFwd_AvoidRight/Left`,
  `Walk_3-Loop_BankLeft` and crowd-avoid clips while turning. Rust stays in `Walk_3-Loop`
  because `DiffAngleToTarget` and `CharacterTurnAngle` are not computed and damp nodes pass
  through. With no wall or crowd, it is not known whether the bank clips would play natively.
- **Free-run turn law.** The exponential approach plus clip turns of `loco-turn-delta-2` is not
  implemented. Rust free run turns by clip root motion only.
- **Walk start variants by stick angle:**
  - From idle, Rust plays `PreNavWalkMid_Lt90 (m)` for a right stick, `Rt90 (m)` for a left
    stick and `Lt180 (m)` for a back stick.
  - Native played `Lt180` from behind and a `Rt90` that turned left (so presumably mirrored).
  - Mirror flags are not in the live capture.
- **TimeScale.** Native TimeScale varies 0.7–1.3 within a walk; Rust uses 1.0. Its source is
  unknown (ACU-RE-0067).
- **Turning while walking at half stick.** Rust uses the full-stick rate; there is no native
  measurement.

## Reproduction

From `acu-rs/`:

```
ACU_GAME_DIR=<install> cargo test --locked --release -p acu_bevy -- --ignored installed_locomotion_segments --nocapture
ACU_LOCO_TRACE=1 ACU_GAME_DIR=<install> cargo test --locked --release -p acu_bevy -- --ignored installed_locomotion_segments --nocapture
ACU_DUMP=3677 ACU_FIND=IdleHigh_tr_IdleLow ACU_GAME_DIR=<install> cargo test --locked --release -p acu_bevy -- --ignored installed_graph_dump --nocapture
ACU_NEWDEMO=$PWD/../local/atomgraph/NewDemo.bin ACU_GAME_DIR=<install> ACU_LIVE_RUN=$PWD/../local/native-probe/runs/loco-params-1 cargo test --locked --release -p acu_motion --test atom_graph_replay -- --ignored --nocapture
```

From the project root:

```
python tools/native_probe/loco_segments.py local/native-probe/runs/loco-params-1/live-state2.jsonl --windows 0.5
python tools/native_probe/loco_segments.py local/native-probe/runs/loco-turn-delta-1/live-state.jsonl --windows 0.5
```
