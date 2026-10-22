Latest follow-up, 2026-10-19: the old idle was static; a verified animated
original idle now plays. Upper-leg vertices primarily use previously unevaluated
helper bones. Eight bind-relative constraints now deform those vertices. This
authored adapter awaits native solver comparison; see
[finding0020](docs/findings/ACU-RE-0020-idle-leg-helper-audit.md).

Seven fresh idle/walk/sprint captures were visually inspected in
`local/captures/acu-leg-idle-20261019/manifest.json`. Idle poses change and thighs
deform. Clothing remains stiff; foot IK and native solver parity are open.
Current gates pass: fmt, Clippy with warnings denied, 107 workspace tests
(19 explicitly ignored), workspace build, 22 Python tests, check and doctor.
Both explicit install regressions pass, including actual render-mesh thigh
deformation and a disabled-helper negative control. Executable SHA-256:
`40ded8de3629e875c504f50fb896922d4f1e1de478a60afca4d5ebc324ac362c`. [unidentified project artifact, 2026-10-19; not the installed game binary, the mapped image or the reconstructed PE - see ACU-RE-0031]

Animation audit records 9,682 attempts, 3,317 decoded animations, 6,365 explicit
failures and 261,388 track rows. Actor ownership remains unverified; this does
not establish all Arno locomotion. See [database](docs/ANIMATION_DATABASE.md).
The [native probe and injector](tools/native_probe/README.md) are built and
fixture-tested, with memory/image capture, verified execution probes, registers,
watches and module-qualified profiles. No semantic game offsets are enabled.
Original launch exits code 42 even without injection. An injected startup image
capture completed but exposes zero conventional unwind/RTTI candidates.
Gameplay bone/traversal/raycast comparison requires a functioning game session.
The [supplied reference review](docs/REFERENCE_LINKS_20261019.md) records game scope
and BuildTable/skeleton leads. Earlier validation below is historical.

Directional matcher follow-up, 2026-10-19: original lateral walk/sprint takes
now participate in in-profile trajectory search; actor facing no longer erases
the lateral request. FH pose/prediction objectives are reused with explicit
Unity features and authored steering. See docs/findings/ACU-RE-0019-directional-navigation.md.
Current gates: fmt, clippy -D warnings, 106 workspace tests (18 ignored), build,
20 Python tests, check and doctor pass. Explicit original-install directional
regression passes, including after the I/O follow-up. Eight directional captures
were inspected; both fresh latest I/O-build captures, traces and asset identities
match their pre-I/O counterparts byte-for-byte. Direct
package decoding now runs in Bevy IoTaskPool with async checkpoints every 32
blocks, preserving synchronous CLI validation. Startup still waits. FH cloth reference is
reviewed; Unity cloth bindings/solver compatibility remain open.

# Movement reconstruction progress

Updated 2026-10-19. Target: Assassin's Creed Unity, Rust 1.99.0 MSVC /
Bevy 0.19.1 (existing locked workspace). Mission and exit gates:
[movement plan](docs/MOVEMENT_PLAN.md).

Test character requirement: keep Arno in **Arno's Tailored** hood, coat, gloves
and greaves. Resolve the game's modular equipment references before assigning
that name to the existing FearlessAssassin assembly; no silent outfit substitution.

## Done

- Original navigation track playback and a Unity matcher adapter now replace the
  procedural gait in the viewer. Low profile selects walking; held high profile
  selects sprinting, with original clip root displacement and local pose blending.
  Finding0018 records mappings/codecs and the external pelvis-frame correction.
  This is a bounded locomotion slice; original climbing/vaults, native selection
  graphs, foot IK and Tailored equipment remain open.

- Animation metadata catalogue: 110,447 occurrences / 37,803 distinct IDs from
  the full existing scan, with original byte identities and role hints; actor
  ownership stays unknown pending decoded references. Six catalogue tests pass;
  see [database commands and scope](docs/ANIMATION_DATABASE.md). The reviewed
  For Honor matcher core is imported with 44 passing tests. Its primary objective
  is now used by the Unity locomotion adapter; broader live-dispatcher adaptation
  remains active under ACU-008.

- Rig-validation source (finding0017): original complete skin weights and bind
  matrices, 38 diagnostic candidate hierarchies, authored fixed tick movement,
  synthetic lighting/shadows and inferred packed-normal PBR shading. Controller
  bindings prioritize RT/R2 high profile, +X up, +Circle down and right-stick
  yaw/pitch. ACU-007 passes 51 authored tests, three selected original-rig tests,
  fmt/clippy/build, fresh replay captures and a live DX12 keyboard/camera review.
  Physical controller use remains unverified; authored gait has foot sliding.
  Native clips,
  traversal, modifier solvers and one weighted shoulder attachment remain open.

- Referenced rig discovery (finding0016): ten BuildTables decoded, eleven
  skeleton links identified, specific head rig decoded (134 bones / 106 wrinkle
  inputs / 147 packed poses). Bootstrap support reaches 20/28 rigs. Head hash
  matches improve to 110/116; no composed skinning claim.
- Neutral GitHub landing page and description; source identities remain in
  research records. Feature branches carry work until validated milestone merges.

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
