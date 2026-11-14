//! Print the integer tracks (codec 18/19) of local clip payloads: `clip_int_tracks <file>...`.
fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).unwrap();
        let clip = acu_assets::animation::parse(&bytes).unwrap();
        println!(
            "{path}: duration {} tracks {}",
            clip.duration,
            clip.tracks.len()
        );
        for (i, t) in clip.tracks.iter().enumerate() {
            if !matches!(t.codec, 18 | 19) {
                continue;
            }
            let vals: Vec<String> = t
                .encoded_values
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| format!("{:08x}", u32::from_le_bytes(*c)))
                .collect();
            println!(
                "  track {i} hash {:08x} codec {} times {:?} values {:?}",
                t.hash, t.codec, t.times, vals
            );
        }
    }
}
