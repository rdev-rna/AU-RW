//! Native parkour candidates against the Rust ledge finder and candidate distances
//! (ACU-RE-0055, oracle `data/oracles/parkour_geometry.json`).
//!
//! The oracle rows are the game's own 672-byte candidates gathered live (ACU-RE-0042) in
//! Versailles. Their distances and positions are independent of this crate's code.

use acu_sim::block::{Block, LedgeEdge, LedgeRule, find_ledges_with, native_to_arena};
use acu_sim::candidate::candidate_scalars;
use glam::{Vec2, Vec3};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

fn oracle() -> Value {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../data/oracles/parkour_geometry.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["schema"], "acu-oracle/parkour-geometry/1");
    v
}

struct Rows<'a> {
    cols: HashMap<&'a str, usize>,
    rows: &'a [Value],
}

impl<'a> Rows<'a> {
    fn new(v: &'a Value) -> Self {
        let cols = v["row_columns"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, c)| (c.as_str().unwrap(), i))
            .collect();
        Self {
            cols,
            rows: v["rows"].as_array().unwrap(),
        }
    }
    fn get<'r>(&self, row: &'r Value, col: &str) -> &'r Value {
        &row[self.cols[col]]
    }
    fn vec3(&self, row: &Value, col: &str) -> [f32; 3] {
        let a = self.get(row, col).as_array().unwrap();
        std::array::from_fn(|i| a[i].as_f64().unwrap() as f32)
    }
    fn f32(&self, row: &Value, col: &str) -> f32 {
        self.get(row, col).as_f64().unwrap() as f32
    }
}

/// `+0xC0..+0xC8` from the source reproduce the game's values within 2 mm on every row the
/// oracle marks `scalars_reproduced` (the reference point is the source on those rows).
#[test]
fn candidate_scalars_match_native_rows() {
    let v = oracle();
    let r = Rows::new(&v);
    let (mut checked, mut worst) = (0usize, 0.0f32);
    for row in r.rows {
        if !r.get(row, "scalars_reproduced").as_bool().unwrap() {
            continue;
        }
        let s = candidate_scalars(r.vec3(row, "source"), r.vec3(row, "target"));
        for (got, col) in [(s.height, "c0"), (s.horizontal, "c4"), (s.distance, "c8")] {
            let err = (got - r.f32(row, col)).abs();
            worst = worst.max(err);
            assert!(
                err <= 0.002,
                "{col}: {got} vs {} (row {row})",
                r.f32(row, col)
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 4534, "rows marked scalars_reproduced");
    eprintln!("candidate scalars: {checked} rows, worst error {worst:.5} m");
}

fn block_path() -> Option<PathBuf> {
    std::env::var_os("ACU_PARKOUR_BLOCK").map(PathBuf::from)
}

/// Edges on a 1 m horizontal grid (test lookup only).
struct EdgeGrid<'a> {
    edges: &'a [LedgeEdge],
    cells: HashMap<(i32, i32), Vec<u32>>,
}

impl<'a> EdgeGrid<'a> {
    fn new(edges: &'a [LedgeEdge]) -> Self {
        let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (i, e) in edges.iter().enumerate() {
            let lo = e.a.min(e.b);
            let hi = e.a.max(e.b);
            for x in lo.x.floor() as i32..=hi.x.floor() as i32 {
                for z in lo.z.floor() as i32..=hi.z.floor() as i32 {
                    cells.entry((x, z)).or_default().push(i as u32);
                }
            }
        }
        Self { edges, cells }
    }

    /// Edges within `reach` of `p`, with their distances.
    fn near(&self, p: Vec3, reach: f32) -> Vec<(f32, &'a LedgeEdge)> {
        let mut seen = Vec::new();
        for x in (p.x - reach).floor() as i32..=(p.x + reach).floor() as i32 {
            for z in (p.z - reach).floor() as i32..=(p.z + reach).floor() as i32 {
                if let Some(ids) = self.cells.get(&(x, z)) {
                    seen.extend_from_slice(ids);
                }
            }
        }
        seen.sort_unstable();
        seen.dedup();
        seen.into_iter()
            .map(|i| &self.edges[i as usize])
            .map(|e| (e.closest(p).0.distance(p), e))
            .filter(|(d, _)| *d <= reach)
            .collect()
    }
}

#[derive(Default, Debug)]
struct Tally {
    rows: usize,
    covered: usize,
    outward_agrees: usize,
}

/// ACU-RE-0055 gate for `find_ledges`: every native candidate with a non-degenerate span on a
/// decoded convex ledge has an emitted edge within 5 cm of its target, parallel within 10°, with
/// the same outward side. The default rule also covers the native candidates on convex edges
/// whose side is sloped (|nz| 0.3..0.7, the oracle's `convex_other`).
/// Install-dependent: needs `ACU_PARKOUR_BLOCK`, an ACUBLK01 export of Versailles around the
/// oracle rows, e.g.
/// `python tools/world_collision.py block 46 -233 --half 125 --samples 3 --write local/blocks/versailles-parkour-oracle.bin`.
#[test]
#[ignore = "needs ACU_PARKOUR_BLOCK (a local Versailles collision export)"]
fn ledge_edges_cover_native_candidates() {
    let Some(path) = block_path() else {
        eprintln!("ACU_PARKOUR_BLOCK unset: unavailable, not a pass");
        return;
    };
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let block = Block::parse(&bytes).unwrap();
    let origin = block.origin;
    let to_block = |p: [f32; 3]| {
        native_to_arena([
            (p[0] as f64 - origin[0]) as f32,
            (p[1] as f64 - origin[1]) as f32,
            (p[2] as f64 - origin[2]) as f32,
        ])
    };
    let v = oracle();
    let r = Rows::new(&v);
    let cos_10 = 10f32.to_radians().cos();
    // (name, rule, convex_ledge rows that must be covered, convex_other rows at least covered)
    let rules = [
        (
            "default (top 0.7, side 0.7)",
            LedgeRule::default(),
            4274,
            368,
        ),
        (
            "steep sides only (side 0.3)",
            LedgeRule {
                top_ny: 0.7,
                side_ny: 0.3,
            },
            4274,
            10,
        ),
    ];
    for (name, rule, ledge_rows, other_min) in rules {
        let edges = find_ledges_with(&block.triangles, rule);
        let grid = EdgeGrid::new(&edges);
        let mut by_class: BTreeMap<String, Tally> = BTreeMap::new();
        let mut missed = Vec::new();
        for row in r.rows {
            let (a, b) = (r.vec3(row, "segment_a"), r.vec3(row, "segment_b"));
            let (a, b) = (to_block(a), to_block(b));
            if a.distance(b) <= 0.002 {
                continue; // degenerate span: a point target, not an edge
            }
            let class = r
                .get(row, "edge_class")
                .as_str()
                .unwrap_or("none")
                .to_string();
            let target = to_block(r.vec3(row, "target"));
            let dir = (b - a).normalize();
            let n = r.get(row, "normal_xy").as_array().unwrap();
            // Native (x, y) horizontal to the arena's (x, z) plane: (x, -y).
            let outward = Vec2::new(
                n[0].as_f64().unwrap() as f32,
                -n[1].as_f64().unwrap() as f32,
            );
            let hit = grid
                .near(target, 0.05)
                .into_iter()
                .filter(|(_, e)| (e.b - e.a).normalize().dot(dir).abs() >= cos_10)
                .min_by(|x, y| x.0.total_cmp(&y.0));
            let t = by_class.entry(class.clone()).or_default();
            t.rows += 1;
            if let Some((_, e)) = hit {
                t.covered += 1;
                if e.outward.dot(outward) > 0.0 {
                    t.outward_agrees += 1;
                }
            } else if class == "convex_ledge" {
                missed.push((r.get(row, "action_type").clone(), target));
            }
        }
        eprintln!("{name}: {} edges", edges.len());
        for (class, t) in &by_class {
            eprintln!(
                "  {class:<14} rows {:>5}  covered {:>5}  outward agrees {:>5}",
                t.rows, t.covered, t.outward_agrees
            );
        }
        let ledge = &by_class["convex_ledge"];
        assert_eq!(ledge.rows, 4274, "oracle convex_ledge rows with a span");
        assert_eq!(
            ledge.covered,
            ledge_rows,
            "{name}: first misses {:?}",
            &missed[..missed.len().min(5)]
        );
        assert_eq!(
            ledge.outward_agrees, ledge.covered,
            "{name}: outward normal sign"
        );
        let other = &by_class["convex_other"];
        assert!(
            other.covered >= other_min,
            "{name}: convex_other covered {}",
            other.covered
        );
        assert_eq!(
            other.outward_agrees, other.covered,
            "{name}: outward normal sign"
        );
    }
}
