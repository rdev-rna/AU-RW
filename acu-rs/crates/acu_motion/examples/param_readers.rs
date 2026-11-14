//! List graph states whose nodes read a parameter: `param_readers <game dir> <param>`.
//! Diagnostic for ACU-RE-0069.
use acu_assets::{Limits, atom_stream, forge::Forge, package};
use acu_motion::atom_graph::{Body, Graph};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().expect("game dir"));
    let param = args.next().expect("param name");
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
    let names: std::collections::HashMap<u64, String> = nav
        .resources
        .iter()
        .filter_map(|r| Some((u64::from_str_radix(&r.file_id, 16).ok()?, r.name.clone())))
        .collect();
    let Some(pi) = graph.params.iter().position(|x| x.name == param) else {
        panic!("no parameter {param}");
    };
    for (si, s) in graph.states.iter().enumerate() {
        let Body::Graph { nodes, .. } = &s.body else {
            continue;
        };
        let readers: Vec<&str> = nodes
            .iter()
            .filter(|n| n.inputs.iter().any(|p| p.kind == 0 && p.q as usize == pi))
            .map(|n| n.class)
            .collect();
        if readers.is_empty() {
            continue;
        }
        let clips: Vec<&str> = nodes
            .iter()
            .filter_map(|n| n.clip)
            .filter_map(|c| names.get(&c).map(|s| s.as_str()))
            .collect();
        if clips.is_empty() {
            continue;
        }
        println!(
            "state {si} at {} readers {readers:?} clips {}: {:?}",
            s.at,
            clips.len(),
            &clips[..clips.len().min(5)]
        );
    }
}
