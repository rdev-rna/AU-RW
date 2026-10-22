# ACU-RE-0032 — Native translation-correction (target warp) kernel at 0x140188F00

Date: 2026-10-22. Investigator: claude-board. Related: issue #110, ACU-RE-0030 (#50), `acu_motion::target_warp`.
State: partially established. Pseudocode structure and the two correction modes are observed in the
corpus; no live capture gate exists for this kernel in the repository, the caller is not identified,
and code identity is unavailable (ACU-RE-0031).

Why this exists: `acu-rs/crates/acu_motion/src/target_warp.rs` was headed `ACU-RE-0031` for this
kernel, but ACU-RE-0031 is the corpus-binding finding and no finding recorded 0x140188F00. This file is the
missing record; the Rust header now points here.

## Provenance

Binary path / SHA-256 / image base / VA / RVA: installed `ACU.exe`, SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`, image base `0x140000000`
(ACU-RE-0031). Function VA `0x140188F00` (RVA `0x188F00`), size 4145 bytes per the corpus header.
Code bytes in the installed file are packed, so the export's code is **not** byte-verified against the
executable (ACU-RE-0031 limit).
Corpus path and lines: `ACU - Decompiled/140188F00.c` (628 lines, `sub_140188F00(__int64 a1, float a2, int a3)`).
Archive identity / range / file ID: not applicable.
Tool/provider/version / evidence IDs: corpus reading only (this session); no REA/IDA session, no live tap.
Exact commands and search boundary: read `140188F00.c` in full; read `target_warp.rs`; searched
`docs/` and `acu-rs/crates` for `140188f00` (only `target_warp.rs` matched); read handoff
`20261020T1544-root-traversal-scanner-targets-05.md` (no mention of this address).

## Result

Observations (from `140188F00.c`):
- `a2` is a float step time (`dt`) and `a3` an int selecting a correction mode. The function returns early with
  `0` if `sub_141CD1A80() == *(a1+340)` (lines 129-130) and with `1` if the entity pointer, a pointer at
  `+0x60` of it, byte `+64`, or word `+202` fail checks (lines 131-139).
- Two arrays of 80-byte records are scanned from the back: one at `*(a1+298)` with count `u16 @ a1+308`,
  one at `*(a1+310)` with count `u16 @ a1+320`. A record is live when its first dword is not `-1`; selection
  walks back from the end and stops at a live record with a flag byte (`+17`, or `+18`) set; the exact flag
  semantics and the list used for each test (the first loop reads `+18` through the second list's pointer) are
  not analysed. Per-record fields read: `+4` (index), `+8`
  (kind, values 1 and 2 handled), `+12` (float), `+17/+18/+19` (bytes), `+20` (float, compared with `dt` and
  used as time remaining), `+32` and `+48` (16-byte vectors) (lines 157-202, 242-283).
- When `a3 == 0` (lines 390-491): the dominant axis of the vector at record `+32` (variable `v111`) is chosen
  with `|component| > 0.0099999998` and Y winning only if strictly greater than both X and Z, else X if greater
  than Z, else Z. On the dominant axis a shear term `error[i] / remaining[axis]` is written for the other
  axes; for each non-dominant axis the scale is `error / remaining + 1.0`, or if `|remaining| <= 0.01` the
  translation is `error * dt / time_remaining` (`v97`).
- When `a3 != 0` (lines 492-533): the same scheme is applied using the animation step vector (`v105`) with
  correction `error * dt * (1 / time_remaining)`; the dominant axis is chosen with `>=` comparisons (X first), so tie
  ordering differs from the `a3 == 0` branch.
- `v113` is the error vector produced by `sub_142778E10`; a horizontal-complete flag is
  `|v113.x| <= 0.01 && |v113.y| <= 0.01` (line 378).
- The result is composed with a 4x4 transform, subtracted from the step, and applied by
  `sub_1425F2500` / `sub_14262DA70` when the entity has a physics/mover object (`sub_141CD0190`), else by
  rebuilding an orthonormal basis (three `rsqrt` normalisations) and calling `sub_1420F7510` (lines 540-617).
- Byte flags `a1+344` and `a1+345` are set to `dt >= record float` (lines 318, 356); a float timer at `a1+324`
  is decremented by `dt` and clamped to 0 (lines 541-544).

Cross-check against the Rust port (`target_warp.rs::translation_correction`): the dominant-axis rule, 0.01
threshold, scale/shear/translation formulas and horizontal-complete test match the two branches above
line by line as read here. The Rust tests use synthetic inputs with expected values written alongside the code,
so they are not an independent native oracle.

Inferences and alternatives:
- The function performs a per-frame correction of the animation step toward a target using event-supplied
  remaining-motion and error vectors, i.e. target-driven translation warping of root motion. This is an
  inference from the arithmetic; the name, owner type and what supplies `a3` are unknown.
- `target_warp.rs` states `a3` is "native controller +448". The pseudocode shows `a3` as a function argument
  only; the +448 claim comes from the caller, which this finding did not examine. Unverified here.
- The relation to the ACU-RE-0030 scanner (`0x14017f8e0` / `0x1401a7000`, target storage at +400/+416) is
  plausible but not established.

Unknowns / incomplete coverage:
- Callers of `0x140188F00`, the type owning `a1`, what writes the 80-byte records, and the meaning of record
  fields not listed above.
- The excerpt addresses quoted in `target_warp.rs` (`0x14018979e..0x140189b81`) cannot be checked against
  pseudocode, which carries no instruction addresses.
- Rotation handling, event lookup and collision (outside the Rust port) are present in this function and not
  analysed here.
- No live gate: no tap capture compares native deltas with `translation_correction` in the repo.

Independent comparison / falsification test: capture `a1`, `dt`, `a3`, the selected record, error and output
step at runtime (probe/tap, lock `game-process` #30) and compare `delta` with `translation_correction`; the
reading is falsified if native outputs differ beyond float rounding for the same inputs.

Reproduction steps: open `ACU - Decompiled/140188F00.c` and read the line ranges cited above; run
`cargo test -p acu_motion target_warp` for the Rust-side tests.

Implementation implications and finding-linked constants: tolerance `0.0099999998` (f32 of 0.01); mode
split on `a3`; keep `translation_correction` marked approximate until a live gate exists. This is evidence that
ACU applies target-driven translation correction; it does not by itself describe full locomotion.
