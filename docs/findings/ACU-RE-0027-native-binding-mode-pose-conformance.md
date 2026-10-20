# ACU-RE-0027 — Native clip bindings and flagged pose comparison

Date: 2026-10-20. Task ACU-008. State: partially established.

## Provenance

Target: PID 16136, image base `0x140000000`, executable SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`.
Hooks were clear. Runtime analysis view:
`local/native-probe/motion-link-20261020/runtime-view/ACU.runtime-analysis.pe`,
SHA-256 `47a81dc9f29c48a709cbdc1ef60a7e7db9eb8caefaabf30940dd4e1ee755d8c2`.
`verify_mode_runtime.py` compared the live process with this view for each
listed function and constant; all comparisons passed.

IDA Pro MCP analysis of that preserved image (the saved analysis report does not
expose a provider build/version):

- `0x14001BE40` / RVA `0x1BE40` reads the active clip binding rows and dispatches
  each bound source channel.
- `0x140020770` / RVA `0x20770` reads the target channel type and calls the
  quaternion mode helper at `0x1400153B0` for type 6.
- `0x1400153B0` / RVA `0x153B0` is the mode helper. Runtime bytes SHA-256:
  `98b94c1795bc136d9adfdb86039e59ff936b5098edb17c43839b59b100843732`.
- Live-verified constant bytes: `0x142E77EB0` =
  `00000080000000800000000000000000`; `0x142E77EC0` =
  `00000000000000800000008000000000`; `0x1435F2510` =
  `00000080000000000000008000000000`; `0x142E76400` =
  `00000080000000800000008000000080`.

The evaluator, target-channel helper and two blend-related function ranges were
also checked against live bytes. All addresses, lengths and hashes are retained
in `local/native-probe/motion-link-20261020/mode-runtime-guards.json` and the
IDA reports in `runtime-view/`.

## Runtime binding interpretation

The evaluator uses the low 13 bits of each packed binding word as the source
track index and the upper 3 bits as the transform mode. Target channel records
provide a byte destination offset and a type word; the low four type bits equal
6 for the quaternion rows compared here. The byte offset divided by 32 agrees
with the independently captured 421-joint hash table. These observations match
the bounded corpus references in finding0026.

For an XYZW input quaternion `(x,y,z,w)`, the runtime mode helper implements:

| Mode | Result |
| --- | --- |
| 0 | unchanged |
| 1 | `(x,-y,-z,w)` |
| 2 | `(y,x,-w,-z)` |
| 3 | `(-x,y,-z,w)` |
| 4 | `(-w,z,y,-x)` |
| 5 | `(-x,-y,z,w)` |
| 6 | `(-z,w,-x,y)` |
| 7 | unchanged |

The mode-1/3/5 sign masks and mode-2/4/6 shuffle sequences come directly from
the verified instructions. Lane names use the XYZW record order established in
finding0025. The equations are transformations observed in the evaluator; they
do not establish a human-readable “mirror axis” interpretation.

## Crowded route and capture

Run 52 tested a shallow input below the movement deadzone: the root stayed fixed
and the graph remained idle. Run 53 raised the forward input while keeping a
small lateral component; the root moved `0.375` recorded units during the
one-second input and the graph selected walk/avoidance clips.

Run 54 then used six 500 ms leftward ramps with forward input held at 14000,
increasing the horizontal stick from -3500 to -8500, followed by a neutral
observation. The player/root chain moved through the nearby crowd into the open
lane. The per-ramp root translation deltas, in captured transform coordinates,
are `[0.009,0.432,0.049]`, `[0.141,0.847,-0.065]`, `[0.032,1.023,0.050]`,
`[-0.007,0.782,0.025]`, `[-0.057,0.484,0.011]` and `[-0.061,0.279,0.004]`.
No coordinate axis or world-unit conversion is inferred. The six-ramp path
magnitude is 4.264 recorded units. Screenshot:
`local/native-probe/motion-link-20261020/run-54/runner/screenshots/007_observe_stop.png`.

Run 55 captured two short forward/stop passes from that open lane. It recorded
103 root/pose/graph samples, runner exit 0, maximum read span 21.6 ms, and trace
SHA-256 `e954136ba145c6a2cf1962b3fd722ea2131abbca39a8c38ebce767c547348822`.
Each graph sample now includes the three pose-context flags. The local context
at `0x1A4A24230` reached `0x0C000000` six times. The capture is read-only apart
from virtual XInput input; no memory hooks were installed.

## Stage-aligned comparison

The same animation resource ID can have different live binding tables. Run 55
recorded `000000316e56997d` with both a mode-zero binding pointer and a distinct
mode-4/5 binding pointer. The graph leaf's playback bytes at `+56` identify the
binding pointer used for that sample; matching by resource ID alone would pick
the wrong row. The mode-4/5 table contains 21 primary rotation targets: 20 mode
5, one mode 4, and 16 source hashes that differ from target hashes.

Four frames (indices 53, 65, 67 and 68) have local-context flag
`0x0C000000`, the `Rt0` transition clip at weight `0.999689`, and pose read
spans under 22 ms. Direct source-track sampling at recorded playback times,
followed by the runtime-verified per-target mode, matches the captured local
rotations for all 21 bound targets. Frame mean errors are `0.00209`, `0.00228`,
`0.00157` and `0.00244` degrees. A broader five-frame weighted-blend check,
including one mixed transition frame, spans `0.00157`–`0.00581` degrees over
the 21 common targets.

This is a direct-time, 21-joint pose result for one selected transition in one
captured stage. It does not establish all 23 primary targets, the native blend
equation, root motion, translation channels, crowd collision response, or full
locomotion parity. The mode-zero and mode-4/5 tables are distinguished by live
binding pointer. The observed codec-7 avoidance asset remains undecoded.

## Rust adapter and validation

`acu_motion::unity::PoseRig::pose_with_bindings` applies the recovered
target/source map and modes for quaternion channels. It validates bounds,
channel types, duplicate targets and unsupported modes. The existing direct-hash
matcher is not switched to this path because it does not yet load graph-selected
native binding tables. Native root/translation handling remains open.

The ignored install regression reads the machine-local fixture produced from
runs 55 and 56, reopens each exact resource through the Rust decoder, verifies
the payload hash and compares the Rust pose against four walk-transition and
one stop sample. Its mode-zero variant is an independent negative control.
Reproduction:

```powershell
cd acu-rs
$env:ACU_GAME_DIR="D:/Games/Assassin's Creed Unity"
$env:ACU_NATIVE_BINDING_FIXTURE="../local/native-probe/motion-link-20261020/run-55/binding-regression-fixture.json"
cargo test --locked --release -p acu_bevy --bin acu_bevy installed_binding_modes_match_flagged_player_rotations -- --ignored --nocapture
```

The fixture remains machine-local because it contains captured game pose data.
It preserves the executable hash, binding rows, payload hash, exact sample
times, local-context flag, trace hash and expected target rotations.

On this machine the regression passed: four walk-transition frames across 21
joints and one stop frame across all 23 primary joints. The worst frame mean
angular error is `0.003931` degrees. Forcing the same captured source tracks
through mode zero gives mean error `48.629986` degrees across all five frames.
The independent Python capture comparison also passes all five flagged rows
(including one mixed walk-transition row):
the transformed blend median is `0.002281` degrees, while its mode-zero control
median is `49.088443` degrees. The four direct-dominant samples used by Rust
exclude that mixed row.

Focused validation passed:

```text
cargo fmt --all -- --check
cargo clippy --locked -p acu_motion -p acu_bevy --all-targets -- -D warnings
cargo test --locked --release -p acu_motion binding
  2 passed: native_binding_modes_match_runtime_xyzw_lane_operations
            pose_binding_uses_source_index_and_target_hash
cargo test --locked --release -p acu_bevy --bin acu_bevy installed_binding_modes_match_flagged_player_rotations -- --ignored --nocapture
  1 passed; 4 x 21-joint walk samples and 1 x 23-joint stop sample;
  worst frame mean 0.003931 degrees; mode-zero control mean 48.629986 degrees
```

The fixture environment variable was set to the absolute machine path
`E:/ReverseEngineering/Projects/Unity/local/native-probe/motion-link-20261020/run-55/binding-regression-fixture.json`.
Its schema-2 stop case refers to run 56's `00000029a2217bc0`
(`xx_Nav_WalkSlow_Stop_LP`): one full-weight sample at the `0x0c000000`
local-pose stage, all 23 targets, and a `0.001697` degree frame mean. The Rust
mode-zero stop control is `43.366016` degrees.

## Next falsification

Repeat the flagged comparison for the `Rt90` transition; establish how the two
absent transition targets are preserved or blended. Then connect the bound
evaluator to graph selection and compare native root/facing against a Rust
walk/stop trace. Runs 59/62 in the widened street record a 459-joint pair of
live pose contexts (flags `0x80000000`, `0xa0000000`, and `0x88000000`). Run 59's
graph includes generic woman and man navigation resources; its arrays were
excluded from Arno comparisons. In run 62 the user-directed full-scale sprint
circle moves the owner-linked root and activates both pose arrays while Arno is
visibly moving, resolving actor association for that component. Live metadata
now maps 444 of its 459 rows uniquely to the Arno render rig; 15 hashes are
unmatched and two parent links disagree. Run 63 repaired graph-leaf capture by
following the current animation-component controller. It recorded sprint
`000000016737c2c4` and directional sprint `0000000f9f512b7f` leaves with
playback times and weights, plus 37 binding snapshots. The linked pose arrays
pass pose double-read and stable-owner consistency checks in 126/139 frames.
This establishes actor association and usable phase evidence, not yet Rust
locomotion parity: binding semantics and weighted playback need validation,
followed by per-bone comparison on coherent frames. Runs 57/58 were rejected
when the old context chain changed; the reader now follows live contexts and
records per-sample counts. Recover codec 7 before including crowd avoidance.
No full animation or movement parity claim is made.
