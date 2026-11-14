# ACU-RE-0062 — Automated window entries: targets, recipes, a deterministic replay, and where the variant is chosen

Date: 2026-11-14. Related: [ACU-RE-0045](ACU-RE-0045-decision-clips-and-window-sweep.md) (window
entries without a parkour decision), [ACU-RE-0047](ACU-RE-0047-paris-placements-offline.md)
(placements), [ACU-RE-0050](ACU-RE-0050-world-collision-ground.md) (collision ground),
[ACU-RE-0059](ACU-RE-0059-trace-replay-root-motion.md) (warp residual),
[ACU-RE-0061](ACU-RE-0061-animation-codecs-3-7-8.md) (full clip decode).
State:
- **Observed live:** the entries and the replay.
- **Observed offline:** targets, from placements and collision.
- **Inference:** that the variant is chosen in the graph (`NewDemo`). Only a candidate link
  supports it so far.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, base
  `0x140000000`. Launch via `capture_run.py --until world --keep-game --save-pin capture-start-1
  --fps-cap 60`. The focus shim blocked user input. Closed with `capture_run.py --close`. The
  saves were restored; they are byte-identical to the pin.
- Runs (local, ignored), `local/native-probe/runs/`:
  - `traversal-1`…`traversal-4`;
  - `window-replay-1`, with `replay.jsonl` and 60 Hz samples per stage;
  - game-window frames (`PrintWindow` on the game window only).
- Targets: `local/traversal/windows-district1.json`, from `tools/native_probe/traversal_targets.py
  windows --bounds 650 -350 1000 -100`. Inputs: `local/paris/placements.jsonl` and the Paris
  collision dumps.
- Tools:
  - `tools/native_probe/traversal_targets.py`;
  - `traversal_driver.py`;
  - `window_replay.py`;
  - headless tests in `test_traversal.py` (11 tests, simulated game).

## Observations

1. **Open windows are placements.** Every earlier `Window_Run_WindowEntry_*` play in the old
   Versailles traces (12 plays) starts 0.3–0.5 m from an `FRV_Interior_D_1F_WD_01_Open`
   placement position.
   - Their lateral offset (along rotation row 1) is within about 0.1 m.
   - The bounding-box centre is about 0.45 m to one side, so the placement position marks the
     opening.
   - Row 0 is the window plane's normal.
   - Outside and inside follow from collision: the inside column has `Interior_*` floors.
   - District (650…1000, −350…−100): 108 open windows (25 on the first floor, 56 on the second,
     27 on the third), all classified; 101 have no blocker outside.
2. **Run-in recipe.**
   - Free run with RT only, starting 6 m out on the normal and pursuing a point 1.5 m inside:
     2 of 4 runs entered. They played `xx_Window_Run_WindowEntry_Fd45Deg150cm_Footr`, then
     `…Fd0Deg150cm_Footr` (blended, as in ACU-RE-0045), and `…Fd0Deg150cm_Footr`.
   - Holding A as well makes Arno climb the wall (`WallEntryFwd_tr_Climb_*`; traversal-1).
   - Runs aimed at the bounding-box centre arrived about 0.8 m off and braced against the wall
     (`SprintWallBraceNohand_*`; traversal-2).
   - The frames show real open windows next to the brace points.
3. **Side entry from the wall, and a deterministic replay.**
   - In traversal-4, Arno hung on the wall 1.04 m along +row 1 from a second-floor window
     (`0000001ff93eef60`), feet at z 7.06 (sill 6.58, top 8.50). Pushing toward the window in low
     profile played the climb "intent" clips for about 1 s
     (`Climb_Intent_Lt_Idle`, `Climb_Intent_DnLt_Idle`), then
     `xx_WindowGratingEntry_Climb_Lt-High_tr_IdleHigh`. He landed inside at
     (803.364, −168.658, 5.997).
   - The replay (`window_replay.py`) reached a hang 0.64 m from the recorded spot, checked by
     position and a frame. The push played the same lead-in and the same clip, and landed at
     (803.3639, −168.6582, 5.9965): identical to the millimetre.
4. **"Grating" = balcony railing.** The frames show wrought-iron railings in front of these
   windows. The game chose the `WindowGratingEntry` family for a window with a railing.
5. **Stick mapping on the wall.** Stick right moves Arno along forward × up = (fy, −fx). Two
   independent movements agree; for these windows that is +row 1.
6. **Clip names alone don't give the direction.** `xx_Climbing_ClimbJump_Lt90Deg240cm` moved
   Arno 2.5 m along +row 1, to his right, so the clip played mirrored. Captures must record the
   playback mirror bit (`playback+0x37E`, ACU-RE-0058).
7. **Climbing limits.**
   - Beside a window the wall gave out at about 3.2 m (`Climb_Intent_Up_Stop`). The frames and
     window columns had handholds.
   - Under a second-floor window there is often a first-floor window. Pushing into the wall
     there (RT + A) ran Arno into the lower window; the user saw this live and the run was
     stopped.
   - A failed stall heuristic (flipping the lateral stick) sent Arno 3.5 m the wrong way. It is
     now off by default.
8. **The variant is chosen in the animation graph (inference).**
   - The asset index links `xx_WindowGratingEntry_Climb_Lt-High_tr_IdleHigh` (file ID
     `00000029595e4e0e`) up to the AtomGraph resources `NewDemo` (`000000013d5bc4d8`) and
     `NewDemo_DEV` (`000000017f0a92b7`), type `c0c861cd`. This is a candidate link (byte match),
     not a decoded reference.
   - With ACU-RE-0045 obs. 3 (no parkour-selection decision before window entries), the
     variant (angle, height, grating) is most likely picked by graph nodes from inputs the game
     sets.

## Unknowns

- `NewDemo`'s node format: which nodes hold the window clips, and which inputs and conditions
  select them.
- What the game writes into the graph for a window entry, and when.
- Whether the low-profile ground run can enter at all. Every run-in so far was high profile;
  the planned low/high-profile ring was stopped in favour of this route.

## Development handoff

- Use `traversal_targets.py` for window targets in any district.
  `traversal_driver.py --plan run_in` runs ground entries.
- `window_replay.py position|push` reproduces a recorded wall entry with a frame check before
  the push. The recorded side entry is a deterministic trigger for live hooks.
- Next: decode `NewDemo` around the window clips (offline). Then watch the graph inputs during
  the replayed entry.
