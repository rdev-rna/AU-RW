//! Root motion of one clip over one step (ACU-RE-0058).
//!
//! Native evidence (`ACU.exe` 748abdc6…7e40): the playback leaf (`0x1400B30D0` →
//! `0x140018F90`) samples the clip's root tracks, track 0 translation and track 1 rotation
//! (ACU-RE-0018), at the previous clip time A and the new time B (playback offset + time):
//! rotation = Q(A)⁻¹·Q(B), translation = R(Q(A))⁻¹·(T(B) − T(A)), i.e. the motion in the root
//! frame at A. The playback's mirror bit negates x (`0x14001ABA0`) and flips the rotation's y/z
//! (`0x140019CA0`, mask `0x142E77EC0`). A loop wrap joins A..end and 0..B (`0x1400F8DE0`,
//! composition `0x1400125C0`).
//!
//! Implementation choices: keys are interpolated with [`crate::unity::sample`] (linear for
//! translation, slerp for rotation). That reproduces the live native deltas to about 0.02 mm for
//! the clips in `data/oracles/root_delta.json`; it is not a decoded copy of the native codec
//! sampler. The wrap composition uses scale 1; the native scale lane's source is not traced and
//! no wrap frame was captured live. In a clip-to-clip transition the native delta came from the
//! incoming clip alone (one transition type observed); choosing that clip is the caller's job.
use crate::unity::sample;
use acu_assets::animation::Clip;
use glam::{Quat, Vec3};

/// Root translation and rotation over one step, in the root frame at the step's start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RootDelta {
    pub translation: Vec3,
    pub rotation: Quat,
}

impl RootDelta {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
    };

    /// `self` followed by `next` (whose translation is in the frame at the end of `self`).
    pub fn then(self, next: Self) -> Self {
        Self {
            translation: self.translation + self.rotation * next.translation,
            rotation: (self.rotation * next.rotation).normalize(),
        }
    }

    /// Reflection through the root's YZ plane: x negated, rotation (x, −y, −z, w).
    pub fn mirrored(self) -> Self {
        let q = self.rotation;
        Self {
            translation: Vec3::new(-self.translation.x, self.translation.y, self.translation.z),
            rotation: Quat::from_xyzw(q.x, -q.y, -q.z, q.w),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RootDeltaError {
    /// Track 0 must be a translation (codec 9) and track 1 a rotation (codec ≤ 8).
    MissingRootTracks,
    /// A time outside 0..=duration (tolerance 1e-4 s) or not finite.
    TimeOutOfRange { time: f32, duration: f32 },
}

impl std::fmt::Display for RootDeltaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRootTracks => write!(f, "clip has no root translation/rotation tracks"),
            Self::TimeOutOfRange { time, duration } => {
                write!(f, "clip time {time} outside 0..={duration}")
            }
        }
    }
}

impl std::error::Error for RootDeltaError {}

const TIME_TOLERANCE: f32 = 1e-4;

fn check(clip: &Clip, times: &[f32]) -> Result<(), RootDeltaError> {
    let [translation, rotation, ..] = clip.tracks.as_slice() else {
        return Err(RootDeltaError::MissingRootTracks);
    };
    if translation.codec != 9
        || rotation.codec > 8
        || translation.times.is_empty()
        || rotation.times.is_empty()
    {
        return Err(RootDeltaError::MissingRootTracks);
    }
    for &time in times {
        if !time.is_finite() || time < -TIME_TOLERANCE || time > clip.duration + TIME_TOLERANCE {
            return Err(RootDeltaError::TimeOutOfRange {
                time,
                duration: clip.duration,
            });
        }
    }
    Ok(())
}

/// Root position and orientation at one clip time.
pub fn root_pose(clip: &Clip, time: f32) -> Result<(Vec3, Quat), RootDeltaError> {
    check(clip, &[time])?;
    let t = sample(&clip.tracks[0], time);
    let q = Quat::from_array(sample(&clip.tracks[1], time)).normalize();
    Ok((Vec3::new(t[0], t[1], t[2]), q))
}

/// Root motion from `a` to `b` without wrapping (`b` may be below `a`: motion played backwards).
pub fn segment(clip: &Clip, a: f32, b: f32) -> Result<RootDelta, RootDeltaError> {
    let (ta, qa) = root_pose(clip, a)?;
    let (tb, qb) = root_pose(clip, b)?;
    let inverse = qa.conjugate();
    Ok(RootDelta {
        translation: inverse * (tb - ta),
        rotation: (inverse * qb).normalize(),
    })
}

/// Root motion of a looping or one-shot clip stepping forward from time `a` to `b`
/// (playback offset + time, seconds). `b < a` means the clip wrapped past its end.
pub fn clip_root_delta(
    clip: &Clip,
    a: f32,
    b: f32,
    mirror: bool,
) -> Result<RootDelta, RootDeltaError> {
    check(clip, &[a, b])?;
    let delta = if b >= a {
        segment(clip, a, b)?
    } else {
        segment(clip, a, clip.duration)?.then(segment(clip, 0.0, b)?)
    };
    Ok(if mirror { delta.mirrored() } else { delta })
}

#[cfg(test)]
mod tests {
    use super::*;
    use acu_assets::animation::Track;
    use std::f32::consts::FRAC_PI_2;

    fn track(codec: u8, keys: &[(f32, [f32; 4])]) -> Track {
        Track {
            hash: 0,
            codec,
            offset: 0,
            times: keys.iter().map(|k| k.0).collect(),
            values: keys.iter().map(|k| k.1).collect(),
            encoded_values: Vec::new(),
        }
    }

    /// Root moving `distance` along `direction` over `duration`, rotating linearly from `q0` to `q1`.
    fn clip(duration: f32, direction: Vec3, distance: f32, q0: Quat, q1: Quat) -> Clip {
        let end = direction.normalize() * distance;
        Clip {
            duration,
            key: 0,
            mapping_offset: 0,
            tracks: vec![
                track(
                    9,
                    &[(0.0, [0.0; 4]), (duration, [end.x, end.y, end.z, 0.0])],
                ),
                track(5, &[(0.0, q0.to_array()), (duration, q1.to_array())]),
            ],
        }
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-5
    }

    #[test]
    fn straight_walk_gives_track_difference() {
        let c = clip(2.8, Vec3::Y, 6.9, Quat::IDENTITY, Quat::IDENTITY);
        let d = clip_root_delta(&c, 1.0, 1.0 + 1.0 / 60.0, false).unwrap();
        assert!(close(d.translation, Vec3::new(0.0, 6.9 / 2.8 / 60.0, 0.0)));
        assert!(d.rotation.angle_between(Quat::IDENTITY) < 1e-6);
    }

    #[test]
    fn translation_is_expressed_in_the_start_frame() {
        // Root faces +90° about z for the whole clip and moves 1 m along world +x.
        let q = Quat::from_rotation_z(FRAC_PI_2);
        let c = clip(1.0, Vec3::X, 1.0, q, q);
        let d = clip_root_delta(&c, 0.0, 1.0, false).unwrap();
        // R(+90° z)⁻¹ maps +x to −y.
        assert!(close(d.translation, Vec3::new(0.0, -1.0, 0.0)), "{d:?}");
    }

    #[test]
    fn rotation_delta_is_relative_to_the_start() {
        let c = clip(
            1.0,
            Vec3::Y,
            0.0,
            Quat::IDENTITY,
            Quat::from_rotation_z(FRAC_PI_2),
        );
        let d = clip_root_delta(&c, 0.5, 1.0, false).unwrap();
        let expected = Quat::from_rotation_z(FRAC_PI_2 / 2.0);
        assert!(d.rotation.angle_between(expected) < 1e-5, "{d:?}");
    }

    #[test]
    fn wrap_joins_end_and_start_segments() {
        let c = clip(2.8, Vec3::Y, 6.9, Quat::IDENTITY, Quat::IDENTITY);
        let d = clip_root_delta(&c, 2.7, 0.1, false).unwrap();
        assert!(close(d.translation, Vec3::new(0.0, 6.9 * 0.2 / 2.8, 0.0)));
    }

    #[test]
    fn wrap_rotates_the_second_segment_by_the_first() {
        // Turn 90° about z over the clip while moving 1 m along local +y at the start frame.
        let c = clip(
            1.0,
            Vec3::Y,
            1.0,
            Quat::IDENTITY,
            Quat::from_rotation_z(FRAC_PI_2),
        );
        let d = clip_root_delta(&c, 0.5, 0.5, false).unwrap();
        assert!(close(d.translation, Vec3::ZERO));
        let w = clip_root_delta(&c, 0.75, 0.25, false).unwrap();
        let first = segment(&c, 0.75, 1.0).unwrap();
        let second = segment(&c, 0.0, 0.25).unwrap();
        assert!(close(
            w.translation,
            first.translation + first.rotation * second.translation
        ));
        assert!(w.rotation.angle_between(first.rotation * second.rotation) < 1e-5);
    }

    #[test]
    fn mirror_negates_x_and_turn_direction() {
        let c = clip(
            1.0,
            Vec3::new(1.0, 1.0, 0.0),
            1.0,
            Quat::IDENTITY,
            Quat::from_rotation_z(0.2),
        );
        let plain = clip_root_delta(&c, 0.0, 1.0, false).unwrap();
        let m = clip_root_delta(&c, 0.0, 1.0, true).unwrap();
        assert!(close(
            m.translation,
            Vec3::new(
                -plain.translation.x,
                plain.translation.y,
                plain.translation.z
            )
        ));
        assert!(m.rotation.angle_between(Quat::from_rotation_z(-0.2)) < 1e-5);
    }

    #[test]
    fn idle_clip_has_identity_delta() {
        let c = clip(19.2, Vec3::Y, 0.0, Quat::IDENTITY, Quat::IDENTITY);
        assert_eq!(
            clip_root_delta(&c, 1.0, 1.1, false).unwrap(),
            RootDelta::IDENTITY
        );
    }

    #[test]
    fn invalid_inputs_fail_clearly() {
        let c = clip(1.0, Vec3::Y, 1.0, Quat::IDENTITY, Quat::IDENTITY);
        assert!(matches!(
            clip_root_delta(&c, 0.0, 1.5, false),
            Err(RootDeltaError::TimeOutOfRange { .. })
        ));
        assert!(clip_root_delta(&c, f32::NAN, 0.5, false).is_err());
        assert!(clip_root_delta(&c, -0.1, 0.5, false).is_err());
        let mut no_rotation = c.clone();
        no_rotation.tracks.truncate(1);
        assert_eq!(
            clip_root_delta(&no_rotation, 0.0, 0.5, false),
            Err(RootDeltaError::MissingRootTracks)
        );
        let mut swapped = c.clone();
        swapped.tracks.swap(0, 1);
        assert_eq!(
            clip_root_delta(&swapped, 0.0, 0.5, false),
            Err(RootDeltaError::MissingRootTracks)
        );
    }
}
