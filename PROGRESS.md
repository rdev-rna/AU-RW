# Movement reconstruction progress

Updated 2026-10-19. Target: Assassin's Creed Unity, Rust 1.99.0 MSVC /
Bevy 0.19.1 (existing locked workspace). Mission and exit gates:
[movement plan](docs/MOVEMENT_PLAN.md).

Test character requirement: keep Arno in **Arno's Tailored** hood, coat, gloves
and greaves. Resolve the game's modular equipment references before assigning
that name to the existing FearlessAssassin assembly; no silent outfit substitution.

## Done

- Collaboration repository published under Rdev at
  [rdev-rna/AU-RW](https://github.com/rdev-rna/AU-RW), using a separate
  source-only checkout and fresh history. No project license assigned;
  the earlier preparation-only notes below are historical.

- Project title selected: AU-RW. Added a portable collaborator onboarding prompt
  linked from README, CONTRIBUTING and MASTER_PROMPT; no hosting/publication
  change. See docs/COLLABORATOR_PROMPT.md.

- Collaboration setup (ACU-006): configurable game/corpus paths, optional reuse
  checkout, contributor guide and local source-only export tooling. No public
  repository hosting or license selected. Movement and rig-selection priorities remain
  unchanged; see CONTRIBUTING.md and docs/STATUS.md for validation.

- Forge/LZO runtime readers and selected typed texture/mesh/material readers.
- Textured static Arno in a synthetic grey arena: eleven meshes, nineteen draw
  parts, eight materials, seven diffuse textures; fresh three-angle and DX12
  window inspection. See findings0011–0013 and docs/STATUS.md.
- Bounded simple skeleton reader and independent native-LZO vectors (finding0010).
- Full 109-bone male and 68-bone base skeleton decoding with four modifier
  schemas, typed IK/COM and the observed Reflex subset (finding0014). Explicit
  install test passes 19 of 28 bootstrap skeletons; nine remain rejected.
- Numerical hierarchy check: stored xyzw Hamilton parent-times-local composition
  matches all 292 internal links across male, base and generic head diagnostic
  rigs (max position error2.465e-7); mesh bind transforms remain unresolved.
- Numerical skin audit (finding0015): 29,481 vertices / 19 parts have valid
  hardware palettes and weight sums of 255. Male-rig hashes match 105/397 active
  mesh-bone entries; column-major matrices support a candidate inverse-bind
  relationship. Additive rigs, 292 entries and controlled deformation remain open.

## In progress

- Phase 0 evidence/code map; export-to-installed-binary correspondence unverified.
- Phase 1 humanoid skeleton modifiers, hierarchy and mesh bone remaps, followed
  by compressed animation decoding and original clip-selection reconstruction.
- Resolve Arno's Tailored four-slot equipment preset and make it the default.

## Blocked / unknown

- Additional modifier classes, pose groups and solver variants remain unsupported;
  the male base rig now parses, but bind transforms and bone remaps are unverified.
- Animation codec, root-motion ownership, native movement states/constants,
  collision and traversal probes have not been recovered.
- Native shader/material parity and original-world geometry are not established.
- Lighting target: Day, Paris. Review the user's video with claude-watch and
  trace baked GI/lighting and environment capture selection (docs/LIGHTING_RESEARCH.md).
  Video review and developer-reference indexing are complete; native lighting
  selection remains open. Ubisoft2025 historical slides confirm baked irradiance
  volumes/four time-of-day keyframes for Unity.

## Next

Latest gates: preparation check/doctor/inventory, Cargo fmt/clippy,40 workspace
tests and build pass;14 install tests remain ignored by default. Six Python
tests and the focused eleven-mesh original-install skin audit pass. The earlier
explicit skeleton install test also passed. No visible scene change this slice.

Resolve the selected additive rigs and their roots, then inspect bind-pose
hierarchy against Arno and validate controlled deformation. Retain all
unknowns; static rendering is not playable movement or full asset support.
