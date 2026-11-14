# ACU-RE-0061 — Animation key codecs 3, 7 and 8; codec 11 scale; codec 13 repeated keys

Date: 2026-11-14. Related: [ACU-RE-0018](ACU-RE-0018-animation-tracks.md) (track layout),
[ACU-RE-0058](ACU-RE-0058-root-delta-extraction.md) (samplers `0x140019CA0` / `0x14001ABA0`),
[ACU-RE-0059](ACU-RE-0059-trace-replay-root-motion.md) (339 of 590 played clips blocked).
State:
- **Observed (static).** The decoders are read from the binary.
- **Independently cross-checked** for codecs 7, 8, 11 and 13 against a third-party export of 92
  clips.
- **Codec 3** has unit tests and a continuity check only.

## Provenance

- `ACU.exe` SHA-256 `748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, image
  base `0x140000000`.
- Mapped image `9a1124f1c4c368939ad3d08315b3dc968e6e48cc22f7b096f34a94a5e7380520` (ACU-RE-0025),
  read for the dispatch table and the SIMD constants.
- Corpus (`ACU - Decompiled/<VA>.c`, SHA-256 prefix):

  | File | SHA-256 prefix | Role |
  | --- | --- | --- |
  | `140019CA0.c` | `497cb9753b3da28c` | track sampler; dispatch `off_145137A40[type byte]`, slot 2 |
  | `140064940.c` | `62320597499bdc16` | type 21 (codec 7) sampler |
  | `140087830.c` | `9de943b6dae8142f` | codec 7 key-pair decoder |
  | `1400646E0.c` | `a067a974854671fc` | type 24 (codec 8) sampler |
  | `140087ED0.c` | `c4b998db377024ea` | codec 8 key decoder |
  | `140064320.c` | `bffe130cd9fb4443` | type 9 (codec 3) sampler |
  | `140087B60.c` | `7f877076b407a3e6` | codec 3 key decoder |
  | `140065450.c` | `536870087f71b36c` | type 33 (codec 11) sampler |
  | `140063E60.c` | `8b9f97efeb12b5c5` | type 39 (codec 13) sampler |

- Assets: `DataPC.forge` entry 928 (file ID `0000002633b2c79d`, stored SHA-256
  `08b0a18bd2fa603a…`), 8,103 animations.
- Local outputs (ignored):
  - `local/anim-codec78/audit-928b.jsonl` (`20ff501c3d440be7…`);
  - `json-compare/compare.json` (`99cd11618ce94569…`);
  - `json-compare/ours.json` (`ba309f35704f51ab…`);
  - `codec3-sample.json` (`191c316b9e41b134…`).
- Tool: `tools/anim_json_compare.py` (`8fbae03de225c42a…` before the finding-number rename).
- Independent reference:
  - Package: the "Unity Mod" (AC Mirage) zip, SHA-256 `cfe73ff33c064283…72a779`
    ([PUPPET_NATIVE_REFERENCE.md](../PUPPET_NATIVE_REFERENCE.md)).
  - Contents: 93 JSON exports of ACU clips (92 distinct), produced by a third-party tool. Each
    filename's number equals the clip's resource ordinal in entry 928 (all 92 agree with the
    asset index).
  - Handling: the export stays local and is never committed. Only derived error statistics are
    recorded.

## Observations

1. **Dispatch.** Each track's type byte `t` indexes a 64-entry table at `0x145137A40`.
   - Each entry points to a record: name, output size (16, 4 or 1 bytes, or 8 for t = 63),
     sampler (slot 2), a seek helper (slot 3), and `(t << 8) | kind` (slot 4).
   - The seek helpers group by value size × time mode. They place codecs 1, 7 and 14 (2 bytes),
     3, 10 and 12 (4 bytes), 8 and 9 (12 bytes) and 4 and 11 (6 bytes) together. This matches the
     decoders below.
   - Codec = t / 3 and time mode = t % 3, as in ACU-RE-0018.
2. **Codec 7: 2 bytes, one axis.**
   - Raw u16 `r`: z = (r & 0x7FFF) · √2/32766 − 1/√2, with the scale bit-exact `0x383507C7`, and
     w = √(1 − z²).
   - `r` ≥ 0 as i16 gives (0, 0, z, w); otherwise the components swap, giving (0, 0, w, z).
   - This is codec 6 at 15-bit precision.
3. **Codec 8: 12 bytes, smallest three in floats.**
   - Three f32 values a0, a1, a2; the omitted component is s = √(1 − Σa²).
   - Omitted index = (bit 0 of a0) + 2·(bit 0 of a1).
   - The components are used as stored, flag bits included.
4. **Codec 3: 4 bytes, 10-bit smallest three.**
   - u32 `p`: index = p >> 30; components are (p >> 20, p >> 10, p) & 1023, each times √2/1022
     (`0x3AB55FA3`), minus 1/√2.
   - The components fill the non-omitted slots in order.
5. **Codec 11 is millimetres.** The type-33 sampler multiplies each i16 by 0.001. Our earlier
   decoder divided by 10,000, a factor of 10 too small. The tracks affected in the 92 clips are
   bone IDs `0xFFFFFFE5` and `0xFFFFFFE6`.
6. **Codec 13 keys can share a time.**
   - Codec 13 is a scalar, i8 · 0.008, interpolated linearly by its sampler.
   - Shipped clips repeat a key time in codec 13 tracks: 70 of 8,103 clips in entry 928, including
     3 of the 92 exported ones. The repeated key is a step.
   - The parser now accepts equal times for codec 13. Codecs 0–12 and 14 still reject them.

## Validation

| Check | Result |
| --- | --- |
| Unit tests (hand-computed values, truncation) | codecs 3, 7, 8 pass; fixture tests pass |
| Install test `installed_navigation_tracks` | pass |
| Export cross-check, 92 clips (`anim_json_compare.py`) | 8,496 tracks, 183,911 keys, 0 type / time / count / bone-ID mismatches, **max component error 6.7e-7** (the export prints 6 decimals) |
| …codec 7 (t = 21–23) | 153 tracks, 336 keys, max 5.5e-7 |
| …codec 8 (t = 24, 26) | 275 tracks, 851 keys, max 5.6e-7 |
| …codec 11 (t = 33, 35) before / after the fix | max 9.0 / 5.9e-8 |
| Codec 3 continuity, 40 clips | median 1.29°/frame, p99 16.7° (codec 4: 1.38°, p99 9.3°; codec 5: 0.83°, p99 8.5°) |
| Entry 928 full decode | **8,071 of 8,103** (before: codecs 3/7/8 unsupported) |
| Clips played in our traces (ACU-RE-0059) | **586 of 590** fully decode (before: 251) |

- The 32 remaining failures are:
  - 16 with unsupported root channels (facial and NPC clips);
  - 15 key-timing failures in other codecs;
  - 1 additive clip.
- The 4 played clips that still fail are facial clips with no root channels (`Facial_*`).
- The 2 locomotion clips whose codec 8 root blocked `parse_root` in ACU-RE-0059 now decode:
  `xx_NavSneak_LowProfile_DropDownCmn_tr_WalkCr_050cm_RC` and `xx_Nav_WalkStop_RC`.
- Codec 3 has no independent sample-level check: the export contains no codec 3 tracks. A live
  pose comparison (ACU-RE-0022 pose buffer) would give one.

## Unknowns

- Codecs 1 and 10 have samplers in the table (`0x140064080`, `0x1400651D0`) but no tracks in
  entry 928. They are still rejected explicitly.
- The two-key interpolation helper `0x14008A260` (slerp or nlerp) is not decoded here.
  `acu_motion` interpolates with its own sampler (ACU-RE-0058).
- What bone IDs `0xFFFFFFE5` / `0xFFFFFFE6` drive. They are in the assassination clips' codec 11
  channels.

## Development handoff

- `acu_assets::animation::parse` now decodes full poses for traversal clips. Bevy and rig playback
  can use it for 586 of the 590 clips seen in play, instead of root-only.
- Re-run `tools/anim_json_compare.py JSON_DIR OUT_DIR` after any decoder change. It needs the
  local export path, so it is an explicit local check, not CI.
