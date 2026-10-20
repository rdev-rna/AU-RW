# ACU-RE-0028 — Model-space pose stage and skeleton bone-modifier writers

Date: 2026-10-20. Task: motion parity plan P1/P2 (`docs/PLAN_MOTION_PARITY.md`).

### Coat solver translation evidence (2026-10-20)

Live target remains ACU.exe SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
PID 24480, base `0x140000000`. REA's IDA provider was explicitly rebound to
that executable. Analysis below uses the live bytes through the project's
acu_tap pipe and Capstone, rather than the zero-filled disk code regions.
`tools/native_tap/coat_inspect.py` saves bounded chain state and both routines:
`local/native-probe/s2-maxlookat/coat-code-45510-24480.bin`, 11,368 bytes,
SHA-256 `d1f0ebe6b178c7a2225db41182b233d2224cf8a246d0c1f836a484152cdac12e`.
The apply body hash agrees with the earlier live binding capture.

Observed live instructions: `0x14004552e` tests solver `+32`; `0x140045549`
copies xmm1 (dt), `0x140045575` tests u16 node count `+18`; `0x140045590`
passes r8 (pose component) to the update helper. Corpus
`ACU - Decompiled/140045510.c:512-1560` supplies the control-flow hypothesis:
Verlet prediction, four constraint sweeps, inverse-mass distance correction,
and shortest-arc local rotation output at node `+288`. Node `+224/+240`
retain current/previous positions; `+208` retains the preceding position.
These semantic names are inferences to be checked by replay.

Read-only snapshot: all 21 chains contain one or two nodes; all have null
collision containers (`solver+24`, `node+104`) and zero wind amplitude
(`node+144`). The default acceleration constant at `0x142e78030` is
`[0,0,-9.81,0]`; `0x142e77fe0` is `[1,0,0,0]`.
Unsupported collision, wind and external-reference variants must fail explicitly
in the bounded port. No parity claim follows from these static observations.
`coat_capture.py` records atomic subsolver entries/exits with complete live node
state, world pose frame and dt (new `SNAP ... xmm1` in acu_tap13).
Experimental replay result (2026-10-20): `acu_motion::coat` translates the
bounded no-wind/no-collision subsolver; `acu_tap_bridge::coat` decodes captured
state; `coat_gate` compares atomic entry/exit snapshots. Reproduce from
`acu-rs/`:

```powershell
cargo run --locked -q -p acu_tap_bridge --features acu_motion/hw-rsqrt --bin coat_gate -- ../local/native-probe/s2-maxlookat/coat-visible-idle-24480.bin ../local/native-probe/s2-maxlookat/coat-visible-idle-24480.json
```

3,200 calls, 2,688 solved, zero unsupported; worst 0.006673167 degrees /
3.419879e-5 units. Default math fails: 0.03020005 degrees / 1.086480e-4 units.
Native packed/scalar addition order and the CPU reciprocal-square-root estimate
are material at these world coordinates. Joint 78 uses custom gravity through
node `+80`; snapshots 15/16 now retain the handle and resolved vector. The first
node's cached quaternion at `+256` seeds the solver; recomputing it from the
parent model pose loses precision. These corrections were independently checked
against live constraint-pass snapshots (`coat_capture.py --stages`).

This is an idle subsolver result only. Walking, POKE falsification, apply wrapper,
asset decoding and integration into `modifier_frame` remain open. Reset,
collisions, wind and external frames are explicitly unsupported; the alternative
cone mode has no live coverage. Do not mark the coat class complete.

State: MaxLookAt, PositionConstraint and OrientationConstraint are **live-gated**
(Mode A per-apply replay, below; worst 3.96e-5 deg / 2.9e-7 units over 59,354
applies, with POKE falsification). Hinge (worst 3.4e-3 deg) and Reflex (worst
1.8e-3 deg; 2.6e-5 deg with the CPU's `rsqrtss` estimate, lookups then
bit-identical) are ported and live-gated too. The large unattributed writers are
page neighbours outside the array. The evaluation order (per-LOD list stable-sorted
by the u16 at modifier+36, per-bone runs with a channel post-blend) is recovered,
ported and matches the traced frame (138 exact positions + 10 equal-key same-bone
ties out of 148). Whole-frame gate (143 live frames): the level-5 list has 307
modifiers in eight classes. The coat class (`0x142e77580`) is now bound to its
live apply and subsolver targets, but is not ported. Two classes
(`0x142e77e20`, `0x142e77d40`) remain unbound/unported. The 287 joints
independent of them match to 2.8e-3 deg / 7.7e-6 units.

## Provenance

- Binary: installed `ACU.exe`, SHA-256
  `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, length
  30907608, image base `0x140000000`.
- Live coat-class binding (2026-10-20, PID 24480, `acu_tap12.dll` SHA-256
  `0fa9f50099a4fe99c65e76eddf467cba514e5487b6919789d76f32677e152e44`): the
  saved whole-frame event contains 25 objects recorded with vtable
  `0x142e77580`. A live `READ` of 16 qwords at that vtable shows slot 11 is
  `0x140045240`. A five-second byte-guarded DR-hook at RVA `0x45240` observed
  19,800 entries, exactly 792 for each of the 25 objects; entry `a0` matched
  each object pointer, with no other receivers. Guard:
  `488bc448895810488970185557415648`. Trace:
  `local/native-probe/s2-maxlookat/coat-vtable-apply-24480.bin`, SHA-256
  `cfbf8eac93de81d02c7b13b9ac9302afd9f2a1cfbf4a5445927000784a8af69f`.
  The captured live 714-byte function body is
  `local/native-probe/s2-maxlookat/coat-apply-runtime-24480.bin`, SHA-256
  `4afb09425cd8e61ffd39f120f91a0afda9c763bfbe18f174282d6a959450562a`.
  The live subsolver at RVA `0x45510` was independently guarded with
  `488bc44c894018488948085541564157`; its four-second trace observed 13,475
  entries across 21 unique receivers. Those receivers exactly match the 21
  unique pointers read from the coat objects' `+0x48` field (no unexpected or
  missing receiver). Trace:
  `local/native-probe/s2-maxlookat/coat-subsolver-24480.bin`, SHA-256
  `a003cf204f2aa22288e8e71c5105b4bae86b380ef19e5cec9a3cfa855244c580`.
  This establishes runtime dispatch and the object-to-subsolver relationship.
  It does not establish that the installed disk image's bytes match the
  address-named corpus pseudocode: the prior IDA database had zero bytes at
  these addresses. Preserve that uncertainty during implementation.
- Static image: preserved runtime mapping (file offset == RVA)
  `local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`,
  SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`,
  length 125980672.
- Corpus (analysis artifact, not proof of execution): `ACU - Decompiled/`
  files `1400988D0.c`, `140097EA0.c`, `140080FC0.c`, `1400994E0.c`,
  `140014A60.c`, `1400528E0.c`, `140073090.c`, `140072FC0.c`, `140093110.c`,
  `1400494A0.c`, `14001C680.c` (whole files), `140031620.c`, `140091870.c`.
- Live trace: `local/native-probe/wtrace-24480-b.bin`, SHA-256
  `92622a96dbd8b45f2d9ed359f38cf9ce01d0a573dc952daa435163245477ff43`, 44984
  WHit records of 376 bytes, session PID 24480 (Paris, player pose component
  `0x1e3ad5f50`, slot-2 array `0x1e3af7610`; addresses are per session). Captured
  with the in-process tap page-protection tracer (`tools/native_tap/tap.cpp`
  WWATCH, build `acu_tap7.dll`; that build has known stop-path races, the
  recorded hits are unaffected).
- Reference provider for names only: AnvilToolkit 1.3.7.0 decompiled with
  ilspycmd 8.2.0.7535 (`local/reference-atk/BoneModifierRegistry.cs`,
  `MaxLookAtBoneModifier.cs`); see ACU-RE-0014. Third-party names, not runtime evidence.
- Live gate captures (2026-10-20, same PID 24480, player pose component
  `0x1cf1c2cf0` found by its Arno-only idle leaf `00000003836e1d49`; the earlier
  `0x1e3ad5f50` had been freed): tap build `acu_tap10.dll` SHA-256
  `c3ed75d0c3ce89693177ef7a5750052ca553faf079d868be3ac1c2104a0ca458` from
  `tools/native_tap/tap.cpp` (adds `DRFILTER slot value arg`). Dumps
  `local/native-probe/s2-maxlookat/ev-<tag>-24480.bin` (+ `.json` meta holding the
  hierarchy bytes):

  | tag | class | events | seconds | input | dump SHA-256 |
  | --- | --- | --- | --- | --- | --- |
  | mla-idle1 | MaxLookAt | 13464 | 1.5 | idle | `8821f2618596cc16d5b8a86c9cc7c103554e37323f535e5c1a0f331ef705d0b4` |
  | mla-arc1 | MaxLookAt | 25704 | 3.0 | arc walk | `599feaed2d002c9dfcf37061ac5e864977e140887b9fdb92bc49063178b1de39` |
  | mla-poke1 | MaxLookAt | 8636 | 1.0 | idle + POKE | `e51a58f10d0c25987c7a32f3d29ead19d6b32757bd4ea1c6cca4a6d6613e313a` |
  | pc-idle1 | PositionConstraint | 6400 | 1.5 | idle | `1bb9a109366947bc5bd31598d5278ae8d705df9cf3ee358f525c6294a1fb2d0d` |
  | pc-arc2 | PositionConstraint | 12576 | 3.0 | arc walk | `dae1bc949f9e234fd64da82614bdce04a89887780674626161441781a739899a` |
  | pc-poke1 | PositionConstraint | 4192 | 1.0 | idle + POKE | `cd54667b1c69f946bf8f8079e94cfcace87dc037f52ab8ea74b15f6f483a7140` |
  | oc-idle1 | OrientationConstraint | 14784 | 1.0 | idle | `47deec4fd7e3f7439f3ff60ae1f428cfa9af0f4e19f0140f0ea820450bdf27e8` |
  | oc-arc3 | OrientationConstraint | 29792 | 2.0 | arc walk | `d1df260e5ac373cbe7e0553d86b68cd52c499de72624aaef491836027a38373c` |
  | oc-poke1 | OrientationConstraint | 15120 | 1.0 | idle + POKE | `28d78cd3aa58bcfa3fb3997a3a23e986b1022da78d83635e7b53f8c1978ddc33` |

  Events = entry + exit records. Arc walk = `s2-maxlookat/arc-walk-profile.json`
  (walk-magnitude stick, heading swept 0 to 300 deg over 5 s, slew-limited).
  POKE logs with old/new/restored bytes: `s2-maxlookat/poke-log-<tag>.json`.
- Tools: `tools/acu_static/callgraph.py` (capstone 5; `callers` is a vectorized
  E8/E9 rel32 scan of `.text`, direct calls only), `tools/native_tap/wtrace_sites.py`,
  `tools/native_tap/wtrace_chains.py` (inside/outside-array split with stack chains).

Commands (from the project root):

```
python tools/acu_static/callgraph.py <mapped.bin> callers 140080fc0 140097ea0 1400988d0 1400994e0 140091870 140014a60
python tools/native_tap/wtrace_sites.py local/native-probe/wtrace-24480-b.bin 1e3af7610 140049438 1400494fd 140049ef6 14004b61a
python tools/acu_static/callgraph.py <mapped.bin> disasm 14001c680 2000   # field compares at +0x50..+0x5c
```

## Observations

### Pose objects and composition

`0x140080fc0(ref, component, pos*, rot*)` reads the local pose object at
`component+640` and the model pose object at `component+648`; each pose object
holds a 32-byte-per-joint array pointer at `+376` (vec4 position with w=0, xyzw
quaternion) and a skeleton at `+368` whose `+104` hierarchy has 12-byte entries:
u16 parent at +0, u16 descendant count at +2, LOD bits 4..6 of byte +10.

- `0x1400988d0(first, count, model, local, lod)` composes local → model for joints
  in depth-first order: `model.q = normalize(parent.q * local.q)`,
  `model.p = parent.p + rotate(parent.q, local.p)` (rotate as
  `2*((w²-½)v + (q·v)q + w(q×v))`); joint 0 uses identity as parent; an entry
  whose LOD bits exceed `lod` is skipped together with its descendants.
  Normalization is `rsqrtps` plus one Newton step (constants 3.0, 0.5).
- `0x1400994e0` stores one joint's 32-byte model record.
- `0x140097ea0(j, model, local, lod)` derives `local[j]` from the parent model
  (`local.p = rotate(conj(parent.q), model.p - parent.p)`,
  `local.q = normalize(conj(parent.q) * model.q)`), recomposes j's descendants
  with `0x1400988d0`, and clears bit 27 (`0x08000000`) of the dword at `+140` of
  both pose objects. The same bit appeared in the live slot flags (handoff P1).

The slot-2 array traced previously is therefore the **model-space** pose
(`component+648`). `0x140091870` is a pose copy (full when joint = 0xFFFF);
it is why joints 50/60 appeared as writers, not a modifier.

### Weighted model-space set (shared by every modifier apply)

`0x140014a60(modifier, component, target{pos,rot})`: `w = *(f32*)(modifier+32)`.
If `w < 1e-5` nothing happens. If `w < 0.99999`, the current model transform of
the modifier's bone (bone ref at `modifier+16`: hash +8, cached index +12,
generation +14) is read with `0x140073090`; rotation becomes
`interp(current.q, target.q, w)` (`0x1400528e0`) and position
`current.p + (target.p - current.p)*w`. Otherwise the target is used as is.
Position w is cleared, then `0x140080fc0` stores it and runs `0x140097ea0`.

`0x1400528e0(out, a, b, t)`: first `normalize(t*b + (1-t)*a)`; returned if the
dot product's float bit pattern is `>= 1065174635` (dot ≥ 0.98935574). Otherwise
if dot ≤ -1 or ≥ 1, return b; θ = acos(|dot|) (`0x1428f7ea0`); if
`|θ| ≤ 1e-5` return b; else
`normalize(sin((1-t)θ)/sinθ · a + s·sin(tθ)/sinθ · b)` with s = sign(dot)
(`0x1428f8d50` = sin, disassembly `0x140052a09..0x140052b08`).

### Modifier classes, vtables and live attribution

Each class's Anvil type record (u32 class hash) is followed at +0x2c by a factory
thunk; the constructor it reaches stores the class vtable. Apply is vtable index 11.

| Class (hash) | Type record | vtable | Apply | `0x140014a60` call site |
| --- | --- | --- | --- | --- |
| MaxLookAt `49f4ca3e` | `0x1439c6ce4` | `0x142e779b0` | `0x1400494a0` | `0x1400494fd` |
| OrientationConstraint `75116750` | `0x1439c60d4` | `0x142e77780` | `0x140049db0` | `0x140049ef6` |
| PositionConstraint `d0c34a81` | `0x1439c6024` | `0x142e77710` | `0x14004b490` | `0x14004b61a` |
| Hinge? `324796c7` (843552455) | `0x1439c6820` (ptr slot) | **`0x142e77940`** (corrected) | `0x1400492a0` | `0x140049438` |
| Reflex `057daa86` | `0x1439c72f4` | `0x142e768c0` | `0x14004b640` | local set `0x140014b50` |

Shared constraint base vtable `0x142e776a0`. Hinge's class name comes only from
the AnvilToolkit registry value 843552455 = `0x324796c7`.

Correction (2026-10-20, image read): slot 11 of `0x142e778d0` is `0x140048860`,
not `0x1400492a0`. The 48 live player objects that reach `0x1400492a0` all carry
vtable `0x142e77940` (slot 11 = `0x1400492a0`). Which class hash owns
`0x142e77940`, and which class owns `0x142e778d0`/`0x140048860`, is not re-derived.

Write attribution (first matching return address in each hit's stack, offsets
from `0x1e3af7610` / 32): 141 joints were written through modifier applies,
4 hits each. Joints 52–54 receive PositionConstraint then MaxLookAt; Hinge writes
the arm/hand/leg chains (30–50, 60–68, 155–156, 292–439 subset); OrientationConstraint
writes helper/twist joints (8, 19, 26, 64–76, 93–107, 124–131, 354–376 ...).
Full table: rerun the command above. Writes with no modifier site on the stack:
RIP `0x14008af2b` (29431), `0x14008aeaf` (4704), composition `0x140098c43/47`
(3916 each), copy `0x14009196a..7d` (458 each).

Range split (2026-10-20, static re-analysis of the same trace;
`python tools/native_tap/wtrace_chains.py local/native-probe/wtrace-24480-b.bin
<mapped.bin> 1e3af7610 459`). The tracer guards whole pages, and its hits span
`0x1e3af7000..0x1e3afaff0`. The 459-record array is `0x1e3af7610..0x1e3afaf70`.
- **Inside the array (10,274 hits):** composition `0x1400988d0` (stores
  `0x140098a9b/9f` joint 0, `0x140098c43/47` joints 1–458), pose copy
  `0x140091870` (`0x14009196a..7d`, `0x1400919a7`), and model-record store
  `0x1400994e0` (`0x1400994f2/fa`, reached from `0x140014a60` in modifier applies).
  No other writer touches the array.
- **Outside the array (34,710 hits), so not pose writers:** every
  `0x14008af2b` hit is at `-0x3a4..-0xf` and every `0x14008aeaf` hit at
  `-0x5f0..-0x3a8`. Both are below the array base on the shared first page. Other
  outside hits are pose copies `0x140091870` into neighbouring blocks below and
  past the array. Observation: these are heap neighbours sharing the guarded pages.
  Which object owns them is unknown (the session has ended).
- The two RIPs are stores inside `0x14008AE60(dst, a, b, field, t, t4*, t255)`
  (corpus `14008AE60.c`; `0x14008af09` is a separate `.pdata` chunk of the same
  function). It is a typed-field lerp: the field descriptor's u16 `+4` is the data
  offset, and `+6 & 0xf` is the type: 0 u8 and 2 u32 fixed-point
  (`a + (b-a)*t255/255`), 1 f32, 4 vec3 (the pseudocode reads only `a`, so the
  result is `a`; this reading of the disassembly `0x14008af5f..` is unverified),
  5 vec4, 6 via `0x14008A260`. The stores are 0 → `0x14008af2b` and 1 →
  `0x14008aeaf`. Callers seen on the stack: `0x140090440` (corpus `140090440.c`,
  site `0x140090880`), reached from `0x14008F220` (corpus `14008F220.c`, site
  `0x14008f28a`, the `mask == 0xFFFF` path). It blends three objects with the
  pose-object shape (`+368` descriptor whose `+8` holds 8-byte field entries,
  `+376` data). Inference: this is a generic channel-set blend. It is not part of
  the player model-pose write set.

### MaxLookAt (`0x1400494a0` → `0x14001c680`)

Runtime fields, from the class property table at `0x1439c1320` (0x38-byte
entries, offset = `(raw >> 16) >> 2`), in serialized order: UpNode ptr `+0x40`,
LookAtBone ptr `+0x48`, LookAtPosOffset vec4 `+0x30`, LookAtUsedAxis `+0x50`,
UpUsedAxis `+0x54`, UpNodeControl `+0x58` (distinct enum type `f1ed581b`),
SourceAxis `+0x5c`; runtime-only bone refs (flag 0x80) for the up node `+0x60`
and look bone `+0x70`. Axis enum values 0..5 = X, Y, Z, -X, -Y, -Z (reference names).

Apply: returns unless LookAtBone and the component are non-null; if the solver
succeeds, target = (current owner model position, solved rotation) through the
weighted set above.

Solver (all vectors model space; `tiny(v)` = every |component| ≤ 1e-5):
1. `d = look.p - owner.p` (look.p = 0 if no look bone); normalize unless
   `|d|² ≤ 1e-6`; negate if LookAtUsedAxis ≥ 3.
2. Up node present: its axes (q·eₓ, q·e_y, q·e_z, verified by emulating the SSE
   block) and `u = up.p - owner.p` normalized unless `tiny(u)`. If UpNode ==
   LookAtBone and UpNodeControl == 0, the solver **clears UpNode permanently**
   (`*(modifier+64) = 0`), uses identity axes and `u = normalize(-owner.p)`.
   No up node: identity axes, `u = normalize(-owner.p)` (no tiny guard).
3. Up vector `up` = selected up-node axis when UpNodeControl == 1 (UpUsedAxis
   1/4 → Y, 2/5 → Z, otherwise X), else `u`.
4. Aim axis A = LookAtUsedAxis mod 3 gets `d` (unit default if `tiny(d)`); the
   second axis is chosen by SourceAxis mod 3 and is
   `normalize(cross(aim, cross(up, aim)))` (unit default if tiny); the third is
   the right-handed cross product of the two (default if tiny, else normalized).
   SourceAxis mod 3 == A returns failure; LookAtUsedAxis > 5 returns failure.
5. Negation of the second axis, exactly as compiled (raw compares verified):

   | Aim | Second | Negated when |
   | --- | --- | --- |
   | X | Y | SourceAxis == 4 |
   | X | Z | SourceAxis == 5 |
   | Y | X | **UpUsedAxis** == 3 |
   | Y | Z | SourceAxis == 5 |
   | Z | X | **UpUsedAxis** == 3 |
   | Z | Y | **UpUsedAxis** == 4 |

6. Columns X, Y, Z → quaternion (trace branch if `X.x+Y.y+Z.z+1 > 1`, else
   largest-diagonal with next table {1,2,0} at `0x1439c8d58`); negate if w < 0;
   normalize.

LookAtPosOffset is not read anywhere on this path.

Live object check (34 MaxLookAt objects of the player): vtable `0x142e779b0`;
bone refs at +0x10/+0x60/+0x70 hold a pointer, the u32 name hash (+8) and the
cached runtime joint index (+12, 0xFFFF when absent); weight +0x20 (1.0 on all
34); UpNode/LookAtBone pointers +0x40/+0x48; axes/control +0x50..+0x5c.
Joints 52-54: no up node, axes (0, 2, 1, 2), look bones 55/56/54.

### PositionConstraint (`0x14004b490`)

Live object (0x50 bytes, vtable `0x142e77710`; 16 objects on the player): owner
ref +0x10, weight +0x20, target pointer array +0x28, u16 count +0x32, flag byte
+0x34 (bit 1 set on every apply, bit 0 = offset), offset vec4 +0x40. Each target
record: node pointer +0, raw f32 weight +8, cached u16 joint index +12.

1. Return unless some target has a non-null node pointer.
2. `total` = sequential f32 sum of the weights of targets with a node pointer;
   return if `total == 0`.
3. `p = sum((w_i * model[j_i].p) / total)` over **every** target whose index is
   not 0xFFFF (null-node targets add position but no weight; as compiled).
4. Offset flag: `p += scale * rotate(model[parent(owner)].q, offset)`, parent from
   the skeleton hierarchy (`0x140073190`), scale = `*(f32*)(*(component+0x10)+0xc0)`
   (1.0 live), rotate = `0x142771c20` with constants 0.5 (`0x14525b950`) and 2.0
   (`0x14525b940`).
5. Target = (p, owner's current model rotation) through the weighted set.

### OrientationConstraint (`0x140049db0`, blend `0x14001d980`)

Same 0x50-byte layout (vtable `0x142e77780`; 56 objects on the player, 6 with
count 0). Disassembly `0x14001d9c0..0x14001dbd6`:

1. Return unless some target has a node pointer (the blend ignores it).
2. Blend over targets with an index: the first (or while `total == 0`, including
   unordered) sets `acc = q`, `total = w`; afterwards `s = w + total`,
   `t = 1 - (s > 0 ? total / s : 1)`, `total = s`, `d = (y*y'+w*w')+(x*x'+z*z')`;
   `d <= -1`, unordered or `d >= 1` gives `acc = q`; else theta = acosf(|d|),
   a = sinf((1-t) theta), b = +/-sinf(t theta) (negative for d < 0);
   `|theta| <= 1e-5` (`0x142e77ed8`) gives `acc = q`; else
   `acc = N((b/sin theta) q + (a/sin theta) acc)`. Initial acc = identity
   (`0x142e75be0`); with no indexed target the native output stays
   uninitialized (Rust: no apply).
3. Offset flag: `acc = acc * offset` (Hamilton, offset quaternion at +0x40).
4. `N` = `rsqrtps` + one Newton step with 3.0 (`0x142e780b0`) and 0.5
   (`0x14361ea60`); target = (owner model position, N(acc)) through the weighted set.

### Hinge (`0x1400492a0` → `0x14012acc0` → `0x140129a30`, angle `0x1401293b0`)

Recovered 2026-10-20 from capstone disassembly of the mapped image
(`local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`).
Shuffle-heavy blocks were turned into exact lane formulas with
`tools/native_tap/simd_symbolic.py`, a sympy emulator of straight-line SSE.
They were then checked numerically against closed forms (max difference
≤ 9e-16 over random inputs; the alternative compositions differed by 0.45 to 4.4).

Live object (0xc0 bytes, vtable `0x142e77940`, 48 on the player): owner ref
+0x10 (index +0x1c), weight +0x20, pivot +0x30, frame quaternion +0x40,
reference axis A +0x50, hinge axis B +0x60, sphere array pointer +0x70 with u16
count +0x7a (1 to 6), angle ramp +0xa0/+0xa4 (live 10°/90°, 25°/75°, 45°/90°),
distance ramp +0xa8/+0xac (0/0 on every live object). Sphere entry (32 bytes):
radius f32 +8, bone ref +0x10 (cached index +0x1c). An unresolved ref yields the
zero position at `0x14525fbc0`.

Apply: M = owner model matrix (row form). The extra `0x14012acc0` thunk forwards
its stack arguments unused. If |θ| > 1e-5, then `out = F⁻¹ · R(B_raw, θ) · F · M`
(row-vector order; column form `M F R F⁻¹`). F = (rows of the unnormalized
frame quaternion, pivot **unscaled**, w = 1). F⁻¹ = (transpose, −Rᵀt). R is
Rodrigues about the raw, unnormalized axis with sinf/cosf (`0x1428f8d50`/`0x1428f8a50`).
Otherwise `out = M`. `0x14002c1b0` converts `out` to (row 3, quaternion) with
the trace / largest-diagonal method (next table {1,2,0} at `0x1439c8d68`). It
does **not** flip the sign or normalize. The result goes through the weighted set
even when θ ≈ 0.

Angle `0x1401293b0(obj, M, M⁻¹, component)` (M⁻¹ is the general 4x4 inverse
with `rcpps` and two Newton steps, `0x140129a90..0x140129cef`):
1. Return 0 if count = 0 or B or A is tiny (all four lanes ≤ 1e-5).
2. s = entity scale (`0x140030b00`, 1.0 fallback `0x1436d6410`).
   A' = q·A, B' = q·B, C = B'×A', P = M(s·pivot), Cm = M₃·C.
3. For each sphere (radius r = s·radius, p = sphere bone model position):
   v = M⁻¹₃(p − P). Distance falloff is 1 when both ramp values compare equal
   to 0 (`ucomiss`). Otherwise it is `ramp(|v|, s·lo, s·hi)`.
   v₂ = v − (v·B')B'; α = `0x140129100(v₂, A', B')` = ±acos(v̂₂·Â')
   (Newton-normalized; negative when B'·(v̂₂×Â') < 0; 0 for tiny v₂; π
   when the dot is ≤ −1).
   If r > 1e-5: α −= asinf(r/|v₂|) when |v₂| ≥ r; otherwise α += −π/2 and
   r = |v₂| (`0x1428f812c` = asinf, constant −π/2 at `0x142e77fd0`).
   If α ≥ 0 the contribution is 0. Otherwise probe = reject_A'(M⁻¹₃(r·Cm + p − P))
   (or reject_A'(v) when r ≤ 1e-5), c = `0x140129300` = clamp(n̂(probe)·C, −1, 1)
   (1 for tiny probe), and contribution = −(ramp(|acosf c|, lo, hi) · α · falloff)
   with `0x1401299d0` as the ramp.
4. θ = max(0, contributions) (`comiss`/`jae`, so NaN propagates).

`ramp(x, lo, hi)` = clamp((hi − x)/(hi − lo), 0, 1) when |hi − lo| > 1e-5;
otherwise 0 if x > hi, else 1.

### Reflex (`0x14004b640`; direction `0x14005b0e0`, lookup `0x14006cf00`, set `0x140014b50`)

Recovered 2026-10-20 from the corpus files `14004B640.c`, `14005B0E0.c`,
`14006CF00.c`, `140014B50.c`, `140084E30.c`, `1400980C0.c`, `140027B00.c`,
`140093210.c`, `140093260.c`, `142771D40.c`, and checked against capstone
disassembly of `0x14006cf00..0x14006d6b8` (tree walk, clamps, constants).
Vtable slot 11 of `0x142e768c0` read from the mapped image is `0x14004b640`
(guard `488bc4488958104c8948205556574154`).

Live object (0x40 bytes, 15 on the player, owners 28, 29, 57, 70, 71, 73, 74, 75,
77, 130, 152, 154, 360, 404, 406): owner ref +0x10 (index +0x1c), weight +0x20
(1.0 on all), base record pointer +0x08 (base local position +0x30, rotation
+0x40), a second list +0x28 with u16 count +0x32 (64-byte entries, weights from
`0x140028110`; empty on every live object, not ported), and the connector list
+0x34 with u16 count +0x3e (24-byte entries; one per live object). Connector
entry: u16 sample block +0, kind byte +2 (0 = three samples, else nine samples
from two lookups; only kind 0 is live and ported), record sub-index +3, record
owner +8 (`*(+60) + 96·sub` is the 96-byte record), cache owner +0x10 (joint-ref
table `+16`/u16 count `+26`, component `+8`, 32-byte cache records `*(+40) + 32·sub`).
Record: frame offset quaternion +0x00, joint-ref indices u16 +0x18/+0x1a/+0x1c
(frame joint, from joint, to joint), tree pointer +0x1e (unaligned qword),
per-lane position scale +0x30, stride i32 +0x4c, int16 sample table +0x50
(16-byte rows: xyzw rotation then xyzw position; u16 row count +0x5a).

1. Direction (`0x14005b0e0`): a ref index past the table gives `+X`. Joint reads
   go through the model pose object: when bit 26 of its `+0x8c` flags is clear
   (live value `0x80000000`), position = world.p + rotate(world.q, p) and
   rotation = world.q · q, world transform at pose object `+0x00/+0x10`. F =
   q(frame joint) · offset; v = p(to) − p(from); `+Y` when every lane is within
   1e-5; otherwise rotate(conj F, v) normalized with `rsqrtps` + Newton. The
   rotate dot product is summed `x + (y + z)`.
2. Lookup (`0x14006cf00`): from node 0, `mask = movmskps((x·r0 + y·r1) + z·r2)`
   over the node's three rows; `code = i16 at node + 0x30 + 2·mask`; node +=
   4·code until `code & 3 ≠ 0`. If `code & 1`, node += 4·code − 8 and the sign of
   `dot3(node, d)` selects u16 +0xc/+0xe, shifted left 2. Leaf = tree +
   `((code & ~3) << 4)`: rows A, B (w cleared), C (raw), plane N, with u16 ids
   +0x0c/+0x0e (A row), +0x1c/+0x1e (B row). Live leaf normals are
   (C − A) × (B − A). u = clamp01(dot3(C − A, d × A)/dot3(N, d)),
   v = clamp01(−dot3(B − A, d × A)/dot3(N, d)) (NaN → 0, `comiss`), w = 1 − u − v.
   Edge points use weights + 4.9999999e-5 (`0x142e7e4a4`): CB, CA, BA are
   normalized (4-lane `rsqrtps` + Newton). Arcs use a polynomial acos
   (`0x142e7ba80..0x142e7bac0`, π/2, π) on clamped dots; chords are
   max(0.005, |x − d|). ratio_i = max(φ_i, chord φ_i)·chord θ_i /
   (max(θ_i, chord θ_i)·chord φ_i) for (φ_CB, θ_A), (φ_CA, θ_B), (φ_BA, θ_C).
   Weights w·r0, u·r1, v·r2 over their sum (floored at 0.01), stored with ids
   (+0x0c, +0x0e, +0x1c) and leaf id +0x1e in the cache record.
3. Apply: start from the base record; for each weight above 0.0005 (bit pattern
   973279855, signed), q = int16 xyzw · `0x38000100` (1/32767), p = int16 xyzw ·
   position scale. rotation = rotation · s(q, w), position += p · w. s is an
   approximate slerp from identity (Blow-style correction: k = 0.58549219 ·
   (1 − 0.82279688 · q.w)², `q` negated when q.w < 0, cubic in t mirrored at 0.5),
   normalized with `rsqrtps` + Newton. Position × component `+0xca4` (1.0 live).
4. `0x140014b50`: the local-space weighted set (current local via the local pose
   object when weight < 0.99999), position w cleared, `0x140099950` stores the
   local record and `0x1400980c0` recomposes the joint and its descendants with
   `0x1400988d0`, clearing bit 27 on both pose objects.

### Mode A live gate

Capture (`tools/native_tap/modifier_capture.py`): DR entry+exit probe on the
apply, filtered on r9 = player pose component; game-thread snapshots of the
modifier object, the full local and model arrays at entry and exit, constraint
target records and the entity scale. Replay (`acu-rs` bin
`modifier_gate <dump> <meta>`): `ModelPose::from_parts(hierarchy, entry local,
entry model)`, decode the live object, `apply`, compare with the exit arrays.
Errors are computed in f64 (`angle = 4 asin(|a-b|/2)`).

| class | applies (idle / arc / poke) | owners | worst rot err deg (idle / arc / poke) | worst pos err | max native edit |
| --- | --- | --- | --- | --- | --- |
| MaxLookAt | 6732 / 12852 / 4318 | 34 | 3.26e-5 / 3.50e-5 / 3.11e-5 | 2.9e-7 | 179 deg |
| PositionConstraint | 3200 / 6288 / 2096 | 16 | 3.51e-5 / 3.25e-5 / 3.31e-5 | 1.8e-7 | 0.17 units |
| OrientationConstraint | 7392 / 14896 / 7560 | 56 (50 active) | 3.48e-5 / 3.96e-5 / 3.45e-5 | 2.6e-7 | 95 deg |
| Hinge | 9984 / 15936 / 10176 | 48 | 1.50e-3 / 3.39e-3 / 9.40e-4 | 2.0e-5 | 61.4 deg |
| Reflex | 3030 / 5700 / 3135 | 15 | 9.57e-4 / 1.76e-3 / 2.68e-4 | 4.1e-6 | 33.0 deg |

Reflex captures (2026-10-20, PID 24480, `acu_tap10`; snaps 1 object, 7 base
record, 8/9 model/local pose object headers, 30 scale, exit 10 cache records;
static connector bytes in the meta `reflex` key, read by
`tools/native_tap/reflex_static.py` right after the dump):
`ev-reflex-idle1-24480.bin` SHA-256
`329467cd1f5fe02be8c55da61071ebec0f503e31e07f9c74c57a1332b6976dc0`,
`ev-reflex-arc1-24480.bin` `b518905205b32ea61ab34761e72d3d15ed4d5e30d3db225700c0285da5ab0225`
(arc walk through `controller_autoplay.py`, slot 3, run `autoplay-reflex-arc1`),
`ev-reflex-poke1-24480.bin` `e1f70a68d035d2c0dae5fceff04e2af43decf556dea1986b7738aa6498a9bbf6`.
Gate outputs `gate-reflex-{idle1,arc1,poke1}.txt`. Every Rust lookup matched the
native cache record's leaf and sample indices (11,865 of 11,865); weight error
max 9.1e-5. With feature `acu_motion/hw-rsqrt` (this CPU's `rsqrtss` as the
Newton start; `gate-reflex-*-hwrsqrt.txt`) the weights are bit-identical in all
11,865 applies and the worst rotation error is 2.6e-5 deg / 1.6e-7 units. The
default-build residual is therefore the estimate plus the `x + (y + z)` dot order
at world-space magnitudes (switching the world rotate from glam's `(x + y) + z`
order was needed for the bit match). Measured, not inferred. The same feature
on `hinge-arc1` gives 2.27e-3 deg (from 3.39e-3), so Hinge has another
residual source, not yet located.
Reflex POKEs (`poke_reflex.py`, `poke-log-reflex-poke1.json`, all restored and
read back equal): owner 70 frame offset × 25 deg about Y moved its native edit
26.7 → 3.7 deg; owner 74 weight 1.0 → 0.5 moved it 5.0 → 12.9 deg (local
interpolation path); owner 71 sample rotations with x/z swapped took it 7.4 → 0
deg. Rust tracked all three (worst 2.7e-4 deg; bit-identical lookups with `hw-rsqrt`).

Hinge captures (2026-10-20, PID 24480, `acu_tap10`): `ev-hinge-idle1-24480.bin`
SHA-256 `199ea9bc8070322f73bcc16574634feebf7b119eda13f436124c3cb493b709b4`,
`ev-hinge-arc1-24480.bin` `5a8115d35d5c83f3501390a4e225e9411e8e403274bcc07b9e0876177b9da365`,
`ev-hinge-poke1-24480.bin` `9697305b4ffca84f4e032536750e7617802099323e4f42848dde904ff871abd0`.
Gate outputs: `gate-hinge-{idle1,arc1,poke1}.txt`. Per-owner mean rotation error is
about 1e-6 to 6e-5 deg. The larger maxima (above 1e-3 deg) sit on owners with small
native edits (0.3 to 5 deg), where acosf near 1 amplifies the difference between the
native `rsqrtps` + Newton estimate and Rust's exact root. This is an inference,
not separately measured. Hinge POKEs (`poke-log-hinge-poke1.json`, all restored
and read back): owner 430's sphere radius 0.183 → 0.30 moved the native max edit
from 54.7 to 61.4 deg. Owner 332's angle-ramp end π/2 → 1.0 moved it from 45.5 to 43.2 deg.
Owner 334's distance ramp (0, 0) → (0.05, 0.4) moved it from 45.0 to 30.0 deg and
exercises the distance-falloff branch, which no live object uses. Rust tracked all three
(worst 9.4e-4 deg).

"Worst" covers the owner and every descendant; per-owner mean rotation error is
about 1-8e-6 deg. In every capture the native apply changed no joint outside the
owner's subtree. Nothing is fitted, so every frame is held out.

Falsification by POKE (restored and read back after each capture): MaxLookAt
joint 53 weight 1.0 to 0.5 moved the native edit 177.5 to 88.6 deg; joint 54
SourceAxis 2 to 5 moved it 179.0 to 8.6 deg. PositionConstraint joint 55 target
weight 0.5 to 0.125 moved the owner edit 0.068 to 0.173 units; joint 339 offset
change 0.090 to 0.107. OrientationConstraint joint 72 offset set to 40 deg about Z
moved the edit 23.4 to 95.1 deg; joint 127 second-target weight 0.5 to 2.0
changed it 6.4 to 5.5 deg. Rust, reading the poked live objects, tracked every
change within the tolerances above.

Constants read from the image: unit X/Y/Z/identity `0x142e77fe0`,
`0x1435f22e0`, `0x142e75bc0`, `0x142e75be0`; epsilons 1e-6 `0x1435eded0`,
1e-5 `0x1435f2db0`; -1.0 `0x142e77fcc`.

### Evaluation order (`0x1400210a0` build, `0x1400377e0` frame, `0x1400379e0` run)

Sources: corpus `1400210A0.c` (lines ~300–495), `1400347C0.c`, `14000E3C0.c`,
`14000DBB0.c`, `14000C630.c`, `1400377E0.c`, `1400379E0.c`, `140072190.c`,
`1400756F0.c`, `14007C170.c`, `140076310.c`, `140093080.c`, `140073F20.c`,
`140073CF0.c`, `140073DC0.c`, `140081A50.c`, `1400818F0.c`, `140099880.c`,
`140099750.c`, plus disassembly of the call site `0x140037b88` (`call [rax+0x58]`).

Observations (corpus/disassembly):
- Every live apply in all five gate captures returns to `0x140037b8b`, inside
  `0x1400379E0`.
- **Build** (`0x1400210A0`, called from the component rebuild `0x1400347C0`): the
  builder walks the component hierarchy (`component+624` → `+104`, 12-byte entries,
  count `+114`). For each entry it takes the host bone object's modifier array
  (`bone+100`, count `+110`), clones each modifier (vfunc `+24`), and appends it
  to a bucket. The bucket index is the parent's position in an ancestor stack
  seeded with `0xFFFF`: a parent equal to the previous joint is pushed, any
  other parent pops down to it. For a depth-first hierarchy this is the joint
  depth. For m = 5..0, list m (`component + 720 + 12*m`, pointer +0, u16 count
  +10) gets, in bucket order, every modifier whose bone ref is valid
  (`0x14007C170`, index ≠ `0xFFFF`) and whose bone LOD
  (`0x1400756F0` = bits 4..6 of hierarchy byte +10) is ≤ m. Each list is then
  stable-sorted (`0x14000E3C0` → `0x14000DBB0` merge sort, `0x14000C630`
  insertion sort) by the **u16 at modifier+36**.
- **Frame** (`0x1400377E0(component, f, i, skip*)`): `level = (u16 [component+232]
  >> 3) & 7`; level 0 does nothing. The function walks list `level` in runs of
  equal bone index (`0x140072190` = u16 at ref+12; ref = modifier+16). A run
  whose bone is in the optional u16 skip list is passed over. Levels 6/7 would
  index past the six built lists; whether they occur is unknown.
- **Run** (`0x1400379E0`): the bone's model transform is read once
  (`0x140073F20`). Each modifier in the run with byte `+38` bit 0 set is applied
  (slot 11, args: modifier, 4×4 matrix built from that transform, running
  index, component). Then, if the byte `b` = per-joint channel of the
  **local** pose object (`0x140076310` → `0x140093080`, a typed field through
  the same `+368` descriptor tables as `0x14008AE60`) is not `0xFF`, the bone
  is pulled back toward its run-start transform: rotation
  `0x1400528E0(start, now, b·0.0039215689)` is set first (`0x140081A50`:
  `0x140099880` + `0x140097EA0`), then position `start + (now−start)·w`
  (`0x1400818F0`: `0x140099750` + `0x140097EA0`). The post-blend runs even when
  no modifier in the run is enabled. The setters map through the pose object's
  reference transform unless bit 26 of dword `+140` is set. Inference: the
  getters apply the inverse, so the blend is the same in model space up to
  rounding. Not verified.

Live comparison (static re-analysis, no new run):
`python tools/native_tap/modifier_order_export.py local/native-probe/s2-maxlookat 24480
local/native-probe/wtrace-24480-b.bin 1e3af7610 local/native-probe/s2-maxlookat/order-24480.json`
(SHA-256 `bf815d7cca0898186b4c0e0e99a5992fad40c2d4110d7dd8d7c13d98fa3160f1`), then
`cargo run --locked -p acu_tap_bridge --bin modifier_order_check -- ../local/native-probe/s2-maxlookat/order-24480.json`.
- Inputs: slot keys/flags/bones come from the 169 dumped objects of the gate
  session (component `0x1cf1c2cf0`). The observed sequence is the first traced
  frame of the earlier write trace (component `0x1e3ad5f50`, same PID and
  character): 148 model-record stores labelled by class return site.
  Both frames in the trace carry an identical sequence.
- Result at level 5: 148/148 observed applies are in the predicted list;
  138 positions match exactly; the other 10 are 5 same-bone pairs with equal
  keys (PositionConstraint before OrientationConstraint on bones 144, 396, 456,
  325, 339), whose order comes from the bone's own modifier array, which was not
  captured. 0 mismatches. Levels 1–4 do not fit (observed applies are missing from them).
- Listed but never stored: 15 Reflex slots, expected, because Reflex sets local
  space through `0x140014b50` and composition, not the `0x1400994e0` store. The
  6 OrientationConstraints on bones 30/34/38/45/41/48 (weight 1.0, keys
  `0x0501`/`0x0504`, Hinge-driven arm bones) also never stored. Why is unknown.
- Observation: the order interleaves classes, so it is a data order (key, then
  depth, then host and array order), not a class order. The trace's per-frame
  sequence is compose (`0x140098a9b`), pose copy (`0x140091870`), compose, then
  the modifier list.

Ported as `acu_motion::modifier_order` (`depth_buckets`, `build_lod_lists`,
`bone_runs`, `run_post_blend`, `evaluate_frame`; 6 unit tests with independent
expectations: hand-ordered lists, `Quat::slerp`/`Vec3::lerp` for the
post-blend, child recomposition).

### Whole-frame gate (slot 1 → slot 2 at `0x1400377e0`)

Capture (live, same game process PID 24480, player pose component `0x1cf1c2cf0`,
459 joints): tap `acu_tap12.dll` (SHA-256
`0fa9f50099a4fe99c65e76eddf467cba514e5487b6919789d76f32677e152e44`; new
`SNAPMODS` snapshot kind in `tools/native_tap/tap.cpp`), injected next to the
quiesced `acu_tap10`/`acu_tap11` with `acu_tap_launch.exe --attach-only`.
`python tools/native_tap/frame_capture.py 24480 1cf1c2cf0 f2 1.0` → DR entry+exit
probe on `0x1400377e0` filtered on rcx = component; live prologue
`4c8bdc534154415641574883ec780fb7` (`mov r11,rsp; push rbx/r12/r14/r15; sub rsp,78h;
movzx …[comp+232]`). The .text bytes on disk are not plain code, so the guard is
the live prologue checked against the corpus entry. Dump
`local/native-probe/s2-maxlookat/frame-f2-24480.bin` (SHA-256
`3f4ac31a727dd9f5e9c9b5b51d353283c6f84f1bfd7d3781cdb7df987a44543e`, 143
entry/exit pairs, 0 dropped), meta `frame-f2-24480.json` (SHA-256
`11ab6136ac4940f2937c072c67ac9b5447a2dc5172c7a529fbefb5d733bd5b5f`). Replay:
`cargo run --locked --release -p acu_tap_bridge --bin modifier_frame -- <dump> <meta>`.

Observations:
- All 143 calls: level 5, list count **307**, no skip list (r9 = 0), and every run
  channel byte (local pose `+376` block via the `+368` descriptor) is `0xFF`. The
  post-blend is therefore a no-op in this capture and remains untested live.
- The level-5 list holds **eight** classes, all enabled. Five are ported:
  OrientationConstraint 56, Hinge 48, MaxLookAt 34, PositionConstraint 16,
  Reflex 15 (= the 169 objects of the order check). Three are not ported:
  - `0x142e77e20` ×105, apply (slot 11) `0x14004dcf0`. Its distance sum over
    `+77` entries is dead code; it ends in `0x140099c80(0x14002eb50(comp+3278,
    *(comp+16)), bone)`.
  - `0x142e77580` ×25, apply `0x140045240`. It converts the bone's model matrix
    to a quaternion at `+256`, may call `0x140033990`, and with a sub-object at
    `+72` runs `0x140045510` and sets through `0x140014b50`.
  - `0x142e77d40` ×8, apply `0x14004d490`. A stateful spring: it uses the dt
    stack argument (skips when `< 0.001`), history at `+128..+197` and a velocity
    limit of 30, then sets through `0x140014b50`.
- Result: with the three classes replayed as no-ops, the error is large (up to
  62.2° and 3.7e-2 units) and concentrated on chains 304–341. These carry
  `0x142e77580`/`0x142e77d40`; inference: coat/cloth dynamics. **Clean subset**: joints outside every
  subtree rooted at an unported class's bone, and outside the subtree of any
  ported modifier that reads such a joint (constraint targets, Hinge spheres,
  MaxLookAt up/look bones, Reflex connector joints; to a fixpoint). That leaves
  287/459 joints, with a worst error over 143 frames of **2.79e-3 deg / 7.7e-6
  units** (joint 53, MaxLookAt + PositionConstraint). Without the
  dependency step, joints 339 (constraint targets 327/328 in the coat chain) and
  429–431 (Hinge spheres) show 7.6°–16°. The dependency step accounts for them.
- The earlier order check used only the five ported classes; the native list
  interleaves the other three by the same key/depth rule. That interleave has
  not been checked against `build_lod_lists`.

## Inferences and alternatives

- Slot 1 → slot 2 deviations (~1.7° median, joints 53/54 etc.) are consistent with
  modifier evaluation on the model pose, not with blending. Not yet tested numerically.
- The second-axis negation on UpUsedAxis for Y/Z aims may be an original-code
  quirk; it is reproduced, not corrected.
- Which modifiers belong to the player's runtime skeleton (merged body/head/addon
  skeletons) and their evaluation order are not established beyond the trace order
  for joints 52–54.

## Unknowns / incomplete coverage

- Reflex: the nine-sample connector kind and the second list (`+0x28`,
  `0x140028110`) are not ported (no live player object uses them; the decoder
  rejects them). The live connectors' joint refs were taken from cached indices
  (generation `+14` = 1 = component `+3272` at capture); the refresh path
  `0x14002baa0` is not modelled. The steady-state zero edits of owners 28/29/71
  (POKE) suggest those joints are not rewritten by an earlier stage each frame;
  not verified.
- Hinge is ported and gated (section above). The scale
  path `0x140030b00` (through `0x1426f6680` from the owner ref pointer) was not
  traced. The gate uses `*(*(component+0x10)+0xc0)` (1.0 live), so a non-unit
  entity scale is untested. `0x14007c170` (sphere ref validity) is assumed to be
  "cached index ≠ 0xFFFF" (the sphere entries in the gated captures were not
  checked individually for a 0xFFFF index).
- Whole-frame parity needs ports of `0x142e77580` and `0x142e77d40`.
  `0x142e77d40` needs the dt argument, which the tap does not record because it
  arrives in xmm and on the stack, plus object bytes beyond `0xc0`. Whether
  `0x142e77e20` is pose-neutral is not established (its bones are not in the
  residual chains). The run post-blend has only been seen with channels = `0xFF`.
- Evaluation order is recovered and matches the trace (section "Evaluation
  order"). Still open: the bone-array order that breaks equal-key ties on
  one bone, the live run channel bytes (never captured; a gate would need
  local pose object `+376` data), the skip-list caller argument, levels 6/7,
  and why 6 arm OrientationConstraints never stored.
- RIP `0x14008af2b`/`0x14008aeaf`: resolved as out-of-array page neighbours
  written by the typed channel blend `0x14008AE60` (section "Write attribution").
  Their owning object and the vec3 lerp case are not established.
- LOD filtering in composition is not modelled; `rsqrtps` results are
  CPU-specific, the Rust port uses exact normalization (measured deviation
  at most 4e-5 deg and 3e-7 units on this CPU).
- `0x14003adc0` side effect after a set (`0x14002e010` predicate) is not modelled.

## Independent comparison / falsification

Rust tests (`acu_motion::bone_modifier`) check the solver against an independent
Gram–Schmidt basis via `glam::Mat3`/`Quat::from_mat3`, interpolation against
`Quat::slerp` and its nlerp region, the negation table, the UpNode reset, and
local/model consistency after a set, the PositionConstraint weighted average,
offset and null-node quirk, and the OrientationConstraint blend against
`Quat::slerp` chains and `a * b` offsets. `acu_motion::reflex` tests check the
polynomial acos against libm, the approximate slerp against `Quat::slerp`
(< 2e-3 rad), lookup weights at the vertices and the symmetric centre of a
synthetic octant triangle, plane routing, out-of-bounds and cyclic trees, frame
independence of the direction and the local set with recomposition.
`acu_tap_bridge` tests cover record
parsing (truncation, bad magic, oversized snapshot, nested entries), hierarchy
descendant checks and the object layouts. Live gate: "Mode A live gate" above.

## Implementation implications

`acu-rs/crates/acu_motion/src/bone_modifier.rs` implements the weighted set,
subtree recompose, interpolation, MaxLookAt, PositionConstraint and
OrientationConstraint and Hinge with the constants above;
`acu-rs/crates/acu_motion/src/reflex.rs` implements Reflex. Per-apply parity is
gated for all five. Full slot-2 parity also needs the evaluation order and the
unattributed writers. `acu-rs/crates/acu_tap_bridge` (outside the simulation
crates) reads tap dumps for Mode A replay.
