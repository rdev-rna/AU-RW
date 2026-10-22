//! Candidate reference scan (#132). For every resource payload, report each place where eight
//! bytes equal a known file ID (an entry ID or any resource ID in any baseline carrier), at
//! any byte offset. A hit is an observation that the bytes are equal, not a claim about what
//! the field means; it can be coincidence or a different kind of value. Writes TSV only.
//!
//! Usage: acu-links REPORT_DIR [--archive NAME_SUBSTRING]... [--max-entries N] [--min-bits N]
//! Targets always come from every carrier; the flags only limit which sources are scanned.
use acu_assets::{
    Limits, Result, control,
    forge::{self, Forge},
};
use serde_json::json;
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
};

/// Every `(offset, value)` in `bytes` whose little-endian u64 is in `ids`, excluding `own`.
/// File IDs have their top three bytes zero (observed for all IDs in the baseline), so the
/// window is rejected cheaply before the set lookup. Offsets are unaligned on purpose.
fn find_ids(bytes: &[u8], ids: &HashSet<u64>, own: Option<u64>) -> Vec<(usize, u64)> {
    let mut hits = Vec::new();
    if bytes.len() < 8 {
        return hits;
    }
    for offset in 0..=bytes.len() - 8 {
        if bytes[offset + 7] != 0 || bytes[offset + 6] != 0 || bytes[offset + 5] != 0 {
            continue;
        }
        let value = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        if Some(value) != own && ids.contains(&value) {
            hits.push((offset, value));
        }
    }
    hits
}

fn bit_length(value: u64) -> u32 {
    64 - value.leading_zeros()
}

/// A fake ID with the same bit length as `id` that is not in `real`. Used only to measure how
/// often a same-sized value matches by coincidence. `None` when no distinct fake exists.
fn control_id(id: u64, real: &HashSet<u64>) -> Option<u64> {
    if bit_length(id) < 2 {
        return None;
    }
    let mask = (1u64 << (bit_length(id) - 1)) - 1;
    [
        0x5A5A_5A5A_5A5A_5A5Au64,
        0x3C3C_3C3C_3C3C_3C3C,
        0x0F0F_0F0F_0F0F_0F0F,
    ]
    .iter()
    .map(|k| id ^ (k & mask))
    .find(|fake| *fake != id && !real.contains(fake))
}

fn parse_id(text: &str) -> Option<u64> {
    u64::from_str_radix(text, 16).ok()
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(args.next().ok_or(
        "usage: acu-links REPORT_DIR [--archive SUBSTR]... [--max-entries N] [--min-bits N]",
    )?);
    let (mut filters, mut max_entries) = (Vec::<String>::new(), usize::MAX);
    // Pilot calibration (BelleEpoque): control hits ~0 from 24 bits up, comparable below.
    let mut min_bits = 24u32;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--archive" => filters.push(args.next().ok_or("--archive needs a value")?),
            "--max-entries" => {
                max_entries = args
                    .next()
                    .ok_or("--max-entries needs a value")?
                    .parse()
                    .map_err(|_| "--max-entries must be a number")?
            }
            "--min-bits" => {
                min_bits = args
                    .next()
                    .ok_or("--min-bits needs a value")?
                    .parse()
                    .map_err(|_| "--min-bits must be a number")?
            }
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    if output.exists() {
        return Err("report directory already exists; choose a fresh output path".into());
    }
    let game = PathBuf::from(
        std::env::var("ACU_GAME_DIR").map_err(|_| "set ACU_GAME_DIR to the original install")?,
    );
    let limits = Limits::default();
    let mut archives = Vec::new();
    for path in forge::discover(&game)? {
        let name = path
            .strip_prefix(&game)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        archives.push((name, Forge::open(&path, limits)?));
    }

    // Pass 1: every entry ID and resource ID, from the cheap metadata frame only.
    let mut ids: HashSet<u64> = HashSet::new();
    let mut metadata_failures = 0usize;
    // Per archive: data entries and resource records, from the metadata table alone. This is a
    // parse independent of the full-package scan, so the asset index can be checked against it.
    let mut records_per_archive = BTreeMap::<String, [u64; 2]>::new();
    for (name, archive) in archives.iter_mut() {
        eprintln!("ids: {name}");
        for i in 0..archive.entries.len() {
            if control::is_control(&archive.entries[i].file_id) {
                continue;
            }
            ids.extend(parse_id(&archive.entries[i].file_id));
            let counts = records_per_archive.entry(name.clone()).or_default();
            counts[0] += 1;
            match archive.read_metadata(i) {
                Ok(meta) => {
                    counts[1] += meta.records.len() as u64;
                    ids.extend(meta.records.iter().filter_map(|r| parse_id(&r.file_id)))
                }
                Err(_) => metadata_failures += 1,
            }
        }
    }
    eprintln!("{} distinct target IDs", ids.len());
    let control_ids: HashSet<u64> = ids.iter().filter_map(|&id| control_id(id, &ids)).collect();
    // Per ID bit length: how many targets, real hits, and hits on the same-sized fake IDs.
    let mut by_bits = BTreeMap::<u32, [u64; 3]>::new();
    for &id in &ids {
        by_bits.entry(bit_length(id)).or_default()[0] += 1;
    }

    // Pass 2: scan resource payloads.
    fs::create_dir_all(&output)?;
    let mut rows = BufWriter::new(File::create(output.join("links.tsv"))?);
    writeln!(
        rows,
        "archive\tentry\tordinal\tsrc_file_id\toffset\tdst_file_id\thits"
    )?;
    let (mut scanned_entries, mut scanned_bytes, mut hit_count, mut rows_written) =
        (0usize, 0u64, 0u64, 0u64);
    let mut failures = Vec::new();
    let mut failure_count = 0usize;
    let mut align8 = BTreeMap::<usize, u64>::new();
    let mut align4 = BTreeMap::<usize, u64>::new();
    'archives: for (name, archive) in archives.iter_mut() {
        if !filters.is_empty() && !filters.iter().any(|f| name.contains(f.as_str())) {
            continue;
        }
        eprintln!("scan: {name}");
        for i in 0..archive.entries.len() {
            if control::is_control(&archive.entries[i].file_id) {
                continue;
            }
            if scanned_entries >= max_entries {
                break 'archives;
            }
            scanned_entries += 1;
            let package = match archive.read_package(i) {
                Ok(p) => p,
                Err(e) => {
                    failure_count += 1;
                    if failures.len() < 50 {
                        failures.push(json!({"archive": name, "entry": i, "error": e.to_string()}));
                    }
                    continue;
                }
            };
            for (ordinal, res) in package.resources.iter().enumerate() {
                let Some(payload) = package.bytes.get(res.payload.clone()) else {
                    continue;
                };
                scanned_bytes += payload.len() as u64;
                let own = parse_id(&res.file_id);
                for (_, fake) in find_ids(payload, &control_ids, None) {
                    by_bits.entry(bit_length(fake)).or_default()[2] += 1;
                }
                // Below `min_bits` a match is indistinguishable from an ordinary integer
                // (see hits_by_id_bits); those hits are counted but not written.
                let mut kept = BTreeMap::<u64, (usize, u64)>::new();
                for (offset, value) in find_ids(payload, &ids, own) {
                    hit_count += 1;
                    by_bits.entry(bit_length(value)).or_default()[1] += 1;
                    if bit_length(value) >= min_bits {
                        *align8.entry(offset % 8).or_default() += 1;
                        *align4.entry(offset % 4).or_default() += 1;
                        kept.entry(value).or_insert((offset, 0)).1 += 1;
                    }
                }
                let mut ordered: Vec<_> = kept.into_iter().collect();
                ordered.sort_by_key(|(_, (offset, _))| *offset);
                for (value, (offset, count)) in ordered {
                    rows_written += 1;
                    writeln!(
                        rows,
                        "{name}\t{i}\t{ordinal}\t{}\t{offset}\t{value:016x}\t{count}",
                        res.file_id
                    )?;
                }
            }
        }
    }
    rows.flush()?;
    let summary = json!({
        "schema_version": 1,
        "target_ids": ids.len(), "metadata_failures": metadata_failures,
        "entries_scanned": scanned_entries, "payload_bytes_scanned": scanned_bytes,
        "metadata_records_per_archive": records_per_archive.iter().map(|(name, v)| (name.clone(),
            json!({"data_entries": v[0], "resource_records": v[1]}))).collect::<BTreeMap<_, _>>(),
        "candidate_hits_all_sizes": hit_count, "rows_written": rows_written,
        "min_id_bits": min_bits,
        "entry_decode_failures": failure_count, "failure_samples": failures,
        "offset_mod_8": align8, "offset_mod_4": align4,
        "noise_calibration_note": "control = hits on fake IDs of the same bit length that are \
             not real IDs; where control is comparable to real, a hit is not evidence",
        "hits_by_id_bits": by_bits.iter().map(|(bits, v)| (bits.to_string(),
            json!({"targets": v[0], "real_hits": v[1], "control_hits": v[2]}))).collect::<BTreeMap<_, _>>(),
        "filters": filters, "max_entries": if max_entries == usize::MAX { None } else { Some(max_entries) },
        "limitations": [
            "Hits are equal bytes only. They may be coincidence or a non-reference value.",
            "Only 64-bit file IDs are visible; handles, hashes and 32-bit references are not.",
            "Resource payload ranges as given by the package table; frames are not re-validated.",
            "A resource's own ID is excluded from its hits.",
            "One row per (source resource, target): first offset and the number of hits.",
            "IDs below min_id_bits are not written; their raw counts are in hits_by_id_bits.",
            "min_id_bits was chosen from the control columns of a pilot run, not assumed.",
        ],
    });
    fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!(
        "{scanned_entries} entries; {hit_count} raw hits, {rows_written} rows (>= {min_bits} bits); {failure_count} decode failures"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &[u64]) -> HashSet<u64> {
        values.iter().copied().collect()
    }

    #[test]
    fn control_ids_keep_bit_length_and_avoid_real_ids() {
        let real = ids(&[0x66, 0x0a, 0x0000_0007_c244_8f42, 0x0000_000f_fb7b_e6d3]);
        for &id in &real {
            let fake = control_id(id, &real).unwrap();
            assert_eq!(bit_length(fake), bit_length(id));
            assert_ne!(fake, id);
            assert!(!real.contains(&fake));
        }
        assert_eq!(control_id(0, &real), None);
        assert_eq!(control_id(1, &real), None);
    }

    #[test]
    fn control_skips_a_fake_that_collides_with_a_real_id() {
        let first = 0x40u64 ^ (0x5A5A_5A5A_5A5A_5A5A & 0x3f);
        let real = ids(&[0x40, first]);
        let fake = control_id(0x40, &real).unwrap();
        assert_ne!(fake, first);
        assert_eq!(bit_length(fake), 7);
    }

    #[test]
    fn finds_unaligned_ids_and_reports_offsets() {
        let id = 0x0000_0007_c244_8f42u64;
        let mut bytes = vec![0xAAu8; 3];
        bytes.extend_from_slice(&id.to_le_bytes());
        bytes.extend_from_slice(&[0xBB; 5]);
        assert_eq!(find_ids(&bytes, &ids(&[id]), None), vec![(3, id)]);
    }

    #[test]
    fn excludes_own_id_but_keeps_other_ids() {
        let (a, b) = (0x0000_0007_c244_8f42u64, 0x0000_000f_fb7b_e6d3u64);
        let mut bytes = a.to_le_bytes().to_vec();
        bytes.extend_from_slice(&b.to_le_bytes());
        assert_eq!(find_ids(&bytes, &ids(&[a, b]), Some(a)), vec![(8, b)]);
    }

    #[test]
    fn ignores_values_with_nonzero_high_bytes() {
        // Same low bytes as a known ID but a high byte set: not a match.
        let known = 0x0000_0007_c244_8f42u64;
        let bytes = (known | 0x01u64 << 56).to_le_bytes();
        assert!(find_ids(&bytes, &ids(&[known]), None).is_empty());
    }

    #[test]
    fn handles_short_buffers_and_exact_edge() {
        let id = 0x0000_0001_0000_0002u64;
        assert!(find_ids(&[], &ids(&[id]), None).is_empty());
        assert!(find_ids(&id.to_le_bytes()[..7], &ids(&[id]), None).is_empty());
        assert_eq!(
            find_ids(&id.to_le_bytes(), &ids(&[id]), None),
            vec![(0, id)]
        );
    }

    #[test]
    fn overlapping_windows_do_not_invent_matches() {
        // Zeros everywhere: value 0 is only a hit if 0 is a known ID.
        assert!(find_ids(&[0u8; 32], &ids(&[0x0000_0007_c244_8f42]), None).is_empty());
        assert_eq!(find_ids(&[0u8; 9], &ids(&[0]), None).len(), 2);
    }
}
