//! Walk straight, then hold TargetAngle at a fixed value; print the machine path and the
//! transitions of the deepest machine. `walk_turn_probe <game dir> <angle degrees>`.
//! Diagnostic for ACU-RE-0069 (no rig needed).
use acu_assets::{Limits, animation, atom_stream, forge::Forge, package};
use acu_motion::atom_graph::{Body, ClipInfo, Graph, Player, Value};
use std::collections::HashMap;

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().expect("game dir"));
    let angle: f32 = args.next().map_or(90.0, |a| a.parse().unwrap());
    let limits = Limits::default();
    let mut forge = Forge::open(dir.join("DataPC.forge"), limits).unwrap();
    let p = package::decode(&forge.raw(1518).unwrap(), limits).unwrap();
    let r = p
        .resources
        .iter()
        .find(|r| r.file_id == "000000013d5bc4d8")
        .unwrap();
    let graph =
        Graph::from_root(&atom_stream::parse(&p.bytes[r.payload.clone()]).unwrap()).unwrap();
    let nav = package::decode(&forge.raw(928).unwrap(), limits).unwrap();
    let mut infos = HashMap::new();
    let mut names = HashMap::new();
    for r in &nav.resources {
        let Ok(id) = u64::from_str_radix(&r.file_id, 16) else {
            continue;
        };
        if let Ok(c) = animation::parse(&nav.bytes[r.payload.clone()]) {
            infos.insert(id, ClipInfo::from_clip(&c));
            names.insert(id, r.name.clone());
        }
    }
    let hub = graph.state_at(9_974_731).unwrap();
    let param_names: Vec<String> = graph.params.iter().map(|p| p.name.clone()).collect();
    let state_at: Vec<usize> = graph.states.iter().map(|s| s.at).collect();
    let transitions: Vec<Vec<(u16, f32, String)>> = graph
        .states
        .iter()
        .map(|s| {
            s.transitions
                .iter()
                .map(|t| (t.target, t.blend, format!("{:?}", t.expr)))
                .collect()
        })
        .collect();
    let machines: Vec<Option<Vec<usize>>> = graph
        .states
        .iter()
        .map(|s| match &s.body {
            Body::Machine { states, .. } => Some(states.clone()),
            _ => None,
        })
        .collect();
    let mut pl = Player::new(graph, hub, infos).unwrap();
    for (n, v) in [
        ("EdgeDistanceY", 10.0),
        ("WallDistanceY", 10.0),
        ("DenseCrowdDistanceY", 10.0),
        ("TimeScale", 1.0),
    ] {
        pl.set(n, Value::Scalar(v));
    }
    pl.set("UseBipedFootIK", Value::Bool(true));
    pl.start_in(&[1, 5]);
    let mut last = Vec::new();
    for tick in 0..300 {
        let moving = tick >= 10;
        pl.set(
            "MovementSpeed",
            Value::Scalar(if moving { 2.2 } else { 0.0 }),
        );
        pl.set("Parkour", Value::Int(i32::from(moving)));
        pl.set(
            "TargetAngle",
            Value::Scalar(if tick >= 150 { angle } else { 0.0 }),
        );
        pl.step(1.0 / 60.0);
        let path = pl.path();
        if path != last {
            let top = pl
                .leaves
                .iter()
                .max_by(|a, b| a.weight.total_cmp(&b.weight))
                .map(|l| names.get(&l.clip).cloned().unwrap_or_default())
                .unwrap_or_default();
            println!(
                "t{tick} path {:?} top {top}",
                path.iter().map(|&s| state_at[s]).collect::<Vec<_>>()
            );
            last = path;
        }
        if tick == 160 {
            // The deepest machine on the path and its current child's transitions.
            for w in last.windows(2) {
                let (m, c) = (w[0], w[1]);
                if let Some(children) = &machines[m] {
                    let i = children.iter().position(|&x| x == c).unwrap_or(usize::MAX);
                    println!(
                        "machine {} child #{i} (state {}):",
                        state_at[m], state_at[c]
                    );
                    for (target, blend, expr) in &transitions[c] {
                        println!(
                            "   -> #{target} blend {blend} {}",
                            short(expr, &param_names)
                        );
                    }
                }
            }
        }
    }
}

fn short(expr: &str, names: &[String]) -> String {
    let mut s: String = expr.chars().take(600).collect();
    for (i, n) in names.iter().enumerate().rev() {
        let _ = (i, n);
    }
    s.truncate(600);
    s
}
