# ACU-RE-0011 — Clustered vertex attribute decode and the first character render

Date: 2026-10-18. Task: ACU-005. State: partially established.

## Provenance

Asset-byte evidence against installed ACU.exe identity SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
image base `0x140000000`. No native VA/RVA, export correspondence, runtime
execution of game code or original-engine parity is established by this finding;
all decoded values come from archive bytes plus a third-party format reader.

Read-only reference AnvilToolkit.dll 1.3.7.0 SHA-256
`940374775553172b06d55537c4db78048bc3206b9b2f72bcc61bacfdcdb57636`
(the same reference build as ACU-RE-0008). Focused ilspycmd 8.2.0.7535
decompilation:

```
ilspycmd -t AnvilToolkit.FileTypes.AnvilNext.Models.Vertex -o local/reference-atk C:/Users/PC/Desktop/ATK/AnvilToolkit.dll
ilspycmd -t AnvilToolkit.FileTypes.AnvilNext.Models.VertexFormatsMap -o local/reference-atk ...
ilspycmd -t AnvilToolkit.Common.PackedS16Vector4 -o local/reference-atk ...
ilspycmd -t AnvilToolkit.Common.PackedUV -o local/reference-atk ...
ilspycmd -t AnvilToolkit.Common.PackedU8Vector4 -o local/reference-atk ...
ilspycmd -t AnvilToolkit.Common.PackedRGBA -o local/reference-atk ...
ilspycmd -t AnvilToolkit.Common.Vector.PackedJoints -o local/reference-atk ...
```

Retained decompiled sources with their decoded-text SHA-256:
`Vertex.cs` (1047 lines, `c92ae5cfd1c4f81770c63c604d33e911046751d94d43ce14923dee67c43f1fc9`),
`VertexFormatsMap.cs` (`e41239faba9aafd8e34a8e12109a74656f5cbd69312a24f0ab5bafe06c7e26a7`),
`PackedS16Vector4.cs` (`5f4b354625713d5d00372c81fa53994d14766325210a3539df3fa6968b62e412`),
`PackedUV.cs` (`dd7f9226fe3e35acf75331f787798d701d9076487162279cffd7bec0eed16fc2`),
`PackedU8Vector4.cs` (`1a6ca1722f95e13142dda2ce8905ab2b1f3554ce2ec3009b5c6af271fe827d7f`),
`PackedRGBA.cs` (`134daa9d90c9dc15a41ee5b75c6456f3596f0e5020b32091f5d607eeb7a94e18`),
`PackedJoints.cs` (`f23a09f19c2aa068b0c0738200b2133bb7555f2b993b904c777ecd67dae9a3e8`),
all under ignored `local/reference-atk/`. An earlier 1.3.1.0 copy
(`C:/Users/PC/Desktop/Anvil/AnvilToolkit.dll`, SHA-256
`ba8db2234c75bc1daf67ca28f199ff54b305c1210262e2aecdfb2aaa10ee3a96`) was
diffed for the classes used: `PackedU8Vector4` text is identical and the
position/UV formulas match; only unrelated members differ
(`PackedRGBA` sRGB helpers, `PackedJoints` fill/index API). These are
third-party format observations; no implementation text was imported.

Relevant locations in the retained 1.3.7 sources: `Vertex.cs` `ReadFromFile`
line 413, quantization factor line 415, format branches Joint4 line 418,
Joint8 line 428, plain format line 458, `Col4ub` format line 465, tangent-only
format line 474; `PackedS16Vector4.cs` lines 54–69; `PackedUV.cs` lines 34–46;
`PackedU8Vector4.cs` lines 46–53; `PackedRGBA.cs` `Sign` line 11 and
`PackedRGBA(short)` line 45; `PackedJoints.cs` byte/byte constructor line 246
and weight normalization line 256; `VertexFormatsMap.cs` `UnityVertexFormatMap`
lines 176–254 (Unity uses this map, line 956).

Installed archive identities used for numeric validation (each is one stored
range, not a whole-file hash; all saved under ignored
`local/mesh-box-validation-20261018/`):

| Mesh | Archive / entry / resource | File ID | Payload SHA-256 |
| --- | --- | --- | --- |
| `TPL_WP_Rock_LOD0` | `DataPC.forge` 110 / 2471 | `000000007ec1ea7d` | `a5dc08f3b7f38809f56ae9aa56bfe6163e39eb5f525d8009b74444a5f14b29a4` |
| `VIL_LM_Desk_ArnoRoom_01_LOD0` | `DataPC_ACU_Paris_assets.forge` 20931 / 0 | `0000001235f5b8c8` | `a17afca34c9219b3bf9336b5c798d51bbe11da0746477cb6d3df793a734d63f2` |
| `VIL_LM_Bed_ArnoRoom_01_LOD0` | `DataPC_ACU_Paris_assets.forge` 20935 / 0 | `000000121177e49a` | `302d0a33328fe23d696612f16fafbda727b21e31540e9b647bc8bd144d915007` |
| `CN_U_FR_Arno_Head_LOD0` | `DataPC_ACU_LGS_BelleEpoque.forge` 351 / 1 | `000000126c7eb58b` | `4e42910b4c700f68c63b48d8940e411cb9a5b83c5fc5fcbc8c5fdd0cc8dfab44` |
| `CN_P_Arno_FearlessAssassin_Legs_LOD0` | `DataPC_ACU_LGS_BelleEpoque.forge` 679 / 23 | `00000023dab178b2` | `ed3ee99d09824b8764ff0a255aa1837ff668e75f1aca65857f3b900ed6ab314c` |

The 679 entry as a whole has stored SHA-256
`a17f3e13fbc3ded2486460e34067256ea6435891b990109d23278a38b1b6c680`.

## Observations

`VertexFormatsMap.cs` lines 176–254 map Unity vertex-format indices to
attribute lists; the codes observed in installed data and their recovered
layouts are:

| Code | Attributes (recovered order) | Stride | ATK name |
| --- | --- | --- | --- |
| 0 | position, normal, tangent, binormal, UV, 4 joint indices, 4 weights | 32 | `Pos3s_Col1s_Norm3ub_Col1ub_Tan4ub_Binorm4ub_Tex2s_Joint4` |
| 1 | as 0 with 8 joints and 8 weights | 40 | `..._Joint8` |
| 4 | position, normal, tangent, UV | 20 | `Pos3s_Col1s_Norm3ub_Col1ub_Tan4ub_Tex2s` |
| 5 | position, normal, tangent, 4 color bytes, UV | 24 | `Pos3s_Col1s_Norm3ub_Col1ub_Tan4ub_Col4ub_Tex2s` |
| 6 | position, normal, UV | 16 | `Pos3s_Col1s_Norm3ub_Col1ub_Tex2s` |

Attribute order and widths are read from the reference `ReadFromFile` branches,
not inferred: position is four `i16`; normal, tangent and binormal are each four
bytes; the UV is two `i16`; joint indices and weights are one byte each.
Format 5 additionally reads four color bytes after the tangent (line 471) and
before the UV (line 472); format 6 reads only position, normal, UV.

Formulas, all from the reference read path:

- Position: `x,y,z` are `i16`; `w` is a fourth `i16` whose sign selects the
  divisor, `i16 / (32767 * sign(w))`, with `w == 0` treating the divisor as
  positive (`PackedS16Vector4.cs` lines 56–68). Unity is not in the
  AC1/AC2/Brotherhood/Revelations exemption list (lines 61–64).
- Normal/tangent/binormal: `(byte - 127) / 127` per component; the fourth byte
  is not part of the vector (`PackedU8Vector4.cs` lines 46–53). The reference
  reuses that byte as a color channel; this decoder preserves it verbatim as
  `normal_w`/`tangent_w`/`binormal_w`.
- UV: `i16 / 2048` (`PackedUV.cs` lines 34–46; the
  `GhostReconBreakpoint` 32768/UVScale variant at `Vertex.cs` line 415 does not
  apply to Unity).
- Joints: index bytes are raw (`PackedJoints.cs` line 246 constructor takes the
  index bytes verbatim); weights normalize as `byte / 255` (line 256). This
  decoder stores both raw.
- `w` is also read by the reference as a packed color/scale:
  `Color0 = new PackedRGBA((short)Position.w)` (`Vertex.cs` line 430 for
  format 1).

The mesh's `CompiledMesh` quantization factor is applied to the decoded
position (`i16 / (32767 * sign(w)) * quantization_factor`). The reference
divides only by `32767 * sign(w)` and applies the quantization factor at
export; the archive truth is the quantization factor, which is validated
numerically below.

Installed observations (legs mesh, 2359 vertices): every stored `w` is `32767`,
so the positive divisor applies; joint index bytes are all below 128.

## Numeric validation: decoded bounds versus the stored cluster box

Each `ClusteredMeshData` stores a float `center` and `half_extent` for the
whole mesh. Decoding the clustered vertex buffer linearly with the layout above
must place every vertex inside `center ± half_extent` if the formula and the
quantization multiplication are right. The bound comparison uses the stored
box expanded by `2 * quantization_factor / 32767` (two quantization steps,
`6.104e-5` at factor 2 and `9.155e-5` at factor 3); the maximum observed
per-axis absolute deviation between decoded bounds and the stored box is:

| Mesh | Format / stride | Quantization factor | Clusters | Vertices / triangles | Max \|decoded − box\| |
| --- | --- | --- | ---: | ---: | ---: |
| Rock | 4 / 20 | 1 | 1 | 52 / 72 | `1.4e-5` |
| Desk | 4 / 20 | 3 | 74 | 7414 / 4637 | `8.0e-5` |
| Bed | 5 / 28 | 3 | 64 | 4061 / 3976 | `9.7e-5` |
| Arno head | 1 / 40 | 2 | 1 | 4620 / 8640 | `8.6e-5` |
| Arno legs | 1 / 40 | 2 | 2 | 2359 / 4126 | `5.1e-5` |

The factor-2 and factor-3 cases discriminate against omitting the
quantization multiplication (bounds would be 2–3x the stored box). Every
decoded vertex of all five meshes lies within the expanded box; the `mesh::decode`
install test asserts this containment plus the legs vertex/index byte hashes
(`b2e5bf0dde5036f61f08080f5daff577e2286bd7c529962fff4ef0d17020228c` and
`f58465a98a320c2f213ce00d9c5704645f67ae0453f2335256651b9e7bf3b5b7`).
The stored box is an engine-written artifact independent of this decoder, so the
comparison is an independent check of position decode and of linear handling of
multi-cluster meshes. Note (ACU-RE-0012): the mesh-wide box check passes for any
base choice, because it only bounds positions; the per-cluster boxes are what
pin the primitive-relative index bases.

## Unresolved / inferred

- Cluster descriptor, bit mask and primitive-descriptor buffers remain opaque
  in this finding (`Mesh::unresolved()` still reports `"cluster descriptors/bit masks"`).
  Per-cluster vertex/index partitioning is not decoded; the linear buffer
  interpretation is what the box check validates. **Superseded in part by
  [ACU-RE-0012](ACU-RE-0012-clustered-mesh-layout.md)**, which types the 52-byte
  descriptor and 36-byte draw-primitive records, recovers the primitive base
  rule, and shows that the linear (no-base) reading of the clustered index buffer
  was wrong for multi-cluster meshes; only the bit mask stays unresolved.
- Format 5 with stride 28 is an installed variant whose attribute list is not in
  the reference Unity map (the reference format-5 list `..._Tan4ub_Col4ub_Tex2s`
  is 24 bytes). Its decode as `..._Tan4ub_Binorm4ub_Col4ub_Tex2s` (position,
  normal, tangent, binormal, color, UV) is inferred from the naming convention;
  only the position field and total stride are validated by the bed box check.
- Attribute order beyond position and the UV/normal semantics are not
  independently verified; the grey renderer does not consume normals, tangents,
  binormals or color, so a field-order error could not show up visually.
- The deviation magnitudes up to ~1.4 quantization steps are unexplained in
  detail (candidate causes: engine-side rounding when writing the box, or a
  step-aware quantization); the containment assertion only relies on two steps.
- Joint indices are decoded but not mapped to the skeleton's bones, and no
  skinning is performed; bone index semantics remain unknown.
- Native axes and units are not established by this finding. Character heights
  in decoded native units are consistent with metres, but no axis claim is made
  and the render applies an explicit presentation rotation.
- The reference reader and writer disagree about the interpretation of `w`
  (reader: sign only; writer: `PackedRGBA.Sign` of the packed color). Original
  engine semantics of `w` remain unestablished. No installed mesh with a
  negative `w` was observed; the negative-`w` path is exercised by a synthetic
  test only.

## Implementation implications and finding-linked constants

- `acu_assets/src/vertex.rs`: `Layout` table keyed by `(format, stride)`,
  `decode()` implementing the formulas above, `Geometry` with bounds/extent.
  Normal components clamp at 1.0 (`Math.Min(..., 1.0)` exists in the reference
  AC3-style normal path but not in `PackedU8Vector4`); this is an explicit
  implementation choice, reversible because the raw bytes are preserved.
- `acu_assets/src/mesh.rs`: `Mesh::decode()` chooses non-clustered buffers when
  populated, otherwise the clustered block; index width comes from `MeshData`;
  topology uses the first non-depth-only triangle-list primitive, falling back
  to the shadow list.
- `acu_bevy`: first visible reconstruction slice — loads eleven Arno mesh
  resources (nine FearlessAssassin battle-outfit pieces, head, hood-up hair
  plane) straight from the installed Forge archives, prints per-piece provenance
  (archive, entry, resource, file ID, entry and payload SHA-256,
  format/stride/vertices/triangles) and renders them in a grey synthetic arena
  at 1280x720 from three orbit angles (`local/acu-arno-35.png`,
  `local/acu-arno-145.png`, `local/acu-arno-260.png`; manifest in
  `local/captures/acu-arno-20261018/capture.json`). Winding is unverified, so
  meshes are rendered double-sided; the native Z-up to Bevy Y-up rotation is a
  presentation choice. A post-gate rebuild reproduced all three captures
  byte-for-byte.
- Headless capture requires
  `RenderPlugin { synchronous_pipeline_compilation: true }`: with the default
  asynchronous pipeline compilation a short headless run captured only the
  clear colour or black because mesh and screenshot pipelines were never
  compiled (diagnosis logs in `local/logs/`). The windowed screenshot path is
  not usable on this machine (Vulkan swapchain validation errors, no DX12
  image); the offscreen `RenderTarget::Image` path is the validated one.

This is a synthetic arena over real decoded bytes. It establishes decode
plausibility and forward progress, not original-world, material, animation or
gameplay parity.

## Reproduction

```powershell
$env:ACU_GAME_DIR = "D:/Games/Assassin's Creed Unity"
cd acu-rs
cargo test --offline -p acu_assets --test vertex -- --include-ignored
target/debug/acu-mesh.exe "D:\Games\Assassin's Creed Unity\DataPC_ACU_LGS_BelleEpoque.forge" 679 23
target/debug/acu_bevy.exe --capture ..\local\acu-arno-35.png --frames 30 --orbit-deg 35
```

`synthetic-vertex-tests`: `cargo test --offline -p acu_assets --test vertex`
runs four authored tests covering format 1 with positive and negative `w`,
formats 4/5/6 shapes, the stride-28 lookup, unknown format/stride messages,
non-multiple buffer lengths, nonpositive/nonfinite quantization factors,
configured limits and the empty buffer. The ignored install test additionally
pins the legs entry, resource, payload and clustered buffer hashes, cluster
counts, decoded counts and box containment; without `ACU_GAME_DIR` it reports
the install as unavailable instead of passing.
