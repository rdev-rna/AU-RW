//! A block of decoded world collision for the test arena (ACU-RE-0050, ACU-RE-0055).
//!
//! The triangles come from `tools/world_collision.py block` (`ACUBLK01`): MeshShape
//! triangles of ShadowRigidBody and Entity RigidBody placements, in native Z-up metres
//! relative to an origin on the ground. They are converted here to the arena's Y-up frame
//! with the same rotation the renderer uses for native meshes: native `(x, y, z)` ->
//! arena `(x, z, -y)`. That rotation is a presentation choice, not a claim about axes.
//!
//! What is decoded and what is authored:
//! - ground height uses the ACU-RE-0050 rule (highest surface crossed by the vertical line
//!   at or below the probe height plus a step);
//! - ledge edges follow the ACU-RE-0055 port recipe (convex triangle edge, walkable top
//!   `|n.y| >= 0.7`, side `|n.y| <= 0.7`), gated against live candidates in
//!   `tests/parkour_geometry.rs`;
//! - the body shape, wall contact and climb/mantle rules in [`crate::Player::step_in_block`]
//!   are authored for this arena; no native capsule, reach or timing is claimed.

use glam::{Vec2, Vec3};
use std::collections::HashMap;

pub const MAGIC: &[u8; 8] = b"ACUBLK01";
/// Largest block accepted (the exporter's own limit).
pub const MAX_TRIANGLES: usize = 2_000_000;
/// Grid cell, metres (implementation choice).
const CELL: f32 = 1.0;
/// Walkable-top and side thresholds (analysis thresholds). ACU-RE-0055's recipe had a steep
/// side (|nz| <= 0.3); live candidates also sit on convex edges with sloped sides, |nz| 0.3..0.7
/// (368 of 371 such oracle rows are covered at 0.7, 10 at 0.3). So a side is any face that is not
/// walkable.
pub const TOP_NY: f32 = 0.7;
pub const SIDE_NY: f32 = 0.7;
/// Far-vertex distance below the top plane that makes an edge convex (analysis choice,
/// `classify_edge` in `tools/parkour_geometry.py`).
pub const CONVEX_M: f32 = 0.01;

#[derive(Debug, Clone, Copy)]
pub struct Triangle {
    pub v: [Vec3; 3],
    /// Unit face normal; the sign is arbitrary because native winding is not established.
    pub normal: Vec3,
    /// Index into the exporter's instance list (shape and body ids are in its JSON sidecar).
    pub instance: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct LedgeEdge {
    pub a: Vec3,
    pub b: Vec3,
    /// Horizontal unit normal pointing away from the walkable top face.
    pub outward: Vec2,
    pub top_ny: f32,
    pub side_ny: f32,
    pub instance: u32,
}

impl LedgeEdge {
    /// Closest point of the edge to `p`, and its parameter on a..b.
    pub fn closest(&self, p: Vec3) -> (Vec3, f32) {
        let ab = self.b - self.a;
        let t = ((p - self.a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
        (self.a + ab * t, t)
    }
}

#[derive(Debug)]
pub struct Block {
    pub origin: [f64; 3],
    pub triangles: Vec<Triangle>,
    pub ledges: Vec<LedgeEdge>,
    min: Vec2,
    dims: (usize, usize),
    cells: Vec<Vec<u32>>,
    ledge_cells: Vec<Vec<u32>>,
    pub min_bound: Vec3,
    pub max_bound: Vec3,
}

#[derive(Debug, PartialEq)]
pub enum BlockError {
    Magic,
    Truncated,
    Count(usize),
    NonFinite(usize),
}

impl std::fmt::Display for BlockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Magic => write!(f, "not an ACUBLK01 block"),
            Self::Truncated => write!(f, "block length does not match its triangle count"),
            Self::Count(n) => write!(f, "block holds {n} triangles, above {MAX_TRIANGLES}"),
            Self::NonFinite(i) => write!(f, "triangle {i} has a non-finite coordinate"),
        }
    }
}

impl std::error::Error for BlockError {}

/// Native Z-up `(x, y, z)` to the arena's Y-up `(x, z, -y)`.
pub fn native_to_arena(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], p[2], -p[1])
}

fn face_normal(v: &[Vec3; 3]) -> Vec3 {
    (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero()
}

impl Block {
    /// Parse an `ACUBLK01` file: magic, f64[3] origin, u32 n, n x (f32[9], u32 instance).
    pub fn parse(bytes: &[u8]) -> Result<Self, BlockError> {
        if bytes.len() < 36 || &bytes[..8] != MAGIC {
            return Err(BlockError::Magic);
        }
        let f64_at = |at: usize| f64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
        let origin = [f64_at(8), f64_at(16), f64_at(24)];
        let count = u32::from_le_bytes(bytes[32..36].try_into().unwrap()) as usize;
        if count > MAX_TRIANGLES {
            return Err(BlockError::Count(count));
        }
        if bytes.len() != 36 + count * 40 {
            return Err(BlockError::Truncated);
        }
        let mut triangles = Vec::with_capacity(count);
        for i in 0..count {
            let at = 36 + i * 40;
            let f = |k: usize| {
                f32::from_le_bytes(bytes[at + k * 4..at + k * 4 + 4].try_into().unwrap())
            };
            let raw: [f32; 9] = std::array::from_fn(f);
            if raw.iter().any(|x| !x.is_finite()) {
                return Err(BlockError::NonFinite(i));
            }
            let v = [
                native_to_arena([raw[0], raw[1], raw[2]]),
                native_to_arena([raw[3], raw[4], raw[5]]),
                native_to_arena([raw[6], raw[7], raw[8]]),
            ];
            let normal = face_normal(&v);
            if normal == Vec3::ZERO {
                continue; // degenerate: no surface to stand on or push against
            }
            let instance = u32::from_le_bytes(bytes[at + 36..at + 40].try_into().unwrap());
            triangles.push(Triangle {
                v,
                normal,
                instance,
            });
        }
        Ok(Self::from_triangles(origin, triangles))
    }

    pub fn from_triangles(origin: [f64; 3], triangles: Vec<Triangle>) -> Self {
        let mut min_bound = Vec3::splat(f32::INFINITY);
        let mut max_bound = Vec3::splat(f32::NEG_INFINITY);
        for t in &triangles {
            for p in t.v {
                min_bound = min_bound.min(p);
                max_bound = max_bound.max(p);
            }
        }
        if triangles.is_empty() {
            min_bound = Vec3::ZERO;
            max_bound = Vec3::ZERO;
        }
        let min = Vec2::new(min_bound.x, min_bound.z).floor();
        let dims = (
            ((max_bound.x - min.x) / CELL).floor() as usize + 1,
            ((max_bound.z - min.y) / CELL).floor() as usize + 1,
        );
        let mut block = Self {
            origin,
            triangles,
            ledges: Vec::new(),
            min,
            dims,
            cells: vec![Vec::new(); dims.0 * dims.1],
            ledge_cells: vec![Vec::new(); dims.0 * dims.1],
            min_bound,
            max_bound,
        };
        for (i, t) in block.triangles.iter().enumerate() {
            let lo = t.v[0].min(t.v[1]).min(t.v[2]);
            let hi = t.v[0].max(t.v[1]).max(t.v[2]);
            for cell in block.cell_range(lo, hi) {
                block.cells[cell].push(i as u32);
            }
        }
        block.ledges = find_ledges(&block.triangles);
        for (i, e) in block.ledges.iter().enumerate() {
            for cell in block.cell_range(e.a.min(e.b), e.a.max(e.b)) {
                block.ledge_cells[cell].push(i as u32);
            }
        }
        block
    }

    fn cell_range(&self, lo: Vec3, hi: Vec3) -> impl Iterator<Item = usize> + use<> {
        let clamp = |v: f32, origin: f32, n: usize| {
            (((v - origin) / CELL).floor().max(0.0) as usize).min(n.saturating_sub(1))
        };
        let (nx, nz) = self.dims;
        let x0 = clamp(lo.x, self.min.x, nx);
        let x1 = clamp(hi.x, self.min.x, nx);
        let z0 = clamp(lo.z, self.min.y, nz);
        let z1 = clamp(hi.z, self.min.y, nz);
        (z0..=z1).flat_map(move |z| (x0..=x1).map(move |x| z * nx + x))
    }

    /// Triangles whose grid cells overlap the box (unique, unordered).
    fn near(&self, lo: Vec3, hi: Vec3, out: &mut Vec<u32>) {
        out.clear();
        if hi.x < self.min_bound.x
            || lo.x > self.max_bound.x
            || hi.z < self.min_bound.z
            || lo.z > self.max_bound.z
        {
            return;
        }
        for cell in self.cell_range(lo, hi) {
            out.extend_from_slice(&self.cells[cell]);
        }
        out.sort_unstable();
        out.dedup();
    }

    /// Every surface crossing the vertical line through (x, z): (height, |n.y|), top first.
    pub fn column(&self, x: f32, z: f32) -> Vec<(f32, f32)> {
        let mut near = Vec::new();
        let p = Vec3::new(x, 0.0, z);
        self.near(p, p, &mut near);
        let mut hits: Vec<(f32, f32)> = near
            .iter()
            .filter_map(|&i| {
                let t = &self.triangles[i as usize];
                vertical_hit(&t.v, x, z).map(|y| (y, t.normal.y.abs()))
            })
            .collect();
        hits.sort_by(|a, b| b.0.total_cmp(&a.0));
        hits
    }

    /// ACU-RE-0050 support: the highest surface at or below `probe + step`.
    pub fn support(&self, x: f32, z: f32, probe: f32, step: f32) -> Option<f32> {
        self.column(x, z)
            .into_iter()
            .find(|&(y, ny)| y <= probe + step && ny > 1e-3)
            .map(|(y, _)| y)
    }

    /// Whether a sphere touches any triangle; returns the push-out normal (horizontal part).
    pub fn sphere_contact(&self, centre: Vec3, radius: f32) -> Option<Vec3> {
        let mut near = Vec::new();
        self.near(
            centre - Vec3::splat(radius),
            centre + Vec3::splat(radius),
            &mut near,
        );
        let mut best: Option<(f32, Vec3)> = None;
        for &i in &near {
            let t = &self.triangles[i as usize];
            let q = closest_on_triangle(centre, &t.v);
            let d = (centre - q).length_squared();
            if d < radius * radius && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, (centre - q).normalize_or(t.normal)));
            }
        }
        best.map(|(_, n)| n)
    }

    /// Nearest ray hit within `max`: (distance, face normal facing the ray origin).
    pub fn ray(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<(f32, Vec3)> {
        let end = origin + dir * max;
        let mut near = Vec::new();
        self.near(origin.min(end), origin.max(end), &mut near);
        let mut best: Option<(f32, Vec3)> = None;
        for &i in &near {
            let t = &self.triangles[i as usize];
            if let Some(d) = ray_triangle(origin, dir, &t.v)
                && d <= max
                && best.is_none_or(|(bd, _)| d < bd)
            {
                let n = if t.normal.dot(dir) > 0.0 {
                    -t.normal
                } else {
                    t.normal
                };
                best = Some((d, n));
            }
        }
        best
    }

    /// Ledge edges with any part inside the horizontal box around `p`.
    pub fn ledges_near(&self, p: Vec3, reach: f32) -> Vec<&LedgeEdge> {
        let lo = p - Vec3::splat(reach);
        let hi = p + Vec3::splat(reach);
        if hi.x < self.min_bound.x
            || lo.x > self.max_bound.x
            || hi.z < self.min_bound.z
            || lo.z > self.max_bound.z
        {
            return Vec::new();
        }
        let mut ids: Vec<u32> = self
            .cell_range(lo, hi)
            .flat_map(|c| self.ledge_cells[c].iter().copied())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids.into_iter().map(|i| &self.ledges[i as usize]).collect()
    }
}

/// Height of the vertical line through (x, z) on a triangle, if it crosses it.
pub fn vertical_hit(v: &[Vec3; 3], x: f32, z: f32) -> Option<f32> {
    let [a, b, c] = *v;
    let det = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
    if det.abs() < 1e-9 {
        return None;
    }
    let l1 = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / det;
    let l2 = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / det;
    let l3 = 1.0 - l1 - l2;
    let eps = -1e-6;
    (l1 >= eps && l2 >= eps && l3 >= eps).then_some(l1 * a.y + l2 * b.y + l3 * c.y)
}

/// Closest point on a triangle (Ericson, Real-Time Collision Detection 5.1.5).
pub fn closest_on_triangle(p: Vec3, v: &[Vec3; 3]) -> Vec3 {
    let [a, b, c] = *v;
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// Möller–Trumbore, two-sided.
fn ray_triangle(o: Vec3, d: Vec3, v: &[Vec3; 3]) -> Option<f32> {
    let e1 = v[1] - v[0];
    let e2 = v[2] - v[0];
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - v[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let w = d.dot(q) * inv;
    if w < 0.0 || u + w > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t >= 0.0).then_some(t)
}

/// Face-slope thresholds for [`find_ledges_with`] (|normal.y| in the arena frame, = native |nz|).
/// Analysis choices from ACU-RE-0055, not recovered native constants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LedgeRule {
    /// A top face counts as walkable at or above this.
    pub top_ny: f32,
    /// A side face counts at or below this.
    pub side_ny: f32,
}

impl Default for LedgeRule {
    fn default() -> Self {
        Self {
            top_ny: TOP_NY,
            side_ny: SIDE_NY,
        }
    }
}

/// Convex edges between a walkable top and a side that is not walkable (ACU-RE-0055 recipe).
pub fn find_ledges(triangles: &[Triangle]) -> Vec<LedgeEdge> {
    find_ledges_with(triangles, LedgeRule::default())
}

/// [`find_ledges`] with explicit thresholds.
///
/// Triangles are joined across shapes by vertex positions quantised to 1 mm, since the
/// export already composed each placement into world space.
pub fn find_ledges_with(triangles: &[Triangle], rule: LedgeRule) -> Vec<LedgeEdge> {
    let key = |p: Vec3| {
        (
            (p.x * 1000.0).round() as i64,
            (p.y * 1000.0).round() as i64,
            (p.z * 1000.0).round() as i64,
        )
    };
    type Key = (i64, i64, i64);
    let mut edges: HashMap<(Key, Key), Vec<(u32, usize)>> = HashMap::new();
    for (i, t) in triangles.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (key(t.v[k]), key(t.v[(k + 1) % 3]));
            if a == b {
                continue;
            }
            let pair = if a < b { (a, b) } else { (b, a) };
            edges.entry(pair).or_default().push((i as u32, k));
        }
    }
    let mut out = Vec::new();
    let mut keys: Vec<_> = edges.keys().copied().collect();
    keys.sort_unstable(); // deterministic output order
    for pair in keys {
        let users = &edges[&pair];
        for &(ti, k) in users {
            let top = &triangles[ti as usize];
            if top.normal.y.abs() < rule.top_ny {
                continue;
            }
            let (a, b) = (top.v[k], top.v[(k + 1) % 3]);
            let inner = top.v[(k + 2) % 3];
            let edge = Vec2::new(b.x - a.x, b.z - a.z);
            if edge.length_squared() < 1e-6 {
                continue;
            }
            let mut outward = Vec2::new(edge.y, -edge.x).normalize();
            if outward.dot(Vec2::new(inner.x - a.x, inner.z - a.z)) > 0.0 {
                outward = -outward;
            }
            // Best (steepest) side across the edge. Convex: the side's far vertex lies more than
            // CONVEX_M below the top face's plane (the ACU-RE-0055 oracle's classify_edge).
            // A thin lip (a slab edge or stair nosing a few centimetres tall) counts.
            let up = top.normal * top.normal.y.signum();
            let side = users
                .iter()
                .filter(|&&(si, _)| si != ti)
                .filter_map(|&(si, sk)| {
                    let s = &triangles[si as usize];
                    if s.normal.y.abs() > rule.side_ny {
                        return None;
                    }
                    let far = s.v[(sk + 2) % 3];
                    ((far - a).dot(up) < -CONVEX_M).then_some(s.normal.y.abs())
                })
                .min_by(f32::total_cmp);
            if let Some(side_ny) = side {
                out.push(LedgeEdge {
                    a,
                    b,
                    outward,
                    top_ny: top.normal.y.abs(),
                    side_ny,
                    instance: top.instance,
                });
            }
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Synthetic box (not game data): a `w x h x d` block resting on a 20 m floor.
    pub fn box_block(centre: Vec3, half: Vec3) -> Block {
        let mut tris = Vec::new();
        let mut quad = |a: Vec3, b: Vec3, c: Vec3, d: Vec3| {
            for v in [[a, b, c], [a, c, d]] {
                tris.push(Triangle {
                    v,
                    normal: face_normal(&v),
                    instance: 0,
                });
            }
        };
        let f = 10.0;
        quad(
            Vec3::new(-f, 0.0, -f),
            Vec3::new(f, 0.0, -f),
            Vec3::new(f, 0.0, f),
            Vec3::new(-f, 0.0, f),
        );
        let lo = centre - half;
        let hi = centre + half;
        let p = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
        quad(
            p(lo.x, hi.y, lo.z),
            p(hi.x, hi.y, lo.z),
            p(hi.x, hi.y, hi.z),
            p(lo.x, hi.y, hi.z),
        );
        quad(
            p(lo.x, lo.y, lo.z),
            p(hi.x, lo.y, lo.z),
            p(hi.x, hi.y, lo.z),
            p(lo.x, hi.y, lo.z),
        );
        quad(
            p(lo.x, lo.y, hi.z),
            p(hi.x, lo.y, hi.z),
            p(hi.x, hi.y, hi.z),
            p(lo.x, hi.y, hi.z),
        );
        quad(
            p(lo.x, lo.y, lo.z),
            p(lo.x, lo.y, hi.z),
            p(lo.x, hi.y, hi.z),
            p(lo.x, hi.y, lo.z),
        );
        quad(
            p(hi.x, lo.y, lo.z),
            p(hi.x, lo.y, hi.z),
            p(hi.x, hi.y, hi.z),
            p(hi.x, hi.y, lo.z),
        );
        for t in &mut tris {
            t.normal = face_normal(&t.v);
        }
        Block::from_triangles([0.0; 3], tris)
    }

    fn encode(origin: [f64; 3], tris: &[[f32; 9]]) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        for o in origin {
            out.extend_from_slice(&o.to_le_bytes());
        }
        out.extend_from_slice(&(tris.len() as u32).to_le_bytes());
        for t in tris {
            for v in t {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&7u32.to_le_bytes());
        }
        out
    }

    #[test]
    fn parse_converts_native_z_up_and_rejects_bad_input() {
        let bytes = encode(
            [1.0, 2.0, 3.0],
            &[[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.5]],
        );
        let b = Block::parse(&bytes).unwrap();
        assert_eq!(b.origin, [1.0, 2.0, 3.0]);
        // native (0, 1, 0.5) -> arena (0, 0.5, -1)
        assert_eq!(b.triangles[0].v[2], Vec3::new(0.0, 0.5, -1.0));
        assert_eq!(b.triangles[0].instance, 7);
        assert_eq!(Block::parse(b"ACUBLK02").unwrap_err(), BlockError::Magic);
        assert_eq!(
            Block::parse(&bytes[..bytes.len() - 1]).unwrap_err(),
            BlockError::Truncated
        );
        let mut huge = bytes.clone();
        huge[32..36].copy_from_slice(&(MAX_TRIANGLES as u32 + 1).to_le_bytes());
        assert_eq!(
            Block::parse(&huge).unwrap_err(),
            BlockError::Count(MAX_TRIANGLES + 1)
        );
        let nan = encode(
            [0.0; 3],
            &[[f32::NAN, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]],
        );
        assert_eq!(Block::parse(&nan).unwrap_err(), BlockError::NonFinite(0));
    }

    #[test]
    fn support_ray_contact_and_ledges_on_a_box() {
        let b = box_block(Vec3::new(0.0, 1.0, -3.0), Vec3::new(1.0, 1.0, 1.0));
        // Floor away from the box; box top above it; the 0.5 m step rule picks the floor
        // when the probe is on the ground.
        assert_eq!(b.support(5.0, 5.0, 0.0, 0.5), Some(0.0));
        assert_eq!(b.support(0.0, -3.0, 0.0, 0.5), Some(0.0));
        assert_eq!(b.support(0.0, -3.0, 2.0, 0.5), Some(2.0));
        assert_eq!(b.column(0.0, -3.0).len(), 3);
        let (d, n) = b.ray(Vec3::new(0.0, 1.0, 0.0), -Vec3::Z, 5.0).unwrap();
        assert!((d - 2.0).abs() < 1e-5 && n.z > 0.99);
        assert!(b.sphere_contact(Vec3::new(0.0, 1.0, -1.8), 0.28).is_some());
        assert!(b.sphere_contact(Vec3::new(0.0, 1.0, -1.6), 0.28).is_none());
        // Four top edges are convex ledges; the floor/wall seams are concave and excluded.
        assert_eq!(b.ledges.len(), 4, "{:?}", b.ledges);
        for e in &b.ledges {
            assert!((e.a.y - 2.0).abs() < 1e-6 && (e.b.y - 2.0).abs() < 1e-6);
            let mid = (e.a + e.b) / 2.0;
            let out = Vec2::new(mid.x, mid.z + 3.0).normalize();
            assert!(e.outward.dot(out) > 0.99, "outward must leave the top face");
        }
        let front = b.ledges_near(Vec3::new(0.0, 1.0, -1.5), 0.6);
        assert!(front.iter().any(|e| e.outward.y > 0.99));
    }
}
