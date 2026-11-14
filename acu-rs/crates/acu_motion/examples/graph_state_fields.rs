//! Print every node field of the NewDemo graph states that play the named clips:
//! `graph_state_fields <game dir> <clip name substring>...`. Diagnostic for ACU-RE-0069.
use acu_assets::{Limits, atom_stream, forge::Forge, package};
use acu_motion::atom_graph::{Body, Graph};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().expect("game dir"));
    let wanted: Vec<String> = args.collect();
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
    let name = |id: u64| names.get(&id).cloned().unwrap_or(format!("{id:016x}"));
    for (si, s) in graph.states.iter().enumerate() {
        let Body::Graph { nodes, root, .. } = &s.body else {
            continue;
        };
        let clips: Vec<String> = nodes.iter().filter_map(|n| n.clip).map(name).collect();
        if !clips
            .iter()
            .any(|c| wanted.iter().any(|w| c.contains(w.as_str())))
        {
            continue;
        }
        println!("state {si} at {} class {} root {:?}", s.at, s.class, root);
        for (i, n) in nodes.iter().enumerate() {
            let clip = n.clip.map(name).unwrap_or_default();
            let ports: Vec<String> = n
                .inputs
                .iter()
                .map(|p| match p.kind {
                    0 => format!(
                        "param:{}",
                        graph
                            .params
                            .get(p.q as usize)
                            .map_or("?", |x| x.name.as_str())
                    ),
                    3 => format!("f32:{}", f32::from_bits(p.j)),
                    9 | 11 => format!("node-out:{}", p.q),
                    k => format!("k{k}:{}", p.q),
                })
                .collect();
            println!(
                "  node {i} {} {clip} ports {ports:?} fields {:?} ints {:?}",
                n.class,
                n.fields
                    .iter()
                    .map(|(k, v)| format!("{k}={v:#x}"))
                    .collect::<Vec<_>>(),
                n.ints
            );
        }
    }
}
