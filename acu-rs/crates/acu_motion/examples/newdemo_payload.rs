//! Check that the NewDemo AtomGraph decodes from the install: `newdemo_payload <game dir>`.
use acu_assets::{Limits, atom_stream, forge::Forge, package, sha256};

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let limits = Limits {
        max_decoded: 256 << 20,
        max_stored: 256 << 20,
        ..Limits::default()
    };
    let mut forge = Forge::open(dir.join("DataPC.forge"), limits).unwrap();
    let p = package::decode(&forge.raw(1518).unwrap(), limits).unwrap();
    for r in &p.resources {
        if r.file_id == "000000013d5bc4d8" {
            let b = &p.bytes[r.payload.clone()];
            println!("{} {} bytes sha256 {}", r.name, b.len(), sha256(b));
            let root = atom_stream::parse(b).unwrap();
            println!("parsed root class {:?}", root.class);
        }
    }
}
