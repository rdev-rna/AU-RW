//! Cross-archive duplicate file IDs (#35). Reads archive indexes, and the stored bytes of
//! duplicated non-control IDs only to hash them. No payload is decoded. Writes JSON reports.
//! Nothing here implies which copy the game uses: archive order in the output is path order.
use acu_assets::{
    Limits, Result, control,
    forge::{self, Forge},
    sha256,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
};

struct Occurrence {
    archive: usize,
    index: usize,
    stored_length: u32,
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let output = PathBuf::from(args.first().ok_or("usage: acu-dup-ids REPORT_DIR")?);
    if output.exists() {
        return Err("report directory already exists; choose a fresh output path".into());
    }
    let game = PathBuf::from(
        std::env::var("ACU_GAME_DIR").map_err(|_| "set ACU_GAME_DIR to the original install")?,
    );
    let paths = forge::discover(&game)?;
    let limits = Limits::default();
    let mut archives: Vec<Forge> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut index_errors = Vec::new();
    for path in &paths {
        let name = path
            .strip_prefix(&game)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        match Forge::open(path, limits) {
            Ok(archive) => {
                names.push(name);
                archives.push(archive);
            }
            Err(e) => index_errors.push(json!({"archive": name, "error": e.to_string()})),
        }
    }

    let mut by_id: BTreeMap<String, Vec<Occurrence>> = BTreeMap::new();
    let mut control_ids: BTreeMap<String, usize> = BTreeMap::new();
    let mut per_archive = Vec::new();
    for (a, archive) in archives.iter().enumerate() {
        let (mut controls, mut data) = (0usize, 0usize);
        for e in &archive.entries {
            if control::is_control(&e.file_id) {
                controls += 1;
                *control_ids.entry(e.file_id.clone()).or_default() += 1;
                continue;
            }
            data += 1;
            by_id
                .entry(e.file_id.clone())
                .or_default()
                .push(Occurrence {
                    archive: a,
                    index: e.index,
                    stored_length: e.stored_length,
                });
        }
        per_archive.push(
            json!({"archive": names[a], "declared": archive.declared_count,
            "reachable": archive.entries.len(), "control_entries": controls,
            "data_entries": data}),
        );
    }

    fs::create_dir_all(&output)?;
    let mut rows = BufWriter::new(File::create(output.join("duplicates.jsonl"))?);
    let (mut unique, mut dup_ids, mut identical_ids, mut differing_ids) = (0usize, 0, 0, 0);
    let (mut same_archive_ids, mut read_errors) = (0usize, 0usize);
    let mut pairs: BTreeMap<String, [usize; 2]> = BTreeMap::new();
    let mut shape: BTreeMap<usize, usize> = BTreeMap::new();
    for (id, occ) in &by_id {
        if occ.len() == 1 {
            unique += 1;
            continue;
        }
        dup_ids += 1;
        *shape.entry(occ.len()).or_default() += 1;
        let mut hashes = Vec::with_capacity(occ.len());
        for o in occ {
            hashes.push(match archives[o.archive].raw(o.index) {
                Ok(bytes) => Some(sha256(&bytes)),
                Err(_) => {
                    read_errors += 1;
                    None
                }
            });
        }
        let all_same = hashes.iter().all(|h| h.is_some() && h == &hashes[0]);
        if all_same {
            identical_ids += 1;
        } else {
            differing_ids += 1;
        }
        if (1..occ.len()).any(|i| occ[..i].iter().any(|p| p.archive == occ[i].archive)) {
            same_archive_ids += 1;
        }
        for i in 0..occ.len() {
            for j in i + 1..occ.len() {
                let key = format!("{} | {}", names[occ[i].archive], names[occ[j].archive]);
                let slot = pairs.entry(key).or_default();
                let same = hashes[i].is_some() && hashes[i] == hashes[j];
                slot[usize::from(!same)] += 1;
            }
        }
        let row: Vec<Value> = occ
            .iter()
            .zip(&hashes)
            .map(|(o, h)| {
                json!({"archive": names[o.archive], "entry": o.index,
                    "stored_length": o.stored_length, "stored_sha256": h})
            })
            .collect();
        writeln!(
            rows,
            "{}",
            json!({"file_id": id, "identical": all_same, "occurrences": row})
        )?;
    }
    rows.flush()?;

    let pair_rows: Vec<Value> = pairs
        .iter()
        .map(|(k, v)| json!({"archives": k, "identical": v[0], "differing": v[1]}))
        .collect();
    let summary = json!({
        "schema_version": 1,
        "game_dir_note": "archive paths are relative to ACU_GAME_DIR",
        "carriers_opened": archives.len(), "index_errors": index_errors,
        "data_ids_total": by_id.len(), "ids_in_one_archive": unique,
        "ids_in_several_archives": dup_ids, "duplicate_ids_byte_identical": identical_ids,
        "duplicate_ids_differing": differing_ids,
        "duplicate_ids_also_repeated_within_one_archive": same_archive_ids,
        "stored_read_errors": read_errors,
        "occurrence_count_histogram": shape,
        "control_ids": control_ids,
        "per_archive": per_archive, "archive_pairs": pair_rows,
        "limitations": [
            "Hashes are of stored (still framed) bytes, not decoded resources.",
            "Control entries (global metadata, prefetch) repeat per archive by design and are \
             counted separately, never as conflicts.",
            "Identical stored bytes mean precedence cannot matter for that ID; differing bytes \
             only show a conflict exists, not which copy the game uses.",
        ],
    });
    fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!(
        "{} archives; {} data IDs; {} in several archives ({} identical, {} differing)",
        archives.len(),
        by_id.len(),
        dup_ids,
        identical_ids,
        differing_ids
    );
    Ok(())
}
