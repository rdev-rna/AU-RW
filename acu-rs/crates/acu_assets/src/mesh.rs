//! Composable ACU compiled MeshData reader. See ACU-RE-0008 and ACU-RE-0012.
//!
//! Clustered descriptor records are typed from ACU-RE-0012: 52 bytes per
//! cluster (native-axis bounding box plus a normal cone), 36 bytes per draw
//! primitive, and a 48-byte per-cluster bit mask whose semantics remain
//! unresolved. Cluster-only meshes take their index spans and vertex bases from
//! those records; `MeshData` primitive spans describe the original non-clustered
//! buffers and disagree with the clustered layout on fixed-size cluster meshes.
use crate::{Limits, Result, invalid, reader::Reader};
use serde::Serialize;

pub const TYPE_MESH: &str = "415d9568";
pub const CLASS_MESH_DATA: u32 = 0x0645_abb5;
pub const CLASS_MESH_PRIMITIVE: u32 = 0xa573_87ef;

/// Serialized size of one clustered descriptor record (ACU-RE-0012).
pub const CLUSTER_DESCRIPTOR_BYTES: usize = 52;
/// Serialized size of one clustered draw-primitive record (ACU-RE-0012).
pub const CLUSTER_PRIMITIVE_BYTES: usize = 36;
/// Serialized size of the per-cluster bit mask; semantics unresolved.
pub const CLUSTER_BIT_MASK_BYTES: usize = 48;

/// One 52-byte clustered descriptor record. See ACU-RE-0012.
///
/// `center`/`half_extent` are the cluster's native-space axis-aligned bound:
/// for every sampled cluster the decoded positions satisfied
/// `|p - center| <= half_extent` within two quantization steps. `cone_axis`
/// and `cone_angle` are a normal cone; on fixed-size cluster meshes it enclosed
/// every triangle normal within 0.01 rad, while `[1, 0, 0]` with `PI` is the
/// observed disabled sentinel.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ClusterDescriptor {
    pub center: [f32; 3],
    pub half_extent: [f32; 3],
    pub cone_axis: [f32; 3],
    pub cone_angle: f32,
    /// First index of this cluster in the clustered index buffer.
    pub index_offset: u32,
    /// Number of indices in the cluster.
    pub index_count: u32,
    /// Byte offset of the owning primitive's vertex group in 4-byte units.
    pub vertex_offset_dwords: u32,
}

/// One 36-byte clustered draw-primitive record. See ACU-RE-0012.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ClusterPrimitive {
    pub center: [f32; 3],
    pub half_extent: [f32; 3],
    pub stride: u32,
    /// 1 when `value` is the primitive's total index count (variable cluster
    /// sizes); 0 when `value` is the primitive's cluster count (fixed-size
    /// clusters). The two variants agreed with `fixed_cluster_size` on every
    /// sampled mesh.
    pub flag: u8,
    pub value: u32,
    /// Index start of the primitive's clusters in the clustered index buffer.
    pub index_start: u32,
}

#[derive(Debug, Serialize)]
pub struct MeshBone {
    pub file_id: String,
    pub name_hash: String,
    /// Serialized matrix order, before any inversion or coordinate conversion.
    pub matrix: [f32; 16],
    pub used_by_submeshes: bool,
}

#[derive(Debug, Serialize)]
pub struct FileReference {
    pub tag: u8,
    pub global: u8,
    pub file_id: String,
}

#[derive(Debug, Serialize)]
pub struct UVChannel {
    pub file_id: String,
    pub kind: u32,
    pub layer_index: u8,
    pub coordinates: Vec<[f32; 2]>,
    pub original_indices: Vec<u32>,
}

#[derive(Debug, Serialize)]
pub struct SubMesh {
    pub file_id: String,
    pub managed: bool,
    pub kind: u32,
    pub indices: Vec<u32>,
    pub original_indices: Vec<u32>,
    pub smoothing_groups: Vec<u32>,
    pub visible_edges: Vec<u8>,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 3]>,
    pub bitangents: Vec<[f32; 3]>,
    pub colors: Vec<[u8; 4]>,
    pub layer_weights: Vec<f32>,
    pub bones_per_vertex: u8,
    pub bone_weights: Vec<f32>,
    pub bone_indices: Vec<u16>,
    pub collapsed_vertex_mapping: Vec<u16>,
    pub uv_channels: Vec<UVChannel>,
    pub material: FileReference,
    pub fast_load_serialized: bool,
    pub processed_mesh: SubMeshPointer,
}

#[derive(Debug, Serialize)]
pub enum SubMeshPointer {
    Null,
    Reference(String),
    Inline(Box<SubMesh>),
}

#[derive(Debug, Serialize)]
pub struct InstancingData {
    pub file_id: String,
    pub shadow_caster: bool,
    pub submesh_index: u16,
    pub material_type: u16,
    pub vertex_count: u16,
    pub material: Option<String>,
    pub hardware_to_mesh_bones: Vec<u16>,
    /// Fixed 256-byte bone-table tail, preserved rather than assigned semantics.
    pub bone_table_padding: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct ClusteredData<'a> {
    pub file_id: String,
    pub version: u32,
    pub vertex_format: u8,
    pub vertex_stride: u32,
    pub cluster_count: u32,
    pub center: [f32; 3],
    pub half_extent: [f32; 3],
    pub draw_primitive_count: u32,
    pub clusters_per_primitive: Vec<u32>,
    pub vertex_offsets: Vec<u32>,
    pub fixed_cluster_size: bool,
    /// Typed 52-byte cluster records in index order.
    pub clusters: Vec<ClusterDescriptor>,
    /// Typed 36-byte draw-primitive records in draw order.
    pub draw_primitives: Vec<ClusterPrimitive>,
    #[serde(skip)]
    pub vertices: &'a [u8],
    #[serde(skip)]
    pub indices: &'a [u8],
    /// Retained 52-byte records; typed view is `clusters`.
    #[serde(skip)]
    pub descriptors: &'a [u8],
    /// Per-cluster bit mask; semantics unresolved, retained as raw bytes.
    #[serde(skip)]
    pub bit_mask: &'a [u8],
    /// Retained 36-byte records; typed view is `draw_primitives`.
    #[serde(skip)]
    pub primitive_descriptors: &'a [u8],
}

impl ClusteredData<'_> {
    /// Index count of the clustered index buffer, using the `MeshData` index
    /// width (the clustered block itself carries no width flag).
    pub fn index_count(&self, width: usize) -> usize {
        self.indices.len() / width
    }

    /// First vertex of a draw primitive's vertex group.
    ///
    /// The serialized offsets are byte offsets in 4-byte units, so the vertex
    /// base is `offset * 4 / stride`. This is the authoritative base for
    /// clustered indices; `MeshData` `min_index` agrees on variable-size
    /// cluster meshes but not on fixed-size ones (bed differed by 240).
    pub fn vertex_base(&self, primitive: usize) -> Option<usize> {
        let dwords = usize::try_from(*self.vertex_offsets.get(primitive)?).ok()?;
        let bytes = dwords.checked_mul(4)?;
        Some(bytes / usize::try_from(self.vertex_stride).ok()?)
    }

    /// Vertex group `(base, count)` of a draw primitive: from its 4-byte-unit
    /// offset to the next primitive's base (or the vertex buffer end).
    /// `validate_clustered` guarantees the group lies inside the buffer.
    pub fn vertex_group(&self, primitive: usize, vertex_count: usize) -> Option<(usize, usize)> {
        let base = self.vertex_base(primitive)?;
        let end = if primitive + 1 < self.vertex_offsets.len() {
            self.vertex_base(primitive + 1)?
        } else {
            vertex_count
        };
        (end >= base).then_some((base, end - base))
    }

    /// Draw primitive owning a cluster, from `clusters_per_primitive` order.
    pub fn primitive_of_cluster(&self, cluster: usize) -> Option<usize> {
        let mut base = 0usize;
        for (primitive, &count) in self.clusters_per_primitive.iter().enumerate() {
            let count = usize::try_from(count).ok()?;
            if cluster < base.checked_add(count)? {
                return Some(primitive);
            }
            base += count;
        }
        None
    }
}

/// One material bound to a draw primitive, from the instancing record at the
/// same serialized position (ACU-RE-0013).
#[derive(Debug, PartialEq, Serialize)]
pub struct DrawPartMaterial {
    pub file_id: String,
    /// `InstancingData::submesh_index` of the bound record.
    pub submesh_index: u16,
    pub material_type: u16,
}

/// One draw part of a decoded mesh: the triangles of one serialized draw
/// primitive, in the decoded (absolute) vertex index space, plus the material
/// the instancing list binds to that primitive when the pairing checks out.
#[derive(Debug, PartialEq, Serialize)]
pub struct DrawPart {
    /// Index of the draw primitive in serialized order.
    pub primitive: usize,
    pub vertex_base: usize,
    pub vertex_count: usize,
    /// `None` when no instancing record pairs with this primitive.
    pub material: Option<DrawPartMaterial>,
    pub triangles: Vec<[u32; 3]>,
}

/// Per-cluster audit row from `Mesh::audit_clusters` (ACU-RE-0012).
#[derive(Debug, Serialize)]
pub struct ClusterAudit {
    pub cluster: usize,
    pub primitive: usize,
    pub vertex_base: usize,
    pub distinct_vertices: usize,
    pub triangles: usize,
    /// `max_k |p_k - center_k| / half_extent_k` over the cluster's vertices.
    pub aabb_fit: f32,
    /// Largest angle between a triangle geometric normal and the cone axis.
    pub cone_max_angle: f32,
    pub cone_angle: f32,
}

#[derive(Debug, Serialize)]
pub struct CompiledMesh<'a> {
    pub file_id: String,
    /// Nonempty opaque compiled data prevents complete typed coverage.
    #[serde(skip)]
    pub data: &'a [u8],
    pub clustered: Option<ClusteredData<'a>>,
    pub mesh_data: MeshData<'a>,
    pub instancing: Vec<InstancingData>,
    pub platform_version: u32,
    pub sdk_version: u32,
    pub quantization_factor: f32,
}

#[derive(Debug, Serialize)]
pub struct Mesh<'a> {
    pub file_id: String,
    pub descriptor_mask: u8,
    pub category: u32,
    pub generated: bool,
    pub submeshes: Vec<SubMeshPointer>,
    pub bones: Vec<MeshBone>,
    pub local_extent_min: [f32; 4],
    pub local_extent_max: [f32; 4],
    pub compiled: CompiledMesh<'a>,
    pub materials: Vec<FileReference>,
    /// Unity order: dynamic, precomputed skinning, fake primitive masking,
    /// fake vertex format, keep submeshes, keep nonclustered data, entity bounds,
    /// ignore alpha test in depth pass, ignore position modification in depth pass.
    pub flags: [bool; 9],
    pub dynamic_vertex_count: u32,
    pub dynamic_index_count: u32,
    pub user_category: u32,
}

fn scalar(r: &mut Reader<'_>) -> Result<f32> {
    let value = f32::from_bits(r.u32()?);
    if !value.is_finite() {
        return invalid("nonfinite mesh scalar");
    }
    Ok(value)
}

fn floats<const N: usize>(r: &mut Reader<'_>) -> Result<[f32; N]> {
    let mut values = [0.; N];
    for v in &mut values {
        *v = scalar(r)?;
    }
    Ok(values)
}

fn reference_pointer(r: &mut Reader<'_>) -> Result<Option<String>> {
    match r.u8()? {
        1 => Ok(Some(format!("{:016x}", r.u64()?))),
        3 => Ok(None),
        tag => invalid(format!("unsupported mesh object-pointer variant {tag}")),
    }
}

fn buffer<'a>(r: &mut Reader<'a>, limits: Limits) -> Result<&'a [u8]> {
    let n = count(r, limits.max_decoded, 1)?;
    r.take(n)
}

fn integer_array(r: &mut Reader<'_>, limits: Limits) -> Result<Vec<u32>> {
    let n = count(r, limits.max_resources, 4)?;
    (0..n).map(|_| r.u32()).collect()
}

fn float_array<const N: usize>(r: &mut Reader<'_>, limits: Limits) -> Result<Vec<[f32; N]>> {
    let n = count(r, limits.max_resources, N * 4)?;
    (0..n).map(|_| floats(r)).collect()
}

fn short_array(r: &mut Reader<'_>, limits: Limits) -> Result<Vec<u16>> {
    let n = count(r, limits.max_resources, 2)?;
    (0..n).map(|_| r.u16()).collect()
}

fn file_reference(r: &mut Reader<'_>) -> Result<FileReference> {
    let tag = r.u8()?;
    let global = r.u8()?;
    let id = r.u64()?;
    if !matches!(tag, 1 | 3) || global > 1 || (tag == 3 && id != 0) {
        return invalid("unsupported mesh material reference");
    }
    Ok(FileReference {
        tag,
        global,
        file_id: format!("{id:016x}"),
    })
}

fn submesh(r: &mut Reader<'_>, limits: Limits, managed: bool, depth: usize) -> Result<SubMesh> {
    let file_id = header(r, 0x5755_de7f)?;
    let kind = r.u32()?;
    let indices = integer_array(r, limits)?;
    let original_indices = integer_array(r, limits)?;
    let smoothing_groups = integer_array(r, limits)?;
    let visible_edges = buffer(r, limits)?.to_vec();
    let positions = float_array(r, limits)?;
    let normals = float_array(r, limits)?;
    let tangents = float_array(r, limits)?;
    let bitangents = float_array(r, limits)?;
    let n = count(r, limits.max_resources, 4)?;
    let mut colors = Vec::with_capacity(n);
    for _ in 0..n {
        colors.push(r.take(4)?.try_into()?);
    }
    let layer_weights = float_array::<1>(r, limits)?
        .into_iter()
        .map(|v| v[0])
        .collect();
    let bones_per_vertex = r.u8()?;
    let bone_weights = float_array::<1>(r, limits)?
        .into_iter()
        .map(|v| v[0])
        .collect();
    let bone_indices = short_array(r, limits)?;
    let collapsed_vertex_mapping = short_array(r, limits)?;
    let n = count(r, limits.max_resources, 25)?;
    let mut uv_channels = Vec::with_capacity(n);
    for _ in 0..n {
        uv_channels.push(UVChannel {
            file_id: header(r, 0xbdad_8273)?,
            kind: r.u32()?,
            layer_index: r.u8()?,
            coordinates: float_array(r, limits)?,
            original_indices: integer_array(r, limits)?,
        });
    }
    let material = file_reference(r)?;
    let fast_load_serialized = boolean(r)?;
    let processed_mesh = submesh_pointer(r, limits, depth + 1)?;
    Ok(SubMesh {
        file_id,
        managed,
        kind,
        indices,
        original_indices,
        smoothing_groups,
        visible_edges,
        positions,
        normals,
        tangents,
        bitangents,
        colors,
        layer_weights,
        bones_per_vertex,
        bone_weights,
        bone_indices,
        collapsed_vertex_mapping,
        uv_channels,
        material,
        fast_load_serialized,
        processed_mesh,
    })
}

fn submesh_pointer(r: &mut Reader<'_>, limits: Limits, depth: usize) -> Result<SubMeshPointer> {
    if depth > 64 || depth > limits.max_tables {
        return invalid("submesh graph nesting exceeds configured limit");
    }
    Ok(match r.u8()? {
        0 | 4 => {
            let managed = boolean(r)?;
            SubMeshPointer::Inline(Box::new(submesh(r, limits, managed, depth)?))
        }
        1 => SubMeshPointer::Reference(format!("{:016x}", r.u64()?)),
        3 => SubMeshPointer::Null,
        tag => return invalid(format!("unsupported submesh pointer {tag}")),
    })
}

/// Decode the 52-byte cluster descriptor block (ACU-RE-0012).
fn cluster_descriptors(
    bytes: &[u8],
    count: usize,
    limits: Limits,
) -> Result<Vec<ClusterDescriptor>> {
    if count > limits.max_resources {
        return invalid("cluster count exceeds configured limit");
    }
    let expected = count
        .checked_mul(CLUSTER_DESCRIPTOR_BYTES)
        .ok_or("cluster descriptor block size overflow")?;
    if bytes.len() != expected {
        return invalid(format!(
            "unsupported cluster descriptor block: {} bytes for {count} clusters (expected {expected})",
            bytes.len()
        ));
    }
    let mut r = Reader::new(bytes);
    let mut clusters = Vec::with_capacity(count);
    for _ in 0..count {
        let center = floats(&mut r)?;
        let half_extent = floats(&mut r)?;
        let cone_axis = floats(&mut r)?;
        let cone_angle = scalar(&mut r)?;
        if half_extent.iter().any(|v| *v < 0.0) {
            return invalid("negative cluster half extent");
        }
        if !(0.0..=std::f32::consts::PI + 1e-3).contains(&cone_angle) {
            return invalid("cluster cone angle outside [0, PI]");
        }
        clusters.push(ClusterDescriptor {
            center,
            half_extent,
            cone_axis,
            cone_angle,
            index_offset: r.u32()?,
            index_count: r.u32()?,
            vertex_offset_dwords: r.u32()?,
        });
    }
    Ok(clusters)
}

/// Decode the 36-byte clustered draw-primitive block (ACU-RE-0012).
fn cluster_primitives(bytes: &[u8], count: usize, limits: Limits) -> Result<Vec<ClusterPrimitive>> {
    if count > limits.max_resources {
        return invalid("draw primitive count exceeds configured limit");
    }
    let expected = count
        .checked_mul(CLUSTER_PRIMITIVE_BYTES)
        .ok_or("cluster primitive block size overflow")?;
    if bytes.len() != expected {
        return invalid(format!(
            "unsupported cluster primitive block: {} bytes for {count} primitives (expected {expected})",
            bytes.len()
        ));
    }
    let mut r = Reader::new(bytes);
    let mut primitives = Vec::with_capacity(count);
    for _ in 0..count {
        let center = floats(&mut r)?;
        let half_extent = floats(&mut r)?;
        if half_extent.iter().any(|v| *v < 0.0) {
            return invalid("negative cluster primitive half extent");
        }
        let stride = r.u32()?;
        let packed = r.u32()?;
        let flag = (packed & 0xFF) as u8;
        if flag > 1 {
            return invalid("unsupported clustered primitive flag");
        }
        primitives.push(ClusterPrimitive {
            center,
            half_extent,
            stride,
            flag,
            value: packed >> 8,
            index_start: r.u32()?,
        });
    }
    Ok(primitives)
}

/// Cross-buffer checks for the recovered clustered layout (ACU-RE-0012).
///
/// Every rule below held on every sampled real mesh (rock, six Arno pieces,
/// head, desk, bed); a failure is an unsupported variant rather than a silent
/// misread.
fn validate_clustered(c: &ClusteredData<'_>, d: &MeshData<'_>) -> Result<()> {
    if !c.vertex_stride.is_multiple_of(4) {
        return invalid("clustered vertex stride is not a multiple of 4");
    }
    let width = if d.index32 { 4usize } else { 2 };
    let index_count = c.index_count(width);
    let vertex_count = c.vertices.len() / c.vertex_stride as usize;
    let mut cursor = 0usize;
    for cluster in &c.clusters {
        if cluster.index_offset as usize != cursor {
            return invalid("cluster index spans do not tile the clustered index buffer");
        }
        cursor = cursor
            .checked_add(cluster.index_count as usize)
            .ok_or("cluster index span overflow")?;
        if cursor > index_count {
            return invalid("cluster index span exceeds clustered index buffer");
        }
    }
    if cursor != index_count {
        return invalid("cluster index spans leave unclaimed clustered indices");
    }
    let mut cluster_cursor = 0usize;
    for (primitive_index, &cluster_count) in c.clusters_per_primitive.iter().enumerate() {
        let primitive = c
            .draw_primitives
            .get(primitive_index)
            .ok_or("cluster primitive block does not cover draw primitives")?;
        if primitive.stride != c.vertex_stride {
            return invalid("clustered primitive stride disagrees with clustered stride");
        }
        // Flag 1 carries the index count for variable-size clusters; flag 0
        // carries the cluster count for fixed-size clusters.
        if (primitive.flag == 1) == c.fixed_cluster_size {
            return invalid("clustered primitive flag disagrees with fixed cluster size");
        }
        let base = c
            .vertex_base(primitive_index)
            .ok_or("clustered vertex offset exceeds buffer")?;
        if c.vertex_offsets[primitive_index]
            .checked_mul(4)
            .ok_or("clustered vertex offset overflow")?
            % c.vertex_stride
            != 0
        {
            return invalid("clustered vertex offset is not a whole number of vertices");
        }
        let group_end = c.vertex_base(primitive_index + 1).unwrap_or(vertex_count);
        if base > group_end || group_end > vertex_count {
            return invalid("clustered vertex group exceeds vertex buffer");
        }
        let first = c
            .clusters
            .get(cluster_cursor)
            .ok_or("clusters per primitive exceeds cluster list")?;
        if primitive.index_start != first.index_offset {
            return invalid("clustered primitive index start disagrees with its first cluster");
        }
        let mut indices = 0u64;
        for _ in 0..cluster_count as usize {
            let cluster = c
                .clusters
                .get(cluster_cursor)
                .ok_or("clusters per primitive exceeds cluster list")?;
            if cluster.vertex_offset_dwords != c.vertex_offsets[primitive_index] {
                return invalid("cluster vertex offset disagrees with its primitive vertex offset");
            }
            for t in 0..cluster.index_count as usize {
                let raw = index_value(c.indices, width, cluster.index_offset as usize + t)
                    .ok_or("cluster index outside clustered index buffer")?;
                if base + raw as usize >= group_end {
                    return invalid("cluster index outside its primitive vertex group");
                }
            }
            indices += cluster.index_count as u64;
            cluster_cursor += 1;
        }
        if primitive.flag == 1 && primitive.value as u64 != indices {
            return invalid("clustered primitive index count disagrees with its clusters");
        }
        if primitive.flag == 0 && primitive.value as u64 != cluster_count as u64 {
            return invalid(
                "clustered primitive cluster count disagrees with clusters per primitive",
            );
        }
    }
    if cluster_cursor != c.clusters.len() {
        return invalid("clusters per primitive does not cover the cluster list");
    }
    Ok(())
}

fn clustered<'a>(r: &mut Reader<'a>, limits: Limits) -> Result<ClusteredData<'a>> {
    let file_id = header(r, 0xc351_ee43)?;
    let version = r.u32()?;
    if version != 1 {
        return invalid("unsupported clustered mesh data version");
    }
    let vertex_format = r.u8()?;
    let vertex_stride = r.u32()?;
    let cluster_count = r.u32()?;
    let center = floats(r)?;
    let half_extent = floats(r)?;
    let draw_primitive_count = r.u32()?;
    let clusters_per_primitive = integer_array(r, limits)?;
    let vertex_offsets = integer_array(r, limits)?;
    let fixed_cluster_size = boolean(r)?;
    let vertices = buffer(r, limits)?;
    let indices = buffer(r, limits)?;
    let descriptors = buffer(r, limits)?;
    let bit_mask = buffer(r, limits)?;
    let primitive_descriptors = buffer(r, limits)?;
    if vertex_stride == 0
        || vertex_stride > 255
        || !vertices.len().is_multiple_of(vertex_stride as usize)
        || cluster_count as usize > limits.max_resources
        || draw_primitive_count as usize > limits.max_resources
        || clusters_per_primitive.len() != draw_primitive_count as usize
        || vertex_offsets.len() != draw_primitive_count as usize
    {
        return invalid("invalid clustered mesh counts/stride");
    }
    let clusters = cluster_descriptors(descriptors, cluster_count as usize, limits)?;
    let expected_mask = (cluster_count as usize)
        .checked_mul(CLUSTER_BIT_MASK_BYTES)
        .ok_or("cluster bit mask size overflow")?;
    if bit_mask.len() != expected_mask {
        return invalid(format!(
            "unsupported cluster bit mask block: {} bytes for {cluster_count} clusters (expected {expected_mask})",
            bit_mask.len()
        ));
    }
    let draw_primitives =
        cluster_primitives(primitive_descriptors, draw_primitive_count as usize, limits)?;
    Ok(ClusteredData {
        file_id,
        version,
        vertex_format,
        vertex_stride,
        cluster_count,
        center,
        half_extent,
        draw_primitive_count,
        clusters_per_primitive,
        vertex_offsets,
        fixed_cluster_size,
        clusters,
        draw_primitives,
        vertices,
        indices,
        descriptors,
        bit_mask,
        primitive_descriptors,
    })
}

fn compiled<'a>(r: &mut Reader<'a>, limits: Limits) -> Result<CompiledMesh<'a>> {
    let file_id = header(r, 0xfc9e_1595)?;
    let data = buffer(r, limits)?;
    let clustered = match r.u8()? {
        0 => Some(clustered(r, limits)?),
        3 => None,
        _ => return invalid("unsupported clustered mesh pointer"),
    };
    let mesh_data = parse_data(r, limits)?;
    // Cluster-only variants keep empty non-clustered buffers; their primitive
    // spans must fit the compiled clustered index buffer, using the MeshData
    // index width. Both buffers are real archive bytes for this resource.
    if mesh_data.indices.is_empty()
        && let Some(clustered) = &clustered
    {
        let width = if mesh_data.index32 { 4 } else { 2 };
        let index_count = clustered.indices.len() / width;
        for p in mesh_data.standard.iter().chain(&mesh_data.shadow) {
            if !p.depth_only && p.kind == 1 {
                let end = (p.start_index as usize)
                    .checked_add(
                        (p.primitive_count as usize)
                            .checked_mul(3)
                            .ok_or("mesh triangle count overflow")?,
                    )
                    .ok_or("mesh index range overflow")?;
                if end > index_count {
                    return invalid("mesh primitive index range exceeds clustered buffer");
                }
            }
        }
    }
    let n = count(r, limits.max_resources, 278)?;
    let mut instancing = Vec::with_capacity(n);
    for _ in 0..n {
        let file_id = header(r, 0x0b1d_34c1)?;
        let shadow_caster = boolean(r)?;
        let submesh_index = r.u16()?;
        let material_type = r.u16()?;
        let bone_count = r.u16()? as usize;
        let vertex_count = r.u16()?;
        if bone_count > 128 || vertex_count > i16::MAX as u16 {
            return invalid("invalid mesh instancing bone/vertex count");
        }
        let material = reference_pointer(r)?;
        let mut hardware_to_mesh_bones = Vec::with_capacity(bone_count);
        for _ in 0..bone_count {
            hardware_to_mesh_bones.push(r.u16()?);
        }
        let bone_table_padding = r.take(256 - bone_count * 2)?.to_vec();
        instancing.push(InstancingData {
            file_id,
            shadow_caster,
            submesh_index,
            material_type,
            vertex_count,
            material,
            hardware_to_mesh_bones,
            bone_table_padding,
        });
    }
    let platform_version = r.u32()?;
    let sdk_version = r.u32()?;
    let quantization_factor = scalar(r)?;
    if let Some(clustered) = &clustered {
        validate_clustered(clustered, &mesh_data)?;
    }
    Ok(CompiledMesh {
        file_id,
        data,
        clustered,
        mesh_data,
        instancing,
        platform_version,
        sdk_version,
        quantization_factor,
    })
}

/// Parse the root graph for supported variants. The cluster bit mask bytes are
/// retained without recovered semantics; call `unresolved()` for coverage.
pub fn parse<'a>(payload: &'a [u8], expected_id: &str, limits: Limits) -> Result<Mesh<'a>> {
    if payload.len() > limits.max_decoded {
        return invalid("mesh payload exceeds configured limit");
    }
    let mut r = Reader::new(payload);
    if r.u8()? != 1 {
        return invalid("unsupported mesh root wrapper");
    }
    let file_id = header(&mut r, 0x415d_9568)?;
    if file_id != expected_id {
        return invalid("mesh root identity mismatch");
    }
    let descriptor_mask = r.u8()?;
    let category = r.u32()?;
    let generated = boolean(&mut r)?;
    let n = count(&mut r, limits.max_resources, 1)?;
    let mut submeshes = Vec::with_capacity(n);
    for _ in 0..n {
        submeshes.push(submesh_pointer(&mut r, limits, 0)?);
    }
    let n = count(&mut r, limits.max_resources, 81)?;
    let mut bones = Vec::with_capacity(n);
    for _ in 0..n {
        bones.push(MeshBone {
            file_id: header(&mut r, 0x9ef0_e7a1)?,
            name_hash: format!("{:08x}", r.u32()?),
            matrix: floats(&mut r)?,
            used_by_submeshes: boolean(&mut r)?,
        });
    }
    let local_extent_min = floats(&mut r)?;
    let local_extent_max = floats(&mut r)?;
    if r.u8()? != 0 {
        return invalid("unsupported compiled mesh pointer");
    }
    let compiled = compiled(&mut r, limits)?;
    let n = count(&mut r, limits.max_resources, 10)?;
    let mut materials = Vec::with_capacity(n);
    for _ in 0..n {
        materials.push(file_reference(&mut r)?);
    }
    let mut flags = [false; 9];
    for flag in &mut flags {
        *flag = boolean(&mut r)?;
    }
    let dynamic_vertex_count = r.u32()?;
    let dynamic_index_count = r.u32()?;
    let user_category = r.u32()?;
    if r.remaining() != 0 {
        return invalid("trailing mesh resource bytes");
    }
    Ok(Mesh {
        file_id,
        descriptor_mask,
        category,
        generated,
        submeshes,
        bones,
        local_extent_min,
        local_extent_max,
        compiled,
        materials,
        flags,
        dynamic_vertex_count,
        dynamic_index_count,
        user_category,
    })
}

impl Mesh<'_> {
    /// Decode presentation geometry split by serialized draw primitive: one
    /// `DrawPart` per drawn primitive, in draw order, holding absolute decoded
    /// vertex indices and the triangles of `Mesh::decode` grouped per part.
    ///
    /// Material binding follows the observed instancing layout (ACU-RE-0013):
    /// the instancing records are serialized one per drawn primitive, in the
    /// same order, so a record binds to the part at its own position when its
    /// `vertex_count` equals that part's vertex count. Any other shape leaves
    /// `material` as `None` instead of guessing a pairing.
    pub fn decode_parts(&self, limits: Limits) -> Result<Vec<DrawPart>> {
        Ok(self.decode_split(limits)?.1)
    }

    /// Single decode pass shared by `decode` and `decode_parts`.
    fn decode_split(&self, limits: Limits) -> Result<(crate::vertex::Geometry, Vec<DrawPart>)> {
        let mesh_data = &self.compiled.mesh_data;
        let (vertex_bytes, format, stride, vertex_source) = if !mesh_data.vertices.is_empty() {
            (
                mesh_data.vertices,
                mesh_data.vertex_format,
                mesh_data.vertex_stride,
                "mesh_data",
            )
        } else if let Some(clustered) = &self.compiled.clustered {
            let stride = u8::try_from(clustered.vertex_stride)
                .map_err(|_| "clustered vertex stride exceeds u8")?;
            (
                clustered.vertices,
                clustered.vertex_format,
                stride,
                "clustered",
            )
        } else {
            return invalid("mesh has no vertex buffer");
        };
        let vertices = crate::vertex::decode(
            vertex_bytes,
            format,
            stride,
            self.compiled.quantization_factor,
            limits,
        )?;
        let width = if mesh_data.index32 { 4usize } else { 2 };
        let mut parts: Vec<DrawPart> = Vec::new();
        let (index_source, topology_source);
        if mesh_data.indices.is_empty() {
            let clustered = self
                .compiled
                .clustered
                .as_ref()
                .ok_or("mesh has no index buffer")?;
            // One part per serialized draw primitive, before any cluster walk,
            // so zero-cluster primitives cannot shift the material pairing.
            for primitive in 0..clustered.draw_primitives.len() {
                let (base, count) = clustered
                    .vertex_group(primitive, vertices.len())
                    .ok_or("clustered vertex group exceeds vertex buffer")?;
                parts.push(DrawPart {
                    primitive,
                    vertex_base: base,
                    vertex_count: count,
                    material: None,
                    triangles: Vec::new(),
                });
            }
            for (cluster_index, cluster) in clustered.clusters.iter().enumerate() {
                let primitive = clustered
                    .primitive_of_cluster(cluster_index)
                    .ok_or("cluster primitive mapping exceeds draw primitive list")?;
                let part = parts
                    .get_mut(primitive)
                    .ok_or("cluster primitive outside draw primitive list")?;
                let base = part.vertex_base;
                let start = cluster.index_offset as usize;
                let end = start
                    .checked_add(cluster.index_count as usize)
                    .ok_or("cluster index span overflow")?;
                if end > clustered.index_count(width) {
                    return invalid("cluster index span exceeds clustered index buffer");
                }
                let mut n = start;
                while n + 3 <= end {
                    let mut corner = [0u32; 3];
                    for (slot, value) in corner.iter_mut().enumerate() {
                        let raw = index_value(clustered.indices, width, n + slot)
                            .ok_or("cluster index outside buffer")?;
                        *value = u32::try_from(
                            base.checked_add(raw as usize)
                                .ok_or("cluster vertex index overflow")?,
                        )?;
                    }
                    if corner.iter().any(|&i| i as usize >= vertices.len()) {
                        return invalid("cluster index outside vertex buffer");
                    }
                    part.triangles.push(corner);
                    n += 3;
                }
            }
            index_source = "clustered";
            topology_source = "cluster_descriptors";
        } else {
            let index_bytes = mesh_data.indices;
            let index_count = index_bytes.len() / width;
            let (list, source) = if mesh_data
                .standard
                .iter()
                .any(|p| !p.depth_only && p.kind == 1)
            {
                (&mesh_data.standard, "standard")
            } else {
                (&mesh_data.shadow, "shadow")
            };
            for p in list.iter().filter(|p| !p.depth_only && p.kind == 1) {
                let end = (p.start_index as usize)
                    .checked_add(
                        (p.primitive_count as usize)
                            .checked_mul(3)
                            .ok_or("mesh triangle count overflow")?,
                    )
                    .ok_or("mesh index range overflow")?;
                if end > index_count {
                    return invalid("mesh primitive index range exceeds index buffer");
                }
                let mut triangles = Vec::with_capacity(p.primitive_count as usize);
                let mut n = p.start_index as usize;
                while n + 3 <= end {
                    let (a, b, c) = (
                        index_value(index_bytes, width, n).ok_or("mesh index outside buffer")?,
                        index_value(index_bytes, width, n + 1)
                            .ok_or("mesh index outside buffer")?,
                        index_value(index_bytes, width, n + 2)
                            .ok_or("mesh index outside buffer")?,
                    );
                    if [a, b, c].iter().any(|&i| i as usize >= vertices.len()) {
                        return invalid("mesh index outside vertex buffer");
                    }
                    triangles.push([a, b, c]);
                    n += 3;
                }
                parts.push(DrawPart {
                    primitive: parts.len(),
                    vertex_base: p.min_index as usize,
                    vertex_count: p.vertex_count as usize,
                    material: None,
                    triangles,
                });
            }
            index_source = "mesh_data";
            topology_source = source;
        }
        if self.compiled.instancing.len() == parts.len() {
            for (primitive, part) in parts.iter_mut().enumerate() {
                let record = &self.compiled.instancing[primitive];
                if usize::from(record.vertex_count) != part.vertex_count {
                    continue;
                }
                if let Some(file_id) = &record.material {
                    part.material = Some(DrawPartMaterial {
                        file_id: file_id.clone(),
                        submesh_index: record.submesh_index,
                        material_type: record.material_type,
                    });
                }
            }
        }
        let triangles = parts
            .iter()
            .flat_map(|p| p.triangles.iter().copied())
            .collect();
        Ok((
            crate::vertex::Geometry {
                vertices,
                triangles,
                vertex_source,
                index_source,
                topology_source,
            },
            parts,
        ))
    }

    /// Decode presentation geometry through the recovered attribute layouts.
    ///
    /// Cluster-only variants (empty non-clustered buffers) take index spans and
    /// vertex bases from the typed cluster descriptors; `MeshData` primitive
    /// spans describe the original non-clustered buffers and disagree with the
    /// clustered layout on fixed-size cluster meshes (ACU-RE-0012). Index width
    /// comes from `MeshData`, which both variants serialize. See ACU-RE-0011.
    pub fn decode(&self, limits: Limits) -> Result<crate::vertex::Geometry> {
        Ok(self.decode_split(limits)?.0)
    }

    /// Per-cluster numeric audit of the recovered conventions (ACU-RE-0012):
    /// native-axis bounding-box fit and normal-cone fit, measured from the
    /// decoded clustered buffers. One row per cluster in descriptor order.
    pub fn audit_clusters(&self, limits: Limits) -> Result<Vec<ClusterAudit>> {
        let clustered = self
            .compiled
            .clustered
            .as_ref()
            .ok_or("mesh has no clustered block")?;
        let stride = u8::try_from(clustered.vertex_stride)
            .map_err(|_| "clustered vertex stride exceeds u8")?;
        let vertices = crate::vertex::decode(
            clustered.vertices,
            clustered.vertex_format,
            stride,
            self.compiled.quantization_factor,
            limits,
        )?;
        let width = if self.compiled.mesh_data.index32 {
            4usize
        } else {
            2
        };
        let mut rows = Vec::with_capacity(clustered.clusters.len());
        for (cluster_index, cluster) in clustered.clusters.iter().enumerate() {
            let primitive = clustered
                .primitive_of_cluster(cluster_index)
                .ok_or("cluster primitive mapping exceeds draw primitive list")?;
            let base = clustered
                .vertex_base(primitive)
                .ok_or("clustered vertex offset exceeds stride")?;
            let start = cluster.index_offset as usize;
            let end = start
                .checked_add(cluster.index_count as usize)
                .ok_or("cluster index span overflow")?;
            let mut distinct = std::collections::BTreeSet::new();
            let mut aabb_fit = 0f32;
            for n in start..end {
                let raw = index_value(clustered.indices, width, n)
                    .ok_or("cluster index outside buffer")?;
                let index = base
                    .checked_add(raw as usize)
                    .ok_or("cluster vertex index overflow")?;
                let position = vertices
                    .get(index)
                    .ok_or("cluster index outside vertex buffer")?
                    .position;
                distinct.insert(index);
                for (k, value) in position.iter().enumerate() {
                    let delta = (value - cluster.center[k]).abs();
                    let extent = cluster.half_extent[k];
                    let ratio = if extent > 0.0 {
                        delta / extent
                    } else if delta == 0.0 {
                        0.0
                    } else {
                        f32::INFINITY
                    };
                    aabb_fit = aabb_fit.max(ratio);
                }
            }
            let mut triangles = 0usize;
            let mut cone_max_angle = 0f32;
            let mut n = start;
            while n + 3 <= end {
                let corner: Vec<[f32; 3]> = (0..3)
                    .map(|slot| {
                        let raw = index_value(clustered.indices, width, n + slot)?;
                        let index = base.checked_add(raw as usize)?;
                        vertices.get(index).map(|v| v.position)
                    })
                    .collect::<Option<Vec<_>>>()
                    .ok_or("cluster index outside vertex buffer")?;
                triangles += 1;
                let edge0 = [
                    corner[1][0] - corner[0][0],
                    corner[1][1] - corner[0][1],
                    corner[1][2] - corner[0][2],
                ];
                let edge1 = [
                    corner[2][0] - corner[0][0],
                    corner[2][1] - corner[0][1],
                    corner[2][2] - corner[0][2],
                ];
                let normal = [
                    edge0[1] * edge1[2] - edge0[2] * edge1[1],
                    edge0[2] * edge1[0] - edge0[0] * edge1[2],
                    edge0[0] * edge1[1] - edge0[1] * edge1[0],
                ];
                let length =
                    (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
                if length > 0.0 {
                    let axis = cluster.cone_axis;
                    let axis_length =
                        (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
                    if axis_length > 0.0 {
                        let dot = (normal[0] * axis[0] + normal[1] * axis[1] + normal[2] * axis[2])
                            / (length * axis_length);
                        cone_max_angle = cone_max_angle.max(dot.clamp(-1.0, 1.0).acos());
                    }
                }
                n += 3;
            }
            rows.push(ClusterAudit {
                cluster: cluster_index,
                primitive,
                vertex_base: base,
                distinct_vertices: distinct.len(),
                triangles,
                aabb_fit,
                cone_max_angle,
                cone_angle: cluster.cone_angle,
            });
        }
        Ok(rows)
    }

    /// An empty list only establishes typed field coverage for this parser's
    /// accepted variants, not reference resolution, rendering or game parity.
    pub fn unresolved(&self) -> Vec<&'static str> {
        let mut result = Vec::new();
        if !self.compiled.data.is_empty() {
            result.push("opaque compiled data");
        }
        if self.compiled.clustered.is_some() {
            result.push("cluster bit mask semantics");
        }
        if !self.compiled.mesh_data.vertices.is_empty() {
            if crate::vertex::layout(
                self.compiled.mesh_data.vertex_format,
                self.compiled.mesh_data.vertex_stride,
            )
            .is_none()
            {
                result.push("vertex attribute format");
            }
        } else if let Some(clustered) = &self.compiled.clustered {
            let stride = u8::try_from(clustered.vertex_stride).unwrap_or(u8::MAX);
            if crate::vertex::layout(clustered.vertex_format, stride).is_none() {
                result.push("vertex attribute format");
            }
        }
        if self
            .submeshes
            .iter()
            .any(|p| matches!(p, SubMeshPointer::Reference(_)))
        {
            result.push("external submesh graph");
        }
        if self
            .submeshes
            .iter()
            .any(SubMeshPointer::external_processed)
        {
            result.push("external processed submesh graph");
        }
        if self
            .compiled
            .mesh_data
            .standard
            .iter()
            .chain(&self.compiled.mesh_data.shadow)
            .any(|p| p.depth_only || p.kind != 1)
        {
            result.push("topology/depth-only buffers");
        }
        result
    }
}

impl SubMeshPointer {
    fn external_processed(&self) -> bool {
        match self {
            Self::Inline(s) => {
                matches!(s.processed_mesh, Self::Reference(_))
                    || s.processed_mesh.external_processed()
            }
            _ => false,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Primitive {
    pub file_id: String,
    pub min_index: u32,
    pub depth_only: bool,
    pub vertex_count: u32,
    pub start_index: u32,
    pub primitive_count: u32,
    /// Native value; 1 is the observed triangle-list variant.
    pub kind: u32,
}

#[derive(Debug, Serialize)]
pub struct MeshData<'a> {
    pub file_id: String,
    pub index32: bool,
    pub vertex_format: u8,
    pub vertex_stride: u8,
    pub standard: Vec<Primitive>,
    pub shadow: Vec<Primitive>,
    #[serde(skip)]
    pub vertices: &'a [u8],
    #[serde(skip)]
    pub indices: &'a [u8],
}

fn count(r: &mut Reader<'_>, limit: usize, width: usize) -> Result<usize> {
    let n = r.u32()? as usize;
    if n > i32::MAX as usize || n > limit || n > r.remaining() / width {
        return invalid("mesh count exceeds input or configured limit");
    }
    Ok(n)
}

/// Read one index value with the `MeshData` width (2 or 4 bytes).
fn index_value(bytes: &[u8], width: usize, n: usize) -> Option<u32> {
    let at = n.checked_mul(width)?;
    match width {
        2 => Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?) as u32),
        _ => Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?)),
    }
}

fn boolean(r: &mut Reader<'_>) -> Result<bool> {
    match r.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => invalid("invalid mesh boolean"),
    }
}

fn header(r: &mut Reader<'_>, expected: u32) -> Result<String> {
    let id = format!("{:016x}", r.u64()?);
    if r.u32()? != expected {
        return invalid("unexpected mesh class hash");
    }
    Ok(id)
}

fn primitives(r: &mut Reader<'_>, limits: Limits) -> Result<Vec<Primitive>> {
    let n = count(r, limits.max_resources, 36)?;
    let mut result = Vec::with_capacity(n);
    for _ in 0..n {
        let file_id = header(r, CLASS_MESH_PRIMITIVE)?;
        let min_index = r.u32()?;
        let depth_only = match r.u32()? {
            0 => false,
            1 => true,
            _ => return invalid("invalid mesh depth-only flag"),
        };
        let vertex_count = r.u32()?;
        let start_index = r.u32()?;
        let primitive_count = r.u32()?;
        let kind = r.u32()?;
        if [min_index, vertex_count, start_index, primitive_count]
            .iter()
            .any(|&v| v > i32::MAX as u32)
        {
            return invalid("negative mesh primitive field");
        }
        result.push(Primitive {
            file_id,
            min_index,
            depth_only,
            vertex_count,
            start_index,
            primitive_count,
            kind,
        });
    }
    Ok(result)
}

/// Read an embedded class, beginning at its u64 ID and u32 class hash.
/// Caller retains responsibility for the containing object's boundary.
pub fn parse_data<'a>(r: &mut Reader<'a>, limits: Limits) -> Result<MeshData<'a>> {
    let file_id = header(r, CLASS_MESH_DATA)?;
    let index32 = boolean(r)?;
    let vertex_format = r.u8()?;
    let vertex_stride = r.u8()?;
    if vertex_stride == 0 {
        return invalid("zero mesh vertex stride");
    }
    let standard = primitives(r, limits)?;
    let shadow = primitives(r, limits)?;
    let n = count(r, limits.max_decoded, 1)?;
    let vertices = r.take(n)?;
    let n = count(r, limits.max_decoded.saturating_sub(vertices.len()), 1)?;
    let indices = r.take(n)?;
    if !vertices.len().is_multiple_of(vertex_stride as usize)
        || !indices.len().is_multiple_of(if index32 { 4 } else { 2 })
    {
        return invalid("mesh buffer length is not a multiple of element size");
    }
    let mesh = MeshData {
        file_id,
        index32,
        vertex_format,
        vertex_stride,
        standard,
        shadow,
        vertices,
        indices,
    };
    // Depth-only primitives may address other buffers. Unsupported topology values
    // and clustered draw offsets are preserved, not guessed or validated as lists.
    // Variants with emptied non-clustered buffers keep the geometry in the compiled
    // clustered block; `compiled` validates those primitive spans.
    for p in mesh.standard.iter().chain(&mesh.shadow) {
        if !p.depth_only && p.kind == 1 && !mesh.indices.is_empty() {
            let end = (p.start_index as usize)
                .checked_add(
                    (p.primitive_count as usize)
                        .checked_mul(3)
                        .ok_or("mesh triangle count overflow")?,
                )
                .ok_or("mesh index range overflow")?;
            if end > mesh.index_count() {
                return invalid("mesh primitive index range exceeds buffer");
            }
        }
    }
    Ok(mesh)
}

pub fn parse_data_exact(bytes: &[u8], limits: Limits) -> Result<MeshData<'_>> {
    let mut r = Reader::new(bytes);
    let data = parse_data(&mut r, limits)?;
    if r.remaining() != 0 {
        return invalid("trailing compiled MeshData bytes");
    }
    Ok(data)
}

/// Raw native packed attributes for format 4, with no inferred world transform.
#[derive(Debug, PartialEq)]
pub struct PackedVertex4 {
    pub position: [i16; 3],
    pub position_aux: i16,
    pub normal_color: [u8; 4],
    pub tangent_color: [u8; 4],
    pub uv: [i16; 2],
}

impl MeshData<'_> {
    pub fn vertex_count(&self) -> usize {
        self.vertices.len() / self.vertex_stride as usize
    }

    pub fn index_count(&self) -> usize {
        self.indices.len() / if self.index32 { 4 } else { 2 }
    }

    pub fn index(&self, n: usize) -> Option<u32> {
        if self.index32 {
            let at = n.checked_mul(4)?;
            Some(u32::from_le_bytes(
                self.indices.get(at..at.checked_add(4)?)?.try_into().ok()?,
            ))
        } else {
            let at = n.checked_mul(2)?;
            Some(
                u16::from_le_bytes(self.indices.get(at..at.checked_add(2)?)?.try_into().ok()?)
                    as u32,
            )
        }
    }

    pub fn packed_vertex4(&self, n: usize) -> Result<PackedVertex4> {
        if self.vertex_format != 4 || self.vertex_stride != 20 {
            return invalid("unsupported packed vertex format/stride");
        }
        let at = n.checked_mul(20).ok_or("mesh vertex offset overflow")?;
        let bytes = self
            .vertices
            .get(at..at.checked_add(20).ok_or("mesh vertex end overflow")?)
            .ok_or("mesh vertex outside buffer")?;
        let s = |i| i16::from_le_bytes([bytes[i], bytes[i + 1]]);
        Ok(PackedVertex4 {
            position: [s(0), s(2), s(4)],
            position_aux: s(6),
            normal_color: bytes[8..12].try_into()?,
            tangent_color: bytes[12..16].try_into()?,
            uv: [s(16), s(18)],
        })
    }
}
