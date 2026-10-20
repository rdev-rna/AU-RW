//! Per-call coat subsolver state gate, ACU-RE-0028.
use acu_tap_bridge::stats::{angle_deg, distance};
use acu_tap_bridge::{
    coat::{decode_chain, quat, vec3},
    decode_hierarchy, decode_joints, pair_entry_exit, parse_events,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 && args.len() != 4 {
        return Err("usage: coat_gate dump.bin meta.json [visual.json]".into());
    }
    let raw_events = parse_events(&std::fs::read(&args[1])?)?;
    let mut events: Vec<acu_tap_bridge::TapEvent> = Vec::new();
    for event in raw_events {
        if event.hook != 0 {
            if let Some(entry) = events
                .iter_mut()
                .rev()
                .find(|e| e.phase == 0 && e.tid == event.tid)
            {
                entry.snaps.extend(event.snaps);
            }
        } else {
            events.push(event);
        }
    }
    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let h = meta["hierarchy_hex"].as_str().ok_or("hierarchy")?;
    let bytes = (0..h.len() / 2)
        .map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16))
        .collect::<Result<Vec<_>, _>>()?;
    let parents = decode_hierarchy(&bytes)?;
    let (mut calls, mut solved, mut errors) = (0, 0, 0);
    let (mut max_angle, mut max_pos) = (0.0f64, 0.0f64);
    let mut per_joint = std::collections::BTreeMap::<usize, (f64, f64)>::new();
    let mut visual_frames: Vec<serde_json::Value> = Vec::new();
    let mut center = glam::Vec3::ZERO;
    for (i, o) in pair_entry_exit(&events)? {
        let (e, x) = (&events[i], &events[o]);
        let snap = |ev: &acu_tap_bridge::TapEvent, id| {
            ev.snap(id).map(|s| s.to_vec()).ok_or("missing snapshot")
        };
        let state = snap(e, 1)?;
        let n = usize::from(u16::from_le_bytes([state[18], state[19]]));
        if n > 2 {
            return Err("capture only supports two nodes".into());
        }
        let records = (0..n)
            .map(|k| snap(e, 2 + k as u16))
            .collect::<Result<Vec<_>, _>>()?;
        let refs = records.iter().map(|r| r.as_slice()).collect::<Vec<_>>();
        let gravity_handle = snap(e, 15)?;
        let gravity = if u32::from_le_bytes(gravity_handle[12..16].try_into()?) & 0x8000_0000 != 0
            && u64::from_le_bytes(gravity_handle[..8].try_into()?) != 0
        {
            vec3(&snap(e, 16)?, 0)
        } else {
            glam::Vec3::new(0.0, 0.0, -9.81)
        };
        let mut chain = match decode_chain(&state, &refs, gravity) {
            Ok(c) => c,
            Err(err) => {
                errors += 1;
                if errors < 4 {
                    eprintln!("unsupported {err}");
                }
                continue;
            }
        };
        let dt = f32::from_le_bytes(snap(e, 10)?[..4].try_into()?);
        let header = snap(e, 12)?;
        let model = decode_joints(&snap(e, 13)?, parents.len())?;
        let p = parents[chain.nodes[0].joint].ok_or("root coat joint")?;
        let parent = if u32::from_le_bytes(header[140..144].try_into()?) & 0x4000000 != 0 {
            model[p].rotation
        } else {
            acu_motion::coat::multiply(quat(&header, 16), model[p].rotation)
        };
        let done = chain.done;
        let mut predicted_error = 0.0f64;
        let mut stage_index = 0;
        let chain_first_joint = chain.nodes[0].joint;
        chain.solve_observed(dt, vec3(&snap(e, 11)?, 32), |positions| {
            if let Some((_, Some(native))) =
                e.snaps.iter().filter(|(id, _)| *id == 20).nth(stage_index)
            {
                for (k, p) in positions.iter().enumerate() {
                    predicted_error = predicted_error.max(distance(*p, vec3(native, k * 16)));
                }
                if predicted_error > 1e-6 && chain_first_joint == 316 {
                    eprintln!(
                        "stage {} seq {} error {predicted_error:.6e}",
                        stage_index, e.seq
                    );
                }
            }
            stage_index += 1;
        });
        if predicted_error > 1e-6 {
            eprintln!(
                "prediction seq {} joint {} error {predicted_error:.6e}",
                e.seq, chain.nodes[0].joint
            );
        }
        let _ = parent;
        if !done && args.len() == 4 {
            if chain.nodes[0].joint == 453 || visual_frames.is_empty() {
                let world = |p: glam::Vec3| {
                    if u32::from_le_bytes(header[140..144].try_into().unwrap()) & 0x4000000 != 0 {
                        p
                    } else {
                        quat(&header, 16) * p + vec3(&header, 0)
                    }
                };
                center = world(model[0].position);
                let rig = model
                    .iter()
                    .map(|j| (world(j.position) - center).to_array())
                    .collect::<Vec<_>>();
                visual_frames.push(serde_json::json!({"rig":rig,"nodes":[],"dt":dt}));
            }
            let nodes = visual_frames.last_mut().unwrap()["nodes"]
                .as_array_mut()
                .unwrap();
            let (mut native_start, mut rust_start) = (chain.nodes[0].anchor, chain.nodes[0].anchor);
            for (k, node) in chain.nodes.iter().enumerate() {
                let native = snap(x, 2 + k as u16)?;
                let native_end = vec3(&native, 224);
                nodes.push(serde_json::json!({"joint":node.joint,"native":[(native_start-center).to_array(),(native_end-center).to_array()],"rust":[(rust_start-center).to_array(),(node.current-center).to_array()],"angle":angle_deg(node.output_rotation,quat(&native,288))}));
                native_start = native_end;
                rust_start = node.current;
            }
        }
        calls += 1;
        if !done {
            solved += 1;
        }
        for (k, node) in chain.nodes.iter().enumerate() {
            let native = snap(x, 2 + k as u16)?;
            let r = angle_deg(node.output_rotation, quat(&native, 288));
            let p = distance(node.current, vec3(&native, 224))
                .max(distance(node.previous, vec3(&native, 240)));
            max_angle = max_angle.max(r);
            max_pos = max_pos.max(p);
            let stats = per_joint.entry(node.joint).or_default();
            stats.0 = stats.0.max(r);
            stats.1 = stats.1.max(p);
        }
    }
    for (j, (r, p)) in per_joint {
        println!("joint {j}: rotation {r:.6e} deg position {p:.6e}");
    }
    println!(
        "calls {calls} solved {solved} unsupported {errors}; max rotation {max_angle:.6e} deg position {max_pos:.6e}"
    );
    if args.len() == 4 {
        std::fs::write(
            &args[3],
            serde_json::to_vec(
                &serde_json::json!({"frames":visual_frames,"parents":parents,"max_angle":max_angle,"max_position":max_pos,"solved":solved,"unsupported":errors,"source":args[1]}),
            )?,
        )?;
    }
    if solved == 0 || errors > 0 || max_angle >= 0.01 || max_pos >= 1e-4 {
        return Err("coat parity gate failed".into());
    }
    Ok(())
}
