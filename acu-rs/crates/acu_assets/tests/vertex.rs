//! Synthetic vertex-layout tests plus one ignored install test that pins the
//! first rendered character mesh. See ACU-RE-0011.

use acu_assets::{Limits, Result, vertex};

/// One format-1 (stride 40) vertex: position i16x4, three packed unit vectors,
/// UV i16x2, then 8 joint indices and 8 weights.
#[allow(clippy::too_many_arguments)]
fn format1_vertex(
    position: [i16; 4],
    normal: [u8; 4],
    tangent: [u8; 4],
    binormal: [u8; 4],
    uv: [i16; 2],
    joints: [u8; 8],
    weights: [u8; 8],
) -> Vec<u8> {
    let mut b = Vec::new();
    for x in position {
        b.extend(x.to_le_bytes());
    }
    b.extend(normal);
    b.extend(tangent);
    b.extend(binormal);
    for x in uv {
        b.extend(x.to_le_bytes());
    }
    b.extend(joints);
    b.extend(weights);
    assert_eq!(b.len(), 40);
    b
}

#[test]
fn format1_decodes_position_sign_normal_uv_and_joints() -> Result<()> {
    let mut b = format1_vertex(
        [1000, -2000, 30000, 2],
        [127, 254, 0, 255],
        [1, 2, 3, 4],
        [5, 6, 7, 8],
        [2048, -4096],
        [0, 1, 2, 3, 4, 5, 6, 7],
        [8, 9, 10, 11, 12, 13, 14, 15],
    );
    // Same position with a negative `w`: the reference selects the scale sign
    // from `w`, so every coordinate flips.
    b.extend(format1_vertex(
        [1000, -2000, 30000, -2],
        [255, 128, 129, 0],
        [255, 255, 255, 1],
        [0, 0, 0, 0],
        [0, 1],
        [255; 8],
        [0; 8],
    ));
    let v = vertex::decode(&b, 1, 40, 2.0, Limits::default())?;
    assert_eq!(v.len(), 2);

    let a = &v[0];
    assert_eq!(a.position_w, 2);
    assert_eq!(a.position[0], 0.061_037_02);
    assert_eq!(a.position[1], -0.122_074_04);
    assert_eq!(a.position[2], 1.831_110_6);
    assert_eq!(a.normal, [0.0, 1.0, -1.0]);
    assert_eq!(a.normal_w, 255);
    assert_eq!(
        a.tangent.unwrap(),
        [1.0 / 127.0 - 1.0, 2.0 / 127.0 - 1.0, 3.0 / 127.0 - 1.0]
    );
    assert_eq!(a.tangent_w, Some(4));
    assert_eq!(
        a.binormal.unwrap(),
        [5.0 / 127.0 - 1.0, 6.0 / 127.0 - 1.0, 7.0 / 127.0 - 1.0]
    );
    assert_eq!(a.binormal_w, Some(8));
    assert_eq!(a.uv0, Some([1.0, -2.0]));
    assert_eq!(a.color, None);
    assert_eq!(a.joints.as_deref(), Some(&[0, 1, 2, 3, 4, 5, 6, 7][..]));
    assert_eq!(
        a.joint_weights.as_deref(),
        Some(&[8, 9, 10, 11, 12, 13, 14, 15][..])
    );

    let b_vertex = &v[1];
    assert_eq!(b_vertex.position_w, -2);
    assert_eq!(b_vertex.position[0], -0.061_037_02);
    assert_eq!(b_vertex.position[1], 0.122_074_04);
    assert_eq!(b_vertex.position[2], -1.831_110_6);
    // Byte 255 exceeds 1.0 before clamping; 128/129 are the numerators 1/2.
    assert_eq!(
        b_vertex.normal,
        [1.0, 128.0 / 127.0 - 1.0, 129.0 / 127.0 - 1.0]
    );
    assert_eq!(b_vertex.normal_w, 0);
    assert_eq!(b_vertex.uv0, Some([0.0, 1.0 / 2048.0]));
    assert!(b_vertex.joints.as_ref().unwrap().iter().all(|&j| j == 255));
    Ok(())
}

#[test]
fn layouts_are_looked_up_by_format_and_stride() {
    assert_eq!(vertex::layout(1, 40).unwrap().joints, 8);
    assert_eq!(vertex::layout(0, 32).unwrap().joints, 4);
    assert_eq!(vertex::stride_candidates(4), [20]);
    assert_eq!(vertex::stride_candidates(5), [24, 28]);
    // The stride-28 variant is the only format-5 layout carrying a binormal.
    assert!(!vertex::layout(5, 24).unwrap().binormal);
    assert!(vertex::layout(5, 28).unwrap().binormal);
    assert!(vertex::layout(5, 28).unwrap().color);
    assert!(!vertex::layout(6, 16).unwrap().tangent);
    assert_eq!(vertex::layout(1, 32), None);
    // Format 2 stride 40 was recovered from a dumped buffer (see ACU-RE-0039): same
    // attribute order as format 1's stride-40 layout, different enum id.
    assert_eq!(vertex::layout(2, 40).unwrap().joints, 8);
    assert!(vertex::layout(2, 40).unwrap().binormal);
    assert_eq!(vertex::stride_candidates(2), [40]);
    assert_eq!(vertex::layout(2, 24), None);
}

#[test]
fn decode_rejects_unknown_layouts_and_bad_buffers() -> Result<()> {
    let error_message = |r: Result<Vec<vertex::Vertex>>| r.unwrap_err().to_string();

    let message = error_message(vertex::decode(&[0; 40], 1, 32, 2.0, Limits::default()));
    assert!(
        message.contains("unsupported vertex format 1 stride 32"),
        "{message}"
    );
    assert!(message.contains("[40]"), "{message}");
    let message = error_message(vertex::decode(&[0; 24], 2, 24, 2.0, Limits::default()));
    assert!(
        message.contains("unsupported vertex format 2 stride 24"),
        "{message}"
    );
    let message = error_message(vertex::decode(&[0; 41], 1, 40, 2.0, Limits::default()));
    assert!(message.contains("not a multiple of stride"), "{message}");
    for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let message = error_message(vertex::decode(&[0; 40], 1, 40, bad, Limits::default()));
        assert!(message.contains("quantization factor"), "{message}");
    }
    let message = error_message(vertex::decode(
        &[0; 80],
        1,
        40,
        2.0,
        Limits {
            max_resources: 1,
            ..Limits::default()
        },
    ));
    assert!(message.contains("limit"), "{message}");
    assert!(vertex::decode(&[], 1, 40, 2.0, Limits::default())?.is_empty());
    Ok(())
}

#[test]
fn format_variants_without_joints_or_binormals() -> Result<()> {
    // Format 4 (stride 20): position, normal, tangent, UV. No binormal, color
    // or joints.
    let mut b = Vec::new();
    for x in [1_i16, 2, 3, 4] {
        b.extend(x.to_le_bytes());
    }
    b.extend([127, 127, 127, 7]);
    b.extend([1, 1, 1, 1]);
    for x in [0_i16, 0] {
        b.extend(x.to_le_bytes());
    }
    let v = vertex::decode(&b, 4, 20, 1.0, Limits::default())?;
    let a = &v[0];
    assert_eq!(a.position, [1.0 / 32767.0, 2.0 / 32767.0, 3.0 / 32767.0]);
    assert_eq!(a.normal, [0.0, 0.0, 0.0]);
    assert!(a.tangent.is_some());
    assert_eq!(a.binormal, None);
    assert_eq!(a.color, None);
    assert_eq!(a.joints, None);
    assert_eq!(a.joint_weights, None);

    // Format 5 (stride 24): 4 color bytes replace the binormal slot.
    let mut b = Vec::new();
    for x in [32767_i16, 0, -32767, 1] {
        b.extend(x.to_le_bytes());
    }
    b.extend([128, 128, 128, 3]);
    b.extend([128, 128, 128, 4]);
    b.extend([9, 10, 11, 12]);
    for x in [2048_i16, 2048] {
        b.extend(x.to_le_bytes());
    }
    let v = vertex::decode(&b, 5, 24, 1.0, Limits::default())?;
    let a = &v[0];
    assert_eq!(a.position, [1.0, 0.0, -1.0]);
    assert_eq!(a.color, Some([9, 10, 11, 12]));
    assert_eq!(a.binormal, None);
    assert_eq!(a.tangent_w, Some(4));

    // Format 6 (stride 16): position, normal, UV only.
    let mut b = Vec::new();
    for x in [0_i16, 0, 0, 0] {
        b.extend(x.to_le_bytes());
    }
    b.extend([127, 127, 127, 0]);
    for x in [0_i16, 0] {
        b.extend(x.to_le_bytes());
    }
    let v = vertex::decode(&b, 6, 16, 1.0, Limits::default())?;
    assert!(v[0].tangent.is_none());
    assert_eq!(v[0].uv0, Some([0.0, 0.0]));
    Ok(())
}

#[test]
#[ignore = "requires explicit ACU_GAME_DIR; directly reads installed Forge"]
fn installed_arno_legs_geometry() -> Result<()> {
    use acu_assets::{forge::Forge, mesh, package, sha256};
    let root = std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: ACU_GAME_DIR is unset")?;
    let mut f = Forge::open(
        std::path::Path::new(&root).join("DataPC_ACU_LGS_BelleEpoque.forge"),
        Limits::default(),
    )?;
    let raw = f.raw(679)?;
    assert_eq!(
        sha256(&raw),
        "a17f3e13fbc3ded2486460e34067256ea6435891b990109d23278a38b1b6c680"
    );
    let p = package::decode(&raw, Limits::default())?;
    let r = &p.resources[23];
    assert_eq!(r.file_id, "00000023dab178b2");
    assert_eq!(r.name, "CN_P_Arno_FearlessAssassin_Legs_LOD0");
    let payload = &p.bytes[r.payload.clone()];
    assert_eq!(
        sha256(payload),
        "ed3ee99d09824b8764ff0a255aa1837ff668e75f1aca65857f3b900ed6ab314c"
    );
    let graph = mesh::parse(payload, &r.file_id, Limits::default())?;
    assert_eq!(graph.compiled.quantization_factor, 2.0);
    assert_eq!(graph.unresolved(), ["cluster bit mask semantics"]);
    let cluster = graph.compiled.clustered.as_ref().unwrap();
    assert_eq!(cluster.vertex_format, 1);
    assert_eq!(cluster.vertex_stride, 40);
    assert_eq!(cluster.cluster_count, 2);
    assert_eq!(cluster.draw_primitive_count, 2);
    assert_eq!(cluster.vertices.len(), 94360);
    assert_eq!(cluster.indices.len(), 24756);
    assert_eq!(cluster.descriptors.len(), 104);
    assert_eq!(cluster.bit_mask.len(), 96);
    assert_eq!(cluster.primitive_descriptors.len(), 72);
    // Typed descriptors carry the per-cluster index spans and vertex-group
    // bases (ACU-RE-0012): 13100 dwords * 4 / 40 = vertex 1310.
    assert_eq!(
        cluster
            .clusters
            .iter()
            .map(|c| (c.index_offset, c.index_count, c.vertex_offset_dwords))
            .collect::<Vec<_>>(),
        [(0, 6642, 0), (6642, 5736, 13100)]
    );
    assert_eq!(cluster.primitive_of_cluster(1), Some(1));
    assert_eq!(cluster.vertex_base(1), Some(1310));
    assert_eq!(
        cluster
            .draw_primitives
            .iter()
            .map(|p| (p.flag, p.value, p.index_start))
            .collect::<Vec<_>>(),
        [(1, 6642, 0), (1, 5736, 6642)]
    );
    assert_eq!(
        sha256(cluster.vertices),
        "b2e5bf0dde5036f61f08080f5daff577e2286bd7c529962fff4ef0d17020228c"
    );
    assert_eq!(
        sha256(cluster.indices),
        "f58465a98a320c2f213ce00d9c5704645f67ae0453f2335256651b9e7bf3b5b7"
    );

    let geometry = graph.decode(Limits::default())?;
    assert_eq!(geometry.vertex_source, "clustered");
    assert_eq!(geometry.index_source, "clustered");
    assert_eq!(geometry.topology_source, "cluster_descriptors");
    assert_eq!(geometry.vertices.len(), 2359);
    assert_eq!(geometry.triangles.len(), 4126);
    // Per-cluster bounds fit the stored cluster boxes (ACU-RE-0012).
    let audit = graph.audit_clusters(Limits::default())?;
    assert_eq!(audit.len(), 2);
    assert_eq!(audit[0].vertex_base, 0);
    assert_eq!(audit[1].vertex_base, 1310);
    assert_eq!(audit[0].triangles + audit[1].triangles, 4126);
    for row in &audit {
        assert!(row.aabb_fit < 1.001, "cluster {}", row.cluster);
        assert!(
            row.cone_max_angle < row.cone_angle,
            "cluster {}",
            row.cluster
        );
    }
    // Every decoded vertex must fall in the stored cluster box expanded by two
    // quantization steps (`quantization_factor / 32767`); see ACU-RE-0011.
    let center = cluster.center;
    let half_extent = cluster.half_extent;
    let tolerance = 2.0 * graph.compiled.quantization_factor / 32767.0;
    let (min, max) = geometry.bounds();
    for (i, expected) in [
        center[0] - half_extent[0],
        center[1] - half_extent[1],
        center[2] - half_extent[2],
    ]
    .iter()
    .enumerate()
    {
        assert!(
            (min[i] - expected).abs() < tolerance,
            "axis {i}: {} vs {expected}",
            min[i]
        );
    }
    for (i, expected) in [
        center[0] + half_extent[0],
        center[1] + half_extent[1],
        center[2] + half_extent[2],
    ]
    .iter()
    .enumerate()
    {
        assert!(
            (max[i] - expected).abs() < tolerance,
            "axis {i}: {} vs {expected}",
            max[i]
        );
    }
    for v in &geometry.vertices {
        for i in 0..3 {
            assert!(v.position[i] >= center[i] - half_extent[i] - tolerance);
            assert!(v.position[i] <= center[i] + half_extent[i] + tolerance);
        }
        // Every stored `w` is the positive full-scale value for this mesh.
        assert_eq!(v.position_w, 32767);
        assert!(v.joints.as_ref().unwrap().iter().all(|&j| j < 128));
    }
    Ok(())
}
