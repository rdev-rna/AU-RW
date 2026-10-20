//! Portable matcher imported at the user's direction from the For Honor rewrite.
//! All native addresses/layouts/settings in imported modules refer to For Honor,
//! never Unity. See docs/FOR_HONOR_MOTION_REUSE.md for exact source provenance.
//! Unity clip decoding, feature generation and playback adapters are separate.
// The validation-only `hw-rsqrt` feature needs one SSE intrinsic call.
#![cfg_attr(not(feature = "hw-rsqrt"), forbid(unsafe_code))]

pub mod amo;
// Preserve the reviewed imported call interfaces and algorithm structure.
#[allow(clippy::too_many_arguments)]
pub mod amo_prediction;
#[allow(clippy::needless_range_loop)]
pub mod amo_request;
#[allow(clippy::manual_is_multiple_of)]
pub mod amo_samples;
pub mod amo_search;
pub mod amo_velocity_smoother;
pub mod bone_modifier;
pub mod coat;
#[allow(clippy::collapsible_if)]
pub mod loco_match;
pub mod modifier_order;
pub mod native_pose;
pub mod reflex;
pub mod target_warp;
pub mod traversal;

#[cfg(test)]
#[path = "fh_root_test.rs"]
mod motion;
pub mod unity;
