# ACU-RE-0045 — Parkour decisions paired with clips; first staged sweep (window entry)

Date: 2026-11-14. Related: [ACU-RE-0042](ACU-RE-0042-parkour-candidate-trace.md),
[ACU-RE-0043](ACU-RE-0043-parkour-action-classes.md), [ACU-RE-0044](ACU-RE-0044-player-state-and-repeatable-start.md),
ACU-RE-0025 (graph/playback route).
State: observed. Decisions and clip starts share a clock and are paired per run; a staged
approach sweep at one window produced angle-dependent window-entry blends. Small samples.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, base
  `0x140000000`; pinned save `capture-start-1` (ACU-RE-0044); capture preset; 60 fps cap.
- Clip readout `tools/native_probe/live_state.py` (ReadProcessMemory, reads only): animation
  component vtable `0x142E7F780` → controller `+0xD0` → graph rows `+0x6A8` → playback leaf
  (method `+0x58 == 0x1400BCC20`) → asset handle `+0x10` file ID, time `+0x384` (ACU-RE-0025).
- Pairing `tools/parkour_clip_pairs.py`; names from the local asset index
  (`tools/asset_query.py where`, label `Animation`, confidence `crc-name`).
- Runs (machine-local): `nav-1/pair-1-state.jsonl` (`c87856b2…f5d`), `catalog-1/live-state.jsonl`
  (`f55eda0e…d7ea`), `window-sweep-2/live-state.jsonl` (`409d51e7…78d9`) and
  `window-sweep-2/native-session.jsonl` (`8392bcd4…a49e`).
- Sweep `tools/native_probe/staged_sweep.py … --spot 69.34 -217.88 --heading 75.5
  --angles -60 -40 -20 0 20 40 60 --distance 6 --run-seconds 3 --route catalog-1/live-state.jsonl`.

## Observations

1. Idle readout: one leaf, file ID `00000003836e1d49` (`xx_Nav_IdleLow_Footr`), weight 1.0 —
   the same clip ACU-RE-0025 identified.
2. Decision → first clip start within 1.5 s (nav-1 free-run burst): type 53 →
   `xx_Parkour_WallEntry45_tr_ClimbPass_Fd200cm-Up270cm` (+0.30 s); type 39 →
   `xx_Parkour_Leaping_Fd650cm-Dn200cm_V03` (+1.01 s); type 8 → the same leap (+0.00 s) then
   `xx_Beam_Landing_LandMid_tr_IdleHigh0cm` (+1.02 s). In catalog-1: type 67 →
   `xx_Climbing_Climb_Intent_Up_Start`, type 106 → `xx_Parkour_Muraille_Exit_Fd100cm-Dn100cm`.
   Temporal association only.
3. Window entries start **without** a preceding parkour-selection decision (`0x140133B00`):
   in the sweep they appear only in the clip stream. Another system selects them.
4. Each window entry starts two angle variants together and their graph weights settle within
   about 0.2 s, then hold:

   | Run | Travel / facing (deg) | Speed | Settled weights |
   | --- | --- | --- | --- |
   | catalog-1 | 75.2 / 75.5 | 5.8 m/s | Fd0 0.95, Fd45 0.05 (Footl) |
   | sweep θ = −20° | 18.2 / 25.8 | 5.4 m/s | Fd45 0.60, Fd0 0.40 (Footl) |
   | sweep θ = +40° | 119.3 / 120.2 | 4.2 m/s | Fd0 0.55, Fd45 0.45 (Footr) |

   Arno's height rises to about 1.65 (clip name: 150 cm) and he lands inside at 0.50.
5. Approaches at −60°, −40° ran into the wall and played the `SprintWallBraceNohand_MoveRt*`
   family; 0° and +20° reached the interior through a different opening (heights −0.16 → 0.50,
   slope clips) without a window clip. θ = +60° could not reach its approach point.
6. Run-ups were 2–3 m, not the requested 6 m: facing steps walk Arno forward.

Inferences (not established): the window system blends angle variants by approach angle;
foot variant follows stride phase. The window normal is not measured, so the angles above are
travel directions, not angles to the window.

## Navigation lessons (implementation)

Straight-line approach walks stuck on a ledge (all seven trials of window-sweep-1 skipped);
`navigate.py` now aborts after 4 s without progress, calibrates from any of four directions,
follows recorded paths (`follow`, `--route`), and each trial returns along its own path in
reverse with free run (back out through the window), which worked in five of six trials.

Next: repeat the sweep with longer run-ups and repeats per angle, measure the window plane
(from the entry/landing positions of several trials), and find the selecting system for
window entries (the probe can hook it once located).

## Development handoff
- **Confidence:** partial — the observations are live (finding state: observed), but
  decision → clip links are temporal association only, samples are small and the window-entry
  selector is not located.
- **Established:**
  - Live clip readout: animation component vtable `0x142E7F780` → controller `+0xD0` → graph
    rows `+0x6A8` → playback leaf (method `+0x58 == 0x1400BCC20`) → file ID at `+0x10`, time at
    `+0x384`. Idle = `00000003836e1d49` `xx_Nav_IdleLow_Footr`, weight 1.0.
  - Decisions and clip starts share a clock. Observed follow-ups (nav-1, catalog-1): type 53 →
    `xx_Parkour_WallEntry45_…` (+0.30 s); types 39 and 8 → `xx_Parkour_Leaping_Fd650cm-Dn200cm_V03`;
    type 67 → `xx_Climbing_Climb_Intent_Up_Start`; type 106 → `xx_Parkour_Muraille_Exit_…`.
  - Window entries start without a `0x140133B00` decision; each starts two angle variants
    (Fd0/Fd45) whose weights settle within about 0.2 s (three-run table above).
- **Not established:** the window normal (angles are travel directions); that blend weight
  follows approach angle or foot variant follows stride phase (inferences); causality of any
  decision → clip pair; repeatability per angle; clip names are `crc-name` labels.
- **Interface:** for `acu_motion`: a clip-start event stream (file ID, start time, graph weight)
  and a two-variant window-entry blend whose selector input is unknown. The readout and sweep
  tools are live tooling only, not product code.
- **Oracle:** `tools/native_probe/live_state.py` (clip stream), `tools/parkour_clip_pairs.py`
  (pairing), `tools/native_probe/staged_sweep.py … --spot 69.34 -217.88 --heading 75.5`;
  compare clip IDs, start offsets and settled weights per approach.
- **Spawned / follow-up:**
  - Repeat the sweep with longer run-ups and several repeats per angle.
  - Measure the window plane from entry/landing positions.
  - Locate the system that selects window entries and hook it.
