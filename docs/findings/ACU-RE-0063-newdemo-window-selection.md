# ACU-RE-0063 — How NewDemo picks the window-entry clip

Date: 2026-11-14. Related: [ACU-RE-0062](ACU-RE-0062-automated-window-entries.md) (live
entries, replay), [ACU-RE-0045](ACU-RE-0045-decision-clips-and-window-sweep.md) (run-in blend),
[ACU-RE-0058](ACU-RE-0058-root-delta-extraction.md) (mirror bit).

State:
- **Observed offline:** the graph structure, parameters, thresholds and clips (from the bytes).
- **Observed in the binary:** the meanings of operators, If ports, the interval rule and condition
  chaining (runtime evaluators).
- **Inferences** are marked where they occur.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, base
  `0x140000000`. Pseudocode: `ACU - Decompiled/<VA>.c` (analysis artifact). The vtable slots come
  from the symbol-index database (other worktree, read-only).
- Asset: `DataPC.forge`. Index entry 1519, ordinal 1 (`acu-resource --entry 1518`, 0-based).
  File ID `000000013d5bc4d8`, `NewDemo`, type `c0c861cd` (AtomGraph, `crc-name`).
  Payload 42,174,002 bytes, SHA-256 `c8c358512edc2dc64c5c64cb9702f4bc9c478a5b50693a8d46c2f33ddd0589ee`
  (equal to the index's `payload_sha256`). The payload stays local, in `local/atomgraph/NewDemo.bin`.
- Tool: `tools/atomgraph.py` (`params`, `machine --at`, `find --clip`). Tests:
  `python -m unittest tools/test_atomgraph.py`. That runs 13 synthetic tests, plus one
  install-dependent test that is skipped (reported as unavailable) when the dump is missing.
- Decoded machines (local): `local/atomgraph/machine-37005472.json` (climb),
  `machine-13803765.json` and `machine-13829873.json` (ground run-in). Offsets are payload
  offsets.

## Format (observed in the bytes)

1. **Objects.** Each object has a header: 1 byte, then a u64 id, then a u32 class (CRC-32 of the
   class name).
   - Every Atom object in NewDemo has id 0.
   - The first byte varies (0, 3, 0x24, 0x3f, 0x64), so it cannot be used to find objects.
   - 59 Atom classes occur (list in the tool).
2. **Parameters.** NewDemo has 534 `AtomRTCPDescriptor`s, each a name, a type and CRC32(name).
   File order gives the index that ports and conditions use. Relevant here:
   - 197 `Direction` (int);
   - 261 `HeightDifferential`, 450 `SyncA_TargetAngle`, 455 `SyncB_TargetHeight`,
     468 `TargetAngle`, 481 `TargetHeight` (all scalar);
   - 366 `Parkour` (int); `HighProfile` (bool).
3. **Ports.** `AtomInputPort` is a u32 kind followed by data:
   - 0 = parameter index;
   - 9 = output of another node, by index in the state's node list;
   - 3 = constant f32 at +12;
   - 2 = small constant (0/1);
   - 4 = unused/default.
4. **Conditions.** An `AtomCondition` holds: u32 param, i32, u32 kind, u32 op, u32 combine,
   u32 (2 in the window machines), u8, u16 0xFFFF, u8 count. Then an `AtomDataContainerWrapper`
   follows: a u8 type (0 bool, 1 scalar, 2 int, 10 none), the scalar at +4, the int or bool at +8.
   - Kind 0 compares a parameter with the value.
   - Kind 2 is a group that owns the next `count` entries.
   - Kinds 1, 3 and 4 take other runtime paths (event or tag IDs, a time window). They are not
     decoded and are shown as `event(0x…)`.
5. **States.** An `AtomStateMachineNode` lists its states in order. Each state is either:
   - an `AtomGraphStateNode` whose nodes follow in order, ending with `AtomAnimationRootNode`; or
   - a nested machine.

   The machine's `AtomInitialState` rules come next. Each rule is an `AtomConditionExpression`
   (u32, u32, u32 count, conditions), then f32 1.0, then u32 n, then n parameter indices. The
   indices always equal the set of compared parameters, which the tool checks. A final u32
   follows (see below).
6. **Clips.** `AtomAnimationDataNode` is 321 bytes, with the clip file ID as a u64 at +293. It has
   four ports; port 2 is a kind-2 constant 0/1.

## Meanings from the binary

- **Operators.** `(u16 >> 3) & 0xF` selects: 0 `==`, 1 `>`, 2 `>=`, 3 `<`, 4 `<=`, 5 `!=`.
  - Sources: scalar `0x14009E7C0`, int `0x14009E690`, bool `0x14009EE70`.
  - Scalar `==` and `!=` use |a−b| ≤ 1e-5.
  - The serialized op byte is used directly (all 75,875 conditions are in 0..5). Data check:
    `LeftStickLength` uses only ops 0 and 5.
- **Condition lists.** `0x1400C3400` walks a flat list.
  - A group entry (kind ≠ 0/1/3/4) recurses into its `count` children (`0x1400C23D0`).
  - The combine field of each entry joins it to the next, left to right with no precedence:
    0 = and (evaluate next only if true so far), 1 = or.
  - A node's result is "evaluator returned 0" (`0x1400D6D00`).
  - **Inference:** the serialized u32 after `op` is this combine field. In the Down state, the
    two subgroups carry 1 and the outer group carries 0. Only "or" makes that expression
    satisfiable: (≥170 and <180) or (<−158 and ≥−180).
- **AtomIfNode** (`0x1400EB790`): port 0 is the condition, port 1 the value if true, port 2 the
  value if false.
- **AtomSelectFromIntervalNode** (`0x1400ECF70` → `0x1400A0730`).
  - Ports: input, opt₀, thr₀, opt₁, thr₁, …, optₙ.
  - It returns the first k with input < thr_k, otherwise optₙ.

## The window machines (observed; offsets in the payload)

**Climb entry, machine 37,005,472.** An identical copy sits at 37,850,748. Four states, entered
by `Direction`:

| Direction | State | Selection |
|---|---|---|
| 2 | Up | `HeightDifferential >= 0.2` → `WindowGratingEntry_Climb_Up`, else `WindowEntry_Climb_Up_v02` |
| 3 | Dn | event `0x0040001a` picks a flag-0 or a flag-1 branch. Then, if `SyncA_TargetAngle` is in [170, 180) or [−180, −158) → `Climb_Dn`; else an AxialBlend on `SyncA_TargetAngle`: −180 Dn, −150 Dn135Deg, 150 Dn135Deg, 180 Dn |
| 1 | Lt clips, flag 1 | `SyncB_TargetHeight` < −2.0 → `Lt-VeryHigh`; < −1.25 → `Lt-High`; < −0.3 → `Lt`; otherwise `UpLt45Deg`. Each band: `HeightDifferential >= 0.2` → `WindowGratingEntry_*`, else `WindowEntry_*` |
| 0 | Lt clips, flag 0 | the same tree, byte for byte, except the clip-node flag port and node IDs |

**Ground run-in, machines 13,803,765 and 13,829,873.**
- Entry is by `TargetHeight` bands (< 1.25, [1.25, 1.75) or [1.25, 1.85), ≥ 1.75 or ≥ 1.85)
  and by `HighProfile`.
- In 13,829,873, state 0 (height [1.25, 1.85)) blends by `TargetAngle`: 0° →
  `Fd0Deg150cm`, 90° → `Fd45Deg150cm`.
  - Event `0x00400030` picks `Footl` or `Footr`. The clip flag follows the sign of
    `TargetAngle`.
  - This is the `Fd45Deg150cm` + `Fd0Deg150cm` blend seen live (ACU-RE-0045, ACU-RE-0062
    obs. 2).
- The low-profile state (< 1.25, `HighProfile == True`) is a nested two-state machine using
  `Fd0Deg`/`Fd90Deg` (blend at 55°).
- Some states end in `AtomTimeScaleNode`, with constants 0.8, 0.85 and 1.1.

**Consistency with the replay (ACU-RE-0062 obs. 3).** The game played
`xx_WindowGratingEntry_Climb_Lt-High_tr_IdleHigh` at a window with a railing. Under this machine
that requires:
- `Direction` ∈ {0, 1};
- `HeightDifferential >= 0.2` (a railing is plausible);
- −2.0 ≤ `SyncB_TargetHeight` < −1.25.

For scale, feet z 7.06 against window top 8.50 gives −1.44. That is an unconfirmed candidate
for what the parameter measures.

## Inferences and alternatives

- **Rule targets.** We read the final u32 of each initial state as the target state's index.
  Evidence:
  - in every machine decoded, the targets of a machine's rules are a permutation of its states;
  - the rules that do not fit (repeated targets) are exactly the ones after a nested machine,
    which belong to the enclosing machine.

  The tool assigns rules on that basis and fails on a conflict.
- **Mirror flag.** Clip-node port 2 (0/1) is probably the mirror flag:
  - it is the only difference between the two Lt states;
  - in the run-in it flips with the sign of `TargetAngle`.

  Not verified against `playback+0x37E` (ACU-RE-0058). So which `Direction` (0 or 1) means
  "left" is open.
- **AxialBlend.** Ports are read as input, two settings, then (point, value) pairs; the constants
  ascend. **TimeScale.** The constant is read as a rate factor. Neither has been checked against
  the runtime.

## Unknowns

- What the game writes to `Direction`, `SyncB_TargetHeight`, `HeightDifferential` and
  `SyncA_TargetAngle` for a window, and when it writes them.
- The meaning of event IDs `0x0040001a`, `0x00400030`, `0x00400034`, `0x0040004f` and
  `0x00400007`.
- The u32 after `combine` (always 2 here), the u8 before the 0xFFFF entity field, and the first
  header byte.
- Transitions (`AtomStateTransitionTarget`), which are not decoded.

## Reproduction

```
acu-resource "<game>/DataPC.forge" --entry 1518 --fid 000000013d5bc4d8 --len 0 --out local/atomgraph/NewDemo.bin
python -I tools/atomgraph.py machine local/atomgraph/NewDemo.bin --at 37005472 --out local/atomgraph/machine-37005472.json
python -m unittest tools/test_atomgraph.py
```

## Implementation implications

- Window-entry selection is data: a port can evaluate these machines directly from NewDemo,
  given the parameter values.
- The remaining work is computing the parameters from geometry: `Direction`,
  `SyncB_TargetHeight`, `HeightDifferential`, `SyncA_TargetAngle`, `TargetHeight`,
  `TargetAngle`.
- Next: a live read of those parameters during the deterministic replay (`window_replay.py`),
  plus the playback mirror bit.
