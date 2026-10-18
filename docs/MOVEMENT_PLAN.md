# Unity player movement mission

The 2026-10-19 mission focuses this project on a standalone, playable Unity
movement reconstruction. Existing Rust 1.99.0 MSVC / Bevy 0.19.1 pins remain;
the template's game/engine/path placeholders resolve to this project and its
known local ACU install and pseudocode corpus. No second engine/project is needed.
The testing equipment preset is Arno's Tailored hood, coat, gloves and greaves;
retain modular piece identities rather than baking clothing into the body mesh.
The lighting-stage default target is Day, Paris; the requested video reference
and baked-lighting investigation are tracked in [lighting research](LIGHTING_RESEARCH.md).
Broader ACU-005 asset completeness remains tracked, without blocking a bounded
character movement slice on every unrelated resource family.

## Work order and gates

| Stage | Work | Exit evidence |
| --- | --- | --- |
| Recon | Code/symbol map, asset families, risk register and evidence identities | Verified locations separated from name/string candidates; exact next schema/behavior question |
| Skeleton and skinning | Hierarchy, transforms, names/hashes, palettes, weights, inverse bind data | Complete bounded parsing, invalid-input tests, independent vectors, inspected bind-pose rig and controlled deformation |
| Animation | Compression, interpolation, root channels, events, timing and native selection | Multiple clips decoded and played; numeric/visual original comparison; unresolved channels explicit |
| Core locomotion | Input, native state transitions, acceleration, turns, slopes, capsule, landing | Mechanics spec before code; deterministic input replay; side-by-side/numeric comparisons |
| Ledges and climbing | Probe geometry, target rules, alignment, shimmy/corners, vault/mantle selection | Scoped obstacle course, probe/state overlays, native comparison and failure-case tests |
| Shading | Original bytecode/pass/math, texture packing, render states and color space | Same pose/camera/light reference; diffs and explained deltas |
| Camera and polish | Native follow/collision/recenter; frame-step and slow motion; profiling | Matched scenarios, debug overlays, repeatable builds and documented limits |
| Optional matcher | Pose/trajectory features, search, filtering and blending | Separate togglable module; original selection remains the reference; A/B evidence |

The user's explicit priority is skeleton/animation → core locomotion → ledges/
climbing → shading → camera → optional motion matching. Preserve the existing
inspection camera and diffuse rendering while researching their native versions.

## Five principal risks

1. Humanoid modifiers, additive skeletons and local hardware bone remaps may
   prevent a correct rig even when the static mesh looks plausible.
2. Clip compression and root-motion/event ownership may differ from For Honor;
   that project is a read-only reference, never ACU evidence.
3. Native state/selection rules may be distributed across code and data graphs;
   no placeholder movement constants may be presented as recovered behavior.
4. Traversal can depend on authored metadata and animation-driven alignment;
   generic raycasts or a stock controller do not establish fidelity.
5. Shader equivalence requires original passes, lighting and matched captures;
   default Bevy PBR with diffuse maps is only current presentation.

## Evidence and delivery rules

Read original archives in memory through configurable ACU_GAME_DIR / --game-dir.
This preserves the user's earlier direct-runtime-loading requirement; conversion
tools/caches may be added when justified, but no extracted folder is required.
Proprietary data stays ignored under local/assets/outputs and outside source.
No proprietary bytecode, disassembly or decompiled source is committed.

Use docs/code_map.md and symbol_map.md, numbered findings with hashes/locations,
and per-mechanic specs under docs/mechanics before implementing mechanics. Record
approximations/inferences with confidence in docs/INFERRED.md. Record shader
comparisons in docs/shading_deltas.md. PROGRESS.md is the running milestone view;
docs/STATUS.md retains broader parser coverage and validation facts.

Use the sibling ForHonorRustRewrite for motion matcher, animation, shading and
lighting architecture/comparison ideas. Review/provenance any imported module;
do not modify it or transfer game-specific constants/codecs/axes silently.
Original ACU execution and reversible hooks remain authorized by AGENTS.md.
