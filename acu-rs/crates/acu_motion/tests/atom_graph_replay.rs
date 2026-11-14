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

    let mut player = Player::new(graph, hub, clips).unwrap();
    let mut started = false;
    let params = rows(&run.join("params2.jsonl"));
    let live = rows(&run.join("live-state2.jsonl"));
    let mut live_at = 0;
    let (mut agree, mut total) = (0, 0);
    let mut last = (String::new(), String::new());
    let mut prev_qpc = None;
    for p in &params {
        let qpc = p["qpc"].as_u64().unwrap();
        let dt = prev_qpc
            .map(|q| (qpc - q) as f32 / 1e7)
            .unwrap_or(1.0 / 30.0);
        prev_qpc = Some(qpc);
        for (name, v) in p["v"].as_object().unwrap() {
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
        player.step(dt);
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
    for t in player.diag.transitions.iter().take(40) {
        println!("transition {t:?}");
    }
    println!(
        "unsupported {:?} notes {:?}",
        player.diag.unsupported, player.diag.notes
    );
}
