# ACU-RE-0067 — Live locomotion graph parameters; axial blend and phase-sync rules

Date: 2026-11-15. Related: [ACU-RE-0066](ACU-RE-0066-atomgraph-stream-and-navigation-hub.md)
(graph structure), [ACU-RE-0025](ACU-RE-0025-player-root-full-rig.md) (controller and leaf rows),
[ACU-RE-0044](ACU-RE-0044-player-state-and-repeatable-start.md) (player chain).

State: **observed live** for the parameter values during scripted walking, running, free-run
sprinting, stops and turns. **Observed in the binary** for the axial-blend weight rule and the
two-child sync blend. Several runtime details are **unresolved** (listed below).

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, base
  `0x140000000`. Pseudocode `Unity/ACU - Decompiled/<VA>.c` and IDA (idalib) on
  `.../bone-trace-20261019/main-image.normalized.exe.i64`.
- Live run `local/native-probe/runs/loco-params-1` (local only):
  - launched with `capture_run.py --until world --keep-game --save-pin capture-start-1
    --fps-cap 60`, PID 126104, executable hash checked;
  - closed with `capture_run.py --close` (graceful, 2.1 s); saves put back with `save_pin.py putback`.
- Tools:
  - `tools/native_probe/graph_params.py` reads parameter values (ReadProcessMemory only, nothing
    written);
  - `live_state.py` reads player transform and playing clips;
  - `controller_autoplay.py --background --in-process-pad` drives the input (moves file
    `moves.json` in the run directory).
- Parameter list: NewDemo `AtomRTCPDescriptor`s from `tools/atomgraph_stream.py`
  (`local/atomgraph/params-newdemo.json`). Fields:
  - `f0` name;
  - `f8` type name;
  - `f20` value offset;
  - `f32` the default value wrapper.

## Observations

1. **Parameter store** (`sub_14009EFA0`, `sub_14010A2F0`):
   - value(i) is read at `*(I + 1056) + offset` for the graph instance I, with def =
     `*(I + 1656)`;
   - before each read, `sub_14010A2F0` runs the parameter's provider: a byte table at store +284
     maps the parameter to a 40-byte record at store +260; its callback (+24) runs once per frame
     (done-flag +34).
   - So values are computed lazily, only for parameters something queries.
2. **The player's animation controller is the graph instance.** Chain: player entity →
   animation component (vtable `0x142E7F780`) → controller (+0xD0). The controller's +1656 is a
   graph def, and its +1056 is a value block of 2,314 bytes.
3. **The value block follows the descriptors' `f20` offsets** (bools one byte, unaligned):
   - free run toggles `HighProfile` (byte 1221) 0 → 1;
   - `MovementSpeed` (1656) reads 0.621 / 1.411 / 2.2 / 6.277;
   - `TimeScale` (2196) reads 0.7–1.3.

   The 534-entry table at `[def + 208]` is **not** this layout: it puts `BankingAngle`, a float,
   at byte 109, and `AllowedToTurn` inside a vector3. What that table indexes is unresolved.
4. **Native locomotion, by input** (camera-relative stick, flat street near (530, 10, −0.3) in
   the pinned save; 30 Hz samples):

   | Input | MovementSpeed | HighProfile | Parkour | Main clips | Ground speed |
   |---|---|---|---|---|---|
   | none | 0 | False | 0 | `xx_Nav_IdleLow_Footr` / `_Footl` | 0 |
   | stick 0.25 | 0 | False | 0 | idle (dead zone) | 0 |
   | stick 0.5 | 0.621 | False | 1 | `xx_Nav_PreNavWalkMid_Lt180_tr_Walk` (start), then `xx_Nav_WalkSlow_3-Loop` | ≈1.1 m/s |
   | stick 0.75 | 1.411 | False | 1 | `xx_Nav_WalkSlow_3-Loop`, `xx_Nav_Walk_3-Loop` | ≈1.1 m/s |
   | stick 1.0 | 2.2 | False | 1 | `xx_Nav_Walk_3-Loop` | ≈1.4 m/s |
   | release after walk | 0 | False | 0 | `xx_Nav_WalkStop_RC` | — |
   | stick 1.0 + free run | 6.277 (peaks of 20.2) | True | 1 / 21 | `xx_Nav_Sprint_3-Loop_V02` (wall-brace clips at a wall) | — |
   | stick reversed while walking | 2.2 | False | 1 | `xx_Nav_WalkFwd_PlantAndTurnLt180_Improper` | — |

   - `TargetAngle` is the stick direction relative to the character, in degrees: 180 at a start
     from behind, 90 for a right stick.
   - `Parkour` also takes 11 and 21; their meaning is open.
   - `LeftStickLength`, `LeftStickAngle` and `DiffAngleToFacing` stayed 0 in this run, so no rule
     active there read them (observation 1).
5. **Axial blend weights** (`sub_1400BDF30`, from `0x1400BDE40`, vtable slot 20 of
   `AtomAxialBlendNode`). With n children, a node has 3 + 2n input ports. Thresholds t[k] and
   children alternate (runtime ports 64 bytes apart). For the axis value x, the loop runs over
   k = 1 … n−1 and stops at the first k with x < t[k]:
   - the weight is w = clamp((x − t[k−1]) / (t[k] − t[k−1]), 0, 1), applied to child k against
     child k−1;
   - if |w| ≤ 1e-5, only child k−1 plays (the weight is set to FLT_MAX, the "no blend" sentinel
     at `0x1439D71C8`);
   - if no threshold exceeds x, the last child plays alone;
   - with one child, that child plays.
6. **Two-child blend** (`sub_1400CB600` → `sub_1400FD7A0`):
   - the outputs carry a float at +140, which is blended linearly: (1 − w)·a + w·b;
   - they also carry a sync-marker list ((id, value) pairs, +0, count +10) and a sync id (+144).
   - When both children have the same sync id and flag, the marker lists are merged pairwise, and
     each value is blended by the same w. This is marker-based phase synchronisation of blended
     cycles.
   - If the ids differ, sync is dropped (+144 = −2).
7. **Clip node with markers** (`sub_1400C8750`): the node's phase advance uses the sync record
   (+140 duration) and splits a step that crosses the cycle end into two updates
   (`sub_1400CAD80`).

## Inferences

- The +140 float is the cycle duration of the blended child, and markers are foot events
  (`0x00400030`/`31`, ACU-RE-0066 obs. 14). Not verified.
- `MovementSpeed` is a quantized target speed from stick magnitude: 0.5 → 0.621, 0.75 → 1.411,
  1.0 → 2.2, free run → 6.277. The thresholds between the sampled magnitudes are not measured.

## Unknowns

- The input-to-`MovementSpeed` mapping between the sampled magnitudes; dead-zone size.
- `TimeScale`'s source (it varies 0.7–1.3 within one walk).
- `Parkour` 11 and 21; the event intervals from clip tracks (interval length, mode 2).
- The table at `[def+208]`.

## Reproduction

```
python tools/native_probe/capture_run.py local/native-probe/runs/<run> --until world --keep-game --save-pin capture-start-1 --fps-cap 60
python tools/native_probe/graph_params.py <pid> <run>/params.jsonl --names local/atomgraph/params-newdemo.json --hz 30 --stop-file <run>/stop
python tools/native_probe/controller_autoplay.py <pid> <run>/moves.json <run>/scenario --expected-sha256 748abdc6…7e40 --background --in-process-pad
python tools/native_probe/capture_run.py --close <pid>
```

## Implementation implications

- The arena's locomotion can be driven by the native graph, with `MovementSpeed`,
  `HighProfile`, `Parkour` and `TargetAngle` produced from the stick by the measured mapping.
- The measured clip sequence per input is the acceptance oracle: start clip, loops, stop clip,
  plant-and-turn.
