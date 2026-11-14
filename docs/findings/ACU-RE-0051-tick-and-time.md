# ACU-RE-0051 — Simulation tick, frame clock and time step

Date: 2026-11-14. Related: [ACU-RE-0044](ACU-RE-0044-player-state-and-repeatable-start.md),
[ACU-RE-0049](ACU-RE-0049-world-travel.md), REWRITE_READINESS backlog item 2.
State: **partly live-verified (run tick-1, 30 and 60 fps caps).** The static part is read from the
pseudocode corpus and the mapped image; the live part (section "Live result") is from probe
sessions on the running game. 100 fps and uncapped are still open. Oracle: `data/oracles/tick.json`.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, image base
  `0x140000000`.
- Mapped image `local/native-probe/motion-link-20261020/runtime-view/capture/main-image.mapped.bin`
  (Unity project), SHA-256 `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520`;
  VA = `0x140000000` + file offset. Used for constants, tables, entry bytes and rel32 xrefs.
- Corpus `E:/ReverseEngineering/Projects/Unity/ACU - Decompiled/` (file = function VA). Files and
  lines cited inline; first 16 hex digits of each file's SHA-256:
  `1427743F0.c` `baca713677fbf87f`, `14276D750.c` `f6285cc757b551f3`, `1425FB720.c`
  `d41f0af32f1ca7c4`, `141CC19A0.c` `eac21669b1ef9825`, `142748C80.c` `b71506838aa74f5a`,
  `141CE4650.c` `dce3b28aeb40cf1c`, `141D36870.c` `e19c14904c679598`. Export-to-install
  correspondence of corpus code bytes is unverified (ACU-RE-0031).
- Tools: `rg` over the corpus (bounded); a scratch numpy rel32 scanner over `.text` of the mapped
  image (target = disp-site + 4 + disp; functions from `.pdata`); capstone disassembly
  (`acudis.py dis`). The scanner gives candidate references, not a full xref database: it misses
  computed/indirect references and `.pdata` boundaries can be chunk starts.
- Imports are packed (one IAT entry per DLL), so `QueryPerformanceCounter` has no name in the
  corpus; the raw tick source `0x142129CC0` jumps into a protected stub `0x1456E0F54` (`.UBX0`).

## Observations (static)

1. **Frame job graph.** `0x141CE4650` builds the engine loop's named jobs (job record stride 424,
   function pointer at job+384): `BeginFrame` → `0x141CC19A0` (lines 161–166), `BeginEngineFrame`
   → `0x141CC1840`, `Step` ("Engine::EngineLoop::Step") → `0x141CE6380`, `StepPreWorlds` →
   `0x141CE6850`, `UpdateCamera` ("Ai::UpdateCamera") → `0x141EA42A0` (lines 320–325),
   `EndFrame` → `0x141CC9440`, plus Domino, PreStep, network, sound and others. The context of
   `BeginFrame` is the tagged pointer built from `qword_145217380` (line 160).
2. **BeginFrame** `0x141CC19A0` (lines 22–25): `++*(u32*)(ctx+8)`; `*(u64*)(ctx+40) =
   sub_14275C200()` (engine game time before the update); then `sub_1427743E0()` →
   `sub_1427743F0([0x14525FA20])`, the engine frame clock update.
3. **Engine frame clock** (160-byte object, global pointer `qword_14525FA20`, created in
   `0x14275E410` line 193; ctor `0x142748C80`). Update `0x1427743F0`:
   - `+0x80 = +0x78` (previous raw ticks); `+0x78 = sub_142129CC0()` (raw ticks) (lines 25–27).
   - measured dt (s) = `(cur − prev) / qword_14525D9A8 * 0.001`, where `qword_14525D9A8 =
     qword_14525D988 / 1000` (`0x14275E930`), i.e. ticks per millisecond.
   - mode `u32 +0x94` (line 35) selects the dt policy:
     - 0: `dt = 1 / f32 +0x90` (fixed; ctor sets `+0x90 = 30.0`, `0x41F00000`, line 28).
     - 1: quantised ladder over the table `f32 +0x0C[10]` = 1/1, 1/10, 1/12, 1/15, 1/20, 1/25,
       1/30, 1/60, 1/90, 1/120 (ctor lines 12–22), index `+0x04` (init 9), count `+0x08` (10),
       hysteresis counter `+0x00` (steps up after more than 10 frames under the current entry).
     - 2 (**ctor default**, line 29): `dt = min(measured, f32 +0x0C)` = min(measured, 1.0 s).
     - 3: mean of a ring at `+0x34` (`× 0.06666667`, line 64; ring byte index `+0x98`).
     - other: measured.
   - then `dt = max(dt, 0.0001)` (lines 103–104); `u64 +0x88 += (int)(dt × 100000)` (game time
     in 10 µs units, line 107); `sub_14276D750(qword_14525D9D0, dt × 1000, 1)` (line 108) feeds
     the root clock in **milliseconds**; for modes 0, 1, 3 it also stores a pacing deadline at
     `+0x70` = cur + ticks/ms × dt_ms (mode 2 does not).
   - The only static writer of the mode found is a debug key handler (`0x140008130`, case 70)
     cycling `dword_1439BF178` = {0, 1, 2, 3} through `0x14276D8F0`.
4. **Clock tree** `0x14276D750(clock, dt_ms, advance)` (88-byte clock, root pointer
   `qword_14525D9D0`, created `0x14275E410` line 133; disassembly for the exact offsets):
   - `+0x20` = previous dt if |+0x18| > 1e-5; pause counter `+0x38` += pending `+0x40`; if the
     counter > 0, dt = 0 (paused).
   - otherwise scales: `× f32 +0x24` if byte `+0x44`, `× f32 +0x28` if byte `+0x45` (when both
     are on, a floor of 0.01 ms).
   - `f32 +0x18 = dt × 0.001` (**seconds**), `f32 +0x1C = 1/dt`, `u32 +0x30 = (int)(dt_ms × 30 +
     0.1)`; when advancing: `u32 +0x2C += 1`, `u64 +0x00 += +0x30` (ticks of 1/30000 s),
     `u64 +0x08 = +0x00 / 30` (milliseconds) (corpus lines 41–54).
   - children: pointer array at `+0x47`, u16 count at `+0x51` (unaligned, from the
     disassembly at `0x14276D866`/`0x14276D86A`); each child gets this clock's scaled dt_ms, or
     `+0x20 × 1000` when the child's byte `+0x46` is set.
5. **World clocks.** The World ctor `0x141D36870` (World type descriptor name "World", type hash
   `fbb63e47`, matching the asset-index World type) initialises three clocks at `World+0x7D8`
   (2008), `+0x830` (2096), `+0x888` (2184) (lines 109–149: scales 1.0, `+0x30` = 100) and
   registers each as a child of the root clock (`0x14274D0D0(qword_14525D9D0, …)`, lines 158–160).
6. **Gameplay reads the World clock.** About a hundred corpus functions pick
   `World+0x830` if `*(u64*)(entity+0x88) & 0x0400000000000000` else `World+0x7D8`, with the World
   from `entity+0x60` (e.g. `141663D10.c` lines 36–51; getters `0x140031750` = `+2008`,
   `0x140031600` = `+2096`). A script opcode returns `f32 clock+0x18` as delta time
   (`1420B9250.c` lines 1110–1130 → `0x140DC57D0`). Some consumers clamp it to 1/30 s locally
   (`141663D10.c` line 49–50, `1416597F0.c` line 158–159).
7. **Physics step planner** `0x1425FB720(physicsMgr)` (caller `0x1425FB6C0`; reads
   `f32 [[physicsMgr]] + 0x7F0`, i.e. World clock A dt, disassembly `0x1425FB739`):
   - `+0x10F0` = frame dt; `v = dt + carry(+0x10FC)`, capped at 1.0; below 1e-5: zero steps.
   - frame rate `r = 1/v`. If 15 ≤ r ≤ 30: one step of **1/30**. Otherwise pick from the table
     `flt_143621720` = {120, 60, 45, 30} Hz with ±5 Hz hysteresis against the previous rate
     (`+0x10F8`): one step of **1/table rate**, carry 0, `+0x1100` = `dword_143621730` =
     {1, 4, 6, 8}. Below that (r < 15): `n` steps of 1/30 while the remainder ≥ 1/35, carry =
     dt − n/30 (lines 46–96).
   - outputs: `+0x10F4` step dt, `+0x10F8` rate, `+0x10FC` carry, `+0x1100` table parameter,
     `+0x1104` step count, `+0x1108` per-step dt. Defaults set in `0x1425EB240` lines 154–156
     (1/30, 1/30, 30.0). Users of the step: `0x142604620` lines 127–130 (`+0x10F4` and
     `+0x10F8` in a `½h²` integration), getters `0x142611D70`/`0x142611D80`.
8. One subsystem pre-warms with 20 fixed steps of 1/30 (`142630CE0.c` lines 13–31).

## Inferences (to be checked live)

- **Gameplay is variable-step**: one update per rendered frame with dt = engine dt (default
  mode 2: measured, clamped to [0.0001, 1.0] s) × root scales × World-clock scales, zero while a
  clock is paused. No fixed-step accumulator was found at the engine-loop level.
- **Physics is quantised, not free**: its step is snapped to 1/120, 1/60, 1/45 or 1/30 s from the
  frame rate (one step per frame), with 1/30 sub-steps only below 15 fps. Taken literally, at e.g.
  100 fps the step would be 1/60 (100 < 120 − 5) — physics time would not track game time. That
  reading must be confirmed live before it is used.
- `qword_145217380` is the engine-loop object and `+8` its frame counter (context reconstructed
  from the tagged job pointer; not proven).
- `sub_142129CC0` returns QueryPerformanceCounter ticks (`qword_14525D988` = frequency).
- World clock B (`+0x830`) serves entities with flag bit 58 at `+0x88`; its purpose (e.g. UI /
  unscaled / cinematic) is unknown. Clock C (`+0x888`) has no identified consumer.

## Unknowns

- Who sets the engine clock mode in retail, and whether the shipping game runs mode 2.
- Where the physics step count `+0x1104` is consumed (Havok sub-step loop not located) and what
  the 1/4/6/8 table value means.
- Which threads run which jobs; whether any gameplay system keeps its own accumulator.
- The pacing deadline `+0x70` reader (frame limiter).
- Float determinism (unchanged from ACU-RE-0044).

## Live verification plan

Probe register index: 0 = rcx, 1 = rdx, 2 = r8, 3 = r9 (`probe.cpp` `regValue`); every hit also
records QPC, return address, 64 stack bytes and xmm0–xmm5. Profile:
`tools/native_probe/tick_time.profile.json`. Run each at 30 / 60 / uncapped with the shim frame
cap, standing still and while moving.

| # | Label | VA (RVA) | First 16 bytes | Snapshot | Confirms if | Falsified if |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | `engine_frame_clock` | `0x1427743F0` (`27743f0`) | `40534883ec30488b41780f2974242048` | rcx, 160 B (whole clock) | one hit per rendered frame; rcx == `[0x14525FA20]`; `+0x94` = 2; `(+0x78 − +0x80)/ticks_per_ms` ≈ 1000/fps ms; `+0x88` grows by Σ dt × 1e5 | hit rate ≠ frame rate, or mode ≠ 2 with an uncapped frame rate |
| 2 | `clock_update` | `0x14276D750` (`276d750`) | `48895c240848896c2410488974241857` | rcx, 88 B | xmm1 = dt_ms; for rcx == `[0x14525D9D0]` the same dt as hook 1; for rcx == World+0x7D8 the same dt × scales; `+0x18` after = dt s | World clocks not among the children, or dt differs without a scale/pause |
| 3 | `physics_step_plan` | `0x1425FB720` (`25fb720`) | `4883ec18488b01488bd10f293424488b` | rcx + 0x10F0, 32 B (values from the previous call) | at 60 fps step 1/60, count 1; at 30 fps step 1/30; at 100 fps step 1/60 (the literal reading); carry 0 above 15 fps | step equals frame dt, or count > 1 above 15 fps |
| 4 | `begin_frame` | `0x141CC19A0` (`1cc19a0`) | `40564883ec20488b15bb59550348895c` | rcx, 64 B | rcx == `[0x145217380]`; u32 `+8` increases by 1 per hit | rcx differs or counter not monotonic |

Memory reads without hooks (sample each frame or every 50 ms with `player_state.py`-style READs):

- `[0x14525FA20]` → engine clock: `u32 +0x94` mode, `f32 +0x90` fixed fps, `u64 +0x88` game time
  (10 µs), `u64 +0x78/+0x80` raw ticks; `u64 [0x14525D9A8]` ticks/ms, `u64 [0x14525D988]` frequency.
- `[0x14525D9D0]` → root clock: `f32 +0x18` dt (s), `u64 +0x08` ms, `u32 +0x2C` update count,
  `u32 +0x38` pause count, scales `+0x24/+0x28` with enables `+0x44/+0x45`.
- World `W = [[0x145217368] + 0x98]`: clock A dt `f32 W+0x7F0`, ms `u64 W+0x7E0`, pause
  `u32 W+0x810`; clock B dt `f32 W+0x848`; clock C dt `f32 W+0x8A0`.
- Confirm: root `+0x2C` and `[0x145217380]+8` advance by exactly one per frame; `W+0x7F0` equals
  root dt in normal play and drops to 0 in the pause menu (a pause test: open the in-game menu).

Oracle target: `data/oracles/tick.json` — per fps cap: hit rate of hook 1, dt distribution, mode,
World dt vs root dt, physics step/count/rate.

## Live result (run tick-1, 2026-11-14)

Setup: pid 86540, Paris, pinned save `capture-start-1`, focus shim t8 with a frame cap chosen at
launch, Arno standing. Probe session `tools/native_probe/client.py ... session
tools/native_probe/tick_time.profile.json`. Raw events are in `local/native-probe/runs/tick-1/`
(`events.jsonl` at the 60 cap, `events-cap30.jsonl` at the 30 cap) and `clock_reads.json` (memory
reads). Export: `python tools/oracle_export.py tick` → `data/oracles/tick.json`.

8. **One engine frame per rendered frame; variable step.** The engine frame clock `0x1427743F0`
   ran at 59.55 Hz under the 60 cap (1,060 hits in 17.8 s) and at 29.84 Hz under the 30 cap. rcx
   was the same object every hit. Mode `+0x94` was 2 in every frame, and the f32 `+0x90` read
   30.0 in both runs, so it does not limit the rate in mode 2. Game time `+0x88` advanced by the
   measured frame time: median 0.01666 s (p10 0.0166, p90 0.01673) at 60, and 0.03333 s at 30.
   Game time against wall time was 0.9997 and 0.9998.
9. **Clock tree.** `0x14276D750` ran about 10 times per frame (10.07 at 60, 9.85 at 30), with dt
   in xmm1 equal to the frame time in ms (median 16.668 at 60, 33.334 at 30). The memory reads
   agree: root clock `+0x18` and World clocks A/B/C (`W+0x7F0/+0x848/+0x8A0`) held the same dt
   (e.g. 0.016659 s) in every read.
10. **The physics step follows the frame-rate bucket, one step per frame.** The planner snapshot
    (`rcx+0x10F0`, values from the previous call) showed:

    | Cap | Frames | Step | Rate | Count |
    | --- | --- | --- | --- | --- |
    | 60 | 1,047 of 1,051 | 0.01667 s | 60 Hz | 1 |
    | 30 | 187 of 194 | 0.03333 s | 30 Hz | 1 |

    Isolated frames gave 45, 30 and 120 Hz at 60, and 60 Hz at 30, all with count 1. So the step is
    picked from a set of rates (at least 30/45/60/120) to match the frame, never sub-stepped at
    these rates. This **falsifies** the inference that the step stays at 1/60 with several
    sub-steps at low frame rates; it is consistent with "step equals frame dt" quantised to a
    bucket.
11. **BeginFrame** `0x141CC19A0` hit at 59.6 Hz (60 cap) with a single rcx. Its rate in the 30 fps
    file is not usable because the probe ring dropped events (the session reported overwritten
    hits).

Open: 100 fps and uncapped (which bucket above 60, and whether the count ever exceeds 1); the
bucket table in `0x1425FB720` and its thresholds; pause behaviour.

## Implementation implications

- `acu_sim` should take an explicit dt per update (not a constant 1/60) and clamp it to
  [0.0001, 1.0] s, with an explicit pause/scale per clock; time accumulates in integer ticks of
  1/30000 s and in milliseconds as the original does. These are candidates until the oracle exists.
- A physics/solver step, if ported, needs its own quantised step policy (table above), not the
  frame dt; record that choice in `DECISIONS.md` once confirmed.

## Development handoff

- **Consumer:** `acu_sim` tick policy.
- **Native behaviour (30/60 fps observed):** one simulation frame per rendered frame. Game time
  advances by the measured frame dt, and every gameplay clock (root, World A/B/C) sees that same
  dt. Physics takes one step per frame with the step set to a rate bucket matching the frame
  (60 Hz at 60 fps, 30 Hz at 30 fps).
- **For the port:** a deterministic fixed tick is still an implementation choice (AGENTS.md). Record
  in `DECISIONS.md` that the native game is variable-step, and test parity at the 1/60 and 1/30
  buckets, which are the native steps at those caps. Do not claim parity at other rates until 100
  fps and uncapped are measured.
- **Oracle:** `data/oracles/tick.json` (`acu-oracle/tick/1`). Regenerate it with a tick_time
  probe session per cap, then `python tools/oracle_export.py tick`.
