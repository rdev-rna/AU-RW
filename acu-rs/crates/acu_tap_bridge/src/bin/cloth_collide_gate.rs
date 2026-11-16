//! Collision-op replay against the game, ACU-RE-0081.
//!
//!     cloth_collide_gate <collide-capture.bin>
//!
//! Input: `tools/native_tap/cloth_collide_capture.py`. Each flap step entry (hook 0) is followed,
//! on the same thread, by its collision-path calls (hook 1, entry and exit; the discrete path
//! `0x1423d68d0` unless captured with `--swept`). Two
//! checks per collision call:
//! - **kernel**: our `collide` (the path the settings select) on the game's own entry positions, previous and records,
//!   against the game's exit positions (isolates the collision op);
//! - **pipeline**: our step replayed op by op from the captured step entry; our positions at
//!   each collision entry and exit against the game's (isolates the ops before it).
//!
//! A group counts only when the first call's previous equals the step entry's positions
//! (integrate's write), which rejects calls attributed to the wrong step.
//!
//! Needs ACU_GAME_DIR (the flap's `Cloth` resource, BelleEpoque_assets entry 5882). Settings are
//! the external `SoftBodySettings`, as in `cloth_gate`. Prints one JSON report.
use acu_assets::{Limits, cloth, forge::Forge, package};
use acu_sim::cloth as sim;
use acu_sim::cloth::collision;
use acu_tap_bridge::cloth::{RECORD, decode_shape, snap, vec4s};
use acu_tap_bridge::{TapEvent, parse_events};
use glam::Vec4;
use serde_json::json;
use std::path::Path;

const FLAP_ID: &str = "0000001460e45331";

#[derive(Default, Clone, Copy)]
struct Err3 {
    max: f32,
    sum: f64,
    n: usize,
}
impl Err3 {
    fn add(&mut self, d: f32) {
        self.max = self.max.max(d);
        self.sum += d as f64;
        self.n += 1;
    }
    fn json(&self) -> serde_json::Value {
        json!({"max_mm": self.max * 1000.0,
               "mean_mm": if self.n > 0 { self.sum / self.n as f64 * 1000.0 } else { 0.0 }, "n": self.n})
    }
}

fn dist(a: Vec4, b: Vec4) -> f32 {
    (a.truncate() - b.truncate()).length()
}

fn shapes(e: &TapEvent, list: u16, recs: u16) -> Result<Vec<collision::Shape>, String> {
    let l = snap(e, list).map_err(|x| x.to_string())?;
    let count = u32::from_le_bytes(l[8..12].try_into().unwrap()) as usize;
    let r = snap(e, recs).map_err(|x| x.to_string())?;
    if r.len() < count * RECORD {
        return Err("more shape records than captured".into());
    }
    (0..count)
        .map(|i| decode_shape(&r[i * RECORD..(i + 1) * RECORD]).map_err(|x| x.to_string()))
        .collect()
}

/// Opcodes in execution order, repeat blocks expanded.
fn expand(schedule: &[(u32, u32)]) -> Vec<u32> {
    let mut ops = Vec::new();
    let mut i = 0;
    while i < schedule.len() {
        let (op, cnt) = schedule[i];
        if op == 10 {
            let end = schedule[i..].iter().position(|x| x.0 == 11).unwrap() + i;
            for _ in 0..cnt {
                ops.extend(schedule[i + 1..end].iter().map(|x| x.0));
            }
            i = end + 1;
            continue;
        }
        ops.push(op);
        i += 1;
    }
    ops
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        return Err("usage: cloth_collide_gate collide-capture.bin [--worst]".into());
    }
    let root = std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: ACU_GAME_DIR is unset")?;
    let mut forge = Forge::open(
        Path::new(&root).join("DataPC_ACU_LGS_BelleEpoque_assets.forge"),
        Limits::default(),
    )?;
    let raw = forge.raw(5882)?;
    let p = package::decode(&raw, Limits::default())?;
    let r = &p.resources[0];
    let c = cloth::parse(&p.bytes[r.payload.clone()], &r.file_id)?;
    if c.state_id != FLAP_ID {
        return Err(format!("unexpected cloth state {}", c.state_id).into());
    }
    let blob = &c.lod0_blob;
    let n = blob.particle_count()?;
    let sr = p
        .resources
        .iter()
        .find(|q| q.type_id == "f7a7e4df")
        .ok_or("no SoftBodySettings in the entry")?;
    let ext = cloth::soft_body_settings(&p.bytes[sr.payload.clone()], &sr.file_id)?;
    let base = sim::asset::solver(blob, Some(&ext))?;
    let params = sim::asset::collision_params(blob, Some(&ext))?.ok_or("no collision params")?;
    let ops = expand(&base.schedule);
    let cap = (n + 15) & !15;

    let events = parse_events(&std::fs::read(&args[1])?)?;
    // Group: step entry, then that thread's collision entry/exit pairs until its next step.
    struct Call {
        entry: usize,
        exit: Option<usize>,
    }
    let mut groups: Vec<(usize, Vec<Call>)> = Vec::new();
    let mut open: std::collections::HashMap<u32, usize> = Default::default();
    let mut pending: std::collections::HashMap<u32, Option<usize>> = Default::default();
    for (i, e) in events.iter().enumerate() {
        match (e.hook, e.phase) {
            (0, 0) => {
                open.insert(e.tid, groups.len());
                groups.push((i, Vec::new()));
            }
            (1, 0) => {
                // Only this body's calls (*obj, snapshot 20); the object also serves others.
                let body = open.get(&e.tid).map(|&g| events[groups[g].0].args[1]);
                let this = snap(e, 20)
                    .ok()
                    .map(|b| u64::from_le_bytes(b[..8].try_into().unwrap()));
                pending.insert(e.tid, None);
                if let (Some(&g), Some(b)) = (open.get(&e.tid), body)
                    && this == Some(b)
                {
                    groups[g].1.push(Call {
                        entry: i,
                        exit: None,
                    });
                    pending.insert(e.tid, Some(g));
                }
            }
            (1, _) => {
                if let Some(Some(g)) = pending.remove(&e.tid)
                    && let Some(last) = groups[g].1.last_mut()
                    && last.exit.is_none()
                {
                    last.exit = Some(i);
                }
            }
            _ => {}
        }
    }

    let (mut kernel, mut pipe_in, mut pipe_out) =
        (Err3::default(), Err3::default(), Err3::default());
    let mut per_iter_in = [Err3::default(); 4];
    let mut per_iter_out = [Err3::default(); 4];
    let mut kernel_worst = (0.0f32, 0usize, 0usize, 0usize);
    let mut complete = 0usize;
    let mut unpaired = 0usize;
    let mut counts = std::collections::BTreeMap::new();
    for (gi, (si, calls)) in groups.iter().enumerate() {
        *counts.entry(calls.len()).or_insert(0usize) += 1;
        if calls.len() != 4 || calls.iter().any(|c| c.exit.is_none()) {
            continue;
        }
        let se = &events[*si];
        // Pairing check: integrate (op 6) sets prev = the entry positions, so the first
        // collision call's prev must equal this step's entry exactly.
        let entry_pos = vec4s(snap(se, 2)?, n)?;
        let first_prev = vec4s(snap(&events[calls[0].entry], 22)?, n)?;
        if (0..n).any(|i| entry_pos[i].truncate() != first_prev[i].truncate()) {
            unpaired += 1;
            continue;
        }
        complete += 1;
        let step_shapes = shapes(se, 9, 10)?;
        let dt = f32::from_le_bytes(snap(se, 7)?[..4].try_into().unwrap());
        let wind = snap(se, 8)?;
        let mut cl = base.clone();
        cl.state.pos = vec4s(snap(se, 2)?, n)?;
        cl.state.prev = vec4s(snap(se, 3)?, n)?;
        cl.state.target = vec4s(snap(se, 5)?, n)?;
        let tn = vec4s(snap(se, 6)?, n)?;
        cl.normals = vec4s(snap(se, 4)?, cap)?;
        cl.drag.0 = Vec4::new(
            f32::from_le_bytes(wind[0..4].try_into().unwrap()),
            f32::from_le_bytes(wind[4..8].try_into().unwrap()),
            f32::from_le_bytes(wind[8..12].try_into().unwrap()),
            0.0,
        );
        cl.shapes = step_shapes;
        cl.collision = Some(params.clone());
        let mut k = 0usize;
        let debug = args
            .iter()
            .position(|a| a == "--trace-group")
            .is_some_and(|j| args[j + 1].parse::<usize>().ok() == Some(gi));
        let g0 = vec4s(snap(&events[calls[0].entry], 21)?, n)?;
        let gap = |c: &sim::Cloth| {
            let (mut mx, mut sum, mut m) = (0.0f32, 0.0f64, 0usize);
            for (g, &p) in g0.iter().zip(&c.state.pos) {
                if g.w >= 0.0 {
                    let d = dist(p, *g);
                    mx = mx.max(d);
                    sum += d as f64;
                    m += 1;
                }
            }
            (mx * 1000.0, sum / m as f64 * 1000.0)
        };
        if debug {
            eprintln!(
                "start: gap to game collision-1 input (max, mean mm) {:?}",
                gap(&cl)
            );
        }
        for &op in &ops {
            if debug && k == 0 && op != 7 {
                eprintln!("before op {op}: {:?}", gap(&cl));
            }
            if debug && op == 7 {
                let gk = vec4s(snap(&events[calls[k].entry], 21)?, n)?;
                let (mut mx, mut sm, mut m, mut wi) = (0.0f32, 0.0f64, 0usize, 0usize);
                for (i, (g, &p)) in gk.iter().zip(&cl.state.pos).enumerate() {
                    if g.w >= 0.0 {
                        let d = dist(p, *g);
                        if d > mx {
                            mx = d;
                            wi = i;
                        }
                        sm += d as f64;
                        m += 1;
                    }
                }
                eprintln!(
                    "collision {k} entry: mean {:.4} max {:.3} mm (particle {wi})",
                    sm / m as f64 * 1e3,
                    mx * 1e3
                );
            }
            if op == 7 {
                let call = &calls[k];
                let (e, x) = (&events[call.entry], &events[call.exit.unwrap()]);
                let gin = vec4s(snap(e, 21)?, n)?;
                let gprev = vec4s(snap(e, 22)?, n)?;
                let gout = vec4s(snap(x, 25)?, n)?;
                // Kernel: the game's own input through our collision.
                let mut st = sim::State {
                    pos: gin.clone(),
                    prev: gprev.clone(),
                    target: cl.state.target.clone(),
                };
                let sh = shapes(e, 23, 24)?;
                collision::collide(&mut st, &sh, &params);
                for i in 0..n {
                    if gin[i].w < 0.0 {
                        continue;
                    }
                    let d = dist(st.pos[i], gout[i]);
                    kernel.add(d);
                    if d > kernel_worst.0 {
                        kernel_worst = (d, gi, k, i);
                    }
                    let din = dist(cl.state.pos[i], gin[i]);
                    pipe_in.add(din);
                    per_iter_in[k].add(din);
                }
                // Pipeline: our own state through the op.
                let one = sim::Cloth {
                    schedule: vec![(7, 1)],
                    ..cl.clone()
                };
                cl = one;
                cl.step(dt, &tn);
                for i in 0..n {
                    if gin[i].w >= 0.0 {
                        pipe_out.add(dist(cl.state.pos[i], gout[i]));
                        per_iter_out[k].add(dist(cl.state.pos[i], gout[i]));
                    }
                }
                if k == 3 && args.iter().any(|a| a == "--per-step") {
                    let (mut mx, mut sm, mut m) = (0.0f32, 0.0f64, 0usize);
                    for i in 0..n {
                        if gin[i].w >= 0.0 {
                            let d = dist(cl.state.pos[i], gout[i]);
                            mx = mx.max(d);
                            sm += d as f64;
                            m += 1;
                        }
                    }
                    eprintln!(
                        "group {gi} last-collision exit: mean {:.4} max {:.3} mm",
                        sm / m as f64 * 1e3,
                        mx * 1e3
                    );
                }
                k += 1;
            } else {
                let one = sim::Cloth {
                    schedule: vec![(op, 1)],
                    ..cl.clone()
                };
                cl = one;
                cl.step(dt, &tn);
            }
        }
    }
    let report = json!({
        "finding": "ACU-RE-0081",
        "capture": args[1],
        "steps": groups.len(),
        "calls_per_step": counts,
        "complete_steps": complete,
        "unpaired_steps": unpaired,
        "kernel_game_input": kernel.json(),
        "kernel_worst": {"error_mm": kernel_worst.0 * 1000.0, "step_group": kernel_worst.1,
                         "iteration": kernel_worst.2, "particle": kernel_worst.3},
        "pipeline_collision_entry": pipe_in.json(),
        "pipeline_collision_entry_by_iteration": per_iter_in.iter().map(|e| e.json()).collect::<Vec<_>>(),
        "pipeline_collision_exit": pipe_out.json(),
        "pipeline_collision_exit_by_iteration": per_iter_out.iter().map(|e| e.json()).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    if complete == 0 {
        return Err("no complete step (4 collision calls with exits): unavailable".into());
    }
    Ok(())
}
