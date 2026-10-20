//! Coverage validation: reads archive entries in memory, writes JSON reports only.
use acu_assets::{
    Limits, Result, control,
    forge::{self, Forge},
    mesh, package, sha256, skeleton, texture,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let output = Path::new(args.first().ok_or("usage: acu-scan REPORT_DIR [ARCHIVE]")?);
    if output.exists() {
        return Err("report directory already exists; choose a fresh output path".into());
    }
    let game = PathBuf::from(
        std::env::var("ACU_GAME_DIR").map_err(|_| "set ACU_GAME_DIR to the original install")?,
    );
    let mut paths = Vec::new();
    if let Some(path) = args.get(1) {
        paths.push(PathBuf::from(path));
    } else {
        paths = forge::discover(&game)?;
    }
    paths.sort();
    if paths.is_empty() {
        return Err("unavailable: no Forge archives found".into());
    }
    let manifest = paths
        .iter()
        .map(|p| Ok(json!({"path":p,"length":fs::metadata(p)?.len()})))
        .collect::<Result<Vec<_>>>()?;
    fs::create_dir_all(output)?;
    let mut rows = BufWriter::new(File::create(output.join("entries.jsonl"))?);
    let mut types: BTreeMap<String, Value> = BTreeMap::new();
    let mut archives = Vec::new();
    let limits = Limits::default();
    for path in paths {
        eprintln!("{}", path.display());
        let mut archive = match Forge::open(&path, limits) {
            Ok(a) => a,
            Err(e) => {
                archives.push(json!({"path":path,"index_error":e.to_string()}));
                continue;
            }
        };
        let (mut decoded, mut framed, mut failed, mut resources, mut typed, mut extensions) =
            (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
        let (mut controls, mut resident_textures, mut dynamic_textures) = (0usize, 0usize, 0usize);
        let mut typed_partial = 0usize;
        let mut typed_by_family: BTreeMap<&'static str, usize> = BTreeMap::new();
        for i in 0..archive.entries.len() {
            let entry = archive.entries[i].clone();
            let mut row = json!({"archive":path,"entry":entry,"typed_status":"unsupported"});
            match archive.raw(i) {
                Err(e) => {
                    failed += 1;
                    row["error"] = json!(e.to_string());
                }
                Ok(raw) => {
                    row["stored_sha256"] = json!(sha256(&raw));
                    if control::is_control(&entry.file_id) {
                        match control::decode(&entry.file_id, &raw, limits) {
                            Ok(decoded) => {
                                controls += 1;
                                row["typed_status"] = json!("parsed_control");
                                row["control"] = decoded.summary();
                            }
                            Err(e) => {
                                failed += 1;
                                row["error"] = json!(e.to_string());
                            }
                        }
                        serde_json::to_writer(&mut rows, &row)?;
                        writeln!(rows)?;
                        continue;
                    }
                    let result = package::decode_frames(&raw, limits).and_then(|f| {
                        framed += 1;
                        row["frames_decoded"] = json!(true);
                        row["decoded_sha256"] = json!(sha256(&f.bytes));
                        row["metadata_sha256"] = json!(sha256(&f.metadata));
                        row["frames"] = serde_json::to_value(&f.frames)?;
                        package::parse(f, limits)
                    });
                    match result {
                        Err(e) => {
                            failed += 1;
                            row["error"] = json!(e.to_string());
                            row["prefix"] = json!(
                                raw.iter()
                                    .take(24)
                                    .map(|b| format!("{b:02x}"))
                                    .collect::<String>()
                            );
                        }
                        Ok(package) => {
                            decoded += 1;
                            resources += package.resources.len();
                            row["resource_envelopes_parsed"] = json!(true);
                            row["metadata_extension_length"] =
                                json!(package.metadata_extension.len());
                            row["metadata_extension_sha256"] =
                                json!(sha256(&package.metadata_extension));
                            if !package.metadata_extension.is_empty() {
                                extensions += 1;
                            }
                            let mut resource_rows = Vec::new();
                            for r in &package.resources {
                                let item = types.entry(r.type_id.clone()).or_insert_with(||json!({"count":0,"example":r.name,"typed_parsed":0,"typed_unsupported_or_failed":0}));
                                item["count"] = json!(item["count"].as_u64().unwrap_or(0) + 1);
                                let mut rr = serde_json::to_value(r)?;
                                rr["payload_sha256"] =
                                    json!(sha256(&package.bytes[r.payload.clone()]));
                                let payload = &package.bytes[r.payload.clone()];
                                let mut family: Option<&'static str> = None;
                                match r.type_id.as_str() {
                                    texture::TYPE_TEXTURE_MAP => {
                                        family = Some("texture");
                                        match texture::parse_resource(payload, &r.file_id) {
                                            Ok(t) => {
                                                typed += 1;
                                                item["typed_parsed"] = json!(
                                                    item["typed_parsed"].as_u64().unwrap_or(0) + 1
                                                );
                                                rr["typed_status"] = json!("parsed");
                                                rr["texture"] = serde_json::to_value(&t)?;
                                                match t {
                                                    texture::TextureResource::Resident(t) => {
                                                        resident_textures += 1;
                                                        rr["pixel_sha256"] =
                                                            json!(sha256(t.pixels));
                                                        rr["usable_pixels"] = json!(true);
                                                    }
                                                    texture::TextureResource::NoInlinePixels(_) => {
                                                        dynamic_textures += 1;
                                                        rr["usable_pixels"] = json!(false);
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                rr["typed_error"] = json!(e.to_string());
                                            }
                                        }
                                    }
                                    mesh::TYPE_MESH => {
                                        family = Some("mesh");
                                        match mesh::parse(payload, &r.file_id, limits) {
                                            Ok(m) => {
                                                typed += 1;
                                                item["typed_parsed"] = json!(
                                                    item["typed_parsed"].as_u64().unwrap_or(0) + 1
                                                );
                                                rr["typed_status"] = json!("parsed");
                                                rr["mesh_unresolved"] = json!(m.unresolved());
                                            }
                                            Err(e) => {
                                                rr["typed_error"] = json!(e.to_string());
                                            }
                                        }
                                    }
                                    skeleton::TYPE_SKELETON => {
                                        family = Some("skeleton");
                                        match skeleton::parse(payload, &r.file_id, limits) {
                                            Ok(s) => {
                                                typed += 1;
                                                item["typed_parsed"] = json!(
                                                    item["typed_parsed"].as_u64().unwrap_or(0) + 1
                                                );
                                                rr["typed_status"] = json!("parsed");
                                                rr["bone_count"] = json!(s.bones.len());
                                            }
                                            Err(e) => {
                                                rr["typed_error"] = json!(e.to_string());
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                                let mut partial = false;
                                if let Some(family) = family
                                    && rr["typed_status"] == "parsed"
                                {
                                    *typed_by_family.entry(family).or_insert(0) += 1;
                                }
                                if let Some(items) = rr.get("mesh_unresolved")
                                    && items.as_array().is_some_and(|a| !a.is_empty())
                                {
                                    partial = true;
                                    typed_partial += 1;
                                }
                                if rr["typed_status"] != "parsed" {
                                    item["typed_unsupported_or_failed"] = json!(
                                        item["typed_unsupported_or_failed"].as_u64().unwrap_or(0)
                                            + 1
                                    );
                                } else if partial {
                                    item["typed_partial"] =
                                        json!(item["typed_partial"].as_u64().unwrap_or(0) + 1);
                                }
                                resource_rows.push(rr);
                            }
                            row["resources"] = json!(resource_rows);
                        }
                    }
                }
            }
            serde_json::to_writer(&mut rows, &row)?;
            writeln!(rows)?;
        }
        rows.flush()?;
        let summary = json!({"path":path,"declared_count":archive.declared_count,"reachable_count":archive.entries.len(),
            "count_matches":archive.declared_count==archive.entries.len(),"entry_frames_decoded":framed,"resource_packages_indexed":decoded,
            "failed_or_unsupported_entries":failed,"resource_envelopes":resources,"typed_resources_parsed":typed,"unparsed_metadata_extensions":extensions});
        let mut summary = summary;
        summary["controls_parsed"] = json!(controls);
        summary["resident_textures"] = json!(resident_textures);
        summary["dynamic_texture_descriptors"] = json!(dynamic_textures);
        summary["typed_by_family"] = json!(typed_by_family);
        summary["typed_partial_resources"] = json!(typed_partial);
        eprintln!("{summary}");
        archives.push(summary);
        fs::write(
            output.join("summary.json"),
            serde_json::to_vec_pretty(
                &json!({"manifest":manifest,"archives":archives,"types":types,"scan_finished":false,"all_assets_parsed":false}),
            )?,
        )?;
    }
    rows.flush()?;
    let all_forge_assets_parsed = archives.iter().all(|a| {
        a.get("index_error").is_none()
            && a["count_matches"] == true
            && a["failed_or_unsupported_entries"] == 0
            && a["unparsed_metadata_extensions"] == 0
            && a["resource_envelopes"] == a["typed_resources_parsed"]
            && a["typed_partial_resources"] == 0
    });
    fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(
            &json!({"manifest":manifest,"archives":archives,"types":types,
        "scan_finished":true,"all_forge_assets_parsed":all_forge_assets_parsed,"all_assets_parsed":false,
        "scope":"Original top-level and DLC Forge files; backups/extracted copies excluded. Other carriers not yet covered."}),
        )?,
    )?;
    if !all_forge_assets_parsed {
        return Err("coverage incomplete; see summary.json and entries.jsonl".into());
    }
    Ok(())
}
