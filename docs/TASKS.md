# Task queue

Statuses: ready, active, blocked (with reason), done. Keep one bounded task active
per worker. No delegation is required by this workflow.

## ACU-006 — Prepare collaboration and a source-only publication candidate

Publication follow-up (2026-10-19): user authorized hosting under Rdev.
Publish the reviewed source-only snapshot to
[rdev-rna/AU-RW](https://github.com/rdev-rna/AU-RW) from the independent
`C:/Users/PC/source/AU-RW` checkout with fresh history and Rdev authorship.
Acceptance: exported workflow/Python checks pass, committed paths match the
source manifest, remote main matches the local commit and the repository link
is accessible. No game/corpus/local data or parent history; no new license.
Earlier preparation-only and publication-unset notes below are historical.

Follow-up (2026-10-19): user selected **AU-RW** as the project/repository title.
README and contributor entry points use that name. The portable
[onboarding prompt](COLLABORATOR_PROMPT.md) covers context, local setup, task
coordination, evidence, implementation, validation and completion. The master
prompt no longer directs newcomers to restart completed ACU-001. Acceptance:
linked and bundled onboarding instructions; workflow check and authored Python
suite pass. Hosting destination, license and publication remain unset.

Status: done (2026-10-19), bounded preparation only. User-directed side slice
before resuming ACU-005's rig/reference work. No public upload authorized or made.

Validation: ten Python tests pass in both working tree and clean source export;
workflow check passes 21 required files. Doctor/inventory pass against this
machine's ignored configuration (217,041 corpus files, 59 top-level archives,
10 DLLs; executable hash unchanged). The clean export correctly reports absent
inputs, then doctor passes with external paths and no sibling checkout.
PowerShell launcher syntax and configuration resolution pass; no scene or Rust
code changed. The final local candidate is
`local/unity-source-review-final-20261019.zip` (93 source files plus manifest).

Acceptance:

- Collaborators configure their own game and decompiled corpus directories;
  no machine-specific defaults or sibling checkout requirement.
- The scene launcher resolves local configuration as well as explicit overrides.
- Contributor instructions describe build/evidence gates and excluded game data.
- Produce a reviewable source-only ZIP with file hashes, excluding decompiled
  files, proprietary data, machine configuration, build products and history.
- Verify configuration precedence and export boundaries with authored tests,
  and preparation commands against the current machine's ignored settings.
- Keep the parent repository unchanged; defer host/repository/license selection
  and publication until the owner approves the concrete candidate.

## ACU-000 — Prepare the workspace

Status: done. Instructions, master prompt, workflow, templates, inspection tools
and initial inventory are present. See `docs/STATUS.md` for validation results.

## ACU-001 — Establish Forge compatibility

Status: done (bounded compatibility investigation, 2026-10-18). Depends on ACU-000.
See [ACU-RE-0001](findings/ACU-RE-0001-forge-compatibility.md).
Plaintext container reuse validated for four archives; compressed codec/provider
selection remains an explicit implementation boundary. No decoded-read support claimed.

Acceptance:

- Identify representative ACU base/patch archive byte ranges and preserve hashes.
- Compare container, tables, entries, encryption and compression with the For
  Honor runtime reader; mark supported, different and unknown separately.
- Establish at least one entry boundary from a base archive and one from a patch
  without trusting arbitrary counts/offsets. Relate an extracted payload if possible.
- Record required codec/provider and reuse source provenance; do not claim decode
  support before an independently checked payload succeeds.
- Produce a finding and a bounded implementation plan, including unresolved
  blockers and parser limits. No full-install extraction is required.

## ACU-002 — Bootstrap Rust and the bounded reader

Status: done for the runtime asset foundation (2026-10-18). Workspace and acu_assets
exist; base/patch vectors independently checked, bounded in-memory decompression,
CLI and format tests implemented. User prioritizes comprehensive runtime decoding;
empty app/simulation crates are deferred. See ACU-RE-0002 and ADR-0001.

Acceptance:

- Verify official current Rust/Bevy compatibility; record pins, toolchain and lockfile.
- Adapt reviewed parser modules into this project with provenance and explicit variants.
- Add a CLI probe that lists entries and reads a specified entry with hashes;
  no hardcoded install path, no install mutation and no bulk decoding by default.
- Prove base/patch reads with identified real samples; cover invalid input and
  allocation/cycle limits with authored fixtures. Document unsupported codecs.
- Pass Cargo gates; document one repeatable probe command and its actual output.

## ACU-003 — Bind the pseudocode export to the native build

Status: ready; can be investigated independently of archive decoding.

Acceptance: identify the export source/build if available; bind an ACU analysis
session by executable hash; verify several exported function locations/signatures
against native bytes; document image base, VA/RVA mapping and gaps. Do not rename
or regenerate the original corpus. If its source build cannot be established,
retain the mismatch/unknown status and limit behavioral claims accordingly.

## ACU-004 — Display one identified asset

Textured extension complete (2026-10-19): eleven total pieces (nine outfit,
head and hood-up hair), nineteen draw parts, eight materials, seven diffuse
textures. Three offscreen angles and a live DX12 window were inspected; no
unbound part or missing texture was reported. `tools/run_arno.ps1` launches the
synthetic grey world with orbit/zoom controls. See ACU-RE-0013 and
`local/captures/acu-arno-textured-20261019/capture.json`. This supersedes the
older grey-only material gap below; skinning, animation and native shaders stay
open.

Status: done (2026-10-18) for the first Arno character slice, re-validated after
the ACU-RE-0012 clustered-descriptor decode fix. The acu_bevy
inspection app loads eleven meshes (nine outfit, head and hair) directly from
the installed `DataPC_ACU_LGS_BelleEpoque.forge`, prints per-piece provenance
(archive, entry, resource, file ID, entry and payload SHA-256, format/stride,
vertex/triangle counts) and renders a grey synthetic arena at 1280x720 from
three orbit angles (`local/captures/acu-clusters-20261018/acu-arno-35.png`,
`...-145.png`, `...-260.png`; the pre-fix captures are `local/acu-arno-35.png`
etc.). See [ACU-RE-0011](findings/ACU-RE-0011-clustered-vertex-decode.md) and
[ACU-RE-0012](findings/ACU-RE-0012-clustered-mesh-layout.md). The scene
claims no original-world, material, lighting, animation or axis parity; winding
is unverified, so meshes render double-sided.

Acceptance: identify a bounded mesh/material resource chain, document transforms
and display it in a repeatable Bevy inspection scene; preserve source IDs, hashes
and decoder provenance; save and inspect a fresh capture. The mesh, material and
diffuse chain and the explicit presentation transform are now documented and
captured by ACU-RE-0013. General native transform/material parity remains open
under ACU-005.

## ACU-005 — Complete runtime asset parsing

Status: active. User success criterion: every asset parses, including models,
maps, animations and textures, for runtime use directly from the original install.
See [asset coverage acceptance](ASSET_COVERAGE.md). This objective is not achieved.

Current slice: in-memory container/frame/resource readers; typed controls (63 global
metadata + 63 prefetch indexes), all 65,466 TextureMap records (65,464 with inline
pixels, 2 typed descriptors without pixels), meshes including the cluster-only
geometry variant, the clustered vertex attribute decode and the clustered
descriptor/draw-primitive layout (ACU-RE-0012), simple skeletons and the selected
Arno Material/TextureSet chain (ACU-RE-0013). No complete material census yet.
The humanoid extension (ACU-RE-0014) now parses 19/28 bootstrap rigs, including
the full male/base skeletons; nine remain rejected. Bind transforms, skin remaps,
pose groups, additional modifiers and animation codecs remain active work.
The movement mission is sequenced in MOVEMENT_PLAN.md / PROGRESS.md. Required
defaults are Arno's Tailored hood/coat/gloves/greaves and Day, Paris lighting;
their native asset selection remains unresolved. LIGHTING_RESEARCH.md and
RENDERING_SOURCES.md track the reviewed video and developer rendering reference.

Completed bounded numerical skin audit (ACU-RE-0015): eleven pieces, 29,481
vertices, 19 parts; all weighted hardware-palette accesses and weight sums pass.
105/397 active mesh-bone entries match the male rig; 292 stay unresolved. Four
matrix storage/order hypotheses support a candidate inverse-bind relationship
for the matched bones; blade/hair differences are retained. Acceptance for this
slice: reproducible reports with byte identities, explicit unknowns, independent
matrix controls and invalid-input tests. No skinning or visible-rig gate passed.
Next movement slice: asset-referenced additive rig composition, inspected
bind-pose hierarchy and controlled deformation before animation playback.

No asset extraction pipeline; no extracted folder or AnvilToolkit
dependency in the runtime library.

Next: close remaining package/control-entry variants and metadata extensions,
then expand typed textures/external mips, mesh/skeleton, world and animation readers
using the coverage report to choose representative variants. Preserve all failures.
Cluster bit-mask semantics, general cluster-to-submesh/bone mapping and native
axes/units stay explicit unknowns. The inspected Arno instancing records now bind
materials by matched draw/vertex counts and UV0 gives a coherent diffuse render;
this does not establish universal mesh/material or texture conventions.

Acceptance:

- Enumerate original Forge archives including DLC; record additional asset carriers.
- Resolve all container/control-entry, framing and resource metadata variants.
- Implement typed models/skeletons, world/maps, animations, textures and every
  remaining resource type; resolve references and required patches explicitly.
- Validate real install inputs with independent vectors and malformed-input tests.
- Complete full scans with zero unknown types, failures, omissions or opaque
  placeholders counted as parsed. Decompression alone does not satisfy this gate.
- Prove assets are usable through the runtime API and subsequently rendering/playback.

The ACU-RE-0011 inspection slice satisfies the first half of the last bullet
(decoded meshes are usable through the runtime API and render), and ACU-RE-0012
extends it to the typed cluster descriptors, the primitive-relative index bases
and the desk/bed/rock renders. ACU-RE-0013 adds diffuse texture presentation for
Arno; animation and native material playback remain open.
