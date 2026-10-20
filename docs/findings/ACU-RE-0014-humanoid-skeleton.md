# ACU-RE-0014 — Humanoid skeleton modifiers and solver records

Date: 2026-10-19. Task: movement mission Phase1 / ACU-005.
State: partially established; archive structure, not runtime solver behavior.

## Provenance

Installed ACU.exe SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, preferred
image base `0x140000000`. No VA/RVA or export correspondence is established here.
DataPC.forge entry110, stored offset2438262, length48507518, stored SHA-256
`7d5f500049eeb22158005bbd74fb8c22b84b84501d226aa6bfc66c4b0aa2334f`.
Native-LZO decoded package length112976083, SHA-256
`dc857aaed9c39456ef246fab5231a512583f9143acd587469b73a0e4c9242400`.
ACU_BaseSkeleton_Male ID `00000006a522da4d`, payload range
`[42822599,42841885)`, length19286, SHA-256
`28cf74f0bc1ae0ad05504f4c91350914a28c30eb69ee7a51b73f4ca1e8da4ba1`.
Other original resource ranges/hashes are in finding0010 and ignored
local/skeleton-candidates-child.json.

Provider: original framing decoded independently with the native lzo.dll from
finding0010. Layout hypotheses from read-only AnvilToolkit.dll1.3.7.0 SHA-256
`940374775553172b06d55537c4db78048bc3206b9b2f72bcc61bacfdcdb57636`, ilspycmd
8.2.0.7535. Targeted classes saved to local/skeleton-re-20261019/*.cs and the
earlier local/reference-atk/. These are third-party schema references, not ACU
runtime observations. Do not copy their decompiled implementation into source.

## Observations

`python local/skeleton-re-20261019/probe.py` walks fields sequentially with exact
class hashes and counts, without marker searching, skipping unknown records or
resynchronizing. It consumes the full 109-bone male skeleton, 68-bone ACU_Base
and several additive rigs; output includes exact record ranges, native transforms
and all modifier fields in probe.json. Male has 32 OrientationConstraint,
6 MaxLookAt,4 PositionConstraint and2 Reflex modifiers; no pose groups.

Every modifier has byte0, u64 ID, u32 class hash, owner pointer, float blend weight,
and a boolean UseModifiedBoneMatrix, followed by class-specific fields:

| Class hash | Layout after common header |
| --- | --- |
| `49f4ca3e` MaxLookAt | up-node pointer, look-bone pointer, vec4 offset, four i32 axis/control values |
| `75116750` OrientationConstraint | counted target pointers, keep-offset boolean, vec4 offset |
| `d0c34a81` PositionConstraint | counted target pointers, keep-offset boolean, vec4 offset |
| `057daa86` Reflex | counted inline connector-info objects, counted inline ball-joint-info objects |

ConstraintTarget (`df638110`) holds bone pointer + float weight. Connector info
(`eb384b0a`) holds u32 connector index + vec4 position + quaternion components;
ball-joint info (`de5f5e63`) holds boolean, u8 joint index and i16 pose index.
Class names/numeric enum meanings follow the identified reference provider;
runtime evaluation and solver semantics remain unimplemented.

Skeleton trailing IKData is a header-only inline class `9a33101e` for Unity;
COMData `9a67179a` has two vec4s. ReflexSystem `d417bb8c` is a managed pointer
(tag0 plus managed boolean, or tag4 plus boolean), with string name and counted
measurement/element/connector/ball-joint/double-joint/bone-ID lists. Male has zero
measurements/elements/connectors/double-joints,2 ball-joints and6 bone IDs.
Each ball-joint has name, quaternion, three signed16 slots, an inline pose hull
(counted vec4s) and pose set (counted bone hashes, counted packed eight-i16 bone
records, pose count and vec4 position scale). Packed field interpretation and
actual Reflex evaluation are not established by reading their layout.

Broader Python hypotheses consume27 of28 DataPC bootstrap skeletons, including
Generic_Head_Male's pose groups; the sword addon fails finite-value validation.
This is diagnostic coverage only. Rust must claim only schemas it actually
implements and tests; unsupported modifiers, pose groups or solver variants must
still fail explicitly.

## Unknowns / next validation

### Stored transform consistency check

`python local/skeleton-re-20261019/check_hierarchy.py` tests four explicit
composition candidates against every internal parent link. It reads the full
sequential native-LZO probe output, records its SHA-256 in hierarchy-check.json,
and does not alter transforms. Hamilton quaternions in xyzw order give:
`global_p = parent_global_p + rotate(parent_global_q, local_p)` and
`global_q = parent_global_q * local_q` (quaternion sign treated as equivalent).

| Resource | Links | Maximum position error | Maximum quaternion component-distance error |
| --- | --- | --- | --- |
| ACU_BaseSkeleton_Male | 108 | 1.633e-7 | 4.153e-7 |
| ACU_Base | 67 | 2.465e-7 | 3.658e-7 |
| Generic_Head_Male_SKELETON | 117 | 4.935e-8 | 1.354e-7 |

Reversing quaternion multiplication produces errors near0.98-1.41; using inverse
parent orientation for position produces errors near0.092-2.609 native units.
Maximum quaternion norm-squared deviation across these resources is3.379e-7.
This supports that stored local/global composition for these three resources;
it does not identify gameplay axes, world unit scale, mesh bind matrices, root
motion ownership, modifier evaluation or all possible skeleton variants. The
head resource is diagnostic Python coverage, not supported by the Rust parser.

Rust now implements the four modifier schemas above plus typed inline IK/COM,
constraint targets and this bounded ReflexSystem variant. The explicit
`original_skeleton_subset` install test passes 19 of 28 bootstrap skeletons,
including the complete male and ACU_Base payloads. It checks the male payload
SHA-256, 109 bones, exact per-class modifier counts and the 2-joint/6-hash Reflex
system. Nine resources remain rejected (unknown schemas or sword-addon nonfinite
values); there is no opaque-data fallback. Two authored nested-input tests check
values, every truncation prefix, invalid class/pointer/boolean/count/float fields
and an unsupported Reflex list. Solver execution is not implemented.

Mesh bind transforms, hardware/mesh/skeleton
bone remaps, rotation conventions, names and root translation must be checked
numerically and visually before skinning. Skeleton global coordinates differ
from the assembled mesh origin; do not simply attach the two without evidence.
No movement, gravity, IK evaluation or animation-selection behavior is recovered
here. No asset extraction directory or original install mutation is required.
