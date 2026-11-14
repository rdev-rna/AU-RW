# ACU-RE-0066 — Exact AtomGraph stream format; the navigation hub; transition selection

Date: 2026-11-15. Related: [ACU-RE-0065](ACU-RE-0065-atomgraph-reflection-and-locomotion-machines.md)
(reflection layouts; partly superseded here), [ACU-RE-0063](ACU-RE-0063-newdemo-window-selection.md)
(condition semantics), [ACU-RE-0064](ACU-RE-0064-window-graph-parameter-sources.md).

State: **observed** for the stream format: the parser follows the native loaders and consumes
three graphs exactly. **Observed in the binary** for transition selection, condition kinds and
parameter storage. The meanings of most member fields and of the code-raised events are
**unresolved**.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, base
  `0x140000000`.
- Sources:
  - pseudocode `Unity/ACU - Decompiled/<VA>.c` (analysis artifact);
  - IDA (idalib, ida-pro-mcp) on
    `Unity/local/native-probe/bone-trace-20261019/main-image.normalized.exe.i64`;
  - the mapped image `.../motion-link-20261020/runtime-view/capture/main-image.mapped.bin`
    (SHA-256 `9a1124f1…0520`), VA = base + offset.
- Assets (local only, never committed):
  - `DataPC.forge` entry 1519 `NewDemo` `000000013d5bc4d8`, 42,174,002 bytes, SHA-256
    `c8c35851…89ee`;
  - the `Dog` and `MachineGun` graphs (5,162 and 1,845 bytes) from ACU-RE-0063;
  - the walk clip `xx_Nav_Walk_3-Loop` `0000000fc08fd67a` (DataPC.forge entry 928, 23,563 bytes).
- Tool: `tools/atomgraph_stream.py`, tests `python -m unittest tools/test_atomgraph_stream.py`
  (5 tests, synthetic).
- Names: class names by CRC-32 (`crc-name`). Parameter names come from the graph's own
  `AtomRTCPDescriptor` strings (decoded).

## Observations

1. **Class to loader.** The engine creates an object from its class hash through
   `sub_14270B650` → `sub_1426EB3B0` (hash map `qword_14525BA10`, hash → class info) →
   `sub_1426EB1A0`:
   - it allocates `size` = class info +40;
   - it calls the constructor at class info +80;
   - it calls the loader through vtable slot 2 (`*(*obj + 16)`).

   The class info sits at descriptor − 0x20, so the constructor is at descriptor + 0x30. Small
   non-polymorphic classes use the loader at class info + 0x70 (embedded objects,
   `sub_14270BA90`). Reading the constructor at descriptor − 0x80, as the first map did, paired
   classes with their neighbours' loaders.
2. **Stream primitives** (pseudocode of each function):
   - arrays: u32 count (`sub_14000B2A0`);
   - embedded objects: u64 id, u32 class hash, then the static loader (`sub_14270BA90`);
   - pointers (`sub_14270BEA0`):
     - u8 tag 0: u64 id, u32 hash, then the hash's loader;
     - tag 2: a u64 reference;
     - tag 3: null;
   - static-loader pointers (`sub_14270B920`);
   - objects through their own vtable (`sub_14270BC80`);
   - handles (`sub_14270E560`): u8 tag, u8, u64, and for tag 0 an inline object;
   - strings (`sub_14270E8E0`): u32 n, then n + 1 bytes;
   - raw arrays (`sub_14270C020`).

   A resource payload starts with u8 1, u64 id, u32 class.
3. **Exact consumption.** With one transcription per loader, the parser reads NewDemo to its last
   byte: 42,174,002 of 42,174,002 bytes, 945,730 objects, 60 classes. It reads `Dog` and
   `MachineGun` exactly too. Any wrong field size or class mapping would misalign every later
   object.
4. **Graph root** (`AtomGraph`, loader `0x1400C5320`):
   - `f16`: the root `AtomStateMachineNode`;
   - `f24`: an `AtomGraphStateNode`;
   - `f32`: an `AtomNullStateNode`;
   - `f40`: 534 `AtomRTCPDescriptor`s, the parameters (name, type, …);
   - `f56`: an `AtomStateDescriptor` (218 properties);
   - `f96`: an `AtomCustomTransitionSystem` (795 cells);
   - `f116`: an `ExternalIdMap`.
5. **State structure.** Every state starts with `AtomStateNode` (loader `0x14010C100`):
   - `f16`: its outgoing `AtomStateTransitionTarget`s;
   - `f104`: its node ID.

   On top of that:
   - `AtomStateMachineNode` (`0x14010BF10`) adds:
     - `f128` child states;
     - `f140` `AtomInitialState` entry rules;
     - `f152` machine-level transitions.
   - `AtomGraphStateNode` (`0x14010B9B0`) adds `f128` graph nodes: the blend tree (clips,
     axial and angular blends, time scale, parameter read/write, switches).
   - `AtomLayeringStateNode` (`0x1400C5D70`) holds `AtomLayeringInfo` layers. Each layer's
     `f56` is a state machine.
6. **Entry rule** (`AtomInitialState`, `0x14010BC60`): a condition-expression pointer, then
   **u16** target state index, then an `AtomNodeID`. This corrects ACU-RE-0065, which read the
   target as one byte.
7. **Transition** (`AtomStateTransitionTarget`, `0x14010C4E0`). Fields in stream order:
   - condition expression;
   - u16 target (`f36`);
   - u8 (bit 11 of `f38`);
   - five u32 bitfields (`f38` bits 0–2, 5–6, 7–8, 9–10, 3–4);
   - **two u32 that both land in `f32`**: the first is overwritten by the second, a float of 0
     to 3 in NewDemo (constructor default 2.0);
   - three u8 flags (`f38` bits 15, 13, 12);
   - an `AtomNodeID`.

   Targets `0xFFFD` and `0xFFFE` take a separate path in `sub_1400FABF0`.
8. **Transition selection** (state-machine update `sub_1400FABF0`):
   - `sub_14010EAE0` builds the candidate list: the **current state's own transitions in order**,
     then the **machine's `f152` transitions**. A machine transition whose target is the current
     state is skipped, unless the current state is the graph's special state (instance +1648, +32).
   - `sub_1401260C0` walks the list and returns the **first** candidate whose expression passes
     (`sub_1400C2790`). It also applies a per-transition check against a time map (`a2[6]`) and
     skips an entry while that check fails.
   - If none passes, `sub_1400FBE20` updates the current state.
9. **Condition kinds** (`sub_1400C23D0`):
   - kind 0 compares a parameter: the value type selects `sub_14009EE70` (bool), `…E7C0` (float),
     and others;
   - kind 2 is a group (`sub_1400C3400`);
   - kind 3 is **state time** against the threshold `f16`;
   - kinds 1 and 4 are **events**: `sub_1400C7500` looks up event id `f0` in a sorted interval
     list (source + 2064, 32-byte entries, id at +8, start +20, end +24). The condition holds if
     an interval overlaps the frame's [t0, t1]. Kind 4 uses another source (instance + 2120).
10. **Float comparison** (`sub_14009E7C0`):
    - `==` is |a − b| ≤ 1e-5 and `!=` its negation; `>`, `>=`, `<`, `<=` are strict as written;
    - with bit 11 (`0x800`) set, the right side is a second parameter (index `f16`), not the
      constant.
11. **Parameter storage** (`sub_14009EFA0`). For the own entity (`f10 == 0xFFFF`):

    value(i) = u32 at `*(instance + 1056) + *(u32 *)(*(def + 208) + 4·i)`, with
    def = `*(instance + 1656)`. `sub_14010A2F0(def, i, instance + 1024)` runs first.
12. **The navigation hub** is `AtomStateMachineNode` at payload offset 9,974,731. Path from the
    root: root 14 → … → layer 6,784,526 → … → layering state 9,972,547 → layer 9,974,572 →
    machine 9,974,654 → hub.

    | State | Payload offset | Content (clips) |
    |---|---|---|
    | 0 | 9,974,808 | wall brace |
    | 1 | 10,033,410 | low-profile walk (`HighProfile == False`) |
    | 2 | 10,149,624 | high-profile run and sprint |
    | 3 | 10,261,138 | beam, free aim |
    | 4 | 11,162,713 | idle, stops, pre-nav sprint turns |
    | 5 | 11,388,579 | hide spot, haystack |
    | 6 | 11,437,044 | mantle leap |
    | 7 | 11,440,927 | sneak |
    | 8 | 11,619,752 | sprint start (layering) |
    | 9 | 11,768,074 | get hit |

    The hub has 12 entry rules and 7 machine transitions. Example: walk → run fires on
    `HighProfile == True and Parkour == 1 and TargetAngle in [-110, 110) and event(0x0040000b)`,
    with `f32` = 0.5.
13. **Walk machine** (10,033,410):
    - states:
      - 0: strafe;
      - 1: forward (10,052,097);
      - 2: stumble;
      - 3: sprint-to-walk;
      - 4: turn on spot;
      - 5: idle;
    - the forward machine splits on `Crowd_Interaction` and `InDenseCrowd`, down to the walk loop
      of ACU-RE-0065.
14. **Clip events.** The walk clip ends with event tracks:
    - alternating keys `0x00400030` / `0x00400031` (six keys);
    - `0x0040002f`, `0x80000004` and `0x00400001` (byte offsets 23,244–23,483).

    The events the hub tests most (`0x0040000b`, `0x00400034`, `0x00400005`) do not occur in
    that clip.

## Inferences

- `f32` of a transition is its blend duration in seconds. The values (0, 0.1–3.0) and their use on
  every transition fit; the runtime read is not traced yet.
- `0x00400030` / `0x00400031` are left/right foot events (they alternate, six per loop). Not
  verified.
- `0x0040000b`, `0x00400034` and `0x00400005` are raised by gameplay code rather than clips. A
  corpus search for the immediates is pending.

## Unknowns

- Which code raises each event id, and what each means.
- How the controller computes `Parkour`, `HighProfile`, `TargetAngle` and the other parameters.
- The runtime meaning of the transition bitfields, of the `a2[6]` time map, of targets
  0xFFFD/0xFFFE and of the graph-node fields (blend evaluators).

## Reproduction

```
python tools/atomgraph_stream.py local/atomgraph/NewDemo.bin --stats --out local/atomgraph/newdemo-stream.json
python -m unittest tools/test_atomgraph_stream.py
```

## Implementation implications

- A Rust port can load NewDemo with this grammar and evaluate the state machines by observations
  8–11.
- Matching native walking and running also needs the parameter and event inputs, from the
  controller code or from live reads of the parameter array (observation 11) during scripted
  walks.
