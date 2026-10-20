# Task queue

User priority (2026-10-20): reconstruct predictive traversal scanning linked to
dynamic root-motion target matching. Start from ACU-RE-0030, not the authored
matcher weights. First bounded kernel: `acu_motion::traversal::landing_score`
ports native `0x140177c40` (three independent geometry/boundary tests pass).
Search `0x14017f8e0` is called by `0x1401a7000`, which stores start/selected
target at +400/+416 after an additional clearance query. Next establish the
owning state, query contracts and actual animation-displacement consumer, then
gate one transition using coherent native inputs/target/root output. Do not
mark the scanner or root warper complete from the score kernel or type names.

User-prioritized visual acceptance case (2026-10-20): four cycles of right 2 s /
left 2 s. Native capture `local/native-probe/reversals-20261020-01-meta.json`
has 2,119 pose frames and no drops. Establish native plant-and-turn clip timing,
blend weights and root/facing ownership, then implement the corresponding
transition. Current approximate adapter has no plant-and-turn takes and rejects
turning roots; changing the authored facing cap is not acceptance. Use visible
`--scenario reversals`; set `ACU_REVERSAL_TRACE` for a diagnostic trace.

Coat continuation: experimental subsolver port and decoder exist. Idle
`coat_gate` meets thresholds with `acu_motion/hw-rsqrt` only (0.006673167 deg /
3.419879e-5 units); default math fails (0.03020005 deg / 1.086480e-4 units).
Capture walking and POKE, resolve precision sensitivity, implement apply wrapper,
and integrate into `modifier_frame` before marking the class complete.

Active (2026-10-20): motion parity S2, skeleton modifiers on the model pose
(`docs/PLAN_MOTION_PARITY.md`, ACU-RE-0028). Done and live-gated:
MaxLookAt, PositionConstraint, OrientationConstraint (worst 3.96e-5 deg), Hinge
(worst 3.39e-3 deg over 36,096 applies, POKE-falsified; meets the 0.01 deg criterion),
Reflex (worst 1.76e-3 deg over 11,865 applies; 2.6e-5 deg and bit-identical
lookups with `acu_motion/hw-rsqrt`; POKE-falsified).
Evaluation order recovered and ported (`acu_motion::modifier_order`; 138 exact +
10 equal-key ties / 148 traced applies). The `0x14008af2b`/`0x14008aeaf` writers are
out-of-array page neighbours.
Whole-frame gate built (`frame_capture.py` + `modifier_frame`): the 287 joints
independent of unported classes match to 2.8e-3 deg over 143 frames.
Next, in order:
1. Port the runtime-bound coat solver first, then rerun `modifier_frame`:
   `0x142e77580` (apply `0x140045240`, 25 objects; live vtable slot 11 and all
   25 receivers confirmed; sub-object `+72` → `0x140045510`, also live-traced;
   set via `0x140014b50`). Preserve the open disk-image/corpus identity gap
   while translating. Next handle `0x142e77d40` (apply `0x14004d490`, 8
   objects, spring with dt). For the spring, extend the tap to record the frame's
   xmm1 (dt) and object bytes to at least `0xd0`. Establish whether
   `0x142e77e20` (apply `0x14004dcf0`, 105 objects) touches the pose
   (`0x140099c80`, `0x14002eb50`).
   Also check the eight-class interleave against `build_lod_lists`, and find a
   state with channel bytes ≠ `0xFF` to test the post-blend live.
2. Wire the gated modifiers into the Bevy rig (needs modifier data from assets;
   asset-side decoder not established).
3. Hinge follow-ups (optional): trace the scale path `0x140030b00` and gate a
   non-unit entity scale; `hw-rsqrt` only lowers its maxima to 2.27e-3 deg, so
   look for a sum-order difference as in Reflex's world rotate.
4. Then S1 leftovers (sequential slerp in `installed_weighted_blend_matches_coherent_459_context`,
   codec-7 clip `0000001003c5cf97`), S3, S4 (XInputGetState hook), S5.
Acceptance per class: per-apply worst error < 0.01 deg / 1e-4 units over idle and
walking captures, plus one POKE falsification.

Next ACU-008 comparison: repeat the runtime-verified mode-4/mode-5 comparison
for the `Rt90` transition and resolve the two missing transition targets. The
23-joint stop clip now has one flagged sample and passes the Rust binding
regression. The user's full-scale 4-second sprint circle visibly moves Arno and
activates the owner-linked 459-record pose arrays (run 62); run 61's low-radius
circle did not move him. The live index/hash table now maps 444/459 rows to the
Arno render rig, with 15 unmatched rows and two parent mismatches. Run 63 now
captures graph leaves, playback time and 37 binding snapshots; 126/139 frames
pass pose double-read and stable-owner checks. The initial weighted-rotation
hypothesis is rejected: both captured output contexts are about 107–120° off
on the 21–23 jointly bound joints, while the mode-zero control is similarly
poor. First identify the final pose stage (neither context has the known
`0x0c000000` flag) or a required stage transform. Then decode the five
remaining codec-7/8 resources, repeat capture, and compare coherent
phase-aligned frames against Rust. Connect validated bindings to graph-selected
playback and compare root/facing on the same sprint arc.
Codec 7 still blocks crowd-avoidance decoding. See findings0025/0027; full
locomotion parity remains open.

ACU-008 binding checkpoint (2026-10-20): live runtime bytes and assembly confirm
the binding access and quaternion modes. Four walk samples at local-context flag
`0x0c000000` compare 21 mode-4/5 bound rotations to native within 0.0025 degrees
worst frame mean in the independent model. One flagged stop sample covers all
23 primary joints. The Rust install regression passes at 0.003931 degrees worst
frame mean across five samples; its mode-zero control mean is 48.629986 degrees.
`PoseRig::pose_with_bindings` implements the captured quaternion rows. It is an
evidence adapter; production graph selection and root/translation handling are
not switched over. Run 59 also captured 128 frames in the widened street, but
the graph's woman/man navigation resources leave actor identity unresolved; its
459-joint pose is excluded from Arno comparisons pending topology/owner checks.
See
[ACU-RE-0027](findings/ACU-RE-0027-native-binding-mode-pose-conformance.md).

Primary idle checkpoint: the existing Rust pose evaluator now passes an explicit
native fixture regression at recorded playback times (31 × 23 joints; worst
frame mean 0.002711 degrees, shifted-time control 0.622396 degrees). Selection
uses an independently observed context flag. ACU-008 remains active: next recover
the recorded start/stop/avoidance blend contributions and root/turn behavior.

Graph follow-up: run 49 now records resource IDs and playback times for recognized
graph leaves. Compare the identified idle and movement blends at native times;
resolve stage/timing outliers before treating double-read equality as coherent.
Finding0025 records the 332-sample passive idle control and direct-time errors.

ACU-008 latest checkpoint (2026-10-20): movement-linked owner now has a direct
421-joint pose chain; 404 hashes map uniquely to Rust. Runs 47/48 captured full
poses with root and per-sample ownership checks. Next resolve evaluation stage,
active graph/clip identity and held-out phase agreement before Rust tuning.
Two parent differences and 17 unmatched hashes remain explicit. See
[ACU-RE-0025](findings/ACU-RE-0025-player-root-full-rig.md). Earlier entries below
are historical, including the previously unresolved owner shortlist.

Statuses: ready, active, blocked (with reason), done. Keep one bounded task active
per worker. No delegation is required by this workflow.

## ACU-008 — Original animation tracks and live motion matcher

Idle/thigh follow-up: animated idle and eight authored leg-helper evaluations
are implemented and freshly inspected; finding0020 records the remaining native
equation gap. Animation audit imports 261,388 tracks, with unsupported codecs and
actor ownership explicit. The broad native probe/injector is implemented and
now attached to a sustained Ubisoft-launched game session. See
[probe commands and limits](../tools/native_probe/README.md) and
[research links](REFERENCE_LINKS_20261019.md). This does not complete traversal,
native raycasting bindings, Tailored selection or all-Arno ownership.

Status: active (2026-10-19), user-directed priority after the procedural rig test.
Current pose discovery (2026-10-20): exact topology signature matching and
metadata references reacquired 621 contexts matching the captured local pose
translations, including 43 that changed during passive polling. Actor identity
remains open. Next capture these current candidates through deliberate
movement/stop phases alongside fresh transform candidates, then verify a direct
player ownership relationship. Use the wider crowded square as the measured
route. The passive scanner installs no hooks; see
[ACU-RE-0024](findings/ACU-RE-0024-pose-signature-rediscovery.md).

User turn-review follow-up (2026-10-19): removed the idle-to-move facing snap
and slowed the authored walk/sprint facing limits to 90/135 degrees per second.
The focused acu_bevy test passes and a fresh 180-tick corner capture was visually
inspected. A live gradual-arc input/readback capture was also completed, but its
cluttered contextual movement prevents clean native turn-rate comparison. This
closes only the user-reviewed turn-in responsiveness issue; it does not establish
Unity steering or full locomotion parity. See finding0019 and the latest status.

Native pose follow-up captured the live 25-record output array from a byte-guarded
bone update site while verified walk/turn inputs ran. Live readback and the
routine's math establish 32-byte translation/quaternion records, parent links,
and child translation composition for the 25-entry subset. Quaternion
composition matches the source equation across 3,600 children in a stable
menu-state capture; dynamic captures remain sensitive to non-atomic reads.
The `0x13E137AF0` R9 context points to local pose data at `0x13E137C90`.
The live 25-entry hash table now maps each runtime joint uniquely to a Rust rig
row, replacing the earlier structural-only candidate. Character identity,
atomic phase sampling, root ownership, and a phase-matched Rust comparison
remain open. Rust reports include all matcher matrices, parents, and name
hashes; a verified four-direction follow-up captured 6,144 guarded updates
across 160 local-pose contexts, including 37 contexts using the 25-bone path.
The previously tracked owner again stayed fixed through input in the crowded
contextual scene, leaving player/Arno identity open. A 136-frame follow-up
read all 37 candidate 25-bone local arrays through those phases without read
failures; the previously mapped context changed in 131 frames and 21/25
records, but other candidates also moved. An exploratory comparison at a similar walk-input time still
had a `0.229 m` median root-relative translation error, with native clip and
actor identity unverified. All 25 mapped parent links now agree with the Rust
rig, and nonroot local translations match one saved Rust pose with p95 error
`3.73e-7`; rotations remain phase/actor unresolved. See
[finding0022](findings/ACU-RE-0022-live-bone-pose-buffer.md). The selected Rust
clip is decoded as resource `0000000fc08fd67a` (`xx_Nav_Walk_3-Loop`), with 23
mapped rotation tracks. Grid fitting phase and rate across the captured forward
series retains `33.0 degrees` median per-frame mean joint error and selects the
minimum tested rate. Five more directional variants were decoded; the best
across all inputs was the right-sway walk, still at `26.9 degrees` forward and
the minimum tested rate, including a similar score for backward and neutral.
The current live candidate is not phase-matched to a proven active clip.
An open-street capture then produced 229 identical full-region double reads
with verified walk/sprint/lateral/backward/neutral input. Arno is visible
running in the sprint screenshot, but the mapped owner transform stayed fixed,
so this pose context remains unlinked to the player. A new six-resource fit
again selected one left-sway clip for every phase, including neutral, at the
minimum `0.1` relative rate and `22.1-23.9 degrees` error; it does not identify
the active clip. Pose composition remains low-residual for translations, with
rare quaternion outliers. The direct actor/skeleton link, active clip/time,
root-motion ownership and matched-phase pose comparison remain open; no parity
claim follows. Capture and analysis provenance are recorded in finding0022.
Two synchronized open-plaza captures then found candidate pose-context pointers
inside moving transform-class object neighborhoods. A candidate pair moved
10.47 units with its local pose changing in 114/187 intervals, but a second run
had a 57.59-unit single-sample jump and a changed object set. These heuristic
links do not establish Arno/skeleton identity; parity remains open. See
finding0022.
Rebinding later found R9 context `0x151CFBFE0` inside a transform object that
moved smoothly under verified sprint and walk inputs and was visible in the
same captures as Arno. Its best six-clip walk fit still has `21.81 degrees`
median mean-joint error and does not consistently identify one active clip.
This is a stronger player/skeleton candidate, not a proven actor link or parity
result. Short repeat inputs at the dock were obstructed by a post; the failed
jump-back native trace was unhooked and excluded. See finding0022.

Live XInput follow-up (2026-10-20) sampled the 16-byte state and return value
at the ACU caller after its guarded XInput call. Nine current-revision samples
matched scripted right-stick/parkour inputs and eight were neutral; older ring
events were excluded. This verifies the returned controller state, not actor
movement. A separate 102-frame linked capture found 13 transform candidates,
with the largest displacement `0.3614 m`; multiple candidates moved in the
crowded plaza. No Arno owner or pose context is established. A lower native
hook sample rate avoided event-buffer overload. Continue with sparse bone and
post-call input hooks together across a cleaner repeat route. See finding0022.

Corrected pose discovery used `every=1` only to discover active dispatch
contexts, then sampled the 49 25-record contexts with read-only memory reads.
The forward/reverse replay found one moving-pose candidate, but neighborhood
matches remain non-unique and six-clip fits do not stabilize on one clip or
rate. A high-profile A + RT + forward replay visibly moved Arno over a railing;
its session faulted at the hooked bone site `0x988e3` (`0x80000004`). Exclude
that run from parity claims and do not reinstall the bone hook. Next use an
XInput-only hook profile and read-only pose sampling from a fresh Ubisoft-
launched session, establish an exact component/transform link, then compare a
matched phase against Rust before tuning. See finding0022.

The Ubisoft relaunch kept the same executable image as PID 16136. XInput1_3's
entry bytes changed, so the old guard rejected the profile; a derived XInput-
only profile now uses the live bytes and excludes the unsafe bone site. Slot 3
readbacks still show scripted states, while the partial native caller capture
showed only the persistent baseline. Some sessions ended before event drain,
and the current game remains responsive with hooks clear. Resolve caller/index
delivery and event draining in this process, then rediscover pose contexts
without the RVA `0x988e3` hook. See finding0023.

The next `every=1` trace found inconsistent input-source association: during
the runner's forward/backward replay, the caller mostly returned the persistent
`(-3356,-1869)` state. Two positive-Y samples appeared in forward, while 113
negative-Y samples appeared in backward. A passive five-second trace also
returned only the nonzero baseline while several owner candidates moved. These
windows are not neutral or clean controlled-motion evidence. Resolve caller
and device identity before using the trajectories for parity; details and
hashes are in finding0022.

The Bevy-independent `acu_motion::native_pose` record/hash-index decoders and
nonroot composer are implemented. All 58 acu_motion tests pass; package Clippy
passes with warnings denied.

Locomotion sub-slice now decodes original idle/walk/sprint clips and applies their
local joint transforms through the composed rig. The Unity adapter calls the
imported primary pose objective, matches transition phases, blends locally and
uses the same selected interval for root displacement. Low profile walks; held
high profile sprints. Directional follow-up adds lateral/backward walk and lateral
sprint takes, FH trajectory costs, continuous in-profile search and independent
facing. Direct scene/rig/navigation package decompression now uses Bevy
IoTaskPool with cooperative checkpoints every 32 blocks; startup still waits.
See findings0018/0019 and VALIDATION.md. The native mapped-image cross-reference
tool joins corpus VAs to unwind ranges as candidates; it does not verify installed
binary identity. Native traversal, actor ownership, complete FH dispatcher
adaptation and actual Tailored equipment remain open. See
[xref workflow](NATIVE_DECOMPILE_XREF.md).
Controller-first live capture is documented in [CONTROLLER_AUTOPLAY.md](CONTROLLER_AUTOPLAY.md).
The two-A menu route reached the Paris world. A 29-step ordinary X360 sweep and
byte-guarded XInputGetState trace completed; the capture launcher now offers 14
category checkboxes, with only input currently backed by a verified native site.
Per-button action semantics and traversal/raycast hooks remain open. The
standalone all-region dump stopped at 2.57GB during the turn interruption; its
manifest is explicitly partial. Completing a full-memory snapshot is
deprioritized: it is not movement-decision evidence or a prerequisite in the
reconstruction-first workflow in [MOVEMENT_PLAN.md](MOVEMENT_PLAN.md).
Port the sibling For Honor live matcher with reviewed source provenance, decode
Unity's original animation tracks and bone bindings, and drive root and pose
from one selected clip/time. Start with idle/walk/sprint, then upward/downward
traversal/climbing; retain held RT+X/Circle and right-stick camera controls.
Resolve the actual Tailored equipment references; the displayed Fearless set
does not satisfy that requirement. Acceptance: multiple original clips decoded
with independent checks, live selection and blended playback visibly inspected,
traceable clip/track/root/pose decisions and correct equipment identities. A
copied matcher or synthetic feature test alone does not complete this task.
Movement scope now follows [the reconstruction-first plan](MOVEMENT_PLAN.md):
build scenario/replay automation and inspect ACU world/traversal data before
implementing native-data-driven vault and climb decisions. Keep the current
matcher explicitly as an adapter until ACU ownership, selection and blending
are verified.

User-requested supporting slice: build a reusable animation database covering
all animation envelopes in the full coverage scan, their original identities,
role hints and evidenced actor/rig/track associations. See
[database schema and commands](ANIMATION_DATABASE.md). Unknown ownership must
stay explicit; this metadata index does not substitute for original clip playback.

Live input-routing iteration (2026-10-20): ViGEm reports target index 0 while
the calibrated pad is observable at XInput index 3. Run 30 confirmed the exact
forward, reverse and neutral `XINPUT_STATE` values at ACU's post-call site over
the visible Paris gameplay route; 170 samples at each XInput entry and 85 caller
returns were recorded. No sampled XInput1_3 entry met the conditional 0-to-3
redirect guard; the caller received input through its observed index-3 path.
This closes controlled input delivery for the
captured route. Earlier candidate pose captures used other processes and do not
identify the actor in PID 16136. Run 37 then replayed the same timed turn arc at
the verified slot 3 without a calibration pulse; input readback and caller
snapshots followed the arc, though the caller's pre-phase state retained an
unrelated nonzero baseline. Open-bridge screenshots show visible turning, not
measured root or facing. A Rust `turn-arc` scenario now matches the 60 Hz phase
schedule and writes per-tick yaw/root rows. The locked offline Bevy build and
format check pass; a fresh 516-tick synthetic capture shows a maximum 1.5°
facing step and was visually inspected. During run 38, 28/63 candidate
transforms passed the read filter; the most mobile moved 0.6401 units but has
no proven player or pose link. A forward/reverse follow-up moved that candidate
only 0.0061 units, while another drifted under nominal neutral input. Screenshots
showed Arno traversing a bridge edge, so the route is excluded from ordinary
locomotion evidence. These captures quantify the test-world response and
candidate rejection only; native turn/root parity remains open. Runs 40-43
repeat the gradual arc in the wider but crowded square, with pedestrians and
carts crossing the route. The run-40 mover did not repeat: a different candidate
led run 41 and its phase displacement directions were weakly aligned. The
pre-phase slot state remains `(-3356,-1869)`, so it is not a clean neutral
baseline. Two valid 800-byte candidate arrays remained static throughout run 43
and are not verified bone buffers. Next: identify Arno's live root transform
and actual animation/bone buffer, then compare matched locomotion and
crowd-contact phases against Rust. Treat crowd and carts as measured constraints.
Hooks are clear. See
[ACU-RE-0023](findings/ACU-RE-0023-xinput-caller-routing.md).


## ACU-007 — Controllable rig validation build

Status: done (2026-10-19), bounded rig-validation slice within ACU-005.
Acceptance: original meshes, bind matrices and complete four/eight-slot weights
drive visible deformation; controller left stick, RT high profile, RT+X up,
RT+Circle down and right-stick yaw/pitch, plus keyboard/reset equivalents work in
the synthetic arena; lighting, shadow and shader paths render in fresh captures
and the DX12 window. Preserve source/binary/asset identities and fixed replay
inputs, run the workspace gates, and record every unsupported attachment,
native animation/material gap and unresolved Tailored/Day Paris selection.
Authored test movement is specified in mechanics/test-world-movement.md;
rig evidence is finding0017. No native behavior or shading parity is implied.

Result: 51 authored tests, three explicit skeleton install tests, fmt, clippy
with warnings denied and workspace build pass. Nine fresh offscreen captures
and a DX12 window review cover visible deformation, movement, jump, reset and
camera controls. Controller held-button combinations, release/disconnect/focus
loss and both camera axes pass automated tests; physical button testing remains
unavailable. Gait foot contact, original clips/traversal, the unresolved weighted
shoulder, Tailored selection and Day Paris remain subsequent work.

## ACU-006 — Prepare collaboration and a source-only publication candidate

Publication follow-up (2026-10-19): user authorized hosting under Rdev.
Published the reviewed source-only snapshot to
[rdev-rna/AU-RW](https://github.com/rdev-rna/AU-RW) from the independent
`C:/Users/PC/source/AU-RW` checkout with fresh history and Rdev authorship.
Acceptance: exported workflow/Python checks pass, committed paths match the
source manifest, remote main matches the local commit and the repository link
is accessible. No game/corpus/local data or parent history; no new license.
Result: complete. Both checkouts pass workflow checking and ten Python tests;
94 reviewed paths are committed under Rdev, GitHub author resolves to `rdev-rna`,
public main matches the pushed commit and onboarding is accessible.
Earlier preparation-only and publication-unset notes below are historical.

Follow-up (2026-10-19): user selected **AU-RW** as the project/repository title.
README and contributor entry points use that name. The portable
[onboarding prompt](COLLABORATOR_PROMPT.md) covers context, local setup, task
coordination, evidence, implementation, validation and completion. The master
prompt no longer directs newcomers to restart completed ACU-001. Acceptance:
linked and bundled onboarding instructions; workflow check and authored Python
suite pass. Hosting destination, license and publication remain unset.

Status: done (2026-10-19), bounded preparation only. User-directed side slice
before resuming ACU-005's rig/reference work. No public upload authorized or made.

Validation: ten Python tests pass in both working tree and clean source export;
workflow check passes 21 required files. Doctor/inventory pass against this
machine's ignored configuration (217,041 corpus files, 59 top-level archives,
10 DLLs; executable hash unchanged). The clean export correctly reports absent
inputs, then doctor passes with external paths and no sibling checkout.
PowerShell launcher syntax and configuration resolution pass; no scene or Rust
code changed. The final local candidate is
`local/unity-source-review-final-20261019.zip` (93 source files plus manifest).

Acceptance:

- Collaborators configure their own game and decompiled corpus directories;
  no machine-specific defaults or sibling checkout requirement.
- The scene launcher resolves local configuration as well as explicit overrides.
- Contributor instructions describe build/evidence gates and excluded game data.
- Produce a reviewable source-only ZIP with file hashes, excluding decompiled
  files, proprietary data, machine configuration, build products and history.
- Verify configuration precedence and export boundaries with authored tests,
  and preparation commands against the current machine's ignored settings.
- Keep the parent repository unchanged; defer host/repository/license selection
  and publication until the owner approves the concrete candidate.

## ACU-000 — Prepare the workspace

Status: done. Instructions, master prompt, workflow, templates, inspection tools
and initial inventory are present. See `docs/STATUS.md` for validation results.

## ACU-001 — Establish Forge compatibility

Status: done (bounded compatibility investigation, 2026-10-18). Depends on ACU-000.
See [ACU-RE-0001](findings/ACU-RE-0001-forge-compatibility.md).
Plaintext container reuse validated for four archives; compressed codec/provider
selection remains an explicit implementation boundary. No decoded-read support claimed.

Acceptance:

- Identify representative ACU base/patch archive byte ranges and preserve hashes.
- Compare container, tables, entries, encryption and compression with the For
  Honor runtime reader; mark supported, different and unknown separately.
- Establish at least one entry boundary from a base archive and one from a patch
  without trusting arbitrary counts/offsets. Relate an extracted payload if possible.
- Record required codec/provider and reuse source provenance; do not claim decode
  support before an independently checked payload succeeds.
- Produce a finding and a bounded implementation plan, including unresolved
  blockers and parser limits. No full-install extraction is required.

## ACU-002 — Bootstrap Rust and the bounded reader

Status: done for the runtime asset foundation (2026-10-18). Workspace and acu_assets
exist; base/patch vectors independently checked, bounded in-memory decompression,
CLI and format tests implemented. User prioritizes comprehensive runtime decoding;
empty app/simulation crates are deferred. See ACU-RE-0002 and ADR-0001.

Acceptance:

- Verify official current Rust/Bevy compatibility; record pins, toolchain and lockfile.
- Adapt reviewed parser modules into this project with provenance and explicit variants.
- Add a CLI probe that lists entries and reads a specified entry with hashes;
  no hardcoded install path, no install mutation and no bulk decoding by default.
- Prove base/patch reads with identified real samples; cover invalid input and
  allocation/cycle limits with authored fixtures. Document unsupported codecs.
- Pass Cargo gates; document one repeatable probe command and its actual output.

## ACU-003 — Bind the pseudocode export to the native build

Status: ready; can be investigated independently of archive decoding.

Acceptance: identify the export source/build if available; bind an ACU analysis
session by executable hash; verify several exported function locations/signatures
against native bytes; document image base, VA/RVA mapping and gaps. Do not rename
or regenerate the original corpus. If its source build cannot be established,
retain the mismatch/unknown status and limit behavioral claims accordingly.

## ACU-004 — Display one identified asset

Textured extension complete (2026-10-19): eleven total pieces (nine outfit,
head and hood-up hair), nineteen draw parts, eight materials, seven diffuse
textures. Three offscreen angles and a live DX12 window were inspected; no
unbound part or missing texture was reported. `tools/run_arno.ps1` launches the
synthetic grey world with orbit/zoom controls. See ACU-RE-0013 and
`local/captures/acu-arno-textured-20261019/capture.json`. This supersedes the
older grey-only material gap below; skinning, animation and native shaders stay
open.

Status: done (2026-10-18) for the first Arno character slice, re-validated after
the ACU-RE-0012 clustered-descriptor decode fix. The acu_bevy
inspection app loads eleven meshes (nine outfit, head and hair) directly from
the installed `DataPC_ACU_LGS_BelleEpoque.forge`, prints per-piece provenance
(archive, entry, resource, file ID, entry and payload SHA-256, format/stride,
vertex/triangle counts) and renders a grey synthetic arena at 1280x720 from
three orbit angles (`local/captures/acu-clusters-20261018/acu-arno-35.png`,
`...-145.png`, `...-260.png`; the pre-fix captures are `local/acu-arno-35.png`
etc.). See [ACU-RE-0011](findings/ACU-RE-0011-clustered-vertex-decode.md) and
[ACU-RE-0012](findings/ACU-RE-0012-clustered-mesh-layout.md). The scene
claims no original-world, material, lighting, animation or axis parity; winding
is unverified, so meshes render double-sided.

Acceptance: identify a bounded mesh/material resource chain, document transforms
and display it in a repeatable Bevy inspection scene; preserve source IDs, hashes
and decoder provenance; save and inspect a fresh capture. The mesh, material and
diffuse chain and the explicit presentation transform are now documented and
captured by ACU-RE-0013. General native transform/material parity remains open
under ACU-005.

## ACU-005 — Complete runtime asset parsing

Status: active. User success criterion: every asset parses, including models,
maps, animations and textures, for runtime use directly from the original install.
See [asset coverage acceptance](ASSET_COVERAGE.md). This objective is not achieved.

Current slice: in-memory container/frame/resource readers; typed controls (63 global
metadata + 63 prefetch indexes), all 65,466 TextureMap records (65,464 with inline
pixels, 2 typed descriptors without pixels), meshes including the cluster-only
geometry variant, the clustered vertex attribute decode and the clustered
descriptor/draw-primitive layout (ACU-RE-0012), simple skeletons and the selected
Arno Material/TextureSet chain (ACU-RE-0013). No complete material census yet.
The humanoid extension (ACU-RE-0014) now parses 19/28 bootstrap rigs, including
the full male/base skeletons; nine remain rejected. Bind transforms, skin remaps,
pose groups, additional modifiers and animation codecs remain active work.
The movement mission is sequenced in MOVEMENT_PLAN.md / PROGRESS.md. Required
defaults are Arno's Tailored hood/coat/gloves/greaves and Day, Paris lighting;
their native asset selection remains unresolved. LIGHTING_RESEARCH.md and
RENDERING_SOURCES.md track the reviewed video and developer rendering reference.

Completed bounded numerical skin audit (ACU-RE-0015): eleven pieces, 29,481
vertices, 19 parts; all weighted hardware-palette accesses and weight sums pass.
105/397 active mesh-bone entries match the male rig; 292 stay unresolved. Four
matrix storage/order hypotheses support a candidate inverse-bind relationship
for the matched bones; blade/hair differences are retained. Acceptance for this
slice: reproducible reports with byte identities, explicit unknowns, independent
matrix controls and invalid-input tests. No skinning or visible-rig gate passed.
Reference follow-up (ACU-RE-0016): ten selected BuildTables decode in the Python
diagnostic, including the eleven rig-reference row and the specific head/rig
association. The Rust head rig reads 134 bones / 106 wrinkle inputs and one pose
group (147 poses / 2,526 packed bone records). Bootstrap rig support is now 20/28.
110/116
active head bones match numerically. Row/default selection and root attachment
transforms remain unknown; this is not completed additive composition.
Next movement slice: follow VisualMaster table `00000012ca6b42bf`, resolve
the eleven rig payloads and remaining head hashes, then additive rig composition, inspected
bind-pose hierarchy and controlled deformation before animation playback.

No asset extraction pipeline; no extracted folder or AnvilToolkit
dependency in the runtime library.

Next: close remaining package/control-entry variants and metadata extensions,
then expand typed textures/external mips, mesh/skeleton, world and animation readers
using the coverage report to choose representative variants. Preserve all failures.
Cluster bit-mask semantics, general cluster-to-submesh/bone mapping and native
axes/units stay explicit unknowns. The inspected Arno instancing records now bind
materials by matched draw/vertex counts and UV0 gives a coherent diffuse render;
this does not establish universal mesh/material or texture conventions.

Acceptance:

- Enumerate original Forge archives including DLC; record additional asset carriers.
- Resolve all container/control-entry, framing and resource metadata variants.
- Implement typed models/skeletons, world/maps, animations, textures and every
  remaining resource type; resolve references and required patches explicitly.
- Validate real install inputs with independent vectors and malformed-input tests.
- Complete full scans with zero unknown types, failures, omissions or opaque
  placeholders counted as parsed. Decompression alone does not satisfy this gate.
- Prove assets are usable through the runtime API and subsequently rendering/playback.

The ACU-RE-0011 inspection slice satisfies the first half of the last bullet
(decoded meshes are usable through the runtime API and render), and ACU-RE-0012
extends it to the typed cluster descriptors, the primitive-relative index bases
and the desk/bed/rock renders. ACU-RE-0013 adds diffuse texture presentation for
Arno; animation and native material playback remain open.
