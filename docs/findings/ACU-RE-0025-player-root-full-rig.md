# ACU-RE-0025 — Movement-linked root and 421-joint component chain

Date: 2026-10-20. Task: ACU-008. State: partially established.

## Provenance

ACU PID 16136, image base `0x140000000`, installed executable SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`.
All evidence paths below are relative to
`local/native-probe/motion-link-20261020/`. External ReadProcessMemory sampling;
resident probe transport for status and mapped dump. Hooks clear throughout.

Direct IDA Pro MCP session `6e46e1a9`, headless, global autoanalysis disabled,
analyzed a separately derived runtime image because the original disk PE returns
zeros at the relevant code addresses. Session saved and closed; database:
`runtime-view/ACU.runtime-analysis.pe.i64`. Source capture is not atomic.
The derived analysis view is not runnable; imports/runtime patches remain.

- Mapped image: 125,980,672 bytes, no read failures; SHA-256
  `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`.
- Derived view SHA-256:
  `47a81dc9f29c48a709cbdc1ef60a7e7db9eb8caefaabf30940dd4e1ee755d8c2`.
- `runtime-view/analysis-view.json` records header changes/provenance.
- Live/IDA byte guards agree at VAs `0x1400988e3`, `0x1401040a0`, and vtable
  slot `0x142e7f9a8` (`native-code-guards.json`).
- `runtime-view/ida-component-method.json`: VA `0x1400e9cc0`, RVA `0xe9cc0`,
  166 bytes; confirms component `+0xd0/+0xd8`, state `+0x280/+0x2a8`.
  Corpus `ACU - Decompiled/1400E9CC0.c:9-24` agrees.
- `runtime-view/ida-bone-count-method.json`: VA `0x14009a7a0`, RVA `0x9a7a0`,
  66 bytes; loads context `+0x170`, then WORD at metadata `+0x5a`.
  Corpus `ACU - Decompiled/14009A7A0.c:10-18` agrees.
- `runtime-view/ida-anim-eval.json`: VA `0x1401040a0`, RVA `0x1040a0`,
  471 bytes; controller graph state `+0x20e0`, auxiliary `+0x20e8`.

These are focused instruction correspondences, not a universal export/build
mapping. Partial IDA pseudocode produces JUMPOUT stubs; use saved assembly.
`evidence-hashes.json` identifies saved analyses, static arrays and tables.

## Observations

Fresh owner-vtable scans found over 21,000 candidates. Runs 44/45 replayed two
2-second forward walks separated by 2-second stops. Owner `0x1a6ab09c0` follows
both walks and settles during stops in both runs. Run 44 walk displacements:
0.124/2.101 native units; run 45: 2.861/2.368. Crowd/carts constrain traversal.
Trace SHA-256s:

- Run 44: `463a28b527e58961055630f2060073bb765fcf5e345e8702098f194729719ab0`.
- Run 45: `3c4e42afa09a015337276ab76517a04097f25154aa71fd62f8fef2c2029dbe9c`.

An exact root-pointer scan examined 5,916,524,544 readable bytes through query
end `0x7fffffff0000`, finding 607 references. Component checks and native
assembly establish this chain:

| Object | Observed fields |
| --- | --- |
| Owner `0x1a6ab09c0` | Transform `+0x20`, position `+0x50` |
| Animation component `0x1a6c7b480` | Vtable `0x142e7f780`; owner `+0x10`; controller `+0xd0 = 0x191779af0`; skeleton component `+0xd8` |
| Skeleton component `0x1a6c79dd0` | Same owner `+0x10`; local context `+0x280`; output contexts `+0x288/+0x290` |
| Local context `0x1a4a24230` | Metadata `+0x170 = 0x1a6b0e490`; array `+0x178 = 0x1a834f330` |
| Outputs `0x1a6ba50e0`, `0x1a6baeb90` | Same metadata; arrays `0x1a83567b0`, `0x1a8352d70` |

Output slots swap. Run 46 rejected an overly strict fixed-order guard; its
controller runner completed and neutralized. Runs 47/48 validate the output set
and direct ownership chain every sample. Metadata count `+0x5a` is **421**.
Name table `+0x38 = 0x1a833f380`; topology `+0x68 = 0x1a833df50`, 12-byte
stride. Metadata pair at `+0x70` is 430/421: the old equality heuristic excluded
this rig. Conventional length/capacity semantics are unproven.

Local context header XYZ equals owner position exactly in a stationary read:
`(-173.374420,607.066711,-0.447044879)`; context W0, owner W1.
Arrays contain 421 × 32 bytes. The 8-byte name rows were interpreted as 32-bit
hash, 16-bit index, 16 opaque bits: low indices cover 0..420; 147 upper fields
are nonzero with unresolved meaning. Do not silently transfer the earlier
25-joint decoder's 32-bit index interpretation.

404 hashes map uniquely to Rust's saved 721-joint report; 17 are unmatched.
Two mapped parent links differ (native indices 102/360). Across 401 matching
nonroot parent pairs, translation residual median is `1.85e-7`, p95 `0.00570`,
maximum `0.04676` native units. `player421-mapping.json` preserves every mapping,
mismatch and Rust report identity. Names: 3,368 bytes, SHA-256
`b7bd640cee1f87a103639e5d2c0cb308f3271d5ed247fb7480331a7d99eca3fb`;
topology: 5,052 bytes, SHA-256
`71deba6628e24851a580a32ee1ce583f09ea0e878206560bdf9e58e9b1574b68`.

### Full-scale sprint-circle follow-up (2026-10-20)

With the user's Arno-specific sprint cue, run 62 replayed a four-second,
full-scale (`~26,000/32,767`) 16-phase stick circle with right trigger held at
255, followed by a one-second stop. The controller readback confirms slot 3
received the complete profile. The original-game screenshot shows Arno moved
along the curb beside the lamppost; the HUD distance changed from about 130 m
to 127 m across the preceding low-input and full-input captures. The same
animation-component owner transform `0x1a6ab09c0` moved through 17.45 captured
transform units, with 3.85 units net displacement during the circle. These are
transform-space measurements; no unit or axis conversion is claimed. The
10,000-magnitude circle in run 61 produced no measured owner-transform travel,
consistent with the user's observation that Arno had not moved.

Run 62 captured 132 frames with one owner and two output pose arrays; maximum
sampling span was 6.02 ms. Both arrays used 459 records (metadata `+0x5a`),
with observed flags `0x80000000`, `0xa0000000`, and `0x88000000`. This is a
different live count from the earlier 421-record capture even though the
animation/skeleton component chain and owner are the same. The captured
quaternion lanes changed throughout the sprint phases. The user identified
sprint as an Arno-exclusive action; delivered sprint input, visible Arno
movement, owner-transform travel and activity in both component-owned pose
arrays coincide in this capture. Actor association for the owner and 459-record
pose pair is therefore resolved for run 62. This is a controlled correlation,
not a textual actor label read from the game. The 459-row hash/name-to-mesh
mapping is not recovered, and the separate graph-leaf reader reported `graph
count outside bounds` in all 132 frames. Per-bone Rust parity is still open.

Capture provenance: PID 16136; ACU image SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, image
base `0x140000000`; no hooks active. Profile:
`local/native-probe/motion-link-20261020/run-62-profile.json`; trace:
`local/native-probe/motion-link-20261020/run-62/motion.bin.gz`, SHA-256
`28749f197e914190671b087d815d2cb8f90a38964153005c9936c54be50c68c6`;
summary:
`local/native-probe/motion-link-20261020/run-62/summary.json`. The capture
reader records per-frame context pointers, flags and bone counts using the
`ACUPOSE02` variable-count format.

### 459-record name/topology mapping

The run-62 live metadata was reread from the same animation/skeleton component
after the sprint capture. Metadata `+0x38` points to the 8-byte joint-name/hash
rows; `+0x68` points to the 12-byte parent rows. The count is 459 at `+0x5a`,
and the neighboring pair at `+0x70` is 464/459. Both tables and the header
matched on two consecutive reads. This is bounded `ReadProcessMemory` evidence;
it does not make the pose samples atomic.

All 459 native row indices are unique. Comparing row hashes with the existing
721-joint Arno render-rig report maps 444 uniquely, leaves 15 hashes unmatched,
and finds no ambiguous hash. Two mapped parent relationships disagree with the
Rust rig (native rows 131 and 393). The mapping is now sufficient to compare
444 named Arno joints; preserve the two parent discrepancies and 15 unmatched
rows rather than forcing a match.

Metadata/table provenance: metadata VA `0x1aabcf2f0`; name table VA
`0x1b71818c0`, 3,672 bytes, SHA-256
`59bf1737bbcf7453896407726031dd7d2b23c7721410a38881f026d902779d2e`; topology
VA `0x153b7ea10`, 5,508 bytes, SHA-256
`ebd1bcb20581d2f7bf7789547da86b35f05691a78c7aa86ed22451b6182184d5`. Rust rig
report SHA-256 remains
`bbb1f24f1519a5b166d68b18c21930e9e8b56723695db8679b64fd3b80653c64`. The
reproducible mapping is in
`local/native-probe/motion-link-20261020/run-62/rig-mapping-459.json`.

### Run 63: live graph and pose consistency

Run 62's graph failure came from reading the controller left by an earlier
scene. The animation component at `0x1a6c7b480` owns its current controller
through `+0xd0` (`0x1a6c7b550`); run 63 followed that pointer and read its graph
list at controller `+0x6a8`. The four-second full-scale circle profile from
run 62 was replayed. The reader captured 139 frames, graph counts from 5 to 25,
and no graph or leaf errors. It recognized sprint `000000016737c2c4` and
directional sprint `0000000f9f512b7f` playback leaves, alongside idle
`00000003836e1d49`, with playback times and weights. It wrote 37 distinct
binding records. These IDs and labels are corroborated against the existing
asset catalogue/findings; interpreting the binding fields still needs the
remaining validation.

For each 459-row pose array, run 63 reads the full array twice and marks byte
equality. It also compares the owner matrix immediately before and after the
pose reads. 126 of 139 frames pass both checks; 135 pass pose equality alone.
This rejects observed mid-update changes, but does not prove an engine-lock or
evaluator-boundary atomic snapshot. Maximum overall capture frame span is
40.56 ms, so use the explicit per-frame flags instead of treating every row as
phase-exact. Raw data and reports:
`local/native-probe/motion-link-20261020/run-63/motion.bin.gz`,
`graph.jsonl`, `bindings.json`, `manifest.json` and `summary.json`; trace
SHA-256 `68db2b7db3f8d428d90e508cb3a57298db4d81895a4bbf15cdbf27d1a38fc7ee`.

The capture establishes which live playback leaves and times coincide with
the Arno-linked 459-row pose arrays. The first blend comparison used 150
coherent pose samples and 21/23 primary targets per sample. Its
hemisphere-aligned normalized weighted-local-rotation hypothesis scored
roughly 107–120 degrees against both captured contexts; forcing binding modes
to zero was similarly poor (104–121 degrees). This rejects the current
comparison, not actor association or clip/time evidence. Neither run-63
context has the previously correlated `0x0c000000` local-stage flag. The
output evaluator stage or a required transform remains unresolved; do not use
these numbers as a Rust parity score. Five of 34 observed resources remain
undecoded because the current decoder rejects codecs 7/8. Locate the final
pose stage, then compare coherent frames and report the 15 unmapped and two
parent-mismatch rows separately.

## Root-and-pose replay

Runs 47/48: slot 3, no calibration pulse; neutral 1s, `[0,14000]` 2s, neutral
2s, repeated walk/stop. Initial readback remains `[-3356,-1869]`; later stick
readbacks match the script including final zero. Initial baseline is unexplained.
Inspected screenshots show Arno traversing beside pedestrians; run 48 has a
foreground pedestrian. This is a crowded route, not an unconstrained speed test.

| Run | Samples | Max read span | Walk net distance | Stop path |
| --- | --- | --- | --- | --- |
| 47 | 222 | 15.034ms | 1.794 / 1.717 | 0.0253 / 0.0223 |
| 48 | 226 | 3.000ms | 1.731 / 1.744 | 0.00818 / 0.0373 |

Distances cover phase interiors with 100ms margins, not complete start/stop
motion. All 421 joints in all three buffers change. Run 47 records are finite,
with unit quaternions throughout. Output translation W stays zero; local W
sometimes reaches one. Local evaluation stages/update boundaries remain unknown.
Quaternion diagnostics validate rotations separately from local composition.

- Run 47 trace SHA-256:
  `3649e7bdcaab4acaa9e95a49e033da917ff4ae5e4a7b5d4e2561fbd38e6ed45a`.
- Run 48 trace SHA-256:
  `a117ccedefd23ac17a14f2f4a2a49cb312e1cb00dc0db4a418db08a73a41e04f`.

Six decoded walk variants were fitted to 23 primary nonroot rotations by hash:
60Hz shortest-path slerp phase grid, rate 0.1..2.0 in 0.025 increments, minimizing
median per-frame mean angular error independently per phase. Walking best errors:
run 47 **9.426/9.603 degrees**, run 48 **9.517/9.382 degrees**, rates 0.70..0.85.
Bank-left/right variants vary. Stops retain 11.19..11.46 degrees at rates
0.10..0.125. `run-47/clip-fits.json` and run-48 equivalent preserve clip file IDs
and hashes. This is an unconstrained fit; active clip and phase continuity are
unverified. No Rust tuning follows from these errors alone.

## Interpretation, falsification and next step

Repeated input response, direct ownership, header/root agreement and rig hashes
strongly support the controlled player's skeletal chain. Independent player-manager
identity remains absent; rig hashes alone also fit related characters. Next
validate primary-joint output/local composition at a consistent update boundary,
recover native graph/clip identity and test fitted phase on held-out motion.
Keep unmatched joints and helper-parent differences explicit. Parity remains open.

## Reproduction and validation

### Graph/playback follow-up

Run 49 additionally saves the 24-byte graph rows at controller `+0x6a8` and
recognized leaf playback objects, all by external reads. There are 227 samples,
zero graph/leaf read errors. Root/pose trace SHA-256:
`8aa6ef6bf111ce6e1f368633d41a6a195199a57d3fe6921a098167d6c912fa90`;
graph JSONL SHA-256:
`b436577c44d15ae973a34dc90b814803764c51a7492219cf4d9a221c27f4a7a9`.

Focused IDA session `31ff5647` confirms these accesses in the same runtime view:
VA `0x1400bcc20` (RVA `0xbcc20`, 406 bytes) reads the node WORD at `+0x64`,
uses the graph row's state and its `+0x260` pointer to index playback objects,
and evaluates the pose via `0x140017df0`. VA `0x1400315b0` (9 bytes) reads
playback float `+0x384` (900); `0x140020840` (12 bytes) adds float `+0x36c`
(876). Corresponding saved analyses are `runtime-view/ida-playback-leaf.json`,
`ida-playback-time.json`, `ida-playback-offset.json`. Corpus exact files agree.
Native `14001BE40.c:108-115` also uses their sum for evaluation. This is
focused code evidence; the asset-handle ID field interpretation is additionally
supported by catalogue/pose agreement below, not a complete handle decoder.

Recognized graph leaves use vtable `0x142e80690`, method `+0x58 = 0x1400bcc20`.
At idle: node `0xfc0f0130`, state `0x9a2f300`, node slot 1, playback array
`0x9a2f570`, playback `0xa11f010`, asset handle `0xf4134718`. The handle's
`+0x10` contains ID `00000003836e1d49`. Catalogue occurrence 3319 identifies
`xx_Nav_IdleLow_Footr`, DataPC.forge entry 928, resource 1772, decoded package
range 62000352..62057714, payload SHA-256
`dbac8b4f36c5bc1fc803bfe47706122ba53d9e0dea547c980dbf527f35959a62`.
Archive entry offset 101271720, stored length 123303092, stored SHA-256
`08b0a18bd2fa603ae2b77f2b17587c4f41c7154050552b5874edcd8c2bceda00`.
`run-49/graph-summary.json` preserves occurrence identities for all 12 observed
IDs, including walk/slow-walk, bank-right, slope, avoidance, start and stop clips.
Presence does not prove final blend contribution; graph weights still need
native evaluation semantics. Kind 3/4 rows are not ordinary vtable objects.

The existing `acu-mat` tool decoded the identified idle with full source
provenance (`idle-footr-decoded.json`, SHA-256
`8243b41deccfe7537aabb732dbb9b1975dcadca3551fb117ce9332b7448e7341`).
Comparison at **recorded time900 + offset876**, without fitted phase/rate,
gives median per-frame mean error **0.570 degrees** over 16 pre-phase samples
and 23 primary joints. There is an 11.68-degree outlier; no parity claim.

An independent four-second passive capture
`stable-idle-1792443204649946500/` accepted 332 of 352 attempts with identical
double reads of the contiguous 43,296-byte three-buffer span and unchanged
graph rows/playback times (max span 2.013ms). Fifteen pose-change and five
graph/time-change attempts were rejected. Sample SHA-256:
`43501034b11395eb38d4a207de526e71f1819f54350709ec2192314201c37085`.
At direct recorded time, 85/332 samples have mean primary-joint error below
0.01 degrees; overall median 0.454 degrees, p95 10.770 degrees. Filtering only
by an independent composition check (minimum of two output mean residuals
below 0.01 degrees) retains 229 frames: median 0.555, p95 1.017, max 1.045
degrees. Double-read equality therefore does not establish an atomic evaluation
boundary. Timing/pose stage remains the next specific gap. The detailed reports
retain all samples and errors, including rejected agreement hypotheses.

### Repeated context-flag association

A second four-second capture adds identical context-header reads:
`stable-idle-1792443347261563300/`, 317/347 accepted, source SHA-256
`ddfc72c843cd1cd8ff6f5cc4eea24c6b311f5855d902b44a1441d33c7202ab36`.
Eight samples whose local context DWORD `+0x8c` is `0x0c000000` all give
direct-time primary-joint mean error 0.00129..0.00219 degrees. The other
observed local value is `0x04000000`; it includes large outliers. Native
`0x14009a7a0` clears `0x0c000000` bits on its output after composition, but the
full meaning of those flags/stages is not yet recovered.

A separate 16-second repeat, `stable-idle-1792443377692104700/`, accepts
1,278/1,389 attempts. Source SHA-256:
`bd59894040a7fc9c0bfc5f9242e6f6199c6ee0c71dfce536c257c9464a84738a`.
Selecting by the same **flag alone**, independently of clip residual, gives
31 samples at 31 distinct playback times. Every sample's 23-joint mean error
is below 0.00272 degrees; output composition mean residuals stay below
`4.4e-6` degrees. This repeats the stage association; it does not prove an
atomic boundary or cover the final deformed pose.

`export_idle_regression.py` preserves that independently selected subset in
ignored `rust-idle-regression.json`, SHA-256
`3055ae8e6ba0f351c7f3415f56f34b1b5c05e25d7ca6f6a47ef18969d8e85f80`.
The explicit ignored Rust install regression now evaluates the existing
`PoseRig::pose` at those recorded times, checks clip/executable hashes and
mapped parent hashes, and includes a +0.3s phase negative control. It requires
`ACU_GAME_DIR` and `ACU_NATIVE_IDLE_FIXTURE`; unavailable data cannot pass.
No capture bytes are added to source. Explicit execution **passes**: 31 frames
× 23 joints; worst frame mean **0.002711 degrees**, +0.3s negative-control
mean **0.622396 degrees**. Log:
`rust-idle-regression-no-incremental.log`. The first incremental build was
cancelled after apparent lack of compiler progress; its owned compiler/linker
were stopped. The successful retry used `CARGO_INCREMENTAL=0`, took 2m59s
including the initial build-lock wait, and executed the test in 20.42s.
No product behavior changed: this validates the existing Rust evaluator for
the declared primary idle subset, not final-pose or locomotion parity.

Local scripts: `capture_walk_stop.py`, `capture_player421.py`, `map_player421.py`,
`analyze_player421.py`, `inspect421_trace.py`, `fit_player421.py`. Execute as
`python -m local.native-probe.motion-link-20261020.<module>`; capture/analyze/fit
accept `--run`. Capture refuses existing directories. Addresses require fresh
discovery after restart. No hooks were installed; do not reinstall the crashing
bone-dispatch hook at RVA `0x988e3`.

Reusable `tools/native_probe/runtime_analysis_view.py` preserves mapped source
and emits separate remapped PE section raw spans. Five synthetic unit tests pass
(headers, truncation, alignment, overlap, payload preservation). Scanner hit-cap
resume control over 12KiB found 3 references; capped 1 + resumed 2 exactly match
the full scan (`resume-control-result.json`). Multiple-pattern overlap/resume
coverage is still limited. Syntax checks pass. The only Rust addition is the
explicit native idle regression; product behavior is unchanged. Workspace gate
results are recorded in `workspace-gates.json`. The original game's fresh
screenshots were inspected; no new Rust visual behavior is claimed.
