# ACU-RE-0074 — Transition entry: null sync source, latched graph conditions, damp nodes

Date: 2026-11-15. Related: [ACU-RE-0072](ACU-RE-0072-flat-ground-locomotion-parity.md) (locomotion
parity, differences 1 and 2), [ACU-RE-0069](ACU-RE-0069-atomgraph-evaluation-sync-and-mirroring.md)
(SelectBestMatchingTags, sync records). Report: [LOCOMOTION_PARITY.md](../LOCOMOTION_PARITY.md).

State: **partially established**.
- **Observed** in pseudocode: the transition path, the condition latch and the select activation
  below.
- **Inference:** the damp law and its constant.
- **Unresolved:** how native takes the half-stick stop branch.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, image base
  `0x140000000`.
- Corpus files: `ACU - Decompiled/<VA>.c`. The export-to-binary correspondence is unverified.
  Files read:
  - `140109CB0.c` lines 150–232: state-machine transition;
  - `1400B7970.c`: new-state activation context;
  - `1400D6D00.c`: `AtomGraphConditionNode` vtable slot 8;
  - `1400EB6B0.c`: slot 19;
  - `1400D7640.c`: SelectBestMatchingTags activation;
  - `1400E7C00.c`: graph-state activation, first 150 lines;
  - `1400CAD80.c` and `1400C8750.c`: clip event list.
- Vtable slots from the local symbol index (`tools/symbol_query.py methods
  AtomGraphConditionNode`; vtable `0x142E802B0`, slot 8 = `0x1400D6D00`, slot 19 = `0x1400EB6B0`;
  `reflection-vtable`, high confidence).
- NewDemo AtomGraph: `DataPC.forge` entry 1518, file id `000000013d5bc4d8` (ACU-RE-0069). Graph
  states:
  - 3723: walk stop, under machine 3678 = stop machine 3677 state 0;
  - 3745: stop machine state 5;
  - 3336: walk graph.
- Live captures (local, ignored): `loco-stops-2/live-state.jsonl` (SHA-256 prefix `d98eb08e`);
  `loco-params-1/live-state2.jsonl` (`17f8dafc`) for the damp fit.

## Observations

1. **A transition starts the new state without a sync source.**
   - After the rule picks a transition, `sub_140109CB0` calls `sub_1400B7970(v47, target)`.
   - That builds the activation context `v9` with qword `+16` = 0 and passes it to
     `sub_1400B5130` for the target state.
   - Graph-state activation (`sub_1400E7C00`) keeps a context source only when `*(source+20)` is
     nonzero. Otherwise the source is nulled and the threshold set to −FLT_MAX.
2. **SelectBestMatchingTags without a source keeps port 2.**
   - `sub_1400D7640` writes 2 to the node's index byte.
   - Only with a source does it:
     - evaluate the source's record (source `+8`, vtable `+32`);
     - compare the record's group (`+144`) with port 0's bits;
     - read the position (vtable `+64`; cur 255 = none);
     - run the cost search.
   - The latch byte (`+92`) is set when that fails or is skipped. The threshold is ctx `+32`;
     a negative one means FLT_MAX.
3. **`AtomGraphConditionNode` is latched at activation.**
   - Activation `sub_1400D6D00` clears the node's byte (instance offset `f88`).
   - When ctx dword `+36` ≠ 2 and the context source `a2[2]` is non-null, it does two things:
     - collects that source's events (source `+8`, vtable `+24`, gathering context with 0.01
       and 1.0);
     - evaluates the expression once over them.
   - The update `sub_1400EB6B0` copies the byte. It re-evaluates (window
     `[clock − 1e-5, clock]`, instance list `+2120`) only when ctx dword `+92` == 2.
4. **The walk stop's slow branch depends on that latch.**
   - 3745 is `If(cond 0x00400026) → WalkSlow_3-Loop else Walk_3-Loop`.
   - 3723 is `If(cond 0x00400026) → Select(WalkSlow_Stop_LC, LC(m), LP, LP(m)) else
     Select(WalkStop_RC, RP, RC(m), RP(m))`.
   - Both conditions are `[k1 0x00400026 op0 mode0]`. 3723 is reached by 3745 → state 0
     (machine 3678) → entry rule.
   - `WalkSlow_3-Loop` carries whole-clip `0x00400026`. `WalkSlow_Stop_LC` and `WalkStop_RC` do
     not (`ACU_CLIP_EVENTS` dump).
5. **The Rust per-frame condition flipped mid-stop (now removed).**
   - Evaluated every frame against the previous step's events, the condition played
     `WalkSlow_Stop_LC` for 0.1 s.
   - Then the stop clip's own events had no `0x00400026`, so it switched to `WalkStop_RC` with
     0.01 m of travel.
   - It also took the slow branch after full-stick walks. At MovementSpeed 2.2 the walk graph's
     axial node (thresholds 1 and 2.25) keeps about 4 % of the `WalkSlow` branch, and every leaf
     emits its events.
6. **Damp fit (live).**
   - Method:
     - take the `Walk_3-Loop` : `WalkSlow` blend weights in `live-state2`;
     - map them through the walk graph's MovementSpeed thresholds (1, 2.25) to the damped value;
     - track it after a stick change.
   - The error to the target falls by about one half per 30 Hz sample. That gives
     k ≈ 20.6 /s for the rate-5 damp node.

7. **Sprint graph and blend agree with native.**
   - Both sprint in graph 3381: hub run machine 3358 state 2 = 3370, then 3379, then 3381.
     The start machine 3359 leaves to state 2 after 0.05 s.
   - 3381 blends a run side and a sprint side with an axial node on damped MovementSpeed:
     - the damp node has hash 4170351878 = `MovementSpeed` and rate 10;
     - thresholds are 3.5 and 7.
   - Native while `Sprint_3-Loop_V02` dominates (`loco-stops-2`, 59 samples):
     - sprint 0.68, slope sprint 0.08, avoid 0.02;
     - `Run_3-Loop` 0.16, run slopes 0.02.
     Rust: sprint 0.79, run 0.21.
   - Sync blend rates:
     - native sprint clip 0.88–0.90×, run clip 1.24–1.28× (clip time against QPC);
     - Rust sprint 0.88–0.95×, run 1.19–1.47× (`ACU_LEAF_TRACE`).
   - Native parameters during sprints (`loco-params-1/params2.jsonl`):
     - TimeScale 1.0 and SpeedFactor 1.0;
     - `WallDistanceY` median 4.55 m, min 0.57 m;
     - `VirtualFootIkSlope` −5.1° to +8.3°;
     - `TargetDistanceX` median 0.27.
   - Speeds: native 6.05 m/s over dominant runs, Rust 6.39 m/s.
   - Correction: damp hash 1736508977 is `WallDistanceY`, not `MovementSpeed`.
8. **State 8947 is shaped like an overlay layer machine.**
   - The masked layers of the root layering state (state 1: 6739, 6764, …) have the same form:
     - state 0 is a non-graph placeholder (`Body::Other`);
     - later states play overlay clips.
   - In 8947, 8948 moves to state 1 (8949) at once. 8949 plays `IdleHigh_tr_IdleLow_*` and
     exits to target 65534.
   - Not checked:
     - no layering state the interpreter parses lists 8947;
     - all 6,494 transitions carry only `f16` (node id), `f32`, `f36` and `f38` bits, so none
       references a graph.

## Inferences

- With no source at transition entry, every SelectBestMatchingTags in a newly entered state
  plays its port 2. For the walk stop that is `WalkStop_RC` unmirrored. Native picked it in 13 of
  13 stops spread over the walk cycle.
- Inferred, not traced: entry rules below the new state inherit the null-source context.
- Damp: `x += (target − x)·(1 − e^(−k·dt))`. CHOICE: k = 4.12 × rate. Only rate 5 is fitted;
  the native updater of `f96` is not located.
- Activation starts from the last value of a damp node with the same parameter hash, per
  `sub_1400D74D0` (ACU-RE-0069 notes). CHOICE: Rust keeps one value per hash for the whole player.

## Unknowns

- **Half-stick stop.**
  - Native played `WalkSlow_Stop_LC` twice and `_LP` once after half-stick walks. So native
    hands 3723 a source whose events include `0x00400026`, and once picked by phase (LP).
  - Candidates:
    - the machine's vtable `+80` call in `sub_140109CB0` (a copy of the caller's context with
      `HIBYTE(+0x60) |= 1`);
    - another caller of `sub_1400B5130` (`0x1400D3120`, `0x1400D53D0`, `0x1400D7F90`,
      `0x140116AC0`, `0x14011C0D0`).
  - If such a source exists, the full-walk stop must still not pick by phase, and its events
    must lack the 4 % `WalkSlow` leaf. Neither is explained.
- Sprint speed: the native runs were near walls and on uneven ground, which Rust's arena
  doesn't model. An open, level native sprint with parameters captured would show whether
  any gap is left.
- Who activates state 8947 (game code request, or a layering link the parser misses).
- What sets `*(source+20)`.
- Context pass mode 2 (`+92`).
- The 0.01 / 1.0 values of the event-gathering context.
- k for other rates.

## Independent comparison

- `installed_walk_stop_sweep` (new, ignored, needs the install):
  - releases after holds of 1.80–2.95 s in 0.05 s steps cover more than one walk cycle (0.93 s);
  - it asserts `WalkStop_RC` for every full-stick stop: 24 of 24 (native 13 of 13);
  - before this change Rust chose RP(m) / RC(m) / RP / RC with phase (ACU-RE-0072 obs. 3).
- `installed_locomotion_segments`: "walk stop clip" is no longer a known difference, so it is
  asserted. The other four remain.
- Replay agreement (`atom_graph_replay`, `loco-params-1`): see LOCOMOTION_PARITY.md.

## Reproduction

From `acu-rs/`, with `ACU_GAME_DIR` set:

```bash
cargo test --locked -p acu_bevy --bins installed_walk_stop_sweep -- --ignored --nocapture
```

```bash
cargo test --locked -p acu_bevy --bins installed_locomotion_segments -- --ignored --nocapture
```

`ACU_NODES=3723,3745` and `ACU_CLIP_EVENTS=WalkSlow_Stop_LC,WalkStop_RC` with
`installed_graph_dump` print the graphs and clip events used above.

## Implementation implications

- `acu_motion::atom_graph`:
  - `Eval::select_source` is `None` for every graph state. Selects latch port 2; the handed-over
    record still sets the entry phase.
  - `AtomGraphConditionNode` returns its activation latch: false without a source.
  - Graph-pass nodes see the previous step's events and window.
  - `DAMP_K_PER_RATE` = 20.6 / 5; the damp value is carried by parameter hash.
