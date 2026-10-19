# ACU-RE-0017 — Rig assembly and controlled skinning

Date: 2026-10-19. Task: movement skeleton stage. State: bounded experiment validated;
native rig selection, solvers and animation remain unresolved.

Target install identity: ACU.exe SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
preferred image base `0x140000000`. Archive evidence only; no native VA/RVA or
export-to-install correspondence claim. Continues findings 0015 and 0016.

Bounded question: can the selected meshes' original palettes, all four/eight
weight slots and stored bind matrices drive visible deformation using the
referenced humanoid/additive hierarchies? Preserve exact payload identities,
reject unsupported schemas and distinguish assembly choices from native solvers.

The `acu-rig` diagnostic reads selected entries in memory and emits resource
indices, source table records, stored/payload SHA-256 and typed skeleton data or
an explicit error. Initial reports in `local/rigged-world-20261019/` cover
BelleEpoque334 and DataPC110/968/1006. The eleven references are from finding0016.
Five of six embedded auxiliary rigs parse already; the dress stops at
`c0dd28a9`. Pistols and other clothing stop at `4bf66801`.

Layout corroboration: previously reviewed AnvilToolkit.dll1.3.7.0 SHA-256
`940374775553172b06d55537c4db78048bc3206b9b2f72bcc61bacfdcdb57636`,
ilspycmd8.2.0.7535; local skeleton-re-20261019 HingeVectorBoneModifier,
HingeVectorTarget, BallJointBoneModifier and ConstraintBoneModifier classes.
Unity HingeVector stores four vector4s, four scalar ramps, then counted
`246559fc` targets (object header, bone pointer, radius). BallJoint stores i32
angle type, ten f32 scalars, i32 gravity mode, vector4 own gravity, three f32
offsets, file reference, orientation object pointer and ragdoll file reference.
Constraint `84240425` has targets and a boolean without an offset vector.
Reading inputs does not establish solver equations or runtime selection.

Further exact rejection `f9d25748` is SpringBoxModifier (class-name CRC lookup
followed by ilspycmd on the identified assembly). Its reviewed Read method stores
three f32 spring/damping/wind inputs, vector4 gravity and two vector4 box bounds.
The independent cursor then reaches payload end on all four formerly rejected
avatar variants. Full cursor report: `independent-rigs.json`; 73/74 selected
skeleton resources parse independently, with the Sword_Tag nonfinite value still
rejected. Rust still deliberately rejects the separate `f465cc0f` class in two
bootstrap rigs. The bootstrap Rust expectation is now 25/28; it is not the same
population as the 74-resource independent diagnostic.

The assembly prototype registers shared name-hash frames, preserving first-source
base hierarchy and original per-mesh bind matrices. Candidate clothing sources
are diagnostic choices, not recovered native row selection. An initial launch
failed registration because unrelated DataPC1006 props were included; restrict
that entry to the two referenced character rig IDs and DataPC968 to Pecks.
The corrected source population is 38 rigs / 721 distinct name hashes in the
independent diagnostic. One active shoulder mesh hash `e393c94d` remains absent
from all 74 investigated skeleton payloads, including the independently decoded
SpringBox variants. Preserve its actual mesh bind joint and weights, parentless
and frozen; record it rather than inventing an attachment.

The test pose is authored in mesh axes around stored bind pivots; all original
four/eight influence slots are evaluated on CPU, then positions, normals and
tangents are uploaded to Bevy. It does not execute original modifiers or clips.
This implements a controlled experiment, not native rig composition parity.

Acceptance: independent byte cursor agreement; bounds, truncation and schema
tests; all selected weighted palette entries resolved or explicitly unavailable;
numerical bind identity and independent one-bone rotation control; fresh visual
inspection of bind, posed and moving character. No broad asset completeness,
native movement, lighting or shader parity is implied.

The selected 19 draw parts / 29,481 vertices now retain all active influences;
maximum numerical bind reconstruction error is 7.173293852e-7. CPU skinning,
including normals and orthogonalized tangents, measured 2.626 ms mean over 60
frames in the interim v3 capture. This is not a steady frame-rate benchmark.

Normal shading experiment: the original MapNormal selector/TextureSet chain
resolves seven maps (six format98/BC7, one format71/BC1). Independent in-memory
DDS/Pillow diagnostics preserve the original pixel hashes and sample their RG
channels near the unit disk. `local/rigged-world-20261019/normal-channel-audits.json`
records identities and channel statistics. The shader interprets RG as tangent
XY and reconstructs positive Z with a clamped square root; original tangent
handedness is preserved. Channel signs and B/A meanings are not recovered native
math. Use Bevy PBR lighting, explicit roughness and a synthetic daylight rig.
Fresh bind/posed/moving captures were inspected. See shading_deltas.md for
outstanding original material/lighting work.

Final validation (2026-10-19): fmt check, clippy `-D warnings`, workspace tests
(51 authored / 16 install ignored), three explicit skeleton install tests and
workspace build pass. Logs: `local/rigged-world-20261019/{clippy-final,tests-final,
install-v5,build-final-retry}.log`. One intermediate build overlapped an active
capture and could not replace the Windows executable; stop that own capture,
rebuild successfully, then regenerate all final captures. No install mutation.

Executable SHA-256:
`77b308a50154a5f3102427123797dbf0b7d394b3e0d4a52627f0bff6a39a5d5e`.
Authored Rust/shader/Cargo source digest:
`8f84871c3133cf385f5bf7830f7689c8dc77d847df46a4461bb8f137dc069e34`.
Nine fresh outputs and exact launch/replay/report/asset hashes are recorded in
`local/captures/acu-arno-final-20261019/manifest.json`. Walk40 reaches z=-1.08;
run45 reaches z=-2.568334; jump105 y=0.775; profile-up100 y=0.751667,z=-6.693328;
profile-down100 remains grounded at z=-5.519999 against the block. These are
authored test scenarios, not native comparisons. CPU skin means across their
60-frame samples range 2.538–3.034 ms, not a steady frame-time benchmark.

The real DX12 window review logs scoped foreground keyboard events and captures
walk/run/jump/stop/reset/orbit/pitch/zoom/bind. Early arrow-key injection lacked
Windows' extended-key bit and did not orbit; the helper now sets that bit and
the virtual-key scan code before repeating the review. Controller combinations,
release, focus loss/disconnect and right-stick camera yaw/pitch are independently
tested using Bevy input state. Physical controller operation is not verified;
the host detects a pad with a fallback mapping warning. Authored gait foot
contact/sliding, original modifier/IK solvers, clips, native traversal/material
math, one shoulder parent and Tailored/Day Paris selection remain open.

Successful DX12 review: `local/captures/acu-arno-window-20261019-111754/review.json`
(reused own viewer PID7248 from the 111558 startup, exact command line recorded).
Reset/orbit/pitch/zoom/walk/jump/bind images inspected. A prior retry stopped on
focus loss before injecting input; the helper temporarily attaches the calling
input thread for foreground acquisition, detaches it immediately, and still
guards every keydown against the own HWND. Keyup cleanup always releases the
injected keys, including if focus changes during the review.
