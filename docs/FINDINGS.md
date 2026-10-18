# Finding index

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
