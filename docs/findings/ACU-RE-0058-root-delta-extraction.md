# ACU-RE-0058 — How the animation graph extracts the root delta from clips

Date: 2026-11-14. Related: [ACU-RE-0057](ACU-RE-0057-body-velocity-from-root-motion.md) (the root
delta drives the physics body), [ACU-RE-0018](ACU-RE-0018-animation-tracks.md) (tracks 0/1 =
root translation/orientation), [ACU-RE-0025](ACU-RE-0025-player-root-full-rig.md) (graph rows,
playback leaves), [ADR-0005](../decisions/0005-root-motion-ownership.md).
State: **static chain (corpus and disassembly) plus a live per-frame match.** For a single clip at
weight 1, the native root delta equals the difference of the decoded root track to within
0.003–0.018 mm. In one observed clip-to-clip transition, the delta comes from the incoming clip
alone. Rotating roots and other blend node types are not tested.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, base
  `0x140000000`.
  - Mapped image (Unity project)
    `local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`, SHA-256
    `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520` (capstone; constants
    read from it).
  - Corpus files: `1400EF1B0.c`, `1400FA080.c`, `1400F8DE0.c`, `1400B2F20.c`, `1400B0690.c`,
    `1400B30D0.c`, `140018F60.c`, `140018F90.c`, `140019200.c`, `140019CA0.c`, `14001ABA0.c`,
    `1400125C0.c`, `14003E870.c`, `140099980.c`. Export identity is unverified (ACU-RE-0031).
  - The anim-module corpus range `0x1400A0000`–`0x140130000` (2,321 files) was searched for uses of
    the context's output pointer `(a2 + 16)`.
- Assets: `DataPC.forge` entry 928 (file ID `0000002633b2c79d`), stored SHA-256
  `08b0a18bd2fa603ae2b77f2b17587c4f41c7154050552b5874edcd8c2bceda00`, decoded package SHA-256
  `e33c8441ffc8b8189391b4078e212716e90bbcccc196230070f36328c1f927fd`. Names below are
  `crc-name` labels from the asset index.

  | File ID | Resource | Name | Payload SHA-256 |
  | --- | --- | --- | --- |
  | `0000000fc08fd67a` | 4190 | `xx_Nav_Walk_3-Loop` | `b55338f07ddc5e8e…` |
  | `000000316e569781` | 4748 | `xx_Nav_PreNavWalkMid_Lt0_tr_Walk` | `192b4afea3347ce1…` |
  | `00000003836e1d49` | 1772 | `xx_Nav_IdleLow_Footr` | `dbac8b4f36c5bc1f…` |
  | `0000000fe013b72b` | 4191 | `xx_Nav_WalkFwd_3-Loop_SwayLt90` | `847612425053c10b…` |

- Tools:
  - New `acu-root-tracks` (acu_assets bin) dumps tracks 0/1 from the reviewed animation decoder.
  - New `tools/native_probe/root_delta_capture.py` (SHA-256 prefix `bd6a43b248f59df8`): the
    `M+0x90` write watch plus leaf samples about every 5 ms on the probe's QPC clock.
  - New `tools/root_delta_compare.py` (`7f4297da2a9f0414`).
  - Native probe build `d7b69cca81c32cd3…` (debug-register re-sync fix, see Tooling note).
- Live: Paris, pinned save `capture-start-1`, pid 122972, **60 fps cap**, focus shim t8,
  Invisible + God. Run `local/native-probe/runs/root-extract-2/` (local, ignored):
  `delta-vs-clip.jsonl` (`080799ebb6235b1d…`, idle 1 s, walk forward 6 s, stop 1.5 s; 535
  setter events, 0 dropped), `root-tracks.json` (`ac243c34f101d63a…`), `compare.json`.

## Observations (static)

1. **Entry.** `0x1400EF1B0` calls `0x1400FA080(controller = [anim+0xD0], dt, &out, …)`
   (ACU-RE-0057).
   - `out` = {translation `+0x00`, rotation `+0x10`, an array of 32-byte entries at `+0x20`
     (count u16 `+0x2A`), u32 `+0xD0`}.
   - `0x1400FA080` builds a 104-byte evaluation context: `+0x00` controller, `+0x08`
     controller+16, **`+0x10` = &out**, `+0x50` weight 1.0, `+0x54` dt. It then calls the root
     graph node `[[controller+0x670]+0x10]+8` through vtable `+8` (update) and `+0x50`
     (evaluate).
   - The out struct sits on the stack of whichever job worker runs the update: six worker
     stacks were seen, all at the same frame offset.
2. **Per-node root delta cache.** Each graph node has a 256-byte record at
   `[controller+0x2F0] + slot·256` (`+752`).
   - `+32` holds the node's translation delta, `+48` its rotation delta, `+64` an entry array,
     and `+240` flags. `+244` is the advanced time (`FLT_MAX` = not yet), and `+248` means
     "root delta computed this frame".
   - The nodes receive a node-evaluation context, a different struct from the top-level one:
     weight f32 `+0x50`, flags u32 `+0x54`. When flag bit 2 is set (root motion wanted),
     `0x1400B2F20` copies the evaluated node's record into the parent's `out` (`+32/+48` →
     `out+0/+16`).
3. **Playback leaf.** `0x1400B30D0` takes playback `[[state+0x260] + 8·slot]`, the playback
   object of ACU-RE-0025.
   - With flag 4 set and the record not yet computed, it calls `0x140018F90(playback, …,
     &record+32, &record+240)`. On the time-set path (flag 8) it calls `0x140018F60` →
     `0x140019200` instead.
   - `0x140018F90` resets out to (0, 0, 0, 0) and (0, 0, 0, 1). The clip descriptor
     `[playback+0x38]` names two tracks: **rotation** at descriptor `+28` and **translation** at
     `+24`. Each is a u16 pair whose second u16's low 13 bits are the track index (`0x1FFF` = none).
   - It then runs the rotation sampler `0x140019CA0`, then the translation sampler `0x14001ABA0`.
     Both look up the codec block `[[playback+0x20]+96][index]` and decode through
     `off_145137A40[codec byte]` method 2. Key frames are indexed as `(int)(60.0 × time)`
     (constant `0x1439CC2B0` = 60.0). A segmented or cached sampler exists at `0x140035AE0` /
     `0x14007BEF0`.
4. **Delta math** (A = the first time sampled, B = the second; both = playback offset `+0x36C` +
   a time):
   - Rotation: `Q(A)`, `Q(B)`, then `a6 = conj(Q(A))`, rotation delta = normalise(conj(Q(A)) ⊗
     Q(B)).
   - Translation: delta = 2·[rotate(conj Q(A), T(B)) − rotate(conj Q(A), T(A))] / 2, i.e.
     **R(Q(A))⁻¹ · (T(B) − T(A))**, the motion expressed in the root frame at A. Constants:
     `0x14525B940` = (2, 2, 2, 0), `0x14525B950` = 0.5.
   - Mirror: byte `playback+0x37E` (894) bit 0 negates the x component of translation (and flips
     the rotation sign mask `0x142E77EC0`).
5. **Loop wrap and composition.** `0x1400F8DE0`, when the time passes the end (`v37 < v39`),
   evaluates two segments and joins them with `0x1400125C0`. That composes (translation, scale
   in w) ∘ (translation, rotation): `t = t1 + R(q1)·(s1·t2)` and `q = q1 ⊗ q2`, with
   `0x142E78010` = (2, 2, 2, 0) in the rotate step.
6. **After the graph** (ACU-RE-0057): `0x1400EF1B0` subtracts the pending offsets `anim+0x2F0`
   (translation) and `+0x300` (rotation) and resets them.

## Observations (live, run root-extract-2)

7. **Descriptors.** Idle clip `00000003836e1d49`: descriptor bytes `+24..+31` =
   `ffff0000 ffff0100`, i.e. translation → track 0 and rotation → track 1. These are the two
   root channels the acu_assets decoder already validates (ACU-RE-0018). Its mirror byte is
   `0x04` (bit 0 clear).
8. **Single clip at weight 1** (`tools/root_delta_compare.py`; A and B = the leaf's offset +
   time read just before and just after each frame's update; tracks interpolated linearly):

   | Clip | Frames | Native \|delta\| per frame (median) | Error median / max | \|native\| / \|predicted\| |
   | --- | --- | --- | --- | --- |
   | `xx_Nav_IdleLow_Footr` | 119 | 0 | 0 / 0 mm | n/a |
   | `xx_Nav_PreNavWalkMid_Lt0_tr_Walk` (mirror bit 1) | 10 | 6.2 mm | 0.003 / 0.005 mm | 0.9995 |
   | `xx_Nav_Walk_3-Loop` | 10 | 33.0 mm | 0.016 / 0.018 mm | 0.9995 |

9. **Two playbacks of the same loop crossfading** (76 frames): the pose-weighted sum and either
   leaf give the same delta. Error: median 0.016 mm, max 0.019 mm.
10. **Clip-to-clip transition** (walk start → walk loop, 13 frames, start-clip weight 0.99 →
    0.81): the native delta equals the **incoming** walk loop's delta. Error: median 0.024 mm,
    max 1.24 mm. The graph-weighted blend is wrong by a median 12.7 mm (example: native 32.4 mm,
    blend 12.8 mm, walk loop 32.4 mm, at start-clip weight 0.991). In 11 of 13 frames the best
    match is the leaf whose weight is rising.
11. **Walk loop root track:** 82 keys over 2.8 s, 6.90 m along local +y, constant root
    orientation. The walk start moves 4.64 m over 2.97 s. The root channel of all four clips has a
    single constant rotation key.

## Inferences

- The native root delta for a playing clip is the decoded root track's difference over the frame's
  time step, in the root frame at the start time. Linear interpolation between keys reproduces
  it to about 0.02 mm for these clips. The 0.05% length ratio could come from interpolation or
  from time rounding.
- In a transition between different clips, root motion follows the transition target, while
  the graph weights (ACU-RE-0025) only blend the pose. This is one transition type, and the
  mechanism (which node selects the source) is not traced.
- The ACU-RE-0032 correction kernel and the pending offsets (`anim+0x2F0/+0x300`) act after the
  extraction; they were zero or unobserved in this run.

## Unknowns

- Clips with a rotating root channel (turns, pivots): the quaternion path is derived statically
  but not compared live.
- Blend nodes other than a transition: directional blends such as the sway clip (weight up to
  0.12 in run root-extract-1) and additive layers. Those frames were not single-leaf or not
  decoded: `00000007fddf8cdc`, `00000018b7887b1b` and `0000000ff1c61e15` are not in the dumped set.
- The codec sampler's interpolation (`off_145137A40` method 2) beyond the linear match shown, and
  the segmented sampler `0x140035AE0`.

## Tooling note: stale debug registers (probe fixed)

- In run root-extract-1 a stack watch fired about 49,000 times a second, so the script's rate
  guard unhooked it.
- One worker thread then kept the old watch: `SetThreadContext` on a thread that is inside the
  trap handler is undone when the handler returns its saved context, and the probe marked the
  thread as updated.
- A later capture on another slot ended with ACU exiting on an unhandled `0x80000004` (single
  step) in `d3d11.dll`. The stray trap from the stale register is the probable cause.
- Fix (`probe.cpp`):
  - The handler re-syncs the trapping thread's DR0–3/DR7 to the current hooks on every trap.
  - It claims any debug-register trap on an address the probe armed instead of passing it on.
  - `STATUS` reports `stray_traps`.
- The fixture regression passes. The previous build is kept as
  `local/native-probe/acu_probe_pre-drsync.dll` (restore by copying it back to `acu_probe.dll`).
- Run root-extract-2 with the fix: 52 traps/s, 0 dropped, 0 conflicts, 0 stray traps.

## Development handoff

- **Consumer:** `acu_motion` (sampling) and `acu_sim` (ADR-0005, `clip_root`).
- **Port recipe for one clip:**
  - `delta_t = R(Q(A))⁻¹ (T(B) − T(A))` and `delta_q = Q(A)⁻¹ Q(B)`, with T/Q = tracks 0/1 at
    offset + time, interpolated linearly. Mirror negates x.
  - On a loop wrap, compose A→end with 0→B using `t = t1 + R(q1)t2` and `q = q1 q2`.
  - In a clip-to-clip transition, take the delta from the incoming clip (observed for one
    transition type).
- **Acceptance test available:** `tools/root_delta_compare.py` on a capture plus the
  `acu-root-tracks` output. A port should reproduce the per-frame native deltas in
  `compare.json` to within 0.02 mm for single-clip frames.
- **Safety:** use one data watch on heap addresses only. Never watch the job-worker stack.
