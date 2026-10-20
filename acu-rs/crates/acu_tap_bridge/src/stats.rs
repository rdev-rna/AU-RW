//! Error measures for stage gates, in f64 so f32-level differences stay visible.

use glam::{Quat, Vec3};

/// Rotation angle between two quaternions in degrees, sign-insensitive.
/// For unit quaternions `|a - b| = 2 sin(angle / 4)` after hemisphere
/// alignment, so `angle = 4 asin(|a - b| / 2)`; unlike `2 acos(|dot|)` it stays
/// accurate near zero.
pub fn angle_deg(a: Quat, b: Quat) -> f64 {
    let (a, b) = (a.to_array().map(f64::from), b.to_array().map(f64::from));
    let dot: f64 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
    let s = if dot < 0.0 { -1.0 } else { 1.0 };
    let chord = a
        .iter()
        .zip(&b)
        .map(|(x, y)| (x - s * y).powi(2))
        .sum::<f64>()
        .sqrt();
    let norm = (a.iter().map(|x| x * x).sum::<f64>() * b.iter().map(|x| x * x).sum::<f64>())
        .sqrt()
        .sqrt();
    4.0 * (0.5 * chord / norm).min(1.0).asin().to_degrees()
}

pub fn distance(a: Vec3, b: Vec3) -> f64 {
    let d = [a.x - b.x, a.y - b.y, a.z - b.z].map(f64::from);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

#[derive(Clone, Debug, Default)]
pub struct Distribution {
    values: Vec<f64>,
}

impl Distribution {
    pub fn push(&mut self, v: f64) {
        self.values.push(v);
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// `(mean, p99, max)`; NaN values sort last and therefore surface as max.
    pub fn summary(&self) -> (f64, f64, f64) {
        if self.values.is_empty() {
            return (f64::NAN, f64::NAN, f64::NAN);
        }
        let mut v = self.values.clone();
        v.sort_by(|a, b| a.total_cmp(b));
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let p99 = v[((v.len() as f64 * 0.99).ceil() as usize).clamp(1, v.len()) - 1];
        (mean, p99, v[v.len() - 1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_is_accurate_near_zero_and_sign_insensitive() {
        let a = Quat::from_rotation_y(0.3);
        let b = Quat::from_rotation_y(0.3 + 1e-5);
        // f32 components carry ~3e-8 rad of rounding; acos(|dot|) would be off by ~1e-3 deg here.
        assert!((angle_deg(a, b) - 1e-5f64.to_degrees()).abs() < 1e-5);
        assert!((angle_deg(a, -b) - angle_deg(a, b)).abs() < 1e-12);
        for angle in [1.0f32, 2.5, 3.1] {
            let d = angle_deg(Quat::IDENTITY, Quat::from_rotation_x(angle));
            assert!(
                (d - f64::from(angle).to_degrees()).abs() < 1e-4,
                "{angle}: {d}"
            );
        }
    }

    #[test]
    fn summary_reports_mean_p99_max() {
        let mut d = Distribution::default();
        (1..=100).for_each(|v| d.push(v as f64));
        assert_eq!(d.summary(), (50.5, 99.0, 100.0));
        assert!(Distribution::default().summary().0.is_nan());
    }
}
