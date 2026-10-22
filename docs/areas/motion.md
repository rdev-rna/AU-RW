# Area: motion

Epic: #10 (label `area:motion`). Owner: whoever holds the open claim on this area's slices (docs/PROJECT_BOARD.md).
Edit this file only in the PR that lands this area's work.

- **Status:** partial (done / partial / started / none)
- **Scope:** native locomotion/animation selection (recover, do not assume), reversals, root-motion ownership, codec-7
- **Anchors and reuse (leads, not proof):** PLAN_MOTION_PARITY (stale, see note); ANIMATION_SOURCES.md; ACU-RE-0029

Evidence rules: string hits are not proof of mechanics (AGENTS.md).

## Maintainer note (2026-10-22)

ACU does not use For Honor-style motion matching. The `amo*` and `loco_match` modules in
`acu_motion` are For Honor-derived viewer scaffolding, not recovered ACU behaviour. Do not extend
them; recover the native selection mechanism instead (issue #99). Public sources: `ANIMATION_SOURCES.md`.
