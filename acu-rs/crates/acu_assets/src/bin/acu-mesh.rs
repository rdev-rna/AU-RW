//! Focused mesh diagnosis: decode one entry and report typed parse status per resource.
use acu_assets::{Limits, Result, forge::Forge, mesh, package, sha256};
use serde_json::json;

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut hex_dump = false;
    let mut geo_dump = false;
    let mut clusters = false;
    let mut refs = false;
    let mut skin = false;
    let mut out: Option<String> = None;
    let mut positional = Vec::new();
    for arg in &args {
        match arg.as_str() {
            "--hex" => hex_dump = true,
            "--geo" => geo_dump = true,
            "--clusters" => clusters = true,
            "--refs" => refs = true,
            "--skin" => skin = true,
            _ => {
                if let Some(path) = arg.strip_prefix("--out=") {
                    out = Some(path.to_string());
                } else {
                    positional.push(arg.clone());
                }
            }
        }
    }
    let path = positional.first().ok_or(
        "usage: acu-mesh ARCHIVE ENTRY [RESOURCE...] [--hex] [--geo] [--clusters] [--refs] [--skin] [--out=FILE]",
    )?;
    let entry: usize = positional.get(1).ok_or("entry index required")?.parse()?;
    let limits = Limits::default();
    let mut forge = Forge::open(path, limits)?;
    let raw = forge.raw(entry)?;
    let package = package::decode(&raw, limits)?;
    let wanted: Vec<usize> = positional[2..]
        .iter()
        .map(|a| a.parse::<usize>().map_err(Into::into))
        .collect::<Result<_>>()?;
    let mut report = json!({
        "archive": forge.path,
        "entry": entry,
        "entry_record": forge.entries[entry],
        "stored_sha256": sha256(&raw),
        "decoded_length": package.bytes.len(),
        "decoded_sha256": sha256(&package.bytes),
        "resource_count": package.resources.len(),
        "resources": []
    });
    for (index, resource) in package.resources.iter().enumerate() {
        if !wanted.is_empty() && !wanted.contains(&index) {
            continue;
        }
        let payload = &package.bytes[resource.payload.clone()];
        let mut item = json!({
            "index": index,
            "file_id": resource.file_id,
            "type_id": resource.type_id,
            "name": resource.name,
            "payload_length": payload.len(),
            "payload_range": resource.payload,
            "payload_sha256": sha256(payload),
        });
        if resource.type_id == mesh::TYPE_MESH {
            match mesh::parse(payload, &resource.file_id, limits) {
                Ok(m) => {
                    item["typed_status"] = json!("parsed");
                    item["unresolved"] = json!(m.unresolved());
                    item["submeshes"] = json!(m.submeshes.len());
                    item["bones"] = json!(m.bones.len());
                    if skin {
                        item["mesh_bones"] = json!(m.bones);
                        item["skin_instancing"] = json!(m.compiled.instancing);
                    }
                    if refs {
                        item["materials"] = serde_json::to_value(&m.materials)?;
                        item["instancing_materials"] = serde_json::to_value(
                            m.compiled
                                .instancing
                                .iter()
                                .map(|i| {
                                    json!({
                                        "file_id": i.file_id,
                                        "submesh_index": i.submesh_index,
                                        "material_type": i.material_type,
                                        "material": i.material,
                                        "vertex_count": i.vertex_count,
                                    })
                                })
                                .collect::<Vec<_>>(),
                        )?;
                        let mut submesh_materials = Vec::new();
                        for (index, pointer) in m.submeshes.iter().enumerate() {
                            let mut node = pointer;
                            let mut depth = 0;
                            while let mesh::SubMeshPointer::Inline(s) = node {
                                submesh_materials.push(json!({
                                    "root": index,
                                    "depth": depth,
                                    "file_id": s.file_id,
                                    "kind": s.kind,
                                    "material": s.material,
                                    "uv_channels": s.uv_channels.len(),
                                }));
                                node = &s.processed_mesh;
                                depth += 1;
                                if depth > 16 {
                                    break;
                                }
                            }
                        }
                        item["submesh_materials"] = json!(submesh_materials);
                    }
                    item["standard_primitives"] = json!(m.compiled.mesh_data.standard.len());
                    item["shadow_primitives"] = json!(m.compiled.mesh_data.shadow.len());
                    item["vertex_bytes"] = json!(m.compiled.mesh_data.vertices.len());
                    item["index_bytes"] = json!(m.compiled.mesh_data.indices.len());
                    item["index32"] = json!(m.compiled.mesh_data.index32);
                    item["mesh_vertex_format"] = json!(m.compiled.mesh_data.vertex_format);
                    item["mesh_vertex_stride"] = json!(m.compiled.mesh_data.vertex_stride);
                    if let Some(c) = &m.compiled.clustered {
                        item["clustered_vertex_format"] = json!(c.vertex_format);
                        item["clustered_vertex_stride"] = json!(c.vertex_stride);
                        item["clustered_vertices"] = json!(c.vertices.len());
                        item["clustered_indices"] = json!(c.indices.len());
                        item["clustered_descriptors"] = json!(c.descriptors.len());
                        item["clustered_bit_mask"] = json!(c.bit_mask.len());
                        item["clustered_primitive_descriptors"] =
                            json!(c.primitive_descriptors.len());
                        if hex_dump {
                            item["clustered_descriptors_hex"] = json!(hex(c.descriptors));
                            item["clustered_bit_mask_hex"] = json!(hex(c.bit_mask));
                            item["clustered_primitive_descriptors_hex"] =
                                json!(hex(c.primitive_descriptors));
                            item["clustered_vertices_hex"] = json!(hex(c.vertices));
                            item["clustered_indices_hex"] = json!(hex(c.indices));
                        }
                    }
                    item["instancing"] = json!(m.compiled.instancing.len());
                    item["quantization_factor"] = json!(m.compiled.quantization_factor);
                    item["platform_version"] = json!(m.compiled.platform_version);
                    item["sdk_version"] = json!(m.compiled.sdk_version);
                    if let Some(c) = &m.compiled.clustered {
                        item["cluster_count"] = json!(c.cluster_count);
                        item["cluster_center"] = json!(c.center);
                        item["cluster_half_extent"] = json!(c.half_extent);
                        item["draw_primitive_count"] = json!(c.draw_primitive_count);
                        item["clusters_per_primitive"] = json!(c.clusters_per_primitive);
                        item["vertex_offsets"] = json!(c.vertex_offsets);
                        item["fixed_cluster_size"] = json!(c.fixed_cluster_size);
                        item["clustered_version"] = json!(c.version);
                        if clusters {
                            item["clusters"] = json!(c.clusters);
                            item["cluster_primitives"] = json!(c.draw_primitives);
                            match m.audit_clusters(limits) {
                                Ok(audit) => {
                                    let max_aabb_fit =
                                        audit.iter().map(|a| a.aabb_fit).fold(0f32, f32::max);
                                    let outside = audit
                                        .iter()
                                        .filter(|a| a.cone_max_angle > a.cone_angle + 1.1e-2)
                                        .count();
                                    let max_slack = audit
                                        .iter()
                                        .map(|a| a.cone_max_angle - a.cone_angle)
                                        .fold(f32::NEG_INFINITY, f32::max);
                                    item["cluster_audit"] = json!(audit);
                                    item["cluster_audit_summary"] = json!({
                                        "cluster_count": audit.len(),
                                        "max_aabb_fit": max_aabb_fit,
                                        "cone_outside": outside,
                                        "max_cone_slack": max_slack,
                                    });
                                }
                                Err(e) => item["cluster_audit_error"] = json!(e.to_string()),
                            }
                        }
                    }
                    item["compiled_data_bytes"] = json!(m.compiled.data.len());
                    item["local_extent_min"] = json!(m.local_extent_min);
                    item["local_extent_max"] = json!(m.local_extent_max);
                    item["standard_primitives_detail"] = serde_json::to_value(
                        m.compiled
                            .mesh_data
                            .standard
                            .iter()
                            .map(|p| {
                                json!({
                                    "min_index": p.min_index,
                                    "start_index": p.start_index,
                                    "primitive_count": p.primitive_count,
                                    "vertex_count": p.vertex_count,
                                    "kind": p.kind,
                                    "depth_only": p.depth_only,
                                })
                            })
                            .collect::<Vec<_>>(),
                    )?;
                    match m.decode(limits) {
                        Ok(g) => {
                            if skin {
                                item["skin_parts"] = json!(m.decode_parts(limits)?);
                                item["skin_vertices"] = json!(
                                    g.vertices
                                        .iter()
                                        .map(|v| {
                                            json!({"position": v.position, "joints": v.joints,
                                           "weights": v.joint_weights})
                                        })
                                        .collect::<Vec<_>>()
                                );
                            }
                            let (min, max) = g.bounds();
                            item["decoded_vertices"] = json!(g.vertices.len());
                            item["decoded_triangles"] = json!(g.triangles.len());
                            item["decoded_bbox_min"] = json!(min);
                            item["decoded_bbox_max"] = json!(max);
                            if geo_dump {
                                item["decoded_positions"] = serde_json::to_value(
                                    g.vertices
                                        .iter()
                                        .map(|v| v.position.map(f64::from))
                                        .collect::<Vec<_>>(),
                                )?;
                                item["decoded_triangles_list"] =
                                    serde_json::to_value(&g.triangles)?;
                            }
                            item["geometry_source"] =
                                json!(format!("{}/{}", g.vertex_source, g.index_source));
                            item["topology_source"] = json!(g.topology_source);
                            item["primitive_bounds"] = serde_json::to_value(
                                m.compiled
                                    .mesh_data
                                    .standard
                                    .iter()
                                    .filter(|p| !p.depth_only && p.kind == 1)
                                    .map(|p| {
                                        let (lo, hi) = (p.min_index as usize, (p.min_index as usize + p.vertex_count as usize).min(g.vertices.len()));
                                        let mut mn = [f32::INFINITY; 3];
                                        let mut mx = [f32::NEG_INFINITY; 3];
                                        for v in g.vertices.get(lo..hi).unwrap_or_default() {
                                            for i in 0..3 {
                                                mn[i] = mn[i].min(v.position[i]);
                                                mx[i] = mx[i].max(v.position[i]);
                                            }
                                        }
                                        json!({"min_index": p.min_index, "vertex_count": p.vertex_count, "min": mn, "max": mx})
                                    })
                                    .collect::<Vec<_>>(),
                            )?;
                        }
                        Err(e) => item["decode_error"] = json!(e.to_string()),
                    }
                }
                Err(e) => {
                    item["typed_status"] = json!("failed");
                    item["error"] = json!(e.to_string());
                }
            }
        }
        report["resources"].as_array_mut().unwrap().push(item);
    }
    let text = serde_json::to_string_pretty(&report)?;
    match out {
        Some(path) => std::fs::write(path, text.as_bytes())?,
        None => println!("{text}"),
    }
    Ok(())
}
