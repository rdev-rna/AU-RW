# ACU-RE-0059 — Existing traces replayed against decoded clip root motion

Date: 2026-11-14. Related: [ACU-RE-0058](ACU-RE-0058-root-delta-extraction.md) (root-delta
formula), [ACU-RE-0057](ACU-RE-0057-body-velocity-from-root-motion.md) (world = R(entity) · delta),
[ACU-RE-0018](ACU-RE-0018-animation-tracks.md) (root tracks 0/1), ACU-RE-0032 (warp kernel),
ACU-RE-0042/0045 (parkour decisions and clips).
State: **offline analysis of earlier live traces plus asset decode.** No new game run.
- Locomotion is explained by clip root motion; parkour and climbing plays are not.
- Falls are gravity on the body.
- The link between the parkour residual and the chosen target is **not** established.

## Provenance

- Traces (local, ignored; `live-state.jsonl`, SHA-256 prefixes):

  | Run | `live-state.jsonl` |
  | --- | --- |
  | vault-scan-1 | `93e6e02a4cf53f72` |
  | vault-scan-2 | `457d0beba508fc66` |
  | vault-scan-3 | `68cbbcd9b853c2fd` |
  | window-sweep-1 | `3611aa8db2ccc705` |
  | window-sweep-2 | `409d51e75f1066d2` |
  | prop-probe-1 | `7f97a69fb46f25e6` |
  | catalog-1 | `f55eda0e599e0b10` |

  These are `tools/native_probe/live_state.py` samples at about 29 Hz (QPC, entity, position,
  forward, leaves with file ID, graph weight, playback time `+0x384`, offset `+0x36C`). Binary
  `748abdc6…7e40`, base `0x140000000`, as recorded in those runs. Frame caps differ between
  runs; intervals are in QPC seconds.
- Clips: 591 distinct file IDs played (graph weight > 0.001).
  - 590 are in `DataPC.forge` entry 928 (file ID `0000002633b2c79d`, stored SHA-256
    `08b0a18bd2fa603a…`), with no differing duplicates in other archives. The 591st is the null ID
    `0000000000000000`.
  - Root tracks are dumped with `acu-root-tracks`, now using the new `animation::parse_root`:
    584 decode. Failures: 4 facial clips have no root channels, and 2 locomotion clips
    (`xx_NavSneak_LowProfile_DropDownCmn_tr_WalkCr_050cm_RC`, `xx_Nav_WalkStop_RC`) use
    root codec 8.
  - Output: `local/native-probe/runs/root-extract-2/trace-root-tracks.json` (`011378d2a3688287…`).
  - The full parser (`animation::parse`) decodes only 251 of these clips: codec 7/8/3 in later
    bone tracks.
- Tool: `tools/root_replay.py` (`5a7d56be6eb0acc6…`). Report and rows:
  `root-extract-2/replay.json` (`aca61a87ccc0ebe7…`). Decision pairs:
  `root-extract-2/decision-play-pairs.json`.

## Method

- An **interval** is two consecutive samples that show the same single leaf (same graph row and
  file ID, weight ≥ 0.999, nothing else above 0.001), at most 60 ms apart.
- Predicted world displacement = R(sampled facing at the first sample) · clip root delta from
  time A to time B (ACU-RE-0058 formula, linear/slerp keys, loop wrap composed).
- Residual = actual − predicted.
- A **play** is a run of consecutive intervals of one clip with clip time non-decreasing. Plays
  that contain an interval over 1 m (a teleport) are excluded.
- The mirror bit and the right-axis handedness are not in these traces. Per interval the better of
  mirrored and unmirrored is kept. Because a mirror flip and a handedness flip are the same
  operation on x, the handedness cannot be chosen from this data (both give a median error of
  0.026 m).

## Observations

1. **Root-only decode coverage, by action family** (clips that played; family by name pattern):

   | Action | Clips seen | Root decoded | Replayed clips (intervals) |
   | --- | --- | --- | --- |
   | swing | 37 | 37 | 11 (267) |
   | side hop / lateral | 133 | 133 | 34 (2,889) |
   | drop / let go | 6 | 5 | 1 (2,333) |
   | controlled descent | 27 | 26 | 1 (14) |
   | controlled ascent | 51 | 51 | 14 (670) |
   | leap / jump | 121 | 121 | 37 (1,575) |
   | vault / skip over | 20 | 20 | 3 (125) |
   | wall entry / wall run | 73 | 73 | 30 (1,271) |
   | beam | 64 | 64 | 17 (623) |
   | hang | 34 | 34 | 5 (89) |
   | landing | 17 | 17 | 11 (1,814) |
   | window | 6 | 6 | 0 |

   The families overlap; a clip can match several.
2. **Locomotion is clip root motion.** 12,093 nav intervals have a median residual speed of
   0 m/s, p90 0.18 m/s. Per play (206 plays, median predicted travel 0.39 m), the residual has
   median 2.0 cm and p75 6.1 cm, and 147 of 206 plays are within 5 cm.
3. **Falls are physics.** `xx_Parkour_FallCycle` has a static root track, yet Arno moves during it.
   - Over 35 fall sequences whose vertical speed is linear in time (r² > 0.9), the vertical
     acceleration is **−9.55 m/s²** (median; IQR −9.67 to −9.35).
   - Horizontal speed is 0 in these sequences: straight drops in the vault scans.
   - The value is measured at the 29 Hz sampling; the engine constant is not read.
4. **Parkour and climbing plays deviate from the clip.** The magnitude agrees per interval
   (|actual| / |predicted| median 1.000 parkour, 0.998 climbing). But the residual over the
   predicted step is a median 11% (parkour) and 21% (climbing). Summed per play:

   | Family | Plays | Median predicted travel | Residual median / p75 | Within 5 cm | Over 30 cm |
   | --- | --- | --- | --- | --- | --- |
   | parkour | 168 | 1.30 m | 21.8 / 49.9 cm | 32 | 62 |
   | climbing | 23 | 1.68 m | 31.4 / 39.9 cm | 0 | 12 |

   - The largest residuals are wall entries, climb passes and skip-overs. For example,
     `xx_Parkour_WallEntry45_tr_ClimbPass_Fd200cm-Up070cm` was predicted at 2.04 m and actually
     moved 1.85 m, with a residual of (−0.24, 0.11, **+0.89**) m.
   - Climb-pass residuals often have a vertical part of 0.6–0.9 m.
5. **Clip names are authored situations, not travel.** Of 183 decoded clips whose names carry
   `Fd/Bk/Up/Dn/Lt/Rt…cm`:
   - 20 end exactly at the named offset (for example `Leaping_Fd350cm-Dn200cm` ends at
     (0, 3.5, −2.0));
   - 15 end at twice the named forward distance (beam steps, `SprintingStepDown`);
   - 148 differ, typically by matching the vertical part but adding run-in or run-out
     (`Sprint_SkipOver_Fd100cm-Up100cm` ends 9.8 m forward, at 0 m height).
6. **Decisions to plays (inconclusive).** 201 parkour decisions (ACU-RE-0042 format) are
   followed within 0.6 s by a single-leaf play. Only 3 of those plays start at clip time 0:
   single-leaf intervals exclude the blend-in.
   - Correlation of the selected candidate's height difference (an ACUFixes-derived, unverified
     field) with the play's actual rise is 0.49; with the clip's own rise it is 0.38.
   - The median absolute differences are 1.04 m and 1.07 m.

## Inferences

- Locomotion can be ported as `clip_root` without a correction term (ADR-0005), to about 2 cm per
  play at these sample rates.
- Airborne phases (fall cycles) need a ballistic body: the port applies gravity to the character
  body instead of clip motion. The −9.55 m/s² estimate is close to standard gravity; reading the
  engine constant would settle it.
- Parkour and climbing motion is root motion plus a correction worth tens of centimetres per
  action. This fits the ACU-RE-0032 target-warp kernel and the authored-situation clip names: the
  game picks the clip whose authored situation is nearest and warps the rest. It is an inference:
  the target-to-residual link (obs. 6) is not established, and collision or physics could
  contribute too.

## Unknowns and next steps

- **Blended intervals.** Most action plays begin while blending. Using the ACU-RE-0058 rule (root
  from the incoming clip) needs the playback identity and mirror bit, which these traces do not
  record. `root_delta_capture.py` records both, so a new capture of each action family
  (descents, drops, side hops, swings, vaults, windows) can use the rule.
- **The warp itself.** Watch the pending offset (`anim+0x2F0` / `+0x300`, ACU-RE-0057) and the
  warp kernel's output during actions. Compare it with the selected candidate's target position
  (ACU-RE-0042/0055), not the unverified height field.
- **Codec 7/8 bone tracks** (339 of 590 clips) block full pose playback of most traversal clips;
  root motion is not blocked.
- The engine gravity constant, and horizontal momentum in jumps that are not straight drops.

## Development handoff

- `acu_assets::animation::parse_root` decodes root motion for 584 of 590 played clips. Use it for
  traversal root motion until codecs 7 and 8 are decoded.
- `tools/root_replay.py` is the regression for root-motion ports on recorded traces: locomotion
  should stay within 5 cm per play for most plays.
