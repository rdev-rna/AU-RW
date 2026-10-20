# ACU-RE-0018 — Navigation animation track subset

2026-10-19 / ACU-008. State: partially established, asset observations.

## Provenance

DataPC.forge entry 928, file ID `0000002633b2c79d`, stored offset 101271720,
stored length 123303092, SHA-256
`08b0a18bd2fa603ae2b77f2b17587c4f41c7154050552b5874edcd8c2bceda00`.
Decoded package SHA-256
`e33c8441ffc8b8189391b4078e212716e90bbcccc196230070f36328c1f927fd`.
Walk resource 4190 / `0000000fc08fd67a`, range 124046536..124070099,
23563 bytes, SHA-256 `b55338f07ddc5e8e9798988496d0e9aabda3717961c512f3fb5ec453b7c22cfb`.
Sprint resource 3106 / `000000016737c2c4`, range 98193112..98208100,
14988 bytes, SHA-256 `1b985aece0ca2f1ae230f456041cb21958171041cc2da0eb6b5a74c250d3c732`.
These are asset byte ranges, not binary addresses. No native execution claim.
Idle resource 4898 / `00000006af254769` (`xx_Nav_WaitSt_LookAt_Fd0`),
range 138016525..138018567, 2042 bytes, SHA-256
`1cdc7e7373367c6efe6c755d990dfd4cb29f542779d9213b9bf2dad72671e176`,
has 62 tracks, one-second duration and a stationary root.
Installed executable identity remains `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
image base `0x140000000`; no VA/RVA is assigned to these asset observations.

## Observations and interpretation

Bounded Python struct probes of the existing acu-mat payload hex establish a
common object header, duration/key, a mapping object class `0181efe8`, and repeated
16-byte mapping objects (zero u64, class `653caa76`, u32 track hash).
The walk mapping count is 79, sprint 81. Following mappings: zero i16 reference
count, category 60, two i16 track counts (second zero), sequential tracks.
The walk and sprint tables end at EXACT payload end. No external built-in tracks
occur in these samples. The first two tracks have hash zero and are vector root
translation and quaternion root orientation. Other track hashes include the
loaded male rig's existing thigh/knee/arm hashes.

Type / 3 selects value encoding; type % 3 selects byte times, ushort times,
or a bitmap with byte length and frame shift in its prefix. Time units of 1/60 s
are corroborated by the last root time equalling stored duration (2.8 s walk,
1.4 s sprint). Quaternion 24/48 encodings store three components and the omitted
component selector; 64-bit encoding uses two ordered u32 words. Schema/codec
hypotheses were inspected read-only in AnvilToolkit.dll 1.3.7.0, SHA-256
`940374775553172b06d55537c4db78048bc3206b9b2f72bcc61bacfdcdb57636`,
using ilspycmd 8.2.0.7535. Its Animation SupportedGames list is empty: this is
not an independent Unity decoder or a support claim. Diagnostic source files
and original payload reports are in `local/animation-matcher-20261019/`.

## Boundaries and falsification

Initial fresh `walk-36.png` exposed a doubled pelvis height and 90-degree frame
rotation. The male skeleton stores an external root placement (translation
z=0.9599999785, quaternion z/w=0.70710677) while its first bone `2c52cbb0`
has zero bone-global position / identity rotation. The clip's same hash instead
contains the placed pelvis (~0.95 z / ~90-degree quaternion). Remove the decoded
external root placement ONCE at this root before hierarchical composition.
This uses original skeleton fields, not a tuned render-height offset.
Diagnostic `local/animation-matcher-20261019/rig-110.json` preserves all values;
the scene report preserves mesh-to-skeleton matrices (~z=-0.964). The initial
failed capture is retained as a negative visual control.

The decoder locates a UNIQUE structurally validated mapping object behind opaque
event bytes, validates every mapping header, key/count agreement, channel sizes,
time ordering, quaternion finiteness and exact payload end. Integer event tracks
may have repeated timestamps (observed sprint tracks 67/73); transform keys must
increase strictly. Original encoded value bytes are retained. Events remain opaque,
and this is partial animation coverage. Unsupported codecs, external built-ins,
mapping ambiguity, truncation and counts fail explicitly. Actor ownership and
native graph selection are unknown. Navigation names are discovery hints;
applying compatible tracks to Arno is a reconstruction choice.

Runtime root/pose must share one clip and time. The adapter's profile eligibility,
search weights, blending, steering and collision policy are authored choices,
not recovered Unity matcher constants. Synthetic arena obstacle behavior does
not establish original parkour. Validate decoded poses visually and compare
rest/local frames and root displacement before claiming correct playback.

## Validation result

101 workspace tests pass, 17 install tests explicitly ignored in the default
suite. The requested install test separately passes four navigation vectors
(idle/walk/sprint/ladder), including malformed prefixes and trailing bytes.
Fmt, clippy `-D warnings`, build and all 20 Python tests pass. Eight final fresh
captures are indexed in `local/captures/acu-motion-20261019/manifest.json`.
Full skeleton provenance is retained by `acu-rig` output `rig-110.json`.

Reproduce partial independent frame check from project root:
`python local/animation-matcher-20261019/audit_pose.py` (existing NumPy).
Inputs: `walk-decoded.json`, `sprint-decoded.json` from acu-mat's Animation
decoder, `rig-110.json` from acu-rig, and final walk scene's mesh-to-skeleton
matrix. Independent NumPy quaternion interpolation / FK places decoded toe hashes
`058b96dd` and `e556c9be` in mesh space and adds the transformed root trajectory.
Samples: 60 Hz, within 0.025 source units of each toe's minimum height and vertical
speed below 0.3 source units/s. Walk has 67 / 75 samples: median planar toe drift
0.062606 / 0.023854 source units/s. Reversing root direction gives 4.772763 /
4.571495. This strongly supports the frame direction/placement interpretation
for walking. It shares Rust-decoded values and is not an independent codec oracle.
Sprint has zero qualifying samples; its stance test is unavailable, not passed.
No new native executable capture or runtime comparison was made.

DX12 process launched successfully, but both scoped OS keyboard reviews refused
input because the viewer could not remain foreground. Do not count these reviews
as live control validation. Existing automated binding tests still pass.
