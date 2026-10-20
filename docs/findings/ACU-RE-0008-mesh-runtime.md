# ACU-RE-0008 — Embedded compiled mesh data

Date: 2026-10-18. Task: ACU-005. State: partially established.

## Provenance

Asset-byte evidence against installed ACU.exe identity SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
image base `0x140000000`. No native VA/RVA, export correspondence, runtime
execution or visual parity is established by this finding.

`DataPC.forge` entry 10, file ID `0000000000000028`, stored offset 946135,
length 97553, SHA-256
`a2f006ae6c30466a93474e239eb59927c29371748dfcbc8b1c2a13aa3551574d`.
Decoded resource frame length 500381, SHA-256
`51ddfd60e1f97ac48c27c0fff7b23d28491a65ff6bc0cdb6edc9c5fcda1cdf26`.
The archive identity is this exact stored range and hash, not a whole-file hash.
First resource name `UnitSphere`, Mesh type `415d9568`, payload frame range
23..6686, length 6663, SHA-256
`b1c644de6856ae51b975840e05410dbd02d669ae3bfe74e02fd623eaf1ecb490`.
Its embedded MeshData class occupies payload range 3263..6326, length 3063,
SHA-256 `bbaa28e3e98c4213ba962c7d43b617e80df70313dc74c6345819d41c61a3996a`.

Read-only reference AnvilToolkit.dll 1.3.7.0 SHA-256
`940374775553172b06d55537c4db78048bc3206b9b2f72bcc61bacfdcdb57636`,
REA provider `rea-dotnet-static`, identity evidence
`ev_01f1308aac08023bb993cc29cf977fa03e4a7076b1985e3d9d02ee25a0c7db1a`.
Focused ilspycmd 8.2.0.7535 decompilation:
`ilspycmd -t AnvilToolkit.FileTypes.AnvilNext.Models.MeshData C:/Users/PC/Desktop/ATK/AnvilToolkit.dll`;
likewise Mesh, CompiledMesh, MeshPrimitive, ClusteredMeshData, MeshInstancingData,
VertexFormatsMap, Vertex and Common.PackedS16Vector4/Common.PackedUV.
Saved outputs under ignored `local/reference-atk/`.
Relevant references: MeshData.cs lines 190–211; MeshPrimitive.cs lines 107–114;
VertexFormatsMap.cs Unity mapping beginning line 176; Vertex.cs lines 413–483.
These are third-party format observations; no implementation text was imported.

## Observations

An embedded MeshData class starts with little-endian u64 ID and u32 class hash
`0645abb5`, followed by boolean index32, u8 native vertex-format code and u8
stride. Two signed-i32-count arrays follow: standard and shadow primitives.
Each primitive contains u64 ID, class hash `a57387ef`, then six signed i32
fields: minimum index, depth-only flag, vertex count, start index, primitive
count, native topology. Negative sizes/counts fail. Then two signed i32 byte
lengths precede the vertex and index buffers. No outer Mesh wrapper is consumed.

The independent Python oracle uses native lzo1x_decompress_safe from
`C:/Users/PC/Desktop/ATK/Libs/lzo.dll`, SHA-256
`6407c266e7a954e58a3ffff0ea4303020dde3bd58997789d662a131bf1c9a7b9`;
the Rust runtime instead uses its pure Rust decoder. Retained oracle scripts:
`local/probe_mesh_child.py` and `local/mesh_vector_child.py`; result
`local/mesh-vector.json`. The bounded investigation read one installed entry;
no full-install rescan or extracted-directory dependency was added.

Both primitive arrays contain one triangle-list primitive: minimum index 0,
depth-only false, vertex count 100, start index 0, primitive count 160,
topology 1. Native format is 4 and stride is 20. Payload vertex range
3362..5362 has length 2000 and SHA-256
`187845cca84fddb48819887b152e8af497497c00c364a4402e476ebdfb1dcf3c`.
Index range 5366..6326 has length 960 and SHA-256
`d4065d81b704e7135fb3affe2ecae45ad8dd4171ad67f05952c7f633b3172a85`.
All 480 u16 indices fall within the 100-vertex buffer.

Format 4 exposes i16 position[3], i16 auxiliary position/color field,
four normal/color bytes, four tangent/color bytes, and i16 UV[2]. The first
vertex is position `[4533,3294,15396]`, auxiliary 2, normal/color
`[168,151,244,255]`, tangent/color `[182,12,130,255]`, UV `[1307,1986]`.
First 12 indices are `[0,1,2,3,1,0,2,4,0,3,0,5]`.

## Implementation and limits

`src/mesh.rs` provides composable `parse_data` and bounded
`parse_data_exact`, typed primitives, borrowed buffers, index accessors and a
checked format-4 packed-attribute accessor. The reader checks lengths, class
hashes, boolean values, count/allocation limits, buffer stride/alignment,
triangle-list index spans and trailing bytes. Primitive values and native
format codes remain explicit. Other vertex formats remain uninterpreted byte
streams and their packed-attribute accessor fails.

The composable root `mesh::parse(payload, expected_id, limits)` now follows
the serialized graph without searching for class hashes or using fixed offsets.
It exposes root descriptor/category/generated fields, local bounds, nine Unity
flags, dynamic counts, material references, and compiled platform/SDK/quantization
fields. MeshBone records expose name hashes, serialized 16-float matrices and
used-by-submeshes flags, without the reference tool's automatic matrix inversion.
Compiled instancing exposes submesh/material types, vertex count, material ID
and the hardware-to-mesh bone table, retaining the unused portion of its fixed
256-byte storage. Null/external submesh pointers are supported; inline submeshes
and other pointer variants fail explicitly. Unknown reference-global/tag variants
and trailing resource bytes also fail.

ClusteredMeshData version 1 exposes format/stride, center/half-extent, cluster
and draw counts, per-primitive cluster counts and vertex offsets, and the fixed
cluster-size flag. It borrows clustered vertex/index and descriptor/mask ranges.
The clustered descriptor/mask buffers remain opaque here; `Mesh::unresolved()`
reports their presence. *(Superseded in part by
[ACU-RE-0012](ACU-RE-0012-clustered-mesh-layout.md): the 52-byte cluster
descriptor and 36-byte draw-primitive records are now typed and validated; only
the 48-byte-per-cluster bit mask stays opaque.)* Nonempty compiled blobs, unknown
attribute formats, external submesh graphs and depth-only/unknown topologies are
also explicitly reported. **Root byte consumption is not complete typed Mesh
coverage.** Material resolution, native position scale/axes, triangle winding and
render interpretation remain unknown or unverified.
The observed topology 1 range check does not establish all native topology
or depth-only/clustered buffer semantics. This implementation preserves those
values rather than inferring draw behavior.

## Cluster-only geometry variant

`DataPC.forge` entry 110, file ID `0000000000000800` (stored offset 2438262,
stored length 48507518, stored SHA-256
`7d5f500049eeb22158005bbd74fb8c22b84b84501d226aa6bfc66c4b0aa2334f`;
the same package identity as ACU-RE-0010). Resource 2471,
ID `000000007ec1ea7d`, name `TPL_WP_Rock_LOD0`, resource type `415d9568`,
payload length 2215, SHA-256
`a5dc08f3b7f38809f56ae9aa56bfe6163e39eb5f525d8009b74444a5f14b29a4`.

The embedded `MeshData` for this resource declares native format 4, stride 20,
16-bit indices, one standard and one shadow primitive with vertex count 52 and
primitive count 72, and **zero-length** non-clustered vertex and index buffers.
The compiled clustered block carries the geometry instead: stride 20, one
cluster, one draw primitive, clusters-per-primitive `[1]`, vertex offset `[0]`,
fixed cluster size false, 1040 vertex bytes (52 x 20), 432 index bytes
(216 x 2 = 3 x 72 u16 indices), plus 52 descriptor bytes, a 48-byte bit mask
and 36 primitive-descriptor bytes. Vertex SHA-256
`b2fd30e28f1c0d5e43cb0306e5f31d79af7bb0200771c340dc3cddc59618c21c`
and index SHA-256
`225021946fbb6b8248ad85eaf92985d77b50a3d8ab48835fcd03fb25a513b76f`.
An earlier manual re-read of this payload was off by the two array-count u32
fields; the Rust reader's positions (cluster fields end at 147, buffers
1040/432/52/48/36, MeshData at 1775) are the corrected reading.

The previous parser rejected this family because it validated the primitive
triangle span against the empty non-clustered index buffer. The reader now
validates the span against the non-clustered buffer when it is non-empty, and
against the clustered index buffer (same index width) when the non-clustered
buffer is empty; both are real archive bytes of the same resource. The
correspondence of `MeshData` primitives to the clustered draw primitives and
the native draw call remain inferred, not observed. Cluster descriptor and
primitive-descriptor semantics were unresolved at this finding's date and are
typed in [ACU-RE-0012](ACU-RE-0012-clustered-mesh-layout.md); the bit mask is
still unresolved.

## Validation

From `acu-rs/`, `cargo test --locked -p acu_assets --test mesh` passes six
authored tests with three install-dependent tests ignored (the synthetic
clustered-block tests were added by ACU-RE-0012). The tests cover both
index widths, independently authored attributes, every truncation of the fixture,
bad class/count/boolean/stride values, out-of-bounds draw ranges, trailing bytes,
configured limits, access overflow and the cluster-only geometry variant
(span checked against clustered indices, too-short clustered buffer rejected).
The authored root fixture independently checks matrix field order, bounds,
resource identity, material ID, missing byte rejection at every boundary,
trailing bytes and nonfinite matrix values.

Set `ACU_GAME_DIR` and run
`cargo test --locked -p acu_assets --test mesh -- --ignored --nocapture`.
This passed against the direct installed Forge: UnitSphere
(stored/payload/class/buffer hashes plus independent expected
primitive/attribute/index values), the Arno head graph (117 bones, one inline
submesh, format 1 stride 40, cluster-only buffers) and the TPL_WP_Rock_LOD0
cluster-only vector above (identity, counts, buffer lengths and vertex/index
hashes). It requires the explicit install and reports missing configuration as
unavailable; no visual validation or complete model success is claimed.

## Root graph independent vector

The native-LZO/Python graph oracle is retained as `local/mesh_graph_child.py`.
For UnitSphere it independently locates each field by serialized lengths:
category 5; no submeshes or bones; compiled platform 28 / SDK 7;
quantization factor 2; one instancing record with shadow-caster true, submesh 0,
material-type 0, zero hardware bones, vertex count 100, material ID
`0000000000000033`. The root compiled-material array references the same ID.
Only keep-nonclustered-data (sixth of nine Unity flags) is true.

Clustered class payload range 77..3263: version 1, format 4, stride 20,
one cluster, one draw primitive, per-primitive counts `[1]`, vertex offsets `[0]`,
fixed-size false. Center `[0,2.9802322387695312e-8,0]`, half-extent
`[0.9848077893257141,0.936607837677002,1]`.
Clustered vertex payload range 151..2151 has length 2000 and SHA-256
`a0f5dbd897cf9bf4f53fd0aee54ba39e20bc957edb7475dafd770ad516dbfb9d`;
clustered index range 2155..3115 has length 960 and SHA-256
`bbd4acd72cd0ce451bafb482eda7d8057f39f0b9b24bee7ebff19f149d8a9894`.
Descriptor range 3119..3171, mask range 3175..3223 and primitive-descriptor
range 3227..3263 were uninterpreted at this finding's date; ACU-RE-0012 types
the descriptor and primitive-descriptor ranges. The installed Rust test verifies
the root fields, reference IDs, cluster arrays, independent clustered buffer
hashes and (since ACU-RE-0012) the typed descriptors and audit; it asserts
`unresolved() == ["cluster bit mask semantics"]`.
Additional reference locations: Mesh.cs lines 960–1080 and 1122–1127,
CompiledMesh.cs lines 591–626, ClusteredMeshData.cs lines 117–152,
MeshInstancingData.cs Unity branch lines 175–190,
MeshBone.cs Unity branch lines 79–105, ObjectPtr.cs lines 97–121,
Common.FileReference.cs lines 30–37. Static third-party evidence establishes
candidate serialization; the independent real-input checks establish the
declared case only. Inline submesh, skeletal character and additional cluster
variants require further independent real vectors before compatibility claims.

`cargo clippy --locked -p acu_assets --test mesh -- -D warnings` passes.
The explicit install test passes after adding the root graph decoder.
