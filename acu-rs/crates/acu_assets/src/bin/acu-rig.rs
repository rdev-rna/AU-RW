//! Bounded skeleton inventory with exact stored/payload provenance.
use acu_assets::{Limits, Result, forge::Forge, package, sha256, skeleton};
use serde_json::json;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let path = args
        .first()
        .ok_or("usage: acu-rig ARCHIVE ENTRY[,ENTRY] OUTPUT")?;
    let entries = args.get(1).ok_or("entry list required")?;
    let mut forge = Forge::open(path, Limits::default())?;
    let mut reports = Vec::new();
    for entry in entries.split(',') {
        let entry: usize = entry.parse()?;
        let record = forge.entries.get(entry).ok_or("entry outside archive")?;
        let record = serde_json::to_value(record)?;
        let raw = forge.raw(entry)?;
        let p = package::decode(&raw, Limits::default())?;
        for (index, r) in p.resources.iter().enumerate() {
            if r.type_id != skeleton::TYPE_SKELETON {
                continue;
            }
            let payload = &p.bytes[r.payload.clone()];
            let mut report = json!({"archive":path,"entry":entry,"entry_record":record,
                "stored_sha256":sha256(&raw),"resource_index":index,"resource":r,
                "payload_sha256":sha256(payload),"payload_length":payload.len()});
            match skeleton::parse(payload, &r.file_id, Limits::default()) {
                Ok(s) => {
                    report["skeleton"] = serde_json::to_value(s)?;
                }
                Err(e) => {
                    report["error"] = e.to_string().into();
                }
            }
            reports.push(report);
        }
    }
    std::fs::write(
        args.get(2).ok_or("output required")?,
        serde_json::to_vec_pretty(&reports)?,
    )?;
    println!("{} skeleton reports", reports.len());
    Ok(())
}
