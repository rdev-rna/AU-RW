//! AMO non-refining range scan (RVA 0x163526c) and clip exclusion (0x1637728).
//! Complete objective, special searches, refinement and live dispatch remain separate.
use crate::amo_samples::{SampleBank, mapped_sample_index};

/// Recovered numeric fields, deliberately without guessed gameplay names.
#[derive(Clone, Copy, Debug)]
pub struct ExclusionContext {
    pub value880: i32,
    pub settings_dc: bool,
    pub flag_a19_bit2: bool,
}
/// Pure filtering branch after the native optional observer callback.
pub fn clip_excluded(tags58: &[i32], byte4d: bool, ctx: ExclusionContext) -> bool {
    if ctx.value880 != 1 && ctx.settings_dc && !tags58.contains(&ctx.value880) && tags58 != [1] {
        return true;
    }
    !ctx.flag_a19_bit2 && byte4d
}

pub struct SearchClip<'a> {
    pub bank: SampleBank<'a>,
    pub table: &'a [u32],
    pub dt: f32,
    pub removed_count: u32,
    /// Entry +220 after resolving a valid native resource handle (0x162aef8).
    pub eligible: bool,
    pub tags58: &'a [i32],
    pub byte4d: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct SearchRange {
    pub first_clip: usize,
    pub last_clip: usize,
    pub start_time: f32,
    pub end_time: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub clip: usize,
    pub logical: u32,
    pub physical: usize,
    pub time: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct SearchHit {
    pub candidate: Candidate,
    pub cost: f32,
}

/// RVA 0x1f37840. Ordinary finite nonnegative time domain; unsigned final clamp.
/// This is nearest-frame rounding, not truncation used by interpolating cursors.
pub fn logical_at_time(
    time: f32,
    dt: f32,
    count: usize,
    removed: u32,
) -> Result<u32, &'static str> {
    if !time.is_finite()
        || time < 0.0
        || !dt.is_finite()
        || dt <= 0.0
        || count == 0
        || count > 0x3fff
    {
        return Err("invalid range time/count");
    }
    let ratio = (time / dt).trunc();
    if ratio >= i32::MAX as f32 {
        return Err("range time exceeds supported conversion");
    }
    let base = ratio as u32;
    let frac = (time - base as f32 * dt) / dt;
    let rounded = base + u32::from(frac >= 0.5);
    let last = (count as u32)
        .checked_add(removed)
        .and_then(|x| x.checked_sub(1))
        .ok_or("logical count overflow")?;
    Ok(rounded.min(last))
}

/// Ascending clip/logical order and strict-minimum replacement preserve first ties.
/// The caller must supply its objective; no unknown native costs are set to zero.
/// Only param5==0 is ported. Resource lifetime/debug callbacks are caller-owned.
pub fn range_search<F>(
    clips: &[SearchClip<'_>],
    range: SearchRange,
    step: f32,
    ctx: ExclusionContext,
    max_cost: f32,
    mut objective: F,
) -> Result<Option<SearchHit>, &'static str>
where
    F: FnMut(Candidate, f32) -> Result<f32, &'static str>,
{
    if range.first_clip > range.last_clip
        || range.last_clip >= clips.len()
        || !step.is_finite()
        || step < 0.0
        || !max_cost.is_finite()
        || !range.start_time.is_finite()
        || !range.end_time.is_finite()
    {
        return Err("invalid search range");
    }
    let mut best = max_cost;
    let mut hit = None;
    for (index, clip) in clips
        .iter()
        .enumerate()
        .take(range.last_clip + 1)
        .skip(range.first_clip)
    {
        if !clip.eligible || clip.bank.len() < 2 || clip_excluded(clip.tags58, clip.byte4d, ctx) {
            continue;
        }
        let last_time = clip.bank.sample_time(clip.bank.len() - 1)?;
        let start = if index == range.first_clip && range.start_time >= 0.0 {
            range.start_time
        } else {
            0.0
        };
        let end = if index == range.last_clip && range.end_time <= last_time {
            range.end_time
        } else {
            last_time
        };
        let end = end.min(last_time - 0.5);
        if end < 0.0 {
            return Err("negative native scan endpoint outside supported domain");
        }
        let first = logical_at_time(start, clip.dt, clip.bank.len(), clip.removed_count)?;
        let last = logical_at_time(end, clip.dt, clip.bank.len(), clip.removed_count)?;
        let ratio = (step / clip.dt).trunc();
        if !ratio.is_finite() || ratio >= i32::MAX as f32 {
            return Err("scan step exceeds supported conversion");
        }
        let stride = (ratio as u32).max(1);
        let mut logical = first;
        while logical <= last {
            let physical = mapped_sample_index(clip.table, logical)?;
            // Native skips exactly -1; reject other unsafe negative results explicitly.
            if physical != -1 {
                if physical < 0 || physical as usize >= clip.bank.len() {
                    return Err("invalid mapped search sample");
                }
                let candidate = Candidate {
                    clip: index,
                    logical,
                    physical: physical as usize,
                    time: clip.bank.sample_time(physical as usize)?,
                };
                let cost = objective(candidate, best)?;
                if !cost.is_finite() {
                    return Err("nonfinite candidate objective");
                }
                if cost < best {
                    best = cost;
                    hit = Some(SearchHit { candidate, cost });
                }
            }
            match logical.checked_add(stride) {
                Some(next) => logical = next,
                None => break,
            }
        }
    }
    Ok(hit)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rounding_is_nearest_and_half_ties_go_up() {
        assert_eq!(logical_at_time(0.49, 1.0, 4, 2).unwrap(), 0);
        assert_eq!(logical_at_time(0.5, 1.0, 4, 2).unwrap(), 1);
        assert_eq!(logical_at_time(99.0, 1.0, 4, 2).unwrap(), 5);
    }
    #[test]
    fn singleton_one_is_wildcard_only_for_tag_gate() {
        let ctx = ExclusionContext {
            value880: 7,
            settings_dc: true,
            flag_a19_bit2: false,
        };
        assert!(!clip_excluded(&[1], false, ctx));
        assert!(clip_excluded(&[1, 2], false, ctx));
        assert!(clip_excluded(&[1], true, ctx));
        assert!(!clip_excluded(
            &[],
            false,
            ExclusionContext { value880: 1, ..ctx }
        ));
    }
    #[test]
    fn scan_skips_removed_frames_uses_stored_times_and_preserves_first_tie() {
        let mut bytes = vec![0; 4 * 0x150];
        for (i, t) in [0.0_f32, 0.2, 0.7, 1.1].into_iter().enumerate() {
            bytes[i * 0x150 + 0x50..i * 0x150 + 0x54].copy_from_slice(&t.to_le_bytes());
        }
        let table = [2, 2, 2, 2, 2]; // logical0 -> -1; then 0,1,2,3
        let clip = SearchClip {
            bank: SampleBank::new(&bytes).unwrap(),
            table: &table,
            dt: 0.2,
            removed_count: 1,
            eligible: true,
            tags58: &[1],
            byte4d: false,
        };
        let ctx = ExclusionContext {
            value880: 3,
            settings_dc: true,
            flag_a19_bit2: false,
        };
        let mut seen = vec![];
        let hit = range_search(
            &[clip],
            SearchRange {
                first_clip: 0,
                last_clip: 0,
                start_time: 0.0,
                end_time: 10.0,
            },
            0.2,
            ctx,
            10.0,
            |c, bound| {
                seen.push((c.physical, c.time, bound));
                Ok(2.0)
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(seen.iter().map(|v| v.0).collect::<Vec<_>>(), vec![0, 1, 2]);
        assert_eq!(seen[2].1, 0.7); // not logical3 * .2
        assert_eq!(seen[1].2, 2.0);
        assert_eq!(hit.candidate.physical, 0);
    }
}
