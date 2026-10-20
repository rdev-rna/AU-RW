# ACU-RE-0012 — Clustered mesh descriptor and draw-primitive layout

Date: 2026-10-18. Task: ACU-005 (also closes the ACU-004 render-quality gap).
State: partially established.

## Provenance

Asset-byte evidence against installed ACU.exe identity SHA-256
`748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40`,
image base `0x140000000`. No native VA/RVA, export correspondence or runtime
execution of game code is established by this finding; every decoded value comes
from archive bytes plus cross-checks against engine-written bounds stored in the
same records. Read-only third-party reference: AnvilToolkit.dll 1.3.7.0 SHA-256
`940374775553172b06d55537c4db78048bc3206b9b2f72bcc61bacfdcdb57636`
(`ClusteredMeshData.cs` in `local/reference-atk-utf8/`, decompiled with
ilspycmd 8.2.0.7535). That reference names the three buffers (`ClusterDescData`,
`ClusterBitMask`, `PrimitiveDescData`), reads them as opaque length-prefixed byte
arrays and never interprets their fields; it independently corroborates the
surrounding order (`ClusterCountPerDrawPrim` and `VertexOffsetPerDrawPrim` as
counted `int` arrays, then `IsFixedClusterSize` as a boolean). The field
semantics below are ours, recovered from numeric validation, not imported text.

Install samples (each entry is one stored range, archived under ignored
`local/cluster-re-20261018/` as `new-*.json` from `acu-mesh --clusters`; payload
hashes are in ACU-RE-0011 and in the per-sample JSON):

| Mesh | Archive / entry / resource | File ID | Format / stride |
| --- | --- | --- | --- |
| `TPL_WP_Rock_LOD0` | `DataPC.forge` 110 / 2471 | `000000007ec1ea7d` | 4 / 20 |
| `UnitSphere` | `DataPC.forge` 10 / 0 | `0000000000000028` | 4 / 20 |
| `CN_U_FR_Arno_Head_LOD0` | `DataPC_ACU_LGS_BelleEpoque.forge` 351 / 1 | `000000126c7eb58b` | 1 / 40 |
| `CN_U_Arno_HairPlane_HoodUp_LOD0` | same, 655 / 1 | `0000002318e0dea9` | 0 / 32 |
| nine `CN_P_Arno_FearlessAssassin_*_LOD0` pieces | same, 679 / {11,15,19,23,27,31,35,39,43} | see manifest | 1 / 40 |
| `VIL_LM_Desk_ArnoRoom_01_LOD0` | `DataPC_ACU_Paris_assets.forge` 20931 / 0 | `0000001235f5b8c8` | 4 / 20 |
| `VIL_LM_Bed_ArnoRoom_01_LOD0` | `DataPC_ACU_Paris_assets.forge` 20935 / 0 | `000000121177e49a` | 5 / 28 |

Exact commands:

```powershell
$env:ACU_GAME_DIR = "D:/Games/Assassin's Creed Unity"
acu-rs/target/debug/acu-mesh.exe "$env:ACU_GAME_DIR/DataPC_ACU_Paris_assets.forge" 20931 0 --clusters --out=local/cluster-re-20261018/new-desk.json
acu-rs/target/debug/acu-mesh.exe "$env:ACU_GAME_DIR/DataPC.forge" 10 0 --clusters --out=local/cluster-re-20261018/new-sphere.json
# one run per sample; local/cluster-re-20261018/run_samples.py repeats all of them
python local/cluster-re-20261018/validate.py   # rule replay over every new-*.json
```

`acu-mesh --clusters` calls `Mesh::audit_clusters`, which decodes the clustered
vertex buffer with the ACU-RE-0011 layouts and measures per-cluster bounding-box
and normal-cone fit from the index spans below. The clustered block itself
carries only `{version u32, vertex_format u8, vertex_stride u32}` for the layout;
the index width comes from the sibling `MeshData`. Mask statistics in this
finding come from `acu-mesh --hex --out=local/cluster-re-20261018/hex-desk.json`.

## Result

The compiled clustered block continues after its vertex/index buffers with three
length-prefixed records whose meaning was previously opaque. Recovered layouts
(all little-endian; every sampled mesh agrees):

```
cluster descriptor, 52 bytes, one per cluster, in index order:
  f32 center[3]              cluster AABB centre, native units
  f32 half_extent[3]         cluster AABB half extents, native units
  f32 cone_axis[3]           normal-cone axis
  f32 cone_angle             normal-cone half angle, radians
  u32 index_offset           first index of this cluster in the index buffer
  u32 index_count            number of indices in this cluster
  u32 vertex_offset_dwords   owning primitive's vertex-group byte offset / 4

per-cluster bit mask, 48 bytes per cluster (semantics unresolved; see below)

draw primitive, 36 bytes, one per draw primitive, in draw order:
  f32 center[3]              primitive AABB centre
  f32 half_extent[3]         primitive AABB half extents
  u32 stride                 must equal the clustered stride
  u32 packed                 1-byte flag | (value << 8)
  u32 index_start            index start of this primitive's first cluster
```

`packed`'s low byte is a flag with exactly two observed values, and
`flag = 1` carries the primitive's total index count while `flag = 0` carries its
cluster count; the flag is the exact inverse of `fixed_cluster_size` on every
sample. Three statement-level inferences from the record shapes (not verified
natively): the AABB pairs are engine-written derived data, the flag selects
between two cluster-sizing modes, and `index_start` duplicates the first
cluster's `index_offset`.

Validation rules, all replayed over the 15 samples; each rule held on every one
of them within the stated tolerances (a violation is treated as an unsupported
variant, not a silent misread):

| # | Rule | Result |
| --- | --- | --- |
| R1 | Cluster `index_offset` values are the running sum of `index_count` and tile the index buffer exactly | Held: 15/15, total equals buffer length |
| R2 | `index_start` equals the primitive's first cluster `index_offset` | Held: 15/15 |
| R3 | flag 1: `value` = sum of the primitive's cluster `index_count`; flag 0: `value` = `clusters_per_primitive[primitive]` | Held: 15/15 |
| R4 | `flag == 1` ⇔ `!fixed_cluster_size` | Held: 15/15 |
| R5 | Primitive `stride` = clustered stride; `vertex_offset_dwords` = `vertex_offsets[primitive]`; vertex base = `dwords * 4 / stride` | Held: 15/15 |
| R6 | Every raw index of a cluster, plus its primitive's base, stays inside that primitive's vertex group | Held: 15/15 |
| R7 | Cluster/primitive AABBs contain the decoded geometry; normal cone contains the triangle normals | Held: 15/15 AABB within 1.0122 (three desk clusters exceed a 1.01 tolerance); cone outside count 0 |

Per-mesh results (`max_aabb_fit` = worst per-axis
`|p − center| / half_extent` over all clusters; `max_cone_slack` =
worst `cone_max_angle − cone_angle`, positive meaning the stored cone is too
tight):

| Mesh | Clusters / primitives | `clusters_per_primitive` | Fixed size | `max_aabb_fit` | `max_cone_slack` (rad) | cone violations |
| --- | ---: | --- | --- | ---: | ---: | ---: |
| UnitSphere | 1 / 1 | `[1]` | no | 1.00003 | −0.3141 | 0 |
| Rock | 1 / 1 | `[1]` | no | 1.00040 | −0.1466 | 0 |
| Head | 1 / 1 | `[1]` | no | 1.00046 | −0.0249 | 0 |
| Hood-up hair plane | 1 / 1 | `[1]` | no | 1.00059 | −0.0672 | 0 |
| ShoulderCover | 1 / 1 | `[1]` | no | 1.00049 | −0.0732 | 0 |
| Flaps | 1 / 1 | `[1]` | no | 1.00017 | −0.0398 | 0 |
| Upperbody Down | 1 / 1 | `[1]` | no | 1.00039 | −0.0415 | 0 |
| Scarf | 1 / 1 | `[1]` | no | 1.00073 | −0.0274 | 0 |
| Hood up | 1 / 1 | `[1]` | no | 1.00035 | −0.1062 | 0 |
| Belt | 4 / 4 | `[1,1,1,1]` | no | 1.00125 | +0.00816 | 0 |
| ThrowBlade | 3 / 3 | `[1,1,1]` | no | 1.00074 | −0.00066 | 0 |
| Upperbody | 3 / 3 | `[1,1,1]` | no | 1.00035 | −0.0198 | 0 |
| Legs | 2 / 2 | `[1,1]` | no | 1.00014 | −0.0084 | 0 |
| Desk | 74 / 3 | `[16,34,24]` | yes | 1.01215 | +0.00972 | 0 |
| Bed | 64 / 4 | `[17,35,2,10]` | yes | 1.00276 | +0.00309 | 0 |

`cone_angle` is π with `cone_axis = (1,0,0)` on every variable-size sample, which
reads as a disabled-cone sentinel; only the 138 fixed-size clusters (desk, bed)
carry usable cones. The earlier "oriented-box" reading of that pair was a
tautology (a π cone contains every normal in every orientation) and is
superseded by the cone interpretation here.

`max_aabb_fit` is a small systematic overrun rather than a few outliers: on the
desk 73 of 74 clusters exceed 1.0 but only five reach 1.004 or more (1.0040,
1.0072, 1.0114, 1.0114, 1.0122), and on the bed the worst is 1.00276. The
magnitude is 0.0–1.2% of a cluster half extent, i.e. millimetres, consistent
with engine-side rounding when the boxes were written; the exact cause is not
established. The replay script flags the three desk clusters above 1.01
(clusters 55, 56, 70) and nothing else, and the containment margin is what R7
relies on.

### Vertex bases

Clustered raw indices are primitive-relative. The base is
`vertex_offset_dwords * 4 / stride`, i.e. the serialized field is a byte offset
in 4-byte units. The sibling `MeshData` `min_index` agrees on the 13
variable-size samples (belt, throwblade, legs, upperbody), which is why the
ACU-RE-0011 linear reading looked plausible, but it is *not* the clustered base
in general:

| Mesh | `vertex_offsets` (dwords) | descriptor base | `MeshData.min_index` | difference |
| --- | --- | --- | --- | --- |
| Belt | `[0, 23700, 33310, 35710]` | `[0, 2370, 3331, 3571]` | `[0, 2370, 3331, 3571]` | 0 |
| Legs | `[0, 13100]` | `[0, 1310]` | `[0, 1310]` | 0 |
| Upperbody | `[0, 35680, 55000]` | `[0, 3568, 5500]` | `[0, 3568, 5500]` | 0 |
| Desk | `[0, 7540, 24580]` | `[0, 1508, 4916]` | `[0, 1508, 4916]` | 0 |
| Bed | `[0, 13272, 23037, 24241]` | `[0, 1896, 3291, 3463]` | `[0, 1656, 3051, 3223]` | **−240 every group after the first** |

The bed is decisive: using `min_index` puts the second and later vertex groups
240 vertices too low, which the cluster AABB rule then rejects, while the
descriptor bases reproduce the stored boxes (R7). The bed's `min_index` chain is
internally consistent (1656+1395 = 3051, 3051+172 = 3223), so it describes a
different, older vertex layout; the descriptor offsets match the clustered
buffer that is actually stored, and are authoritative for it.

The `MeshData` primitive spans of the cluster-only meshes are stale in the same
way: desk spans cover 13,911 indices against 14,208 stored, bed spans 11,928
against 12,288. `Mesh::decode` therefore sources topology from the cluster
descriptors whenever the clustered block is present and the non-clustered
buffers are empty, reporting `topology_source = "cluster_descriptors"`.

### Independent check: rendering

The corrected bases change the rendered result wherever a mesh has more than one
cluster. Fresh headless captures (30 frames, 1280×720, offscreen):

| Output | Subject | SHA-256 |
| --- | --- | --- |
| `local/captures/acu-clusters-20261018/acu-arno-35.png` | Arno set, azimuth 35° | `7dc374fa90a43eed7651d22463d586efc8180d6e810191d8dff5ea2db6e449cc` |
| `.../acu-arno-145.png` | Arno set, azimuth 145° | `f4a3bddd6d3acf28a2a2d2579fa4069c512613ee7f73cfa000591e43e77f5046` |
| `.../acu-arno-260.png` | Arno set, azimuth 260° | `5e0a94a0be1cd4a3fc11155dd08b65a004b13a4ea6e4a552abe13233c74d51be` |
| `.../desk-35.png` | `VIL_LM_Desk_ArnoRoom_01_LOD0` | `42d92bcf435f9bbd79b793fb6f244b125be1823a6fe45b4f27553d0425919128` |
| `.../bed-35.png` | `VIL_LM_Bed_ArnoRoom_01_LOD0` | `8633dd9a05fc1e7e3b9e3690e0384cb4049ca56168fb32fec6d9a1710e9e5e02` |
| `.../rock-35.png` | `TPL_WP_Rock_LOD0` | `7c9359d527b227ceb41fb0237bbe744c3f33566c9b7fa92752a941001fa33cc2` |

Compared with the ACU-RE-0011 captures (`local/acu-arno-35.png` etc.), the Arno
pieces with several clusters now assemble: the previous renders had a mangled
torso and detached arms from misplaced vertex groups, while the new ones show a
coherent hood, coat with buttons, belt, gloves, legs and boots from all three
angles. The desk renders as a readable writing desk with shelves, hutch and
drawers (74 clusters across three vertex groups), and the bed as a four-poster
bed with posts, canopy and mattress (64 clusters, the +240 correction).
Per-piece provenance, bounds and the cluster-audit parameters are in the run
logs `local/logs/capture-clusters-*.log` and in the sample JSONs above.

## Unknowns / incomplete coverage

- **Bit mask semantics remain unresolved.** The mask is exactly 48 bytes (384
  bits) per cluster on every sample. On the desk sample 83% of bits are set, the
  all-ones/all-zero byte counts are 2154/147 of 3552, and no byte position is
  consistently zero; an earlier probe showed a 2-bits-per-index reading only
  fits clusters of 192 indices, so it is not a general per-index field. Nothing
  in this finding depends on it; `Mesh::unresolved()` reports
  `"cluster bit mask semantics"` for every clustered mesh.

  New lead (2026-10-19): the user-provided Haar/Aaltonen SIGGRAPH2015 deck,
  physical PDF pages22-24, describes six per-triangle visibility directions for ACU's
  64-triangle clusters. `6 * 64` matches the observed 384 bits. This strongly
  suggests culling masks for the fixed-size path, not lighting data. The Arno
  variable-size descriptors can span more than64 triangles, so this interpretation
  cannot be applied to every stored record. Bit ordering, direction orientation,
  padding and the variable-size variant still require geometry/native validation.
  See RENDERING_SOURCES.md.

- `clusters_per_primitive` is still a flat list in draw order; the mapping from
  clusters to bones, submeshes, materials or LOD sets is not recovered.
- `fixed_cluster_size` semantics: on desk/bed every cluster has exactly 192
  indices, but whether the mode means "all clusters equal size" or a different
  storage strategy is not established.
- The desk's 1.0122 box overrun and the belt's +0.0082 rad cone slack are not
  explained; candidates are pre-quantization cone computation, per-cluster
  vertex normals, or engine rounding. `2 * quantization_factor / 32767` box
  tolerance from ACU-RE-0011 does not cover 1.0122, so a rounding explanation is
  plausible but unverified.
- The bed's 240-vertex offset between `min_index` and the clustered layout is
  unexplained; the descriptor values reproduce the stored boxes, which is the
  only evidence used.
- Triangle winding, native axes/units, skinning and material binding remain
  open (ACU-RE-0011), as does any relationship between these records and the
  native draw path.

## Implementation implications and finding-linked constants

- `acu_assets/src/mesh.rs`: `ClusterDescriptor` (52 B), `ClusterPrimitive`
  (36 B), `clusters`/`draw_primitives` typed views, `ClusteredData::vertex_base`,
  `primitive_of_cluster`, `index_count`, `validate_clustered` (R1–R7) and
  `Mesh::audit_clusters` returning `ClusterAudit` rows. The mask is retained as
  raw bytes.
- `decode()` uses cluster descriptors for cluster-only meshes (the common case:
  all 15 sampled meshes have empty non-clustered buffers, so `MeshData` is a
  span/`min_index` description and the clustered block is the geometry).
- `acu-mesh --clusters` prints the typed tables plus `cluster_audit` and
  `cluster_audit_summary` (`max_aabb_fit`, `cone_outside` with a 0.011 rad
  tolerance, `max_cone_slack`), which is the reproduction surface for this
  finding.
- `acu_bevy` re-captured the ACU-RE-0011 slice after the decode change and added
  desk/bed/rock captures; no application code changed for this finding.
- Tests: `tests/mesh.rs` builds a synthetic clustered block (typed descriptors,
  11 malformed mutations) and asserts the installed rock, sphere and head
  records; `tests/vertex.rs::installed_arno_legs_geometry` asserts the legs
  spans, bases, decoded counts and per-cluster audits.
