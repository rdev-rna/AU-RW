//! Per-step cloth replay against the game, ACU-RE-0081.
//!
//!     cloth_gate <capture.bin> [--blob-settings | --live-body] [--repeat N] [--no-collision]
//!                [--skip OPS] [--trace K] [--tether-check] [--free-run]
//!
//! Diagnostics (not native modes): `--skip OPS` drops opcodes, `--trace K` prints step K op by
//! op, `--worst K` lists step K's worst particles, `--normals` lists reversed normals,
//! `--stiffness live|a,b,c`, `--drag-scale X`, `--coll friction=..,thickness=..`, `--dump-maps`.
//! `--free-run` steps our own state through the whole capture, carried between steps by the
//! collider-derived transform, instead of reloading each captured entry.
//!
//! Needs ACU_GAME_DIR (the flap's `Cloth` resource: BelleEpoque_assets entry 5882). Each
//! captured step's inputs (positions, previous, normals, targets, target normals, dt, wind,
//! shape records) are loaded into the solver built from the resource, stepped once, and the
//! result compared with the game's exit snapshot. Prints one JSON report.
//!
//! Settings: by default the external `SoftBodySettings` the LOD names (decoded, resource 1 of
//! the entry), as the viewer uses them; `--blob-settings` uses only the blob's record;
//! `--live-body` copies the captured body's settings block instead. The report compares the
//! decoded values with the live body's. `--repeat N` replaces the repeat-block count (op 10),
//! which the native LOD rule (`0x14240c090`) may reduce.
use acu_assets::{Limits, cloth, forge::Forge, package};
use acu_sim::cloth as sim;
use acu_tap_bridge::cloth::{Step, decode_step};
use acu_tap_bridge::{pair_entry_exit, parse_events};
use glam::Vec4;
use serde_json::json;
use std::path::Path;

const FLAP_ID: &str = "0000001460e45331";

fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

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
        json!({"max": self.max, "mean": if self.n > 0 { self.sum / self.n as f64 } else { 0.0 }})
    }
}

fn dist(a: Vec4, b: Vec4) -> f32 {
    (a.truncate() - b.truncate()).length()
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        return Err("usage: cloth_gate capture.bin [--blob-settings | --live-body] [--repeat N] [--no-collision] [--skip OPS] [--trace K] [--tether-check]".into());
    }
    let repeat = args
        .iter()
        .position(|a| a == "--repeat")
        .map(|i| args[i + 1].parse::<u32>())
        .transpose()?;
    let no_collision = args.iter().any(|a| a == "--no-collision");
    let blob_settings = args.iter().any(|a| a == "--blob-settings");
    let live_body = args.iter().any(|a| a == "--live-body");
    let trace = args
        .iter()
        .position(|a| a == "--trace")
        .map(|i| args[i + 1].parse::<usize>())
        .transpose()?;

    let root = std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: ACU_GAME_DIR is unset")?;
    let mut forge = Forge::open(
        Path::new(&root).join("DataPC_ACU_LGS_BelleEpoque_assets.forge"),
        Limits::default(),
    )?;
    let raw = forge.raw(5882)?;
    let p = package::decode(&raw, Limits::default())?;
    let r = &p.resources[0];
    let payload = &p.bytes[r.payload.clone()];
    let c = cloth::parse(payload, &r.file_id)?;
    if c.state_id != FLAP_ID {
        return Err(format!("unexpected cloth state {}", c.state_id).into());
    }
    let blob = &c.lod0_blob;
    let n = blob.particle_count()?;
    let settings = blob.settings()?;
    let sr = p
        .resources
        .iter()
        .find(|q| q.type_id == "f7a7e4df")
        .ok_or("no SoftBodySettings in the entry")?;
    let decoded = cloth::soft_body_settings(&p.bytes[sr.payload.clone()], &sr.file_id)?;
    // `--live-body` starts from the blob and copies the body per step (no double scaling).
    let ext = (!blob_settings && !live_body).then_some(&decoded);
    let mut base = sim::asset::solver(blob, ext)?;
    let resource_params = sim::asset::collision_params(blob, ext)?;
    if let Some(i) = args.iter().position(|a| a == "--skip") {
        // Diagnostic: drop these opcodes from the program (not a native mode).
        let skip: Vec<u32> = args[i + 1]
            .split(',')
            .map(|x| x.parse())
            .collect::<Result<_, _>>()?;
        base.schedule.retain(|s| !skip.contains(&s.0));
    }
    if let Some(i) = args.iter().position(|a| a == "--drag-scale") {
        // Diagnostic: scale the drag numerator.
        base.drag.1 *= args[i + 1].parse::<f32>()?;
    }
    if let Some(k) = repeat {
        for s in base.schedule.iter_mut().filter(|s| s.0 == 10) {
            s.1 = k;
        }
    }

    let tether_check = args.iter().any(|a| a == "--tether-check");
    if args.iter().any(|a| a == "--dump-maps") {
        // Diagnostic: the collision params the solver uses (thickness factor per particle).
        let p = resource_params.as_ref().ok_or("no collision params")?;
        println!(
            "{}",
            json!({"thickness": p.thickness, "friction": p.friction,
            "thickness_factor": p.thickness_factor})
        );
        return Ok(());
    }
    let events = parse_events(&std::fs::read(&args[1])?)?;
    let pairs = pair_entry_exit(&events)?;
    let steps: Vec<Step> = pairs
        .iter()
        .map(|&(i, o)| decode_step(&events[i], &events[o], n))
        .collect::<Result<_, _>>()?;
    if steps.is_empty() {
        return Err("no captured steps: unavailable, not a parity result".into());
    }
    if tether_check {
        // Game exit state of every step: anchor distance / our tether length, enabled tethers.
        let mut r: Vec<f32> = Vec::new();
        let mut over = 0usize;
        for st in &steps {
            for i in 0..n {
                if st.out_pos[i].w < 0.0 || !base.tether.enabled[i] {
                    continue;
                }
                let a = base.anchors[i] as usize;
                let d = dist(st.out_pos[i], st.out_pos[a]);
                let q = d / base.tether.value[i];
                over += (q > 1.0 + 1e-3) as usize;
                r.push(q);
            }
        }
        r.sort_by(f32::total_cmp);
        let pick = |f: f64| r[((r.len() - 1) as f64 * f) as usize];
        eprintln!(
            "tether: {} samples, over-length {}, ratio min {} p50 {} p90 {} max {}",
            r.len(),
            over,
            r[0],
            pick(0.5),
            pick(0.9),
            r[r.len() - 1]
        );
        let lens: Vec<f32> = (0..n)
            .filter(|&i| base.tether.enabled[i])
            .map(|i| base.tether.value[i])
            .collect();
        eprintln!(
            "tether lengths: enabled {} min {:?} max {:?}",
            lens.len(),
            lens.iter().cloned().fold(f32::MAX, f32::min),
            lens.iter().cloned().fold(0.0, f32::max)
        );
    }
    let b0 = &steps[0].body;
    let live = json!({
        "gravity_164": [f32_at(b0, 164), f32_at(b0, 168), f32_at(b0, 172)],
        "damping_176": f32_at(b0, 176), "thickness_180": f32_at(b0, 180),
        "max_speed_184": f32_at(b0, 184), "s56_188": f32_at(b0, 188), "s60_192": f32_at(b0, 192),
        "friction_328": f32_at(b0, 328), "drag_252_256": [f32_at(b0, 252), f32_at(b0, 256)], "b126": b0[126], "b127": b0[127], "b143": b0[143],
    });
    let eq = |a: bool| json!(a);
    let body_vs_decoded = json!({
        "gravity": eq([f32_at(b0, 164), f32_at(b0, 168), f32_at(b0, 172)] == decoded.gravity()),
        "damping": eq(f32_at(b0, 176) == decoded.damping()),
        "thickness": eq(f32_at(b0, 180) == decoded.thickness()),
        "tether_scale": eq(f32_at(b0, 192) == decoded.tether_scale()),
        "friction": eq(f32_at(b0, 328) == decoded.friction()),
        "drag": eq((f32_at(b0, 252), f32_at(b0, 256)) == decoded.drag()),
    });
    let resource = json!({
        "gravity": settings.gravity, "damping": settings.damping, "thickness": settings.thickness,
        "max_speed": settings.max_speed, "blend": settings.blend, "s60": settings.s60,
        "friction": settings.collision.friction, "drag": [settings.drag.a, settings.drag.b],
    });

    let (mut free, mut pinned, mut prev_e, mut norm_e, mut moved) = (
        Err3::default(),
        Err3::default(),
        Err3::default(),
        Err3::default(),
        Err3::default(),
    );
    if args.iter().any(|a| a == "--free-run") {
        println!(
            "{}",
            serde_json::to_string_pretty(&free_run(
                &base,
                &resource_params,
                &steps,
                n,
                no_collision
            ))?
        );
        return Ok(());
    }
    let mut per_block = Vec::new();
    let mut block = Err3::default();
    let mut block_moved = Err3::default();
    let mut worst = (0.0f32, 0usize, 0usize);
    for (k, s) in steps.iter().enumerate() {
        let mut cl = base.clone();
        cl.state.pos = s.pos.clone();
        cl.state.prev = s.prev.clone();
        cl.state.target = s.target.clone();
        cl.normals = s.normals.clone();
        cl.drag.0 = s.wind.extend(0.0);
        // Damping S+44 (body +176) is a per-frame input: the component's reposition boost
        // (`0x141d8b9a0`) rewrites it; zero in these captures outside a boost.
        cl.integration.damping = f32_at(&s.body, 176);
        if let Some(i) = args.iter().position(|a| a == "--stiffness") {
            // Diagnostic: per-set stiffness from body +152/+156/+160 (`live`) or a list.
            let v: Vec<f32> = if args[i + 1] == "live" {
                (0..3).map(|k| f32_at(&s.body, 152 + 4 * k)).collect()
            } else {
                args[i + 1].split(',').map(|x| x.parse().unwrap()).collect()
            };
            for (d, k) in cl.distance.iter_mut().zip(v) {
                d.stiffness = k;
            }
        }
        if live_body {
            // Live settings block S = body+132 (gravity S+32, damping S+44, max speed S+52,
            // blend S+56) and the drag constants the kernel 0x1423d72b0 reads (+252, +256).
            let b = &s.body;
            cl.integration.gravity = Vec4::new(f32_at(b, 164), f32_at(b, 168), f32_at(b, 172), 0.0);
            cl.integration.damping = f32_at(b, 176);
            cl.integration.max_speed = f32_at(b, 184);
            cl.blend = f32_at(b, 188);
            cl.drag.1 = f32_at(b, 252);
            cl.drag.2 = f32_at(b, 256);
            // S+60: tether lengths scaled (inference from the live anchor distances).
            let s60 = f32_at(b, 192);
            cl.tether.value.iter_mut().for_each(|v| *v *= s60);
        }
        if !no_collision {
            cl.shapes = s.shapes.clone();
            cl.collision = resource_params.clone().map(|mut p| {
                if live_body {
                    p.thickness = f32_at(&s.body, 180);
                    p.friction = f32_at(&s.body, 328);
                }
                p
            });
        }
        if let (Some(i), Some(p)) = (
            args.iter().position(|a| a == "--coll"),
            cl.collision.as_mut(),
        ) {
            // Diagnostic: override collision params, e.g. `friction=0,thickness=0`.
            for kv in args[i + 1].split(',') {
                let (key, v) = kv.split_once('=').unwrap();
                let v: f32 = v.parse().unwrap();
                match key {
                    "friction" => p.friction = v,
                    "thickness" => p.thickness = v,
                    _ => panic!("unknown --coll key {key}"),
                }
            }
        }
        if trace == Some(k) {
            trace_step(&cl, s, n);
        }
        cl.step(s.dt, &s.target_normals);
        if let Some(i) = args.iter().position(|a| a == "--worst") {
            // Diagnostic: the 8 worst free particles at step K, with the game's exit distance to
            // the nearest capsule surface (negative = inside) and our result's.
            if args[i + 1].parse::<usize>().ok() == Some(k) {
                let surf_one = |sh: &acu_sim::cloth::collision::Shape, p: glam::Vec3| {
                    let l = sh.current.local(p);
                    match sh.kind {
                        acu_sim::cloth::collision::ShapeKind::Capsule {
                            radius,
                            half_length,
                        } => {
                            let y = l.y.clamp(-half_length, half_length);
                            (l - glam::Vec3::new(0.0, y, 0.0)).length() - radius
                        }
                        _ => f32::INFINITY,
                    }
                };
                let surf = |p: Vec4| {
                    s.shapes
                        .iter()
                        .map(|sh| {
                            let l = sh.current.local(p.truncate());
                            match sh.kind {
                                acu_sim::cloth::collision::ShapeKind::Capsule {
                                    radius,
                                    half_length,
                                } => {
                                    let y = l.y.clamp(-half_length, half_length);
                                    (l - glam::Vec3::new(0.0, y, 0.0)).length() - radius
                                }
                                _ => f32::INFINITY,
                            }
                        })
                        .fold(f32::INFINITY, f32::min)
                };
                let mut v: Vec<(f32, usize)> = (0..n)
                    .filter(|&i| s.pos[i].w >= 0.0)
                    .map(|i| (dist(cl.state.pos[i], s.out_pos[i]), i))
                    .collect();
                v.sort_by(|a, b| b.0.total_cmp(&a.0));
                for &(d, i) in v.iter().take(8) {
                    let r = cl
                        .collision
                        .as_ref()
                        .and_then(|p| p.thickness_factor.as_ref())
                        .map_or(0.0, |t| t[i])
                        * 0.05;
                    eprintln!(
                        "p{i:3} err {:.2} mm  game_surf {:+.4}  ours_surf {:+.4}  in_surf {:+.4}  r_extra {:.4}  delta {:?}",
                        d * 1000.0,
                        surf(s.out_pos[i]),
                        surf(cl.state.pos[i]),
                        surf(s.pos[i]),
                        r,
                        (cl.state.pos[i] - s.out_pos[i]).truncate() * 1000.0
                    );
                    // Per shape: surface distance of game exit, and the collider's motion at the
                    // game exit point (current - previous frame, mm).
                    for (j, sh) in s.shapes.iter().enumerate() {
                        let g = s.out_pos[i].truncate();
                        let d1 = surf_one(sh, g);
                        if d1 < 0.06 {
                            let mv = g - sh.current.world(sh.previous.local(g));
                            eprintln!(
                                "    shape {j} game_surf {d1:+.4} ours_surf {:+.4} in_surf {:+.4} motion {:?}",
                                surf_one(sh, cl.state.pos[i].truncate()),
                                surf_one(sh, s.pos[i].truncate()),
                                mv * 1000.0
                            );
                        }
                    }
                }
            }
        }
        for i in 0..n {
            let d = dist(cl.state.pos[i], s.out_pos[i]);
            let m = dist(s.pos[i], s.out_pos[i]);
            if s.pos[i].w < 0.0 {
                pinned.add(d);
            } else {
                free.add(d);
                block.add(d);
                moved.add(m);
                block_moved.add(m);
                if d > worst.0 {
                    worst = (d, k, i);
                }
            }
            prev_e.add(dist(cl.state.prev[i], s.out_prev[i]));
            let nd = dist(cl.normals[i], s.out_normals[i]);
            norm_e.add(nd);
            if nd > 0.5 && args.iter().any(|a| a == "--normals") {
                eprintln!(
                    "normal step {k} p{i} ours {:?} game {:?} pinned {}",
                    cl.normals[i],
                    s.out_normals[i],
                    s.pos[i].w < 0.0
                );
            }
        }
        if (k + 1) % 30 == 0 || k + 1 == steps.len() {
            per_block.push(json!({"steps_to": k + 1, "free_max": block.max,
                "free_mean": block.sum / block.n.max(1) as f64, "game_move_max": block_moved.max}));
            block = Err3::default();
            block_moved = Err3::default();
        }
    }
    let dts: std::collections::BTreeSet<u32> = steps.iter().map(|s| s.dt.to_bits()).collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "finding": "ACU-RE-0081",
            "capture": args[1], "steps": steps.len(), "particles": n,
            "dt": dts.iter().map(|b| f32::from_bits(*b)).collect::<Vec<_>>(),
            "shapes": steps[0].shapes.len(),
            "schedule": base.schedule,
            "settings": if live_body { "live body" } else if blob_settings { "blob" } else { "external SoftBodySettings" },
            "live_body_equals_decoded": body_vs_decoded,
            "collision": if no_collision { "off" } else { "on" },
            "live_body": live, "resource_settings": resource,
            "error_m": {"free": free.json(), "pinned": pinned.json(), "prev": prev_e.json(),
                        "normals": norm_e.json()},
            "game_move_m": moved.json(),
            "worst_free": {"error": worst.0, "step": worst.1, "particle": worst.2},
            "per_30_steps": per_block,
        }))?
    );
    Ok(())
}

/// `--trace K`: run step K's program op by op (full repeat count) and print, after each op,
/// how far the free particles are from the game's exit state and how far the op moved them.
fn trace_step(base: &sim::Cloth, s: &Step, n: usize) {
    let mut c = base.clone();
    let gap = |c: &sim::Cloth| {
        let (mut mx, mut sum, mut m) = (0.0f32, 0.0f64, 0usize);
        for i in 0..n {
            if s.pos[i].w >= 0.0 {
                let d = dist(c.state.pos[i], s.out_pos[i]);
                mx = mx.max(d);
                sum += d as f64;
                m += 1;
            }
        }
        (mx, sum / m.max(1) as f64)
    };
    let mut ops: Vec<u32> = Vec::new();
    let mut i = 0;
    while i < c.schedule.len() {
        let (op, cnt) = c.schedule[i];
        if op == 10 {
            let end = c.schedule[i..].iter().position(|x| x.0 == 11).unwrap() + i;
            for _ in 0..cnt {
                ops.extend(c.schedule[i + 1..end].iter().map(|x| x.0));
            }
            i = end + 1;
            continue;
        }
        ops.push(op);
        i += 1;
    }
    eprintln!("trace: start gap {:?}", gap(&c));
    for op in ops {
        let before = c.state.pos.clone();
        let one = sim::Cloth {
            schedule: vec![(op, 1)],
            ..c.clone()
        };
        c = one;
        c.step(s.dt, &s.target_normals);
        let moved = (0..n)
            .map(|i| dist(before[i], c.state.pos[i]))
            .fold(0.0f32, f32::max);
        eprintln!("op {op:2}: moved max {moved:.6}  gap {:?}", gap(&c));
    }
}

/// The rigid carry between two steps: step `k`'s current collider frame becomes step `k+1`'s
/// previous frame (observed: entry positions and previous equal the last exit carried by it).
fn carry_between(
    a: &acu_sim::cloth::collision::Frame,
    b: &acu_sim::cloth::collision::Frame,
) -> (glam::Mat3, glam::Vec3) {
    let ra = glam::Mat3::from_cols(a.x, a.y, a.z).transpose();
    let rb_t = glam::Mat3::from_cols(b.x, b.y, b.z);
    let rc = rb_t * ra;
    (rc, b.t - rc * a.t)
}

/// `--free-run`: start from the first captured entry and step our own state through every
/// captured step (the game's targets, target normals, colliders, dt, wind), carried between
/// steps by the collider-derived transform. Compares with the game's exit positions.
fn free_run(
    base: &sim::Cloth,
    params: &Option<acu_sim::cloth::collision::Params>,
    steps: &[Step],
    n: usize,
    no_collision: bool,
) -> serde_json::Value {
    let mut cl = base.clone();
    let s0 = &steps[0];
    cl.state.pos = s0.pos.clone();
    cl.state.prev = s0.prev.clone();
    cl.normals = s0.normals.clone();
    let mut blocks = Vec::new();
    let mut block = Err3::default();
    let mut all = Err3::default();
    let mut last = 0.0f32;
    for (k, s) in steps.iter().enumerate() {
        if k > 0 {
            let (rc, tc) = carry_between(&steps[k - 1].shapes[0].current, &s.shapes[0].previous);
            let mv = |v: Vec4| (rc * v.truncate() + tc).extend(v.w);
            cl.state.pos.iter_mut().for_each(|p| *p = mv(*p));
            cl.state.prev.iter_mut().for_each(|p| *p = mv(*p));
            cl.normals
                .iter_mut()
                .for_each(|q| *q = (rc * q.truncate()).extend(q.w));
        }
        cl.state.target = s.target.clone();
        cl.drag.0 = s.wind.extend(0.0);
        // Damping S+44 (body +176) is a per-frame input: the component's reposition boost
        // (`0x141d8b9a0`) rewrites it; zero in these captures outside a boost.
        cl.integration.damping = f32_at(&s.body, 176);
        if !no_collision {
            cl.shapes = s.shapes.clone();
            cl.collision = params.clone();
        }
        cl.step(s.dt, &s.target_normals);
        let mut m = 0.0f32;
        for i in 0..n {
            if s.pos[i].w >= 0.0 {
                let d = dist(cl.state.pos[i], s.out_pos[i]);
                block.add(d);
                all.add(d);
                m = m.max(d);
            }
        }
        last = m;
        if (k + 1) % 30 == 0 || k + 1 == steps.len() {
            blocks.push(json!({"steps_to": k + 1, "free_max": block.max,
                "free_mean": block.sum / block.n.max(1) as f64}));
            block = Err3::default();
        }
    }
    json!({"mode": "free-run", "steps": steps.len(), "error_m": all.json(),
           "last_step_max": last, "per_30_steps": blocks})
}
