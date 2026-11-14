//! Decode world meshes (`415d9568`) in bulk and report each one's vertex bounding box.
//!
//! `acu-mesh` reports one entry per process with many diagnostics; a whole-map prop
//! census needs only identity and bounds for thousands of meshes, so this bin decodes
//! each listed entry once and writes one JSON line per mesh resource. Decode failures are
//! written as rows with an `error`, never skipped.
//!
//! Read-only: no archive or game file is modified.
//!
//! Usage: acu-mesh-bounds ARCHIVE --entries FILE [--ids FILE] --out PATH
//!
//! `--entries FILE`: entry indices, one per line. `--ids FILE`: optional 16-hex file ids;
//! when given, only those mesh resources are decoded.
use acu_assets::{Limits, Result, forge::Forge, mesh, sha256};
use serde_json::json;
use std::collections::HashSet;
use std::io::Write;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let usage = "usage: acu-mesh-bounds ARCHIVE --entries FILE [--ids FILE] --out PATH";
    let path = args.first().ok_or(usage)?;
    let (mut entries_file, mut ids_file, mut out) = (None, None, None);
    let mut index = 1;
    while index < args.len() {
        let value = args.get(index + 1).ok_or(usage)?.clone();
        match args[index].as_str() {
            "--entries" => entries_file = Some(value),
            "--ids" => ids_file = Some(value),
            "--out" => out = Some(value),
            other => return Err(format!("unknown argument {other:?}; {usage}").into()),
        }
        index += 2;
    }
    let entries_file = entries_file.ok_or("--entries is required")?;
    let out = out.ok_or("--out is required")?;
    let limits = Limits::default();
    let mut forge = Forge::open(path, limits)?;
    let mut entries = Vec::new();
    for line in std::fs::read_to_string(&entries_file)?.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: usize = line.parse()?;
        if value >= forge.entries.len() {
            return Err(format!("entry {value} outside 0..{}", forge.entries.len()).into());
        }
        entries.push(value);
    }
    let wanted: Option<HashSet<String>> = match &ids_file {
        Some(file) => Some(
            std::fs::read_to_string(file)?
                .lines()
                .map(|l| l.trim().to_ascii_lowercase())
                .filter(|l| !l.is_empty())
                .collect(),
        ),
        None => None,
    };

    let mut writer = std::io::BufWriter::new(std::fs::File::create(&out)?);
    let (mut decoded, mut failed, mut failed_entries) = (0usize, 0usize, 0usize);
    for entry in entries {
        let package = match forge.read_package(entry) {
            Ok(package) => package,
            Err(error) => {
                failed_entries += 1;
                writeln!(
                    writer,
                    "{}",
                    json!({"entry": entry, "error": error.to_string()})
                )?;
                continue;
            }
        };
        for (ordinal, resource) in package.resources.iter().enumerate() {
            if resource.type_id != mesh::TYPE_MESH {
                continue;
            }
            if let Some(set) = &wanted
                && !set.contains(&resource.file_id)
            {
                continue;
            }
            let payload = &package.bytes[resource.payload.clone()];
            let mut row = json!({
                "entry": entry,
                "ordinal": ordinal,
                "file_id": resource.file_id,
                "name": resource.name,
                "payload_length": payload.len(),
                "payload_sha256": sha256(payload),
            });
            let result =
                mesh::parse(payload, &resource.file_id, limits).and_then(|m| m.decode(limits));
            match result {
                Ok(geometry) if !geometry.vertices.is_empty() => {
                    let (min, max) = geometry.bounds();
                    row["vertices"] = json!(geometry.vertices.len());
                    row["triangles"] = json!(geometry.triangles.len());
                    row["bbox_min"] = json!(min);
                    row["bbox_max"] = json!(max);
                    decoded += 1;
                }
                Ok(_) => {
                    row["error"] = json!("decoded with zero vertices");
                    failed += 1;
                }
                Err(error) => {
                    row["error"] = json!(error.to_string());
                    failed += 1;
                }
            }
            writeln!(writer, "{row}")?;
        }
    }
    writer.flush()?;
    println!(
        "{}",
        json!({"archive": path, "decoded": decoded, "failed": failed,
               "failed_entries": failed_entries, "out": out})
    );
    Ok(())
}
