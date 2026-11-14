//! Print root track start/end of local clip payloads: `clip_root <file>...`.
fn main() {
    for path in std::env::args().skip(1) {
        let c = acu_assets::animation::parse(&std::fs::read(&path).unwrap()).unwrap();
        let t = &c.tracks[0];
        let q = &c.tracks[1];
        println!(
            "{path}: d={} root first {:?} last {:?} rot first {:?} last {:?}",
            c.duration,
            t.values.first(),
            t.values.last(),
            q.values.first(),
            q.values.last()
        );
    }
}
