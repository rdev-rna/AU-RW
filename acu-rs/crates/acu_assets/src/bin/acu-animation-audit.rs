//! Decode every animation in one package, retaining failures and track provenance.
use acu_assets::{Limits, Result, animation, forge::Forge, package, sha256};
use serde_json::json;
use std::io::Write;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: acu-animation-audit ARCHIVE ENTRY OUTPUT.jsonl".into());
    }
    let entry: usize = args[1].parse()?;
    let limits = Limits::default();
    let mut forge = Forge::open(&args[0], limits)?;
    let raw = forge.raw(entry)?;
    let stored_hash = sha256(&raw);
    let p = package::decode(&raw, limits)?;
    let decoded_hash = sha256(&p.bytes);
    let mut out = std::io::BufWriter::new(std::fs::File::create(&args[2])?);
    let (mut accepted, mut rejected) = (0, 0);
    for (index, r) in p.resources.iter().enumerate() {
        if r.type_id != "0fa3067f" {
            continue;
        }
        let bytes = &p.bytes[r.payload.clone()];
        let mut row = json!({"archive":args[0],"entry":forge.entries[entry],
            "resource_index":index,"resource":r,"payload_sha256":sha256(bytes),
            "stored_sha256":stored_hash,"decoded_sha256":decoded_hash,
            "decoder":"acu_assets::animation; ACU-RE-0018; unsupported variants fail explicitly",
            "actor_ownership":"unknown"});
        match animation::parse(bytes) {
            Ok(c) => {
                accepted += 1;
                row["status"] = json!("decoded");
                row["duration"] = json!(c.duration);
                row["root_start"] = json!(c.tracks[0].values.first());
                row["root_end"] = json!(c.tracks[0].values.last());
                row["tracks"] = json!(c.tracks.iter().enumerate().map(|(i,t)| {
                    let max_delta = t.values.iter().map(|v| {
                        v.iter().zip(t.values[0]).map(|(a,b)| (a-b).abs()).fold(0.0_f32,f32::max)
                    }).fold(0.0_f32,f32::max);
                    json!({"track_index":i,"bone_hash":format!("{:08x}",t.hash),
                        "codec":t.codec,"key_count":t.times.len(),"max_component_delta":max_delta,
                        "data_offset":t.offset,"data_length":c.tracks.get(i+1).map(|n|n.offset).unwrap_or(bytes.len())-t.offset,
                        "channel": match t.codec {0..=8=>"rotation",9|11=>"translation",12..=14=>"scalar",_=>"event"}})
                }).collect::<Vec<_>>());
            }
            Err(e) => {
                rejected += 1;
                row["status"] = json!("unsupported");
                row["error"] = json!(e.to_string());
            }
        }
        serde_json::to_writer(&mut out, &row)?;
        writeln!(&mut out)?;
    }
    out.flush()?;
    println!(
        "decoded={accepted} unsupported={rejected} output={}",
        args[2]
    );
    Ok(())
}
