# Project status

Latest (2026-10-22, corpus binding): **the mapped corpus is bound to this installed
build by headers and resources; code identity is unavailable and unverified**
([ACU-RE-0031](findings/ACU-RE-0031-corpus-binary-identity.md), ACU-003 done, #33).
`tools/bind_corpus.py` compares the installed `ACU.exe`
(`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`) with the mapped
runtime image (`406d47f6...`, 125,980,672 B) and the reconstructed PE (`3723c910...`).
Header bytes `0x0-0x600`, image base `0x140000000`, SizeOfImage `0x7825000`, section
table, `.rsrc` (563,697 B) and `.tls` (273 B) all agree exactly. `.text`, `.rdata`,
`.data` and `.pdata` have RawSize 0 in the installed file, so only 24.5 % of
SizeOfImage is disk-backed and no code byte was compared: the verdict is
`bound_without_code_comparison`, never `bound_by_code`. `.UBX1` differs in 13,208 of
30,335,896 bytes (three 4 KiB pages plus the tail) and is the one declared mutable
section. The 294,030-row export resolves against the binary's own 263,663 unwind starts
at 0.996712 (161,187 exact, 101,601 chained fragments, 867 absent, 0 malformed rows).
The five address-named decompiles resolve to real bytes (`0x1404a4580`, `0x1404ef310`,
`0x140ae2f20`, `0x140af1010`, `0x142062630`), as do `0x142e77580` and `0x140045240`.
Do not describe corpus code as byte-verified against the installed file; the 867 absent
starts and the three patched `.UBX1` pages remain unexplained.

Latest (2026-10-20, traversal research): user prioritizes reconstruction of a
predictive traversal scanner linked to target-based root-motion correction.
[ACU-RE-0030](findings/ACU-RE-0030-traversal-scanner-target-matching.md) identifies
native landing candidate scoring (`0x140177c40`), geometric search caller
(`0x14017f8e0`) and start/target storage (`0x1401a7000`, +400/+416).
The score kernel is ported as `acu_motion::traversal` with three focused tests.
Tracker/target-precision types are located, but the connection to animation
displacement correction, prediction horizon and owning player state is open.
This is a partial scanner reconstruction; the approximate viewer remains unchanged.

Latest (2026-10-20, visible reversal review): **motion matching remains an
approximation; native four-cycle reversal captured, turn transitions missing**.
`tools/native_tap/reversal_capture.py` recorded 2,119 pose frames, zero dropped
events, with four repetitions of right 2 s / left 2 s. The native graph includes
`00000008059484b7` and `00000008059484b9`, archive-named walk plant-and-turn
180-degree clips. The viewer's eight-take adapter excludes turning navigation
roots and blends from a frozen previous pose. This cannot establish turn parity.
The `reversals` scenario now runs visibly and loops; optional
`ACU_REVERSAL_TRACE` writes its pose/clip trace directly to a file.
The increased authored facing caps (walk 360 / run 540 degrees/s) are review
tuning, not native behavior recovered from this capture.
Upper-body follow-up: a second four-cycle capture has 2,291 coherent modifier
entry/exit pairs, no dropped events. Primary joints 2,3,4,5,12,13,14 have zero
local/model rotational change across this stage. Investigate earlier graph
playback/blend and root extraction; do not attribute that stiffness to missing
coat/helper modifiers. See [ACU-RE-0029](findings/ACU-RE-0029-direction-reversal-review.md).

Coat subsolver is now translated experimentally (`acu_motion::coat`, bridge
decoder and `coat_gate`). Idle replay: 3,200 calls / 2,688 solved / 0 unsupported;
with `acu_motion/hw-rsqrt`, worst 0.006673167 degrees / 3.419879e-5 units.
Default math fails the gate at 0.03020005 degrees / 1.086480e-4 units.
Walking, POKE falsification, apply-wrapper integration and full-frame replay
remain required; the viewer does not yet use this solver. See ACU-RE-0028.
Workspace tests passed after the initial port. Current visual review is a
diagnostic comparison, not a completed matcher or a pose parity claim.

Previous (2026-10-20, S2 coat runtime binding): **coat apply and subsolver
confirmed on all captured class objects; solver still unported**
([ACU-RE-0028](findings/ACU-RE-0028-model-pose-bone-modifiers.md), "Live coat-class binding").
- Live vtable `0x142e77580` slot 11 points to `0x140045240`; byte-guarded trace
  saw 19,800 entries (792 on each of 25 candidate objects). A separate guarded
  trace saw 13,475 calls to `0x140045510` over all 21 unique `object+0x48`
  subsolver receivers. Captures, guards and hashes are in ACU-RE-0028.
- The live runtime bytes remain distinct evidence from the installed disk image;
  the existing IDA database had zero bytes at these addresses. Do not assume
  corpus pseudocode is instruction-identical to this installed build.

Previous (2026-10-20, S2 whole-frame gate): **first live slot 1 → slot 2 frame
replay; three modifier classes still unported**
([ACU-RE-0028](findings/ACU-RE-0028-model-pose-bone-modifiers.md), "Whole-frame gate").
- New tap snapshot `SNAPMODS` (`acu_tap12`). In one event it captures the selected
  list's objects and their target/sphere/base records, plus the run channel bytes.
  Capture tool `tools/native_tap/frame_capture.py`; replay bin
  `acu_tap_bridge::modifier_frame`; blob decoder `decode_mods_blob` (unit test).
- Captured 143 frames of the player (PID 24480, which had been running throughout;
  the previous handoff wrongly said it had exited). The level-5 list has 307
  modifiers in eight classes. `0x142e77e20` ×105, `0x142e77580` ×25 and
  `0x142e77d40` ×8 (a dt-driven spring) have no port.
- The 287/459 joints that do not depend on those classes match to **2.8e-3 deg /
  7.7e-6 units**. The rest (coat chains 304–341 and their readers) are off by up
  to 62°. All channel bytes were `0xFF` and there was no skip list.

Checks: fmt, workspace Clippy (0 warnings), workspace tests, workspace build,
`python tools/acu.py check` pass; tap selftest passed for `acu_tap11`/`acu_tap12`.

Previous (2026-10-20, S2 evaluation order): **modifier evaluation order recovered,
ported and matched against a traced frame**
([ACU-RE-0028](findings/ACU-RE-0028-model-pose-bone-modifiers.md), "Evaluation order").
This was a static pass over existing captures and the corpus; no new game run.
- Per-LOD lists (`0x1400210a0`) are filled from depth buckets, filtered by bone
  LOD, and stable-sorted by the u16 at modifier `+36`.
- The frame walk `0x1400377e0` (level = `(u16[component+232] >> 3) & 7`) applies
  per-bone runs (`0x1400379e0`). Each run ends with a post-blend toward the run-start
  transform, weighted by a per-joint u8 channel of the local pose object.
- Against the traced frame: 148/148 applies present, 138 exact positions, and
  10 equal-key same-bone ties (bone-array order not captured), 0 mismatches. Ported
  as `acu_motion::modifier_order` (6 unit tests). New tools:
  `tools/native_tap/wtrace_chains.py` and `modifier_order_export.py`, plus the bin
  `acu_tap_bridge::modifier_order_check`.
- The `0x14008af2b`/`0x14008aeaf` "writers" are page neighbours outside the pose
  array (WWATCH guards whole pages). They come from the typed channel lerp
  `0x14008ae60`, not the player pose.

Checks: fmt, workspace Clippy (0 warnings; also `hw-rsqrt`), workspace tests,
workspace build, `python tools/acu.py check` pass. Open: a whole-frame slot1 → slot2
gate needs the run channel bytes and a frame-entry capture. Also open: equal-key tie
order, 6 arm OrientationConstraints that never stored, the Hinge scale path, and
the Reflex variants.

Previous (2026-10-20, S2 Reflex): **Reflex passes the live per-apply gate**
([ACU-RE-0028](findings/ACU-RE-0028-model-pose-bone-modifiers.md), section
"Reflex"). Apply `0x14004b640` (vtable slot 11 of `0x142e768c0`): a joint-pair
direction in a frame joint's space selects a spherical triangle in a plane tree
(`0x14006cf00`). Its arc-corrected weights blend int16 pose samples onto a base
local transform, which is set in local space (`0x140014b50`). Ported as
`acu_motion::reflex` (6 unit tests) with `decode_reflex` and a gate arm that also
compares each lookup with the native cache record. Across 11,865 applies (idle, arc
walk, POKE) on 15 owners, with native edits up to 33 deg, every leaf and sample index
matched. The worst owner + subtree error is 1.76e-3 deg / 4.1e-6 units. With the new
validation feature `acu_motion/hw-rsqrt` (this CPU's `rsqrtss` estimate) the
lookups are bit-identical and the worst error is 2.6e-5 deg. The default-build
residual is thus measured to be the estimate plus the dot-sum order. The same
feature only lowers Hinge to 2.27e-3 deg, so Hinge has another residual source.
Three POKEs (frame offset, weight, samples) moved the native result and Rust tracked
each one. Checks: fmt, workspace Clippy (0 warnings), workspace tests and build pass.
Open for S2: class order and the `0x14008af2b`/`0x14008aeaf` writers, the Hinge
scale path and residual, and the unported Reflex variants (nine-sample connectors
and the second list; none are live on the player).

Previous (2026-10-20, S2 Hinge): **Hinge passes the live per-apply gate**
([ACU-RE-0028](findings/ACU-RE-0028-model-pose-bone-modifiers.md), section
"Hinge"). Recovered from disassembly with the new sympy SSE emulator
`tools/native_tap/simd_symbolic.py`. Ported as `acu_motion::bone_modifier::Hinge`,
with 3 unit tests against independent geometry, plus the `decode_hinge` decoder
and gate arm. Across 36,096 native applies (idle, arc walk, POKE) over 48 owners,
with native edits up to 61.4 deg, the worst owner + subtree error is 3.39e-3 deg and
2.0e-5 units. Per-owner means are 1e-6 to 6e-5 deg; the maxima sit on small edits
(acosf near 1 against native `rsqrtps`, inferred). POKEs of a sphere radius, the angle
ramp and the distance ramp (an untested live branch) moved the native result and
Rust tracked each one. Visual summary (local, generated):
`outputs/parity/s2-modifier-gates.html`. Checks: fmt, workspace Clippy
(0 warnings) and workspace tests pass. Open for S2: Reflex, class order, the
`0x14008af2b`/`0x14008aeaf` writers, and the Hinge scale path for non-unit entity scale.

Previous (2026-10-20, S2 modifiers): **MaxLookAt, PositionConstraint and
OrientationConstraint pass a live per-apply gate** (Mode A replay of the game's
own entry state; [ACU-RE-0028](findings/ACU-RE-0028-model-pose-bone-modifiers.md)).
Across 59,354 player applies (idle, arc walk, POKE runs) the worst error over
owner + subtree is 3.96e-5 deg and 2.9e-7 units, against native edits of up to
179 deg / 0.17 units; POKEs of weights, axes, target weights and offsets moved the
native result and Rust tracked each one. New: tap build `acu_tap10` (`DRFILTER`
on any argument register), `tools/native_tap/modifier_capture.py`,
`modifier_objects.py`, `find_player_by_leaves.py`, crate `acu_tap_bridge`
(dump reader, layouts, f64 error stats, bin `modifier_gate`). Checks: `cargo fmt
--check`, workspace Clippy (0 warnings), `cargo test --locked --workspace` and
`cargo build --locked --workspace` pass. Open for S2: Hinge (live vtable
corrected to `0x142e77940`), Reflex, class evaluation order, writers at
`0x14008af2b`/`0x14008aeaf`. The streaming pipe / Bevy overlay is not built;
gating uses DUMP files. No full-pose parity claim yet.

Earlier (2026-10-20): [ACU-RE-0028](findings/ACU-RE-0028-model-pose-bone-modifiers.md).
The pose array previously called slot 2 is the model-space pose; its joint
edits come from skeleton bone modifiers applied through one weighted
model-space set (`0x140014a60`). Live write-trace attribution names MaxLookAt,
OrientationConstraint, PositionConstraint and Hinge (`324796c7`). New
`acu_motion::bone_modifier` ports the set/recompose path, quaternion
interpolation and the MaxLookAt solver (6 unit tests against independent
Gram–Schmidt/slerp references). Checks: `cargo fmt --check`, workspace Clippy
and `cargo test --locked --workspace` pass. **No live parity claim**: Rust
modifiers are not yet wired to the runtime skeleton nor gated on captured
slot pairs; other modifier solvers and the Hinge decoder are open.

Previous full-workspace checkpoint: formatting, Clippy, workspace tests (**115
passed, 20 explicitly ignored**) and workspace build passed with
`CARGO_INCREMENTAL=0`. The post-change focused checks are listed below. The
native idle regression separately passes 31 × 23 direct-time comparisons.
The focused native binding regression passes: four stage-flagged walk samples
across 21 bound joints plus one stop sample across all 23 primary joints. Worst
frame mean is `0.003931` degrees; the mode-zero control mean is `48.629986`
degrees. Pure mode and binding tests pass, as do formatting
and Clippy for the changed crates. Post-change package tests pass: 60
`acu_motion` tests and 4 `acu_bevy` tests (4 install-dependent tests ignored in
the ordinary run). `python tools/acu.py check` also passes. Runtime binding modes agree with live bytes;
see [ACU-RE-0027](findings/ACU-RE-0027-native-binding-mode-pose-conformance.md).
Production graph selection, root motion, the two missing transition joints and
codec-7 crowd avoidance remain open. Earlier clip coverage is in
[ACU-RE-0026](findings/ACU-RE-0026-graph-clip-binding-coverage.md).

Latest crowd-route capture (run 59): 128 frames, one owner transform, and two
live 459-joint pose arrays; maximum sampled frame span is 3.01 ms. The graph
reported generic woman/man navigation resources, so that run's graph data stays
excluded from Arno bone comparisons. Runs 57/58 were rejected by the context
guard; the reader now follows live context pointers and records per-frame bone
counts. The captured screen shows Arno walking among nearby pedestrians. See
finding0027 and run-59 machine data under `local/native-probe/motion-link-20261020/`.

Sprint identity follow-up (run 62): a 4-second full-scale stick circle with RT
held produced visible Arno movement along the curb (HUD distance about 130 m to
127 m) and 17.45 capture-unit path / 3.85 net units on the linked root candidate.
The same component chain yielded two animated 459-record pose arrays. This
controlled sprint correlation resolves actor identity for that owner and pose
pair. Live name/topology tables now map 444 rows uniquely to Arno's Rust rig;
15 hashes are unmatched and two parent links disagree. The weak
10,000-magnitude circle in run 61 had no measured root travel; the user's
joystick-radius correction resolved that dead input. Run 63 fixed graph-leaf
capture by following the controller at the live animation component's `+0xd0`
field. It records 139 frames with no graph or leaf errors, 37 binding
snapshots, and exact playback times for sprint and directional locomotion
leaves. A double-read pose check plus stable owner matrix accepts 126 frames
(135 pass the pose-only check); this is a consistency filter, not a
scheduler-atomic snapshot. Trace SHA-256:
`28749f197e914190671b087d815d2cb8f90a38964153005c9936c54be50c68c6`; see
[ACU-RE-0025](findings/ACU-RE-0025-player-root-full-rig.md).

Run-63 binding analysis confirms the sprint and bank-left sprint bindings each
map 23 primary rotations with zero target/source hash mismatches. A broader
weighted-rotation check decoded 26 of 34 observed resources; five resources
remain unsupported because the current decoder rejects codecs 7/8. Across
150 coherent samples (21 or 23 joints), the hypothesis remains about
107–120 degrees from both captured output contexts; the mode-zero control is
also poor (104–121 degrees). Neither context has the previously identified
`0x0c000000` local-stage flag. Reject this as a parity result: first locate the
final pose stage or its required transform, then repeat the same-timestamp
comparison. See the run-63 rejection and coverage in ACU-RE-0025/0027.

Timing follow-up: context-header capture found a repeatable local pose stage
(`+0x8c = 0x0c000000`). A 16-second repeat selects 31 distinct idle times by
that flag alone; all direct-time mean errors across 23 primary joints are below
0.00272 degrees. The explicit Rust regression **passes** against this captured
subset: worst frame mean 0.002711 degrees; shifted-time negative control mean
0.622396 degrees. This is primary idle rotation coverage, not final-pose/full locomotion
parity; see finding0025 and the validation instructions.

Graph follow-up: run 49 follows recognized evaluation leaves to playback
objects and resource handles, identifying 12 clips through movement/stops.
The idle ID resolves to `xx_Nav_IdleLow_Footr`; using recorded native playback
time without fitting gives 0.57-degree median error on 23 primary joints.
A separate 332-sample double-read capture confirms exact agreement in some
samples but substantial timing/stage outliers in others. This establishes a
useful clip/time comparison, not pose parity. Next resolve the native evaluation
boundary and blend contribution. Details and hashes are in finding0025.

Latest ACU-008 result (2026-10-20): repeated walk/stop captures link owner
`0x1a6ab09c0` through animation/skeleton components to a **421-joint** pose chain.
Runs 47/48 saved 448 combined root-and-pose observations without hooks, checking
ownership every sample. 404 hashes map uniquely to Rust; 17 are unmatched and
two mapped parent links differ. Six-clip fitting on 23 joints gives walking
errors of 9.38–9.60 degrees; active native clip/phase and consistent update
boundaries remain open. Strong player-chain evidence replaces the earlier
unidentified shortlist; parity is not established. Direct IDA assembly from a
preserved runtime image confirms fields and count; the disk PE had zeros there.
See [ACU-RE-0025](findings/ACU-RE-0025-player-root-full-rig.md). No Rust tuning.

Latest ACU-008 discovery (2026-10-20): exact captured topology bytes reacquired
current skeletal data in PID 16136 without installing hooks. A bounded metadata
reference scan and record checks found 1,524 valid 25-record buffer candidates;
621 contexts match the captured local nonroot translations, and 43 of those
changed during passive polls. This replaces the stale six-context shortlist
with evidenced current pose candidates. Arno/player identity and root ownership
remain unresolved. Next compare controlled movement/stop phases and establish
a direct owner link before tuning Rust. Scanner controls passed; see
[ACU-RE-0024](findings/ACU-RE-0024-pose-signature-rediscovery.md).

Latest ACU-008 input/turn iteration (2026-10-20): runs 40-43 replayed the same
8.6 s gradual turn arc through verified XInput slot 3 in the wider Paris square.
Screenshots show Arno moving beside pedestrians and carts, so crowd and cart
proximity remain part of the measured route. The slot-3 pre-phase sample still
carried `(-3356,-1869)`; post-phase was zero. Run 40's largest transform
candidate moved 0.4345 units, while the previous run-38 candidate was stationary.
On exact repeat run 41, the largest candidate changed to `0x13ae97fb0` and moved
0.2689 units; its per-phase displacement directions had only 0.337 median
cosine across the repeats. No sampled transform is linked to Arno. The two valid
800-byte candidate arrays remained byte-for-byte static in run 43, so they are
not evidence of live Arno bone motion. No hooks were installed in runs 40-43.
Crowd-constrained traversal is now the active comparison context; root/facing,
collision response, and bone identity remain unresolved.

Rust now has a `turn-arc` replay with the same fixed-tick phase schedule and a
per-tick yaw/root trace. The fresh 516-tick synthetic capture reports a maximum
1.5-degree facing step (the authored walk cap), final root position
`(3.8064,0,-3.8028)` and yaw `-π`. The render was visually inspected in the
synthetic arena. This confirms a gradual Rust test-world turn; it does not
measure ACU turn rate or establish native locomotion parity. The exact matched
input, live candidate sampling, and Rust trace provenance are in
[finding0023](findings/ACU-RE-0023-xinput-caller-routing.md). Direct player/root
and pose identity plus phase-matched movement remain open.

Follow-up pose discovery corrected the earlier stale-context capture: the
`every=1` live trace found 49 contexts on the 25-record path, but broad
neighborhood scans still attach some contexts to multiple nearby transforms.
A repeated forward/reverse candidate moved 2.806 / 1.761 units and its pose
changed throughout; six-clip fits remain at 16.86-21.22 degrees with different
best rates. This is a controlled moving-pose candidate, not a proven Arno link
or clip match. A high-profile parkour attempt visibly moved Arno over a railing,
but Windows recorded a fault at the hooked bone-dispatch site `0x988e3` with
exception `0x80000004`. Do not reinstall that hook. Continue from a fresh
Ubisoft-launched session with XInput-only hooks and read-only pose sampling. A
fresh session now runs as PID 16136 with the same game image. Earlier runs 26-28
showed either stale events or the persistent `(-3356,-1869)` caller baseline;
they are superseded for input-delivery claims by run 30's synchronized
caller-state capture. A direct pose-context/owner link in this process remains
unresolved. No Rust code or motion constants changed; parity remains open.
Details are in finding0022 and finding0023.


Latest native pose observation, 2026-10-20: the live routine processes
25 records of 32 bytes with a 12-byte-stride parent table. The local record is
translation `(x,y,z,0)` followed by an XYZW quaternion; the native function
composes child translation from its parent. A 786-snapshot read-only capture
reproduced that translation equation across 18,864 comparisons per output
buffer, with maximum residual below `1.7e-7`. Source code supports normalized
parent/local quaternion composition, verified on 3,600 child samples from a
stable menu-state output (maximum quaternion error `6.3e-8`). Moving-state
captures still show large orientation outliers because arrays are read while
the game updates joints; a dynamic update-boundary capture remains open. R9 is a pose
context (`0x13E137AF0`); its `+0x178` field points to the local array
(`0x13E137C90`). A live 25-entry hash table now maps this runtime subset to
unique Rust pose rows. Actor identity, atomic capture and phase-matched Rust
comparison remain open. An exploratory comparison at similar walk input timing
had root-relative translation error median `0.229 m`; clip identity and
coordinate basis were not confirmed, so this is a measured mismatch, not a
parity verdict. Rust exports all 721 matcher matrices, parents and hashes. No
bone or locomotion parity is claimed. See
[finding0022](findings/ACU-RE-0022-live-bone-pose-buffer.md).

Live input follow-up, 2026-10-20: a byte-guarded post-call hook at ACU RVA
`0x273A922` sampled the caller's 16-byte XINPUT_STATE and RAX after the
XInput call. In the current hook revision, all 27 results succeeded; nine
samples carried the scripted right-stick/trigger/button state and eight were
neutral. Older-revision ring events were excluded. This confirms the state
returned to the game's caller, not locomotion. A separate 102-frame linked
capture found 13 owner candidates; the largest moved `0.3614 m`, while two
others moved `0.3258 m` and `0.2297 m`. The plaza remained crowded, so no
candidate is identified as Arno. Actor identity, active clip/time, root-motion
ownership, and phase-matched pose comparison remain open. See finding0022 for
binary guards, source reference, trace hashes, and capture paths.

A subsequent `every=1` capture exposed inconsistent controller-to-caller
association: during a replay the runner reported forward/backward states, but
the native caller mostly returned the persistent `(-3356,-1869)` baseline in
both windows. Only two positive-Y forward samples appeared; 113 negative-Y
samples appeared during backward. A passive five-second capture returned only
the same nonzero baseline while four candidate owners moved `0.133-0.346 m`.
These observations do not establish an idle baseline or controlled owner.
Resolve device/caller identity before using the candidate trajectories for
parity. The native tool now retries transient pipe-busy error 231; see
finding0022 for exact hashes and captures.

Two synchronized open-plaza follow-ups found moving transform objects that
contain candidate pose-context pointers. One candidate pair moved 10.47 units
and its local pose changed in 114/187 samples; the repeat run's candidate set
changed and included a 57.59-unit single-sample jump. Memory-neighborhood links
and trajectories remain ambiguous, so Arno's skeleton and root motion are still
unidentified. These captures add no parity claim; see finding0022.

A later rebound found R9 context `0x151CFBFE0` inside a transform object that
moved smoothly in verified sprint and walk windows (9.323 and 1.867 units) and
whose 800-byte pose array changed during both. The short-walk diagnostic fit
selected a forward-sway clip at 21.81 degrees median mean-joint angular error;
the sprint fit selected a different resource at the tested rate floor. This is
a stronger player/skeleton candidate, but active clip/time, Arno ownership and
matched-pose parity remain open. Follow-up dock inputs were blocked by a post.
See finding0022.

Directional follow-up (2026-10-20) applied verified forward/back/left/right
inputs and captured 6,144 guarded bone-update samples. The trace includes 160
local-pose contexts, with 37 contexts on the 25-bone path; the previously
tracked owner again stayed effectively fixed. The controlled character's
skeletal instance is still unidentified, so these contexts are search
candidates rather than matched pose evidence. A subsequent 136-frame capture
read all 37 candidate local arrays through those phases with zero read errors;
the previously mapped context changed in 131 frames and 21/25 records. Other
candidate contexts also moved, so this still does not isolate Arno.

`acu_motion::native_pose` now decodes the recovered records and composes the
nonroot hierarchy independently of Bevy. It also maps live hash/index pairs to
Rust pose rows with duplicate and missing-entry checks; all 58 acu_motion tests
and package Clippy pass.

The latest 25-node cross-check matches all mapped parent links to the Rust rig.
Across 3,264 samples, nonroot local translations match the saved Rust pose with
p95 error `3.73e-7` and maximum `9.08e-4`; local rotation error remains high
(median `26.1 degrees`) at the unmatched pose time. This supports rig mapping,
not animated-pose parity.

The selected Rust clip resolves to decoded resource `0000000fc08fd67a`
(`xx_Nav_Walk_3-Loop`); its internal key `0x6789439a` is not its file ID. Of
the mapped joints, 23 have rotation tracks in the clip. A phase/rate fit against
the captured candidate series bottoms at the minimum tested playback rate and
still has `33.0 degrees` median per-frame mean joint error during forward
input. This is evidence against the current candidate being a phase-matched
instance of that clip, not proof the native game failed to play it.

I also decoded five hash-matched directional walk variants from the same
`DataPC.forge` package. `xx_Nav_WalkFwd_3-Loop_SwayRt90` scored best across
every tested input phase, including backward and neutral: its forward error
was `26.9 degrees`, and its fitted rate still hit the `0.1` lower search bound.
Because the same clip wins for opposite inputs and neutral, this does not
identify ACU's active clip or phase. See the resource provenance and fit report
in finding0022.

Open-street follow-up, 2026-10-20: a 229-sample, 3,360-byte pose-region capture
passed identical full-region double reads with no read failures. XInput readback
verified walk, sprint, lateral, backward and neutral phases; the sprint frame
visibly shows Arno running. The mapped owner transform nevertheless remained
fixed throughout, so the capture still does not identify that pose context as
Arno or establish its root motion. Both output buffers retain low child
translation-composition residuals (p95 about `8.9e-8`); quaternion residuals
have rare maxima near `1.525 rad`. A fresh six-resource phase fit chose the
left-sway walk for every phase at the `0.1` minimum rate, including neutral and
backward, with `22.1-23.9 degrees` error. This is another failed clip
discriminator, not a parity result. Evidence and hashes are in finding0022 and
`local/native-probe/actor-arc-20261020/open-street-pose/`.

Latest locomotion review, 2026-10-19: the turn-in from idle no longer snaps to
the first stick heading. User visual feedback that turning felt too sharp led to
authored walk/sprint facing limits changing from 180/270 to 90/135 degrees per
second. A fresh 180-tick synthetic corner trace advances facing gradually and
finishes on the requested heading; the reviewed capture is
`local/captures/acu-gradual-turn-20261019/turn.png`, with its replay report at
`local/captures/acu-arno-rigged-20261019/scene-turn-180-145.json`. Focused
validation: 4 acu_bevy tests passed; 2 install-dependent tests were ignored.
The live ACU gradual-arc capture has verified input readback but enters a
cluttered contextual-movement situation, so it is not a clean turn-rate reference.
No native turn-rate or steering parity is claimed. See
[finding0019](findings/ACU-RE-0019-directional-navigation.md).

Latest follow-up, 2026-10-19: the old idle was static; a verified animated
original idle now plays. Upper-leg vertices primarily use previously unevaluated
helper bones. Eight bind-relative constraints now deform those vertices. This
authored adapter awaits native solver comparison; see
[finding0020](findings/ACU-RE-0020-idle-leg-helper-audit.md).

Seven fresh idle/walk/sprint captures were visually inspected in
`local/captures/acu-leg-idle-20261019/manifest.json`. Idle poses change and thighs
deform. Clothing remains stiff; foot IK and native solver parity are open.
The current checkout passes `cargo fmt --all -- --check`, all workspace tests,
the 22 Python tests, and the workflow check. Clippy, a fresh release build and
visual replay after the latest FH curve operation-order edit remain pending.
Both explicit install regressions pass, including actual render-mesh thigh
deformation and a disabled-helper negative control. Executable SHA-256:
`40ded8de3629e875c504f50fb896922d4f1e1de478a60afca4d5ebc324ac362c`. [unidentified project artifact, 2026-10-19; not the installed game binary, the mapped image or the reconstructed PE - see ACU-RE-0031 and issue #34]

Animation audit records 9,682 attempts, 3,317 decoded animations, 6,365 explicit
failures and 261,388 track rows. Actor ownership remains unverified; this does
not establish all Arno locomotion. See [database](ANIMATION_DATABASE.md).
The [native probe and injector](../tools/native_probe/README.md) are built and
fixture-tested, with memory/image capture, verified execution probes, registers,
watches and module-qualified profiles. No semantic game offsets are enabled.
The Ubisoft-launched ACU session PID 25068 stayed open through menu entry, input
sweep and live XInput instrumentation. A mapped main image contains 263,663
unwind ranges; address-shaped decompile joins produced 161,087 named ranges,
with export-to-installed-image identity still unknown. See
[finding0021](findings/ACU-RE-0021-live-controller-capture.md). Gameplay
bone/traversal/raycast semantics remain unmapped.
The new [mapped-image/decompile cross-reference](NATIVE_DECOMPILE_XREF.md)
joins address-shaped corpus filenames to native unwind ranges by preferred-base
RVA and records exact versus containment candidates, binary image hash, and
explicit identity limits. A fresh unpacked runtime image is required for useful
names; the earlier early-startup dump has no ranges to join.
Controller-first capture is now live-validated. Virtual Xbox input advanced the
current game's title/menu route into the Paris world. A 29-step sweep sent all
ordinary X360 controls; a concurrent byte-guarded XInputGetState probe recorded
4,243 entries across XINPUT1_3/1_4, including user indices 0 and 1. The probe
reported 84 dropped/overwritten events. The Skills UI was visible after the
sweep, but per-button action effects are not yet mapped. The category checkbox
launcher exposes 14 subsystems; only `input` currently has a verified native
site. The local ViGEmClient build connected to the existing bus. See
[controller workflow](CONTROLLER_AUTOPLAY.md). Full readable process-memory
capture first stopped at 1.9GB due to named-pipe contention. A standalone
all-region retry was interrupted with a partial 2.57GB / 5,736-file artifact;
its manifest is marked `partial` and records unreadable holes. The result is
sequential and non-coherent; a complete all-region snapshot remains open.
The [supplied reference review](REFERENCE_LINKS_20261019.md) records game scope
and BuildTable/skeleton leads. Earlier validation below is historical.

Updated: 2026-10-19. Phase: original navigation clip locomotion available;
comprehensive asset decoding and native movement reconstruction remain incomplete.

ACU-008 directional follow-up fixes the reported forward-only selection. The
bank contains eight original idle/forward/lateral/backward navigation takes.
Low profile walks; held RT/R2 high profile sprints. The imported FH pose and
trajectory objectives search within a profile, using actor-relative travel
requests while facing turns independently. Original root distance and pose
share a clip/time; analog amount scales playback. A continuing take retains
phase; transitions blend locally over 160 ms. This is an explicit Unity adapter,
not a recovered Unity animation graph. See [finding0019](findings/ACU-RE-0019-directional-navigation.md),
[root-frame finding0018](findings/ACU-RE-0018-animation-tracks.md), and the
[reviewed FH source boundary](FOR_HONOR_MOTION_REUSE.md).

Current fmt, Clippy with warnings denied, workspace tests (106 passed / 18
explicitly ignored install tests) and workspace build pass. The new explicit
`player::tests::installed_directional_steering` passes with original install:
both profiles select opposite original lateral takes without an idle/profile
change, corner requests preserve facing, and holding direction returns to a
forward take without churn. Synthetic tests include a forward-only negative
control, phase preservation and independent facing/collision. All 20 Python
tests, preparation check and doctor pass. Physical gamepad operation, original
Unity steering parity and foot contact remain unverified this slice.
Eight motion-matcher captures were freshly inspected in
`local/captures/acu-directional-final-20261019/manifest.json`. The subsequent
cooperative I/O build passes all gates and the install regression. Both fresh
latest-build captures in `local/captures/acu-directional-io-20261019/manifest.json`
were visually inspected; PNG bytes, playback traces and decoded asset identities
match the corresponding earlier captures exactly. Latest executable SHA-256:
`d420d639c79c6eadb007f1d244244a5995e58065b74f2af24e7a30e9cd11b187`; source digest:
`f9fbd9582a2e6f4e1e2fb819299bc47415f6deb605b8ec21ad159b539fdc7dc0`.
Existing thin head/neck geometry artifact, stiff clothing and occasional
intersections remain; no lateral foot-contact or native steering parity is inferred.

Viewer direct mesh/rig/navigation package decodes now use Bevy IoTaskPool with
`yield_now().await` checkpoints every 32 blocks in both decoder modes. The core
reader remains executor independent; its synchronous CLI uses ready checkpoints.
Tests prove real Pending suspensions, unchanged expected bytes, corruption
rejection after yielding and cancellation at a block boundary. Startup still
waits; this does not make catalog/material/texture loading asynchronous. See
[validation boundaries](VALIDATION.md).

For Honor cloth solver, kernels, typed records and runtime skin/deflector/render
integration were reviewed read-only, with source hashes recorded in the reuse
review. Unity cloth records, operation compatibility and garment bindings remain
unresolved; no cloth solver is enabled. Native parkour/climb/vault playback,
foot IK, actor ownership and Tailored equipment remain open. Prior forward-clip
stance/root-frame audit and captures remain in
`local/animation-matcher-20261019/` and `local/captures/acu-motion-20261019/`;
those checks do not validate current lateral foot contact.

The reusable [animation database](ANIMATION_DATABASE.md) retains 110,447 metadata
occurrences / 37,803 distinct IDs. Decoded audit tracks are now imported (coverage above); verified actor ownership
rows remain empty.

The current source retains original four/eight-weight CPU skinning, authored fixed
tick movement, a follow/orbit camera, synthetic daylight/shadows and original
diffuse plus inferred packed-normal PBR shading. Default geometry is still the
identified FearlessAssassin set, not a resolved Arno's Tailored selection.
38 diagnostic candidate rigs supply 721 distinct name hashes; one active shoulder
hash remains parentless/frozen and is explicitly reported. Bind error is below
7.18e-7 in the selected set. See [finding0017](findings/ACU-RE-0017-composed-rig-skinning.md).

Controller priority: left stick movement; hold RT/R2 high profile; hold X/South
with it for upward priority, Circle/East for downward priority; right stick yaw
and pitch. These requested bindings drive the synthetic test approximation,
not recovered native traversal. See [movement specification](mechanics/test-world-movement.md).
Earlier rig-only milestone: fmt, clippy with warnings denied, workspace tests (51 authored passed,
16 install tests explicitly ignored) and build pass. All three selected skeleton
install tests pass, including 25/28 supported bootstrap rigs and the five clothing
vectors. Nine fresh Vulkan captures validate bind/deform/walk/run/turn/jump and
up/down intents; manifests retain source, executable, asset and output hashes in
`local/captures/acu-arno-final-20261019/`. Movement, jump, reset and camera are
also checked in a real DX12 window using the scoped review helper. Physical
controller button operation is not yet manually verified; bindings and both
camera axes pass automated input tests. A fallback gamepad mapping warning is
present on this host. That earlier procedural gait lacked corrected foot contact/IK.
Build/run with `./tools/run_arno.ps1`.
The four user-supplied rendering/crowd/Anvil PDFs are indexed with page scope and
hashes in [the reference library](REFERENCE_LIBRARY.md); later Anvil systems are
not treated as proof of Unity's implementation.

## Current runtime decoder work

Earlier rig-reference milestone (2026-10-19): [ACU-RE-0016](findings/ACU-RE-0016-build-table-rig-references.md)
adds a bounded reference/handle BuildTable diagnostic: ten selected tables reach
exact payload end, including four equipment tables with 43 rows each. The avatar
skeleton table identifies eleven rig references. A head-table row links the
current head package to CN_U_Arno_Head_Rig; the Rust decoder now reads its 134
bones and 106 wrinkle modifier inputs. The rig's one pose group now preserves
147 poses / 2,526 packed bone records; the bootstrap install test supports 20/28
rigs, with eight deliberate rejections. Independent head-palette matching improves
from 5/116 against male alone to 110/116 against the referenced head rig; six
remain unresolved. Runtime rig composition, row selection, Tailored tag mapping,
modifier evaluation and controlled deformation remain open. No renderer change.

Final rig-reference gates pass: fmt, clippy with warnings denied, all 42 authored
Rust tests (15 install tests ignored in the standard suite), and workspace build.
Both explicit skeleton install tests pass: the referenced Arno head and the
20/28 bootstrap subset with eight deliberate rejections. Workflow checking and
all 14 Python tests pass in both checkouts. Logs are retained under
`local/rig-references-20261019/`, including `clippy-final.log`,
`workspace-tests-final.log`, `skeleton-install-final.log` and `build-final.log`.

Development now uses feature branches in the independent published checkout,
merging significant validated milestones into main. The GitHub landing page and
description use neutral technical branding; research findings retain precise
source identities. Existing history remains intact.

Publication follow-up (2026-10-19): published the public collaboration repository
under Rdev: [rdev-rna/AU-RW](https://github.com/rdev-rna/AU-RW).
A separate source-only checkout at `C:/Users/PC/source/AU-RW` uses fresh history
and repository-local Rdev author identity. The research parent repository is
unchanged. The export excludes game data, corpus, local configuration, captures,
build outputs and prior history. No project license is assigned. Runtime code
and the ACU-005 priorities below are unchanged; earlier publication-unset notes
describe historical checkpoints.

Publication validation: workflow check and all ten Python tests pass in both
research and exported checkouts. All 94 exported files match the source manifest
hashes; ZIP integrity, excluded-data boundaries and credential-pattern scan pass.
The initial commit `d8185577d7e73a5f03214cfde29262aca03c0b72` contains exactly
the 94 reviewed source paths, with Rdev authorship linked to `rdev-rna`.
GitHub reports PUBLIC visibility and main matches the pushed commit; onboarding
is accessible. Parent Git root, index and remotes remain untouched. No Rust or
visual gates rerun for publication/documentation-only changes.

The project title is now **AU-RW** (user-selected). New collaborators can use
the [onboarding prompt](COLLABORATOR_PROMPT.md), linked from README,
CONTRIBUTING and MASTER_PROMPT. It explains task coordination, portable paths,
evidence, validation and handoffs; the obsolete ACU-001 first-session assignment
has been replaced. This documentation follow-up changes no runtime behavior,
Git location, remote or publication state. Runtime priorities below remain open.
Validation: workflow check and all ten Python tests pass. Refreshed local review
bundle: `local/au-rw-source-review-20261019-onboarding.zip` (94 authored files
plus manifest); ZIP integrity, onboarding inclusion and all file hashes verified.
No Rust gates rerun for this documentation-only follow-up.

Collaboration preparation (ACU-006) adds portable local configuration for the
game and decompiled corpus, an optional sibling reference, contributor setup
and an allowlisted source-only bundle with per-file hashes. This machine's
paths now live in ignored `local/project.json`. No nested repository, Git
staging, license, remote or publication has been created. See
[CONTRIBUTING.md](../CONTRIBUTING.md). Ten Python tests pass in the working
tree and clean exported snapshot; workflow check passes 21 required files.
Doctor and inventory pass (217,041 corpus files, 59 top-level Forge archives,
10 DLLs; executable hash unchanged). A clean export reports missing inputs
before setup and passes doctor with external paths and no sibling checkout.
Launcher syntax/configuration resolution pass. Final candidate:
`local/unity-source-review-final-20261019.zip`, 93 source/documentation files
plus SHA-256 manifest. No Rust/renderer change or fresh visual claim; prior
Rust gates remain historical. The decoder and remaining rig work are unchanged.

The numerical skin-palette slice now audits all eleven current Arno meshes:
29,481 vertices / 19 parts, every weight-byte sum 255, no weighted joint outside
its hardware palette and no palette index outside its mesh bone table.
Exact name hashes match 105 of 397 active mesh-bone entries to the male rig;
292 remain unresolved against that rig. Column-major stored mesh matrices
multiplied after skeleton globals give a consistent candidate inverse-bind
relationship within the sampled meshes, with blade/hair differences preserved.
See [ACU-RE-0015](findings/ACU-RE-0015-skin-palettes-bind-transform.md) and
`tools/skin_audit.py`. This is diagnostic archive evidence, not resolved additive
rig selection, runtime skinning, a new visible scene or animation playback.
Fresh gates for this slice: preparation check/doctor/inventory, fmt, clippy with
warnings denied, 40 authored Rust tests / 14 ignored install tests, workspace
build and six Python tests pass. The focused eleven-mesh original-install audit
also passes independently of the ignored tests; logs are `local/skin-*.log`.

The movement mission is now sequenced in [MOVEMENT_PLAN.md](MOVEMENT_PLAN.md)
and root PROGRESS.md. User constraints: Arno's Tailored hood/coat/gloves/greaves
(asset selection remains unresolved), and Day, Paris for the native lighting
target (current presentation remains synthetic). The requested video was reviewed
with claude-watch over 12:39-16:39; [LIGHTING_RESEARCH.md](LIGHTING_RESEARCH.md)
records scope and limits. [RENDERING_SOURCES.md](RENDERING_SOURCES.md) indexes the
user-designated SIGGRAPH2015 architecture reference, separating ACU from RedLynx.

The humanoid skeleton extension decodes 19/28 bootstrap skeletons, including the
full 109-bone male rig and 68-bone ACU_Base. Four modifier schemas and bounded
inline IK/COM/Reflex data now parse with exact payload/hash checks. Nine variants
remain explicit failures. See [ACU-RE-0014](findings/ACU-RE-0014-humanoid-skeleton.md).
This is data decoding, not solver execution, resolved skinning or animation.
Fresh validation for this extension: fmt and clippy pass; workspace tests pass
40 authored tests with14 install tests ignored. The explicitly run skeleton
install test also passes19/28 supported rigs with nine deliberate rejections.
The final workspace rebuild passes. Logs are `local/humanoid-*.log`; the explicit
install test was run from the freshly built skeleton test executable.

The latest requested visible slice is complete: eleven Arno mesh pieces, nineteen
draw parts, eight materials and seven resident diffuse textures load directly
from the installed Forge archives. All parts bind a material; the scene report
has no missing-reference or selector/set disagreement notes. Front, rear and side
captures were freshly inspected at `local/captures/acu-arno-textured-20261019/`.
The DirectX 12 window was also opened and visually inspected (`window-dx12.png`)
and left running. Vulkan offscreen rendering works, but its window swapchain
reports invalid layout/semaphore errors on this machine. The launcher selects
DX12 for the window and Vulkan for offscreen captures.

Run `./tools/run_arno.ps1 -GameDir <ACU-install-directory>` from this project.
Arrow keys orbit, Q/E zoom, Escape exits. `-Capture <PNG-path>` captures and exits;
`-NoTextures` restores the grey diagnostic model. This is a static inspection
scene with diffuse maps and alpha testing, not animation, skinning, original
template shaders or native material/lighting parity. See
[ACU-RE-0013](findings/ACU-RE-0013-material-texture-chain.md). The parser now
preserves the nonboolean reference scope byte found in the blade material.

Validation: preparation check/doctor/inventory, Cargo fmt, clippy (no warnings),
workspace tests and build pass. The material suite passes five authored tests
and two explicitly requested install tests (including the blade scope-4 vector).
Logs are `local/arno-textured-*-final.log`; capture identities are in the manifest
and `scene.json`. This validates the selected character chain, not all assets.

The user clarified direct reading and use of original assets at runtime, with
success requiring every asset to parse (models, maps, animations and textures).
This is active task ACU-005; see [coverage acceptance](ASSET_COVERAGE.md).
The implementation does not extract assets or depend on extracted directories.

`acu-rs/` now contains acu_assets with plaintext Forge reads, ACU LZO framing,
resource envelopes, archive controls, textures, meshes, simple skeletons and a
runtime catalog, plus probe/scan/mesh CLIs, and the acu_bevy inspection app.
Rust is pinned to installed 1.99.0 MSVC; Bevy 0.19.1 builds from the workspace
dependency set. See [ADR-0001](decisions/0001-decoder-workspace.md). Cargo.lock
is retained; the parent repository remains unstaged/uncommitted.

[ACU-RE-0002](findings/ACU-RE-0002-runtime-framing.md) records independent native
liblzo agreement for four base/patch frames and zero-seed Adler-32 checksums.
AnvilToolkit was inspected read-only as a format/validation reference and is not
a runtime dependency. [ACU-RE-0003](findings/ACU-RE-0003-texture-runtime.md) covers
borrowed pixel/mip ranges; [ACU-RE-0005](findings/ACU-RE-0005-texture-variants.md),
[ACU-RE-0007](findings/ACU-RE-0007-texture-no-inline-pixels.md) and
[ACU-RE-0008](findings/ACU-RE-0008-mesh-runtime.md) cover cube/volume/padded
textures, descriptor-only placeholders and the cluster-only mesh geometry variant
(empty non-clustered buffers validated against the compiled clustered index
buffer). [ACU-RE-0006](findings/ACU-RE-0006-prefetch-control.md) covers the
ID0x91 prefetch control, [ACU-RE-0010](findings/ACU-RE-0010-skeleton-subset.md)
the simple-skeleton slice, and
[ACU-RE-0011](findings/ACU-RE-0011-clustered-vertex-decode.md) the clustered
vertex attribute layouts, the quantization-scale position formula and the first
visible render: eleven mesh resources (nine FearlessAssassin battle-outfit
pieces, the head mesh and the hood-up hair plane) load straight from the
installed Forge archives into a grey synthetic Bevy arena at 1280x720 from three
orbit angles (`local/acu-arno-35.png`, `local/acu-arno-145.png`,
`local/acu-arno-260.png`; capture manifest and per-piece hashes in
`local/captures/acu-arno-20261018/capture.json`).
[ACU-RE-0012](findings/ACU-RE-0012-clustered-mesh-layout.md) then types the
compiled clustered block: 52-byte cluster descriptors (AABB, normal cone, index
span, vertex-group byte offset), 48-byte per-cluster bit masks and 36-byte draw
primitives (AABB, stride, packed flag/value, index start), with seven rules
validated on fifteen installed meshes. Raw clustered indices are
primitive-relative and their base is `vertex_offset_dwords * 4 / stride`;
`MeshData.min_index` disagrees on the bed by 240 vertices, so descriptors are
authoritative for cluster-only geometry. The corrected bases fixed the
multi-cluster renders: fresh captures of the Arno set at three angles plus the
desk (74 clusters), bed (64 clusters) and rock are in
`local/captures/acu-clusters-20261018/` with the run logs in
`local/logs/capture-clusters-*.log`; the previous Arno captures had a mangled
torso and detached arms where clusters were misplaced.
Decoded native bounds for the assembled set: min
`[-0.49482712, -0.18659016, -0.00024414808]`, max
`[0.49446088, 0.24909207, 1.8527176]`; the stored per-cluster boxes matched
decoded geometry within 1.0122 across all fifteen samples. The cluster bit mask,
glue/cluster-to-submesh mapping, native axes/units, triangle winding and
skinning remain unresolved, so this is an inspection slice, not world, material,
animation or gameplay parity.

Completed audit: 63 Forge archives including DLC, 180,245 reachable entries,
180,115 decoded/resource-indexed packages, 3,289,051 resource envelopes,
484 resource type hashes and 126 typed controls (63 global metadata + 63
prefetch indexes). Zero unparsed metadata extensions. Four Paris entries
(`00000023a0fbf0fb`, `00000023dab179da`, `0000002347f7952c`, `000000232002f58a`)
have no frame magic and remain explicit failures; Paris still declares four more
entries than its reachable table (24,681 vs 24,677). Report:
`local/coverage-typed-full2-20261018/summary.json`, SHA-256
`a17a954651125cfdca6a46f65dcc1f59263ad92be9fe5be31ef95ff4cc7d8130`;
scanner `target/release/acu-scan.exe` SHA-256
`4a0efdd26b01b9401240f817fb6aa9799e286edc0f13f7de9d21158a1be69d2d`.

Typed CPU parsing now covers 280,705 of 3,289,051 resource envelopes (8.5%):
TextureMap 65,466/65,466 (65,464 with validated inline pixel/mip spans, 2 typed
descriptors without inline pixels), Mesh 203,062/203,190 and Skeleton
12,177/19,036. Of the parsed meshes, 202,975 carry compiled cluster descriptors
that are now typed by ACU-RE-0012 but whose 48-byte-per-cluster bit mask is
still unresolved, so they are counted as partial, not complete typed coverage.
Remaining mesh failures: 71 invalid instancing bone/vertex counts,
49 primitive spans outside the clustered index buffer, 8 nonfinite scalars.
Remaining skeleton failures: 5,131 inline bone modifiers, 1,679 unsupported
inline/managed pointers, 49 pose groups. The largest unsupported families by
envelope count are LODSelector (~531k), Entity (~470k), Material (~443k),
TextureSet (~403k), Animation (~110k) and EntityGroup (~91k). The ACU-RE-0011
and ACU-RE-0012 inspection slices render decoded meshes; no texture-mapped,
animated or world rendering is claimed.

Cargo fmt, clippy (`-D warnings`), test and build gates pass from `acu-rs/`.
Forty-three authored tests cover record, frame, texture, mesh (including the
cluster-only variant and a synthetic clustered block with eleven malformed
mutations), vertex layouts and decode, skeleton, prefetch, control and catalog
boundaries; eleven install-dependent tests are explicitly ignored in the
standard suite and pass when run with `ACU_GAME_DIR`. The preparation workflow
checks pass. The full `--locked` workspace suite, the ignored install tests and
the ACU-RE-0012 cluster audits all passed on fresh runs; the acu_bevy capture
path was re-exercised after the decode change and rendered the corrected Arno
set, desk, bed and rock captures listed above.

## ACU-001 result

[ACU-RE-0001](findings/ACU-RE-0001-forge-compatibility.md) verifies the FH
container layout with decryption removed against DataPC, DataPC_patch_01,
DataPC_ACU_Paris and DataPC_extra_patch_01. The bounded Python probe
`tools/forge_probe.py` validated 26,322 reachable entry records and saved
structural range hashes and 12 stored-entry hashes in ignored
`local/forge-compatibility.json`. Three patch extracted files and one Paris
metadata file match stored bytes exactly. No compressed decode was attempted.

Paris declares 24,681 entries but the reachable table has 24,677; index coverage
is incomplete/uncertain. Compressed samples have a different frame magic from
FH's Oodle framing. Codec/provider selection and patch precedence remain unknown.
LZO1X is a candidate from related tooling, not established ACU evidence.

Validation: `python -m unittest discover -s tools -p test_forge_probe.py` passes
three structural fixture tests, covering malformed offsets, counts, cycles and
truncation. The four-archive probe, preparation check and doctor pass.
Inventory refresh passed: executable hash unchanged, 217,041 corpus files,
59 top-level Forge archives and 10 top-level DLLs. No Rust gates apply yet.

Standing authorization clarified by the user: execute ACU game code at the agent's
discretion, including live DLL hooks and comparisons, without per-action approval.
Reversible hook deployment/configuration changes are authorized. This supersedes
the earlier preparation-only runtime restriction and any contrary language in
historical handoffs; see `AGENTS.md` and `docs/WORKFLOW.md`.

## Established locally

- Project root: `E:/ReverseEngineering/Projects/Unity`.
- Git root: `E:/ReverseEngineering/Projects`; no project files were listed by
  `git ls-files -- .` at initial inspection. No new repository/remote/commit made.
- `ACU - Decompiled/` contains 217,041 `.c` files totaling 228,711,062 bytes.
  Sample headers include `// 0x140001000 sub_140001000 size=2621` (spacing normalized).
  This is address-named pseudocode; source tool/build identity is not established.
- Game `ACU.exe`, base/patch Forge archives and extracted folders exist under
  `D:/Games/Assassin's Creed Unity/`. Extracted folders include
  `DataPC_ACU_Paris.forge` and `DataPC_extra_patch_01.forge`.
- ACUFixes/plugin-loader and proxy DLLs are present. Runtime behavior may depend
  on mods; no game launch or DLL loading was performed during setup.
- For Honor's runtime `fh_assets/src/forge.rs` exists and was inspected. Three
  ACU archive prefixes contain plaintext `scimitar`; see `docs/FORGE_REUSE.md`.
- Installed tools observed: Python 3.12.10, Cargo/Rust 1.99.0, Git 2.46.0.windows.1,
  ripgrep 15.2.0. These are observations, not selected project version pins.

## Preparation validation

All three preparation commands passed on 2026-10-18:

```text
python tools/acu.py check
  PASS: 19 required files, JSON/configuration and local Markdown links
python tools/acu.py doctor --output local/doctor.json
  PASS: executable/corpus present; reference Forge reader found; tools reported
python tools/acu.py inventory --output local/inventory.json
  PASS: 217,041 corpus files; 59 top-level Forge archives; 10 top-level DLLs
```

Executable SHA-256:
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`.
Parsed PE metadata: AMD64 (`0x8664`), image base `0x140000000`, entry RVA
`0x7544b1d`, image size 125,980,672 bytes. The first sample export VA therefore
maps arithmetically to RVA `0x1000`; this alone does not verify matching code.

For Honor `fh-rs/crates/fh_assets/src/forge.rs` source SHA-256:
`3a96420b8554ef7787b8a5051eed05a440255bd56c4c5174cae8a283f784dae7`.
Reference source identities, archive prefixes and DLL identities are in ignored
`local/inventory.json`. Archive inventories use size/prefix, not full hashes.
The corpus metadata digest covers names/sizes, not every file's content.

Git ignore checks confirmed the corpus and local inventory are excluded and
`acu-rs/Cargo.lock` is not ignored. Nothing was staged or committed.
Additional smoke checks passed: report output outside `local/`/`outputs/` is
rejected; a missing install makes doctor fail; workflow checking works from a
different working directory; the Python tool compiles successfully.

## Unknown / not yet verified

- Export source executable/hash and correspondence with the current install.
- Archive table layout beyond four sampled archives, payload encryption/codecs,
  patch precedence and resource schemas; Paris count discrepancy.
- Full Unity asset compatibility; the implemented Rust plaintext/LZO path and
  first texture variant cover only the cases reported by current scans.
- General GPU compatibility; Vulkan offscreen and the DX12 window are visually
  validated here. Vulkan window presentation still fails validation.
- Clustered-mesh bit mask semantics, general cluster-to-submesh/bone mapping,
  `fixed_cluster_size` meaning and the bed's 240-vertex descriptor/`min_index`
  discrepancy; see ACU-RE-0012.
- Native gameplay constants, animation selection, root motion, collision and units.
- No playable rewrite or native game-behavior parity claim exists. The only
  visual artifact is a synthetic grey arena with textured decoded Arno, which claims no
  original-world, material, lighting or animation parity.

Next: ACU-005 in `docs/TASKS.md`: close remaining skeleton modifiers/pose groups
and mesh instancing/span/scalar failures, then the large untyped families
(LODSelector, Entity, Material, TextureSet, Animation, EntityGroup), then typed
world/map data; patch/reference resolution remains unresolved. ACU-004's display
gate passed with the Arno slice in ACU-RE-0011 and was re-validated after the
ACU-RE-0012 decode fix; ACU-003 is independently ready; no export/build
correspondence claim has been made.
