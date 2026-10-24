//! Per-frame texture residency queue.
//!
//! Mirrors the original engine's streamer policy, verified from the binary this session
//! (`Graphic::TextureStreamingManager`, policy at RVA 0x2222780, config read by
//! `sub_22055B0`): a pool budget in bytes (live value 104857600 = 100 MiB), a time slice
//! compared against accumulated elapsed time, and a per-pass item cap. The update walks the
//! request queue and hard-stops when the used budget is reached; each texture is dispatched
//! as an async job, never uploaded inline.
//!
//! This module reproduces the budget/slice/cap shape, not the async dispatch: uploads run on
//! the frame's main thread, bounded per frame. Eviction (LRU) and mip downgrade are **not**
//! implemented — when the pool fills, uploads stop and it is logged once.
//!
//! Handles stay stable: every texture gets its own 1x1 placeholder `Image` at setup so
//! materials bind and render flat immediately; an upload replaces the asset behind the same
//! handle, which the render world picks up as an asset change.

use std::time::{Duration, Instant};

use bevy::{prelude::*, render::extract_resource::ExtractResource};

/// Engine pool budget: 104857600 bytes, i.e. exactly 100 MiB.
pub const DEFAULT_POOL_MB: u32 = 100;
/// Engine time slice: the queue walk stops once the slice has elapsed.
pub const DEFAULT_SLICE_MS: f32 = 4.0;
/// Engine per-pass item cap.
pub const DEFAULT_PER_FRAME: usize = 5;

/// Bumped once per frame in which at least one texture was uploaded.
///
/// The upload replaces the asset behind a stable `Handle<Image>`, so a bind
/// group built against the 1x1 placeholder keeps the placeholder's view alive;
/// the render world compares this value against the one stored in
/// `PreparedMapPart` and rebuilds exactly those bind groups. On frames with no
/// upload the counter is unchanged and nothing is rebuilt, which is what keeps
/// the whole-map case affordable.
#[derive(Resource, Default, Clone, Copy, Debug, ExtractResource)]
pub struct TextureGeneration(pub u32);

/// One texture waiting for its pixels to be uploaded.
struct Pending {
    texture: usize,
    handle: Handle<Image>,
    image: Option<Image>,
    bytes: usize,
    distance: f32,
}

#[derive(Resource)]
pub struct TextureResidency {
    /// Sorted on the first frame: furthest first, so `pop()` yields the nearest texture.
    queue: Vec<Pending>,
    /// (texture index, world position of a placement that uses it), consumed once for
    /// priority. Empty after `prioritized` is set.
    sites: Vec<(usize, [f32; 3])>,
    pub pool_bytes: usize,
    pub slice: Duration,
    pub per_frame: usize,
    pub used: usize,
    pub resident: usize,
    pub total: usize,
    frames: u32,
    prioritized: bool,
    stopped_full: bool,
    announced_done: bool,
}

impl TextureResidency {
    pub fn new(pool_mb: u32, slice_ms: f32, per_frame: usize) -> Self {
        Self {
            queue: Vec::new(),
            sites: Vec::new(),
            pool_bytes: usize::try_from(pool_mb)
                .unwrap_or(usize::MAX)
                .saturating_mul(1024 * 1024),
            slice: Duration::from_secs_f32(slice_ms.max(0.0) / 1000.0),
            per_frame: per_frame.max(1),
            used: 0,
            resident: 0,
            total: 0,
            frames: 0,
            prioritized: false,
            stopped_full: false,
            announced_done: false,
        }
    }

    /// Queue one texture: its handle already points at a placeholder `Image`.
    pub fn push(&mut self, texture: usize, handle: Handle<Image>, image: Image, bytes: usize) {
        self.total += 1;
        self.queue.push(Pending {
            texture,
            handle,
            image: Some(image),
            bytes,
            distance: f32::INFINITY,
        });
    }

    /// Record a placement that uses `texture`, for nearest-first priority.
    pub fn site(&mut self, texture: usize, position: [f32; 3]) {
        self.sites.push((texture, position));
    }

    /// Nearest-first order. Called once, when the camera exists.
    fn prioritize(&mut self, eye: Vec3) {
        let mut nearest: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
        for (texture, position) in &self.sites {
            let distance = eye.distance(Vec3::from_array(*position));
            let slot = nearest.entry(*texture).or_insert(f32::INFINITY);
            if distance < *slot {
                *slot = distance;
            }
        }
        for pending in &mut self.queue {
            if let Some(distance) = nearest.get(&pending.texture) {
                pending.distance = *distance;
            }
        }
        // Descending distance: the nearest texture is the last element, so `pop()` is O(1).
        self.queue.sort_by(|a, b| {
            b.distance
                .total_cmp(&a.distance)
                .then(b.texture.cmp(&a.texture))
        });
        self.sites = Vec::new();
        self.prioritized = true;
        println!(
            "stream: pool={}MB slice={:.1}ms cap={}/frame",
            self.pool_bytes / (1024 * 1024),
            self.slice.as_secs_f32() * 1000.0,
            self.per_frame
        );
    }

    fn report(&self) {
        println!(
            "stream: textures resident={}/{} used={}MB after {} frames",
            self.resident,
            self.total,
            self.used / (1024 * 1024),
            self.frames
        );
    }
}

/// Uploads while `items < cap` AND `elapsed < slice` AND `used + bytes <= pool`.
pub fn stream_textures(
    residency: Option<ResMut<TextureResidency>>,
    mut images: ResMut<Assets<Image>>,
    mut generation: Option<ResMut<TextureGeneration>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
) {
    let Some(mut residency) = residency else {
        return;
    };
    let residency = &mut *residency;
    if residency.total == 0 {
        return;
    }
    if !residency.prioritized {
        let eye = camera
            .iter()
            .next()
            .map(|transform| transform.translation())
            .unwrap_or(Vec3::ZERO);
        residency.prioritize(eye);
    }

    let started = Instant::now();
    let mut uploaded = 0usize;
    while uploaded < residency.per_frame {
        if started.elapsed() >= residency.slice {
            break;
        }
        let Some(item) = residency.queue.last() else {
            break;
        };
        if residency.used + item.bytes > residency.pool_bytes {
            if !residency.stopped_full {
                residency.stopped_full = true;
                println!(
                    "stream: pool full at {}MB, resident={}/{}, {} textures stay on the placeholder \
                     (no eviction yet)",
                    residency.used / (1024 * 1024),
                    residency.resident,
                    residency.total,
                    residency.queue.len()
                );
            }
            break;
        }
        let item = residency.queue.pop().expect("peeked above");
        if let Some(image) = item.image
            && let Some(mut slot) = images.get_mut(&item.handle)
        {
            // Same handle identity: materials already bound pick the real pixels up.
            *slot = image;
            residency.used += item.bytes;
            residency.resident += 1;
            uploaded += 1;
        }
    }

    // One bump per frame that changed pixels: the render world rebuilds the
    // affected per-part bind groups (they still point at the placeholder view).
    if uploaded > 0
        && let Some(generation) = generation.as_mut()
    {
        generation.0 = generation.0.wrapping_add(1);
    }

    residency.frames += 1;
    let done = residency.resident == residency.total;
    if residency.frames % 60 == 0 || (done && !residency.announced_done) {
        residency.announced_done |= done;
        residency.report();
    }
}
