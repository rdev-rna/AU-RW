# ACU-RE-0026 — Graph-selected clip coverage and binding transforms

Date: 2026-10-20. Task ACU-008. State: partially established.

## Provenance

Same ACU PID 16136 and base `0x140000000` as finding0025. Executable SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`.
Evidence root: `local/native-probe/motion-link-20261020/`.
All sampling is external read-only memory access with hooks clear. Known
graph/owner path and runtime analysis image identities are in
[finding0025](ACU-RE-0025-player-root-full-rig.md).

## Asset decoding coverage

Run 49's observed resource handles identify 12 clip IDs. Catalogue occurrence
lookup preserves archive, entry offset/length/hashes, resource index, payload
range/hash for each in `run-49/graph-summary.json`. All resolve to DataPC.forge
entry 928, whose stored bytes and decoded package identities are in finding0025.
`decode_observed_graph_clips.py` invokes the existing `acu-mat` decoder for that
bounded ID set and independently checks each payload hash against the catalogue.
`graph-clips/manifest.json` records decoder hash, report hashes, lengths/durations
and explicit failures. Eleven decode successfully. The remaining asset is
`00000007fddf8cda`, resource 4204, `xx_Nav_WalkFwd_AvoidLeft`: unsupported
animation **codec 7**. No substitute values or partial success are asserted.

## Direct-time blend hypothesis

`compare_graph_blend.py` tests a labelled hypothesis: normalized, hemisphere
aligned weighted sums of decoded local rotation samples at the observed native
leaf times/weights. No phase/rate fit. Missing clips/tracks and weight sums
different from one are rejected. Native blend order, masks and equation have
not been established by this experiment.

Run 49: 159/227 frames compared; 21 reject the undecoded avoidance clip and
47 reject incomplete primary hash coverage in two starting clips. There are
no eligible walk-a frames. Twelve walk-b frames remain (11 with multiple leaves):
median frame mean error 0.404 degrees. Restricting by independent output/local
composition agreement (<0.01-degree mean in either output) retains seven,
median 0.459 degrees, maximum 0.941 degrees. Stop-a's corresponding 22 frames
have median 0.597 degrees; stop-b's 16 frames retain **10.197 degrees** median.
The hypothesis therefore does not establish movement parity.

The two stopping playback objects differ in byte `+894`: stop-LC = 4 in all
48 recorded appearances; stop-LP = 5 in all 48. This is a useful correlation,
not proof that toggling that bit alone explains the residual.

## Binding discovery

Relevant corpus paths and bounded accesses:

- `ACU - Decompiled/14001BE40.c:153-172`: reads 4-byte channel/track binding
  rows; low 13 bits select a source track, upper 3 bits pass a transform mode.
- `ACU - Decompiled/140020770.c:6-27`: target channel record supplies a byte
  destination offset; low type bits select rotation/translation behavior.
- `ACU - Decompiled/1400153B0.c:9-16` and remaining branches: mode-specific
  quaternion XOR masks and permutations, including mode 5.
- `ACU - Decompiled/1400C7BD0.c:37-42`: blend path calls `14008F220`, marks
  context bit `0x08000000`, and performs auxiliary blending.

Focused runtime disassembly and byte-for-byte live guards now confirm the
binding reads and mode-dispatch path for this installed image. The runtime
mode equations and a stage-aligned pose comparison are recorded in
[ACU-RE-0027](ACU-RE-0027-native-binding-mode-pose-conformance.md). The corpus
remains a discovery aid; runtime instructions and live bytes are the evidence.

Trying to inspect run 49's old playback pointer later failed its resource-ID
guard: the object had been reused. No old pointer was treated as current evidence.
The next replay captures candidate binding bytes while each playback is active.

Run 50: 225 root/pose/graph samples, controller runner exit 0, direct component
chain guards pass. Trace SHA-256:
`a366a79bb3247a467ad4a016efd5c7e5dde30ef69769a65f27ec4686d26c7818`.
The first binding collection/write stretches one read span to **0.715s**;
this is unsuitable as an atomic pose/time observation. The raw span is retained
and declared. Eleven distinct binding snapshots saved without binding read
errors in `run-50/bindings.json`.

Candidate layout: playback `+48` skeleton metadata, `+56` binding object;
binding first QWORD is a 4-byte-row array, WORD `+10` count. Each row is
`target_channel:u16, source_track_and_mode:u16`. Metadata `+8` points to
8-byte target-channel records, read here as hash:u32, destination offset:u16,
type:u16. Observed target offsets modulo 32 distinguish translation/rotation;
every analyzed primary rotation target hash agrees with the independently
captured 421-joint name at `offset / 32`. Bounds are 4,096 rows/channels.

`run-50/binding-analysis.json` preserves entries, target/hash/offset checks and
source decoded-track comparisons. Seven observed bindings cover all 23 primary
rotation targets with mode zero. Starting clip `000000316e569781` covers 21:
20 use mode 5 and one mode 4; **16 source hashes differ from target hashes**.
Two other newly observed candidate IDs use the same 21-target mode distribution;
their assets are not yet decoded. Thus direct name-hash application alone cannot
reproduce these bindings. There are also genuinely absent primary target writes
in that binding; their native preservation/default semantics remain open.

## Next falsification / implementation boundary

Verify the binding accesses and mode dispatcher against the preserved runtime
Extend the 21-target mode-4/mode-5 comparison to the missing two primary joints,
the second transition clip and the stopping clip; establish native behavior for
unwritten channels and the complete blend equation. Recover codec 7 before
including the avoidance take. The Rust binding-aware pose API is an evidence
adapter and is not yet connected to production graph selection or root motion.
No native movement constants are claimed.
