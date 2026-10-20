# ACU-RE-0029 - Four-cycle direction reversal review

Date: 2026-10-20. State: observed gaps; motion controller unresolved.
User acceptance case: right two seconds, left two seconds, repeated four times.

## Provenance

- Installed ACU.exe SHA-256:
  `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`.
  Live PID 24480, image base `0x140000000`.
- Animation component exit VA `0x1400e9cc0`, RVA `0xe9cc0`; byte guard
  `40534883ec30488bd9488b89d0000000`. No corpus identity assumption.
- Project acu_tap13 provider, game-thread graph/pose snapshots, filtered to
  animation component `0x1e684c790`, pose component `0x1cf1c2cf0`.
  Owner matrix, 459-joint local/model pose arrays and graph leaves captured on
  the same exit. Hook removed in `finally`; saved status has no armed hooks.
- Dump `local/native-probe/ev-reversals-20261020-01.bin`, SHA-256
  `dce438865fbc1ac578b1554a3c4c0cc121933871cb91b2aec7a9ed96a7dd8f73`.
  Metadata `local/native-probe/reversals-20261020-01-meta.json`.
- Input profile `local/native-probe/reversals-20261020-01-profile.json`;
  measured step schedule in `local/native-probe/run-reversals-20261020-01/input-timeline.jsonl`.
  Full stick X +/-32767; neutral one-second bookends, four two-second pairs.
  Virtual XInput slot 3; input runner returned zero with empty stderr.
- Asset-name provenance: existing local audit
  `local/locomotion-audit-20261019/locomotion-candidates.csv`, DataPC.forge,
  entry 928, resources 4639 and 4641. Their payload hashes are respectively
  `9e6da73c8a4fb29e6f5e378e9eca1c984e345225c9dbae8f28d3eec1dd407f8e`
  and `56a5e5d5d9da27f1f58b8afd8796c06592f3515302ade30b4b0bbcc4f9fe5480`.
  Names are archive labels; the graph's use of those IDs is a live observation.

## Results

4,238 events / 2,119 pose exits over 18.42 seconds; zero dropped events.
At weight >0.01, graph leaves include:

| File ID | Archive name | Frames present |
| --- | --- | ---: |
| `0000000fc08fd67a` | xx_Nav_Walk_3-Loop | 1531 |
| `00000008059484b7` | xx_Nav_WalkFwd_PlantAndTurnLt180_Improper | 701 |
| `00000008059484b9` | xx_Nav_WalkFwd_PlantAndTurnLt180_ProperRP | 92 |
| `00000014dcafc8d8` | xx_Nav_WalkStop_RC | 110 |

Up to five leaves are active together. Presence counts do not establish which
layer owns facing, nor whether both turn clips are required for every reversal.
The capture retains playback bytes for the subsequent phase/binding analysis.

Source inspection: `acu_motion/src/unity.rs` uses eight authored adapter takes,
requires identity navigation-root rotation, and blends from a frozen previous
pose over 160 ms. `acu_bevy/src/player.rs` turns actor facing with an authored
cap. Therefore this viewer cannot currently reproduce the observed native turn
transition. A frozen crossfade may contribute to the reported morphing; its
individual contribution has not been isolated.

## Reproduction and next gate

```powershell
python tools/native_tap/reversal_capture.py 24480 1e684c790 reversals-20261020-01
```

Pointers are session-specific: reidentify the player component before another
run. Do not overwrite the retained capture. Foreground verification must pass
before input is sent. The current snapshot location includes nearby pedestrians
and slope changes; camera-relative native input and world-axis synthetic input
are not calibrated. This is not a root trajectory parity gate.

Visible rewrite: `tools/run_arno.ps1 -Scenario reversals`. Optional environment
variable `ACU_REVERSAL_TRACE` records poses and selected clips directly to disk.
`tools/native_tap/reversal_view.py` makes an offline animated skeleton/clip
comparison with a scrubber; current output
`outputs/parity/reversals-native-rust.html` contains 180 display samples and
2,070 native frames inside its aligned input interval. Each skeleton retains its
own hierarchy; there is no asserted joint correspondence or numeric pose gate.

Next: recover turning-root extraction, phase selection and blend/facing ownership
from these graph leaves and controller state; then implement and visually gate
the same four-cycle sequence. Preserve the coat subsolver work recorded in
ACU-RE-0028 while prioritizing this user-visible issue.

## Upper-body stage exclusion (later 2026-10-20)

User explicitly requires the same behavior and reports stiff upper-body shifts;
an approximate adapter is not acceptance. A second run adds a player-pose-filtered
entry/exit hook at modifier evaluator `0x1400377e0` (RVA `0x377e0`). Metadata and
guard: `local/native-probe/reversals-stages-20261020-02-meta.json`; dump and SHA
are recorded there. Hook cleanup succeeded, zero dropped events.

`reversal_stage_review.py` pairs entry/exit by hook and thread: 2,291 modifier
pairs, zero unmatched. For tracked low-index primary joints 2,3,4,5,12,13,14,
both local and model quaternion changes are zero throughout this run. Other
joints change substantially, confirming the stage executes. This **excludes
this modifier stage** as the source of those joints' upper-body rotational shift
in this capture; it does not locate the earlier writer or exclude translation.

The initial attempt to pair this stage inside component-update calls found no
nested pairs. These jobs are not nested in the captured order; do not pair a
nearby final component pose with a modifier frame by time alone. Saved report:
`local/native-probe/reversals-stages-20261020-02-review.json`. Next inspect earlier
graph playback/blend and root/turn extraction with coherent stage ownership.

## External lead supplied by user

Repository: https://github.com/Banned445/AC1-Movement-Rewritten , inspected at
commit `9a228cb135a24279d6eae155a8fb2dd9c83c02bd`. Source-only local snapshot:
`local/references/ac1-movement-9a228cb135a2/manifest.json` records exact URLs,
lengths and SHA-256 per file; no external code executed or imported into Rust.

Its `src/player/move_blend.rs:1-8,130-174` implements action-graph locomotion
weights from speed, hip lean and bank parameters. `ground_tree.rs:1-8,62-67`
describes authored transitions and animation-driven skid turns. `src/anim.rs`
tracks outgoing/incoming action playback and displacement modes. These are
AC1 implementation claims, not evidence of ACU's architecture or constants.

Use them as search questions: does ACU's observed graph generate locomotion
weights from comparable heading/speed/lean parameters, with dedicated turn
transitions? No evidence here establishes that ACU uses motion matching.
Do not transplant the AC1 offsets, blend constants or 17-clip layout.
