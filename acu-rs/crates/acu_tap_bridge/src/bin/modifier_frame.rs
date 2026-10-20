//! Whole-frame gate for modifier evaluation (slot 1 -> slot 2): replay each
//! captured call of the frame evaluator `0x1400377e0` from its entry local +
//! model arrays through `acu_motion::modifier_order::evaluate_frame` with the
//! five class ports, and compare the result with the native exit arrays.
//!
//! Capture: `tools/native_tap/frame_capture.py` (snap ids 2/3 entry local/model,
//! 4/5 exit local/model, 8 model pose header, 11/12 skip list, 20 SNAPMODS blob,
//! 30 entity scale, 31 Reflex scale; static Reflex connectors in meta["reflex"]).
//! The captured list is replayed in its native order (the order itself is
//! checked by `modifier_order_check`). A second replay with every channel byte
//! forced to `0xFF` shows how much the run post-blend contributes.
//! Usage: modifier_frame <dump.bin> <meta.json>

use std::collections::{BTreeMap, BTreeSet};

use acu_motion::bone_modifier::{
    Hinge, MaxLookAt, ModelPose, OrientationConstraint, PositionConstraint,
};
use acu_motion::modifier_order::{LOD_LISTS, ModifierSlot, evaluate_frame};
use acu_motion::reflex::Reflex;
use acu_tap_bridge::stats::{Distribution, angle_deg, distance};
use acu_tap_bridge::{
    HINGE_VTABLE, MAX_LOOK_AT_VTABLE, ModsEntry, ORIENTATION_CONSTRAINT_VTABLE,
    POSITION_CONSTRAINT_VTABLE, REFLEX_VTABLE, ReflexConnectorBytes, TapEvent, decode_hierarchy,
    decode_hinge, decode_joints, decode_max_look_at, decode_mods_blob,
    decode_orientation_constraint, decode_position_constraint, decode_reflex, pair_entry_exit,
    parse_events,
};

type Error = Box<dyn std::error::Error>;

enum Modifier {
    MaxLookAt(MaxLookAt),
    Position(PositionConstraint),
    Orientation(OrientationConstraint),
    Hinge(Hinge),
    Reflex(Reflex),
    /// Vtable of a class without a port.
    Unported(#[allow(dead_code)] u64),
}

impl Modifier {
    /// Joints other than the owner whose model transforms the apply reads.
    fn reads(&self) -> Vec<usize> {
        match self {
            Self::MaxLookAt(m) => [m.up_node, m.look_at_bone].into_iter().flatten().collect(),
            Self::Position(c) => c.targets.iter().filter_map(|t| t.joint).collect(),
            Self::Orientation(c) => c.targets.iter().filter_map(|t| t.joint).collect(),
            Self::Hinge(h) => h.spheres.iter().filter_map(|s| s.joint).collect(),
            Self::Reflex(r) => r
                .connectors
                .iter()
                .flat_map(|c| [c.frame_joint, c.from_joint, c.to_joint])
                .flatten()
                .collect(),
            Self::Unported(_) => Vec::new(),
        }
    }

    fn apply(&mut self, pose: &mut ModelPose) -> Result<(), Error> {
        match self {
            Self::MaxLookAt(m) => m.apply(pose),
            Self::Position(c) => c.apply(pose),
            Self::Orientation(c) => c.apply(pose),
            Self::Hinge(h) => h.apply(pose),
            Self::Reflex(r) => {
                r.apply(pose)?;
            }
            Self::Unported(_) => {}
        }
        Ok(())
    }
}

fn hex_bytes(v: &serde_json::Value) -> Result<Vec<u8>, Error> {
    let s = v.as_str().ok_or("expected a hex string")?;
    Ok((0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16))
        .collect::<Result<_, _>>()?)
}

fn f32_snap(entry: &TapEvent, id: u16) -> Result<f32, Error> {
    let s = entry.snap(id).ok_or(format!("missing snapshot {id}"))?;
    Ok(f32::from_le_bytes(s[..4].try_into()?))
}

fn reflex_connectors(
    meta: &serde_json::Value,
    object: u64,
) -> Result<Vec<ReflexConnectorBytes>, Error> {
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

fn decode(entry: &TapEvent, m: &ModsEntry, meta: &serde_json::Value) -> Result<Modifier, Error> {
    if m.record.len() < 0x40 {
        return Err(format!("object {:#x} unreadable", m.object).into());
    }
    let targets = || -> Vec<&[u8]> { m.targets.iter().map_while(|t| t.as_deref()).collect() };
    let vtable = u64::from_le_bytes(m.record[..8].try_into()?);
    Ok(match vtable {
        MAX_LOOK_AT_VTABLE => Modifier::MaxLookAt(decode_max_look_at(&m.record)?),
        POSITION_CONSTRAINT_VTABLE => Modifier::Position(decode_position_constraint(
            &m.record,
            &targets(),
            f32_snap(entry, 30)?,
        )?),
        ORIENTATION_CONSTRAINT_VTABLE => {
            Modifier::Orientation(decode_orientation_constraint(&m.record, &targets())?)
        }
        HINGE_VTABLE => Modifier::Hinge(decode_hinge(
            &m.record,
            m.spheres.as_deref().ok_or("missing Hinge spheres")?,
            f32_snap(entry, 30)?,
        )?),
        REFLEX_VTABLE => Modifier::Reflex(decode_reflex(
            &m.record,
            m.base.as_deref().ok_or("missing Reflex base")?,
            entry.snap(8).ok_or("missing model pose header")?,
            f32_snap(entry, 31)?,
            &reflex_connectors(meta, m.object)?,
        )?),
        // Not ported yet: replayed as a no-op and reported, so the residual
        // can be attributed to these classes' bones.
        v => Modifier::Unported(v),
    })
}

/// The skip list at r9: pointer +0, u16 count +10, u16 entries.
fn skip_list(entry: &TapEvent) -> Vec<u16> {
    let (Some(head), Some(items)) = (entry.snap(11), entry.snap(12)) else {
        return Vec::new();
    };
    let count = usize::from(u16::from_le_bytes([head[10], head[11]])).min(items.len() / 2);
    (0..count)
        .map(|i| u16::from_le_bytes([items[2 * i], items[2 * i + 1]]))
        .collect()
}

#[derive(Default)]
struct Errors {
    rot: Distribution,
    pos: Distribution,
}

impl Errors {
    fn compare(
        &mut self,
        ours: &ModelPose,
        native: &[acu_motion::bone_modifier::Joint],
    ) -> (f64, f64) {
        let (mut r, mut p) = (0.0f64, 0.0f64);
        for (a, b) in ours.model.iter().zip(native) {
            r = r.max(angle_deg(a.rotation, b.rotation));
            p = p.max(distance(a.position, b.position));
        }
        self.rot.push(r);
        self.pos.push(p);
        (r, p)
    }

    fn line(&self, name: &str) -> String {
        let (rm, rp, rx) = self.rot.summary();
        let (pm, pp, px) = self.pos.summary();
        format!(
            "{name:<22} frame-max rot deg mean {rm:.2e} p99 {rp:.2e} max {rx:.2e} | pos mean {pm:.2e} p99 {pp:.2e} max {px:.2e}"
        )
    }
}

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: modifier_frame <dump.bin> <meta.json>".into());
    }
    let events = parse_events(&std::fs::read(&args[1])?)?;
    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let parents = decode_hierarchy(&hex_bytes(&meta["hierarchy_hex"])?)?;
    let n = parents.len();

    let (mut gated, mut post_off, mut native_edit) =
        (Errors::default(), Errors::default(), Errors::default());
    let (mut frames, mut skipped, mut skip_lists, mut blended_runs, mut runs_total) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut worst: Option<(f64, usize, usize)> = None;
    let mut per_joint = vec![(0.0f64, 0.0f64); n];
    let mut classes: BTreeMap<usize, BTreeSet<u64>> = BTreeMap::new();
    let mut reads: Vec<(usize, Vec<usize>)> = Vec::new();
    for (i, o) in pair_entry_exit(&events)? {
        let (entry, exit) = (&events[i], &events[o]);
        let (Some(l0), Some(m0), Some(m1), Some(blob)) =
            (entry.snap(2), entry.snap(3), exit.snap(5), entry.snap(20))
        else {
            skipped += 1;
            continue;
        };
        let blob = decode_mods_blob(blob)?;
        if blob.entries.len() != blob.list_count {
            return Err(format!(
                "list of {} truncated to {}",
                blob.list_count,
                blob.entries.len()
            )
            .into());
        }
        let channels = blob.channels.clone().ok_or("run channels unreadable")?;
        if channels.len() < n {
            return Err("fewer channel bytes than joints".into());
        }
        let skip = skip_list(entry);
        skip_lists += usize::from(!skip.is_empty());
        if let Some(m) = blob.entries.iter().find(|m| m.record.len() < 0x40) {
            return Err(format!("object {:#x} unreadable", m.object).into());
        }
        let slots: Vec<ModifierSlot> = blob
            .entries
            .iter()
            .map(|m| {
                let bone = usize::from(u16::from_le_bytes([m.record[0x1c], m.record[0x1d]]));
                ModifierSlot {
                    host: bone,
                    bone: Some(bone),
                    key: u16::from_le_bytes([m.record[36], m.record[37]]),
                    enabled: m.record[38] & 1 != 0,
                }
            })
            .collect();
        if let Some(s) = slots.iter().find(|s| s.bone.is_some_and(|b| b >= n)) {
            return Err(format!("listed bone {:?} outside the skeleton", s.bone).into());
        }
        // The native order is replayed as captured: list `level` = 0..count.
        let mut lists: [Vec<usize>; LOD_LISTS] = Default::default();
        if let Some(list) = lists.get_mut(usize::from(blob.level)) {
            *list = (0..slots.len()).collect();
        }
        let (local_in, model_in) = (decode_joints(l0, n)?, decode_joints(m0, n)?);
        let model_out = decode_joints(m1, n)?;
        let start = ModelPose::from_parts(parents.clone(), local_in, model_in)?;

        // Modifiers are decoded afresh per replay: `apply` takes `&mut self`.
        let run =
            |chan: &[u8], errors: &mut Errors| -> Result<(f64, usize, usize, ModelPose), Error> {
                let mut modifiers: Vec<Modifier> = blob
                    .entries
                    .iter()
                    .map(|m| decode(entry, m, &meta))
                    .collect::<Result<_, _>>()?;
                let mut pose = start.clone();
                let mut failure: Option<Error> = None;
                let runs = evaluate_frame(
                    &mut pose,
                    &lists,
                    &slots,
                    blob.level,
                    &skip,
                    chan,
                    |k, p| {
                        if failure.is_none()
                            && let Err(e) = modifiers[k].apply(p)
                        {
                            failure = Some(e);
                        }
                    },
                )?;
                if let Some(e) = failure {
                    return Err(e);
                }
                let blended = runs.iter().filter(|r| chan[r.bone] != 0xFF).count();
                let (rot, _) = errors.compare(&pose, &model_out);
                Ok((rot, runs.len(), blended, pose))
            };
        let (r, nruns, blended, pose) = run(&channels, &mut gated)?;
        for (j, (a, b)) in pose.model.iter().zip(&model_out).enumerate() {
            let e = &mut per_joint[j];
            e.0 = e.0.max(angle_deg(a.rotation, b.rotation));
            e.1 = e.1.max(distance(a.position, b.position));
        }
        for (m, s) in blob.entries.iter().zip(&slots) {
            let vtable = u64::from_le_bytes(m.record[..8].try_into()?);
            classes
                .entry(s.bone.unwrap_or(usize::MAX))
                .or_default()
                .insert(vtable);
            if frames == 0 {
                reads.push((
                    s.bone.unwrap_or(usize::MAX),
                    decode(entry, m, &meta)?.reads(),
                ));
            }
        }
        run(&[0xFF; 512], &mut post_off)?;
        native_edit.compare(&start, &model_out);
        if worst.is_none_or(|w| r > w.0) {
            worst = Some((r, frames, blob.level as usize));
        }
        runs_total += nruns;
        blended_runs += blended;
        frames += 1;
    }
    println!(
        "frames {frames} skipped {skipped} joints {n} with skip list {skip_lists} | runs {runs_total} with channel != 0xFF {blended_runs}"
    );
    println!("{}", gated.line("gated (channels)"));
    println!("{}", post_off.line("post-blend off (0xFF)"));
    println!("{}", native_edit.line("native edit (in->out)"));
    if let Some((r, f, l)) = worst {
        println!("worst frame {f} (level {l}) rotation error {r:.3e} deg");
    }
    // Residual attribution: the worst joints with the classes listed on the
    // joint and its ancestors (an edit moves the whole subtree).
    // Clean gate for the ported classes plus the evaluation order: joints
    // outside every subtree rooted at an unported class's bone, and outside
    // the subtree of any ported modifier that reads such a joint (to a fixpoint).
    let ported = [
        MAX_LOOK_AT_VTABLE,
        POSITION_CONSTRAINT_VTABLE,
        ORIENTATION_CONSTRAINT_VTABLE,
        HINGE_VTABLE,
        REFLEX_VTABLE,
    ];
    let mut roots: BTreeSet<usize> = classes
        .iter()
        .filter(|(_, c)| c.iter().any(|v| !ported.contains(v)))
        .map(|(&b, _)| b)
        .collect();
    let tainted = |roots: &BTreeSet<usize>, j: usize| {
        let mut k = Some(j);
        while let Some(b) = k {
            if roots.contains(&b) {
                return true;
            }
            k = parents.get(b).copied().flatten();
        }
        false
    };
    loop {
        let grown: Vec<usize> = reads
            .iter()
            .filter(|(owner, r)| !tainted(&roots, *owner) && r.iter().any(|&j| tainted(&roots, j)))
            .map(|(owner, _)| *owner)
            .collect();
        if grown.is_empty() {
            break;
        }
        roots.extend(grown);
    }
    let clean: Vec<usize> = (0..n).filter(|&j| !tainted(&roots, j)).collect();
    let (crot, cpos) = clean.iter().fold((0.0f64, 0.0f64), |(r, p), &j| {
        (r.max(per_joint[j].0), p.max(per_joint[j].1))
    });
    let unported: BTreeSet<u64> = classes
        .values()
        .flatten()
        .copied()
        .filter(|v| !ported.contains(v))
        .collect();
    println!(
        "unported classes {unported:x?}; joints outside their subtrees {} of {n}: max rot {crot:.3e} deg, max pos {cpos:.3e}",
        clean.len()
    );
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| per_joint[b].0.total_cmp(&per_joint[a].0));
    let mut clean_order = clean.clone();
    clean_order.sort_by(|&a, &b| per_joint[b].0.total_cmp(&per_joint[a].0));
    for &j in order.iter().take(8).chain(clean_order.iter().take(8)) {
        let mut chain = Vec::new();
        let mut k = Some(j);
        while let Some(b) = k {
            if let Some(c) = classes.get(&b) {
                chain.push(format!(
                    "{b}:{}",
                    c.iter()
                        .map(|v| format!("{v:x}"))
                        .collect::<Vec<_>>()
                        .join("/")
                ));
            }
            k = parents[b];
        }
        println!(
            "  joint {j:>3} rot max {:.3e} deg pos max {:.3e} | listed on self/ancestors {}",
            per_joint[j].0,
            per_joint[j].1,
            chain.join(" ")
        );
    }
    Ok(())
}
