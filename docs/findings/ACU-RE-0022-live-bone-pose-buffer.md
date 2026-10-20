# ACU-RE-0022 — Live skeleton pose output buffer

Date / investigator / related task: 2026-10-19 to 2026-10-20 / Codex / ACU-008 native pose follow-up
State: partially established

## Provenance

Binary path / SHA-256 / image base / VA / RVA:
- Running `D:/Games/Assassin's Creed Unity/ACU.exe`, SHA-256
  `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, base
  `0x140000000`.
- Live probe site `0x1400988E3` (RVA `0x988E3`), bytes
  `0f2970e80f2978d8440fb7d9440f2940`. This is after the live instruction at
  `0x1400988DC` loads RDI from `[R8+0x178]`. The enclosing routine starts at
  `0x1400988D0` (RVA `0x988D0`).
- Captured mapped image SHA-256
  `07afa161c61064659c033827ff8d91b1e3c7a0759a46286e810962f312f02137`.

Corpus path and lines / export identity status:
- `ACU - Decompiled/1400988D0.c:47-56, 93-98, 108-109, 140`: reads output and
  input buffer pointers, advances by `32 * bone_index`, uses the first 16-bit
  table field to select a parent output record, and writes two 128-bit values
  to the indexed output record.
- `ACU - Decompiled/14009A7A0.c:9-18`: reads a 16-bit count and calls the
  routine for bone index zero.
- Live bytes were read from the mapped process and matched the probe guards.
  The corpus filename/export VA correspondence to this installed executable
  remains unverified; source-level names and exact behavior are not inferred
  from the filename alone.

Archive identity / byte range / range hash / file ID: not applicable.
Stored and decoded lengths / SHA-256: not applicable.

Tool/provider/version / evidence IDs or saved output:
- Local `acu_probe.dll`, Windows hardware execution breakpoints, Python 3.12,
  ViGEm-backed controller runner. Probe capabilities report
  `decoded_bone_or_traversal_layouts: false`.
- All captures below are ignored local evidence under
  `local/native-probe/bone-trace-20261019/`.
- `bone-pose-focused-run/native-session.jsonl`, SHA-256
  `584FB0C56083F1A380F19D23EFBA4D485E7B54EDF3F117039735069E6B1A590D`;
  input timeline SHA-256
  `B19906D2C1CBE3BA1ABFDD80E6EC40B975D96DA39D5EA5940FF6F252B8B41684`.
- `bone-buffer-poll.jsonl`, SHA-256
  `7952782A4DB8B86B70F7C01E06CDD0B26FE7EFC5694F25600B2A0CD26CBD7401`.
  It contains 163 reads of each 800-byte output array during a 6.7-second
  replay.
- `bone-buffer-poll2.jsonl`, SHA-256
  `72D8DE8EE8176C3997F2E3392C6C42C995B794D2CD7868E60DD4A1B57020E427`.
  It contains 167 reads of the 25-record input buffer and both output buffers.
- `player-bone-topology-table.bin` is a 300-byte live table capture, SHA-256
  `052eac64ab8e0c38aa01bba22dedad998b0c869f839f10286814ce99491dafd5`;
  its process-local pointers and decoded raw fields are recorded in the adjacent
  `player-bone-topology-table.json`.
- Rust pose capture `local/captures/acu-arno-rigged-20261019/scene-walk-132-145.json`,
  SHA-256 `bbb1f24f1519a5b166d68b18c21930e9e8b56723695db8679b64fd3b80653c64`,
  exports 721 matrices with parent indices and name hashes. Its inspected image
  is `local/captures/acu-bone-pose-20261019/rust-walk.png`, SHA-256
  `8E25C7B545CD049E738620A8C3E25F7E03697BA8732A6B1FEED67B26DE34E1DC`.
- Visual checkpoints are saved as
  `bone-pose-focused-run/screenshots/{001_walk,005_arc_03,008_stop}.png`.

Exact commands and search boundary:
- Runtime probes were byte-guarded at the explicit ACU process base. No other
  binary or game process was used as evidence.
- Replay profile: `bone-pose-input-clean.profile.json`; verified slot-0
  readbacks include walk `[0,10500]`, gradual headings `[2718,10142]`,
  `[5250,9093]`, `[7425,7425]`, `[9093,5250]`, `[10142,2718]`, then stop
  `[0,0]`.
- Probe profile: `bone-pose-focused-probe.profile.json`; replay command:
  `python tools/native_probe/controller_autoplay.py 14396 local/native-probe/bone-trace-20261019/bone-pose-input-clean.profile.json local/native-probe/bone-trace-20261019/bone-pose-focused-run --expected-sha256 748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40 --expected-user-index 0 --probe-profile local/native-probe/bone-trace-20261019/bone-pose-focused-probe.profile.json`.
- Continuous reads used the local-only `local/native-probe/poll_pose_buffers.py`
  against the two output addresses recovered from the guarded trace. Reads are
  not atomic with the game's writer.
- A four-site attempt at adjacent instructions produced no saved events before
  its session exited under excessive hit volume; its sites were explicitly
  unhooked. The one-site guarded capture and external buffer poll are the
  evidence used here.

## Result

### Record fields and parent composition

Pointer correction from the direct capture:
- At the probe, R9 is a pose-context pointer, not the local pose array itself.
  The sampled context is `0x13E137AF0`; its field at `+0x178` contains the
  local pose array pointer `0x13E137C90`. This was independently read as the
  little-endian pointer at `0x13E137C68`. The prior topology JSON labeled the
  context pointer as the input buffer; that label has been corrected. Output
  array pointers remain `0x13E138190` and `0x13E138690`.
- `ACU - Decompiled/1400988D0.c:47-52` loads both arrays and indexes records at
  32 bytes per joint. Lines 108-145 read the parent output quaternion and
  translation, combine the local record, normalize the resulting orientation,
  and store two 16-byte vectors. The observed layout is a translation vector
  `(x,y,z,0)` at bytes 0-15 followed by an XYZW quaternion at bytes 16-31.
  Across 786 live snapshots, all 19,650 local translation `w` values were zero;
  the median absolute quaternion-length error from 1 was `3.3e-8` and the
  maximum was `1.5e-7`.
- The child translation equation tested was
  `t_child = t_parent + rotate(q_parent, t_local)`. A high-rate read-only
  capture sampled the single contiguous range `0x13E137C90..0x13E1389B0`
  through `ReadProcessMemory` in 786 successful reads, with no read errors.
  Against each of the two 800-byte outputs, this gave 18,864 parent/child
  comparisons: median translation residuals were `3.4e-8` and `3.4e-8`, with
  maxima `1.7e-7` and `1.6e-7` respectively.
- The source implies `q_child = normalize(q_parent * q_local)`. A moving-state
  high-rate comparison against output `0x13E138690` had quaternion error
  median `6.4e-4`, p95 `0.293`, and maximum `1.153` while arrays were changing.
  During a stable Progress Tracker menu state, 150 contiguous reads contained
  the same complete output array. All 3,600 child comparisons then matched the
  quaternion equation: median error `2.16e-8`, p95 `5.59e-8`, maximum
  `6.25e-8`. This validates the equation for the nonroot subset. The earlier
  moving-state outliers are consistent with reading while joints are updated;
  use an update-boundary capture for dynamic pose comparisons.

### Runtime name-hash association

- The live metadata object at `0xF78D3C90` contains the pointer `0xF78D7FC0`
  at offset `+0x38`. The pointed-to 200-byte array is 25 sorted 8-byte pairs:
  a little-endian 32-bit hash followed by a 32-bit runtime bone index. All 25
  indices cover `0..24`; the next 8 bytes after the array are zero.
- Each hash matches exactly one `name_hash` in Rust report
  `scene-walk-132-145.json`. This confirms the native-index to Rust-pose-index
  mapping by hash, superseding the earlier topology-only candidate, whose tail
  mapping was wrong:
  `[0,1,2,3,4,5,6,7,8,9,10,11,12,14,15,16,17,52,60,61,63,64,65,66,108]`.
- The correspondence identifies Rust rig nodes for this 25-node runtime
  subset. It does not decode readable original bone labels or prove the live
  component is Arno.
- Raw table `player-bone-namehash-index-table.bin`, SHA-256
  `0706f23958954f39b106ee677f004630832b12c45cff1488dd8df6b5e358a43d`;
  mapping and validation record `player-bone-namehash-index-table.json`,
  SHA-256 `85ed17adeeedafa17ce2659be3cc7eb9d17414136ac8ce31a2728ffb27a55dc0`.
  The matched Rust report SHA-256 is
  `bbb1f24f1519a5b166d68b18c21930e9e8b56723695db8679b64fd3b80653c64`.

Capture provenance for this analysis:
- `bone-buffer-fast-run/memory-samples.jsonl`, SHA-256
  `f63d4b27bf18baf29c8e277f4a5092af99cd715d007ba4a197a1decc10c72a52`;
  derived comparison `bone-buffer-fast-run/bone-algebra-analysis.json`, SHA-256
  `0f30fdeb957c2bf47c63bfbc1e738720b21e0d1c0c2609e53efa847875ef123d`.
- `bone-buffer-contiguous-run/bone-buffer-correct-input.jsonl`, SHA-256
  `d04d663ba25ad678bdd99601e3dad2b0e2db2f799f246f0835725961101c81e5`, is an
  earlier 141-read pass using the same contiguous memory range. The 786-read
  capture is used for the statistics above.
- Stable menu capture `bone-buffer-paused-menu-20261020.jsonl`, SHA-256
  `d6ef981053477922500c175a5bcd0207c72f5cc9c8702fd4fda193e697c08e16`;
  analysis `bone-buffer-paused-menu-analysis.json`, SHA-256
  `391072f42882c59aeed42abad87a2191670326eeee33a7ffc5041efeee57c86d`.
  Its visual checkpoint `local/native-probe/progress-tracker-idle-20261020/`
  `screenshots/000_tracker_idle.png` has SHA-256
  `0313f990c5ecb03b44f4326d5eb82efddc018493c5f5ca5669017af1f9fa7dca`.

Exploratory pose comparison:
- Using the verified hash map, native sample 71 (2.009 s after the input
  profile's verified walk start) was compared with Rust clip
  `0000000fc08fd67a` at 1.983 s. The closest sample was 25.9 ms from that
  nominal time. After subtracting each pose root translation, the direct
  coordinate translation errors over 25 nodes had median `0.229 m`, p95
  `0.952 m`, and maximum `1.020 m`.
- This is not a matched-clip or matched-actor comparison: the native selected
  clip and component identity are still unknown, and the coordinate basis is
  unresolved. It is a measured mismatch, not a parity result or evidence for a
  guessed axis conversion.
- Reproduction summary `bone-rust-walk-exploratory.json`, SHA-256
  `0979b7af89a607a94d393929620ba50907a5eb2bf509874b3d2f8b42a4e45bd4`;
  it records the native sample, timeline, Rust capture and hash map identities.

Local rig cross-check against the saved Rust pose:
- For all 25 mapped rows, the live parent table agrees with the mapped Rust
  `matcher.pose` parent index (zero parent mismatches). For the 24 nonroot
  records, each Rust local transform was derived from its saved global matrices
  and parent, then compared to the hash-mapped native local array at context
  `0x13E137AF0`. Across 136 samples (3,264 comparisons), local translation
  residual median was `1.17e-7`, p95 `3.73e-7`, maximum `9.08e-4` in the
  report's coordinate units.
- Direct local quaternion angular error against that one saved Rust frame was
  median `26.10 degrees`, p95 `95.45 degrees`, maximum `132.57 degrees`.
  The translation and hierarchy agreement strongly supports the rig-row and
  parent mapping, but the rotation mismatch prevents pose parity. This frame
  is not phase-matched and the live actor identity and coordinate basis remain
  unresolved; do not treat the rotation values as evidence of an animation
  codec error yet.
- Inputs: live local-array time series SHA-256
  `e4e3dbfd26c73ae5bb9535420504ede3fcf23a09d90623a405e2ae4a9750b5ce`;
  Rust report SHA-256
  `bbb1f24f1519a5b166d68b18c21930e9e8b56723695db8679b64fd3b80653c64`;
  comparison script SHA-256
  `de1defcf9d5196ba79e9c640bb2020d38c7088811589a5073b1e6e97b515ebe8`.

Selected-clip phase comparison:
- The selected Rust clip `0000000fc08fd67a` is the resource in
  `local/animation-matcher-20261019/walk-decoded.json` (resource payload
  SHA-256
  `b55338f07ddc5e8e9798988496d0e9aabda3717961c512f3fb5ec453b7c22cfb`).
  Its internal animation key
  is `0x6789439a`; that key is not the resource file ID. The decoded resource
  has 79 tracks. Exact name-hash matching finds rotation tracks for runtime
  indices `2..24` (23 of the 24 nonroot records); index 1 has no clip rotation
  track.
- A 60 Hz shortest-path quaternion sampler searched phase offset and playback
  rate against the 136 local-array frames, separately for each verified input
  phase. Search rates were `0.1..3.0` in `0.025` increments. The best rate hit
  the lower bound `0.1` for forward, backward, right and neutral; left selected
  `0.125`. Best median per-frame mean joint angular errors were `33.03°`
  forward, `33.16°` backward, `33.52°` left, `33.82°` right and `34.62°`
  neutral. This does not show a phase match to the selected clip. The active
  actor/clip is unknown, and blending, actor identity or coordinate conventions
  may account for the difference; no clip parity claim follows.
- Decoded resource report SHA-256
  `96e072ef24e90a8abe0aa3ed4304532be1e22956a342f3e04506a33d2e07f5b3`;
  phase-fit script SHA-256
  `1a2ad7d0291c07bee2a35c82d39b376aff5cb5cc3b04f228cb1d104e35a2d221`;
  machine-readable results SHA-256
  `24dc4f5514e48cf5fdcffa6d360c32652442e6eca6e771bcae923e6fc01b9dc5`.
- A follow-up decoded four additional exact-hash-mapped navigation variants
  from `DataPC.forge`, entry 928, with `acu-mat`: walk bank-left, bank-right,
  forward strafe-left, and forward sway-left/right. Each has 23 matching
  nonroot rotation tracks in this subset. Phase/rate fitting against the
  corresponding input phases still did not find a synchronized walk cycle.
  The best candidate was `xx_Nav_WalkFwd_3-Loop_SwayRt90` for every input
  phase, but its median per-frame mean joint error ranged from `26.88°`
  (forward) to `28.28°` (neutral), with fitted rates at the minimum tested
  `0.1` for all but left (`0.125`). The same candidate winning across opposite
  inputs and neutral, with the rate on the search boundary, is not evidence
  that it was selected by ACU. It does not close the phase or identity gap.
- Decoded candidate JSON SHA-256 values: bank-left
  `d7d032748bc26ee93ebfb73f3434904deb3b67a53ada8f36d57e226493e60e85`;
  bank-right
  `94ee09803073dc6db255fb696766ac39acb0cf5073389bc4ac7965843e2112c9`;
  strafe-left
  `51c5609b99ba55613377885cc24c2d9acc2541b2b9262a29fa7f3c20dd226724`;
  sway-left
  `14df75f83b6846fbb80fe4e98fd3dabc3c5f4be014c84b0b3610dd3d002b19fe`;
  sway-right
  `37e06c289d4cf192ac869425bcdb0f9a914a42731429e16a28509615680a6c85`.
  Directional fit results SHA-256
  `3c2da370803b59ef030342d72964d9ef4034aaf714010c9f5e9186b3ecaac337`;
  comparison script SHA-256
  `dbee9e161168ef2a3633bf944c3e694c82f8b4e761abe76d40da85a57cf60b7b`.
- Animation resource provenance: `D:/Games/Assassin's Creed Unity/DataPC.forge`,
  archive SHA-256 `aeccb08142fe1f172ad8c048a6b0a282dc04794f291231a52499532cef968987`;
  entry 928 is file ID `0000002633b2c79d`, offset `101271720`, stored length
  `123303092`, stored SHA-256
  `08b0a18bd2fa603ae2b77f2b17587c4f41c7154050552b5874edcd8c2bceda00`.
  Resource records and decoded payload hashes:

  | File ID | Resource name | Record offset | Record bytes | Payload bytes | Payload SHA-256 |
  | --- | --- | ---: | ---: | ---: | --- |
  | `0000000fc08fd67a` | `xx_Nav_Walk_3-Loop` | 124046390 | 23709 | 23563 | `b55338f07ddc5e8e9798988496d0e9aabda3717961c512f3fb5ec453b7c22cfb` |
  | `0000000fcdad9f80` | `xx_Nav_Walk_3-Loop_BankLeft` | 124314769 | 24035 | 23880 | `7946cc4681900752631ddd779300c3e62745d9e856fbec607476542a7610bdc7` |
  | `0000000fcdad9f9a` | `xx_Nav_Walk_3-Loop_BankRight` | 124290754 | 24015 | 23859 | `1c1e36c12b9ed57b891f9510953b7a2a1e5ac076c65d634a734d863ba8cf9cb4` |
  | `0000000805948331` | `xx_Nav_WalkFwd_StrafeLeft90` | 150934373 | 8449 | 8294 | `62cf47e85f74a2cc1491e1f59cde3964290c08fab4b21d3d0a164179a8345ee2` |
  | `0000000fe013b72b` | `xx_Nav_WalkFwd_3-Loop_SwayLt90` | 124070099 | 25545 | 25387 | `847612425053c10b6dd9069291631e7297f4eabeab2768fde130bce65ee64247` |
  | `0000000fea42eded` | `xx_Nav_WalkFwd_3-Loop_SwayRt90` | 124020849 | 25541 | 25383 | `022e865aa0278d51a97c714635ffbf2e481107f502a58a7f10eeb7ab317a08d4` |

Crowded-scene actor re-identification attempt (2026-10-20):
- A 5.1-second controller sweep applied forward, backward, left and right
  stick values (`+/-11000`) for 1.2 seconds each, followed by neutral. Every
  XInput readback matched the requested state. The start and end screenshots
  show different camera framing in the same crowded contextual scene; this
  does not independently identify the controlled actor or establish its path.
- Re-reading the previously tracked owner at `0x13E133D90` left component
  `0x13E135250` at `(517.9795, 32.5291, 0.0436)` within about `4e-6` world
  units. Its pose/component identity as Arno therefore remains unproven; the
  stationary reading may be a non-player component or a non-root transform.
  Several other tracked objects moved or returned zero-like values after the
  scene changed, so those stale owner reads are excluded from identity claims.
- A fresh guarded capture sampled 6,144 calls at `0x1400988E3` over the
  directional replay. It contained 160 R9 input contexts; 123 sampled calls
  had `RDX == 25`, spread across 37 input contexts. This is a candidate set,
  not a player identification. All hooks were unhooked after capture and the
  game remained responsive.
- The sweep profile SHA-256 is
  `b9664318edeb7b66a23f13f90e6ea4536c6ece7bd151558abd9a15be2a3783d3`;
  its verified-input timeline SHA-256 is
  `e70e6aa32d4ac2031df624de4d753859d3f8927fdac60afa4245966b240712cf`.
  The post-sweep owner snapshot SHA-256 is
  `df08784d962edb3ab3ee2bc9d793129cac9482082fe0f0577ee68a9d2364b8be`.
  The guarded bone trace SHA-256 is
  `2884375a39b5b58ca5b03a3ba71189f4174d0820ac3f593975ddd955db85f54c`;
  the trace-run input timeline SHA-256 is
  `406d8a8ec95ab1561849adccbe35ac1cfdb029f0140120d8f896577af0bc23bd`.
  Evidence is under `local/native-probe/actor-arc-20261020/`.
- Each of the 37 candidate local arrays was then read 136 times across a
  4.1-second verified forward/back/left/right/neutral sweep. All 5,032 full
  array reads succeeded and each array remained finite with zero translation
  W and unit local quaternions. The previously mapped context
  `0x13E137AF0` changed in 131/136 samples, with 21 of 25 records changing
  during the capture. Other contexts also varied, so animation activity alone
  does not identify the controlled actor. This establishes a repeatable way to
  collect each candidate's local pose through input phases.
- Candidate list JSON SHA-256:
  `eb5f706910843dee51214ba74b7dc139697e222f8f76134d3b82cd3d3f553ad2`;
  time-series SHA-256:
  `e4e3dbfd26c73ae5bb9535420504ede3fcf23a09d90623a405e2ae4a9750b5ce`;
  profile SHA-256:
  `68130dd53141cccfaabf66f244da1f593d476f7633eab7b1db3bb9ca2d61e2ae`;
  verified timeline SHA-256:
  `5d0244401abcc5e14bfe3850d93ede1d3422624e7d5a4917e685b81bd3367021`.
  The collector recorded 136 frames with zero read failures.

Open-street controlled-input pose capture (2026-10-20):
- A 5.4-second profile settled, then applied verified walk-forward, sprint-
  forward, walk-left, walk-right, walk-backward and neutral states. XInput slot
  0 readbacks matched each requested state, including right trigger `255` for
  sprint. The sprint screenshot visibly shows Arno running in the open street.
- During that capture, the previously mapped owner at `0x13E133D90` remained
  exactly `(484.4653, 53.4785, -7.08e-8)` in all six phase summaries, including
  walk and sprint. Thus the visual player evidence does not validate this
  owner's identity or show its root translation; it could be a non-root
  transform, a blocked movement interval, or the wrong component.
- The local array and both 25-record outputs were captured as a 3,360-byte
  region. All 229 samples passed identical full-region double reads, with no
  read failures. The local array and output arrays had 16-28 unique snapshots
  per phase. Both output buffers satisfy child translation composition with
  median residual about `3.3e-8` and p95 about `8.9e-8` across 5,496 nonroot
  comparisons each. Quaternion residual p95 is `0.0133 rad` for output A and
  `0.0101 rad` for output B, with maxima near `1.525 rad`; these outliers and
  unknown actor identity keep this from being a parity result.
- A diagnostic phase/rate fit compared the 23 mapped rotation tracks to six
  hash-verified walk resources during each input phase. The left-sway walk won
  every phase, with median mean-joint errors from `22.1` to `23.9 degrees`; all
  fitted relative rates hit the `0.1` lower bound. It also won during neutral
  and backward input, so it does not identify the native active clip or phase.
  The mismatch could reflect actor/context mismatch, blending, timebase or
  update semantics. No animation parity is claimed.
- Capture profile SHA-256 `a4aee8a79f5a04b27dc08a9b14a1a82d69751f01563e7608663ad134430938eb`;
  verified-input timeline SHA-256
  `8c9970a98055141dc46146fe05505ff02cefdc2fd6d4c733bd1741c930760192`;
  pose samples SHA-256
  `649b04a1f5dc7bd1d2cdbbbcbd9b120724e199b32bee62109a2590881db29da9`.
  Composition analysis SHA-256
  `6d26e91410bd22500a279cce8372106f1bda92ef53fdbbad9496011f8209ec2d`;
  clip-fit report SHA-256
  `4be724ccde609f89be991e87605b18acaf2e22d47ade95ada7ba69307564fe59`;
  fit script SHA-256
  `684bcf38f9d6952a87ca7d353ac4d230de7350f866b6598fde37f638e829f67f`.
  Raw capture and screenshots are in
  `local/native-probe/actor-arc-20261020/open-street-pose/`.

Open-plaza transform/pose association follow-up (2026-10-20):
- Two 18-second guarded native sessions overlapped with identical verified
  controller profiles (forward sprint, left-forward sprint, right-forward
  sprint, neutral). The controller runner and native session exited `0` in both
  runs. Across 11,270 and 11,288 captured native events, 500 and 552 events
  respectively had `RDX=25`; each trace included dozens of distinct R8/R9
  contexts. The game remained PID 14396, executable SHA-256
  `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, image
  base `0x140000000`, and guarded target RVA `0x988E3`.
- A memory-neighborhood scan found runtime transform-class objects whose
  allocations contain pointers to candidate pose contexts. In the first run,
  owner `0x13D3790F0` and candidate context `0x13D37C910` were associated at
  object offset `0x2130`; that owner moved `10.47` world units during the
  capture and the sampled 800-byte pose-array hash changed in 114 of 187
  adjacent samples. A second pair (`0x13F4B3880` / `0x13F4B6EA0`, offset
  `0x1F30`) moved `10.11` units. Arno is visibly moving through the plaza in
  the synchronized screenshots, but the memory-neighborhood association does
  not establish that either owner/context pair belongs to him.
- In the second run, the scene and live object set had changed. One nearby
  candidate (`0x13FB69CB0` / `0x13FB6D720`, offset `0x1A10`) changed position by
  `57.68` units, including a single `57.59`-unit sample-to-sample jump; its
  pose-array hash changed in only 34 of 187 sample intervals. Other tracked
  owners also moved substantially. This may reflect manual repositioning,
  object reuse or streaming. It is not evidence of Arno's root motion.
- Profile SHA-256 in both runs:
  `33a57a4391bcf4ce45ffecc4dbccac2244ec1efc4468156fe641cac15c5c05e1`.
  Run 1 input timeline / sample / native-session SHA-256 values:
  `eb89516481f8c1d881b3deed1d414c729ad519f50c4c65ca39abcd982217e63d`,
  `f1be851635e9717a08149a093c464aa22274974818cadd5fd32722963435c282`,
  `b684f7456e11cdfcd9cff994d702888115b2c962eb9af6c467d10c45a9647246`.
  Run 2 values:
  `51d57a4dde2804e4924f4c6e6d4ca5997959b221fcb24df7ff39526f906e1a3c`,
  `dbf32ab770e8ee93942aec47e15f12601077b0a1c38736ac9f94c3e5c56cf7ed`,
  `566f910eb4d471f584616c1005633bec510805a0d59dbdddaf43c67487c4ccea`.
  Captures are under
  `local/native-probe/actor-arc-20261020/player-link-plaza-motion-1/` and
  `.../player-link-plaza-motion-2/`.

Rebound player-transform / local-pose candidate (2026-10-20):
- A later trace on the same build found context `0x151CFBFE0` in the live R9
  event set and a pointer to it at offset `0x1F20` inside a runtime transform-
  class object at `0x151CF7BB0` (vtable `0x143017D60`). A 3-second forward /
  diagonal sprint profile moved this transform `9.323` units with a maximum
  adjacent-sample step of `0.175`; its 800-byte local-array hash changed in
  73/183 adjacent intervals. The synchronized screenshot shows Arno running
  through a market lane. A separate short-walk capture from a user-repositioned
  dock start saw the same context in 16 guarded R9 events; verified input
  `[0,8000]` for 1.801 seconds moved the transform `1.867` units with a maximum
  adjacent step of `0.071`. The array pointer stayed at `0x151CFC180` and the
  hash changed in all 33 walk-phase intervals. These repeated correlations make
  this a stronger player/skeleton candidate, but do not independently prove
  that the object belongs to Arno; the dock screenshot is partly occluded and
  the route is constrained by a post.
- A diagnostic phase fit over 23 hash-mapped rotation tracks and six decoded
  walk resources selected `xx_Nav_WalkFwd_3-Loop_SwayRt90` (resource
  `0000000fea42eded`) for the short forward-walk phase: 34 samples, fitted phase
  `1.7333 s`, relative rate `0.7`, and median mean-joint angular error
  `21.81 degrees`. In the sprint capture, the best forward result instead was
  `xx_Nav_Walk_3-Loop` at `12.88 degrees` and the tested rate floor `0.1`.
  Different winners and unresolved clip ownership/timebase mean these are fit
  diagnostics, not active-clip identification or pose parity.
- The repeated short walk and lateral/backward attempts at the dock produced
  almost no candidate-root movement; screenshots show Arno beside a post.
  Treat those input windows as obstructed, not as negative locomotion evidence.
  A subsequent jump-back capture's native session failed with WinError 231
  (probe pipe busy); its temporary hook was manually removed and status
  confirmed every hook address zero. That failed native trace is excluded.
- Sprint candidate profile / timeline / sampled-array / native-session SHA-256:
  `33a57a4391bcf4ce45ffecc4dbccac2244ec1efc4468156fe641cac15c5c05e1`,
  `577fcc4db9c40a37b32468100a6f086b58319934cd3be1d3e0468c29de947a9d`,
  `28214ee5dacdec45039dd4c8f3652c399dd8198ef9f9d4178359c563620422d7`,
  `55bd45cd59348b755dc96246950aed0f681ae8815ef0e93c2dd7224c708d394a`.
  Walk candidate timeline / sampled-array / native-session SHA-256:
  `3b6806aaa6604e1bbf40b3ed97186c47a12670d693ff9eefb1dcd91657ed9c5e`,
  `6f97165da3a3f8da2ad903d7021cb93fca6b0a764abe511abf46f9d3b16e057d`,
  `328838e5f9f2c269cd6290a1ddfa0cf9e79c12a2fca87ecdd7ddc587d44ab396`.
  Fit report SHA-256 `6853d1adcfc03a88565e63bbe83a4d27812b43874a474d31e7bb87d2164fc9ca`.
  The sprint and walk data are under
  `local/native-probe/actor-arc-20261020/player-link-plaza-motion-5/` and
  `.../player-link-plaza-motion-6/`.

Observations:
- At routine entry, RCX is the 16-bit bone index, RDX is the bone count, R8
  selects the output pose context, and R9 selects the input pose context. At
  `0x1400988E3`, RDI holds the output-buffer base. The focused guarded trace
  sampled 14,592 events.
- One live component chain linked the update context `0x13E137AF0` to local
  pose array `0x13E137C90`. Its call events report bone count 25. Two output contexts
  expose 800-byte arrays at `0x13E138190` and `0x13E138690`, matching
  `25 * 32` bytes. The decompiled routine writes two 16-byte vectors per
  indexed record.
- The associated 25-entry table has a 12-byte stride. Its first 16-bit field
  is used as a parent output index (`32 * field`) by the live transform routine;
  entry zero uses `0xFFFF` and the other entries form this observed hierarchy:
  `[root,0,1,2,3,4,1,6,7,8,0,10,11,12,13,14,15,16,12,18,12,20,21,22,23]`.
  This recovers parent links for the runtime subset.
- The 25-entry hash-to-index table independently associates every runtime
  index with one unique Rust `name_hash`. The earlier structural embedding was
  only a candidate and is superseded by the hash-verified mapping above.
- In the 24 Hz memory poll, each complete buffer, the first 32-byte record, and
  the record at index 24 had 161 distinct values among 163 samples. For the first
  record in buffer `0x13E138190`, the eight observed floats changed from
  `(-0.0379, 0.4252, 0.7657, 0, -0.0347, 0.0635, 0.8597, 0.5056)` near the
  start, to `(0.0381, 0.4301, 0.7509, 0, -0.0438, 0.0158, 0.8408, 0.5394)`
  during the walk, and `(0.0658, 0.4461, 0.7525, 0, -0.0572, -0.0063, 0.8338,
  0.549)` during the turn arc. Under the recovered layout, the first four
  values are translation and the last four are the XYZW rotation.
- The first settle readback still returned the previous stick value
  `[-3356,-1869]`; only subsequent walk and turn phases are treated as verified
  input here.
- The matched local pose buffer is reachable from a sampled runtime component and
  transform owner whose observed position was `(517.9795, 32.5291, 0.0436)`.
  That owner position stayed essentially unchanged across repeated input runs;
  the component is not proven to be the controlled character or Arno. The replay
  also entered a crowded contextual sequence.

Inferences and alternatives:
- The two-vector layout, parent translation composition, and normalized
  parent/local quaternion product are established for this routine and
  nonroot runtime subset. Dynamic update-boundary sampling and root processing
  remain open. The runtime-index to Rust-pose-row mapping is confirmed by the
  live hash table; readable original bone labels remain unavailable.
- Pose records from this component were sampled during the verified input
  replay, but its unchanged root position prevents assigning it to the controlled
  character. NPC animation and shared scene activity remain alternatives.

Unknowns / incomplete coverage:
- The stable menu reads found no changing bytes, but no dynamic update-boundary
  snapshot was taken; high-rate polling may catch moving arrays partway through
  an update. No moving labeled 25-bone pose was captured over an entire update.
- The latest open-street capture still does not link the changing 25-bone pose
  context to Arno: its associated owner did not move during the verified
  phases, and several candidate contexts animate. The 37-context candidate set
  needs a direct actor/component link before phase-matched pose evaluation.
- The open-plaza follow-up found candidate pose pointers inside moving
  transform-class object neighborhoods, but movement and scene changes were
  ambiguous and the candidate object set changed between captures. This does
  not close the actor/component identity gap; a stable controlled start state
  and direct skeleton ownership evidence are still required.
- Context `0x151CFBFE0` is now a stronger live player/skeleton candidate because
  its enclosing transform moved smoothly under verified sprint and walk inputs
  and it appeared in the guarded R9 trace. The clip fit still varies by input
  phase and retains 21.81 degrees of forward-walk error; exact Arno ownership,
  active clip/time, atomic sampling and matched Rust pose/root motion remain
  open.
- The native coordinate basis, root-motion ownership, readable original bone
  labels, and exact
  player/Arno component identity are not established.
- Rust's final `matcher.pose` is now exported with parent indices and hashes.
  The first comparison used native elapsed time 2.50 s and Rust clip time
  1.98 s, with no verified clip-phase or component match; it did not establish
  numeric parity. No bone-level or steering parity is claimed.

Independent comparison / falsification test:
- Identify the controlled actor's actual skeleton component through a live
  owner/transform chain or guarded animation-selection call. Then capture its
  output buffer together with clip identity/time at a controlled input phase.
  Apply the hash-verified 25-index map and compare to Rust at the same phase;
  retain atomic/update-boundary evidence and root-motion ownership.

Reproduction steps:
1. Launch the exact installed ACU build and confirm its SHA-256 and base.
2. Use the saved guarded profile at RVA `0x988E3` to sample RDI (the output
   buffer pointer) and R9 (the input-context pointer).
3. Read the local input-buffer pointer at `R9 + 0x178`, match the context
   through its live component chain, then poll its 25-by-32-byte arrays.
4. Preserve the screenshot, XInput readback, mapped-image identity, and trace
   hashes alongside any comparison. The current open-street snapshots and
   six-clip diagnostic fit are in
   `local/native-probe/actor-arc-20261020/open-street-pose/`.

Implementation implications and finding-linked constants:
- For this routine only, bytes 0-15 are a local translation `(x,y,z,0)`, bytes
  16-31 are an XYZW quaternion, and `32 * bone_index` is the record stride.
  Preserve the source context and keep the coordinate basis and asset-bone
  mapping explicit.
- `acu_motion::native_pose` decodes this record and composes the recovered
  parent-before-child nonroot subset. The caller supplies the root output
  transform because the native root path is separate. The helper rejects
  incomplete/nonfinite records, nonzero translation `w`, degenerate rotations,
  and unsupported hierarchy order. It also decodes the live 8-byte
  hash/index entries and maps runtime joints to Rust pose rows, rejecting
  duplicate or missing hashes and indices. The full `acu_motion` suite passes
  all 58 tests, and package Clippy passes with warnings denied.
- Next slice: export a stable Rust pose trace, establish actor identity, and
  compare named bones and root motion at matched phases.

## Controlled XInput return and fresh plaza owner search (2026-10-20)

The installed ACU binary was still SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40` at base
`0x140000000`. A byte-guarded hook sampled the call return at VA
`0x14273A922` (RVA `0x273A922`), bytes
`85c00f94c08883480800004084ff7408`. The caller is represented in
`ACU - Decompiled/14273A8B0.c:50,68`: its stack buffer `v50` begins at
`[rsp+0x20]`, is passed to `sub_1428F53F6`, and the result is tested against
zero. The trace sampled 16 bytes at `[rsp+0x20]` and post-call RAX. This is
runtime evidence for the installed binary; the corpus export correspondence
remains unverified.

In `local/native-probe/actor-arc-20261020/crowd-exit-right-live-1/`, the
current hook revision produced 27 call-return events with RAX zero. Nine
samples matched the scripted right-stick input: five had left stick
`(9000,0)`, A button `0x1000`, and right trigger `255`; four had `(10000,0)`
with trigger `255`. Eight were neutral `(0,0)`. Ten showed the same nonzero
baseline stick state seen in neutral readback. Twenty-three older-revision
events were present in the ring and are excluded. The current-revision states
occurred during the corresponding input windows in the timestamped timeline.
Native trace SHA-256:
`F9A4320CBD0208FF0ED745A891475792224E87F2C6B74423F67A9DA65A212F41`;
input timeline SHA-256:
`D147A95CFF1E419D139E6D4842A8A878DDB5DEF627790E888C6E0A2FBB434445`.
This confirms that the game call path received successful scripted XInput
states; it does not establish actor movement, device-index ownership, or
locomotion parity. The visible plaza remained crowded and the avatar did not
clearly leave that contextual situation.

A separate read-only linked capture in
`local/native-probe/actor-arc-20261020/crowd-forward-linked-1/` sampled 102
frames, 11 pose contexts and 13 transform-owner candidates during retreat and
advance phases reported by the virtual-controller runner. The
later high-rate caller trace shows that those runner reports are not enough to
bind each phase to the native caller or one device index. Largest owner
displacement was
`0.3614 m` (from `(516.8260,2.7112,-0.4333)` to `(517.0198,3.0162,-0.4333)`);
two other candidates moved `0.3258 m` and `0.2297 m`. Sample SHA-256:
`B5A24126A4509FC11B642E5517CB22BF61F91493ED8FC87088DDF7C794DBE187`;
native trace SHA-256:
`B0F3789F8054CE2D55459582716C00D9E5D6E2E75975CBABBE0CFD165F5C7396`.
Several candidates moved in the crowded scene, so none is assigned to Arno.
The broad bone hook overwhelmed event polling at `every=48` and `every=5000`;
the follow-up reduced its event rate to `every=50000` and yielded a readable
trace. This is a probe-cadence adjustment, not a game behavior result.

Next: repeat the linked capture with the sparse bone hook and the post-call
XInput hook together, then compare owner trajectories against the same input
windows and fresh screenshots. Keep identity unresolved until one owner and
pose context track Arno across repeated routes.

## Caller and scripted-input mismatch (2026-10-20)

The `every=1` caller trace in
`local/native-probe/actor-arc-20261020/xinput-reversal-live-2/` captured 576
current-revision events and 28 older-revision ring events. The replay runner
reported the requested forward and backward states. Using the session QPC
frequency and polling timestamps to align caller events to the input timeline,
the forward window contained 106 successful baseline-state samples and only
two successful `(0,10000)` samples. The backward window contained 114 baseline
samples and 113 successful `(0,-10000)` samples; four calls returned
`0x48F`. This makes the controller-to-caller association inconsistent across
these phases. It does not establish a single XInput user index or prove which
input source drives the avatar. Sample and native trace SHA-256 values are
`6044D2155BCC1722528989663A8E6F798A1823C6481FEEDC4456071EEB0CFEE1` and
`1049D448094E3F48829A432C26EBB08872C4A88EC16F6FC8DC7FD72EB50FFD20` under
`crowd-reversal-test-6/`.

In `passive-linked-7/`, 96 memory frames ran without scripted controller
input. The 32 native call-return samples all contained the persistent
`(-3356,-1869)` stick state; 31 returned success and one returned `0x48F`.
Four linked owner candidates moved between `0.133 m` and `0.346 m` during this
five-second window, and several pose arrays changed on most sampled frames.
The persistent nonzero stick means this is not an idle control; scene activity
and externally held input remain alternatives. Sample SHA-256
`34FCAB2FDA8294DB7E9A76B4EB0EA9E12DB04B0B283E57483CD7D8FD516D1B50` and
native trace SHA-256
`74BD02F72686B36A9E5B3567FDA9C8B79CB4513C69D4AAE084752F66789F5574` identify
the capture. Next, resolve the caller's input-source identity before using
these owner trajectories as controlled motion evidence.

## Corrected live pose-context discovery (2026-10-20)

Run 10 is excluded for pose identity: its profile sampled the bone dispatch
site at `every=100000`, producing only three dispatch events while the memory
sampler continued reading the 37 contexts from the earlier run-2 capture. The
tracked 25-record arrays were byte-identical through the input phases. This
was a stale-context sampling setup, not evidence that the game pose stayed
static.

Run 11 changed the guarded dispatch probe to `every=1`. On PID 24364 and the
same installed image, the trace recorded 10,910 dispatch events and exposed
123 distinct R9 addresses; 49 distinct contexts had `registers[1] == 25`.
Read-only neighborhood scanning found 32 context/object matches. The scan
window spans up to `0x3000` bytes from candidate transform-class objects, so a
single pointer can be attributed to several adjacent allocations. Those
matches are search leads, not ownership proof.

Runs 12 and 14 then sampled the 49 selected contexts plus 28 nearby owner
candidates during controller replay. The replay helper detected and read back
XInput user index 3 before each step. In run 12, the heuristic pair
`0x153f8deb0` / `0x153f8a5a0` moved 2.806 units during three seconds of forward
input and 1.761 units back during three seconds of reverse input; its sampled
pose hash changed in 50/57 and 54/56 intervals. The pair is not uniquely
linked: the broad neighborhood scan associates the same context address with
several nearby transforms. Six-clip quaternion fitting selected
`xx_Nav_WalkFwd_3-Loop_SwayRt90` at 21.22 degrees (relative rate 0.225) forward
and 16.86 degrees (rate 0.1) backward. The different best rates and moderate
residuals do not identify an active clip or phase.

Run 13 is excluded from controlled native evidence: the replay helper read
back its requested states, but its native session exited with code 1 and
recorded no events. Its screenshots and memory samples remain diagnostic only.
Run 14 visibly moved Arno over the railing with A + RT + forward, then RT +
forward; XInput readback showed those states at index 3. The native trace
captured 648 events at each XInput probe and 648 caller returns, but the game
faulted at RVA `0x988e3`, the guarded bone-dispatch probe address, with Windows
exception `0x80000004` during the run. Treat this as a likely probe stability
failure, do not use the run to establish pose or movement parity, and do not
reinstall the bone-dispatch hook in the next iteration. The target process
exited; no game files were modified.

Evidence is under
`local/native-probe/actor-arc-20261020/xinput-redirect-live/run-11/` through
`run-14/`. Run-11 native trace SHA-256 is
`C3570379C729167CD9C834E13F0DC36E54C30B91E5EDE39E0EDD2A8BB9E7E6BB`. Run-12
sample, native trace, and timeline hashes are
`3B703913D0BEF73EDB1C480F4A6CE9A58210A058F6FAD252CA216EF32DC2E165`,
`DF3366DE8837CE352A85506DA936882AAF7BD413E7295DCEE1A3A5465CA9B645`, and
`EB5A649F4619C20FC37996A9AC1751A01DF5828180C581106F7239744915433F`.
Run-14 sample, native trace, and timeline hashes are
`2C8B8C464152C7E4693ED826C601CC85E47B0EB53753703253CFC0D6171939DF`,
`7F83D87DC21D20CF137D79F8480BD82D63104D703E75C9330BBA0451D3B24CDF`, and
`2B218ADB69A0CB17F63AB9A3B9E7EFF83EA0AE758B654D8FA825B68C30FC9737`.

The next comparison should use the clearer street position reached in the
run-14 screenshot, but with only the XInput probes and read-only sampling of
the already-discovered pose arrays. Establish a direct owner/component link
before tuning Rust root motion or claiming actor/clip parity.
