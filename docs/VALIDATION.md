# Validation

## Native player idle regression (ACU-RE-0025)

Checkpoint gates (2026-10-20): fmt, Clippy, **115 workspace tests passed with
20 explicitly ignored**, workspace build passed. Logs:
`local/native-probe/motion-link-20261020/workspace-gates.json` and
`workspace-gates-retry.json`. Initial workspace test build hit a Windows
executable lock because the asset audit was running `acu-mat.exe`; after that
audit finished, the test/build retry passed. Future audits should use a preserved
decoder copy or run sequentially with Cargo builds. `CARGO_INCREMENTAL=0` was
used for these gates. Only the explicit native regression was added to Rust;
there is no new product visual behavior to validate in this slice.

The explicit ignored regression `installed_idle_matches_recorded_player_primary_rotations`
uses a local fixture selected by observed native context flags, without fitting
phase/rate. It checks the original executable and clip payload hashes, mapped
parent hashes and actual Rust `PoseRig::pose` local rotations at recorded native
times. A +0.3s time shift is a negative control. This covers 23 primary nonroot
joints for the identified idle clip; root, helpers, clothing, final evaluation
stage and selection remain outside coverage. Missing install/fixture data is
reported unavailable through a failed explicit invocation, never a parity pass.

From `acu-rs/`, with `ACU_GAME_DIR` configured and `ACU_NATIVE_IDLE_FIXTURE`
pointing to the ignored `rust-idle-regression.json` from finding0025:

```powershell
cargo test --locked -p acu_bevy installed_idle_matches_recorded_player_primary_rotations -- --ignored --nocapture
```

The capture and fixture identities are recorded in
[finding0025](findings/ACU-RE-0025-player-root-full-rig.md).

Explicit run (2026-10-20): **PASS**, 31 frames × 23 joints, worst frame mean
0.002711 degrees; shifted-time control mean 0.622396 degrees. Used
`CARGO_INCREMENTAL=0`; this verifies the existing evaluator's primary idle
rotation slice and does not establish final pose, root or movement parity.

Latest follow-up, 2026-10-19: the old idle was static; a verified animated
original idle now plays. Upper-leg vertices primarily use previously unevaluated
helper bones. Eight bind-relative constraints now deform those vertices. This
authored adapter awaits native solver comparison; see
[finding0020](findings/ACU-RE-0020-idle-leg-helper-audit.md).

Seven fresh idle/walk/sprint captures were visually inspected in
`local/captures/acu-leg-idle-20261019/manifest.json`. Idle poses change and thighs
deform. Clothing remains stiff; foot IK and native solver parity are open.
Current gates pass: fmt, Clippy with warnings denied, 107 workspace tests
(19 explicitly ignored), workspace build, 22 Python tests, check and doctor.
Both explicit install regressions pass, including actual render-mesh thigh
deformation and a disabled-helper negative control. Executable SHA-256:
`40ded8de3629e875c504f50fb896922d4f1e1de478a60afca4d5ebc324ac362c`. [unidentified project artifact, 2026-10-19; not the installed game binary, the mapped image or the reconstructed PE - see ACU-RE-0031]

Animation audit records 9,682 attempts, 3,317 decoded animations, 6,365 explicit
failures and 261,388 track rows. Actor ownership remains unverified; this does
not establish all Arno locomotion. See [database](ANIMATION_DATABASE.md).
The [native probe and injector](../tools/native_probe/README.md) are built and
fixture-tested, with memory/image capture, verified execution probes, registers,
watches and module-qualified profiles. No semantic game offsets are enabled.
Original launch exits code 42 even without injection. An injected startup image
capture completed but exposes zero conventional unwind/RTTI candidates.
Gameplay bone/traversal/raycast comparison requires a functioning game session.
The [supplied reference review](REFERENCE_LINKS_20261019.md) records game scope
and BuildTable/skeleton leads. Earlier validation below is historical.

## Preparation

`python tools/acu.py check` validates required workflow files, configuration
shape and local Markdown links. `doctor` checks configured reference inputs and
reports tools without installing anything. `inventory` writes local provenance;
it does not validate a parser. Missing optional tools are reported separately.

## Rust and formats

Runtime decoder commands and library usage are in [acu-rs README](../acu-rs/README.md).
The full-install coverage command reads entries in memory and emits reports only.
It intentionally exits nonzero while any input or typed decoder remains unsupported.
This is a coverage failure, not a build/test failure. Preserve each run's report;
do not overwrite or compare a partial scan as though it were complete.

Once `acu-rs/` exists, run the four Cargo gates in `AGENTS.md`. Pin versions and
keep the lockfile. Install-independent tests should use small authored fixtures
and independently established expected results. Cover bounds, truncated input,
unsupported variants, integer overflow, cyclic tables, decompression limits,
duplicate IDs and the selected patch-resolution policy.

Separate actual install tests from synthetic tests. An install vector records
archive SHA-256 or byte-range identity/hash, file ID, source offset, framing,
decoder identity, expected decoded length and SHA-256. Compare decoded bytes
against an independently verified extraction or native routine, not just two
ports of the same guessed algorithm. Shared-port agreement is useful but weaker.
Never silently skip a requested install test and report a pass.

## Simulation and presentation

Replay fixed input sequences with explicit initial state, tick interval, RNG
seed, asset revisions and expected transitions. Check displacement, orientation,
contacts and state transitions with justified tolerances. Include a negative
control where useful: a deliberately wrong conversion should fail comparison.

For each visible change create a capture manifest using
`docs/templates/CAPTURE.json`. Record source revision (or source digest for
uncommitted work), executable hash, build command/profile, launch arguments,
scenario, controls, resolution, camera, seed/ticks, asset identity and outputs.
Capture freshly built code and inspect the result. Label synthetic content.
Keep original and rewrite captures separate and match scenario/camera where
possible; record differences in timing, mods, settings or data.

The install contains ACUFixes and loader DLLs. Record active configuration and
hashes when collecting original runtime evidence. The user explicitly authorizes
runtime execution and reversible hook/loader deployment and configuration changes
without further confirmation. Preserve replaced files and document restoration.
For live DLL-hook comparisons, record target RVA/module base, hook identity,
thread/tick context, inputs, native/Rust outputs and tolerances. Distinguish
ordinary gameplay from injected probes and record instrumentation overhead where
relevant. See `WORKFLOW.md` for the live comparison loop.

A completion report states what passed, what failed, what was unavailable and
what remains unverified. No build, screenshot or sample establishes full parity.

## Cooperative package decoding

`frame::decode_cooperative` accepts an executor-owned async checkpoint and awaits
it every 32 decoded blocks in both frame modes. The synchronous `decode` drives
the same implementation with always-ready checkpoints. Boundary tests use a
handwritten 65-block LZO fixture: exactly two Pending polls, expected ABC output,
corruption after suspension, and cancellation at a completed block boundary.
Cancellation discards the partial output; the advanced Reader is not resumable.
A single checksum/LZO block and allocation remain indivisible.

Viewer direct mesh/rig/navigation package decodes use Bevy IoTaskPool and
`bevy::tasks::futures_lite::future::yield_now` (the installed Bevy 0.19 re-export).
Owned raw bytes move into the task without another full buffer copy. Startup
waits for the task result; this change does not add a loading screen or make the
whole startup asynchronous. Catalog lookup/metadata and material/texture reads
retain their synchronous paths. No Tokio dependency was introduced.
