# Project status

Updated: 2026-10-19. Phase: runtime asset reader implemented; a Bevy
inspection scene renders textured Arno in a grey arena; comprehensive typed asset
parsing is active and incomplete. No playable rewrite yet.

## Current runtime decoder work

Publication follow-up (2026-10-19): user authorized the collaboration repository
under Rdev. Destination: [rdev-rna/AU-RW](https://github.com/rdev-rna/AU-RW).
A separate source-only checkout at `C:/Users/PC/source/AU-RW` uses fresh history
and repository-local Rdev author identity. The research parent repository is
unchanged. The export excludes game data, corpus, local configuration, captures,
build outputs and prior history. No project license is assigned. Runtime code
and the ACU-005 priorities below are unchanged; earlier publication-unset notes
describe historical checkpoints.

The project title is now **AU-RW** (user-selected). New collaborators can use
the [onboarding prompt](COLLABORATOR_PROMPT.md), linked from README,
CONTRIBUTING and MASTER_PROMPT. It explains task coordination, portable paths,
evidence, validation and handoffs; the obsolete ACU-001 first-session assignment
has been replaced. This documentation follow-up changes no runtime behavior,
Git location, remote or publication state. Runtime priorities below remain open.
Validation: workflow check and all ten Python tests pass. Refreshed local review
bundle: `local/au-rw-source-review-20261019-onboarding.zip` (94 authored files
plus manifest); ZIP integrity, onboarding inclusion and all file hashes verified.
No Rust gates rerun for this documentation-only follow-up.

Collaboration preparation (ACU-006) adds portable local configuration for the
game and decompiled corpus, an optional sibling reference, contributor setup
and an allowlisted source-only bundle with per-file hashes. This machine's
paths now live in ignored `local/project.json`. No nested repository, Git
staging, license, remote or publication has been created. See
[CONTRIBUTING.md](../CONTRIBUTING.md). Ten Python tests pass in the working
tree and clean exported snapshot; workflow check passes 21 required files.
Doctor and inventory pass (217,041 corpus files, 59 top-level Forge archives,
10 DLLs; executable hash unchanged). A clean export reports missing inputs
before setup and passes doctor with external paths and no sibling checkout.
Launcher syntax/configuration resolution pass. Final candidate:
`local/unity-source-review-final-20261019.zip`, 93 source/documentation files
plus SHA-256 manifest. No Rust/renderer change or fresh visual claim; prior
Rust gates remain historical. The decoder and remaining rig work are unchanged.

The numerical skin-palette slice now audits all eleven current Arno meshes:
29,481 vertices / 19 parts, every weight-byte sum 255, no weighted joint outside
its hardware palette and no palette index outside its mesh bone table.
Exact name hashes match 105 of 397 active mesh-bone entries to the male rig;
292 remain unresolved against that rig. Column-major stored mesh matrices
multiplied after skeleton globals give a consistent candidate inverse-bind
relationship within the sampled meshes, with blade/hair differences preserved.
See [ACU-RE-0015](findings/ACU-RE-0015-skin-palettes-bind-transform.md) and
`tools/skin_audit.py`. This is diagnostic archive evidence, not resolved additive
rig selection, runtime skinning, a new visible scene or animation playback.
Fresh gates for this slice: preparation check/doctor/inventory, fmt, clippy with
warnings denied, 40 authored Rust tests / 14 ignored install tests, workspace
build and six Python tests pass. The focused eleven-mesh original-install audit
also passes independently of the ignored tests; logs are `local/skin-*.log`.

The movement mission is now sequenced in [MOVEMENT_PLAN.md](MOVEMENT_PLAN.md)
and root PROGRESS.md. User constraints: Arno's Tailored hood/coat/gloves/greaves
(asset selection remains unresolved), and Day, Paris for the native lighting
target (current presentation remains synthetic). The requested video was reviewed
with claude-watch over 12:39-16:39; [LIGHTING_RESEARCH.md](LIGHTING_RESEARCH.md)
records scope and limits. [RENDERING_SOURCES.md](RENDERING_SOURCES.md) indexes the
user-designated SIGGRAPH2015 architecture reference, separating ACU from RedLynx.

The humanoid skeleton extension decodes 19/28 bootstrap skeletons, including the
full 109-bone male rig and 68-bone ACU_Base. Four modifier schemas and bounded
inline IK/COM/Reflex data now parse with exact payload/hash checks. Nine variants
remain explicit failures. See [ACU-RE-0014](findings/ACU-RE-0014-humanoid-skeleton.md).
This is data decoding, not solver execution, resolved skinning or animation.
Fresh validation for this extension: fmt and clippy pass; workspace tests pass
40 authored tests with14 install tests ignored. The explicitly run skeleton
install test also passes19/28 supported rigs with nine deliberate rejections.
The final workspace rebuild passes. Logs are `local/humanoid-*.log`; the explicit
install test was run from the freshly built skeleton test executable.

The latest requested visible slice is complete: eleven Arno mesh pieces, nineteen
draw parts, eight materials and seven resident diffuse textures load directly
from the installed Forge archives. All parts bind a material; the scene report
has no missing-reference or selector/set disagreement notes. Front, rear and side
captures were freshly inspected at `local/captures/acu-arno-textured-20261019/`.
The DirectX 12 window was also opened and visually inspected (`window-dx12.png`)
and left running. Vulkan offscreen rendering works, but its window swapchain
reports invalid layout/semaphore errors on this machine. The launcher selects
DX12 for the window and Vulkan for offscreen captures.

Run `./tools/run_arno.ps1 -GameDir <ACU-install-directory>` from this project.
Arrow keys orbit, Q/E zoom, Escape exits. `-Capture <PNG-path>` captures and exits;
`-NoTextures` restores the grey diagnostic model. This is a static inspection
scene with diffuse maps and alpha testing, not animation, skinning, original
template shaders or native material/lighting parity. See
[ACU-RE-0013](findings/ACU-RE-0013-material-texture-chain.md). The parser now
preserves the nonboolean reference scope byte found in the blade material.

Validation: preparation check/doctor/inventory, Cargo fmt, clippy (no warnings),
workspace tests and build pass. The material suite passes five authored tests
and two explicitly requested install tests (including the blade scope-4 vector).
Logs are `local/arno-textured-*-final.log`; capture identities are in the manifest
and `scene.json`. This validates the selected character chain, not all assets.

The user clarified direct reading and use of original assets at runtime, with
success requiring every asset to parse (models, maps, animations and textures).
This is active task ACU-005; see [coverage acceptance](ASSET_COVERAGE.md).
The implementation does not extract assets or depend on extracted directories.

`acu-rs/` now contains acu_assets with plaintext Forge reads, ACU LZO framing,
resource envelopes, archive controls, textures, meshes, simple skeletons and a
runtime catalog, plus probe/scan/mesh CLIs, and the acu_bevy inspection app.
Rust is pinned to installed 1.99.0 MSVC; Bevy 0.19.1 builds from the workspace
dependency set. See [ADR-0001](decisions/0001-decoder-workspace.md). Cargo.lock
is retained; the parent repository remains unstaged/uncommitted.

[ACU-RE-0002](findings/ACU-RE-0002-runtime-framing.md) records independent native
liblzo agreement for four base/patch frames and zero-seed Adler-32 checksums.
AnvilToolkit was inspected read-only as a format/validation reference and is not
a runtime dependency. [ACU-RE-0003](findings/ACU-RE-0003-texture-runtime.md) covers
borrowed pixel/mip ranges; [ACU-RE-0005](findings/ACU-RE-0005-texture-variants.md),
[ACU-RE-0007](findings/ACU-RE-0007-texture-no-inline-pixels.md) and
[ACU-RE-0008](findings/ACU-RE-0008-mesh-runtime.md) cover cube/volume/padded
textures, descriptor-only placeholders and the cluster-only mesh geometry variant
(empty non-clustered buffers validated against the compiled clustered index
buffer). [ACU-RE-0006](findings/ACU-RE-0006-prefetch-control.md) covers the
ID0x91 prefetch control, [ACU-RE-0010](findings/ACU-RE-0010-skeleton-subset.md)
the simple-skeleton slice, and
[ACU-RE-0011](findings/ACU-RE-0011-clustered-vertex-decode.md) the clustered
vertex attribute layouts, the quantization-scale position formula and the first
visible render: eleven mesh resources (nine FearlessAssassin battle-outfit
pieces, the head mesh and the hood-up hair plane) load straight from the
installed Forge archives into a grey synthetic Bevy arena at 1280x720 from three
orbit angles (`local/acu-arno-35.png`, `local/acu-arno-145.png`,
`local/acu-arno-260.png`; capture manifest and per-piece hashes in
`local/captures/acu-arno-20261018/capture.json`).
[ACU-RE-0012](findings/ACU-RE-0012-clustered-mesh-layout.md) then types the
compiled clustered block: 52-byte cluster descriptors (AABB, normal cone, index
span, vertex-group byte offset), 48-byte per-cluster bit masks and 36-byte draw
primitives (AABB, stride, packed flag/value, index start), with seven rules
validated on fifteen installed meshes. Raw clustered indices are
primitive-relative and their base is `vertex_offset_dwords * 4 / stride`;
`MeshData.min_index` disagrees on the bed by 240 vertices, so descriptors are
authoritative for cluster-only geometry. The corrected bases fixed the
multi-cluster renders: fresh captures of the Arno set at three angles plus the
desk (74 clusters), bed (64 clusters) and rock are in
`local/captures/acu-clusters-20261018/` with the run logs in
`local/logs/capture-clusters-*.log`; the previous Arno captures had a mangled
torso and detached arms where clusters were misplaced.
Decoded native bounds for the assembled set: min
`[-0.49482712, -0.18659016, -0.00024414808]`, max
`[0.49446088, 0.24909207, 1.8527176]`; the stored per-cluster boxes matched
decoded geometry within 1.0122 across all fifteen samples. The cluster bit mask,
glue/cluster-to-submesh mapping, native axes/units, triangle winding and
skinning remain unresolved, so this is an inspection slice, not world, material,
animation or gameplay parity.

Completed audit: 63 Forge archives including DLC, 180,245 reachable entries,
180,115 decoded/resource-indexed packages, 3,289,051 resource envelopes,
484 resource type hashes and 126 typed controls (63 global metadata + 63
prefetch indexes). Zero unparsed metadata extensions. Four Paris entries
(`00000023a0fbf0fb`, `00000023dab179da`, `0000002347f7952c`, `000000232002f58a`)
have no frame magic and remain explicit failures; Paris still declares four more
entries than its reachable table (24,681 vs 24,677). Report:
`local/coverage-typed-full2-20261018/summary.json`, SHA-256
`a17a954651125cfdca6a46f65dcc1f59263ad92be9fe5be31ef95ff4cc7d8130`;
scanner `target/release/acu-scan.exe` SHA-256
`4a0efdd26b01b9401240f817fb6aa9799e286edc0f13f7de9d21158a1be69d2d`.

Typed CPU parsing now covers 280,705 of 3,289,051 resource envelopes (8.5%):
TextureMap 65,466/65,466 (65,464 with validated inline pixel/mip spans, 2 typed
descriptors without inline pixels), Mesh 203,062/203,190 and Skeleton
12,177/19,036. Of the parsed meshes, 202,975 carry compiled cluster descriptors
that are now typed by ACU-RE-0012 but whose 48-byte-per-cluster bit mask is
still unresolved, so they are counted as partial, not complete typed coverage.
Remaining mesh failures: 71 invalid instancing bone/vertex counts,
49 primitive spans outside the clustered index buffer, 8 nonfinite scalars.
Remaining skeleton failures: 5,131 inline bone modifiers, 1,679 unsupported
inline/managed pointers, 49 pose groups. The largest unsupported families by
envelope count are LODSelector (~531k), Entity (~470k), Material (~443k),
TextureSet (~403k), Animation (~110k) and EntityGroup (~91k). The ACU-RE-0011
and ACU-RE-0012 inspection slices render decoded meshes; no texture-mapped,
animated or world rendering is claimed.

Cargo fmt, clippy (`-D warnings`), test and build gates pass from `acu-rs/`.
Forty-three authored tests cover record, frame, texture, mesh (including the
cluster-only variant and a synthetic clustered block with eleven malformed
mutations), vertex layouts and decode, skeleton, prefetch, control and catalog
boundaries; eleven install-dependent tests are explicitly ignored in the
standard suite and pass when run with `ACU_GAME_DIR`. The preparation workflow
checks pass. The full `--locked` workspace suite, the ignored install tests and
the ACU-RE-0012 cluster audits all passed on fresh runs; the acu_bevy capture
path was re-exercised after the decode change and rendered the corrected Arno
set, desk, bed and rock captures listed above.

## ACU-001 result

[ACU-RE-0001](findings/ACU-RE-0001-forge-compatibility.md) verifies the FH
container layout with decryption removed against DataPC, DataPC_patch_01,
DataPC_ACU_Paris and DataPC_extra_patch_01. The bounded Python probe
`tools/forge_probe.py` validated 26,322 reachable entry records and saved
structural range hashes and 12 stored-entry hashes in ignored
`local/forge-compatibility.json`. Three patch extracted files and one Paris
metadata file match stored bytes exactly. No compressed decode was attempted.

Paris declares 24,681 entries but the reachable table has 24,677; index coverage
is incomplete/uncertain. Compressed samples have a different frame magic from
FH's Oodle framing. Codec/provider selection and patch precedence remain unknown.
LZO1X is a candidate from related tooling, not established ACU evidence.

Validation: `python -m unittest discover -s tools -p test_forge_probe.py` passes
three structural fixture tests, covering malformed offsets, counts, cycles and
truncation. The four-archive probe, preparation check and doctor pass.
Inventory refresh passed: executable hash unchanged, 217,041 corpus files,
59 top-level Forge archives and 10 top-level DLLs. No Rust gates apply yet.

Standing authorization clarified by the user: execute ACU game code at the agent's
discretion, including live DLL hooks and comparisons, without per-action approval.
Reversible hook deployment/configuration changes are authorized. This supersedes
the earlier preparation-only runtime restriction and any contrary language in
historical handoffs; see `AGENTS.md` and `docs/WORKFLOW.md`.

## Established locally

- Project root: `E:/ReverseEngineering/Projects/Unity`.
- Git root: `E:/ReverseEngineering/Projects`; no project files were listed by
  `git ls-files -- .` at initial inspection. No new repository/remote/commit made.
- `ACU - Decompiled/` contains 217,041 `.c` files totaling 228,711,062 bytes.
  Sample headers include `// 0x140001000 sub_140001000 size=2621` (spacing normalized).
  This is address-named pseudocode; source tool/build identity is not established.
- Game `ACU.exe`, base/patch Forge archives and extracted folders exist under
  `D:/Games/Assassin's Creed Unity/`. Extracted folders include
  `DataPC_ACU_Paris.forge` and `DataPC_extra_patch_01.forge`.
- ACUFixes/plugin-loader and proxy DLLs are present. Runtime behavior may depend
  on mods; no game launch or DLL loading was performed during setup.
- For Honor's runtime `fh_assets/src/forge.rs` exists and was inspected. Three
  ACU archive prefixes contain plaintext `scimitar`; see `docs/FORGE_REUSE.md`.
- Installed tools observed: Python 3.12.10, Cargo/Rust 1.99.0, Git 2.46.0.windows.1,
  ripgrep 15.2.0. These are observations, not selected project version pins.

## Preparation validation

All three preparation commands passed on 2026-10-18:

```text
python tools/acu.py check
  PASS: 19 required files, JSON/configuration and local Markdown links
python tools/acu.py doctor --output local/doctor.json
  PASS: executable/corpus present; reference Forge reader found; tools reported
python tools/acu.py inventory --output local/inventory.json
  PASS: 217,041 corpus files; 59 top-level Forge archives; 10 top-level DLLs
```

Executable SHA-256:
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`.
Parsed PE metadata: AMD64 (`0x8664`), image base `0x140000000`, entry RVA
`0x7544b1d`, image size 125,980,672 bytes. The first sample export VA therefore
maps arithmetically to RVA `0x1000`; this alone does not verify matching code.

For Honor `fh-rs/crates/fh_assets/src/forge.rs` source SHA-256:
`3a96420b8554ef7787b8a5051eed05a440255bd56c4c5174cae8a283f784dae7`.
Reference source identities, archive prefixes and DLL identities are in ignored
`local/inventory.json`. Archive inventories use size/prefix, not full hashes.
The corpus metadata digest covers names/sizes, not every file's content.

Git ignore checks confirmed the corpus and local inventory are excluded and
`acu-rs/Cargo.lock` is not ignored. Nothing was staged or committed.
Additional smoke checks passed: report output outside `local/`/`outputs/` is
rejected; a missing install makes doctor fail; workflow checking works from a
different working directory; the Python tool compiles successfully.

## Unknown / not yet verified

- Export source executable/hash and correspondence with the current install.
- Archive table layout beyond four sampled archives, payload encryption/codecs,
  patch precedence and resource schemas; Paris count discrepancy.
- Full Unity asset compatibility; the implemented Rust plaintext/LZO path and
  first texture variant cover only the cases reported by current scans.
- General GPU compatibility; Vulkan offscreen and the DX12 window are visually
  validated here. Vulkan window presentation still fails validation.
- Clustered-mesh bit mask semantics, general cluster-to-submesh/bone mapping,
  `fixed_cluster_size` meaning and the bed's 240-vertex descriptor/`min_index`
  discrepancy; see ACU-RE-0012.
- Native gameplay constants, animation selection, root motion, collision and units.
- No playable rewrite or native game-behavior parity claim exists. The only
  visual artifact is a synthetic grey arena with textured decoded Arno, which claims no
  original-world, material, lighting or animation parity.

Next: ACU-005 in `docs/TASKS.md`: close remaining skeleton modifiers/pose groups
and mesh instancing/span/scalar failures, then the large untyped families
(LODSelector, Entity, Material, TextureSet, Animation, EntityGroup), then typed
world/map data; patch/reference resolution remains unresolved. ACU-004's display
gate passed with the Arno slice in ACU-RE-0011 and was re-validated after the
ACU-RE-0012 decode fix; ACU-003 is independently ready; no export/build
correspondence claim has been made.
