# ACU-RE-0030 - Traversal candidate scoring and target tracking

Date: 2026-10-20. Related task: ACU-008 / motion parity P4.
State: partially established; scanner-to-root-warper connection unresolved.

## Provenance

- Installed executable identity from inventory: ACU.exe SHA-256
  `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`.
- Preserved mapped runtime image:
  `local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`,
  SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`,
  base `0x140000000`. This is historical memory evidence, not a fresh execution.
- Landing score VA `0x140177c40`, RVA `0x177c40`, corpus-declared size 2045;
  mapped range SHA-256
  `62f48b565c3889c14ebef306640e7f671c417a77a64acc22dcd283cb5e7143ec`.
- Corpus `ACU - Decompiled/140177C40.c:92-137` and `:154-155`;
  scan caller `14017F8E0.c:186-293,349-380`;
  surface classifier candidate `140178440.c`. Full corpus hashes and line counts
  in `local/traversal-research-20261020/manifest.json`.
- Tools: Python, raw exact ASCII/u64 pointer search (`type_seeds.py`), Capstone
  x64 disassembly, NumPy E8/E9/RIP-relative discovery. No REA/IDA session opened,
  no hooks installed, no original files changed. These tools inspect the explicit
  saved ACU image above; no For Honor catalogue or session is used.
- Saved core disassembly:
  `local/traversal-research-20261020/landing-score-disasm.txt`.
  Core mapped instructions agree with the scoring pseudocode. This is a local
  correspondence check, not a blanket export/build equivalence claim.

## Observations

The landing-score function subtracts candidate minus origin, masks to lanes
X/Y (`0x142e764f0 = ffffffff ffffffff 00000000 00000000`), and computes horizontal
distance. It rejects outside inclusive distance and signed Z-delta bounds.
This establishes lane use in this function, not a global units/axes convention.

For survivors:

```text
distance_term = clamp(1 - abs(preferred_distance - distance)/(max_distance-min_distance), 0, 1)
height_term = clamp(1 - abs(preferred_height - delta.z)/(max_height-min_height), 0, 1)
alignment = max(dot(horizontal_direction, supplied_direction), 0)
score = (distance_term * 0.3 + alignment * 0.5) + height_term * 0.2
```

The input direction is not normalized by this routine; alignment has no upper
clamp. For squared horizontal length <= `1e-6`, the native path keeps the raw
horizontal delta; otherwise it uses `rsqrtps` plus one Newton step. Debug text is
`Landing Point Score`, `Angle`, `Horizontal`, `Vertical`. Native rejection
returns zero; a zero score alone does not distinguish rejection.

Caller `0x14017f8e0` queries candidates using `0x14017a390`, checks a further
query `0x140182380`, classifies through `0x140178440`, and retains only strict
score improvements. One branch accepts classifier codes 1..11 or 21..25.
Several query/retry/fallback branches exist; these codes and query signatures
are not yet semantically decoded. The caller returns `best_score > 0`.
This is concrete evidence of geometric candidate selection; prediction horizon,
complete scan coverage and its owning player state remain unresolved.

The mapped direct-call search finds one caller of `0x14017f8e0`,
`0x1401a7000` (call site `0x1401a7160`). Its corpus `1401A7000.c:60-77`
supplies the search direction from the normalized difference of two input points
and parameters from receiver lanes at byte offsets 716..752. After another query
`0x140182740` (`:109-110`), it stores start/selected target at byte offsets
400/416 (`:130,142`); flag byte +436 conditionally transforms both through an
inverse frame. It also writes a horizontal direction at +128. This is an
important caller/data-carrier lead, not yet an identified native gameplay class.
Raw pointers to this function occur in several non-vtable-like method tables
(e.g. `0x1439f2d70` and `0x143a36f40`); there are no direct E8/E9 callers in the
saved-image search. Their dispatch and owners remain unresolved. Identical
offset numbers on unrelated objects must not be equated.

Type-name seeds in the same image:

| Type | Name VA | Pointer to name | Constructor candidate |
| --- | --- | --- | --- |
| ParkourAnalysisData | `0x142e83e18` | `0x143a34df8` | `0x14014cbe0` |
| ClimbAnalysisData | `0x142e83e50` | `0x143a34f58` | `0x140150f80` |
| ACUParkourTargetTracker | `0x142f00968` | `0x143dacd48` | `0x140b2b770` |
| ParkourAnchorTargetTracker | `0x142f00530` | `0x143dad0b8` | `0x140b2b0b0` |
| ParkourInteractionTargetTracker | `0x142eb1c68` | `0x143afbbf8` | `0x14055a500` |
| TrajectoryProjectionTargetPrecision | `0x142eb1630` | `0x143afcd28` | `0x14055a030` |
| IKDisplacementTargetPrecision | `0x142eb1b88` | `0x143afb1a8` | `0x140557f00` |

Constructor candidates are the code pointer at name-pointer location +56 in
these records; tracker constructors explicitly assign vtables `0x142f05ad0`
and `0x142f060c0`. Slot 26 points to `0x140b44580` and `0x140b458a0`, both
calling `0x140b431c0`. That common routine reads/caches two 16-byte outputs at
receiver +336/+320, invokes slot 27 (`0x141ff1560`), and sets a cache-valid byte
at +352 (`140B431C0.c:19-28,63-65`). Slot 27 uses receiver +136 and a positional
blend weight +184, calling `0x141fcdd80`; the latter has polymorphic target
position/orientation paths and transform math. These are target-adjustment
leads, not proof that this path consumes the landing score or warps root motion.

`ProbeScanner` sits beside Rift probe types; do not select it as the traversal
scanner just because of its name. A full-corpus bounded-keyword search for
`predictive|targetmatch|rootmotionwarp|traversalscan` returned no matches (exit 1).
An ASCII runtime string pass likewise found no exact requested system names.
Neither negative search excludes anonymous functions or differently named systems.
`AtomSelectMatchingPoseNode` is also present; this alone does not establish a
modern motion-matching architecture. Its constructor candidate `0x14009caf0`
has no exact-address corpus file.

Additional animation-side seeds (names only, semantics unresolved) are saved in
`local/traversal-research-20261020/root-seeds.json`: `AtomRTCPDescriptor`
(`0x142e82378`, pointer `0x1439dd278`, constructor candidate `0x1400f0700`),
`AtomDampRTCPNode` (`0x142e7edd0`, pointer `0x1439d9558`, candidate `0x14009c9c0`)
and `AtomAnimationRootNode` (`0x142e7f170`, pointer `0x1439d7888`, candidate
`0x14009c820`). The last two candidates have no exact-address corpus file;
do not infer what RTCP stands for or a scanner/warper connection from the names.

## Reconstruction boundary and next gate

First port only the recovered landing-score kernel in `acu_motion::traversal`.
Use explicit input bounds; do not invent scanner radii, horizon, units, collision
code meanings, animation selection or root-warp curves. Finite, ordered nonzero
bounds are the supported API domain; invalid inputs fail explicitly. Native
NaN/degenerate-bound behavior is not claimed. Default reciprocal-sqrt remains
portable; `hw-rsqrt` is a capture-machine validation option, not determinism.

Next trace callers of `0x14017f8e0` and the target precision vtables to establish
the selected target's consumer and animation displacement correction. Before
integrating a scanner/warper into Bevy, capture native candidate inputs, chosen
target and root delta in one owned traversal transition. Falsify with two
obstacle distances: a genuine target correction must change the motion endpoint
with the native target. No full scanner, dynamic root warper, gameplay integration
or visible parity is claimed by this kernel.

Reproduce type seeds:

Validation: formatting, workspace Clippy (no warnings), workspace tests (157
passed / 22 explicitly ignored), and workspace build passed. Three focused
scoring tests also pass with `hw-rsqrt`; these are geometry/domain tests, not a
native runtime replay gate. Preparation check, doctor and inventory passed;
executable hash is unchanged. Logs are under
`local/traversal-research-20261020/`. Initial workspace build failed because the
existing viewer locked its executable; after saving its command line and
stopping that process, build retry passed. The same reversal scenario was
restored (PID 20316 at checkpoint). No visible behavior was changed, and this
research did not perform a new visual comparison or native gameplay capture.

```powershell
python tools/acu_static/type_seeds.py local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin ParkourAnalysisData ClimbAnalysisData ACUParkourTargetTracker ParkourAnchorTargetTracker ParkourInteractionTargetTracker TrajectoryProjectionTargetPrecision IKDisplacementTargetPrecision AtomSelectMatchingPoseNode --output local/traversal-research-20261020/type-seeds.json
python tools/acu_static/callgraph.py local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin callers 140177c40 140178440
```
