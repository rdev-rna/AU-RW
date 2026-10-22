# Area: warping

Epic: #11 (label `area:warping`). Owner: whoever holds the open claim on this area's slices (docs/PROJECT_BOARD.md).
Edit this file only in the PR that lands this area's work.

- **Status:** started (done / partial / started / none)
- **Scope:** motion warping (maintainer claim: central to ACU movement; evidence pending)
- **Anchors and reuse (leads, not proof):** `acu_motion/src/target_warp.rs`

Evidence rules: string hits are not proof of mechanics (AGENTS.md).

## Evidence note (audit 2026-10-22)

`acu_motion/src/target_warp.rs` ports a translation-correction kernel from VA 0x140188f00 (controller +448 mode split). Its header cites ACU-RE-0031, which is actually the corpus-binding finding; no finding records this kernel. See the follow-up issue before relying on it.
