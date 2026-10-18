use acu_assets::{Limits, Result, mesh};

fn fixture(index32: bool) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend(7_u64.to_le_bytes());
    b.extend(mesh::CLASS_MESH_DATA.to_le_bytes());
    b.extend([u8::from(index32), 4, 20]);
    b.extend(1_u32.to_le_bytes());
    b.extend(0_u64.to_le_bytes());
    b.extend(mesh::CLASS_MESH_PRIMITIVE.to_le_bytes());
    for x in [0_u32, 0, 1, 0, 1, 1] {
        b.extend(x.to_le_bytes());
    }
    b.extend(0_u32.to_le_bytes());
    b.extend(20_u32.to_le_bytes());
    for x in [123_i16, -456, 32767, -1] {
        b.extend(x.to_le_bytes());
    }
    b.extend([1, 2, 3, 4, 5, 6, 7, 8]);
    b.extend(2048_i16.to_le_bytes());
    b.extend((-4096_i16).to_le_bytes());
    b.extend((if index32 { 12_u32 } else { 6 }).to_le_bytes());
    b.extend(vec![0; if index32 { 12 } else { 6 }]);
    b
}

fn root_fixture() -> Vec<u8> {
    let mut b = vec![1];
    b.extend(7_u64.to_le_bytes());
    b.extend(0x415d9568_u32.to_le_bytes());
    b.push(0); // Descriptor mask.
    b.extend(5_u32.to_le_bytes());
    b.push(0); // Generated.
    b.extend(0_u32.to_le_bytes()); // Submeshes.
    b.extend(1_u32.to_le_bytes()); // Bones.
    b.extend(8_u64.to_le_bytes());
    b.extend(0x9ef0e7a1_u32.to_le_bytes());
    b.extend(0x12345678_u32.to_le_bytes());
    for x in 0..16 {
        b.extend((x as f32).to_le_bytes());
    }
    b.push(1); // Bone used by submesh.
    for x in [-1_f32, -2., -3., 0., 1., 2., 3., 0.] {
        b.extend(x.to_le_bytes());
    }
    b.push(0); // Inline compiled object.
    b.extend(9_u64.to_le_bytes());
    b.extend(0xfc9e1595_u32.to_le_bytes());
    b.extend(0_u32.to_le_bytes()); // Compiled blob.
    b.push(3); // No clustered data.
    b.extend(fixture(false));
    b.extend(0_u32.to_le_bytes()); // Instancing.
    b.extend(28_u32.to_le_bytes());
    b.extend(7_u32.to_le_bytes());
    b.extend(2_f32.to_le_bytes());
    b.extend(1_u32.to_le_bytes()); // Materials.
    b.extend([1, 0]);
    b.extend(0x33_u64.to_le_bytes());
    b.extend([0; 9]); // Flags.
    b.extend([0; 12]); // Dynamic counts/user category.
    b
}

#[test]
fn root_graph_fields_and_boundaries() -> Result<()> {
    let b = root_fixture();
    let graph = mesh::parse(&b, "0000000000000007", Limits::default())?;
    assert_eq!(graph.category, 5);
    assert_eq!(graph.bones[0].file_id, "0000000000000008");
    assert_eq!(graph.bones[0].name_hash, "12345678");
    assert_eq!(graph.bones[0].matrix[11], 11.);
    assert!(graph.bones[0].used_by_submeshes);
    assert_eq!(graph.local_extent_min, [-1., -2., -3., 0.]);
    assert_eq!(graph.local_extent_max, [1., 2., 3., 0.]);
    assert_eq!(graph.materials[0].file_id, "0000000000000033");
    assert!(graph.unresolved().is_empty());
    assert!(mesh::parse(&b, "0000000000000008", Limits::default()).is_err());
    for n in 0..b.len() {
        assert!(mesh::parse(&b[..n], "0000000000000007", Limits::default()).is_err());
    }
    let mut bad = b.clone();
    bad.push(0);
    assert!(mesh::parse(&bad, "0000000000000007", Limits::default()).is_err());
    // First serialized bone matrix value is nonfinite.
    let mut bad = b.clone();
    bad[43..47].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(mesh::parse(&bad, "0000000000000007", Limits::default()).is_err());
    Ok(())
}

#[test]
fn typed_mesh_fields_and_packed_attributes() -> Result<()> {
    assert_eq!(mesh::TYPE_MESH, "415d9568");
    for wide in [false, true] {
        let b = fixture(wide);
        let data = mesh::parse_data_exact(&b, Limits::default())?;
        assert_eq!(data.file_id, "0000000000000007");
        assert_eq!(data.vertex_count(), 1);
        assert_eq!(data.index_count(), 3);
        assert_eq!(data.index(2), Some(0));
        assert_eq!(data.index(3), None);
        assert_eq!(data.index(usize::MAX), None);
        let v = data.packed_vertex4(0)?;
        assert_eq!(v.position, [123, -456, 32767]);
        assert_eq!(v.position_aux, -1);
        assert_eq!(v.normal_color, [1, 2, 3, 4]);
        assert_eq!(v.tangent_color, [5, 6, 7, 8]);
        assert_eq!(v.uv, [2048, -4096]);
        assert!(data.packed_vertex4(1).is_err());
        assert!(data.packed_vertex4(usize::MAX).is_err());
    }
    Ok(())
}

#[test]
fn mesh_boundaries_reject_malformed_inputs() {
    let b = fixture(false);
    for end in 0..b.len() {
        assert!(mesh::parse_data_exact(&b[..end], Limits::default()).is_err());
    }
    for (at, value) in [(8, 0_u32), (15, u32::MAX), (27, 0), (47, 2)] {
        let mut bad = b.clone();
        bad[at..at + 4].copy_from_slice(&value.to_le_bytes());
        assert!(mesh::parse_data_exact(&bad, Limits::default()).is_err());
    }
    for at in [12, 13, 14] {
        let mut bad = b.clone();
        bad[at] = if at == 12 { 2 } else { 0 };
        if at != 13 {
            assert!(mesh::parse_data_exact(&bad, Limits::default()).is_err());
        }
    }
    let mut trailing = b.clone();
    trailing.push(0);
    assert!(mesh::parse_data_exact(&trailing, Limits::default()).is_err());
    assert!(
        mesh::parse_data_exact(
            &b,
            Limits {
                max_decoded: 20,
                ..Limits::default()
            }
        )
        .is_err()
    );
}

/// Synthetic clustered block shaped like the real one: one vertex group,
/// one cluster, one triangle, typed descriptors. Not archive bytes.
struct ClusterFixture {
    vertex_offset_dwords: u32,
    /// u16 entries written to the clustered index buffer.
    index_entries: u32,
    /// Index count declared by the cluster descriptor.
    cluster_index_count: u32,
    cone_angle: f32,
    half_extent: [f32; 3],
    prim_flag: u8,
    prim_value: u32,
    prim_index_start: u32,
    /// Declared byte lengths of the descriptor/bit-mask/primitive blocks.
    lengths: [u32; 3],
    /// Optional instancing record: (submesh index, vertex count, material ID).
    instancing: Option<(u16, u16, &'static str)>,
}

impl Default for ClusterFixture {
    fn default() -> Self {
        Self {
            vertex_offset_dwords: 0,
            index_entries: 3,
            cluster_index_count: 3,
            cone_angle: std::f32::consts::PI,
            half_extent: [1., 1., 1.],
            prim_flag: 0,
            prim_value: 1,
            prim_index_start: 0,
            lengths: [52, 48, 36],
            instancing: None,
        }
    }
}

fn clustered_fixture(f: &ClusterFixture) -> Vec<u8> {
    let mut b = vec![1];
    b.extend(7_u64.to_le_bytes());
    b.extend(0x415d9568_u32.to_le_bytes());
    b.push(0); // Descriptor mask.
    b.extend(5_u32.to_le_bytes()); // Category.
    b.push(0); // Generated.
    b.extend(0_u32.to_le_bytes()); // Submeshes.
    b.extend(0_u32.to_le_bytes()); // Bones.
    for x in [-1_f32, -1., -1., 0., 1., 1., 1., 0.] {
        b.extend(x.to_le_bytes());
    }
    b.push(0); // Inline compiled object.
    b.extend(9_u64.to_le_bytes());
    b.extend(0xfc9e1595_u32.to_le_bytes());
    b.extend(0_u32.to_le_bytes()); // Compiled blob.
    b.push(0); // Inline clustered data.
    b.extend(10_u64.to_le_bytes());
    b.extend(0xc351ee43_u32.to_le_bytes());
    b.extend(1_u32.to_le_bytes()); // Version.
    b.extend([4_u8]);
    b.extend(20_u32.to_le_bytes()); // Stride.
    b.extend(1_u32.to_le_bytes()); // Clusters.
    for x in [0_f32, 0., 0., 1., 1., 1.] {
        b.extend(x.to_le_bytes());
    }
    b.extend(1_u32.to_le_bytes()); // Draw primitives.
    b.extend(1_u32.to_le_bytes()); // Clusters per primitive count.
    b.extend(1_u32.to_le_bytes()); // Clusters in the primitive.
    b.extend(1_u32.to_le_bytes()); // Vertex offset count.
    b.extend(f.vertex_offset_dwords.to_le_bytes()); // Vertex offsets.
    b.push(1); // Fixed cluster size.
    b.extend(60_u32.to_le_bytes()); // Clustered vertices (3 x 20).
    for _ in 0..3 {
        for x in [0_i16, 0, 0, 1] {
            b.extend(x.to_le_bytes());
        }
        b.extend([127, 127, 127, 0, 127, 127, 127, 0]);
        b.extend(0_i16.to_le_bytes());
        b.extend(0_i16.to_le_bytes());
    }
    b.extend((f.index_entries * 2).to_le_bytes());
    for i in 0..f.index_entries {
        b.extend(((i % 3) as u16).to_le_bytes());
    }
    // One 52-byte cluster descriptor.
    b.extend(f.lengths[0].to_le_bytes());
    for x in [0_f32, 0., 0.] {
        b.extend(x.to_le_bytes()); // Center.
    }
    for x in f.half_extent {
        b.extend(x.to_le_bytes());
    }
    for x in [1_f32, 0., 0.] {
        b.extend(x.to_le_bytes()); // Cone axis.
    }
    b.extend(f.cone_angle.to_le_bytes());
    b.extend(0_u32.to_le_bytes()); // Index offset.
    b.extend(f.cluster_index_count.to_le_bytes());
    b.extend(f.vertex_offset_dwords.to_le_bytes());
    // One 48-byte per-cluster bit mask (semantics unresolved; opaque bytes).
    b.extend(f.lengths[1].to_le_bytes());
    b.extend(vec![0xFF; 48]);
    // One 36-byte draw-primitive descriptor.
    b.extend(f.lengths[2].to_le_bytes());
    for x in [0_f32, 0., 0.] {
        b.extend(x.to_le_bytes());
    }
    for x in [1_f32, 1., 1.] {
        b.extend(x.to_le_bytes());
    }
    b.extend(20_u32.to_le_bytes()); // Stride.
    b.extend((f.prim_flag as u32 | (f.prim_value << 8)).to_le_bytes());
    b.extend(f.prim_index_start.to_le_bytes());
    // MeshData with empty non-clustered buffers.
    b.extend(11_u64.to_le_bytes());
    b.extend(mesh::CLASS_MESH_DATA.to_le_bytes());
    b.extend([0_u8, 4, 20]);
    for _ in 0..2 {
        b.extend(1_u32.to_le_bytes());
        b.extend(12_u64.to_le_bytes());
        b.extend(mesh::CLASS_MESH_PRIMITIVE.to_le_bytes());
        for x in [0_u32, 0, 1, 0, 1, 1] {
            b.extend(x.to_le_bytes());
        }
    }
    b.extend(0_u32.to_le_bytes()); // Vertex bytes.
    b.extend(0_u32.to_le_bytes()); // Index bytes.
    // Instancing: one record per drawn primitive when requested.
    match f.instancing {
        Some((submesh_index, vertex_count, material)) => {
            b.extend(1_u32.to_le_bytes());
            b.extend(13_u64.to_le_bytes());
            b.extend(0x0b1d34c1_u32.to_le_bytes());
            b.push(0); // Shadow caster.
            b.extend(submesh_index.to_le_bytes());
            b.extend(1_u16.to_le_bytes()); // Material type.
            b.extend(0_u16.to_le_bytes()); // Bone count.
            b.extend(vertex_count.to_le_bytes());
            b.push(1); // Material pointer present.
            b.extend(u64::from_str_radix(material, 16).unwrap().to_le_bytes());
            b.extend([0_u8; 256]); // Bone table padding.
        }
        None => {
            b.extend(0_u32.to_le_bytes());
        }
    }
    b.extend(28_u32.to_le_bytes());
    b.extend(7_u32.to_le_bytes());
    b.extend(2_f32.to_le_bytes());
    b.extend(0_u32.to_le_bytes()); // Materials.
    b.extend([0; 9]); // Flags.
    b.extend([0; 12]); // Dynamic counts/user category.
    b
}

#[test]
fn clustered_descriptors_drive_cluster_only_geometry() -> Result<()> {
    let b = clustered_fixture(&ClusterFixture::default());
    let graph = mesh::parse(&b, "0000000000000007", Limits::default())?;
    let cluster = graph.compiled.clustered.as_ref().unwrap();
    assert_eq!(cluster.clusters.len(), 1);
    assert_eq!(cluster.clusters[0].index_count, 3);
    assert_eq!(cluster.clusters[0].vertex_offset_dwords, 0);
    assert_eq!(cluster.draw_primitives[0].flag, 0);
    assert_eq!(cluster.draw_primitives[0].value, 1);
    assert!(graph.compiled.mesh_data.indices.is_empty());
    assert_eq!(graph.unresolved(), ["cluster bit mask semantics"]);
    let geometry = graph.decode(Limits::default())?;
    assert_eq!(geometry.vertex_source, "clustered");
    assert_eq!(geometry.index_source, "clustered");
    assert_eq!(geometry.topology_source, "cluster_descriptors");
    assert_eq!(geometry.triangles, [[0, 1, 2]]);
    let audit = graph.audit_clusters(Limits::default())?;
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].distinct_vertices, 3);
    assert_eq!(audit[0].triangles, 1);
    assert_eq!(audit[0].aabb_fit, 0.0);
    Ok(())
}

#[test]
fn decode_parts_group_triangles_and_bind_instancing_materials() -> Result<()> {
    // One drawn primitive, one triangle, no instancing record: the part exists
    // but carries no material, and flattening parts reproduces `decode`.
    let b = clustered_fixture(&ClusterFixture::default());
    let graph = mesh::parse(&b, "0000000000000007", Limits::default())?;
    let parts = graph.decode_parts(Limits::default())?;
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].primitive, 0);
    assert_eq!(parts[0].vertex_base, 0);
    assert_eq!(parts[0].vertex_count, 3);
    assert_eq!(parts[0].triangles, [[0, 1, 2]]);
    assert_eq!(parts[0].material, None);
    let geometry = graph.decode(Limits::default())?;
    assert_eq!(
        parts
            .iter()
            .flat_map(|p| p.triangles.iter().copied())
            .collect::<Vec<_>>(),
        geometry.triangles
    );

    // A record whose vertex count agrees binds by position.
    let bound_fixture = clustered_fixture(&ClusterFixture {
        instancing: Some((2, 3, "0000000b1b911534")),
        ..ClusterFixture::default()
    });
    let bound = mesh::parse(&bound_fixture, "0000000000000007", Limits::default())?;
    let parts = bound.decode_parts(Limits::default())?;
    assert_eq!(parts.len(), 1);
    let material = parts[0].material.as_ref().unwrap();
    assert_eq!(material.file_id, "0000000b1b911534");
    assert_eq!(material.submesh_index, 2);
    assert_eq!(material.material_type, 1);

    // A record whose vertex count disagrees is not paired: no guess.
    let mismatched_fixture = clustered_fixture(&ClusterFixture {
        instancing: Some((0, 2, "0000000b1b911534")),
        ..ClusterFixture::default()
    });
    let mismatched = mesh::parse(&mismatched_fixture, "0000000000000007", Limits::default())?;
    assert_eq!(
        mismatched.decode_parts(Limits::default())?[0].material,
        None
    );
    Ok(())
}

#[test]
fn clustered_descriptor_variants_fail_loudly() {
    let limits = Limits::default();
    let mutations: Vec<(&str, ClusterFixture)> = vec![
        (
            "descriptor block size",
            ClusterFixture {
                lengths: [51, 48, 36],
                ..ClusterFixture::default()
            },
        ),
        (
            "bit mask block size",
            ClusterFixture {
                lengths: [52, 49, 36],
                ..ClusterFixture::default()
            },
        ),
        (
            "primitive block size",
            ClusterFixture {
                lengths: [52, 48, 35],
                ..ClusterFixture::default()
            },
        ),
        (
            "cluster span below buffer",
            ClusterFixture {
                cluster_index_count: 2,
                ..ClusterFixture::default()
            },
        ),
        (
            "cluster span above buffer",
            ClusterFixture {
                cluster_index_count: 4,
                ..ClusterFixture::default()
            },
        ),
        (
            "cone angle outside [0, PI]",
            ClusterFixture {
                cone_angle: 4.0,
                ..ClusterFixture::default()
            },
        ),
        (
            "negative half extent",
            ClusterFixture {
                half_extent: [-1., 1., 1.],
                ..ClusterFixture::default()
            },
        ),
        (
            "flag disagrees with fixed size",
            ClusterFixture {
                prim_flag: 1,
                prim_value: 3,
                ..ClusterFixture::default()
            },
        ),
        (
            "primitive cluster count mismatch",
            ClusterFixture {
                prim_value: 2,
                ..ClusterFixture::default()
            },
        ),
        (
            "index start mismatch",
            ClusterFixture {
                prim_index_start: 1,
                ..ClusterFixture::default()
            },
        ),
        (
            "vertex offset not whole vertices",
            ClusterFixture {
                vertex_offset_dwords: 1,
                ..ClusterFixture::default()
            },
        ),
    ];
    for (name, fixture) in mutations {
        let b = clustered_fixture(&fixture);
        assert!(
            mesh::parse(&b, "0000000000000007", limits).is_err(),
            "{name} was accepted"
        );
    }
}

#[test]
fn cluster_only_mesh_geometry_must_fit_clustered_indices() -> Result<()> {
    let b = clustered_fixture(&ClusterFixture {
        cluster_index_count: 3,
        ..ClusterFixture::default()
    });
    let graph = mesh::parse(&b, "0000000000000007", Limits::default())?;
    let cluster = graph.compiled.clustered.as_ref().unwrap();
    assert_eq!(cluster.indices.len(), 6);
    assert!(graph.compiled.mesh_data.indices.is_empty());
    assert_eq!(graph.compiled.mesh_data.standard[0].primitive_count, 1);
    // One triangle needs three u16 indices; one entry is too few.
    let short = clustered_fixture(&ClusterFixture {
        index_entries: 1,
        ..ClusterFixture::default()
    });
    assert!(mesh::parse(&short, "0000000000000007", Limits::default()).is_err());
    Ok(())
}

#[test]
#[ignore = "requires explicit ACU_GAME_DIR; directly reads installed Forge"]
fn installed_cluster_only_mesh() -> Result<()> {
    use acu_assets::{forge::Forge, sha256};
    let root = std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: ACU_GAME_DIR is unset")?;
    let mut f = Forge::open(
        std::path::Path::new(&root).join("DataPC.forge"),
        Limits::default(),
    )?;
    let raw = f.raw(110)?;
    assert_eq!(
        sha256(&raw),
        "7d5f500049eeb22158005bbd74fb8c22b84b84501d226aa6bfc66c4b0aa2334f"
    );
    let p = acu_assets::package::decode(&raw, Limits::default())?;
    let r = &p.resources[2471];
    assert_eq!(r.file_id, "000000007ec1ea7d");
    assert_eq!(r.name, "TPL_WP_Rock_LOD0");
    let payload = &p.bytes[r.payload.clone()];
    assert_eq!(
        sha256(payload),
        "a5dc08f3b7f38809f56ae9aa56bfe6163e39eb5f525d8009b74444a5f14b29a4"
    );
    let graph = mesh::parse(payload, &r.file_id, Limits::default())?;
    assert!(graph.compiled.mesh_data.vertices.is_empty());
    assert!(graph.compiled.mesh_data.indices.is_empty());
    assert_eq!(graph.compiled.mesh_data.vertex_format, 4);
    assert_eq!(graph.compiled.mesh_data.vertex_stride, 20);
    assert!(!graph.compiled.mesh_data.index32);
    assert_eq!(graph.compiled.mesh_data.standard[0].vertex_count, 52);
    assert_eq!(graph.compiled.mesh_data.standard[0].primitive_count, 72);
    assert_eq!(graph.compiled.mesh_data.shadow[0].primitive_count, 72);
    let cluster = graph.compiled.clustered.as_ref().unwrap();
    assert_eq!(cluster.vertex_stride, 20);
    assert_eq!(cluster.cluster_count, 1);
    assert_eq!(cluster.draw_primitive_count, 1);
    assert_eq!(cluster.clusters_per_primitive, [1]);
    assert_eq!(cluster.vertex_offsets, [0]);
    assert!(!cluster.fixed_cluster_size);
    assert_eq!(cluster.vertices.len(), 1040);
    assert_eq!(cluster.vertices.len() / 20, 52);
    assert_eq!(cluster.indices.len(), 432);
    assert_eq!(cluster.indices.len() / 2, 3 * 72);
    assert_eq!(cluster.descriptors.len(), 52);
    assert_eq!(cluster.bit_mask.len(), 48);
    assert_eq!(cluster.primitive_descriptors.len(), 36);
    assert_eq!(graph.unresolved(), ["cluster bit mask semantics"]);
    assert_eq!(
        sha256(cluster.vertices),
        "b2fd30e28f1c0d5e43cb0306e5f31d79af7bb0200771c340dc3cddc59618c21c"
    );
    assert_eq!(
        sha256(cluster.indices),
        "225021946fbb6b8248ad85eaf92985d77b50a3d8ab48835fcd03fb25a513b76f"
    );
    // Typed descriptors (ACU-RE-0012), against installed bytes.
    assert_eq!(cluster.clusters.len(), 1);
    let descriptor = &cluster.clusters[0];
    assert_eq!(descriptor.index_offset, 0);
    assert_eq!(descriptor.index_count, 216);
    assert_eq!(descriptor.vertex_offset_dwords, 0);
    assert_eq!(descriptor.cone_axis, [1., 0., 0.]);
    assert_eq!(descriptor.cone_angle, std::f32::consts::PI);
    assert_eq!(descriptor.half_extent[0], 0.081_286_19);
    assert_eq!(cluster.draw_primitives.len(), 1);
    let primitive = &cluster.draw_primitives[0];
    assert_eq!(
        (
            primitive.stride,
            primitive.flag,
            primitive.value,
            primitive.index_start
        ),
        (20, 1, 216, 0)
    );
    let audit = graph.audit_clusters(Limits::default())?;
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].vertex_base, 0);
    assert_eq!(audit[0].distinct_vertices, 52);
    assert_eq!(audit[0].triangles, 72);
    // Cluster AABB holds within one percent of the stored half extents.
    assert!(audit[0].aabb_fit < 1.001, "{}", audit[0].aabb_fit);
    assert!(audit[0].cone_max_angle < audit[0].cone_angle);
    let geometry = graph.decode(Limits::default())?;
    assert_eq!(geometry.vertex_source, "clustered");
    assert_eq!(geometry.index_source, "clustered");
    assert_eq!(geometry.topology_source, "cluster_descriptors");
    assert_eq!(geometry.vertices.len(), 52);
    assert_eq!(geometry.triangles.len(), 72);
    Ok(())
}

#[test]
#[ignore = "requires explicit ACU_GAME_DIR; directly reads installed Forge"]
fn installed_unit_sphere_mesh_data() -> Result<()> {
    use acu_assets::{forge::Forge, sha256};
    let root = std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: ACU_GAME_DIR is unset")?;
    let mut f = Forge::open(
        std::path::Path::new(&root).join("DataPC.forge"),
        Limits::default(),
    )?;
    let raw = f.raw(10)?;
    assert_eq!(
        sha256(&raw),
        "a2f006ae6c30466a93474e239eb59927c29371748dfcbc8b1c2a13aa3551574d"
    );
    let p = acu_assets::package::decode(&raw, Limits::default())?;
    let resource = &p.resources[0];
    assert_eq!(resource.type_id, mesh::TYPE_MESH);
    let payload = &p.bytes[resource.payload.clone()];
    assert_eq!(
        sha256(payload),
        "b1c644de6856ae51b975840e05410dbd02d669ae3bfe74e02fd623eaf1ecb490"
    );
    let slice = &payload[3263..6326];
    let graph = mesh::parse(payload, &resource.file_id, Limits::default())?;
    assert_eq!(graph.category, 5);
    assert!(graph.bones.is_empty());
    assert!(graph.submeshes.is_empty());
    assert_eq!(graph.compiled.quantization_factor, 2.0);
    assert_eq!(graph.compiled.platform_version, 28);
    assert_eq!(graph.compiled.sdk_version, 7);
    assert_eq!(graph.compiled.instancing.len(), 1);
    assert_eq!(graph.compiled.instancing[0].vertex_count, 100);
    assert_eq!(
        graph.compiled.instancing[0].material.as_deref(),
        Some("0000000000000033")
    );
    assert_eq!(graph.materials[0].file_id, "0000000000000033");
    assert_eq!(graph.unresolved(), ["cluster bit mask semantics"]);
    let cluster = graph.compiled.clustered.as_ref().unwrap();
    assert_eq!(cluster.cluster_count, 1);
    assert_eq!(cluster.draw_primitive_count, 1);
    assert_eq!(cluster.clusters_per_primitive, [1]);
    assert_eq!(cluster.vertex_offsets, [0]);
    assert_eq!(cluster.clusters.len(), 1);
    assert_eq!(cluster.clusters[0].index_count, 480);
    assert_eq!(cluster.draw_primitives.len(), 1);
    assert_eq!(cluster.draw_primitives[0].stride, 20);
    assert_eq!(
        (
            cluster.draw_primitives[0].flag,
            cluster.draw_primitives[0].value
        ),
        (1, 480)
    );
    let audit = graph.audit_clusters(Limits::default())?;
    assert_eq!(audit.len(), 1);
    assert_eq!((audit[0].distinct_vertices, audit[0].triangles), (100, 160));
    assert!(audit[0].aabb_fit < 1.001, "{}", audit[0].aabb_fit);
    // Populated non-clustered buffers stay authoritative for this variant.
    let geometry = graph.decode(Limits::default())?;
    assert_eq!(geometry.vertex_source, "mesh_data");
    assert_eq!(geometry.index_source, "mesh_data");
    assert_eq!(geometry.topology_source, "standard");
    assert_eq!(geometry.triangles.len(), 160);
    assert_eq!(
        sha256(cluster.vertices),
        "a0f5dbd897cf9bf4f53fd0aee54ba39e20bc957edb7475dafd770ad516dbfb9d"
    );
    assert_eq!(
        sha256(cluster.indices),
        "bbd4acd72cd0ce451bafb482eda7d8057f39f0b9b24bee7ebff19f149d8a9894"
    );
    assert_eq!(
        sha256(slice),
        "bbaa28e3e98c4213ba962c7d43b617e80df70313dc74c6345819d41c61a3996a"
    );
    let data = mesh::parse_data_exact(slice, Limits::default())?;
    assert_eq!((data.vertex_count(), data.index_count()), (100, 480));
    assert_eq!(data.standard[0].primitive_count, 160);
    assert_eq!(data.shadow[0].primitive_count, 160);
    assert_eq!(
        sha256(data.vertices),
        "187845cca84fddb48819887b152e8af497497c00c364a4402e476ebdfb1dcf3c"
    );
    assert_eq!(
        sha256(data.indices),
        "d4065d81b704e7135fb3affe2ecae45ad8dd4171ad67f05952c7f633b3172a85"
    );
    let v = data.packed_vertex4(0)?;
    assert_eq!(v.position, [4533, 3294, 15396]);
    assert_eq!(v.position_aux, 2);
    assert_eq!(v.normal_color, [168, 151, 244, 255]);
    assert_eq!(v.tangent_color, [182, 12, 130, 255]);
    assert_eq!(v.uv, [1307, 1986]);
    assert_eq!(
        (0..12).map(|i| data.index(i).unwrap()).collect::<Vec<_>>(),
        [0, 1, 2, 3, 1, 0, 2, 4, 0, 3, 0, 5]
    );
    for i in 0..data.index_count() {
        assert!((data.index(i).unwrap() as usize) < data.vertex_count());
    }
    for i in 0..data.vertex_count() {
        data.packed_vertex4(i)?;
    }
    Ok(())
}

#[test]
#[ignore = "requires explicit ACU_GAME_DIR; directly reads installed Forge"]
fn installed_arno_head_graph() -> Result<()> {
    use acu_assets::{forge::Forge, sha256};
    let root = std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: ACU_GAME_DIR is unset")?;
    let mut f = Forge::open(
        std::path::Path::new(&root).join("DataPC_ACU_LGS_BelleEpoque.forge"),
        Limits::default(),
    )?;
    let raw = f.raw(351)?;
    assert_eq!(
        sha256(&raw),
        "48ee5eee987e224c50021e2c21e2afe0f66efd8f70033236352bf592764649b9"
    );
    let p = acu_assets::package::decode(&raw, Limits::default())?;
    let r = p
        .resources
        .iter()
        .find(|r| r.file_id == "000000126c7eb58b")
        .ok_or("missing Arno head resource")?;
    let payload = &p.bytes[r.payload.clone()];
    assert_eq!(
        sha256(payload),
        "4e42910b4c700f68c63b48d8940e411cb9a5b83c5fc5fcbc8c5fdd0cc8dfab44"
    );
    let graph = mesh::parse(payload, &r.file_id, Limits::default())?;
    assert_eq!(graph.unresolved(), ["cluster bit mask semantics"]);
    let cluster = graph.compiled.clustered.as_ref().unwrap();
    assert_eq!(cluster.clusters.len(), 1);
    assert_eq!(cluster.clusters[0].index_count, 25920);
    assert_eq!(cluster.draw_primitives[0].index_start, 0);
    assert_eq!(
        (
            cluster.draw_primitives[0].flag,
            cluster.draw_primitives[0].value,
            cluster.draw_primitives[0].stride
        ),
        (1, 25920, 40)
    );
    println!(
        "Arno mesh bones={} submeshes={} format={} stride={} vertices={} indices={} unresolved={:?}",
        graph.bones.len(),
        graph.submeshes.len(),
        graph.compiled.mesh_data.vertex_format,
        graph.compiled.mesh_data.vertex_stride,
        graph.compiled.mesh_data.vertex_count(),
        graph.compiled.mesh_data.index_count(),
        graph.unresolved()
    );
    Ok(())
}

#[test]
#[ignore = "requires explicit ACU_GAME_DIR; directly reads installed Forge"]
fn installed_arno_parts_bind_instancing_materials() -> Result<()> {
    use acu_assets::{forge::Forge, sha256};
    let root = std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: ACU_GAME_DIR is unset")?;
    let mut f = Forge::open(
        std::path::Path::new(&root).join("DataPC_ACU_LGS_BelleEpoque.forge"),
        Limits::default(),
    )?;
    // Upperbody LOD0, entry 679: three drawn primitives, three materials.
    let raw = f.raw(679)?;
    let p = acu_assets::package::decode(&raw, Limits::default())?;
    let r = p
        .resources
        .iter()
        .find(|r| r.file_id == "00000023dab178a4")
        .ok_or("missing Arno upperbody resource")?;
    let payload = &p.bytes[r.payload.clone()];
    assert_eq!(
        sha256(payload),
        "030cf0cbcb03dae37c4700dac6a3bcabdd6c4555beb0a810e298861db003327f"
    );
    let graph = mesh::parse(payload, &r.file_id, Limits::default())?;
    let parts = graph.decode_parts(Limits::default())?;
    assert_eq!(
        parts
            .iter()
            .map(|p| (p.primitive, p.vertex_base, p.vertex_count))
            .collect::<Vec<_>>(),
        [(0, 0, 3568), (1, 3568, 1932), (2, 5500, 746)]
    );
    assert_eq!(
        parts
            .iter()
            .map(|p| p.material.as_ref().unwrap().file_id.clone())
            .collect::<Vec<_>>(),
        ["0000000b1b911534", "0000000b1b911533", "0000000b1b911530"]
    );
    assert_eq!(
        parts
            .iter()
            .map(|p| p.material.as_ref().unwrap().submesh_index)
            .collect::<Vec<_>>(),
        [1, 0, 2]
    );
    assert_eq!(
        parts.iter().map(|p| p.triangles.len()).sum::<usize>(),
        graph.decode(Limits::default())?.triangles.len()
    );
    // Head LOD0, entry 351: one primitive, one material.
    let raw = f.raw(351)?;
    let p = acu_assets::package::decode(&raw, Limits::default())?;
    let r = p
        .resources
        .iter()
        .find(|r| r.file_id == "000000126c7eb58b")
        .ok_or("missing Arno head resource")?;
    let payload = &p.bytes[r.payload.clone()];
    let graph = mesh::parse(payload, &r.file_id, Limits::default())?;
    let parts = graph.decode_parts(Limits::default())?;
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].vertex_count, 4620);
    assert_eq!(
        parts[0].material.as_ref().unwrap().file_id,
        "000000126c7eb56c"
    );
    Ok(())
}
