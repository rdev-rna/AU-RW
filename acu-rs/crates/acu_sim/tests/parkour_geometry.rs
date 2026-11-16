//! Native parkour candidates against the Rust ledge finder and candidate distances
//! (ACU-RE-0055, oracle `data/oracles/parkour_geometry.json`).
//!
//! The oracle rows are the game's own 672-byte candidates gathered live (ACU-RE-0042) in
//! Versailles. Their distances and positions are independent of this crate's code.

use acu_sim::block::{Block, LedgeEdge, LedgeRule, find_ledges_with, native_to_arena};
use acu_sim::candidate::{candidate_scalars, edge_view};
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

fn edge_features(e: &LedgeEdge, s: Vec3) -> (f32, f32, f32) {
    let v = edge_view(e, s);
    (v.height, v.horizontal, v.facing_cos)
}

/// Measurement, not a parity gate: per-type windows from complete selection calls
/// (ACU-RE-0055 negative examples). For each action type it prints the observed window of
/// `edge_view` over the type's live candidates, how many emitted edges of other calls fall in it
/// (precision), a hold-out recall (windows from even-numbered runs, tested on odd-numbered runs)
/// and how the true edges compare with the others by facing.
/// Needs `ACU_PARKOUR_BLOCK` and `ACU_PARKOUR_CALLS`
/// (`python tools/parkour_geometry.py calls --runs <the oracle's 17 runs> --write local/parkour/calls-versailles.json`).
/// Writes the report to `ACU_PARKOUR_WINDOWS_OUT` when set.
#[test]
#[ignore = "needs ACU_PARKOUR_BLOCK and ACU_PARKOUR_CALLS"]
fn per_type_windows_from_calls() {
    let (Some(block_path), Some(calls_path)) =
        (block_path(), std::env::var_os("ACU_PARKOUR_CALLS"))
    else {
        eprintln!("ACU_PARKOUR_BLOCK / ACU_PARKOUR_CALLS unset: unavailable, not a pass");
        return;
    };
    let block = Block::parse(&std::fs::read(&block_path).unwrap()).unwrap();
    let origin = block.origin;
    let to_block = |p: &Value| {
        let a = p.as_array().unwrap();
        let f = |i: usize| a[i].as_f64().unwrap();
        native_to_arena([
            (f(0) - origin[0]) as f32,
            (f(1) - origin[1]) as f32,
            (f(2) - origin[2]) as f32,
        ])
    };
    let calls: Value = serde_json::from_str(&std::fs::read_to_string(calls_path).unwrap()).unwrap();
    assert_eq!(calls["schema"], "acu-parkour-calls/1");
    let edges = find_ledges_with(&block.triangles, LedgeRule::default());
    let grid = EdgeGrid::new(&edges);
    let index_of = |e: &LedgeEdge| {
        (e as *const LedgeEdge as usize - edges.as_ptr() as usize)
            / std::mem::size_of::<LedgeEdge>()
    };
    let cos_10 = 10f32.to_radians().cos();
    let inside = |p: Vec3| {
        p.x >= block.min_bound.x + 12.0
            && p.x <= block.max_bound.x - 12.0
            && p.z >= block.min_bound.z + 12.0
            && p.z <= block.max_bound.z - 12.0
    };
    // (type) -> positives (call, edge, features, candidate c0/c4)
    struct Pos {
        run: i64,
        call: usize,
        edge: usize,
        f: (f32, f32, f32),
        c: (f32, f32),
    }
    let mut pos: BTreeMap<i64, Vec<Pos>> = BTreeMap::new();
    let mut used_calls = Vec::new();
    let mut unmatched: BTreeMap<i64, usize> = BTreeMap::new();
    for (ci, call) in calls["calls"].as_array().unwrap().iter().enumerate() {
        let cands = call["candidates"].as_array().unwrap();
        if !call["complete"].as_bool().unwrap() || cands.is_empty() {
            continue;
        }
        let s = to_block(&cands[0]["source"]);
        if !inside(s) {
            continue;
        }
        let fwd = call["forward"].as_array().map(|f| {
            let v = Vec2::new(
                f[0].as_f64().unwrap() as f32,
                -f[1].as_f64().unwrap() as f32,
            );
            v.normalize_or_zero()
        });
        used_calls.push((ci, s, fwd));
        for c in cands {
            let Some(ty) = c["type"].as_i64() else {
                continue;
            };
            let (a, b) = (to_block(&c["seg_a"]), to_block(&c["seg_b"]));
            if a.distance(b) <= 0.002 {
                continue;
            }
            let dir = (b - a).normalize();
            let target = to_block(&c["target"]);
            let hit = grid
                .near(target, 0.05)
                .into_iter()
                .filter(|(_, e)| (e.b - e.a).normalize().dot(dir).abs() >= cos_10)
                .min_by(|x, y| x.0.total_cmp(&y.0));
            match hit {
                Some((_, e)) => pos.entry(ty).or_default().push(Pos {
                    run: call["run"].as_i64().unwrap(),
                    call: ci,
                    edge: index_of(e),
                    f: edge_features(e, s),
                    c: (
                        c["c0"].as_f64().unwrap() as f32,
                        c["c4"].as_f64().unwrap() as f32,
                    ),
                }),
                None => *unmatched.entry(ty).or_default() += 1,
            }
        }
    }
    eprintln!(
        "complete calls with a source inside the block: {}",
        used_calls.len()
    );
    let matched: usize = pos.values().map(Vec::len).sum();
    let missed: usize = unmatched.values().sum();
    assert!(used_calls.len() > 2000, "complete calls in the block");
    assert!(
        missed * 100 <= matched,
        "edge candidates without an emitted edge: {missed} of {}",
        matched + missed
    );
    // Hold-out: windows from even-numbered runs, recall on odd-numbered runs.
    for (ty, ps) in &pos {
        let (train, test): (Vec<&Pos>, Vec<&Pos>) = ps.iter().partition(|p| p.run % 2 == 0);
        if train.len() < 10 || test.len() < 5 {
            continue;
        }
        let lo = train.iter().fold((f32::MAX, f32::MAX, f32::MAX), |a, p| {
            (a.0.min(p.f.0), a.1.min(p.f.1), a.2.min(p.f.2))
        });
        let hi = train.iter().fold((f32::MIN, f32::MIN, f32::MIN), |a, p| {
            (a.0.max(p.f.0), a.1.max(p.f.1), a.2.max(p.f.2))
        });
        let hit = test
            .iter()
            .filter(|p| {
                p.f.0 >= lo.0
                    && p.f.0 <= hi.0
                    && p.f.1 >= lo.1
                    && p.f.1 <= hi.1
                    && p.f.2 >= lo.2
                    && p.f.2 <= hi.2
            })
            .count();
        eprintln!(
            "holdout type {ty:>3}: train {:>4} test {:>4} recall {:.1} %",
            train.len(),
            test.len(),
            100.0 * hit as f32 / test.len() as f32
        );
    }
    let mut report = serde_json::Map::new();
    for (ty, ps) in &pos {
        if ps.len() < 20 {
            continue;
        }
        let (mut lo, mut hi) = (
            (f32::MAX, f32::MAX, f32::MAX),
            (f32::MIN, f32::MIN, f32::MIN),
        );
        let mut dc0 = Vec::new();
        for p in ps {
            lo = (lo.0.min(p.f.0), lo.1.min(p.f.1), lo.2.min(p.f.2));
            hi = (hi.0.max(p.f.0), hi.1.max(p.f.1), hi.2.max(p.f.2));
            dc0.push(((p.f.0 - p.c.0).abs(), (p.f.1 - p.c.1).abs()));
        }
        let truth: std::collections::HashSet<(usize, usize)> =
            ps.iter().map(|p| (p.call, p.edge)).collect();
        let calls_with: std::collections::HashSet<usize> = ps.iter().map(|p| p.call).collect();
        let (mut in_window, mut true_in, mut calls_flagged, mut calls_false) =
            (0usize, 0usize, 0usize, 0usize);
        let (mut fwd_true, mut fwd_false, mut rank_near, mut rank_fwd, mut ranked) =
            (Vec::new(), Vec::new(), 0usize, 0usize, 0usize);
        for &(ci, s, fwd) in &used_calls {
            let mut flagged = false;
            let mut cands_in: Vec<(f32, f32, bool)> = Vec::new();
            for (_, e) in grid.near(s, hi.1 + 3.0) {
                let f = edge_features(e, s);
                if f.0 >= lo.0
                    && f.0 <= hi.0
                    && f.1 >= lo.1
                    && f.1 <= hi.1
                    && f.2 >= lo.2
                    && f.2 <= hi.2
                {
                    in_window += 1;
                    flagged = true;
                    let is_true = truth.contains(&(ci, index_of(e)));
                    true_in += is_true as usize;
                    if let Some(fw) = fwd {
                        let (cp, _) = e.closest(s);
                        let to = Vec2::new(cp.x - s.x, cp.z - s.z).normalize_or_zero();
                        let c = fw.dot(to);
                        if is_true {
                            fwd_true.push(c)
                        } else {
                            fwd_false.push(c)
                        }
                        cands_in.push((f.1, c, is_true));
                    }
                }
            }
            if calls_with.contains(&ci) && cands_in.iter().any(|x| x.2) {
                ranked += 1;
                let near = cands_in.iter().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
                let best_f = cands_in.iter().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
                rank_near += near.2 as usize;
                rank_fwd += best_f.2 as usize;
            }
            if flagged {
                calls_flagged += 1;
                calls_false += !calls_with.contains(&ci) as usize;
            }
        }
        let med = |v: &mut Vec<f32>| {
            v.sort_by(f32::total_cmp);
            if v.is_empty() {
                f32::NAN
            } else {
                v[v.len() / 2]
            }
        };
        eprintln!(
            "   type {ty}: forward cos to edge, true median {:.2} (n {}) vs other in-window {:.2} (n {}); calls where the nearest in-window edge is a candidate {rank_near}/{ranked}, the most forward one {rank_fwd}/{ranked}",
            med(&mut fwd_true),
            fwd_true.len(),
            med(&mut fwd_false),
            fwd_false.len()
        );
        dc0.sort_by(|a, b| a.0.total_cmp(&b.0));
        let med = dc0[dc0.len() / 2];
        eprintln!(
            "type {ty:>3}: pos {:>4} (unmatched {:>3}) c0 [{:.2},{:.2}] c4 [{:.2},{:.2}] cos [{:.2},{:.2}] | pairs in window {in_window:>6} true {true_in:>4} ({:.1} %) | calls flagged {calls_flagged} without type {calls_false} | closest-pt vs native c0 med {:.2}",
            ps.len(),
            unmatched.get(ty).unwrap_or(&0),
            lo.0,
            hi.0,
            lo.1,
            hi.1,
            lo.2,
            hi.2,
            100.0 * true_in as f32 / in_window.max(1) as f32,
            med.0
        );
        report.insert(
            ty.to_string(),
            serde_json::json!({
                "positives": ps.len(), "unmatched": unmatched.get(ty).unwrap_or(&0),
                "height": [lo.0, hi.0], "horizontal": [lo.1, hi.1], "facing_cos": [lo.2, hi.2],
                "pairs_in_window": in_window, "true_pairs_in_window": true_in,
                "calls_flagged": calls_flagged, "calls_flagged_without_type": calls_false,
            }),
        );
    }
    if let Some(out) = std::env::var_os("ACU_PARKOUR_WINDOWS_OUT") {
        std::fs::write(out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }
}
