//! Print one resource payload from an archive entry.
//!
//! `acu-scan` counts resources and `acu-probe` shows frame prefixes, but neither hands
//! back a resource's bytes. A `Resource` already carries `payload: Range<usize>` into
//! `Package.bytes`, so this bin is the missing read.
//!
//! Read-only: no archive or game file is modified.
//! Usage: acu-resource ARCHIVE --entry N [--fid HEX | --type HEX] [--len N] [--out PATH]
use acu_assets::{Limits, Result, forge::Forge, sha256};
use serde_json::json;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let path = args.first().ok_or(
        "usage: acu-resource ARCHIVE --entry N [--fid HEX | --type HEX] [--len N] [--out PATH]",
    )?;
    let limits = Limits::default();

    let mut entry: Option<usize> = None;
    let mut fid: Option<String> = None;
    let mut type_id: Option<String> = None;
    let mut len: usize = 64;
    let mut out: Option<String> = None;
    let mut index = 1;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args.get(index + 1);
        match flag {
            "--entry" => entry = Some(value.ok_or("--entry needs an index")?.parse()?),
            "--fid" => fid = Some(normalise(value.ok_or("--fid needs an id")?, 16)?),
            "--type" => type_id = Some(normalise(value.ok_or("--type needs a type id")?, 8)?),
            "--len" => {
                len = value.ok_or("--len needs a count")?.parse()?;
                if len > 4096 {
                    return Err("--len exceeds 4096".into());
                }
            }
            "--out" => out = Some(value.ok_or("--out needs a path")?.clone()),
            other => return Err(format!("unknown argument {other:?}").into()),
        }
        index += 2;
    }

    let entry = entry.ok_or("--entry is required: selecting an entry by file id is a full scan")?;
    let mut forge = Forge::open(path, limits)?;
    if entry >= forge.entries.len() {
        return Err(format!("entry {entry} outside 0..{}", forge.entries.len()).into());
    }
    let entry_id = forge.entries[entry].file_id.clone();
    let package = forge.read_package(entry)?;
    if !package.metadata_extension.is_empty() {
        return Err(format!(
            "entry {entry} has a {} byte metadata extension; refusing to guess its layout",
            package.metadata_extension.len()
        )
        .into());
    }

    let selected: Vec<_> = package
        .resources
        .iter()
        .filter(|r| {
            fid.as_ref().is_none_or(|want| &r.file_id == want)
                && type_id.as_ref().is_none_or(|want| &r.type_id == want)
        })
        .take(64)
        .collect();

    if selected.is_empty() {
        let available: Vec<_> = package
            .resources
            .iter()
            .map(|r| format!("{} {} {}", r.file_id, r.type_id, r.name))
            .take(16)
            .collect();
        return Err(format!(
            "no resource matched in entry {entry} ({} resources, first: {})",
            package.resources.len(),
            available.join(" | ")
        )
        .into());
    }

    let mut rows = Vec::new();
    let single = selected.len() == 1;
    for resource in selected {
        let payload = &package.bytes[resource.payload.clone()];
        let prefix = payload
            .iter()
            .take(len)
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let mut row = json!({
            "file_id": resource.file_id,
            "type_id": resource.type_id,
            "name": resource.name,
            "payload_length": payload.len(),
            "payload_sha256": sha256(payload),
            "prefix_hex": prefix,
            "typed_status": resource.typed_status,
            "metadata_tags": resource.metadata_tags,
        });
        if let Some(path) = &out {
            if single {
                std::fs::write(path, payload)?;
                row["written"] = json!(path);
            } else {
                let directory = std::path::Path::new(path);
                std::fs::create_dir_all(directory)?;
                let target = directory.join(format!("{}.bin", resource.file_id));
                std::fs::write(&target, payload)?;
                row["written"] = json!(target.to_string_lossy());
            }
        }
        rows.push(row);
    }

    let report = json!({
        "archive": path,
        "entry": {"index": entry, "file_id": entry_id},
        "resource_count": package.resources.len(),
        "matched": rows.len(),
        "resources": rows,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// Accept `0x` prefixes and lower-case the id, matching the library's formatting.
fn normalise(text: &str, width: usize) -> Result<String> {
    let trimmed = text.trim_start_matches("0x");
    if trimmed.is_empty() || trimmed.len() > width || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("{text:?} is not a 64-bit hexadecimal id").into());
    }
    Ok(format!("{:0>width$}", trimmed.to_ascii_lowercase()))
}
