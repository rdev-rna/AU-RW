# Finding index

Corpus identity: [ACU-RE-0031](findings/ACU-RE-0031-corpus-binary-identity.md) binds the
mapped runtime image and the function export to installed `ACU.exe` by SHA-256, PE
headers, `.rsrc` and `.tls`, and resolves the export against the binary's own unwind
table (0.996712 of 263,663 starts). Code identity is unavailable: the installed file is
packed, so `.text`/`.rdata`/`.data`/`.pdata` have RawSize 0 and no code byte was
compared. The verdict is `bound_without_code_comparison`; 867 absent starts and three
patched `.UBX1` pages remain unexplained.

Latest pose-stage evidence: [ACU-RE-0028](findings/ACU-RE-0028-model-pose-bone-modifiers.md)
identifies the traced slot-2 array as the model-space pose written by skeleton
bone modifiers (MaxLookAt, Orientation/PositionConstraint, Hinge), recovers
the weighted set and interpolation, and live-gates the MaxLookAt,
PositionConstraint and OrientationConstraint ports per apply (worst 3.96e-5 deg).

Latest movement evidence: [ACU-RE-0027](findings/ACU-RE-0027-native-binding-mode-pose-conformance.md)
confirms native clip-binding modes against runtime bytes and compares a flagged
21-joint transition pose; full graph, root and codec-7 avoidance coverage remain open.

Movement follow-up: [ACU-RE-0026](findings/ACU-RE-0026-graph-clip-binding-coverage.md)
records eleven decoded observed clips, codec-7 avoidance failure, rejected
direct-time blend coverage, and candidate native source/target binding modes.

Latest: [ACU-RE-0025](findings/ACU-RE-0025-player-root-full-rig.md) links a repeated
movement-responsive root through native component fields to a 421-joint rig;
full root/pose replays and focused runtime-image IDA evidence are preserved.
Native clip selection and pose parity remain open.

Latest pose discovery: [ACU-RE-0024](findings/ACU-RE-0024-pose-signature-rediscovery.md)
reacquires changing skeletal buffers from exact captured topology bytes without
hooks. Current pose data is established; player identity and root ownership
remain open.

Latest follow-up: [ACU-RE-0021 — virtual controller enters world and XInput polling is observed](findings/ACU-RE-0021-live-controller-capture.md).

The live sweep proves menu entry and records XInput polling calls; action
bindings, returned controller state and gameplay semantics remain unverified.
The accompanying category launcher records capture requests, with only input
currently mapped to a verified native hook.

Navigation tracks: [ACU-RE-0018](findings/ACU-RE-0018-animation-tracks.md)
records original clip mappings, time/value codecs and external skeleton root
placement. The walk/sprint/idle adapter uses the imported primary pose objective;
events, native selection and traversal remain incomplete.
Codecs 3/7/8: [ACU-RE-0061](findings/ACU-RE-0061-animation-codecs-3-7-8.md) reads the remaining
quaternion key decoders from the binary and fixes codec 11's scale (0.001). Entry 928 now fully
decodes 8,071 of 8,103 clips, and 586 of the 590 played clips. It is cross-checked against a
third-party export of 92 clips: 183,911 keys, max error 6.7e-7.

Use `docs/templates/FINDING.md`; store durable findings under `docs/findings/`.
Link bulky raw evidence in ignored `outputs/` with identity and reproduction steps.

| ID | Question / evidence | State |
| --- | --- | --- |
| PREP-001 | Corpus layout and local inputs: `STATUS.md`, reproducible via `tools/acu.py inventory` | Observed; export/build correspondence unknown |
| PREP-002 | ACU prefixes versus For Honor reader: `FORGE_REUSE.md` | Prefix/source observations only; compatibility unknown |
| ACU-RE-0001 | [Plaintext Forge container compatibility](findings/ACU-RE-0001-forge-compatibility.md) | Four container tables verified; compressed decoder/provider and patch order unresolved |
| ACU-RE-0002 | [Runtime frame decoding](findings/ACU-RE-0002-runtime-framing.md) | LZO frames, zero-seed checksums, base/patch native-library oracle agreement; remaining package variants explicit |
| ACU-RE-0003 | [Runtime TextureMap](findings/ACU-RE-0003-texture-runtime.md) | Inline 2D texture variant with checked pixel/mip ranges; external/other variants unsupported |
| ACU-RE-0004 | [Global archive metadata and control dispatch](findings/ACU-RE-0004-archive-controls.md) | ID0x10 metadata typed; other control IDs explicit |
| ACU-RE-0005 | [Inline texture dimensions, cube faces and volume mip spans](findings/ACU-RE-0005-texture-variants.md) | Cube/volume/padded inline variants; descriptor-only variants explicit |
| ACU-RE-0006 | [ACU prefetch control index](findings/ACU-RE-0006-prefetch-control.md) | ID0x91 prefetch typed and validated against a native census |
| ACU-RE-0007 | [Texture resources without inline pixels](findings/ACU-RE-0007-texture-no-inline-pixels.md) | Two typed descriptors count as parsed but have no uploadable pixels |
| ACU-RE-0008 | [Embedded compiled mesh data](findings/ACU-RE-0008-mesh-runtime.md) | Mesh graph, MeshData and cluster-only geometry typed; cluster descriptor typing extended in ACU-RE-0012, bit-mask semantics unresolved |
| ACU-RE-0010 | [Complete simple skeletons and embedded bones](findings/ACU-RE-0010-skeleton-subset.md) | Simple-skeleton slice validated; modifiers/pose groups unresolved |
| ACU-RE-0011 | [Clustered vertex attribute decode and the first character render](findings/ACU-RE-0011-clustered-vertex-decode.md) | Vertex layouts and position scale validated against stored cluster boxes; Arno renders in a synthetic grey arena; axes/winding/skinning unresolved |
| ACU-RE-0012 | [Clustered mesh descriptor and draw-primitive layout](findings/ACU-RE-0012-clustered-mesh-layout.md) | 52 B cluster / 36 B draw-primitive records typed and validated on 15 meshes; descriptor vertex bases correct a stale `min_index`; bit-mask semantics still unresolved |
| ACU-RE-0013 | [Material and diffuse texture chain](findings/ACU-RE-0013-material-texture-chain.md) | Typed materials/sets/selectors, nonboolean reference scope; nineteen Arno parts bind eight materials and seven diffuse textures in an inspected grey scene |

Humanoid extension: [ACU-RE-0014](findings/ACU-RE-0014-humanoid-skeleton.md)
adds complete male/base payload reads and four modifier schemas; runtime solver
and skinning mapping remain unresolved.

Use `ACU-RE-0001` onward for implementation research. Findings should narrow
uncertainty, not just accumulate search dumps.

Skin audit: [ACU-RE-0015](findings/ACU-RE-0015-skin-palettes-bind-transform.md)
checks nineteen hardware palettes / 29,481 vertices and compares stored mesh
matrices to skeleton globals. 105/397 active bone entries match the male rig;
additive selection and runtime deformation remain unresolved.

Rig references: [ACU-RE-0016](findings/ACU-RE-0016-build-table-rig-references.md)
decodes ten selected BuildTables, identifies eleven skeleton links and the
specific 134-bone Arno head rig, and adds wrinkle modifier input decoding.
The pose-group storage now decodes as bounded raw i16 channels (no unpack/playback).

Controlled rig: [ACU-RE-0017](findings/ACU-RE-0017-composed-rig-skinning.md)
adds typed HingeVector/BallJoint/SpringBox inputs and composed diagnostic skinning.
All original weight slots are preserved; one active shoulder parent is unavailable.
Authored movement and inferred normal shading do not establish native parity.
110/116 active head bones match that rig; composition and deformation stay open.

Original navigation: [ACU-RE-0018](findings/ACU-RE-0018-animation-tracks.md)
decodes original tracks and the external skeleton root frame.
Directional selection: [ACU-RE-0019](findings/ACU-RE-0019-directional-navigation.md)
records original lateral roots, fixes the forward-only adapter and documents
the FH trajectory/steering and cloth reference boundaries.

Parkour selection layouts: [ACU-RE-0041](findings/ACU-RE-0041-acufixes-layout-spot-checks.md)
re-derives ACUFixes' candidate selection loop, fancy-vtable layout and action
fitness/distance offsets against this build; see [the review](ACUFIXES_REFERENCE.md).
Live parkour selection: [ACU-RE-0042](findings/ACU-RE-0042-parkour-candidate-trace.md)
records the game's candidate lists, ranking and chosen action during a scripted run.
Parkour action classes: [ACU-RE-0043](findings/ACU-RE-0043-parkour-action-classes.md) maps the
factory type numbers to constructors, vtables and fancy tables, cross-checked against live traces.
Player state: [ACU-RE-0044](findings/ACU-RE-0044-player-state-and-repeatable-start.md) reads the
player entity and transform live and gives captures a repeatable start.
Decisions and clips: [ACU-RE-0045](findings/ACU-RE-0045-decision-clips-and-window-sweep.md) pairs
parkour decisions with the clips that follow and records the first staged sweep (window entry).
Teleport and vault scan: [ACU-RE-0046](findings/ACU-RE-0046-player-teleport-and-vault-scan.md)
moves the player live (entity matrix + Human behaviour reset; the debug menu teleport is absent
in retail) and scans for spots where vault clips start.
Offline placements: [ACU-RE-0047](findings/ACU-RE-0047-paris-placements-offline.md) decodes every
Paris and Versailles cell placement (397,205 Paris placements, transforms, direct mesh references,
prop types sized from LOD0) and matches live Versailles positions to under 1 mm.
