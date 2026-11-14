# ACU-RE-0056 — Who writes the player's transform: the physics body, synced after the physics step

Date: 2026-11-14. Related: [ADR-0005](../decisions/0005-root-motion-ownership.md) (root-motion
ownership), [ACU-RE-0044](ACU-RE-0044-player-state-and-repeatable-start.md) (player chain,
matrix at E+0x20), [ACU-RE-0051](ACU-RE-0051-tick-and-time.md) (frame and physics step).
State: **observed live (runs position-writers-1, -2).** The writer chain and the per-frame order
are observed with hardware data-write watches. What drives the physics body (velocity source,
animation root motion or not) is **not** established.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, base
  `0x140000000`. Disassembly from the mapped image (Unity project)
  `local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`, SHA-256
  `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`. Corpus files
  `141CE1FF0.c`, `1420F6F70.c`, `1420F7AB0.c`, `142608180.c` (`E:/ReverseEngineering/Projects/Unity/ACU - Decompiled/`).
  Export identity is unverified (ACU-RE-0031).
- Tool: native probe with the new `HOOKDATA` command (hardware data-write watch; DR7 R/W = 01,
  LEN by size; data slots keep 512 stack bytes). Build SHA-256 `0e3fb5d3fc5153b0…` (prefix);
  the previous build is kept as `local/native-probe/acu_probe_pre-hookdata.dll` (`b56d3efd13cfeecc…`).
  Regression: `tools/native_probe/test_fixture.py` (watch on the fixture counter, invalid inputs,
  unhook).
- Script: `tools/native_probe/position_writers.py` (watches E+0x50 x/y, E+0x58 z and E+0x30
  forward on the current player; pad phases idle, walk forward, stop, walk right, free-run with
  RT+A, idle).
- Live: Paris, pinned save `capture-start-1`, fresh launches at a **60 fps cap**: pid 119072
  (run 1, 64-byte stacks) and pid 122204 (run 2, 512-byte stacks). Focus shim t8, Invisible +
  God. Raw data: `local/native-probe/runs/position-writers-{1,2}/writes.jsonl`.

## Observations

1. **One store site writes the player's matrix in every phase.** The writes come from
   `0x1425DE360`, a leaf that copies a 4×4 matrix from `[rdx]` to `rcx+0x20..+0x5F` with four
   `movaps`. The traps land at `0x1425DE36F` (row E+0x30, forward) and `0x1425DE37F` (row
   E+0x50, position): 2,116 + 1,880 hits in run 1 and 1,902 + 1,914 in run 2. There is no other
   writer, in idle, walking, stopping, strafing or free-running.
2. **Its only caller is the entity's "set world matrix".** In all writes the return address is
   `0x141CE2012`, inside `0x141CE1FF0`. That routine also clears `E+0x164`, sets dirty flag
   `0x20000` at `E+0x88`, and notifies each component through component vtable `+560` with the
   new matrix. The player entity's vtable `0x143017D60` (class "Entity") has `0x141CE1FF0` at slot
   `+0x80`.
3. **The caller of that is `0x1420F6F70` (all writes).** Its return address `0x1420F6F99` is at
   the trap's `rsp+0x30` in 2,116 of 2,116 writes. That is the virtual call `[rax+0x80]` on
   `[a1+0x40]`, passing the caller's matrix (rdx) and flag (r8) through. The routine runs
   about 11,000 times a second in total, apparently for every entity. An entry probe on it
   stalled the probe client for 31 s, so it must not be hooked.
4. **Two paths per frame reach it** (run 2, by the return address at `rsp+0x140`):
   - **Path A** (915 writes): `0x1420F7AB0` (set transform from rotation + position) called from
     `0x141A87F1C`. Just before, that code normalises a quaternion (`rsqrtps`) and calls
     `0x141A96090`, then passes rcx = `[[rbx+8]+0x1C88]`, rdx = the routine's result and r8 = a
     position on its stack. The decompiler has no function covering `0x141A87F1C`.
   - **Path B** (971 writes): `0x1420F7350` (a thunk), reached from `0x142608180`, which reads a
     body's world matrix at `[a1+0x768]+0x230..+0x260`, applies the local offset `-a1[+0x70]`,
     and pushes the result when byte `a1+0x29` is set. Stack values `0x1425F0074` and
     `0x14260828C` are in the physics module's range (next to the step planner `0x1425FB720`
     and the ray cast `0x1425FD830`).
5. **Path B carries all the motion; path A changes nothing** (consecutive writes, run 2):

   | Phase | A → B (≈3 ms apart) | B → A (≈13.5 ms apart) |
   | --- | --- | --- |
   | idle | 0 mm | 0 mm |
   | walk forward | 30.0 mm median (max 46.5), \|dz\| 0.42 mm | 0 mm, 0° yaw |
   | walk right | 32.0 mm, yaw 0.43° | 0 mm, 0° |
   | free-run (RT+A) | 63.0 mm (max 112.4), \|dz\| 1.45 mm | 0 mm, 0° |

   The order per frame is A, then B, then (next frame) A. Path A writes exactly the position and
   heading that path B left. Every displacement and heading change appears between A and B.
6. **Speeds at the 60 fps cap:** 30 mm per frame walking is 1.8 m/s; 63 mm per frame
   free-running is 3.8 m/s.

## Inferences

- The player entity's transform is **owned by its physics body**. Each frame, after the physics
  step, path B copies the body's world matrix (minus a local offset) to the entity. Path A is
  character code re-asserting orientation and position from data that already matches.
- The question in ADR-0005 therefore moves one level down: **what drives the physics body.** The
  candidates are a character-controller velocity from movement logic, or one derived from
  animation root motion. Path A's preceding work (`0x141A96090`, quaternion normalisation) is the
  nearest lead on the character side.
- `[[rbx+8]+0x1C88]` in path A is probably the entity reached from the Human object (ACU-RE-0046,
  Human = `[AtomAnimComponent+0xC58]`); that is not checked.

## Unknowns

- The writer of the body's velocity or target (Havok-side, under `0x1425F…`–`0x1426…`).
- The enclosing function of `0x141A87F1C` and what `0x141A96090` returns.
- Whether the A/B order or count differs during parkour actions (free-run here was mostly
  running; a scripted vault was not isolated).

## Development handoff

- **Consumer:** `acu_sim` movement and collision, ADR-0005.
- **Model for the port:** one authoritative character body per player. Movement logic sets its
  motion. The physics step moves and collides it, and the entity transform is a copy of the
  body's pose after the step. This matches the ADR-0005 choice ("simulation owns the transform"),
  with the body as the owner. The native velocity source is still open.
- **Oracle numbers:** walk 1.8 m/s and free-run 3.8 m/s at 60 fps (this run, single samples per
  phase); compare with `data/oracles/locomotion.json` and `camera.json`.
- **Next probe:** `HOOKDATA` on the body's translation (`[a1+0x768]+0x260`, read from path B's
  `a1`) to find the solver's writer. Then find the velocity input (set-velocity call) and compare
  it with the playing clip's root delta per frame.
- **Safety:** never hook `0x1420F6F70` (hot); data watches on one entity are cheap (≈130
  traps/s).
