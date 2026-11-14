# ACU-RE-0068 — Clip event intervals (what `event(id)` conditions test)

Date: 2026-11-15. Related: [ACU-RE-0066](ACU-RE-0066-atomgraph-stream-and-navigation-hub.md)
(event conditions), [ACU-RE-0018](ACU-RE-0018-animation-tracks.md) (track encoding),
[ACU-RE-0067](ACU-RE-0067-live-locomotion-parameters-and-blend-rules.md) (live setup).

State: **observed live** (runtime interval lists read from memory for 15 clips) and **observed in
the binary** (`sub_1400CAD80`, the copy into the graph's list). How the list is built at load time
is not traced.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`; pseudocode
  `ACU - Decompiled/1400CAD80.c`, `1400C7500.c`, `1400CDB20.c`.
- Live run `local/native-probe/runs/loco-events-1` (local only):
  - PID 116708, launched with `capture_run.py --until world --keep-game --save-pin
    capture-start-1 --fps-cap 60`;
  - closed with `--close` (graceful, 2.3 s); saves put back;
  - same moves as ACU-RE-0067.
- Tool: `tools/native_probe/clip_events.py` (ReadProcessMemory only).
- Clip payloads for the track comparison:
  - `local/anim-events/{walk,idle,run,stop}.bin`, from DataPC.forge entry 928;
  - walk `0000000fc08fd67a` (23,563 bytes, SHA-256 `b55338f0…2cfb`, ACU-RE-0018).

## Observations

1. **Where the intervals live.** `sub_1400CAD80` (the clip node's step) reads a list at
   `*(*(S + 13896) + 2096·slot + 2064)`, where S is the leaf row's state and slot the playback
   slot (ACU-RE-0025). The list has 32-byte entries:

   | Offset | Field |
   |---|---|
   | +8 | u32 id |
   | +20 | f32 start (clip seconds) |
   | +24 | f32 end (clip seconds) |
   | +28 | u8 flag |

   Every entry that overlaps the step's clip window [a3·d, (a3 + a4)·d] is copied into the
   graph's list (`*(ctx + 16) + 2064`), with times mapped to graph time. The graph's sort flag
   (+2072) is then set.
2. **Live lists**:

   | Clip | Intervals |
   |---|---|
   | `xx_Nav_Walk_3-Loop` (2.8 s) | `0x00400030` [0, 0.2], [0.7, 1.1667], [1.6333, 2.1], [2.5333, 2.8]; `0x00400031` [0.2, 0.7], [1.1667, 1.6333], [2.1, 2.5333]; `0x0040002f`, `0x00800004`, `0x00400001` [0, 2.8]; `0x80000002` [2.8, 2.8] |
   | `xx_Nav_WalkStop_RC` (3.267 s) | `0x00400030` [0, 0.2]; `0x0040002f` [0, 0.2]; `0x00400031`, `0x0040000b` [0.2, 3.267]; `0x00400001` [0.1667, 3.267]; `0x00400005` [2.1667, 3.267]; `0x80000002` at the end |
   | `xx_Nav_IdleLow_Footl` (19.233 s) | `0x00400030`, `0x0040000b`, `0x00400001` over the whole clip |
   | `xx_Nav_WalkSlow_3-Loop` (4.0 s) | `0x00400026`, `0x00800004`, `0x00400001`, `0x0040002f` over the whole clip |
   | Run loops | as above, with `0x0040002d` |
   | Sprint loops | as above, with `0x00400028` |

   In the other entry fields, +12, +16 and the flag were 0, and the pointer +0 was null, in every
   entry.
3. **Rule: integer event tracks are step tracks.** The walk clip's codec-19 track has keys at
   frames 0, 12, 42, 70, 98, 126, 152 (byte times / 60, implicit first key 0), with values
   alternating `0x00400030` / `0x00400031`. The live intervals are exactly
   [t_k, t_{k+1}] for value k, and the last key runs to the clip duration. The single-key tracks
   (`0x0040002f`, `0x00800004`, `0x00400001`) are single keys at 0 and cover [0, duration].
   - The track bytes `04 00 80 00` are `0x00800004`; ACU-RE-0066 misread them as `0x80000004`.
4. **End marker.** Every clip list ends with `0x80000002` at [duration, duration]. That id is not
   in the clip's tracks, so it is added at runtime.
5. **What the condition tests** (`sub_1400C7500`, ACU-RE-0066 obs. 9). An `event(id)` condition
   holds when a graph-list interval for `id` overlaps the frame's [t0, t1], with tolerance 1e-5.
   So:
   - `event(0x0040000b)` on the hub's walk → run transition holds while the playing clip is in
     a `0x0040000b` window: the idle clips and the stop clip after 0.2 s;
   - `event(0x00400005)` marks a stop's exit window (2.1667 s to the end).

## Inferences

- `0x00400030` / `0x00400031` are left- and right-foot support phases. They alternate and switch
  near foot contacts; contact frames are not measured.
- Run, sprint and walk-slow loops carry one gait id each (`0x0040002d`, `0x00400028`,
  `0x00400026`) over the whole cycle, which lets rules test the current gait.

## Unknowns

- The load-time builder of the per-clip list (the 2096-byte record), and whether any track type
  produces non-step intervals.
- `0x80000002`'s use by rules; mode 2 of `sub_1400C7500` (entering-edge variant).

## Reproduction

```
python tools/native_probe/clip_events.py <pid> <run>/events.jsonl --hz 30 --stop-file <run>/stop
```

## Implementation implications

Event intervals can be built offline from each clip's codec-18/19 tracks by the step rule plus the
end marker. The graph evaluator then answers `event(id)` from the playing clips' current windows,
with no runtime capture needed.
