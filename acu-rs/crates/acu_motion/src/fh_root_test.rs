//! For Honor-only root-frame helper retained for the imported matcher test.
//! Source: fh_combat/src/motion.rs; whole-source SHA256 25d886806168be775b3205b8614b43876d51959a9d885ebe7550a3cd05e78e5e.
pub fn to_world(right: i32, fwd: i32, facing_mdeg: i32) -> (i32, i32) {
    let f = (facing_mdeg as f64 / 1000.0).to_radians();
    let (s, c) = f.sin_cos();
    let (r, w) = (right as f64, fwd as f64);
    (
        (-r * c + w * s).round() as i32,
        (r * s + w * c).round() as i32,
    )
}
