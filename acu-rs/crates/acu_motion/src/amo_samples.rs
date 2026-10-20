//! Bounded access to serialized AMO samples (0x150 bytes per record).
//! Recovered accessors. Complete banks are supplied by the offline resource
//! chunk reader; these APIs do not load archives or implement full selection.
use crate::amo::{LocalPose, wrap_angle};

pub const SAMPLE_STRIDE: usize = 0x150;

/// RVA 0x1f37984: map a logical frame through P+78's packed u32 table.
/// Low bit and high 15 bits are not interpreted here. The native subtraction
/// uses only bits 1..16, clamps to the last table row, and returns 0 if empty.
pub fn mapped_sample_index(table: &[u32], logical: u32) -> Result<i32, &'static str> {
    if table.is_empty() {
        return Ok(0);
    }
    if table.len() > 0x3fff {
        return Err("mapping exceeds native count mask");
    }
    let i = logical.min(table.len() as u32 - 1);
    Ok(i as i32 - ((table[i as usize] >> 1) & 0xffff) as i32)
}

#[derive(Clone, Copy, Debug)]
pub struct SampleCursor {
    pub index: i32,
    pub fraction: f32,
}

#[derive(Clone, Copy, Debug)]
pub enum QueryLocation {
    /// Native sets query flag bit 4 without overwriting pose data.
    Invalid,
    /// A removed logical frame can select the next valid sample past half time.
    Single(usize),
    /// Resolved adjacent physical samples, with the original time fraction.
    Pair(SampleCursor),
}

/// Unknown fields remain bytes. Preserve all four vector components and avoid
/// inventing bone labels or discarding stored vertical derivatives.
pub struct SampleBank<'a> {
    bytes: &'a [u8],
}

impl<'a> SampleBank<'a> {
    pub fn len(&self) -> usize {
        self.bytes.len() / SAMPLE_STRIDE
    }
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
    /// Stored time at sample +50, used by range search rather than index*dt.
    /// Full native flags word at +140 (prediction checks bits 2 and 4).
    pub fn sample_flags(&self, index: usize) -> Result<u32, &'static str> {
        if index >= self.len() {
            return Err("sample index outside bank");
        }
        let offset = index * SAMPLE_STRIDE + 0x140;
        Ok(u32::from_le_bytes(
            self.bytes[offset..offset + 4].try_into().unwrap(),
        ))
    }
    /// Raw integer fields compared by prediction at sample+60 and sample+6c.
    /// Their float bit patterns may be denormals/NaNs: never cast through f32.
    pub fn prediction_constraint_words(&self, index: usize) -> Result<[u32; 2], &'static str> {
        if index >= self.len() {
            return Err("sample index outside bank");
        }
        let start = index * SAMPLE_STRIDE;
        Ok([0x60, 0x6c].map(|offset| {
            u32::from_le_bytes(
                self.bytes[start + offset..start + offset + 4]
                    .try_into()
                    .unwrap(),
            )
        }))
    }
    /// Raw record word for nearest-record readers (+60 mode word, +64 scalar,
    /// +140 flags). Bits are returned unchanged: callers own interpretation.
    pub fn record_word(&self, index: usize, offset: usize) -> Result<u32, &'static str> {
        let start = index
            .checked_mul(SAMPLE_STRIDE)
            .and_then(|i| i.checked_add(offset))
            .filter(|_| offset + 4 <= SAMPLE_STRIDE)
            .ok_or("sample offset overflow")?;
        Ok(u32::from_le_bytes(
            self.bytes
                .get(start..start + 4)
                .ok_or("AMO sample index out of bounds")?
                .try_into()
                .unwrap(),
        ))
    }
    pub fn sample_time(&self, index: usize) -> Result<f32, &'static str> {
        self.read(index, 0x50)
    }

    pub fn new(bytes: &'a [u8]) -> Result<Self, &'static str> {
        if bytes.len() % SAMPLE_STRIDE != 0 {
            return Err("partial AMO sample record");
        }
        Ok(Self { bytes })
    }

    /// Assemble the primary cost's three-component local pose from complete
    /// stored samples. This is not the indexed feature representation: it keeps
    /// the vertical tracked derivative and the otherwise unmapped scalars.
    /// Native auxiliary flags/Vec4 arithmetic and prediction are not certified.
    pub fn candidate_pose(&self, cursor: SampleCursor) -> Result<LocalPose, &'static str> {
        if cursor.index < 0 {
            return Err("invalid candidate pose");
        }
        self.local_pose(cursor, false)
    }

    /// Candidate cursor producer RVA 0x1f378e8. Unlike query_location, a
    /// removed frame stays negative even past half time, and dt==5 is invalid.
    /// None represents the producer's invalid-count/time branch; a negative
    /// cursor represents an actual packed-table result and must be rejected
    /// for primary scoring. Native invalid branches leave fraction untouched.
    pub fn candidate_cursor(
        &self,
        time: f32,
        dt: f32,
        table: &[u32],
    ) -> Result<Option<SampleCursor>, &'static str> {
        let count = self.bytes.len() / SAMPLE_STRIDE;
        if !time.is_finite() || !dt.is_finite() {
            return Err("nonfinite candidate time");
        }
        if count < 2 || dt <= 0.0001 || dt >= 5.0 || time < 0.0 {
            return Ok(None);
        }
        let logical = (time / dt).trunc();
        if logical >= 2147483648.0 {
            return Err("candidate time conversion overflow");
        }
        let fraction = ((time - logical * dt) / dt).min(1.0);
        if fraction < 0.0 {
            return Err("unsupported negative candidate fraction");
        }
        let index = mapped_sample_index(table, logical as u32)?;
        let index = if index >= count as i32 - 1 {
            count as i32 - 2
        } else {
            index
        };
        Ok(Some(SampleCursor { index, fraction }))
    }

    /// Ordinary query path only. Invalid time/mapping returns None, preserving
    /// the distinction from a valid zero pose. Single copies do not wrap stored
    /// angles; pairs use the bulk query sampler's distinct interpolation rules.
    pub fn query_pose(
        &self,
        time: f32,
        dt: f32,
        table: &[u32],
    ) -> Result<Option<LocalPose>, &'static str> {
        let cursor = match self.query_location(time, dt, table)? {
            QueryLocation::Invalid => return Ok(None),
            QueryLocation::Single(i) => SampleCursor {
                index: i as i32,
                fraction: 0.0,
            },
            QueryLocation::Pair(c) => c,
        };
        self.local_pose(cursor, true).map(Some)
    }

    fn local_pose(&self, cursor: SampleCursor, query: bool) -> Result<LocalPose, &'static str> {
        let vector = |offset| {
            if query {
                self.query_vector(cursor, offset)
            } else {
                self.vector(cursor, offset)
            }
        };
        let scalar = |offset| {
            if query {
                self.query_scalar(cursor, offset)
            } else {
                self.scalar(cursor, offset)
            }
        };
        let root = vector(0)?;
        let yaw = scalar(0x54)?;
        let (sin, cos) = yaw.sin_cos();
        let rotate = |v: [f32; 4], position: bool| {
            let x = v[0] - if position { root[0] } else { 0.0 };
            let y = v[1] - if position { root[1] } else { 0.0 };
            let z = v[2] - if position { root[2] } else { 0.0 };
            [cos * x + sin * y, -sin * x + cos * y, z]
        };
        let pose = LocalPose {
            root_velocity: rotate(vector(0x10)?, false),
            tracked_positions: [rotate(vector(0x80)?, true), rotate(vector(0xb0)?, true)],
            tracked_velocities: [rotate(vector(0x90)?, false), rotate(vector(0xc0)?, false)],
            paired_positions: [rotate(vector(0x20)?, true), rotate(vector(0x30)?, true)],
            heading_delta: Some(wrap_angle(scalar(0x58)? - yaw)),
            angular_scalar: Some(scalar(0x5c)?),
            scalar68: Some(scalar(0x68)?),
        };
        if !pose
            .root_velocity
            .iter()
            .chain(pose.tracked_positions.iter().flatten())
            .chain(pose.tracked_velocities.iter().flatten())
            .chain(pose.paired_positions.iter().flatten())
            .chain(pose.heading_delta.iter())
            .chain(pose.angular_scalar.iter())
            .chain(pose.scalar68.iter())
            .all(|v| v.is_finite())
        {
            return Err("local pose transform overflow");
        }
        Ok(pose)
    }

    /// Index/time branch of full query sampler 0x1f369b4. An invalid native
    /// query is represented explicitly; callers must not turn it into zero pose.
    /// Extreme/nonfinite time conversion is unsupported rather than emulating
    /// undefined conversion behavior. Table must be a decoded P+78 array.
    pub fn query_location(
        &self,
        time: f32,
        dt: f32,
        table: &[u32],
    ) -> Result<QueryLocation, &'static str> {
        let count = self.bytes.len() / SAMPLE_STRIDE;
        if !time.is_finite() || !dt.is_finite() {
            return Err("nonfinite query time");
        }
        if count < 2 || dt <= 0.0001 || dt > 5.0 || time < 0.0 {
            return Ok(QueryLocation::Invalid);
        }
        let logical_float = (time / dt).trunc();
        if logical_float >= 2147483648.0 {
            return Err("query time conversion overflow");
        }
        let logical = logical_float as u32;
        let fraction = ((time - logical_float * dt) / dt).min(1.0);
        if fraction < 0.0 {
            return Err("unsupported negative query fraction");
        }
        let index = mapped_sample_index(table, logical)?;
        if index < 0 {
            if fraction <= 0.5 {
                return Ok(QueryLocation::Invalid);
            }
            let next = mapped_sample_index(table, logical + 1)?;
            if next < 0 {
                return Ok(QueryLocation::Invalid);
            }
            if next as usize >= count {
                return Err("mapped query sample out of bounds");
            }
            return Ok(QueryLocation::Single(next as usize));
        }
        let index = (index as usize).min(count - 2);
        Ok(QueryLocation::Pair(SampleCursor {
            index: index as i32,
            fraction,
        }))
    }

    fn read(&self, index: usize, offset: usize) -> Result<f32, &'static str> {
        let start = index
            .checked_mul(SAMPLE_STRIDE)
            .and_then(|i| i.checked_add(offset))
            .ok_or("sample offset overflow")?;
        let value = f32::from_le_bytes(
            self.bytes
                .get(start..start + 4)
                .ok_or("AMO sample index out of bounds")?
                .try_into()
                .unwrap(),
        );
        if !value.is_finite() {
            return Err("nonfinite AMO sample field");
        }
        Ok(value)
    }

    fn indices(
        cursor: SampleCursor,
        low: f32,
        high: f32,
    ) -> Result<Option<(usize, usize, f32)>, &'static str> {
        // All recovered accessors return zero for an invalid negative handle.
        if cursor.index < 0 {
            return Ok(None);
        }
        let t = cursor.fraction;
        // The native reader trusts its caller. Reject unsupported input rather
        // than extrapolating beyond the cursor producer's valid domain.
        if !t.is_finite() || !(0.0..=1.0).contains(&t) {
            return Err("invalid sample fraction");
        }
        let i = cursor.index as usize;
        Ok(Some(if t < low {
            (i, i, 0.0)
        } else if t > high {
            (i + 1, i + 1, 0.0)
        } else {
            (i, i + 1, t)
        }))
    }

    /// Root +00, derivative +10, paired points +20/+30, auxiliary vector +40,
    /// and tracked points +80/+b0/+e0/+110 with derivatives +90/+c0/+f0/+120.
    /// RVAs 1f36f70, 1f37024, 1f371d4, 1f37250, 1f377a4, 1f370fc, 1f37320.
    pub fn vector(&self, cursor: SampleCursor, offset: usize) -> Result<[f32; 4], &'static str> {
        if ![
            0x00, 0x10, 0x20, 0x30, 0x40, 0x80, 0x90, 0xb0, 0xc0, 0xe0, 0xf0, 0x110, 0x120,
        ]
        .contains(&offset)
        {
            return Err("unrecovered vector offset");
        }
        // Root derivative uses a tighter endpoint snap than other vectors.
        let (low, high) = if offset == 0x10 {
            (0.0001, 0.9999)
        } else {
            (0.001, 0.999)
        };
        let Some((a, b, t)) = Self::indices(cursor, low, high)? else {
            return Ok([0.0; 4]);
        };
        let mut out = [0.0; 4];
        for (j, v) in out.iter_mut().enumerate() {
            let x = self.read(a, offset + j * 4)?;
            *v = if a == b {
                x
            } else {
                x * (1.0 - t) + t * self.read(b, offset + j * 4)?
            };
            if !v.is_finite() {
                return Err("sample interpolation overflow");
            }
        }
        Ok(out)
    }

    /// +54 yaw, +58 and +5c use shortest wrapped interpolation (RVAs
    /// 1f37424, 1f374d4, 1f37558). +68 is linear (1f372cc).
    pub fn scalar(&self, cursor: SampleCursor, offset: usize) -> Result<f32, &'static str> {
        if ![0x54, 0x58, 0x5c, 0x68].contains(&offset) {
            return Err("unrecovered scalar offset");
        }
        let Some((a, b, t)) = Self::indices(cursor, 0.001, 0.999)? else {
            return Ok(0.0);
        };
        let x = self.read(a, offset)?;
        if a == b {
            return Ok(x);
        }
        let delta = self.read(b, offset)? - x;
        let value = if offset == 0x68 {
            x + delta * t
        } else {
            wrap_angle(x + wrap_angle(delta) * t)
        };
        if !value.is_finite() {
            return Err("sample interpolation overflow");
        }
        Ok(value)
    }

    /// Query sampler RVA 0x1f369b4 uses .001/.999 for ALL these vectors,
    /// including root velocity. Cursor is an already-resolved adjacent pair;
    /// time conversion, sparse mapping and query flags are not performed here.
    pub fn query_vector(
        &self,
        cursor: SampleCursor,
        offset: usize,
    ) -> Result<[f32; 4], &'static str> {
        if ![
            0, 0x10, 0x20, 0x30, 0x80, 0x90, 0xb0, 0xc0, 0xe0, 0xf0, 0x110, 0x120,
        ]
        .contains(&offset)
        {
            return Err("unrecovered query vector offset");
        }
        let Some((a, b, t)) = Self::indices(cursor, 0.001, 0.999)? else {
            return Err("query sampler cannot read an invalid pair");
        };
        let mut out = [0.0; 4];
        for (j, v) in out.iter_mut().enumerate() {
            let x = self.read(a, offset + j * 4)?;
            *v = if a == b {
                x
            } else {
                t * self.read(b, offset + j * 4)? + (1.0 - t) * x
            };
            if !v.is_finite() {
                return Err("query interpolation overflow");
            }
        }
        Ok(out)
    }

    /// Unlike the candidate accessor, the full query sampler interpolates +5c
    /// LINEARLY. Only +54 and +58 wrap. This distinction is visible in assembly
    /// at 1f36cd1..1f36cf3: subss/mulss/addss, no angle helper call.
    pub fn query_scalar(&self, cursor: SampleCursor, offset: usize) -> Result<f32, &'static str> {
        if ![0x54, 0x58, 0x5c, 0x68].contains(&offset) {
            return Err("unrecovered query scalar offset");
        }
        let Some((a, b, t)) = Self::indices(cursor, 0.001, 0.999)? else {
            return Err("query sampler cannot read an invalid pair");
        };
        let x = self.read(a, offset)?;
        if a == b {
            return Ok(x);
        }
        let d = self.read(b, offset)? - x;
        let v = if offset == 0x54 || offset == 0x58 {
            wrap_angle(x + wrap_angle(d) * t)
        } else {
            x + d * t
        };
        if !v.is_finite() {
            return Err("query interpolation overflow");
        }
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pair(offset: usize, a: f32, b: f32) -> Vec<u8> {
        let mut bytes = vec![0; 2 * SAMPLE_STRIDE];
        bytes[offset..offset + 4].copy_from_slice(&a.to_le_bytes());
        bytes[SAMPLE_STRIDE + offset..SAMPLE_STRIDE + offset + 4].copy_from_slice(&b.to_le_bytes());
        bytes
    }
    #[test]
    fn candidate_time_mapping_does_not_borrow_query_hole_recovery() {
        let bytes = pair(0, 0.0, 1.0);
        let bank = SampleBank::new(&bytes).unwrap();
        let table = [0, 4, 2];
        let c = bank.candidate_cursor(1.75, 1.0, &table).unwrap().unwrap();
        assert_eq!(c.index, -1);
        assert_eq!(c.fraction, 0.75);
        assert!(bank.candidate_pose(c).is_err());
        assert!(bank.query_pose(1.75, 1.0, &table).unwrap().is_some());
        assert!(bank.candidate_cursor(0.0, 5.0, &table).unwrap().is_none());
        assert!(bank.query_pose(0.0, 5.0, &table).unwrap().is_some());
        let c = bank.candidate_cursor(2.0, 1.0, &table).unwrap().unwrap();
        assert_eq!(c.index, 0);
        assert_eq!(c.fraction, 0.0);
    }
    #[test]
    fn local_pose_uses_root_relative_positions_and_preserves_vertical_velocity() {
        let mut bytes = vec![0; 2 * SAMPLE_STRIDE];
        for (offset, value) in [
            (0, 10.0_f32),
            (4, 20.0),
            (0x54, std::f32::consts::FRAC_PI_2),
            (0x80, 11.0),
            (0x84, 22.0),
            (0xc8, 3.5),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        let bank = SampleBank::new(&bytes).unwrap();
        let p = bank
            .candidate_pose(SampleCursor {
                index: 0,
                fraction: 0.0,
            })
            .unwrap();
        assert!((p.tracked_positions[0][0] - 2.0).abs() < 1e-6);
        assert!((p.tracked_positions[0][1] + 1.0).abs() < 1e-6);
        assert_eq!(p.tracked_velocities[1][2], 3.5);
        assert!(bank.query_pose(-1.0, 1.0, &[]).unwrap().is_none());
    }
    #[test]
    // This is a lower-bound test threshold, not a substitute for the PI constant.
    #[allow(clippy::approx_constant)]
    fn query_and_candidate_sampling_are_not_interchangeable() {
        let bytes = pair(0x5c, 3.13, -3.13);
        let bank = SampleBank::new(&bytes).unwrap();
        let cursor = SampleCursor {
            index: 0,
            fraction: 0.5,
        };
        assert_eq!(bank.query_scalar(cursor, 0x5c).unwrap(), 0.0);
        assert!(bank.scalar(cursor, 0x5c).unwrap().abs() > 3.14);
        let bytes = pair(0x10, 0.0, 100.0);
        let bank = SampleBank::new(&bytes).unwrap();
        let cursor = SampleCursor {
            index: 0,
            fraction: 0.0005,
        };
        assert_eq!(bank.query_vector(cursor, 0x10).unwrap()[0], 0.0);
        assert!(bank.vector(cursor, 0x10).unwrap()[0] > 0.0);
    }
    #[test]
    fn packed_mapping_clamps_and_preserves_negative_results() {
        assert_eq!(mapped_sample_index(&[], 123).unwrap(), 0);
        assert_eq!(mapped_sample_index(&[0, 2, 0xaaaa0007], 99).unwrap(), -1);
        assert_eq!(mapped_sample_index(&[0, 3, 0], 1).unwrap(), 0);
    }
    #[test]
    fn removed_query_frames_require_next_valid_frame_and_strict_half_time() {
        let bytes = pair(0, 0.0, 1.0);
        let bank = SampleBank::new(&bytes).unwrap();
        // Logical frames: valid 0, removed 1, valid physical sample 1 at frame 2.
        let table = [0, 4, 2];
        assert!(matches!(
            bank.query_location(1.5, 1.0, &table).unwrap(),
            QueryLocation::Invalid
        ));
        assert!(matches!(
            bank.query_location(1.6, 1.0, &table).unwrap(),
            QueryLocation::Single(1)
        ));
        let QueryLocation::Pair(c) = bank.query_location(2.0, 1.0, &table).unwrap() else {
            panic!()
        };
        // Native clamps last resolved sample to count-2, retains fraction.
        assert_eq!(c.index, 0);
        assert_eq!(c.fraction, 0.0);
        assert!(matches!(
            bank.query_location(-1.0, 1.0, &table).unwrap(),
            QueryLocation::Invalid
        ));
        assert!(matches!(
            bank.query_location(0.0, 0.0001, &table).unwrap(),
            QueryLocation::Invalid
        ));
    }
    #[test]
    fn derivative_endpoint_threshold_is_distinct_and_equal_endpoint_interpolates() {
        let bytes = pair(0x10, 0.0, 100.0);
        let bank = SampleBank::new(&bytes).unwrap();
        let c = SampleCursor {
            index: 0,
            fraction: 0.0005,
        };
        assert!((bank.vector(c, 0x10).unwrap()[0] - 0.05).abs() < 1e-6);
        assert_eq!(bank.vector(c, 0).unwrap(), [0.0; 4]);
        // At the low threshold the native branch still requires the next row.
        let one = SampleBank::new(&bytes[..SAMPLE_STRIDE]).unwrap();
        assert!(
            one.vector(
                SampleCursor {
                    index: 0,
                    fraction: 0.0001
                },
                0x10
            )
            .is_err()
        );
        assert!(
            one.vector(
                SampleCursor {
                    index: 0,
                    fraction: 0.0
                },
                0x10
            )
            .is_ok()
        );
    }
    #[test]
    fn angle_interpolation_uses_short_arc_while_scalar68_is_linear() {
        let bytes = pair(0x54, 3.13, -3.13);
        let bank = SampleBank::new(&bytes).unwrap();
        let c = SampleCursor {
            index: 0,
            fraction: 0.5,
        };
        assert!((bank.scalar(c, 0x54).unwrap().abs() - std::f32::consts::PI).abs() < 1e-6);
        let bytes = pair(0x68, 3.13, -3.13);
        let bank = SampleBank::new(&bytes).unwrap();
        assert_eq!(bank.scalar(c, 0x68).unwrap(), 0.0);
    }
    #[test]
    fn malformed_data_and_unsupported_offsets_fail_without_reading_adjacent_fields() {
        assert!(SampleBank::new(&[0; 3]).is_err());
        let bank = SampleBank::new(&[]).unwrap();
        assert_eq!(
            bank.scalar(
                SampleCursor {
                    index: -1,
                    fraction: f32::NAN
                },
                0x54
            )
            .unwrap(),
            0.0
        );
        assert!(
            bank.scalar(
                SampleCursor {
                    index: 0,
                    fraction: 0.0
                },
                0x50
            )
            .is_err()
        );
        assert!(
            bank.vector(
                SampleCursor {
                    index: 0,
                    fraction: 1.1
                },
                0x10
            )
            .is_err()
        );
    }
}
