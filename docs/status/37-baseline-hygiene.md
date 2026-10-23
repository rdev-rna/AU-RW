# #37 Baseline hygiene (area: assets-containers)

Branch `acu-37-baseline-hygiene`. Files: `tools/baseline.py`, `tools/test_baseline.py`,
`tools/acu.py` (inventory wiring).

## Done
`python tools/acu.py inventory` now writes a `baseline` section: the carrier list that may
form a coverage denominator, and every excluded top-level entry with a rule and reason.

- **Carrier rule (observation of the Rust enumerator):** a regular `*.forge` file at the top
  level or inside a `dlc_*` folder, matching `acu_assets::forge::discover`
  (`acu-rs/crates/acu_assets/src/forge.rs`). A directory with a `.forge` name is not a carrier.
- **Exclusion rules (implementation choice):** `mod-layer` (`ACUFixes*`, excluded by name and
  never opened), `non-original-copy` (`Backups/`, `Extracted/`, `*.rar|zip|7z`), `runtime-log`
  (`*.log`). Names match case-insensitively; symlinks are not followed.

## Observed on this install (2026-10-22)
- 63 baseline carriers (59 top level + 4 in `dlc_*`), matching the 63 archives in STATUS.md.
- `Backups/` holds one 3,770,744,832-byte `DataPC_ACU_Paris.forge`, the same size as the
  baseline archive. Size only; contents were not compared or hashed.
- `Extracted/` holds 24,703 files in two *directories* named `.forge`.

## Tests
`tools/test_baseline.py`, 9 tests on synthetic trees: carrier selection, rule/reason on every
exclusion, case-insensitive names, `.forge`-named directory rejected, same-size backup flagged,
mod folder never walked, missing directory fails, empty install, symlinked folder not followed.

## Unknown / not done
- A mod replacing a baseline carrier in place is not detectable from names or sizes.
- Whether the Rust catalog should also refuse a mismatching carrier list is out of scope.
- The unrelated top-level entries (`sounddata`, `videos`, `Support`, ...) are listed under
  `other_top_level`, neither included nor excluded.
