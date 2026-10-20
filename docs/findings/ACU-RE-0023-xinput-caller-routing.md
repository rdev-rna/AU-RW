# ACU-RE-0023 — XInput caller-index routing in the Paris scene

Date / investigator / related task: 2026-10-20 / Codex / ACU-008 locomotion follow-up
State: partially established

## Provenance

Binary path / SHA-256 / image base / VA / RVA:
- Installed `D:/Games/Assassin's Creed Unity/ACU.exe`, SHA-256
  `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, base
  `0x140000000`.
- XINPUT1_3 entry at VA `0x7FFF9EBC4EE0`, RVA `0x4EE0`, guarded bytes
  `e9eebd2c1e574883ec20488bda8bf9e8`; its return site was VA `0x14273A922`,
  RVA `0x273A922`, guarded bytes `85c00f94c08883480800004084ff7408`.
- XInput 1.4 entry at VA `0x7FFFAFD01750`, RVA `0x1750`, guarded bytes
  `e93ee9180d565741564883ec30488bfa`.
- Loaded module identities in the capture header: XINPUT1_3.dll SHA-256
  `60cd2bcbb97f3a51892ff55ed6e93b2ee0955eb913031dacde885b50e7095dac`;
  xinput1_4.dll SHA-256
  `cc119f987bf33731fcd22c3a9bbcc675f31d80593a4c26428b363d6eedef974b`.

Corpus path and lines / export identity status:
- `ACU - Decompiled/14273A8B0.c:50,68` shows a four-byte user-index value
  passed with stack buffer `v50` to `sub_1428F53F6`, then the call result is
  tested against zero. The corpus export correspondence to this installed
  executable remains unverified.
- The live caller's stack sample at `[RSP+0x20]` is that 16-byte XINPUT_STATE
  buffer. The live code bytes above, rather than the corpus filename, guard the
  instrumented sites.

Archive identity / byte range / range hash / file ID (hex string): not applicable.
Stored and decoded lengths / SHA-256: not applicable.

Tool/provider/version / evidence IDs or saved output:
- Local Windows x64 MSVC build of `tools/native_probe/probe.cpp`; injected
  `local/native-probe/acu_probe_redirect.dll`, SHA-256
  `CF63F03771674B1F8D4DCC0130AE08138BCA4670F9B76BB4621C21EC82CEA1CB`.
  The probe change conditionally redirects RCX only when the API entry sees
  original RCX 1 and the exact return address `0x14273A922`; it records original
  and resulting RCX, return address, registers and caller state. The process
  stayed on the verified game image; no game file was modified.
- PID 24364; Python 3.12; local ViGEm controller replay. Run 1 trace SHA-256
  `4008DE0BCDB70977C06378D4FCE49F1CCFD866FA37E7F50C6E746C400990DD41`
  (93,836,644 bytes), timeline SHA-256
  `EDD2FBAFFB9A4A289882DF984FD72E91527E09F0BC610885801A2C5340D09950`.
- Run 6 used `tools/native_probe/controller_autoplay.py`, SHA-256
  `1E64B6C4D6E0FABFD72B25C1795D7C7E5C5962FF7A140A75D7361DB79851BFD4`; it
  identified the actual XInput slot with a brief `[0,12000]` left-stick probe.
  The current helper SHA-256 is
  `6648B2A92AD4C174DE61B7E727A66CEAE1866C233B572A58F3186EC7E730CF5D` and
  explicitly verifies release before replay, recording calibration metadata.
  On this host, ViGEm's API reported target
  index 0, but changed state appeared at XInput index 3. Run 6 repeated the
  replay with every-call entry/return hooks: native trace SHA-256
  `9CDEBA640D0BCF93C543C73771F298B3077AD5CCE04E25B30AACD4D335CF74F5`;
  timeline SHA-256
  `662823E151506D3C6C9AF50BF5F3AB2CBEFC33728949B3AB05432DACD884600E`.
- Follow-up linked memory capture: 37 sampled pose contexts, five
  transform-class owner candidates, 201 frames. Sample SHA-256
  `4C11D7AE5344174137BDD54342AB475E11DB774DE0652B1336FE065483FE0C62`;
  native trace SHA-256
  `6CE6F24C5867EAC4240FCB04E551555BCC83F467CB7B20A9262F28346345ADFD`;
  timeline SHA-256
  `F8BD8FA51D2F887C8C057883361267997473D2C0F7CF6C4F09A36D16A97A7442`.
- Evidence is retained under
  `local/native-probe/actor-arc-20261020/xinput-redirect-live/`. Forward and
  backward screenshots are `run-1/screenshots/006_forward_controlled.png`,
  `run-1/screenshots/008_backward_controlled.png`, and
  `run-3/run/screenshots/001_forward.png`,
  `run-3/run/screenshots/002_backward.png`.
- Corrected linked replay evidence is under `run-7/`: 197 memory frames,
  37 sampled pose contexts and five transform candidates. Sample SHA-256
  `DC5BD44648E8EC0F313C2101805A7C425BECE8443BAFD8135458B95FB5B74099`;
  native trace SHA-256
  `A9401164F01396B107737B0B0188BCC1DC0FA313BC0230B6FABF3D5B253E1998`;
  timeline SHA-256
  `356244B9582A4D1718E6DED859B89166C0DF27CA9C254180A38C8F2FA7FA43B6`;
  screenshots `run-7/run/screenshots/001_forward.png` and
  `run-7/run/screenshots/003_backward.png`.

Exact commands and search boundary:
- Generated live XInput entry guards with
  `python tools/native_probe/xinput_profile.py 24364 local/native-probe/actor-arc-20261020/xinput-redirect-live/input-profile.json`.
- Injected only into the SHA-verified ACU PID using
  `local/native-probe/acu_probe_launcher.exe --attach 24364 --dll local/native-probe/acu_probe_redirect.dll`.
- Replay and conditional redirect were orchestrated by
  `local/native-probe/actor-arc-20261020/xinput-redirect-live/run_redirect_capture.py`.
  Caller matching joins API-entry RDX (the XINPUT_STATE pointer) to the
  post-call sample address and QPC order; an index alone is not used to label a
  state. Follow-up pose/owner scan and capture are
  `local/native-probe/scan_current_contexts.py 24364 .../run-2` and
  `local/native-probe/actor-arc-20261020/xinput-redirect-live/capture_linked_pose.py`.
- The corrected no-redirect every-call replay used
  `local/native-probe/actor-arc-20261020/xinput-redirect-live/run_every1_capture.py`;
  linked pose/owner sampling with observed slot 3 used
  `local/native-probe/actor-arc-20261020/xinput-redirect-live/capture_linked_corrected.py`.
- After each capture all execution hooks were cleared; the conditional
  redirection was explicitly disabled with `UNREDIRECT 0`.
- The revised helper passed `python -m py_compile tools/native_probe/controller_autoplay.py`
  and a live slot/release check: ViGEm-reported index 0, observed XInput index
  3, then neutral state at index 3 before replay. Runs 6 and 7 used observed
  index 3 as their runner readback and guard.

## Result

Observations:
- The XINPUT1_3 entry at RVA `0x4EE0` reached the same ACU return site with
  several original user indexes. During run 1, a conditional RCX `1 -> 0`
  redirect affected 6,716 sampled index-1 entries; 6,715 were paired to the
  exact returned state buffer. Every paired index-1 buffer held
  `(-3356,-1869)` and successful RAX `0`.
- An initial every-two-events trace appeared to associate `(0,18000)` and
  `(0,-18000)` with some index-3 calls, but this did not reproduce consistently.
  A later every-call capture sampled all four indexes and removed that
  aliasing. ViGEm's API reported target index 0, but the controller report was
  visible at XInput index 3. The runner's brief `[0,12000]` calibration pulse
  identified index 3 and was released before the replay.
- In run 6, the runner read XInput index 3 and packet numbers 3 through 7 for
  neutral, `(0,12000)`, neutral, `(0,-12000)`, and neutral. At the guarded ACU
  call, 134 matched index-3 output buffers contained `(0,12000)` in the forward
  window and 244 contained `(0,-12000)` in the backward window; 26 and 15
  additional samples respectively were neutral between updates. Indexes 0, 1
  and 2 returned `(-3356,-1869)`. All matched calls returned RAX zero; no
  redirect was active.
- The sparse linked capture in run 7 sampled 37 candidate pose contexts and
  five transform-class objects over 197 frames while the corrected runner
  read back index 3. Multiple candidate transforms and pose arrays changed.
  Two moving candidates reversed some displacement between the forward and
  backward windows, but neither is assigned to Arno. Fresh screenshots show
  Arno in the Paris scene; no sampled object is linked to him yet.

Inferences and alternatives:
- The guarded ACU call receives the controller report through XInput index 3 in
  the corrected every-call capture. On this machine, the ViGEm-reported target
  index does not identify its observable XInput slot. This proves report
  delivery to this caller, but not that this caller alone drives Arno.
- The index-1 baseline may reflect another connected input source. Autonomous
  crowd movement and the user's earlier manual repositioning are alternatives
  for owner displacement.

Unknowns / incomplete coverage:
- Whether the guarded caller is the only input path used by Arno, and which
  other caller/device states feed gameplay. Indexes 0-2 retain a nonzero
  baseline even after the ViGEm target is mapped to XInput index 3.
- The identity of Arno's transform and pose context, native coordinate axes,
  units, active animation clip/time, root-motion ownership and matched-phase
  comparison with the Rust reconstruction remain unresolved.
- This capture is not a steering-rate measurement and makes no locomotion,
  bone-motion or visual parity claim.

Independent comparison / falsification test:
- The every-call trace already correlates all four caller indexes to their
  returned buffers. Next, use a wider route and bind one moving transform/pose
  candidate to visible Arno. Compare that identified actor and phase against
  Rust before tuning motion.

Reproduction steps:
- Keep the ACU executable SHA and module byte guards above. Rebuild the local
  probe from the recorded source hash, attach to a fresh verified ACU process,
  regenerate XInput profiles for the new PID, and use
  `run_every1_capture.py` with `all-index-every1-profile.json` for caller/output
  matching. Run `capture_linked_corrected.py` for synchronized pose and owner
  sampling. The earlier `run_redirect_capture.py` is retained as evidence that
  redirecting index 1 to 0 did not provide the controlled state. The saved
  profiles, input plans, timelines, traces, screenshots, module lists and run
  summaries are under the evidence directory above.

Implementation implications and finding-linked constants:
- No Rust locomotion constants were changed from this capture. Keep the current
  authored turn limits separate from native results until actor/input identity
  and phase-matched trajectories are established. The measured ACU sites and
  bytes are input instrumentation guards, not recovered gameplay constants.

## Fresh Ubisoft-launched session recheck (2026-10-20)

The earlier process (PID 24364) faulted at the bone-dispatch probe RVA
`0x988e3`; see finding0022. It was restarted through Ubisoft Connect as PID
16136. The ACU SHA-256 and image base remained
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40` and
`0x140000000`. The in-memory probe DLL was
`acu_probe.dll`, SHA-256
`4B0BBF8BD4FC88110D65485B0CD680A3E6C3F4267254B5E624E7B0F6DB441373`.
No game files were changed.

The new process loaded a different XINPUT1_3 entry signature at VA
`0x7FFFB6454EE0`: `e9eebda306574883ec20488bda8bf9e8`. The previous guarded
profile correctly rejected it before installing hooks. A derived XInput-only
profile was created at
`local/native-probe/actor-arc-20261020/xinput-redirect-live/xinput-caller-sparse-profile.json`;
it has no bone-dispatch hook. On exit, the probe reported all four hook
addresses as zero. The probe DLL remains resident in memory only.

The controller runner again detected XInput index 3 and read back neutral,
forward `(0,14000)`, reverse `(0,-14000)`, and neutral states in run 20. Its
native event ring had 182,419 dropped/overwritten events and yielded no event
records, so the screenshots are visual diagnostics rather than caller-state
proof. Run 21's decimated XInput-only trace captured 264 ACU caller returns;
all sampled 16-byte buffers contained the persistent `(-3356,-1869)` stick
baseline and returned RAX zero. The XINPUT1_3 entry probe sampled user index 3
ten times, but these records were not sufficient to align a returned state to
the caller or one input window. Runs 21 and 22 ended their native session with
code 1 while ACU remained responsive; run 22 recorded no events. These traces
do not verify controlled state delivery to the caller in this new process.

Run 17 was an XInput-only profile with one event per call; it recorded 3,576
XINPUT1_3 events, 3,556 XINPUT1_4 events and 3,538 caller events with a clean
session exit, but its screenshots were at the main menu and do not establish
gameplay input. Run 18 selected Continue and captured the loading transition.
Run 20 screenshots show the character in the Paris crowd before and after
forward/reverse replay, but because run 20 produced no native event records,
manual input or another input source remains possible. Do not treat those
images as a controlled movement comparison.

Hashes: run-20 native trace and timeline
`A4FE2796206AC4AA541230D5B90591E6665AD48628994CA4280119EEBF2139E4` and
`31A7EB672F50BD3A8D3AEEDB10A11652CFAB03D2694173F0D8EAB63F37DABDBA`; run-21
trace and timeline
`6FA1506BD4362B894CE874B260F133B26F6CAF4A63ECDD92A83664886180DCED` and
`447A2D3E96BC66DBF33A9D993264E5D6A5E96E6224A97C205765915B39F89F54`; run-22
trace and timeline
`A2C83F710EBB57D7FC1D4931ACD1C6F1E92D1DFF1CF45B2255A1089AC4E3833C` and
`11B1B3EC971FFE4C59FCB7F122C826C2DF3E63C778A22ED941E415E85F693AF4`.
Full traces, readbacks and screenshots are under
`local/native-probe/actor-arc-20261020/xinput-redirect-live/run-16/` through
`run-22/`.

Next, keep the current Ubisoft-launched process and use only XInput probes.
Stabilize native event draining and establish caller state/index correlation
before using input replays as locomotion evidence. Do not reinstall the bone
hook at RVA `0x988e3`. A direct pose-context/transform link and a matched-phase
Rust comparison remain open; no parity claim follows.

## Synchronized directional caller readback (2026-10-20)

The startup handshake in `tools/native_probe/controller_autoplay.py` now waits
for the session's flushed `armed` trace record before applying the replay. It
also waits for the trace file to be created and allows 35 seconds for session
cleanup. `tools/native_probe/client.py` can apply an optional conditional
XInput-index redirect from a profile before emitting `armed`; the redirect
profile uses a module name plus guarded return RVA. These changes were compiled
with Python 3.12. Run 29 exposed that the original resident `acu_probe.dll`
(SHA-256 `4B0BBF8BD4FC88110D65485B0CD680A3E6C3F4267254B5E624E7B0F6DB441373`)
does not implement the `REDIRECT` command; no input replay was sent. The old
probe was shut down and the existing redirect-capable DLL was attached instead
(SHA-256 `CF63F03771674B1F8D4DCC0130AE08138BCA4670F9B76BB4621C21EC82CEA1CB`).

Run 30 used PID 16136, the installed ACU image SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, image
base `0x140000000`, and the guarded XInput1_3, XInput1_4, and caller sites
listed above. The input timeline recorded index-3 readbacks of neutral,
forward `(0,14000)`, reverse `(0,-14000)`, and neutral. The native trace
contained 170 sampled XInput1_3 entries (all caller index 2), 170 XInput1_4
entries (all caller index 3), and 85 post-call records at ACU RVA `0x273A922`.
The caller returned RAX zero in every record. After decoding the 16-byte
`XINPUT_STATE`, all 29 forward-phase caller samples contained left stick
`(0,14000)`, all 29 reverse-phase samples contained `(0,-14000)`, and each of
the six neutral-pre and six neutral-post samples contained `(0,0)`. The packet
numbers advanced across phases. The conditional index 0-to-3 redirect at return
VA `0x14273A922` was active in the probe, but none of the sampled XInput1_3
entries met its condition; the caller received the input through the normal
index-3 path. All 442 captured
events were drained with zero event loss and the session exited 0. On exit all
four hook addresses were zero.

Fresh start, forward and reverse screenshots were visually inspected. Arno is
seen from behind after forward and faces the camera after reverse, consistent
with the scripted directions. This is evidence of controlled input reaching
the caller with a visible gameplay response. It does not measure world
trajectory, identify a current-process transform or skeleton, establish root
motion ownership, or compare against Rust. The older run-26 to run-28 baseline
traces are diagnostic only; run 30 is the controlled input-delivery result.

Run 30 native trace SHA-256
`1395C99EC38D12611914335018B1FC008A17F4F9897C6B938AC456AE4099EF38`; timeline
SHA-256 `0EBB7468663FD130CF16B6FBE64866714155B585722F9E678578C278CAE7B37D`.
Screenshot SHA-256 values: start
`A3D120E0A723D87CFD7FC02D02F9D737C76364CBEE2A3DFF0E52EC0BCDDA55A8`, forward
`24AC3819F66B617474398AF1DB71273FC9C3DB04A1069FC2FA078C02932A2338`, reverse
`2E94C4AC2EE294DA6CAEF0EECCACC94E20133DD4200C861C1B554ECBBC674D26`.
Evidence is in `local/native-probe/actor-arc-20261020/xinput-redirect-live/run-30/`.
The synthetic `every=128` profile and the combined entry/caller redirect
profile are in the same local evidence directory. Next, find a safe current-PID
route to the already decoded pose context and a directly measured root owner;
do not reinstall the bone-dispatch hook at RVA `0x988E3`.

## No-pulse gradual turn arc (run 37)

Run 37 reused PID 16136 and the same installed image hash. The controller
runner used previously observed XInput slot 3 explicitly, skipped the
calibration stick pulse, and verified its first non-neutral report. The 8.6 s
plan began with 1.2 s neutral, held forward `(0,14000)` for 1.5 s, stepped
through eight 300 ms directions to right `(14000,0)`, held right for 600 ms,
then stepped back through the reverse arc and ended neutral. The screenshot
checkpoints show the open bridge scene and visible orientation changes during
the arc. This removes the known calibration-pulse movement from the start of
this replay, but the world position and facing still are not numerically tied
to Arno's native transform or root motion.

The native trace contains 1,149 events: 489 XInput1_3 entries, 448 XInput1_4
entries, and 212 caller post-call samples at RVA `0x273A922`. Phase-aligned
caller snapshots carry forward, the eight arc directions, reverse arc, and
neutral-post states as scripted. The pre-phase caller snapshot retains the
persistent `(-3356,-1869)` baseline despite the slot-3 neutral readback, so it
is not a clean neutral baseline. Session exit code is zero, thread-slot
conflicts are zero, and all four hooks are clear in the final status. The
status reports 12 cumulative dropped/overwritten events; do not interpret that
cumulative counter as a per-run loss count.

Run-37 native trace SHA-256
`1328F497198429E6D24FEC34F5CF4384FF311D9E3D426E3A97DF85057DEBF858`, input
timeline `4D277C93BF95F8BEEC33EB2B91567408C65217D6F464136C3E30084445AF8AB4`,
and runner metadata
`B03884E0952FB6ED6F080005448D26E45A015047562E686237EF44FC6F467C04`.
Start and arc-08 screenshot hashes are
`453D2C64FA167E1623D13E5C21CEEAF347044B5E4DAB2BE7C1F9A88B4B1B1553` and
`CF785E4997D7A442469B86165869D42363DDEEA9A1B7106FE7D60713339C49A2`.
Files are under
`local/native-probe/actor-arc-20261020/xinput-redirect-live/run-37/`.

## Candidate transform sampling during the same arc (run 38)

Run 38 repeated the 8.6 s input plan in PID 16136 with the same executable
hash, using explicit XInput slot 3 and no calibration pulse. All 20 timeline
readbacks match the plan; the neutral-pre slot-3 report was zero, while the
caller's pre-phase baseline remains `(-3356,-1869)`. The runner exited 0.
The same start and arc-08 screenshots were inspected; Arno is visible on the
bridge near the right rail, but camera motion prevents screen-space displacement
from serving as a root measurement.

While the arc ran, a read-only sampler read each candidate transform position
at `owner+0x50` for 220 frames, 63 candidates from run 31. Twenty-eight
candidates per frame passed the finite-value and `w≈1` filter. The largest
start-to-end displacement was at `0x136002A10`: from
`(-217.4440,389.7108,-0.26935)` to `(-218.0841,389.4077,-0.26935)`, or 0.6401
units across the first two stored position components; its largest adjacent
sample step was 0.0281 units. The next largest candidate moved 0.0760 units.
The leading candidate's
position changed during forward/arc phases and again during the return arc,
with a small nonzero change during neutral. This is a controlled, smooth
moving-transform candidate, not a proven Arno root: the candidate-to-player
and candidate-to-pose links are still absent, the pre-phase native input is
not neutral, and the bridge rail may constrain movement. No native hook was
installed for run 38; the attached probe's hooks remained clear.

Run-38 owner sample SHA-256
`476B6F8B9C51DF96B588C2107DBA16C1D73A4F763A4BC5EC53A7177934C99149`; runner
timeline `33D9D746D450C4BBD0C23733EC2D33B5CAACF2F769FCE1090A695062EE6E487C`,
metadata `C7FC132199D8B1FF70970A13D7FEDD1BEA6FD31FE10C5DB8B5F0189E3D1CA816`.
Start and arc-08 screenshots are
`ECE4B754D517B6F8C7632F034D709481D764A2DE0E673C9DCF56000EB3EA48C1` and
`09B31D41DBF54A149D98DB19CC366B37899840AD7FC04E1BD55C24904C28B424`.
All run files are under
`local/native-probe/actor-arc-20261020/xinput-redirect-live/run-38/`.

## Forward/reverse candidate rejection (run 39)

Run 39 used a no-pulse 1 s neutral, 3 s forward, 0.5 s neutral, 3 s reverse,
1 s neutral sequence at slot 3 while sampling the same 63 transform owners.
All five report readbacks matched the requested values, but the slot-3
neutral-pre report was again `(-3356,-1869)`. Twenty-seven candidates per
frame passed the position filter over 196 frames. The previously largest
run-38 candidate `0x136002A10` moved only 0.0061 units across its first two
stored position components over the whole run, so run 38's 0.6401-unit motion
does not repeat as player-root motion. Candidate `0x13AE8B400` moved 0.147
units during the nominally neutral pre-phase; subsequent direction phases did
not give a clean forward/reverse signature. Another candidate moved 0.260
units overall but also changed during neutral. The existing transform set
therefore still contains no player identity.

The screenshots show Arno already balanced on the bridge edge at start, then
moving along the narrow edge during forward and reverse phases. This is a
parkour/ledge interaction and is excluded from ordinary walk or turn-rate
evidence. The captured readbacks and transform samples are still useful for
rejecting a root candidate, not for native locomotion parity. No native hooks
were installed.

Run-39 owner sample SHA-256
`3F0550E7A7E2DCB7FE87B51D4D4E1861E41F6E9BE408799486B7AEC2366A3DE1`, input
timeline `5F223104EA5A38875918452BE17DF863E5CD47ABDA8D75F429A9FB64E9A5E8A9`,
metadata `122F4274D189AFF35F0188D38B110D13AA7104B3ECAA575D602EAC37FF0EC400`.
Start, forward, and reverse screenshot hashes are
`2CEAF0342DA5C0B6827768B79D1E721C8B65C3BD4F1D91822E3A02500902E197`,
`96D58191413784D81B2983D7423A281C5FA6CC3BA64713E663931FA95236CDB7`, and
`0CD2437EFD130B11F0490E2972BC5B1C161C5955C425968E648548BE75337821`.
Files are under
`local/native-probe/actor-arc-20261020/xinput-redirect-live/run-39/`.

## Crowd-constrained turn repeats and pose-buffer rejection (runs 40-43)

Runs 40, 41, and 43 replayed the same 8.6 s, 20-phase `turn-arc-controlled-input.json`
profile in PID 16136 after Arno was moved to a wider Paris square. Screenshots
show pedestrians and horse-drawn carts crossing and bordering the route. This
is a crowd-constrained traversal observation; it is not an unobstructed-path or
collision-free calibration. The runner used explicit XInput slot 3, with no
calibration pulse. The pre-phase slot-3 readback remained `(-3356,-1869)` and
the post-phase readback was `(0,0)`. The scripted arc readbacks matched their
requested values. All three captures completed with exit code 0 and installed
no hooks.

Run 40 sampled 63 transform candidates at `owner+0x50`; 28 passed the finite,
`w≈1` position filter per frame over 223 frames. Its largest first-two-component
displacement was 0.4345 at `0x13b6506b0`; the former run-38 leader
`0x136002a10` was stationary. Run 41 repeated the phases over 223 frames, but
the largest displacement was 0.2689 at a different candidate,
`0x13ae97fb0`. A bounded same-phase comparison of that candidate produced a
median directional cosine of 0.337 across the two replays, with accumulated
path lengths 1.426 and 2.418 stored units. These movements do not establish
Arno identity or a player-root transform; the candidates can belong to crowd
actors or scene objects.

The six run-31 pose-context candidates were sampled at `context+0x178` as
800-byte buffers (25 × 32 bytes; this length is the probe's sampling window,
not a verified structure definition). Two were readable throughout runs 40,
41, and 43. In run 40 both had one repeated hash. In run 41 one had three hashes,
with its first change after the scripted input had ended; this timing does not
link the change to player motion. Run 43 saved the full candidate bytes; both
valid buffers were byte-for-byte unchanged over all 233 frames. Their run-43
SHA-256 values were `4420eb26722998e9bf0b296a05f0a0d1507075b25ae0c3b25cc6795d94b4ee29`
and `9d7bb794e1a4e91ad6125d7e0789fee0ab69ae050ebb5256bd2c56dc2fc86adc`.
These are candidate pose-context buffers, not confirmed bone transforms, and
no candidate has a verified link to Arno. The previous bone-dispatch hook at
RVA `0x988e3` remains prohibited after its recorded crash.

Run-40 sample SHA-256 `E9B745EC9B3D938EE00A5B46EEB77AADAC7452F8170AAA92F3184F7C4B20E5F2`,
timeline `B5DD402A9A016AD702B7CBC2B02235DE868B2D5A64D2C99089DD39FBB4F804CA`.
Run-41 sample SHA-256 `AE3D8BC03930A554D68E7B315553AF027D5F5F8A3CB1BD45BF50686116BC7228`,
timeline `7E033548D8E75FB211EA61619F73186813A6948B6B1EB19414C6DB6E8BF3B6EA`.
Run-43 full-buffer sample SHA-256
`EA67B117E10E9E76E03728E0F2B42B72E9ABBF08DDB61006F3D3D1C727AC1FE4`,
timeline `D1FD5E19DC7C368A96468EBCFB938420BE8735A034C96E591B335CBD65F945B5`.
Selected run-40 start/arc-08 screenshot hashes are
`8BEC34079B16C4DD7372B41F96A887EFEF2F0845992AE225B526D68AF75C224E` and
`54EB419409B39729645D5A376FAD4A65756BB09A31F99D6A56BD722A91DE0BB2`.
Selected run-43 start/arc-08/return-07 screenshot hashes are
`FE8097DA0992B46C0C3C338F0CDF5BE5EF4939971BE5A3C5B445174B6148A401`,
`CA1B221D7430F8CFECE32E40720F3BC529A6F1277F097E94E641D420E3203D30`, and
`6E372E9CAFFB4EB4457C6D869DD98A21A32B5AEFE1F06782EAC6B0942EE0BF0E`.
Evidence is under
`local/native-probe/actor-arc-20261020/xinput-redirect-live/run-{40,41,43}/`.
Run 42 was a duplicate capture whose attempted full-array field change did not
take effect; it is not used for pose-buffer conclusions.

The installed target was reopened for a focused static check using REA's IDA
adapter v1, bound to this executable SHA-256. Its binary session reports that
`resolve_containing_procedure` is unavailable in this provider; two retries of
`analyze_function` at VA `0x1400988e3` returned unreadable output. The direct IDA
MCP reported no sessions. No static function claim is made from that attempt.
The next useful slice is to identify Arno's live transform and rendered pose
buffer through a new runtime relationship, then compare matched phases with
Rust; do not tune against these unidentified candidates.
