# ACU-RE-0065 — AtomGraph layout from reflection; first locomotion machines

Date: 2026-11-15. Related: [ACU-RE-0063](ACU-RE-0063-newdemo-window-selection.md) (machine
decoder), [ACU-RE-0064](ACU-RE-0064-window-graph-parameter-sources.md),
[ACU-RE-0057](ACU-RE-0057-body-velocity-from-root-motion.md) /
[ACU-RE-0058](ACU-RE-0058-root-delta-extraction.md) (root motion from the graph).

State: partially established. The serialized layout of entry rules is now taken from the
engine's reflection tables, not inferred from bytes. The walk-loop states are decoded. The
locomotion root (idle / walk / run / sprint / start / stop / turn selection) is not found yet:
those clips sit in separate machine trees linked by structures the decoder does not model.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, base
  `0x140000000`.
- Reflection tables read in the IDA database
  `E:/ReverseEngineering/Projects/Unity/local/native-probe/bone-trace-20261019/main-image.normalized.exe.i64`
  (idalib headless, ida-pro-mcp) and in the mapped image
  `.../motion-link-20261020/runtime-view/capture/main-image.mapped.bin`
  (SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`, VA = base + offset).
- Pseudocode: `ACU - Decompiled/1400C3400.c` (analysis artifact).
- Asset: `DataPC.forge` entry 1519 ordinal 1, `NewDemo` `000000013d5bc4d8`, payload
  42,174,002 bytes, SHA-256 `c8c358512edc2dc64c5c64cb9702f4bc9c478a5b50693a8d46c2f33ddd0589ee`
  (local only).
- Names: class and type names by CRC-32 (`crc-name`); field names from
  `local/atomgraph/crc-names.json` where a string exists, else shown as hashes.
- Tool: `tools/atomgraph.py` (changed: `expression_block`, `_initial`); tests
  `python -m unittest tools/test_atomgraph.py` (15, synthetic).

## Observations

1. **Reflection class descriptors.** In `.rdata`, a class descriptor holds `u32 1, u32 class
   CRC-32, u32 size, u32 alignment`. The pointer at descriptor − 0x20 is its field table, made of
   0x38-byte entries: byte 3 = 2, u32 field-name CRC at +4, u32 type CRC at +8 (0 for scalars),
   then a u16 type code at +14 (repeated at +16).
   - `AtomInitialState` (descriptor `0x1439DD750`, size 0x20, fields at `0x1439DD1B0`):
     1. `AtomConditionExpression` (code 0x15, embedded object);
     2. a scalar of code 5;
     3. `AtomNodeID` (code 0x16).
   - `AtomConditionExpression` (`0x1439D85D0`, size 0x28, fields at `0x1439D5ED0`):
     1. a code-9 scalar;
     2. `Conditions` (name resolved), an array (code 0x1D) of `AtomCondition`;
     3. an array of code-7 scalars (code 0x039D).
   - `AtomCondition` (`0x1439D8520`, size 0x30): 10 fields, the last an
     `AtomDataContainerWrapper`. `AtomDataContainerWrapper` (`0x1439D8850`): 5 fields.
2. **Entry rule bytes match that field order** in every rule checked:
   - expression: u32, u32 (array metadata: `0x5000EA8B` when filled, 0 in some empty arrays),
     u32 condition count, the conditions, then u32 n and n parameter indices;
   - then **one byte, the target state** (the code-5 field);
   - then the `AtomNodeID` object.
   The old decoder read a u32 at the target position. Its upper three bytes were the start of
   the next object, so the window machines decoded correctly; they still decode identically
   (37,005,472, 13,829,873, 13,803,765 compared against the HEAD tool).
3. **An empty condition list is true.** `0x1400C3400` returns 0 immediately when its count
   argument is 0 (`if (!a2) { …; return 0; }`), and a return of 0 means the condition holds
   (ACU-RE-0063). Rules with no conditions occur (first at payload offset 10,077,867) and act
   as defaults.
4. **The condition count bounds the list.** `AtomStateTransitionTarget` objects, each with its
   own condition expression, can follow an entry rule directly (e.g. offset 2,619,463). The old
   decoder collected every condition up to the next state and failed on these. Bounding by the
   declared count fixes 1,260 of 1,352 machine failures.
5. **The varying "first header byte" is the tail of the previous object.** Each condition
   wrapper ends with f32 1.0, and its last byte (0x3F) is the byte the decoder had read as the
   next header's flag. The entry-rule target byte is the same case. Headers are therefore
   `u64 id, u32 class`; the decoder keeps its offsets (one byte earlier) for compatibility.
6. **Walk loop (machine 10,052,657, under 10,052,096).** State 0 is an AxialBlend on
   `DiffAngleToFacing`: −90 → `xx_Nav_WalkFwd_3-Loop_SwayLt90`, 0 → `xx_Nav_Walk_3-Loop`,
   +90 → `xx_Nav_WalkFwd_3-Loop_SwayRt90`, under a time-scale node of 0.8. The crowd state
   (`Crowd_Interaction == 1 or InDenseCrowd == True`) blends the same clips, and the
   `xx_DenseCrowd_*` avoid clips, on `CrowdPushIntensity`.
7. **Index of every machine.** NewDemo holds 5,320 `AtomStateMachineNode`s. With the fixes,
   92 do not parse (graph nodes between states: `AtomGraphNode` 51, `AtomMultiplyDivideNode` 30,
   other 11). They form 1,258 separate top-level trees. No tree holds more than two of
   `xx_Nav_Walk_3-Loop`, `xx_Nav_Run_3-Loop`, `xx_Nav_Sprint_3-Loop_V02`, `xx_Nav_IdleLow_Footl`
   and `xx_Nav_SprintStop_Fd100cm`.
8. **Locomotion-relevant parameters** in NewDemo (index: name): 193 `DiffAngleToFacing`,
   263 `HighProfile`, 344 `LeftStickAngle`, 347 `LeftStickLength`, 357 `MotionSpeed`,
   358 `MovementSpeed`, 359 `MovementType`, 378 `PlayerSpeed`, 437 `Speed`, 438 `SpeedFactor`,
   483 `TargetSpeed`, 85 `CharacterTurnAngle`, 498 `Turn180`, 499 `TurnOnSpotType`,
   199 `DistToStopDst`, 432 `SlopeAngle`, 32 `BankingAngle`.

## Inferences

- Locomotion is assembled from several machine trees joined by references the decoder does not
  follow (sub-graph or state references, or the graph nodes in observation 7). Not verified.
- Type code 5 is a 1-byte scalar in serialized form (it occupies one byte in every rule).

## Unknowns

- The structure that joins the locomotion trees, and the root that chooses idle, walk, run,
  sprint, starts, stops and turns.
- `AtomStateTransitionTarget` fields (blend time, conditions, target), the AxialBlend weight
  rule, time-scale semantics, and clip phase synchronisation.
- How the controller computes the parameters in observation 8.
- The full type-code table of the reflection system.

## Reproduction

```
python -I tools/atomgraph.py machine local/atomgraph/NewDemo.bin --at 10052657 --out local/atomgraph/machine-10052657.json
python -m unittest tools/test_atomgraph.py
```

Reflection: in IDA, search bytes `01 00 00 00 30 a6 ed 65` (AtomInitialState) and read the
0x38-byte entries at the pointer 0x20 bytes before the match.

## Development handoff

- Next: a reflection-driven deserializer. Read each class's field table from the image and
  parse NewDemo generically. That removes byte-pattern inference, decodes transitions and
  blend nodes, and exposes the references between machine trees.
- Then: the AxialBlend, transition and time-scale evaluators from the binary; parameter sources
  (static analysis, then live reads as in ACU-RE-0064); a Bevy-free evaluator replacing the
  arena's motion matcher.
