//! Print a skeleton's per-bone mirror links: `skeleton_mirror <game dir> <archive> <entry> <name>`.
use acu_assets::{Limits, forge::Forge, package, skeleton};
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&a[1]);
    let mut forge = Forge::open(dir.join(&a[2]), Limits::default()).unwrap();
    let p = package::decode(
        &forge.raw(a[3].parse().unwrap()).unwrap(),
        Limits::default(),
    )
    .unwrap();
    let r = p
        .resources
        .iter()
        .find(|r| r.name == a[4])
        .expect("resource");
    let s = skeleton::parse(&p.bytes[r.payload.clone()], &r.file_id, Limits::default()).unwrap();
    let ids: HashMap<&str, (usize, u32)> = s
        .bones
        .iter()
        .enumerate()
        .map(|(i, b)| (b.file_id.as_str(), (i, b.name_hash)))
        .collect();
    let mut types: HashMap<u32, usize> = HashMap::new();
    let (mut selfs, mut others, mut none) = (0, 0, 0);
    for (i, b) in s.bones.iter().enumerate() {
        *types.entry(b.mirroring_type).or_default() += 1;
        let m = match &b.mirror {
            skeleton::Pointer::Id(id) | skeleton::Pointer::Link(id) => {
                ids.get(id.as_str()).copied()
            }
            _ => None,
        };
        match m {
            Some((j, _)) if j == i => selfs += 1,
            Some(_) => others += 1,
            None => none += 1,
        }
        if i < 40 {
            println!(
                "{i:3} {:08x} type {} mirror {:?}",
                b.name_hash,
                b.mirroring_type,
                m.map(|x| (x.0, format!("{:08x}", x.1)))
            );
        }
    }
    println!(
        "bones {} self {selfs} other {others} none {none} types {types:?}",
        s.bones.len()
    );
}
