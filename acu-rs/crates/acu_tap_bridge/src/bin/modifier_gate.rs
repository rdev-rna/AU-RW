//! Mode A gate for one modifier apply: replay each captured apply from the
//! native entry state (local + model arrays, live modifier object) through
//! `acu_motion` and compare with the native exit state.
//!
//! Capture: `local/native-probe/s2-maxlookat/capture_applies.py` (snap ids
//! 1 = modifier 0x80 bytes, 2/3 = entry local/model, 4/5 = exit local/model;
//! PositionConstraint adds 6.. = target records, 30 = entity scale; Hinge
//! uses 1 = 0xc0 bytes, 6 = sphere entries, 30 = entity scale; Reflex uses
//! 1 = 0x40 bytes, 7 = base record, 8 = model pose object header, 30 = scale,
//! exit 10 = cache records, plus static connector bytes from meta["reflex"]).
//! The class is chosen by the live vtable.
//! Usage: modifier_gate <dump.bin> <meta.json>

use std::collections::BTreeMap;

use acu_motion::bone_modifier::{
    Hinge, MaxLookAt, ModelPose, OrientationConstraint, PositionConstraint,
};
use acu_motion::reflex::{Reflex, ReflexLookup};
use acu_tap_bridge::stats::{Distribution, angle_deg, distance};
use acu_tap_bridge::{
    HINGE_VTABLE, MAX_LOOK_AT_VTABLE, ORIENTATION_CONSTRAINT_VTABLE, POSITION_CONSTRAINT_VTABLE,
    REFLEX_VTABLE, ReflexConnectorBytes, TapEvent, decode_hierarchy, decode_hinge, decode_joints,
    decode_max_look_at, decode_orientation_constraint, decode_position_constraint, decode_reflex,
    decode_reflex_lookup, pair_entry_exit, parse_events,
};

enum Modifier {
    MaxLookAt(MaxLookAt),
    Position(PositionConstraint),
    Orientation(OrientationConstraint),
    Hinge(Hinge),
    /// With connector 0's record sub-index, to find its exit cache record.
    Reflex(Reflex, usize),
}

fn hex_bytes(v: &serde_json::Value) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let s = v.as_str().ok_or("expected a hex string")?;
    Ok((0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16))
        .collect::<Result<_, _>>()?)
}

fn reflex_connectors(
    meta: &serde_json::Value,
    object: u64,
) -> Result<Vec<ReflexConnectorBytes>, Box<dyn std::error::Error>> {
    let conns = meta["reflex"][format!("{object:#x}")]["connectors"]
        .as_array()
        .ok_or_else(|| format!("meta lacks Reflex object {object:#x}"))?;
    conns
        .iter()
        .map(|c| {
            Ok(ReflexConnectorBytes {
                entry: hex_bytes(&c["entry_hex"])?,
                record: hex_bytes(&c["record_hex"])?,
                refs: hex_bytes(&c["refs_hex"])?,
                tree: hex_bytes(&c["tree_hex"])?,
                samples: hex_bytes(&c["samples_hex"])?,
            })
        })
        .collect()
}

impl Modifier {
    fn decode(
        entry: &TapEvent,
        record: &[u8],
        meta: &serde_json::Value,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let vtable = u64::from_le_bytes(record[..8].try_into()?);
        Ok(match vtable {
            MAX_LOOK_AT_VTABLE => Self::MaxLookAt(decode_max_look_at(record)?),
            POSITION_CONSTRAINT_VTABLE => {
                let targets: Vec<&[u8]> = (6..30).map_while(|id| entry.snap(id)).collect();
                let scale = entry.snap(30).ok_or("missing scale snapshot")?;
                let scale = f32::from_le_bytes(scale[..4].try_into()?);
                Self::Position(decode_position_constraint(record, &targets, scale)?)
            }
            ORIENTATION_CONSTRAINT_VTABLE => {
                let targets: Vec<&[u8]> = (6..30).map_while(|id| entry.snap(id)).collect();
                Self::Orientation(decode_orientation_constraint(record, &targets)?)
            }
            HINGE_VTABLE => {
                let spheres = entry.snap(6).ok_or("missing Hinge sphere snapshot")?;
                let scale = entry.snap(30).ok_or("missing scale snapshot")?;
                let scale = f32::from_le_bytes(scale[..4].try_into()?);
                Self::Hinge(decode_hinge(record, spheres, scale)?)
            }
            REFLEX_VTABLE => {
                let conns = reflex_connectors(meta, entry.args[0])?;
                let base = entry.snap(7).ok_or("missing Reflex base snapshot")?;
                let header = entry.snap(8).ok_or("missing model pose header snapshot")?;
                let scale = entry.snap(30).ok_or("missing scale snapshot")?;
                let scale = f32::from_le_bytes(scale[..4].try_into()?);
                let sub = *conns
                    .first()
                    .and_then(|c| c.entry.get(3))
                    .ok_or("Reflex without connectors")?;
                Self::Reflex(
                    decode_reflex(record, base, header, scale, &conns)?,
                    sub as usize,
                )
            }
            v => return Err(format!("unsupported modifier vtable {v:#x}").into()),
        })
    }

    fn owner(&self) -> usize {
        match self {
            Self::MaxLookAt(m) => m.owner,
            Self::Position(c) => c.owner,
            Self::Orientation(c) => c.owner,
            Self::Hinge(h) => h.owner,
            Self::Reflex(r, _) => r.owner,
        }
    }

    /// Reflex returns its connector lookups for the cache-record comparison.
    fn apply(
        &mut self,
        pose: &mut ModelPose,
    ) -> Result<Vec<ReflexLookup>, Box<dyn std::error::Error>> {
        match self {
            Self::MaxLookAt(m) => m.apply(pose),
            Self::Position(c) => c.apply(pose),
            Self::Orientation(c) => c.apply(pose),
            Self::Hinge(h) => h.apply(pose),
            Self::Reflex(r, _) => return Ok(r.apply(pose)?),
        }
        Ok(Vec::new())
    }
}

/// Reflex intermediate check: Rust lookup vs the native exit cache record.
#[derive(Default)]
struct LookupStats {
    compared: usize,
    index_mismatches: usize,
    weight_err: Distribution,
}

#[derive(Default)]
struct OwnerStats {
    rot: Distribution,
    pos: Distribution,
    local_rot: Distribution,
    subtree_rot: Distribution,
    subtree_pos: Distribution,
    native_delta: Distribution,
    native_move: Distribution,
    applies: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: modifier_gate <dump.bin> <meta.json>".into());
    }
    let events = parse_events(&std::fs::read(&args[1])?)?;
    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let hierarchy = meta["hierarchy_hex"]
        .as_str()
        .ok_or("meta lacks hierarchy_hex")?;
    let hierarchy: Vec<u8> = (0..hierarchy.len() / 2)
        .map(|i| u8::from_str_radix(&hierarchy[2 * i..2 * i + 2], 16))
        .collect::<Result<_, _>>()?;
    let parents = decode_hierarchy(&hierarchy)?;
    let n = parents.len();

    let pairs = pair_entry_exit(&events)?;
    let mut owners: BTreeMap<usize, OwnerStats> = BTreeMap::new();
    let (mut outside_changes, mut skipped) = (0usize, 0usize);
    let mut lookups = LookupStats::default();
    for &(i, o) in &pairs {
        let (entry, exit) = (&events[i], &events[o]);
        let (Some(rec), Some(l0), Some(m0), Some(l1), Some(m1)) = (
            entry.snap(1),
            entry.snap(2),
            entry.snap(3),
            exit.snap(4),
            exit.snap(5),
        ) else {
            skipped += 1;
            continue;
        };
        let mut modifier = Modifier::decode(entry, rec, &meta)?;
        let (local_in, model_in) = (decode_joints(l0, n)?, decode_joints(m0, n)?);
        let (local_out, model_out) = (decode_joints(l1, n)?, decode_joints(m1, n)?);
        let mut pose = ModelPose::from_parts(parents.clone(), local_in, model_in.clone())?;
        let ours = modifier.apply(&mut pose)?;
        if let (Modifier::Reflex(_, sub), Some(first), Some(cache)) =
            (&modifier, ours.first(), exit.snap(10))
        {
            let native =
                decode_reflex_lookup(cache.get(32 * sub..).ok_or("cache snapshot too short")?)?;
            lookups.compared += 1;
            if native.leaf != first.leaf || native.samples != first.samples {
                lookups.index_mismatches += 1;
            }
            for (a, b) in native.weights.iter().zip(&first.weights) {
                lookups.weight_err.push(f64::from((a - b).abs()));
            }
        }

        let owner = modifier.owner();
        let subtree = owner..=owner + pose.descendants(owner);
        let s = owners.entry(owner).or_default();
        s.applies += 1;
        s.rot.push(angle_deg(
            pose.model[owner].rotation,
            model_out[owner].rotation,
        ));
        s.pos.push(distance(
            pose.model[owner].position,
            model_out[owner].position,
        ));
        s.local_rot.push(angle_deg(
            pose.local[owner].rotation,
            local_out[owner].rotation,
        ));
        s.native_delta.push(angle_deg(
            model_in[owner].rotation,
            model_out[owner].rotation,
        ));
        s.native_move.push(distance(
            model_in[owner].position,
            model_out[owner].position,
        ));
        for j in subtree.clone().skip(1) {
            s.subtree_rot
                .push(angle_deg(pose.model[j].rotation, model_out[j].rotation));
            s.subtree_pos
                .push(distance(pose.model[j].position, model_out[j].position));
        }
        // The native apply must not touch joints outside the owner's subtree.
        outside_changes += (0..n)
            .filter(|j| !subtree.contains(j))
            .filter(|&j| {
                model_in[j] != model_out[j] || l0[j * 32..j * 32 + 32] != l1[j * 32..j * 32 + 32]
            })
            .count();
    }

    println!(
        "pairs {} skipped {skipped} joints {n} outside-subtree native changes {outside_changes}",
        pairs.len()
    );
    if lookups.compared > 0 {
        let (mean, p99, max) = lookups.weight_err.summary();
        println!(
            "reflex lookups {} index/leaf mismatches {} weight err mean {mean:.2e} p99 {p99:.2e} max {max:.2e}",
            lookups.compared, lookups.index_mismatches
        );
    }
    println!(
        "{:>5} {:>6} | {:>9} {:>9} {:>9} | {:>9} {:>9} | {:>9} | {:>9} {:>9} | {:>9} {:>9}",
        "owner",
        "n",
        "rot_mean",
        "rot_p99",
        "rot_max",
        "pos_p99",
        "pos_max",
        "lrot_max",
        "sub_rmax",
        "sub_pmax",
        "native_dr",
        "native_dp"
    );
    let (mut all_rot, mut all_pos) = (Distribution::default(), Distribution::default());
    for (owner, s) in &owners {
        let (rm, rp, rx) = s.rot.summary();
        let (_, pp, px) = s.pos.summary();
        let (_, _, lx) = s.local_rot.summary();
        let (_, _, sr) = s.subtree_rot.summary();
        let (_, _, sp) = s.subtree_pos.summary();
        let (_, _, dm) = s.native_delta.summary();
        let (_, _, dp) = s.native_move.summary();
        println!(
            "{owner:>5} {:>6} | {rm:>9.2e} {rp:>9.2e} {rx:>9.2e} | {pp:>9.2e} {px:>9.2e} | {lx:>9.2e} | {sr:>9.2e} {sp:>9.2e} | {dm:>9.3} {dp:>9.4}",
            s.applies
        );
        all_rot.push(rx.max(sr.max(0.0)));
        all_pos.push(px.max(sp.max(0.0)));
    }
    // NaN subtree maxima (owners without descendants) are dropped by f64::max.
    let (_, _, worst_rot) = all_rot.summary();
    let (_, _, worst_pos) = all_pos.summary();
    println!(
        "worst rotation error {worst_rot:.3e} deg, worst position error {worst_pos:.3e} units (owner + subtree; native_dr/dp = max native edit)"
    );
    Ok(())
}
