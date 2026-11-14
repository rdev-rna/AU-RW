//! Replay recorded native graph parameters (ACU-RE-0067 run) through the interpreter and compare
//! the dominant clip with the clip the game played. Local data only:
//!
//! ACU_NEWDEMO=local/atomgraph/NewDemo.bin ACU_GAME_DIR=<install>
//! ACU_LIVE_RUN=local/native-probe/runs/loco-params-1
//! cargo test -p acu_motion --test atom_graph_replay -- --ignored --nocapture
use acu_assets::{Limits, animation, atom_stream, forge::Forge, package};
use acu_motion::atom_graph::{ClipInfo, Graph, Player, Value};
use std::collections::HashMap;
use std::path::PathBuf;

const HUB_AT: usize = 9_974_731;

fn env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn rows(path: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|r| r.get("qpc").is_some())
        .collect()
}

#[test]
#[ignore = "requires local NewDemo, the install and a live run directory"]
fn replay_live_parameters() {
    let (Some(demo), Some(game), Some(run)) =
        (env("ACU_NEWDEMO"), env("ACU_GAME_DIR"), env("ACU_LIVE_RUN"))
    else {
        eprintln!("UNAVAILABLE: set ACU_NEWDEMO, ACU_GAME_DIR and ACU_LIVE_RUN");
        return;
    };
    let root = atom_stream::parse(&std::fs::read(demo).unwrap()).unwrap();
    let graph = Graph::from_root(&root).unwrap();
    let hub = graph.state_at(HUB_AT).expect("navigation hub");

    let limits = Limits::default();
    let mut forge = Forge::open(game.join("DataPC.forge"), limits).unwrap();
    let pkg = package::decode(&forge.raw(928).unwrap(), limits).unwrap();
    let mut clips = HashMap::new();
    let mut names = HashMap::new();
    let mut failed = 0;
    for r in &pkg.resources {
        let Ok(id) = u64::from_str_radix(&r.file_id, 16) else {
            continue;
        };
        if let Ok(c) = animation::parse(&pkg.bytes[r.payload.clone()]) {
            clips.insert(id, ClipInfo::from_clip(&c));
            names.insert(id, r.name.clone());
        } else if r.name.starts_with("xx_") {
            failed += 1;
        }
    }
    println!(
        "clips decoded: {} (named xx_ failures: {failed})",
        clips.len()
    );
    for probe in [
        0x0040_00e9u32,
        0x0040_0034,
        0x0040_000b,
        0x0040_0005,
        0x0040_00cb,
    ] {
        let with: Vec<&String> = clips
            .iter()
            .filter(|(_, c)| c.events.iter().any(|e| e.0 == probe))
            .filter_map(|(id, _)| names.get(id))
            .collect();
        println!(
            "event {probe:08x}: {} clips, e.g. {:?}",
            with.len(),
            &with[..with.len().min(4)]
        );
    }

    if let Ok(want) = std::env::var("ACU_CLIP_EVENTS") {
        for (id, n) in &names {
            if want.split(',').any(|w| w == n) {
                let c = &clips[id];
                let ev: Vec<String> = c
                    .events
                    .iter()
                    .map(|e| format!("{:08x}[{:.3},{:.3}]", e.0, e.1, e.2))
                    .collect();
                println!("clip {n} {id:016x} d={:.3} {}", c.duration, ev.join(" "));
                println!(
                    "  sync {:?}",
                    c.sync_keys
                        .iter()
                        .map(|k| format!("{:.4}:{:08x}", k.0, k.1))
                        .collect::<Vec<_>>()
                );
            }
        }
    }
    let durations: HashMap<u64, f32> = clips.iter().map(|(k, c)| (*k, c.duration)).collect();
    let mut player = Player::new(graph, hub, clips).unwrap();
    let mut started = false;
    // Parkour reads 11 between frames; the per-frame value shows only in some samples.
    let mut parkour = 0i64;
    let params = rows(&run.join("params2.jsonl"));
    let live = rows(&run.join("live-state2.jsonl"));
    let mut live_at = 0;
    let (mut agree, mut total) = (0, 0);
    let mut phase_err: Vec<f64> = Vec::new();
    let mut per_clip: HashMap<u64, Vec<f64>> = HashMap::new();
    let mut last = (String::new(), String::new());
    let mut prev_qpc = None;
    for p in &params {
        let qpc = p["qpc"].as_u64().unwrap();
        let dt = prev_qpc
            .map(|q| (qpc - q) as f32 / 1e7)
            .unwrap_or(1.0 / 30.0);
        prev_qpc = Some(qpc);
        for (name, v) in p["v"].as_object().unwrap() {
            if name == "Parkour" {
                // All 58 real samples of loco-params-1 satisfy: 0 when MovementSpeed is 0,
                // 1 when it is above 0, 21 only beside walls. Between real samples the slot
                // reads 11, so derive 0/1 from MovementSpeed and keep a real 21 while moving.
                let moving = p["v"]["MovementSpeed"].as_f64().unwrap_or(0.0) > 0.0;
                match v.as_i64() {
                    Some(11) | None => {
                        parkour = match (moving, parkour) {
                            (false, _) => 0,
                            (true, 21) => 21,
                            (true, _) => 1,
                        }
                    }
                    Some(x) => parkour = x,
                }
                player.set(name, Value::Int(parkour as i32));
                continue;
            }
            let value = match v {
                serde_json::Value::Bool(b) => Value::Bool(*b),
                serde_json::Value::Number(n) if n.is_i64() || n.is_u64() => {
                    match player
                        .graph
                        .param(name)
                        .map(|i| player.graph.params[i].ty.as_str())
                    {
                        Some("scalar") => Value::Scalar(n.as_f64().unwrap() as f32),
                        _ => Value::Int(n.as_i64().unwrap() as i32),
                    }
                }
                serde_json::Value::Number(n) => Value::Scalar(n.as_f64().unwrap() as f32),
                _ => continue,
            };
            player.set(name, value);
        }
        if !started {
            // The capture starts idle: hub walk machine (1), idle state (5).
            player.start_in(&[1, 5]);
            started = true;
        }
        let seen = player.diag.transitions.len();
        player.step(dt);
        let tsec = qpc as f64 / 1e7;
        let trace = std::env::var("ACU_TRACE")
            .ok()
            .and_then(|v| {
                let (a, b) = v.split_once(',')?;
                Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?))
            })
            .is_some_and(|(a, b)| tsec >= a && tsec <= b);
        if trace {
            for t in &player.diag.transitions[seen..] {
                println!("    transition {t:?}");
            }
            let at: Vec<usize> = player
                .path()
                .iter()
                .map(|&s| player.graph.states[s].at)
                .collect();
            let leaves: Vec<String> = player
                .leaves
                .iter()
                .filter(|l| l.weight > 0.01)
                .map(|l| {
                    format!(
                        "{}:{:.2}@{:.2}",
                        names.get(&l.clip).map(String::as_str).unwrap_or("?"),
                        l.weight,
                        l.time
                    )
                })
                .collect();
            let keys: Vec<String> = std::env::var("ACU_TRACE_KEYS")
                .unwrap_or("MovementSpeed,Parkour,TargetAngle,HighProfile".into())
                .split(',')
                .map(|k| format!("{k}={}", p["v"].get(k).unwrap_or(&serde_json::Value::Null)))
                .collect();
            println!("  {tsec:.3} path {at:?} {leaves:?} {}", keys.join(" "));
            if std::env::var_os("ACU_TRACE_EVENTS").is_some() {
                let ev: Vec<String> = player
                    .events()
                    .iter()
                    .map(|e| format!("{}:{:08x}", player.graph.states[e.0].at, e.1))
                    .collect();
                println!("    events {}", ev.join(" "));
            }
        }
        if total < 3 {
            let at: Vec<usize> = player
                .path()
                .iter()
                .map(|&s| player.graph.states[s].at)
                .collect();
            println!("path {at:?}");
        }
        while live_at + 1 < live.len() && live[live_at + 1]["qpc"].as_u64().unwrap() <= qpc {
            live_at += 1;
        }
        let dominant = |it: &mut dyn Iterator<Item = (u64, f32)>| {
            it.fold((0u64, -1.0f32), |a, b| if b.1 > a.1 { b } else { a })
                .0
        };
        let ours = dominant(&mut player.leaves.iter().map(|l| (l.clip, l.weight)));
        let theirs = dominant(
            &mut live[live_at]["clips"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| {
                    (
                        u64::from_str_radix(c["file_id"].as_str().unwrap_or("0"), 16).unwrap_or(0),
                        c["weight"].as_f64().unwrap_or(0.0) as f32,
                    )
                }),
        );
        let name = |id: u64| names.get(&id).cloned().unwrap_or(format!("{id:016x}"));
        total += 1;
        agree += usize::from(ours == theirs);
        if ours == theirs && ours != 0 {
            // Phase error of the dominant clip, in clip seconds (wrapped).
            let our_t = player
                .leaves
                .iter()
                .filter(|l| l.clip == ours)
                .max_by(|a, b| a.weight.total_cmp(&b.weight))
                .map(|l| l.time);
            let their_t = live[live_at]["clips"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|c| c["file_id"].as_str() == Some(&format!("{ours:016x}")))
                .max_by(|a, b| {
                    a["weight"]
                        .as_f64()
                        .unwrap_or(0.0)
                        .total_cmp(&b["weight"].as_f64().unwrap_or(0.0))
                })
                .and_then(|c| c["time"].as_f64());
            if let (Some(a), Some(b), Some(&d)) = (our_t, their_t, durations.get(&ours)) {
                let d = d as f64;
                let e = ((a as f64 - b).rem_euclid(d)).min((b - a as f64).rem_euclid(d));
                phase_err.push(e);
                per_clip.entry(ours).or_default().push(e);
            }
        }
        let now = (name(ours), name(theirs));
        if now != last {
            println!(
                "t={:7.3}  ours {:45} native {}",
                qpc as f64 / 1e7,
                now.0,
                now.1
            );
            last = now;
        }
    }
    println!("dominant-clip agreement {agree}/{total}");
    let mut rows: Vec<(u64, Vec<f64>)> = per_clip.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1.len()));
    for (id, mut e) in rows.into_iter().take(12) {
        e.sort_by(f64::total_cmp);
        println!(
            "  time error {:45} n={:4} median {:.3} max {:.3}",
            names.get(&id).cloned().unwrap_or_default(),
            e.len(),
            e[e.len() / 2],
            e[e.len() - 1]
        );
    }
    phase_err.sort_by(f64::total_cmp);
    if !phase_err.is_empty() {
        let q = |f: f64| phase_err[((phase_err.len() - 1) as f64 * f) as usize];
        println!(
            "dominant-clip time error (s): n={} median {:.3} p90 {:.3} max {:.3}",
            phase_err.len(),
            q(0.5),
            q(0.9),
            q(1.0)
        );
    }
    for t in player.diag.transitions.iter().take(40) {
        println!("transition {t:?}");
    }
    println!(
        "unsupported {:?} notes {:?}",
        player.diag.unsupported, player.diag.notes
    );
}
