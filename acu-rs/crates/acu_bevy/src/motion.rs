//! Archive-backed selected original clips. No extracted animation dependency.
use crate::rig::Library;
use acu_assets::{Limits, Result, animation, forge::Forge, sha256};
use acu_motion::unity::{Matcher, Profile, Take};
use std::path::Path;

pub fn load(dir: &Path, library: &Library) -> Result<(Matcher, serde_json::Value)> {
    let limits = Limits::default();
    let mut f = Forge::open(dir.join("DataPC.forge"), limits)?;
    let record = f.entries[928].clone();
    let raw = f.raw(928)?;
    let stored = sha256(&raw);
    let p = crate::io::decode_package(raw, limits)?;
    let mut takes = Vec::new();
    let mut sources = Vec::new();
    for (id, profile) in [
        ("00000003836e1d49", Profile::Idle), // animated IdleLow_Footr; old LookAt_Fd0 was static
        ("0000000fc08fd67a", Profile::Walk),
        ("000000016737c2c4", Profile::Sprint),
        ("000000137c558075", Profile::Walk), // decoded +X strafe, finding0019
        ("000000137c55802d", Profile::Walk), // decoded -X strafe
        ("000000137c55824d", Profile::Walk), // decoded -Y backward
        ("0000000fed5e3950", Profile::Sprint), // SwayRt90: +X root
        ("000000080594a3c6", Profile::Sprint), // SwayLt90: -X root
    ] {
        let r = p
            .resources
            .iter()
            .find(|r| r.file_id == id)
            .ok_or("navigation clip absent")?;
        let bytes = &p.bytes[r.payload.clone()];
        let clip = animation::parse(bytes)?;
        let rig = library.motion_rig()?;
        let mapped = clip
            .tracks
            .iter()
            .skip(2)
            .filter(|t| t.codec <= 14 && rig.hashes.contains(&t.hash))
            .count();
        sources.push(serde_json::json!({"resource":r,"payload_sha256":sha256(bytes),"duration":clip.duration,
            "tracks":clip.tracks.len(),"matched_transform_tracks":mapped,"mapping_offset":clip.mapping_offset,
            "root_start":clip.tracks[0].values.first(),"root_end":clip.tracks[0].values.last(),
            "track_map":clip.tracks.iter().map(|t|serde_json::json!({"hash":format!("{:08x}",t.hash),"codec":t.codec,"offset":t.offset,"keys":t.times.len()})).collect::<Vec<_>>(),
            "actor_ownership":"unknown; navigation clip applied to compatible Arno rig"}));
        takes.push(Take {
            name: r.name.clone(),
            file_id: r.file_id.clone(),
            profile,
            clip,
        });
    }
    let report = serde_json::json!({"archive":"DataPC.forge","entry":record,"stored_sha256":stored,"clips":sources,
        "adapter":"Imported FH pose + prediction_geometry_cost over cached Unity joint/root features; actor-relative direction; 30Hz phases; 10Hz and direction-triggered searches; 160ms local pose blend",
        "coverage":"Event objects and actor ownership unknown; authored facing/trajectory warp/collision; no native graph, cloth or parkour parity"});
    Ok((Matcher::new(library.motion_rig()?, takes)?, report))
}

/// Load one explicit animation resource from the navigation package. This is
/// used by binding-aware comparisons while those graph-selected takes remain
/// outside the production matcher.
#[cfg(test)]
pub fn load_clip(dir: &Path, file_id: &str) -> Result<(animation::Clip, String)> {
    let limits = Limits::default();
    let mut forge = Forge::open(dir.join("DataPC.forge"), limits)?;
    let raw = forge.raw(928)?;
    let package = crate::io::decode_package(raw, limits)?;
    let resource = package
        .resources
        .iter()
        .find(|resource| resource.file_id == file_id)
        .ok_or("animation resource absent from navigation package")?;
    let payload = &package.bytes[resource.payload.clone()];
    Ok((animation::parse(payload)?, sha256(payload)))
}
