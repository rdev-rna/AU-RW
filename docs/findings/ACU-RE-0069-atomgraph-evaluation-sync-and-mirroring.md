# ACU-RE-0069 — AtomGraph evaluation: conditions, best-match selection, marker sync and mirroring

Date: 2026-11-15. Related: [ACU-RE-0066](ACU-RE-0066-atomgraph-stream-and-navigation-hub.md)
(stream, hub, transition order), [ACU-RE-0067](ACU-RE-0067-live-locomotion-parameters-and-blend-rules.md)
(live parameters, axial weights), [ACU-RE-0068](ACU-RE-0068-clip-event-intervals.md) (event
intervals), [ACU-RE-0057](ACU-RE-0057-body-velocity-from-root-motion.md) (root motion drives the
body), [ACU-RE-0027](ACU-RE-0027-binding-modes.md) (quaternion binding modes).

State: **partially established**. The evaluators, the event-condition modes, best-match selection
and the sync-record format are observed in the binary. Marker sync is confirmed live: Walk against
WalkSlow agrees to 3–4 decimals. Where the clip sync group and loop flag come from is an
**inference** from data (obs. 9). Damp nodes, masked layers and the stop-entry lag are open.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, image base
  `0x140000000`.
  - IDA (idalib MCP) database
    `local/native-probe/bone-trace-20261019/main-image.normalized.exe.i64` (local only), bound
    explicitly. It is not the For Honor session.
  - Pseudocode `ACU - Decompiled/<VA>.c` for every VA below.
- NewDemo AtomGraph:
  - `DataPC.forge` entry 1518 (0-based), file id `000000013d5bc4d8`;
  - payload 42,174,002 bytes, SHA-256 `c8c35851…89ee`;
  - decodes with the default `Limits`.
- Navigation clips: `DataPC.forge` entry 928, every resource that `animation::parse` accepts.
- Live run `local/native-probe/runs/loco-params-1` (ACU-RE-0067): 1,051 replay samples.
- Code:
  - `acu-rs/crates/acu_motion/src/atom_graph.rs` (interpreter);
  - `sync.rs` (records);
  - `graph_driver.rs` (pose, root motion and controls);
  - `acu-rs/crates/acu_bevy/src/player.rs` (arena integration).
- Diagnostics (examples in `acu_motion`): `clip_int_tracks`, `skeleton_mirror`,
  `graph_state_fields`, `clip_root`, `newdemo_payload`.

## Observations

1. **Evaluator vtables.** Each pose-node class vtable holds `0x1400CB210` followed by its
   evaluator, at slot `+0x98`:

   | Node | Evaluator |
   |---|---|
   | If | `EB790` |
   | Compare | `EB4D0` |
   | Cache | `EAF90` |
   | SelectInterval | `ECF70` |
   | GraphCondition | `EB6B0` |
   | AnimationData (clip) | `EA0C0` |
   | AnimationRoot | `EA210` |
   | AxialBlend | `EA930` |
   | TimeScale | `EE6B0` |
   | weighted blend | `ED510` |
   | vector3 | `EB5C0` |
   | SelectBestMatchingTags | `ECD90` |

   The clip class alone also has a record builder at `+0xA0` (`0x1400CC8A0`).
2. **Event conditions** (`sub_1400C7500`), with the mode in condition bits 9–10:
   - mode 0 holds when the event is present in the window;
   - mode 2 holds when the event is absent for the whole window, or when it is present and ends
     inside the window, in which case the window becomes [end, w1].
3. **SelectBestMatchingTags:**
   - evaluator `0x1400ECD90`: latch flag at node +92, chosen index (byte) at +88;
   - activation `0x1400D7640`: with a sync source whose record group equals port 0, it takes the
     source position; when its fraction ≥ port 1 (0.25) it steps with `sub_1400AFE60`;
   - search `0x1400E01F0`: the minimum of `0x1400CDE10` costs, strictly less wins; with no
     candidate it takes port 2;
   - graph-state activation `0x1400E7C00` passes a null source.
4. **Sync record** (`sub_14002C9B0`, built through `sub_1400CC8A0`):
   - Marker value layout:
     - bits 27–31 id;
     - bits 2–26 counter (the group in the source track);
     - bit 1 mirrorable;
     - bit 0 set only on the start marker `0xF0000001` and the end marker `0xE8000001`.
   - Record layout: (marker, seconds to next) pairs, total at +140, id at +144 (-2 = none), loop
     byte at +148.
   - Only keys whose counter equals the requested group are used.
   - When the clip instance's mirror flag (`+894` bit 0) is set and a marker has bit 1, the id
     becomes `~id`.
   - A **looping** record is rotated to start at its smallest id, and the offset of that key is
     returned. A **one-shot** record is `start, keys…, end`, with offset 0.
   - A group of -1 or -2 gives no markers: total = clip length, id -2.
5. **Clip evaluator** (`0x1400EA0C0`): it builds its record once, while the stored total is 0. It
   calls slot `+0xA0` with ctx = {args[0], args[1], group = args `+88`, loop = args `+98` & 1}.
   The graph state (`0x1400E7C00`) initialises args `+84` = -1, `+88` = -2, `+92` = 1 and
   `+96` = -1.
6. **Root evaluator** (`0x1400EA210`):
   - It keeps the position (`f80`) and phase (`f88`).
   - It reads the input record from the instance table (`+8960`, 152-byte entries).
   - It writes total / global speed to `f84`.
   - When the record id differs from its own `f92`, it resets the position from the phase
     (`sub_1401265B0`). Otherwise it re-reads the phase against the record (`sub_14011D2D0`).
   - `sub_140120520` sets a newly entered state's position by masked marker match, clamped by
     root `f96` when it is ≥ 0.
7. **Position → clip time** (`sub_1400B0A80`): each clip maps the shared position {frac, cur,
   next} through its own record to its own seconds.
   - Live check: Walk and WalkSlow clip times agree with this mapping to 3–4 decimals.
8. **Clip integer track hashes:**
   - `0xFFFFFFE0` events (ACU-RE-0068);
   - `0xFFFFFFE2` sync markers;
   - `0xFFFFFFE3` mirrored events.

   Keys with id 0 carry no interval.
9. **Root fields per graph type** (`graph_state_fields`, NewDemo):
   - Every loop graph checked (walk, walk sway, dense-crowd walk, sneak walk, sprint) has root
     `f92` = 0 and `f112` = 1.
   - Every walk-start (`PreNavWalkMid_*_tr_*`), stop and idle-part graph checked has `f92` = -2
     and `f112` = 0.
   - `f96` is -1 on most roots, 0 on some starts and 0.35 on one loop.
10. **Mirror table** (`sub_14004DF10`, at `*a1+152`): each entry >> 3 is the mirror channel and
    & 7 the mode.
    - `ACU_BaseSkeleton_Male` bones carry a mirror bone pointer and a `mirroring_type`.
    - Of 109 bones, 11 are self-mirrored and 98 paired.
    - Types seen: {2: 1, 4: 2, 5: 106}.
    - The binding modes of ACU-RE-0027 apply to quaternions. Mirrored translation negates x
      (`sub_140018050`).
11. **Root motion in the arena** (`installed_graph_steering`, 60 Hz, open-ground parameters):
    - From idle, a stick right / back plays `PreNavWalkMid_Lt90` / `Lt180` (mirrored). It turns
      yaw by −88° / −176° through the clip root rotation alone, then walks along the stick
      (last-second travel · stick ≥ 0.998).
    - Releasing the stick plays WalkStop, then IdleLow, and the speed returns to 0.

12. **Game-computed steering parameters live** (`loco-params-1/params2.jsonl`, 1,051 samples):
    - `DiffAngleToFacing` (walk-loop sway axis) is 0 in every sample.
    - `CharacterTurnAngle` (sway axis through `AtomDampRTCPNode` hash `0x48ebbdd9`) is non-zero
      in only 36 samples.
    - `DiffAngleToTarget` is non-zero in 311 samples and sometimes equals TargetAngle.
    - Native holds small stick offsets without turning: TargetAngle 3.6° for more than 50
      samples at MovementSpeed 2.2, with yaw constant.

## Inferences

- The clip sync group is root `f92` and the loop flag root `f112` (obs. 5, 6, 9). The root
  compares `f92` with the record group, and the data pattern holds across every graph inspected.
  The instruction that writes args `+88`/`+98` was not found.
- `AtomWriteParamNode` passes its input pose through.
- Native turning while moving comes from the graph (turn starts, sway clips, axial blends by
  TargetAngle) and the root rotation. No procedural facing is involved (ACU-RE-0057 obs. 7).

## Implementation choices (labelled in code)

- Graph phase step = dt / record total (root `+84` = total / global speed, with speed 1), wrapped
  into [0, 1).
- SelectBestMatchingTags cost threshold: `f32::MAX`.
- Root motion of a blend = the weight-normalized blend of the leaves' clip root deltas.
- Pose blend: weighted local transforms with hemisphere-aligned quaternion sums.
- Controls → parameters:
  - MovementSpeed is linear between the measured stick points;
  - Parkour is 0 when idle and 1 when moving (21 beside walls is not modelled);
  - open-ground distances use the 10.0 cap;
  - TimeScale is 1 and UseBipedFootIK is true.
- The arena applies only the yaw part of the root rotation.

## Unknowns

- Damp-node law (`AtomDampRTCPNode`; passed through: 1,462 notes in the replay).
- Masked layers of layering states (not evaluated).
- The native lag between MovementSpeed reaching 0 and Parkour reaching 0: the stop enters 0.18 s
  early.
- Idle phase at capture start: IdleLow time error 7.7 s, an offset only.
- The walk start after a stop picks `Rt0` where native picks `Lt0`.
- Sprint phase error is 0.44 s.

## Independent comparison

- Live replay (`atom_graph_replay`): the dominant clip agrees on 649/1051 samples (it was 181
  before this work). Median dominant-clip time error 0.084 s:
  - Walk 0.088 s;
  - WalkSlow 0.063 s;
  - walk start 0.030 s.
- The root `f92`/`f112` change kept agreement at 649 against 650 and cut the Sprint error from
  0.536 s to 0.439 s.

## Reproduction

From `acu-rs/`:

```
ACU_NEWDEMO=$PWD/../local/atomgraph/NewDemo.bin ACU_GAME_DIR=<install> \
ACU_LIVE_RUN=$PWD/../local/native-probe/runs/loco-params-1 \
cargo test --locked -p acu_motion --test atom_graph_replay -- --ignored --nocapture
ACU_GAME_DIR=<install> cargo test --locked -p acu_bevy -- --ignored installed_graph_steering --nocapture
cargo run --locked -p acu_motion --example graph_state_fields -- <install> PreNavWalkMid_Rt90_tr Walk_3-Loop
```
