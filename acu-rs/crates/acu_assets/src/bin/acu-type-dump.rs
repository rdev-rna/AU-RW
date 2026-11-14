//! Bulk-dump every resource of chosen types from one archive into a single file.
//!
//! `acu-resource` reads one resource per process; a whole-map pass (every Paris
//! `GridCellDataBlock`, `Entity` and `LODSelector`) needs hundreds of thousands of
//! payloads, so this bin decodes each selected entry once and streams the matching
//! resources out. It does not interpret payloads; `tools/paris_placements.py` does.
//!
//! Read-only: no archive or game file is modified.
//!
//! Usage: acu-type-dump ARCHIVE --types HEX[,HEX..] --out PATH [--entries FILE]
//!
//! `--entries FILE` lists entry indices (one per line); without it every entry is read.
//!
//! Output (`ACUTD001`, little-endian), repeated per matching resource:
//! `u32 entry, u32 ordinal, u64 file id, u32 type id, u16 name length, name bytes,
//! u32 payload length, payload bytes`. A JSON summary (counts and every failed entry
//! with its error) is printed on stdout; failures are reported, never skipped silently.
use acu_assets::{Limits, Result, forge::Forge};
use serde_json::json;
use std::io::Write;

const MAGIC: &[u8; 8] = b"ACUTD001";

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let usage = "usage: acu-type-dump ARCHIVE --types HEX[,HEX..] --out PATH [--entries FILE]";
    let path = args.first().ok_or(usage)?;
    let mut types: Vec<u32> = Vec::new();
    let mut out: Option<String> = None;
    let mut entries_file: Option<String> = None;
    let mut index = 1;
    while index < args.len() {
        let value = args.get(index + 1).ok_or(usage)?;
        match args[index].as_str() {
            "--types" => {
                for part in value.split(',') {
                    let part = part.trim().trim_start_matches("0x");
                    if part.is_empty() || part.len() > 8 {
                        return Err(format!("{part:?} is not a 32-bit type id").into());
                    }
                    types.push(u32::from_str_radix(part, 16)?);
                }
            }
            "--out" => out = Some(value.clone()),
            "--entries" => entries_file = Some(value.clone()),
            other => return Err(format!("unknown argument {other:?}; {usage}").into()),
        }
        index += 2;
    }
    if types.is_empty() {
        return Err("--types is required".into());
    }
    let out = out.ok_or("--out is required")?;

    let mut forge = Forge::open(path, Limits::default())?;
    let selected: Vec<usize> = match &entries_file {
        Some(file) => {
            let mut list = Vec::new();
            for line in std::fs::read_to_string(file)?.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let value: usize = line.parse()?;
                if value >= forge.entries.len() {
                    return Err(format!("entry {value} outside 0..{}", forge.entries.len()).into());
                }
                list.push(value);
            }
            list
        }
        None => (0..forge.entries.len()).collect(),
    };

    let mut writer = std::io::BufWriter::new(std::fs::File::create(&out)?);
    writer.write_all(MAGIC)?;
    let mut failed = Vec::new();
    let mut written = 0usize;
    let mut payload_bytes = 0usize;
    let mut entries_read = 0usize;
    for &entry in &selected {
        let package = match forge.read_package(entry) {
            Ok(package) => package,
            Err(error) => {
                failed.push(json!({"entry": entry, "error": error.to_string()}));
                continue;
            }
        };
        entries_read += 1;
        if !package.metadata_extension.is_empty() {
            failed.push(json!({
                "entry": entry,
                "error": format!("{} byte metadata extension (layout not supported)",
                                 package.metadata_extension.len()),
            }));
            continue;
        }
        for (ordinal, resource) in package.resources.iter().enumerate() {
            let type_id = u32::from_str_radix(&resource.type_id, 16)?;
            if !types.contains(&type_id) {
                continue;
            }
            let file_id = u64::from_str_radix(&resource.file_id, 16)?;
            let name = resource.name.as_bytes();
            let name_length = u16::try_from(name.len()).map_err(|_| "name too long")?;
            let payload = &package.bytes[resource.payload.clone()];
            let payload_length = u32::try_from(payload.len()).map_err(|_| "payload too long")?;
            writer.write_all(&(entry as u32).to_le_bytes())?;
            writer.write_all(&(ordinal as u32).to_le_bytes())?;
            writer.write_all(&file_id.to_le_bytes())?;
            writer.write_all(&type_id.to_le_bytes())?;
            writer.write_all(&name_length.to_le_bytes())?;
            writer.write_all(name)?;
            writer.write_all(&payload_length.to_le_bytes())?;
            writer.write_all(payload)?;
            written += 1;
            payload_bytes += payload.len();
        }
    }
    writer.flush()?;
    let report = json!({
        "archive": path,
        "archive_entries": forge.entries.len(),
        "entries_selected": selected.len(),
        "entries_read": entries_read,
        "types": types.iter().map(|t| format!("{t:08x}")).collect::<Vec<_>>(),
        "resources_written": written,
        "payload_bytes": payload_bytes,
        "failed_entries": failed,
        "out": out,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
