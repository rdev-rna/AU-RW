//! Material/TextureSet/TextureMap chain diagnosis: decode one entry and report
//! typed parse status for the requested resources. See ACU-RE-0013.
use acu_assets::{Limits, Result, forge::Forge, material, package, sha256, texture};
use serde_json::json;

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Every file ID a parsed material or texture set points at, in order.
fn material_references(m: &material::Material) -> Vec<(String, String)> {
    let mut refs = vec![
        ("template".to_string(), m.template.file_id.clone()),
        ("texture_set".to_string(), m.texture_set.file_id.clone()),
        (
            "backface_material".to_string(),
            m.backface_material.file_id.clone(),
        ),
    ];
    for (index, p) in m.parameters.iter().enumerate() {
        if let material::ParameterValue::Object(o) = &p.value
            && let material::ObjectData::TextureSelector(s) = &o.data
        {
            let role = material::texture_map_type_name(s.map_type as u32).unwrap_or("unknown");
            refs.push((
                format!("param{index}:{role}"),
                s.texture_map.file_id.clone(),
            ));
        }
    }
    refs
}

fn texture_set_references(set: &material::TextureSet) -> Vec<(String, String)> {
    [
        ("diffuse", &set.diffuse),
        ("normal", &set.normal),
        ("specular", &set.specular),
        ("offset_bump", &set.offset_bump),
        ("emissive", &set.emissive),
        ("transmission", &set.transmission),
        ("occlusion", &set.occlusion),
        ("mask1", &set.mask1),
        ("mask2", &set.mask2),
        ("cookie", &set.cookie),
        ("env_lighting", &set.env_lighting),
        ("generic", &set.generic),
    ]
    .into_iter()
    .map(|(name, r)| (name.to_string(), r.file_id.clone()))
    .collect()
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut hex_dump = false;
    let mut chain = false;
    let mut out: Option<String> = None;
    let mut positional = Vec::new();
    for arg in &args {
        match arg.as_str() {
            "--hex" => hex_dump = true,
            "--chain" => chain = true,
            _ => {
                if let Some(path) = arg.strip_prefix("--out=") {
                    out = Some(path.to_string());
                } else {
                    positional.push(arg.clone());
                }
            }
        }
    }
    let path = positional
        .first()
        .ok_or("usage: acu-mat ARCHIVE ENTRY RESOURCE|FILE_ID [--chain] [--hex] [--out=FILE]")?;
    let entry: usize = positional.get(1).ok_or("entry index required")?.parse()?;
    let key = positional
        .get(2)
        .ok_or("resource index or file ID required")?;
    let limits = Limits::default();
    let mut forge = Forge::open(path, limits)?;
    let raw = forge.raw(entry)?;
    let package = package::decode(&raw, limits)?;
    let resource: usize = if key.len() == 16 && key.bytes().all(|b| b.is_ascii_hexdigit()) {
        let wanted = key.to_ascii_lowercase();
        package
            .resources
            .iter()
            .position(|r| r.file_id == wanted)
            .ok_or_else(|| format!("entry has no resource with ID {wanted}"))?
    } else {
        key.parse()?
    };
    let r = package
        .resources
        .get(resource)
        .ok_or("resource index outside package")?;
    let payload = &package.bytes[r.payload.clone()];
    let mut report = json!({
        "entry": entry,
        "stored_sha256": sha256(&raw),
        "resource_count": package.resources.len(),
        "resource": serde_json::to_value(r)?,
        "payload_length": payload.len(),
        "payload_sha256": sha256(payload),
    });
    if hex_dump {
        report["payload_hex"] = json!(hex(payload));
        // Preserve a requested diagnostic dump even if typed parsing fails.
        if let Some(path) = &out {
            std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
        }
    }
    match r.type_id.as_str() {
        material::TYPE_MATERIAL => {
            let m = material::parse(payload, &r.file_id, limits)?;
            report["material"] = serde_json::to_value(&m)?;
            if chain {
                report["references"] = json!(resolve(&package, &material_references(&m)));
            }
        }
        material::TYPE_TEXTURE_SET => {
            let set = material::parse_texture_set(payload, &r.file_id, limits)?;
            report["texture_set"] = serde_json::to_value(&set)?;
            if chain {
                report["references"] = json!(resolve(&package, &texture_set_references(&set)));
            }
        }
        texture::TYPE_TEXTURE_MAP => {
            let t = texture::parse_resource(payload, &r.file_id)?;
            report["texture"] = serde_json::to_value(&t)?;
            match t {
                texture::TextureResource::Resident(t) => {
                    report["pixel_sha256"] = json!(sha256(t.pixels));
                }
                texture::TextureResource::NoInlinePixels(_) => {}
            }
        }
        _ => {}
    }
    let text = serde_json::to_string_pretty(&report)?;
    match out {
        Some(path) => std::fs::write(path, text.as_bytes())?,
        None => println!("{text}"),
    }
    Ok(())
}

/// Which of the referenced IDs live in the decoded entry.
fn resolve(package: &package::Package, references: &[(String, String)]) -> Vec<serde_json::Value> {
    references
        .iter()
        .map(|(role, id)| {
            let found = package.resources.iter().position(|r| &r.file_id == id);
            json!({
                "role": role,
                "file_id": id,
                "in_entry": found.is_some(),
                "resource": found,
                "type_id": found.map(|i| package.resources[i].type_id.clone()),
                "name": found.map(|i| package.resources[i].name.clone()),
            })
        })
        .collect()
}
