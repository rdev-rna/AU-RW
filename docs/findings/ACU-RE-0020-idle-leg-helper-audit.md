# ACU-RE-0020 — Static idle and unevaluated weighted leg helpers

2026-10-19 / ACU-008. Asset observations, code inspection and reconstruction
adapter. Native modifier parity remains unverified.

## Provenance

Animation package: DataPC.forge entry 928, stored offset 101271720, length
123303092, SHA-256 `08b0a18bd2fa603ae2b77f2b17587c4f41c7154050552b5874edcd8c2bceda00`.
Decoded resource-frame length 206993148, SHA-256
`e33c8441ffc8b8189391b4078e212716e90bbcccc196230070f36328c1f927fd`.
Individual resource ranges/hashes/decoded tracks or explicit errors are in
`local/locomotion-audit-20261019/entry-928.jsonl`; `summary.json` hashes audit files.
Decoder: `acu_assets::animation`, through `acu-animation-audit`.

LegsRollbones skeleton: DataPC_ACU_LGS_BelleEpoque.forge entry 334,
file ID `000000070cf6709b`, stored offset 129748603, length 497250,
stored SHA-256 `42935733c7319be475e358525ef99582a31dbf9b375825c8802244b9d4229137`.
Resource 117, `ACU_Unisex_LegsRollbones`, ID `0000000fffabbc8e`,
decoded payload range 1403968..1408696, length 4728,
SHA-256 `bf1f445abf6402a6c920d612d699d358fde9285259c13c040d15d8c84533a4c3`.
Reproduce with `acu-rig`; retained structured fields are in
`local/rigged-world-20261019/avatar-rigs-v2.json`.

Leg mesh: same archive entry 679, resource 23,
`CN_P_Arno_FearlessAssassin_Legs_LOD0`. Full archive/payload identity,
hardware palettes and original four/eight-slot weights are retained in
`local/skin-bind-20261019/outfit.json` and current scene reports.
This outfit remains the displayed Fearless set, not resolved Tailored equipment.

## Observations

The previous idle `00000006af254769`, resource 4898,
`xx_Nav_WaitSt_LookAt_Fd0`, has one-second duration and **zero changing rotation
tracks**. A running clock cannot make that pose breathe or shift weight.
`xx_Nav_IdleLow_Footr`, resource 1772, ID `00000003836e1d49`, has duration
19.2333335876 seconds and 53 changing rotation tracks. The audit preserves its
exact decoded payload range/hash rather than inferring motion from the name.
That payload is 62000352..62057714 (57362 bytes), SHA-256
`dbac8b4f36c5bc1fc803bfe47706122ba53d9e0dea547c980dbf527f35959a62`.

The leg mesh has summed weight bytes 49,812 on `6f69051f` and 59,712 on
`0b7ad5a4`. The directly animated thigh hashes `176183f0` / `757f1291` receive
only 14,262 / 12,888 weight bytes. Shin hashes `060df401` / `863d09fc` receive
149,188 / 145,639. Consequently, validating direct thigh/shin animation channels
does not validate the upper-leg geometry. The helpers have no ordinary rotation
tracks in the selected walking clip, and they are parented to the pelvis.

The original skeleton records actual modifier dependencies:

| Output hash | Serialized relationship |
| --- | --- |
| `46a1b1ed` | MaxLookAt target `060df401`; up-node `66a4ede8` |
| `58e08e20` | MaxLookAt target `863d09fc`; up-node `7bb780de` |
| `6f69051f` | OrientationConstraint targets `46a1b1ed` weight 1 and `176183f0` weight 0.1 |
| `0b7ad5a4` | OrientationConstraint targets `58e08e20` weight 1 and `757f1291` weight 0.1 |
| `2afd6f95` | OrientationConstraint targets `6f69051f` / `060df401`, each weight 0.5 |
| `554fd4dd` | OrientationConstraint targets `0b7ad5a4` / `863d09fc`, each weight 0.5 |
| `e58f34b8` / `5df2e3e9` | Additional MaxLookAt helpers targeting corresponding shins |

The previous renderer retained their hierarchy but discarded modifier evaluation.
The observed stiff-thigh symptom follows directly from those original weights
and the ignored dependencies. This is an explanation of the Rust defect, not a
claim that the native constraint equations have been recovered.

## Implementation choice and falsification

Use the animated low-profile idle. Preserve original weights and references.
An explicitly authored, bind-relative helper adapter evaluates four aim and four
orientation dependencies in topological order, after animation and transition
blending. Aim follows the bind-to-target direction while retaining bind twist;
orientation blends target rotation deltas using the serialized weights. Parent
and target dependencies are ordered together; cycles/outside references fail.

Native MaxLookAt axes/control fields, up-node/Reflex behavior, exact quaternion
averaging, modifier scheduling, high-profile idle selection, cloth and foot IK
remain unresolved. This adapter closes a visible missing-evaluation gap; it must
be compared with native before claiming the original solver.

Synthetic validation checks independent expected 90-degree helper/child motion,
bind identity and invalid dependency rejection. The explicit install test
`rig::tests::installed_leg_helpers_and_idle_deform_render_mesh` exercises the
same `Skin::update` path that uploads render mesh positions, selects actual
vertices weighted primarily to the thigh helpers, and measures idle/walk/sprint
deformation. Disabling helpers is the negative control: a thigh helper must stay
fixed relative to its pelvis. Fresh visual captures are additionally required.

## Database coverage

`acu-animation-audit` attempts every Animation resource in a specified package,
recording unsupported input instead of discarding it. Across DataPC.forge
entries 110/928/1518, 9,682 animations are attempted: 3,317 decode and 6,365 fail
explicitly. This includes every entry in that archive with a navigation-name
hint in the existing metadata snapshot, plus all other animations in those
packages. It does not establish that other packages contain no unnamed locomotion.

`tools/locomotion_audit.py` imports exact occurrence/hash/range-checked results
into the existing local SQLite database: 261,388 decoded track rows, with
per-track key counts, codecs, byte spans, change metrics and provenance. Atomic
imports preserve annotations and reject changed source identities. The full
metadata snapshot yields 17,897 locomotion/traversal/idle name-hint occurrences,
5,662 distinct IDs. Of these, 1,340 are decoded, 2,751 explicitly unsupported and
13,806 have not been audited. Original actor ownership remains unknown; a CSV
candidate export is not advertised as all verified Arno animations.

## Native evidence boundary

Installed ACU.exe SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
preferred image base `0x140000000`. REA explicitly opened that target with the
IDA headless provider (adapter version 1). Its MaxLookAt string query returns
zero defined matches; the database exposes only 88 strings. Evidence ID
`ev_02bbad6130c14594a1782802a952e491ec6c279f50ce357f3dc5e79ae5fef307`.
This is a bounded search of provider-defined objects, not absence of the solver.

Independent PE inspection confirms `.text`, `.rdata`, `.data`, `.pdata` have
zero file-backed raw length; `.UBX0` / `.UBX1` contain loader/protection data.
No corpus address is therefore promoted directly to a verified installed hook.
`tools/native_image.py` reports zero accessible unwind ranges/RTTI vtables on
that disk image. A live mapped-image capture after unpacking is needed. The
script also accepts RVA-layout mapped captures with their actual module base;
synthetic PE tests independently verify unwind/RVA/RTTI decoding and corruption
rejection. No native runtime parity has been established by these static checks.

`tools/decompile_xref.py` now joins every address-shaped corpus filename to the
mapped unwind inventory and writes every native range, including unmatched rows,
to `native-functions.jsonl`. Exact start and range-containment joins are distinct.
The decompiler-to-installed-image relationship remains unverified, so these are
names for investigation, not verified symbols. `docs/NATIVE_DECOMPILE_XREF.md`
documents the command, output and evidence boundary.

## Completed validation and startup observation

The local transition weight now follows the For Honor matcher selector-3 f32
operation order `(t*t)*3 - ((t+t)*(t*t))` at the existing 160 ms duration.
This replaces the algebraically equivalent Unity adapter expression. FH source
evidence is sibling `docs/re/MATCHER_TRANSITION_CURVE.md` (FH executable SHA
`7ca57147a51c8451c7a3d5b3043a0abf8ea53c41d436d7eba655a91482316916`); it is an
explicit animation-quality experiment, not Unity evidence. The selector itself
is FH's code default; install data may override it. Quaternion slerp and local
hierarchy reconstruction remain ACU adapter behavior.

Fmt, Clippy with warnings denied, build, 107 workspace tests (19 explicit
ignored install tests) and 22 Python tests pass. Both explicit installed
directional and helper-mesh regressions pass. Actual maximum thigh-helper
weighted vertex displacement is 0.046107043 for idle, 0.3393989 for walk,
0.4947471 for sprint (decoded asset units); disabling helpers preserves their
pelvis-relative transform. Seven fresh PNGs were visually inspected; manifest
`local/captures/acu-leg-idle-20261019/manifest.json` records source/executable
hashes, inputs, replay traces, assets and limitations.

Those workspace gates precede the selector-3 operation-order edit above. The
new expression has not yet been rebuilt or visually replayed.

The native probe fixture validates capture/guard/rotation/shutdown behavior;
see `tools/native_probe/README.md`. Earlier ordinary and injected launches exited
code 42. After the user's latest launch update, injected PID 26372 reported
successful LoadLibrary, remained alive at three seconds, and answered STATUS;
the follow-up pipe connection failed and the process was immediately gone. This
is not a sustained gameplay session. At 500 ms the worker captured the entire 125,980,672-byte main
image, base `0x140000000`, no read failures, sequential/non-atomic. Manifest:
`local/native-probe/startup-25828-138507781.json`; mapped SHA-256:
`9f4e1012d3eb361249f3129b7cfc68743cda43e9c45e0277a246b9ac490cb5c2`.
The mapped-image inventory still returns zero unwind ranges and RTTI candidates;
the first 4,096 bytes of each `.text`, `.rdata`, `.pdata` are all zero in this
early capture, consistent with capture before those regions were unpacked.
capture completion does not prove that protection unpacking completed or that
native routines executed. No automatic native function offsets were installed.
