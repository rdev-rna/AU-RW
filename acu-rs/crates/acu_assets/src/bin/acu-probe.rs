use acu_assets::{Limits, Result, forge::Forge, frame, package, reader::Reader, sha256};
use serde_json::json;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let path = args
        .first()
        .ok_or("usage: acu-probe ARCHIVE [ENTRY_INDEX | --list [LIMIT] | --fid HEX_ID]")?;
    let limits = Limits::default();
    let mut forge = Forge::open(path, limits)?;
    let mut report = json!({"archive":path,"declared_count":forge.declared_count,"reachable_count":forge.entries.len()});
    if args.get(1).is_none_or(|a| a == "--list") {
        let limit = args
            .get(2)
            .map(|a| a.parse::<usize>())
            .transpose()?
            .unwrap_or(16);
        if limit > 1000 {
            return Err("list limit exceeds 1000".into());
        }
        report["entries"] =
            serde_json::to_value(forge.entries.iter().take(limit).collect::<Vec<_>>())?;
    } else if let Some(index) = args.get(1) {
        let index: usize = if index == "--fid" {
            let id = u64::from_str_radix(
                args.get(2)
                    .ok_or("--fid requires an ID")?
                    .trim_start_matches("0x"),
                16,
            )?;
            let key = format!("{id:016x}");
            let matches = forge
                .entries
                .iter()
                .filter(|e| e.file_id == key)
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(format!(
                    "ID {key}: {} matches; select an explicit entry index",
                    matches.len()
                )
                .into());
            }
            matches[0].index
        } else {
            index.parse()?
        };
        let raw = forge.raw(index)?;
        let mut r = Reader::new(&raw);
        let mut frames = Vec::new();
        while r.remaining() >= 8
            && r.bytes[r.position..r.position + 8] == frame::MAGIC.to_le_bytes()
        {
            let (data, info) = frame::decode(&mut r, limits)?;
            frames.push(json!({"info":info,"sha256":sha256(&data),"prefix":data.iter().take(96).map(|b|format!("{b:02x}")).collect::<String>(),"suffix":data.iter().skip(data.len().saturating_sub(32)).map(|b|format!("{b:02x}")).collect::<String>()}));
        }
        report["entry"] = serde_json::to_value(&forge.entries[index])?;
        report["stored_sha256"] = json!(sha256(&raw));
        report["frames"] = json!(frames);
        report["remaining"] = json!(r.remaining());
        if acu_assets::control::is_control(&forge.entries[index].file_id) {
            report["control"] = forge.read_control(index)?.summary();
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
        let package_result = package::decode(&raw, limits);
        if let Err(e) = &package_result {
            report["resource_error"] = json!(e.to_string());
        }
        if let Ok(package) = package_result {
            report["metadata_extension_length"] = json!(package.metadata_extension.len());
            report["resource_count"] = json!(package.resources.len());
            report["resources"] =
                serde_json::to_value(package.resources.iter().take(32).collect::<Vec<_>>())?;
            if package
                .resources
                .first()
                .is_some_and(|r| r.type_id == acu_assets::texture::TYPE_TEXTURE_MAP)
            {
                match package.texture(0) {
                    Ok(t) => {
                        report["texture"] = serde_json::to_value(&t)?;
                        report["pixel_sha256"] = json!(sha256(t.pixels));
                    }
                    Err(e) => {
                        report["typed_error"] = json!(e.to_string());
                    }
                }
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
