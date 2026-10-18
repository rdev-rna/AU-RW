//! Fast control-only audit. Does not decode ordinary asset packages.
use acu_assets::{
    Limits, Result,
    control::{self, ArchiveControl},
    forge::{self, Forge},
    sha256,
};
use serde_json::json;
use std::{collections::BTreeSet, fs, path::PathBuf};

fn main() -> Result<()> {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: acu-controls REPORT.json")?,
    );
    if out.exists() {
        return Err("report already exists; choose a fresh output path".into());
    }
    let game =
        PathBuf::from(std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: set ACU_GAME_DIR")?);
    let mut archives = Vec::new();
    let mut errors = 0;
    for path in forge::discover(&game)? {
        let mut archive = Forge::open(&path, Limits::default())?;
        let ordinary_ids: BTreeSet<_> = archive
            .entries
            .iter()
            .filter(|e| !control::is_control(&e.file_id))
            .map(|e| e.file_id.clone())
            .collect();
        let indices: Vec<_> = archive
            .entries
            .iter()
            .filter(|e| control::is_control(&e.file_id))
            .map(|e| e.index)
            .collect();
        let mut controls = Vec::new();
        let mut meta_count = 0;
        let mut prefetch_count = 0;
        for index in indices {
            let entry = archive.entries[index].clone();
            let raw = archive.raw(index)?;
            let mut row = json!({"entry":entry,"stored_sha256":sha256(&raw)});
            match control::decode(&entry.file_id, &raw, archive.limits) {
                Err(e) => {
                    errors += 1;
                    row["error"] = json!(e.to_string());
                }
                Ok(decoded) => {
                    row["parsed"] = json!(true);
                    row["control"] = decoded.summary();
                    match &decoded.data {
                        ArchiveControl::Metadata(_) => {
                            meta_count += 1;
                        }
                        ArchiveControl::Prefetch(p) => {
                            prefetch_count += 1;
                            let ids: BTreeSet<_> =
                                p.entries.iter().map(|e| e.file_id.clone()).collect();
                            let missing: Vec<_> = ordinary_ids.difference(&ids).collect();
                            let extra: Vec<_> = ids.difference(&ordinary_ids).collect();
                            let duplicates = p.entries.len() - ids.len();
                            row["prefetch_package_identity_check"] = json!({
                                "missing":missing,"extra":extra,"duplicate_count":duplicates
                            });
                            if !missing.is_empty() || !extra.is_empty() || duplicates != 0 {
                                errors += 1;
                            }
                        }
                    }
                }
            }
            controls.push(row);
        }
        if meta_count != 1 || prefetch_count != 1 {
            errors += 1;
        }
        let count_matches = archive.entries.len() == archive.declared_count;
        if !count_matches {
            errors += 1;
        }
        archives.push(
            json!({"archive":path,"declared_count":archive.declared_count,
            "reachable_count":archive.entries.len(),"count_matches":count_matches,
            "controls":controls}),
        );
    }
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &out,
        serde_json::to_vec_pretty(&json!({
            "scope":"Original Forge control records only; ordinary payloads not decoded.",
            "scan_finished":true,"errors":errors,"archives":archives,"all_assets_parsed":false
        }))?,
    )?;
    eprintln!("Control audit: {errors} discrepancies; {}", out.display());
    if errors != 0 {
        return Err("control/index audit incomplete; see report".into());
    }
    Ok(())
}
