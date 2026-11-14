//! Dump the root motion tracks (track 0 translation, track 1 orientation; ACU-RE-0018) of chosen
//! animations in one package, for comparison with the native root delta (ACU-RE-0058). A clip the
//! decoder rejects is listed with its error. With `--all-tracks` first, every track is fully
//! decoded (`animation::parse`) and written with its bone hash and codec (ACU-RE-0061).
use acu_assets::{Limits, Result, animation, forge::Forge, package, sha256};
use serde_json::json;

fn main() -> Result<()> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let all = args.first().is_some_and(|a| a == "--all-tracks");
    if all {
        args.remove(0);
    }
    if args.len() < 4 {
        return Err(
            "usage: acu-root-tracks [--all-tracks] ARCHIVE ENTRY OUTPUT.json FILE_ID...".into(),
        );
    }
    let entry: usize = args[1].parse()?;
    let wanted: Vec<String> = args[3..].iter().map(|s| s.to_ascii_lowercase()).collect();
    let limits = Limits::default();
    let mut forge = Forge::open(&args[0], limits)?;
    let raw = forge.raw(entry)?;
    let p = package::decode(&raw, limits)?;
    let mut clips = Vec::new();
    for (index, r) in p.resources.iter().enumerate() {
        if r.type_id != "0fa3067f" || !wanted.contains(&r.file_id.to_ascii_lowercase()) {
            continue;
        }
        let bytes = &p.bytes[r.payload.clone()];
        let parsed = if all {
            animation::parse(bytes)
        } else {
            animation::parse_root(bytes)
        };
        let c = match parsed {
            Ok(c) => c,
            Err(e) => {
                // Unsupported variants fail per clip, explicitly, without stopping the dump.
                clips.push(
                    json!({"file_id": r.file_id, "name": r.name, "resource_index": index,
                    "payload_sha256": sha256(bytes), "error": e.to_string()}),
                );
                continue;
            }
        };
        let track = |i: usize| {
            let t = &c.tracks[i];
            json!({"hash": t.hash, "codec": t.codec, "times": t.times, "values": t.values})
        };
        let mut clip = json!({"file_id": r.file_id, "name": r.name, "resource_index": index,
            "payload_sha256": sha256(bytes), "duration": c.duration,
            "translation": track(0), "rotation": track(1)});
        if all {
            clip["tracks"] = (0..c.tracks.len()).map(track).collect();
        }
        clips.push(clip);
    }
    if clips.len() != wanted.len() {
        return Err(format!(
            "found {} of {} requested animations",
            clips.len(),
            wanted.len()
        )
        .into());
    }
    let report = json!({"archive": args[0], "entry": forge.entries[entry],
        "stored_sha256": sha256(&raw), "decoded_sha256": sha256(&p.bytes),
        "decoder": "acu_assets::animation; ACU-RE-0018", "clips": clips});
    std::fs::write(&args[2], serde_json::to_vec_pretty(&report)?)?;
    println!("{}", args[2]);
    Ok(())
}
