//! Native animation sync markers (ACU-RE-0069). Every pose carries a sync record: a cyclic
//! or bounded list of markers with the time to the next marker. A sync position names a marker
//! segment and the fraction through it, so clips of different lengths stay foot-aligned.
//!
//! Ports of `ACU.exe` (SHA-256 `748abdc6…7e40`, image base 0x140000000):
//! - record build from a clip's sync track: `sub_14002C9B0`;
//! - normalized phase of a position: `sub_14011D2D0`;
//! - position of a normalized phase: `sub_1401265B0`;
//! - weighted merge of two records: `sub_1400FD7A0`;
//! - masked position match used when a state takes over a position: `sub_140120520`;
//! - step to the next marker segment: `sub_1400AFE60`;
//! - select-best-matching-tags cost: `sub_1400CDE10`;
//! - time scaling of a record: `sub_14011EE30`.
//!
//! Marker values: bits 27-31 marker id, bits 2-26 occurrence counter (group id in the source
//! track), bit 1 mirrorable, bit 0 set only on the bounded start/end markers.

const EPS: f32 = 1e-5;
/// Start of a bounded (non-looping) record, id 30 with bit 0.
pub const START: u32 = 0xF000_0001;
/// End of a bounded record, id 29 with bit 0.
pub const END: u32 = 0xE800_0001;
const ID_MASK: u32 = 0xF800_0003;
const COUNTER_BITS: u32 = 0x07FF_FFFC;
/// "Not found" in the native routines (FLT_MAX).
pub const NOT_FOUND: f32 = f32::MAX;

#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    /// (marker value, seconds to the next marker).
    pub markers: Vec<(u32, f32)>,
    /// Total seconds (`+140`).
    pub total: f32,
    /// Sync group (`+144`); -2 when there is no sync.
    pub id: i32,
    /// `+148`.
    pub looping: bool,
}

/// A position: fraction through the segment that starts at marker `cur` and ends at `next`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pos {
    pub frac: f32,
    pub cur: u32,
    pub next: u32,
}

fn masked(v: u32) -> u32 {
    if v & 1 == 0 { v & ID_MASK } else { v }
}

/// `(v ^ (4c)) & 0x7FFFFFC ^ v`: replace the counter bits.
fn with_counter(v: u32, c: u32) -> u32 {
    (v & !COUNTER_BITS) | ((c << 2) & COUNTER_BITS)
}

impl Record {
    /// A record without sync of the given length.
    pub fn none(total: f32) -> Self {
        Record {
            markers: Vec::new(),
            total,
            id: -2,
            looping: false,
        }
    }

    /// The native validity test `(id + 2) > 1` (unsigned): not -2 and not -1.
    pub fn valid(&self) -> bool {
        (self.id.wrapping_add(2) as u32) > 1
    }

    /// `sub_14002C9B0`. `keys` are the clip's sync-track keys (clip seconds, value) in track
    /// order, `[start, end]` the playback window, `group` the requested sync group. Returns the
    /// record and the clip time of its first marker (the rotation offset of a looping record).
    pub fn build(
        keys: &[(f32, u32)],
        start: f32,
        end: f32,
        group: i32,
        mirrored: bool,
        looping: bool,
    ) -> (Record, f32) {
        let span = end - start;
        let mut r = Record {
            markers: Vec::new(),
            total: span,
            id: group,
            looping,
        };
        let mut list: Vec<(u32, f32)> = Vec::new();
        let (mut min_id, mut first) = (u32::MAX, 0usize);
        for &(t, v) in keys {
            let mut t = t;
            if t > end {
                if (t - end).abs() > EPS {
                    break;
                }
                t = end;
            }
            if t < start || v == 31 || ((v >> 2) & 0x1FF_FFFF) as i32 != group {
                continue;
            }
            let mut v = v;
            if mirrored && v & 2 != 0 {
                v = (v & 0x07FF_FFFF) | ((!(v >> 27)) << 27);
            }
            // A looping record drops an end key that repeats the first marker.
            let duplicate_end = looping
                && (t - end).abs() <= EPS
                && list.first().is_some_and(|f| (v ^ f.0) & 0xF800_0000 == 0);
            if duplicate_end {
                continue;
            }
            let m = v & 0xF800_0001;
            list.push((m, t - start));
            if m >> 27 < min_id {
                min_id = m >> 27;
                first = list.len() - 1;
            }
        }
        if list.is_empty() {
            r.id = -2;
            return (r, 0.0);
        }
        let mut counts = [0u32; 32];
        let mut offset = 0.0;
        if looping {
            let n = list.len();
            let (mut prev, mut first_t) = (NOT_FOUND, 0.0);
            let mut last_t = 0.0;
            for i in 0..n {
                let (m, t) = list[(i + first) % n];
                let c = counts[(m >> 27) as usize];
                counts[(m >> 27) as usize] += 1;
                if prev == NOT_FOUND {
                    offset = t;
                    first_t = t;
                } else {
                    let d = if prev >= t { span - prev + t } else { t - prev };
                    r.markers.last_mut().unwrap().1 = d;
                }
                prev = t;
                last_t = t;
                r.markers.push((with_counter(m, c), 0.0));
            }
            r.markers.last_mut().unwrap().1 = if last_t >= first_t {
                span - last_t + first_t
            } else {
                first_t - last_t
            };
        } else {
            r.markers.push((START, 0.0));
            for &(m, t) in &list {
                let c = counts[(m >> 27) as usize];
                counts[(m >> 27) as usize] += 1;
                let prev = r.markers.last_mut().unwrap();
                prev.1 = t - prev.1;
                r.markers.push((with_counter(m, c), t));
            }
            let last = r.markers.last_mut().unwrap();
            last.1 = span - last.1;
            r.markers.push((END, 0.0));
        }
        (r, offset)
    }

    /// `sub_14011EE30`: scale every segment and the total.
    pub fn scale(&mut self, k: f32) {
        for m in &mut self.markers {
            m.1 *= k;
        }
        self.total *= k;
    }

    /// `sub_14011D2D0`: normalized phase of `p` in this record, [0, 1], or `NOT_FOUND`.
    /// `mask` compares marker ids without occurrence counters.
    pub fn phase(&self, p: &Pos, mask: bool) -> f32 {
        if !self.valid() {
            return p.frac;
        }
        let (mut cur, mut next) = (p.cur, p.next);
        let n = self.markers.len();
        if mask {
            cur = masked(cur);
            next = masked(next);
        } else if self.looping {
            // Counters may exceed this record's occurrences (a position from a longer merged
            // record): retry with the counters advanced by whole cycles.
            let mut occ = [0u32; 32];
            let mut k = 0u32;
            loop {
                let mut acc = 0.0f32;
                let mut found = None;
                for (i, &(m, d)) in self.markers.iter().enumerate() {
                    let id = (m >> 27) as usize;
                    let v = if k > 0 {
                        m ^ ((m ^ m.wrapping_add(4 * k * occ[id])) & COUNTER_BITS)
                    } else {
                        occ[id] += 1;
                        m
                    };
                    if v == cur {
                        found = Some((i, v));
                        break;
                    }
                    acc += d;
                }
                if let Some((i, v)) = found {
                    let id = (v >> 27) as usize;
                    let (nv, nn) = if i + 1 >= n {
                        let f = self.markers[0].0;
                        if f == next {
                            return ((self.markers[i].1 * p.frac + acc) / self.total).min(1.0);
                        }
                        (f, f.wrapping_add(4 * (k + 1) * occ[id]))
                    } else {
                        let f = self.markers[i + 1].0;
                        (f, f.wrapping_add(4 * k * occ[id]))
                    };
                    if nv ^ ((nv ^ nn) & COUNTER_BITS) != next {
                        return NOT_FOUND;
                    }
                    return ((self.markers[i].1 * p.frac + acc) / self.total).min(1.0);
                }
                let cur_seen = occ[(cur >> 27) as usize] != 0 && occ[(next >> 27) as usize] != 0;
                if !(k > 0 || cur_seen) {
                    return NOT_FOUND;
                }
                k += 1;
                if k >= 200 {
                    return NOT_FOUND;
                }
            }
        }
        let mut acc = 0.0f32;
        for (i, &(m, d)) in self.markers.iter().enumerate() {
            let v = if mask { masked(m) } else { m };
            if v == cur {
                let nv = if i + 1 >= n {
                    if !self.looping {
                        return NOT_FOUND;
                    }
                    self.markers[0].0
                } else {
                    self.markers[i + 1].0
                };
                let nv = if mask { masked(nv) } else { nv };
                if nv == next {
                    return ((d * p.frac + acc) / self.total).min(1.0);
                }
                if !mask {
                    return NOT_FOUND;
                }
            }
            acc += d;
        }
        NOT_FOUND
    }

    /// `sub_1401265B0`: position at normalized phase `x`.
    pub fn position(&self, x: f32) -> Pos {
        if !self.valid() {
            return Pos {
                frac: x,
                cur: 0,
                next: 0,
            };
        }
        let n = self.markers.len();
        if (x - 1.0).abs() <= EPS {
            let (cur, next) = if self.looping {
                (self.markers[n - 1].0, self.markers[0].0)
            } else {
                (self.markers[n - 2].0, self.markers[n - 1].0)
            };
            return Pos {
                frac: 1.0,
                cur,
                next,
            };
        }
        let t = x * self.total;
        let mut acc = 0.0f32;
        for (i, &(m, d)) in self.markers.iter().enumerate() {
            if acc + d > t {
                let next = self.markers[if i + 1 == n { 0 } else { i + 1 }].0;
                return Pos {
                    frac: (t - acc) / ((acc + d) - acc),
                    cur: m,
                    next,
                };
            }
            acc += d;
        }
        Pos {
            frac: NOT_FOUND,
            cur: 0,
            next: 0,
        }
    }

    /// `sub_1400FD7A0` (record part): `a` weighted by `1 - w`, `b` by `w`. The result keeps
    /// `a`'s group and looping flag; mismatching sequences lose sync (id -2).
    pub fn merge(a: &Record, b: &Record, w: f32) -> Record {
        let v = 1.0 - w;
        let mut out = Record {
            markers: Vec::new(),
            total: v * a.total + w * b.total,
            id: a.id,
            looping: a.looping,
        };
        if !a.valid() {
            return out;
        }
        if a.looping != b.looping || a.id != b.id {
            out.id = -2;
            return out;
        }
        let (na, nb) = (a.markers.len(), b.markers.len());
        if na == nb || !a.looping {
            if na != nb {
                out.id = -2;
                return out;
            }
            for (x, y) in a.markers.iter().zip(&b.markers) {
                if x.0 != y.0 {
                    out.id = -2;
                    out.markers.clear();
                    return out;
                }
                out.markers.push((x.0, v * x.1 + y.1 * w));
            }
            return out;
        }
        // Looping records of different lengths: walk both cyclically, renumbering counters,
        // until both wrap at once.
        let mut occ = [0u32; 32];
        let (mut i, mut j) = (0usize, 0usize);
        let mut total = 0.0f32;
        loop {
            let x = a.markers[i];
            let y = b.markers[j];
            let rx = with_counter(x.0, occ[(x.0 >> 27) as usize]);
            let ry = with_counter(y.0, occ[(y.0 >> 27) as usize]);
            if rx != ry {
                out.id = -2;
                out.markers.clear();
                return out;
            }
            let d = v * x.1 + y.1 * w;
            total += d;
            out.markers.push((rx, d));
            occ[(rx >> 27) as usize] += 1;
            i += 1;
            let wrap_a = i == na;
            if wrap_a {
                i = 0;
            }
            j += 1;
            let wrap_b = j == nb;
            if wrap_b {
                j = 0;
            }
            if wrap_a && wrap_b {
                out.total = total;
                return out;
            }
            if out.markers.len() > 4096 {
                out.id = -2;
                out.markers.clear();
                return out;
            }
        }
    }

    /// `sub_140120520` (phase part): phase of `p` matched by marker ids only.
    pub fn match_phase(&self, p: &Pos) -> f32 {
        if !self.valid() {
            return p.frac;
        }
        self.phase(p, true)
    }

    /// `sub_1400AFE60`: the start of the segment after `p`'s (exact marker values).
    pub fn next_segment(&self, p: &Pos) -> Option<Pos> {
        let n = self.markers.len();
        let i = self.markers.iter().position(|m| m.0 == p.cur)?;
        let j = self.markers.iter().position(|m| m.0 == p.next)?;
        let i = if i == n - 1 { 0 } else { i + 1 };
        let j = if j == n - 1 { 0 } else { j + 1 };
        Some(Pos {
            frac: 0.0,
            cur: self.markers[i].0,
            next: self.markers[j].0,
        })
    }

    /// `sub_1400CDE10`: cost of starting this candidate record at position `p`; lower wins.
    pub fn select_cost(&self, p: &Pos, threshold: f32) -> f32 {
        if !self.valid() {
            return self.phase(p, true);
        }
        let (cur, next) = (masked(p.cur), masked(p.next));
        let n = self.markers.len();
        let mut matched = false;
        if let Some(k) = self.markers.iter().position(|m| masked(m.0) == cur) {
            let nv = if k + 1 >= n {
                self.looping.then(|| self.markers[0].0)
            } else {
                Some(self.markers[k + 1].0)
            };
            matched = nv.is_some_and(|nv| (next ^ nv) & 0xF800_0000 == 0);
        }
        let mut cost = if matched {
            self.phase(p, true)
        } else {
            NOT_FOUND
        };
        if cost > threshold {
            cost += 10.0;
            let mut alt = 10.0f32;
            for &(m, d) in &self.markers {
                if masked(m) == next {
                    break;
                }
                alt += d;
            }
            if cost > alt {
                return alt;
            }
        }
        cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// xx_Nav_Walk_3-Loop sync track (DataPC entry 928): ids 0, 1, 31, 30 every 7 frames.
    fn walk_keys() -> Vec<(f32, u32)> {
        let v = [0x0000_0002u32, 0x0800_0002, 0xf800_0002, 0xf000_0002];
        (0..12).map(|k| (k as f32 * 7.0 / 30.0, v[k % 4])).collect()
    }

    #[test]
    fn looping_record_counts_occurrences_and_wraps() {
        let (r, offset) = Record::build(&walk_keys(), 0.0, 2.8, 0, false, true);
        assert_eq!(offset, 0.0);
        assert_eq!(r.markers.len(), 12);
        assert_eq!(r.markers[0].0, 0x0000_0000);
        assert_eq!(r.markers[4].0, 0x0000_0004); // id 0, second occurrence
        assert_eq!(r.markers[3].0, 0xf000_0000);
        let total: f32 = r.markers.iter().map(|m| m.1).sum();
        assert!((total - 2.8).abs() < 1e-4);
    }

    #[test]
    fn mirroring_flips_marker_ids() {
        let (r, offset) = Record::build(&walk_keys(), 0.0, 2.8, 0, true, true);
        // 0 <-> 31 and 1 <-> 30 turn the time order 0, 1, 31, 30 into 31, 30, 0, 1; rotating to
        // the smallest id gives the same sequence half a step later.
        let ids: Vec<u32> = r.markers.iter().take(4).map(|m| m.0 >> 27).collect();
        assert_eq!(ids, vec![0, 1, 31, 30]);
        assert!((offset - 14.0 / 30.0).abs() < 1e-5);
    }

    #[test]
    fn bounded_record_has_start_and_end_markers() {
        // xx_Nav_WalkStop_RC: ids 0 at 0 s and 1 at 0.2 s, 3.267 s long.
        let keys = [(0.0, 0x0000_0002), (0.2, 0x0800_0002)];
        let (r, _) = Record::build(&keys, 0.0, 3.2667, 0, false, false);
        let ids: Vec<u32> = r.markers.iter().map(|m| m.0).collect();
        assert_eq!(ids, vec![START, 0, 0x0800_0000, END]);
        assert!((r.markers[1].1 - 0.2).abs() < 1e-5);
        assert!((r.markers[2].1 - 3.0667).abs() < 1e-4);
    }

    #[test]
    fn phase_and_position_invert() {
        let (r, _) = Record::build(&walk_keys(), 0.0, 2.8, 0, false, true);
        for x in [0.0, 0.1, 0.37, 0.5, 0.93] {
            let p = r.position(x);
            assert!((r.phase(&p, false) - x).abs() < 1e-5, "{x}");
        }
    }

    #[test]
    fn merge_lerps_matching_segments() {
        let (a, _) = Record::build(&walk_keys(), 0.0, 2.8, 0, false, true);
        let mut b = a.clone();
        b.scale(2.0);
        let m = Record::merge(&a, &b, 0.25);
        assert!((m.total - 3.5).abs() < 1e-4);
        assert!((m.markers[0].1 - a.markers[0].1 * 1.25).abs() < 1e-5);
    }

    #[test]
    fn select_cost_prefers_the_matching_sequence() {
        let rc = [(0.0, 0x0000_0002), (0.2, 0x0800_0002)];
        let (plain, _) = Record::build(&rc, 0.0, 3.2667, 0, false, false);
        let (mirror, _) = Record::build(&rc, 0.0, 3.2667, 0, true, false);
        let p = Pos {
            frac: 0.0,
            cur: 0x0000_0004,
            next: 0x0800_0004,
        };
        assert!(plain.select_cost(&p, NOT_FOUND) < mirror.select_cost(&p, NOT_FOUND));
    }
}
