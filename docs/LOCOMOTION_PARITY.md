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
  - `loco-turn-delta-1` and `loco-turn-delta-2` (60 Hz);
  - **`loco-stops-2`** (new, 60 Hz): eight full-stick walk stops after hold times of
    1.8–4.25 s, alternating camera-relative forward and back; two half-stick stops; two
    free-run sprints.
    - Launched with `capture_run.py --until world --keep-game --save-pin capture-start-1
      --fps-cap 60`.
    - Sampled by `live_state.py --hz 60` and driven by `controller_autoplay.py --background
      --in-process-pad` (`local/native-probe/loco-stops-1-moves.json`).
    - Closed with `capture_run.py --close` (graceful, 2.1 s); saves put back.
    - A first attempt (`loco-stops-1-failed-scenario`) failed before any input, because
      `capture_run.py --moves` saw no focus shim when the scenario started. The game was
      closed and the saves put back the same way.

  `tools/native_probe/loco_segments.py` turns each capture into dominant-clip segments, ground
  speed and yaw. Only segments with no wall-brace, crowd-avoid or slope clip dominant are used.
- **Rust:** `installed_locomotion_segments` (ignored test in `acu_bevy/src/player.rs`) scripts
  the same inputs from the idle start at 60 Hz. It measures the requested root displacement
  only, so the synthetic arena's obstacles cannot affect it. It asserts every behaviour below
  marked as matching, and lists the known differences. `ACU_LOCO_TRACE=1` prints each graph
  transition with the rule that fired and the entered machines' entry rules.

## Matches (asserted)

| Behaviour | Rust | Native | Evidence |
|---|---|---|---|
| Idle | `IdleLow_Footr`, 0 m/s | `IdleLow_Footr/Footl`, 0 m/s | every capture's first 2–5 s |
| Walk start from idle, stick forward | `PreNavWalkMid_Lt0_tr_Walk (m)`, 1.10 s | `PreNavWalkMid_Lt0/Rt0_tr_Walk`, 0.55–1.12 s (8 starts) | all captures |
| Start latency | 2 ticks (0.033 s) | ≤ 1 sample (0.033 s) after MovementSpeed > 0 | `params2` + `live-state2` |
| Full-stick walk loop and speed | `Walk_3-Loop`, 2.35 m/s | `Walk_3-Loop`, 1.99–2.28 m/s per segment, 2.16–2.36 m/s in 0.5 s windows (`loco-stops-2`); 1.75–1.99 m/s (`loco-turn-delta-1`) | native TimeScale varies 0.9–1.3 |
| Half-stick walk loop and speed | `WalkSlow_3-Loop`, 0.75 m/s | `WalkSlow_3-Loop`, 0.72–0.87 m/s (0.75–0.76 per segment in `loco-stops-2`) | `live-state2`, `loco-stops-2` |
| Back to idle after a walk stop | `IdleLow_Footr` | `IdleLow_Foot*` | — |
| Free run from idle to sprint | 1.65 s to `Sprint_3-Loop_V02` | 1.00–1.82 s (3 runs) | `loco-params-1` ×2, `loco-turn-delta-2` |
| Sprint stop clip, length and travel | `SprintStop_Fd100cm`, 0.78 s, 0.73 m | same clip, 0.74–0.78 s, 0.60–0.81 m (3 stops) | `live-state`, `loco-turn-delta-2`, `loco-stops-2` |
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

## Walk stop variant (fixed, ACU-RE-0074)

- Native picked `WalkStop_RC` in 13 of 13 stops:
  - 5 in the older captures, at `Walk_3-Loop` times 0.23–0.73 s;
  - 8 in `loco-stops-2`, after hold times stepped by 0.35 s across a full 2.8 s loop.
- Rust used to pick from graph state 3723 by phase (RP and RC alternating every ~0.24 s), because
  it handed the outgoing state's sync position to SelectBestMatchingTags.
- Native code passes no sync source to a state entered by a transition (`sub_140109CB0` →
  `sub_1400B7970`, context `+16` = 0). Without a source, the select keeps port 2: `WalkStop_RC`.
- Rust now does the same. `installed_walk_stop_sweep` releases after holds of 1.80–2.95 s
  (0.05 s steps, more than one walk cycle) and gets `WalkStop_RC` in 24 of 24.
- `AtomGraphConditionNode` is now latched at activation (`sub_1400D6D00` / `sub_1400EB6B0`), so
  the stop no longer flips branch mid-clip.
- Damp nodes now approach their target exponentially (k ≈ 20.6/s at rate 5, an inference from
  live blend weights). Before, they passed the target straight through.
- Replay agreement: 675/1051 (674 before).

## Differences (listed by the test, not fixed)

1. **Walk stop travel.** Rust's RC stop travels 0.27 m against native 0.28–0.45 m (borderline).
2. **Half-stick stop.** Rust plays `WalkStop_RC`. Native played `WalkSlow_Stop_LP` once and
   `WalkSlow_Stop_LC` twice (3 of 3), each travelling 0.17 m. `loco-sprint-open-1` added two
   more `WalkSlow_Stop_LP` (two further half-stick walks ended at a ledge,
   `PreNavWalkStopOnEdge`). That makes LP 3 and LC 2 of 5: native picks by phase here.
   - The slow branch of 3723 needs its condition (event `0x00400026`) to hold at activation.
   - Native evaluates that condition once, over the events of the context's sync source.
   - With the null source of a transition entry, the condition is false, so Rust takes the
     full-walk branch.
   - Native must hand 3723 a source in this case. Which path does it is not found
     (ACU-RE-0074, Unknowns).
3. **Sprint speed.** Rust's `Sprint_3-Loop_V02` moves at 6.26–6.39 m/s, the clip's own root
   speed (about MovementSpeed 6.277).
   - Native, on a clear 1.59 s segment of `loco-stops-2`: 5.96 m/s (0.5 s windows 5.45–6.17).
     Rust is about 7 % faster.
   - Older captures gave 5.26 and 5.79–5.95 m/s, with blending.
   - Checked (ACU-RE-0074): Rust and native both sprint in graph 3381 (machine 3370 → 3379).
     It blends `Sprint_3-Loop_V02` with `Run_3-Loop` by damped MovementSpeed (thresholds 3.5
     and 7).
     - Native sprint-side weight is about 0.76, run side 0.18; Rust has 0.79 / 0.21.
     - The sync blend makes the sprint clip play at 0.88–0.90× natively (run clip 1.24–1.28×);
       Rust gives 0.88–0.95× (run 1.19–1.47×). The native TimeScale parameter stays 1.0.
     - Native sprints ran near walls (`WallDistanceY` median 4.5 m, down to 0.57 m; below 4 m
       graph 3381 brings in wall-anticipate clips), with slope −5° to +8° and nonzero
       `TargetDistanceX`. Rust's arena is flat and open.
   - New capture `loco-sprint-open-1` (2026-11-16, 60 Hz with parameters at 30 Hz). The save
     started at a different spot (605.7, −28.9), and neither sprint was clear:
     - the first ran into a wall;
     - the second stumbled, then held `Sprint_3-Loop_V02` for 1.6 s at 5.66 m/s, uphill
       (`VirtualFootIkSlope` median 2.77°) and in a crowd (`InDenseCrowd` 41 of 48 samples).
   - Like for like (`installed_sprint_slope_speed`, slope 2.77°), the Rust blend matches that
     window:
     - sprint 0.72 / run 0.19 / uphill sprint 0.07 / uphill run 0.02, against native 0.70 /
       0.16 / 0.08 / 0.02;
     - sprint clip at 0.87× against native 0.88×.
   - Rust still moves at 6.05 m/s against native 5.66 m/s, about 7 % faster.
   - Native body velocity is the per-frame root delta (ACU-RE-0057). So either native root
     motion for this blend is shorter, or crowd contact slows the body.
   - Next: a `root_delta_capture.py` run to compare the native per-frame root delta with
     Rust's.
4. **After a sprint stop.**
   - Native plays `IdleHigh_tr_IdleLow_footl/footr` (1.09–1.1 s) in 3 of 3 runs; Rust goes
     straight to `IdleLow_Footr`.
   - In NewDemo that clip lives only under state 8947 (states 8951, 8952 and 8954). No machine
     reachable from the navigation hub (3296) contains it.
   - The interpreter is rooted at the hub, so it cannot play this clip. Native reaches it
     through a machine above or beside the hub, which is not decoded.
   - Checked (ACU-RE-0074): state 8947 has the shape of the root layering state's masked
     overlay layers (for example 6739, sheathe/unsheathe):
     - state 0 (8948) is a non-graph placeholder;
     - state 1 (8949) plays the transition clips and exits to target 65534.
   - No layering state the interpreter parses lists 8947, and no transition carries a graph
     index (all transitions have only f16 node id, f32, f36 target and f38 bits). How native
     activates it is unknown.
5. **Minor, not asserted:**
   - On release, Rust shows `WalkSlow_3-Loop` dominant for 0.10 s (hub idle/stop state 5,
     which leaves after 0.05 s) before the stop clip; native keeps `Walk_3-Loop` dominant
     until the stop.
   - Rust enters the stop about 0.1 s after release; native about 0.2 s after MovementSpeed
     reaches 0. The replay `WalkStop_RC` time error is 0.16 s.

## Unverified

- **Native variation.** All samples come from the pinned save's street, 2–13 per behaviour.
  One forward free run in `loco-stops-2` stumbled into a crowd and ended in `JogStop_Fd40cm`, so
  only one clean native sprint exists.
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
python tools/native_probe/loco_segments.py local/native-probe/runs/loco-stops-2/live-state.jsonl --timeline local/native-probe/runs/loco-stops-2/scenario/input-timeline.jsonl --windows 0.5
```
