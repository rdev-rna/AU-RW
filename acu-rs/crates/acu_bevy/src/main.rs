//! Synthetic grey test world with Arno read directly from the install.
//!
//! Presentation only: the app reads mesh resources in memory through
//! `acu_assets`, builds Bevy meshes in native units, and stages them in a
//! clearly synthetic arena (grey ground, grey reference blocks). Native space
//! is Z-up as decoded (feet of the Arno set at z ~ 0, head top at z ~ 1.83);
//! the scene root applies an explicit Z-up -> Y-up presentation rotation. That
//! rotation is a presentation choice, not a claim about the engine's axes.
//!
//! With `--capture` the app runs headless: no window and no swapchain. The
//! camera renders into an offscreen image and one screenshot of that image is
//! written to disk, which keeps captures reproducible on machines whose window
//! surfaces misbehave.

mod arena;
mod cache;
mod character_material;
mod controls;
mod io;
mod load;
mod options;
mod player;
mod rig;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use bevy::app::ScheduleRunnerPlugin;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::settings::{WgpuFeatures, WgpuSettings};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::ExitCondition;

use options::{Options, parse};

/// Native (Z-up) to Bevy (Y-up) presentation rotation.
/// Apply a native row-major affine to a point (same convention as the entity matrix).
fn apply_native_matrix(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

/// Fallback material for parts that resolved no material.
#[derive(Resource)]
struct MapGrey(Handle<StandardMaterial>);

/// Set when `--camera-at` pins the camera; follow/orbit then leave it alone.
#[derive(Resource)]
struct FixedCamera;

/// Set when `--auto-walk` drives the player forward for a streaming stress test.
#[derive(Resource)]
struct AutoWalk;

/// Runs after `controls::sample` so the synthetic movement is not overwritten.
fn auto_walk(flag: Option<Res<AutoWalk>>, mut controller: ResMut<controls::Controller>) {
    if flag.is_some() {
        controller.movement = Vec2::new(0.0, -1.0);
        controller.high_profile = true;
    }
}

/// Upload one decoded texture, or skip it when the format is unknown.
fn upload_texture(
    texture: &load::LoadedTexture,
    images: &mut Assets<Image>,
) -> Option<Handle<Image>> {
    if texture.pixels.is_empty() {
        return None;
    }
    let mut format = match texture.dxgi_format {
        71 | 72 => TextureFormat::Bc1RgbaUnormSrgb,
        74 | 75 => TextureFormat::Bc2RgbaUnormSrgb,
        77 | 78 => TextureFormat::Bc3RgbaUnormSrgb,
        98 | 99 => TextureFormat::Bc7RgbaUnormSrgb,
        28 | 29 => TextureFormat::Rgba8UnormSrgb,
        87 | 91 => TextureFormat::Bgra8UnormSrgb,
        83 => TextureFormat::Bc5RgUnorm,
        other => {
            eprintln!(
                "note: texture {} uses unsupported DXGI format {other}; not uploaded",
                texture.file_id
            );
            return None;
        }
    };
    if !texture.srgb {
        format = format.remove_srgb_suffix();
    }
    let mut image = Image::new_uninit(
        Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        format,
        bevy::asset::RenderAssetUsages::default(),
    );
    image.data = Some(texture.pixels.clone());
    image.texture_descriptor.mip_level_count = texture.mip_count;
    image.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
        address_mode_u: bevy::image::ImageAddressMode::Repeat,
        address_mode_v: bevy::image::ImageAddressMode::Repeat,
        ..bevy::image::ImageSamplerDescriptor::linear()
    });
    Some(images.add(image))
}

/// Standard material for one decoded material (diffuse only).
fn stream_material(
    material: &load::LoadedMaterial,
    textures: &[Option<Handle<Image>>],
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    let diffuse = material
        .diffuse_texture
        .and_then(|index| textures.get(index).cloned().flatten());
    materials.add(StandardMaterial {
        base_color: if diffuse.is_some() {
            Color::WHITE
        } else {
            Color::srgb(0.72, 0.72, 0.72)
        },
        base_color_texture: diffuse,
        perceptual_roughness: 0.85,
        metallic: 0.0,
        cull_mode: None,
        ..default()
    })
}

/// Cells loaded by distance around the player.
#[derive(Resource)]
struct Streaming {
    runtime: load::MapRuntime,
    /// (cell entry, native x, native y) from `--map-cell-centres`.
    centres: Vec<(usize, f32, f32)>,
    radius: f32,
    loaded: HashMap<usize, Vec<Entity>>,
    anchor: Vec3,
    root: Entity,
    handles: Vec<Vec<Handle<Mesh>>>,
    textures: Vec<Option<Handle<Image>>>,
    materials: Vec<Handle<StandardMaterial>>,
    /// Part accounting for the streamed ring: how many parts had a material bound, and how
    /// many of those resolved a diffuse map that actually uploaded.
    parts: usize,
    parts_bound: usize,
    parts_textured: usize,
    /// Materials with no uploaded diffuse, reported once each.
    untextured: std::collections::HashSet<String>,
    /// World bounds of everything streamed so far, reported once.
    bounds: Option<(Vec3, Vec3)>,
    /// World-space boxes per cell, for camera collision.
    colliders: std::collections::HashMap<usize, Vec<(Vec3, Vec3)>>,
    /// Where the player was last frame, and for how long they have been still.
    last_position: Vec3,
    still_for: f32,
    /// Wall-clock time of the last cell decode, to space the unavoidable stalls.
    last_load: f64,
}

/// Build one Bevy mesh from a decoded part.
fn part_mesh(piece: &load::LoadedPiece, part: &load::LoadedPart) -> Mesh {
    let mut positions = Vec::with_capacity(piece.vertices.len());
    let mut normals = Vec::with_capacity(piece.vertices.len());
    let mut uvs = Vec::with_capacity(piece.vertices.len());
    let mut tangents = Vec::with_capacity(piece.vertices.len());
    for v in &piece.vertices {
        positions.push(v.position);
        normals.push(v.normal);
        uvs.push(v.uv0.unwrap_or([0.0, 0.0]));
        let n = Vec3::from_array(v.normal).normalize_or_zero();
        let t = Vec3::from_array(v.tangent.unwrap_or([1.0, 0.0, 0.0])).normalize_or_zero();
        let b = Vec3::from_array(v.binormal.unwrap_or([0.0, 1.0, 0.0]));
        tangents.push([
            t.x,
            t.y,
            t.z,
            if n.cross(t).dot(b) < 0.0 { -1.0 } else { 1.0 },
        ]);
    }
    Mesh::new(
        bevy::render::render_resource::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::MAIN_WORLD | bevy::asset::RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, tangents)
    .with_inserted_indices(bevy::mesh::Indices::U32(
        part.triangles.iter().flatten().copied().collect(),
    ))
}

/// Load and unload cells so the world follows the player. Each update adds at most two
/// cells, which keeps the hitch small; startup only loads the first ring.
fn stream_cells(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    time: Res<Time>,
    grey: Res<MapGrey>,
    state: Option<Res<player::State>>,
    controller: Res<controls::Controller>,
    mut streaming: Option<ResMut<Streaming>>,
) {
    let (Some(state), Some(mut stream)) = (state, streaming.as_mut()) else {
        return;
    };
    let position = state.player.position;
    let here = Vec2::new(position.x, position.z);
    let mut target: HashSet<usize> = HashSet::new();
    for &(entry, x, y) in &stream.centres {
        // native (x, y) -> Bevy (x, -y)
        if Vec2::new(x, -y).distance(here) <= stream.radius {
            target.insert(entry);
        }
    }

    // Unload only after moving far enough; filling happens every frame so the first
    // ring arrives progressively instead of in one stall.
    let moved = position.distance(stream.anchor) >= 40.0;
    if moved {
        stream.anchor = position;
    }
    let mut dropped = 0usize;
    if moved {
        let stale: Vec<usize> = stream
            .loaded
            .keys()
            .copied()
            .filter(|entry| !target.contains(entry))
            .collect();
        for entry in stale {
            stream.colliders.remove(&entry);
            if let Some(entities) = stream.loaded.remove(&entry) {
                dropped += 1;
                for entity in entities {
                    commands.entity(entity).despawn();
                }
            }
        }
    }

    // Cell decode costs ~250 ms on the main thread, so it must not run while the player is
    // walking: that was a guaranteed freeze on every step. Cells now fill while standing
    // still, which keeps movement smooth; unloading still happens immediately.
    let walking = controller.movement.length() > 0.1;
    if walking || position.distance(stream.last_position) >= 0.02 {
        stream.still_for = 0.0;
    } else {
        stream.still_for += time.delta_secs();
    }
    stream.last_position = position;
    let mut added = 0usize;
    // One decode per 0.3 s: each costs ~250 ms on the main thread, so back-to-back loads read
    // as a freeze. Spacing them keeps the fill visible as a stutter instead.
    let now = time.elapsed_secs_f64();
    if stream.still_for < 0.25 || time.delta_secs() > 0.05 || now - stream.last_load < 0.3 {
        return;
    }
    stream.last_load = now;
    for entry in target {
        if added >= 1 {
            break;
        }
        if stream.loaded.contains_key(&entry) {
            continue;
        }
        match stream.runtime.load_cells(&[entry]) {
            Ok(instances) => {
                while stream.handles.len() < stream.runtime.pieces().len() {
                    let index = stream.handles.len();
                    let piece = &stream.runtime.pieces()[index];
                    let built: Vec<Handle<Mesh>> = piece
                        .parts
                        .iter()
                        .map(|part| meshes.add(part_mesh(piece, part)))
                        .collect();
                    stream.handles.push(built);
                }
                // Upload any textures/materials this cell introduced, then place it.
                while stream.textures.len() < stream.runtime.scene().textures.len() {
                    let index = stream.textures.len();
                    let texture = &stream.runtime.scene().textures[index];
                    let handle = upload_texture(texture, &mut images);
                    stream.textures.push(handle);
                }
                while stream.materials.len() < stream.runtime.scene().materials.len() {
                    let index = stream.materials.len();
                    let material = &stream.runtime.scene().materials[index];
                    let handle =
                        stream_material(material, &stream.textures, &mut standard_materials);
                    stream.materials.push(handle);
                }
                let (mut parts, mut bound, mut textured) = (0usize, 0usize, 0usize);
                for instance in &instances {
                    // Copy the bindings out first: the audit mutates `stream`, so it cannot
                    // hold a borrow of the runtime across the loop body.
                    let bindings: Vec<Option<usize>> = stream.runtime.pieces()[instance.piece]
                        .parts
                        .iter()
                        .map(|part| part.material)
                        .collect();
                    for binding in bindings {
                        parts += 1;
                        let Some(index) = binding else {
                            continue;
                        };
                        bound += 1;
                        let uploaded = stream
                            .materials
                            .get(index)
                            .and_then(|handle| standard_materials.get(handle))
                            .map(|material| material.base_color_texture.is_some())
                            .unwrap_or(false);
                        if uploaded {
                            textured += 1;
                            continue;
                        }
                        // Distinguish "no diffuse by design" from "diffuse named but not
                        // resolved": only the second is a bug in this loader.
                        let info = stream.runtime.scene().materials.get(index).map(|material| {
                            (
                                material.file_id.clone(),
                                material.selector_diffuse.clone(),
                                material.set_diffuse.clone(),
                                material.textures.len(),
                                material.source.clone(),
                            )
                        });
                        if let Some((file_id, selector, set, textures, source)) = info {
                            if stream.untextured.insert(file_id.clone()) {
                                eprintln!(
                                    "map: untextured material {file_id} selector_diffuse={selector:?} set_diffuse={set:?} textures={textures} source={source}"
                                );
                            }
                        }
                    }
                }
                stream.parts += parts;
                stream.parts_bound += bound;
                stream.parts_textured += textured;
                // Where the streamed geometry actually ends up in world space: a mismatch here
                // against the cell centroids is what puts the camera inside a wall.
                let mut world_min = Vec3::splat(f32::INFINITY);
                let mut world_max = Vec3::splat(f32::NEG_INFINITY);
                let mut cell_boxes: Vec<(Vec3, Vec3)> = Vec::new();
                let mut entities = Vec::with_capacity(instances.len());
                // Cell matrices are native (Z up). The non-streaming path gets the y-up
                // correction from a parent root; streamed entities have no parent, so the
                // same rotation is folded into each transform. Without it the streamed city
                // is rotated 90 degrees against the player and every view is inside geometry.
                for instance in &instances {
                    // The instances are children of stream.root, which already carries the
                    // native Z-up to Bevy Y-up rotation; rotating here again would double it.
                    let world = Mat4::from_cols_array(&instance.matrix);
                    let transform = Transform::from_matrix(world);
                    {
                        let piece = &stream.runtime.pieces()[instance.piece];
                        let (min, max) = piece.bounds();
                        let mut instance_min = Vec3::splat(f32::INFINITY);
                        let mut instance_max = Vec3::splat(f32::NEG_INFINITY);
                        for corner in 0..8 {
                            let local = Vec3::new(
                                if corner & 1 == 0 { min[0] } else { max[0] },
                                if corner & 2 == 0 { min[1] } else { max[1] },
                                if corner & 4 == 0 { min[2] } else { max[2] },
                            );
                            let at = world.transform_point3(local);
                            instance_min = instance_min.min(at);
                            instance_max = instance_max.max(at);
                            world_min = world_min.min(at);
                            world_max = world_max.max(at);
                        }
                        // Camera collision boxes are in world space: the instances sit under
                        // the rotated root, so the same rotation is applied here.
                        let rotated = Mat4::from_quat(native_z_up_to_bevy_y_up()) * world;
                        let mut box_min = Vec3::splat(f32::INFINITY);
                        let mut box_max = Vec3::splat(f32::NEG_INFINITY);
                        for corner in 0..8 {
                            let local = Vec3::new(
                                if corner & 1 == 0 { min[0] } else { max[0] },
                                if corner & 2 == 0 { min[1] } else { max[1] },
                                if corner & 4 == 0 { min[2] } else { max[2] },
                            );
                            let at = rotated.transform_point3(local);
                            box_min = box_min.min(at);
                            box_max = box_max.max(at);
                        }
                        // Small props (chairs, carts, books) must not shove the camera; only
                        // walls, floors and roofs are occluders.
                        if (box_max - box_min).max_element() > 4.0 {
                            cell_boxes.push((box_min, box_max));
                        }
                        // Name the placement that spans kilometres: it is what encloses the
                        // camera when a cell resolves one bad transform.
                        let span = (instance_max - instance_min).abs().max_element();
                        if span > 2_000.0 {
                            eprintln!(
                                "stream: oversized placement {} '{}' span {:.0} m local {:?}..{:?} translation {:?}",
                                piece.file_id,
                                piece.label,
                                span,
                                min,
                                max,
                                [
                                    instance.matrix[12],
                                    instance.matrix[13],
                                    instance.matrix[14]
                                ]
                            );
                        }
                    }
                    let piece = &stream.runtime.pieces()[instance.piece];
                    for (part_index, handle) in stream.handles[instance.piece].iter().enumerate() {
                        let material = piece
                            .parts
                            .get(part_index)
                            .and_then(|part| part.material)
                            .and_then(|index| stream.materials.get(index).cloned())
                            .unwrap_or_else(|| grey.0.clone());
                        entities.push(
                            commands
                                .spawn((
                                    Mesh3d(handle.clone()),
                                    MeshMaterial3d(material),
                                    transform,
                                ))
                                .id(),
                        );
                    }
                }
                commands.entity(stream.root).add_children(&entities);
                stream.bounds = Some(match stream.bounds {
                    Some((lo, hi)) => (lo.min(world_min), hi.max(world_max)),
                    None => (world_min, world_max),
                });
                if stream.loaded.len() <= 2 {
                    eprintln!("stream: world bounds min {world_min:?} max {world_max:?}");
                }
                stream.colliders.insert(entry, cell_boxes);
                stream.loaded.insert(entry, entities);
                added += 1;
            }
            Err(error) => eprintln!("stream: cell {entry}: {error}"),
        }
    }
    if added > 0 || dropped > 0 {
        let (pieces, groups, members) = stream.runtime.stats();
        if added > 0 {
            eprintln!("stream: skipped {}", stream.runtime.skip_report());
        }
        eprintln!(
            "stream: at ({:.0},{:.0}) added {added} dropped {dropped} total {} cells, {pieces} pieces, {groups} buildings ({members} parts), {} parts [{} bound, {} textured]",
            position.x,
            position.z,
            stream.loaded.len(),
            stream.parts,
            stream.parts_bound,
            stream.parts_textured
        );
    }
}

fn native_z_up_to_bevy_y_up() -> Quat {
    Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)
}

#[derive(Resource)]
struct Capture {
    path: PathBuf,
    frames: u32,
    rendered: u32,
    requested: bool,
    /// Frames waited for the screenshot after requesting it.
    waited: u32,
    /// Offscreen render target written by the screenshot.
    image: Option<Handle<Image>>,
}

/// Frames to wait for the screenshot save before giving up.
const CAPTURE_DEADLINE_FRAMES: u32 = 600;

#[derive(Component)]
struct InspectionCamera {
    center: Vec3,
    azimuth: f32,
    elevation: f32,
    distance: f32,
    /// Framing distance chosen at spawn; the zoom clamp is relative to it so a city-scale
    /// camera is not yanked to 20 units on the first frame.
    initial: f32,
    /// Distance actually used after collision.
    actual: f32,
    /// Last distance used, for the periodic status line.
    report: f32,
    /// Requested boom length before collision.
    report_desired: f32,
    /// Collision probe results are cached: sweeping twelve rays against tens of thousands of
    /// boxes every frame cost hundreds of milliseconds.
    allowed: f32,
    probe_after: f32,
}

fn inspect_camera(
    fixed: Option<Res<FixedCamera>>,
    streaming: Option<Res<Streaming>>,
    keys: Res<ButtonInput<KeyCode>>,
    controller: Res<controls::Controller>,
    time: Res<Time>,
    mut cameras: Query<(&mut Transform, &mut InspectionCamera)>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
    if fixed.is_some() {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let axis = |positive, negative| {
        f32::from(u8::from(keys.pressed(positive))) - f32::from(u8::from(keys.pressed(negative)))
    };
    for (mut transform, mut orbit) in &mut cameras {
        orbit.azimuth +=
            (axis(KeyCode::ArrowRight, KeyCode::ArrowLeft) - controller.look.x * 2.4) * dt;
        orbit.elevation = (orbit.elevation
            + (axis(KeyCode::ArrowUp, KeyCode::ArrowDown) - controller.look.y * 1.8) * dt)
            .clamp(-0.1, 1.4);
        orbit.distance = (orbit.distance * (axis(KeyCode::KeyE, KeyCode::KeyQ) * dt).exp())
            .clamp((orbit.initial * 0.02).max(0.05), orbit.initial * 4.0);
        let mut azimuth = orbit.azimuth;
        let elevation = orbit.elevation;
        let mut direction = boom_direction(azimuth, elevation);
        // Keep the eye out of walls: the nearest box along the boom pulls it in. Pulling in
        // snaps (never clip), releasing eases back so the view does not jitter.
        let desired = orbit.distance;
        let mut allowed = orbit.allowed;
        if orbit.probe_after <= 0.0
            && let Some(streaming) = streaming.as_ref()
        {
            // Only boxes near enough to block the boom are worth testing; the ring holds tens
            // of thousands and a full scan costs tens of milliseconds.
            let reach = (desired + 2.0) * (desired + 2.0);
            let near = |min: &Vec3, max: &Vec3| {
                orbit
                    .center
                    .clamp(*min, *max)
                    .distance_squared(orbit.center)
                    <= reach
            };
            let mut nearest = desired;
            for (min, max) in streaming.colliders.values().flatten() {
                if !near(min, max) {
                    continue;
                }
                if let Some(hit) = ray_box(orbit.center, direction, *min, *max)
                    && hit < nearest
                {
                    nearest = hit;
                }
            }
            allowed = nearest.min(desired);
            if nearest < desired {
                allowed = (nearest - 0.35).max(3.0);
                // A blocked boom must not read as "zoom does nothing". Sweep the boom around
                // the player and ease towards the clearest direction, so the camera finds the
                // open side of a street instead of jamming into the nearest wall.
                let mut best = nearest;
                let mut best_azimuth = azimuth;
                for step in 1..12 {
                    let candidate = azimuth + step as f32 * std::f32::consts::TAU / 12.0;
                    let dir = boom_direction(candidate, elevation);
                    let mut hit = desired;
                    for (min, max) in streaming.colliders.values().flatten() {
                        if !near(min, max) {
                            continue;
                        }
                        if let Some(found) = ray_box(orbit.center, dir, *min, *max)
                            && found < hit
                        {
                            hit = found;
                        }
                    }
                    if hit > best {
                        best = hit;
                        best_azimuth = candidate;
                    }
                }
                if best > nearest + 1.0 {
                    let delta = (best_azimuth - azimuth + std::f32::consts::PI)
                        .rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI;
                    let step = (dt * 3.0).max(0.02);
                    azimuth += delta.clamp(-step, step);
                }
            }
        }
        allowed = allowed.min(desired);
        orbit.allowed = allowed;
        orbit.probe_after -= dt;
        if orbit.probe_after <= 0.0 {
            orbit.probe_after = 0.2;
        }
        // Pull in immediately (never clip), ease back out when the way is clear. Zooming out
        // used to be clamped away because `actual` was pinned to the smaller of the two.
        if allowed < orbit.actual {
            orbit.actual = allowed;
        } else {
            orbit.actual += (allowed - orbit.actual) * (dt * 8.0).min(1.0);
        }
        let distance = orbit.actual;
        orbit.azimuth = azimuth;
        orbit.report = distance;
        orbit.report_desired = orbit.distance;
        *transform = Transform::from_translation(orbit.center + direction * distance)
            .looking_at(orbit.center, Vec3::Y);
    }
}

fn main() {
    let options = match parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let capture_path = options.capture.clone();
    let frames = options.frames.max(1);
    let size = options.size;
    let headless = capture_path.is_some();
    let mut app = App::new();
    let mut plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: if headless {
                None
            } else {
                Some(Window {
                    title: "AU-RW | Arno | Low profile walk / RT sprint | X up / Circle down | Original navigation clips".into(),
                    resolution: (size.0, size.1).into(),
                    ..default()
                })
            },
            exit_condition: if headless {
                ExitCondition::DontExit
            } else {
                ExitCondition::OnPrimaryClosed
            },
            close_when_requested: !headless,
            ..default()
        })
        .set(bevy::render::RenderPlugin {
            render_creation: WgpuSettings {
                features: WgpuFeatures::TEXTURE_COMPRESSION_BC,
                ..default()
            }
            .into(),
            // Capture runs are short and headless; pipeline compilation is
            // asynchronous by default, so meshes and the screenshot blit would
            // not be ready before the capture frame. Compile synchronously.
            synchronous_pipeline_compilation: true,
            ..default()
        });
    if headless {
        // Drive the app ourselves; without a window winit has nothing to pump.
        plugins = plugins.disable::<bevy::winit::WinitPlugin>();
        app.add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO));
    }
    app.insert_resource(bevy::winit::WinitSettings {
        // Keep stepping even if the window lost focus; the old reactive mode made the
        // viewer look frozen whenever the terminal held focus.
        focused_mode: bevy::winit::UpdateMode::Continuous,
        unfocused_mode: bevy::winit::UpdateMode::Continuous,
    });
    app.add_plugins(plugins)
        .add_plugins(MaterialPlugin::<arena::ArenaMaterial>::default())
        .add_plugins(MaterialPlugin::<character_material::CharacterMaterial>::default())
        .insert_resource(Time::<Fixed>::from_hz(60.0))
        .init_resource::<controls::Controller>()
        .add_systems(PreUpdate, controls::sample.after(bevy::input::InputSystems))
        .insert_resource(options)
        .add_systems(Startup, setup_world);
    app.add_systems(
        FixedUpdate,
        player::step.run_if(resource_exists::<player::State>),
    )
    .add_systems(
        Update,
        (player::present, player::follow)
            .chain()
            .run_if(resource_exists::<player::State>),
    );
    app.add_systems(Update, inspect_camera.after(player::follow));
    app.add_systems(Update, stream_cells.after(player::follow));
    app.add_systems(Update, report_fps);
    app.add_systems(PreUpdate, auto_walk.after(controls::sample));
    if !headless {
        println!(
            "Controller: left stick walk; hold RT/R2 sprint; RT+X (South/A) upward priority; RT+Circle (East/B) downward priority; right stick camera yaw/pitch. Keyboard: WASD walk; Shift sprint; Shift+Space up; Shift+C down; Space jump; R reset. Arrows camera; Q/E zoom; F2 bind; F3 freeze; Esc exit. Original navigation clips; synthetic arena traversal."
        );
    }
    if let Some(path) = capture_path {
        app.insert_resource(Capture {
            path,
            frames,
            rendered: 0,
            requested: false,
            waited: 0,
            image: None,
        })
        .add_systems(Update, (request_capture, capture_timeout, report_fps));
    }
    bevy::asset::embedded_asset!(app, "shaders/arena.wgsl");
    bevy::asset::embedded_asset!(app, "shaders/character.wgsl");
    app.run();
}

#[allow(clippy::too_many_arguments)] // Bevy independently borrows each asset store.
fn setup_world(
    mut commands: Commands,
    options: Res<Options>,
    capture: Option<ResMut<Capture>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut arena_materials: ResMut<Assets<arena::ArenaMaterial>>,
    mut character_materials: ResMut<Assets<character_material::CharacterMaterial>>,
) {
    println!("install dir: {}", options.game_dir.display());
    let mut loaded = load::load_all(&options).unwrap_or_else(|e| {
        eprintln!("asset load failed: {e}");
        std::process::exit(3);
    });
    let mut library = options
        .rigged
        .then(|| rig::load(&options.game_dir).expect("load original rig candidates"));
    let mut state = library.as_ref().map(|library| {
        let (matcher, report) =
            motion::load(&options.game_dir, library).expect("decode original navigation tracks");
        player::State::new(&options, matcher, report)
    });
    if let (Some(state), Some(spawn)) = (state.as_mut(), options.spawn) {
        state.player.position = Vec3::from_array(spawn);
        state.player.floor = spawn[1];
    }
    let texture_handles: Vec<_> = loaded
        .scene
        .textures
        .iter()
        .map(|texture| {
            if texture.pixels.is_empty() {
                return None;
            }
            // Diffuse maps are displayed as sRGB. This is a presentation conversion,
            // not an implementation of the original material template shader.
            let mut format = match texture.dxgi_format {
                71 | 72 => TextureFormat::Bc1RgbaUnormSrgb,
                74 | 75 => TextureFormat::Bc2RgbaUnormSrgb,
                77 | 78 => TextureFormat::Bc3RgbaUnormSrgb,
                98 | 99 => TextureFormat::Bc7RgbaUnormSrgb,
                28 | 29 => TextureFormat::Rgba8UnormSrgb,
                87 | 91 => TextureFormat::Bgra8UnormSrgb,
                83 => TextureFormat::Bc5RgUnorm,
                other => {
                    // A texture format the uploader does not know must not abort the whole
                    // scene; the material falls back to grey and the texture is named.
                    eprintln!(
                        "note: texture {} uses unsupported DXGI format {other}; not uploaded",
                        texture.file_id
                    );
                    return None;
                }
            };
            if !texture.srgb {
                format = format.remove_srgb_suffix();
            }
            let mut image = Image::new_uninit(
                Extent3d {
                    width: texture.width,
                    height: texture.height,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                format,
                bevy::asset::RenderAssetUsages::default(),
            );
            image.data = Some(texture.pixels.clone());
            image.texture_descriptor.mip_level_count = texture.mip_count;
            image.sampler =
                bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
                    address_mode_u: bevy::image::ImageAddressMode::Repeat,
                    address_mode_v: bevy::image::ImageAddressMode::Repeat,
                    ..bevy::image::ImageSamplerDescriptor::linear()
                });
            Some(images.add(image))
        })
        .collect();
    let bound_materials: Vec<_> = loaded
        .scene
        .materials
        .iter()
        .map(|material| {
            let texture = material
                .diffuse_texture
                .and_then(|i| texture_handles[i].clone());
            println!(
                "material {} diffuse={:?} notes={:?}",
                material.file_id,
                material
                    .selector_diffuse
                    .as_ref()
                    .or(material.set_diffuse.as_ref()),
                material.notes
            );
            let normal_texture = material
                .normal_texture
                .and_then(|i| texture_handles[i].clone());
            let has_normal = normal_texture.is_some();
            character_materials.add(character_material::CharacterMaterial {
                base: StandardMaterial {
                    base_color: Color::WHITE,
                    base_color_texture: texture,
                    double_sided: true,
                    alpha_mode: if material.alpha_test_enabled {
                        AlphaMode::Mask(f32::from(material.alpha_test_value) / 255.0)
                    } else {
                        AlphaMode::Opaque
                    },
                    perceptual_roughness: 0.85,
                    cull_mode: None,
                    ..default()
                },
                extension: character_material::CharacterSurface {
                    packed_normal: normal_texture,
                    params: Vec4::new(if has_normal { 1.0 } else { 0.0 }, 1.0, 0.0, 0.0),
                },
            })
        })
        .collect();
    let map_mode = !loaded.instances.is_empty();
    let instances = loaded.instances;
    let pieces = loaded.pieces;
    let rig_pieces = loaded.rig_pieces;
    // In map mode the rig pieces are appended after the unique map meshes.
    let arno_start = if map_mode {
        pieces.len().saturating_sub(rig_pieces)
    } else {
        0
    };

    let mut bevy_min = Vec3::splat(f32::INFINITY);
    let mut bevy_max = Vec3::splat(f32::NEG_INFINITY);
    let mut native_min = [f32::INFINITY; 3];
    let mut native_max = [f32::NEG_INFINITY; 3];
    for piece in &pieces {
        println!(
            "piece {} archive={} entry={} resource={} file_id={} entry_sha256={} payload_sha256={} qf={} src={}/{} verts={} tris={}",
            piece.label,
            piece.archive.display(),
            piece.entry,
            piece.resource,
            piece.file_id,
            piece.entry_stored_sha256,
            piece.payload_sha256,
            piece.quantization_factor,
            piece.vertex_source,
            piece.index_source,
            piece.vertices.len(),
            piece
                .parts
                .iter()
                .map(|part| part.triangles.len())
                .sum::<usize>(),
        );
    }
    if map_mode && instances.len() > 8 {
        // A multi-cell scene is framed by its placement origins, using the 2nd..98th
        // percentile per axis: river arms and distant chunks otherwise stretch the frame
        // and leave the city small in the middle.
        let mut axes: [Vec<f32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        for instance in &instances {
            for (axis, values) in axes.iter_mut().enumerate() {
                values.push(instance.matrix[12 + axis]);
            }
        }
        let low = ((instances.len() as f32) * 0.02) as usize;
        let high = ((instances.len() as f32) * 0.98) as usize;
        for (axis, values) in axes.iter_mut().enumerate() {
            values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            native_min[axis] = values[low.min(values.len() - 1)];
            native_max[axis] = values[high.min(values.len() - 1)];
        }
        for corner in 0..8 {
            let p = Vec3::new(
                if corner & 1 == 0 {
                    native_min[0]
                } else {
                    native_max[0]
                },
                if corner & 2 == 0 {
                    native_min[1]
                } else {
                    native_max[1]
                },
                if corner & 4 == 0 {
                    native_min[2]
                } else {
                    native_max[2]
                },
            );
            let rotated = native_z_up_to_bevy_y_up() * p;
            bevy_min = bevy_min.min(rotated);
            bevy_max = bevy_max.max(rotated);
        }
    } else if map_mode {
        // One cell: frame the meshes themselves. Outliers are reported.
        let mut outliers: Vec<(f32, String, [f32; 16])> = Vec::new();
        for instance in &instances {
            let piece = &pieces[instance.piece];
            let (min, max) = piece.bounds();
            let mut extent = 0f32;
            for corner in 0..8 {
                let p = [
                    if corner & 1 == 0 { min[0] } else { max[0] },
                    if corner & 2 == 0 { min[1] } else { max[1] },
                    if corner & 4 == 0 { min[2] } else { max[2] },
                ];
                let q = apply_native_matrix(&instance.matrix, p);
                for i in 0..3 {
                    native_min[i] = native_min[i].min(q[i]);
                    native_max[i] = native_max[i].max(q[i]);
                    extent = extent.max(q[i].abs());
                }
                let rotated = native_z_up_to_bevy_y_up() * Vec3::from_array(q);
                bevy_min = bevy_min.min(rotated);
                bevy_max = bevy_max.max(rotated);
            }
            outliers.push((extent, piece.label.clone(), instance.matrix));
        }
        outliers.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (extent, label, matrix) in outliers.iter().take(6) {
            eprintln!(
                "bounds outlier: |coord| {extent:.0} {label} translation {:?} scale-ish {:?}",
                [matrix[12], matrix[13], matrix[14]],
                [matrix[0], matrix[5], matrix[10]]
            );
        }
    } else {
        for piece in &pieces {
            let (min, max) = piece.bounds();
            for i in 0..3 {
                native_min[i] = native_min[i].min(min[i]);
                native_max[i] = native_max[i].max(max[i]);
            }
            for corner in 0..8 {
                let p = Vec3::new(
                    if corner & 1 == 0 { min[0] } else { max[0] },
                    if corner & 2 == 0 { min[1] } else { max[1] },
                    if corner & 4 == 0 { min[2] } else { max[2] },
                );
                let rotated = native_z_up_to_bevy_y_up() * p;
                bevy_min = bevy_min.min(rotated);
                bevy_max = bevy_max.max(rotated);
            }
        }
    }
    if pieces.is_empty() {
        eprintln!("no pieces loaded");
        std::process::exit(3);
    }
    println!(
        "native bounds min={native_min:?} max={native_max:?}; bevy bounds min={bevy_min:?} max={bevy_max:?}"
    );

    let grey = materials.add(StandardMaterial {
        base_color: Color::srgb(0.72, 0.72, 0.72),
        perceptual_roughness: 0.85,
        metallic: 0.0,
        // Native triangle winding is unverified; render both faces.
        cull_mode: None,
        ..default()
    });
    // stream_cells requires this resource; without it the system fails validation on every
    // non-streaming invocation (single cell, dry run), which surfaced as a Resource error.
    commands.insert_resource(MapGrey(grey.clone()));
    let character_grey = character_materials.add(character_material::CharacterMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.72, 0.72, 0.72),
            cull_mode: None,
            double_sided: true,
            perceptual_roughness: 0.85,
            ..default()
        },
        extension: character_material::CharacterSurface {
            packed_normal: None,
            params: Vec4::new(0.0, 1.0, 0.0, 0.0),
        },
    });
    let ground_material = arena_materials.add(arena::ArenaMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.38, 0.38, 0.40),
            perceptual_roughness: 0.95,
            ..default()
        },
        extension: arena::ArenaShader {
            grid: Vec4::new(1.0, 0.65, 0.0, 0.0),
        },
    });

    // Synthetic arena: 24x24 grey ground plus block references at known sizes.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(24.0, 24.0))),
        MeshMaterial3d(ground_material),
    ));
    for (x, z, size) in acu_sim::BLOCKS {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size, size, size))),
            MeshMaterial3d(grey.clone()),
            Transform::from_xyz(x, size / 2.0, z),
        ));
    }

    if map_mode {
        // One mesh asset per unique piece part; every placement is an instance sharing it.
        let mut part_meshes: Vec<Vec<Handle<Mesh>>> = Vec::with_capacity(pieces.len());
        let mut part_materials: Vec<Vec<Handle<character_material::CharacterMaterial>>> =
            Vec::with_capacity(pieces.len());
        for piece in &pieces[..arno_start] {
            let mut positions = Vec::with_capacity(piece.vertices.len());
            let mut normals = Vec::with_capacity(piece.vertices.len());
            let mut uvs = Vec::with_capacity(piece.vertices.len());
            let mut tangents = Vec::with_capacity(piece.vertices.len());
            for v in &piece.vertices {
                positions.push(v.position);
                normals.push(v.normal);
                uvs.push(v.uv0.unwrap_or([0.0, 0.0]));
                let n = Vec3::from_array(v.normal).normalize_or_zero();
                let t = Vec3::from_array(v.tangent.unwrap_or([1.0, 0.0, 0.0])).normalize_or_zero();
                let b = Vec3::from_array(v.binormal.unwrap_or([0.0, 1.0, 0.0]));
                tangents.push([
                    t.x,
                    t.y,
                    t.z,
                    if n.cross(t).dot(b) < 0.0 { -1.0 } else { 1.0 },
                ]);
            }
            let mut meshes_for_piece = Vec::with_capacity(piece.parts.len());
            let mut materials_for_piece = Vec::with_capacity(piece.parts.len());
            for part in &piece.parts {
                let mesh = Mesh::new(
                    bevy::render::render_resource::PrimitiveTopology::TriangleList,
                    bevy::asset::RenderAssetUsages::MAIN_WORLD
                        | bevy::asset::RenderAssetUsages::RENDER_WORLD,
                )
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone())
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals.clone())
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs.clone())
                .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, tangents.clone())
                .with_inserted_indices(bevy::mesh::Indices::U32(
                    part.triangles.iter().flatten().copied().collect(),
                ));
                meshes_for_piece.push(meshes.add(mesh));
                materials_for_piece.push(
                    part.material
                        .and_then(|i| bound_materials.get(i).cloned())
                        .unwrap_or_else(|| character_grey.clone()),
                );
            }
            part_meshes.push(meshes_for_piece);
            part_materials.push(materials_for_piece);
        }
        commands
            .spawn((
                Transform::from_rotation(native_z_up_to_bevy_y_up()),
                Visibility::default(),
            ))
            .with_children(|root| {
                let mut spawned = 0usize;
                let mut drawn = 0usize;
                let radius = options.map_radius;
                let origin = options.spawn.map(Vec3::from_array);
                for instance in &instances {
                    if let (Some(radius), Some(origin)) = (radius, origin) {
                        let at = Vec3::new(
                            instance.matrix[12],
                            instance.matrix[14],
                            -instance.matrix[13],
                        );
                        if at.distance(origin) > radius {
                            continue;
                        }
                    }
                    drawn += 1;
                    let transform = Transform::from_matrix(Mat4::from_cols_array(&instance.matrix));
                    for (mesh, material) in part_meshes[instance.piece]
                        .iter()
                        .zip(&part_materials[instance.piece])
                    {
                        spawned += 1;
                        root.spawn((
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(material.clone()),
                            transform,
                        ));
                    }
                }
                println!(
                    "map: {spawned} entities for {drawn}/{} placements (radius {:?} m)",
                    instances.len(),
                    radius
                );
            });
    }
    if options.rigged {
        // Arno pieces as children of one native-space root.
        commands
            .spawn((
                player::Character,
                Transform::from_rotation(native_z_up_to_bevy_y_up()),
                Visibility::default(),
            ))
            .with_children(|parent| {
                for piece in &pieces[arno_start..] {
                    let mut skin = library
                        .as_mut()
                        .map(|l| l.skin(piece).expect("validate skin influences and bind"));
                    let mut positions = Vec::with_capacity(piece.vertices.len());
                    let mut normals = Vec::with_capacity(piece.vertices.len());
                    let mut uvs = Vec::with_capacity(piece.vertices.len());
                    let mut tangents = Vec::with_capacity(piece.vertices.len());
                    for v in &piece.vertices {
                        positions.push(v.position);
                        normals.push(v.normal);
                        uvs.push(v.uv0.unwrap_or([0.0, 0.0]));
                        let n = Vec3::from_array(v.normal).normalize_or_zero();
                        let t = Vec3::from_array(v.tangent.unwrap_or([1.0, 0.0, 0.0]))
                            .normalize_or_zero();
                        let b = Vec3::from_array(v.binormal.unwrap_or([0.0, 1.0, 0.0]));
                        tangents.push([
                            t.x,
                            t.y,
                            t.z,
                            if n.cross(t).dot(b) < 0.0 { -1.0 } else { 1.0 },
                        ]);
                    }
                    for part in &piece.parts {
                        let mesh = Mesh::new(
                            bevy::render::render_resource::PrimitiveTopology::TriangleList,
                            bevy::asset::RenderAssetUsages::MAIN_WORLD
                                | bevy::asset::RenderAssetUsages::RENDER_WORLD,
                        )
                        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone())
                        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals.clone())
                        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs.clone())
                        .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, tangents.clone())
                        .with_inserted_indices(bevy::mesh::Indices::U32(
                            part.triangles.iter().flatten().copied().collect(),
                        ));
                        let material = part
                            .material
                            .and_then(|i| bound_materials.get(i).cloned())
                            .unwrap_or_else(|| character_grey.clone());
                        let handle = meshes.add(mesh);
                        if let Some(skin) = skin.as_mut() {
                            skin.handles.push(handle.clone());
                        }
                        parent.spawn((
                            Mesh3d(handle),
                            MeshMaterial3d(material),
                            bevy::camera::visibility::NoFrustumCulling,
                        ));
                    }
                    if let (Some(state), Some(skin)) = (state.as_mut(), skin) {
                        state.skins.push(skin);
                    }
                }
            });
    }
    if let Some(library) = library {
        loaded.scene.rig = Some(library.report);
    }
    if let Some(state) = state {
        loaded.scene.replay = Some(
            serde_json::json!({"scenario":options.scenario,"ticks":state.player.tick,
            "position":state.player.position.to_array(),"velocity":state.player.velocity.to_array(),"distance":state.player.distance,
            "yaw":state.player.yaw,"motion_trace":state.motion_trace,
            "bone_pose":state.matcher.pose.iter().enumerate().map(|(index,matrix)|serde_json::json!({
                "index":index,"name_hash":format!("{:08x}",state.matcher.rig.hashes[index]),
                "parent":state.matcher.rig.parents[index],"matrix_cols":matrix.to_cols_array()
            })).collect::<Vec<_>>(),
            "lighting":"Synthetic daylight/PBR, not native Day Paris","motion":state.motion_report,
            "selected_clip":state.matcher.takes[state.matcher.selected].file_id,"selected_time":state.matcher.time,"match_cost":state.matcher.cost}),
        );
        commands.insert_resource(state);
    }
    if let Some(path) = &options.report {
        let report = serde_json::to_vec_pretty(&loaded.scene).expect("serialize scene report");
        std::fs::write(path, report).expect("write scene report");
    }

    if let Some(radius) = options.map_stream {
        let path = options
            .map_cell_centres
            .clone()
            .unwrap_or_else(|| PathBuf::from("local/paris-cell-centres.json"));
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let rows: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap_or_default();
        if rows.is_empty() {
            eprintln!(
                "stream: {} missing or empty; generate it with --map-cells ... --map-dry-run --map-cell-centres",
                path.display()
            );
        }
        let centres: Vec<(usize, f32, f32)> = rows
            .iter()
            .filter_map(|row| {
                Some((
                    row.get("entry")?.as_u64()? as usize,
                    row.get("x")?.as_f64()? as f32,
                    row.get("y")?.as_f64()? as f32,
                ))
            })
            .collect();
        let runtime = load::MapRuntime::new(&options).expect("map runtime");
        let root = commands
            .spawn((
                Transform::from_rotation(native_z_up_to_bevy_y_up()),
                Visibility::default(),
            ))
            .id();
        let centres = match options.map_stream_cell {
            Some(entry) => vec![(entry, 0.0f32, 0.0f32)],
            None => centres,
        };
        let radius = if options.map_stream_cell.is_some() {
            f32::MAX
        } else {
            radius
        };
        eprintln!("stream: {} cell centres, radius {radius} m", centres.len());
        commands.insert_resource(Streaming {
            runtime,
            centres,
            radius,
            loaded: HashMap::new(),
            anchor: Vec3::splat(-1.0e9),
            root,
            handles: Vec::new(),
            textures: Vec::new(),
            materials: Vec::new(),
            parts: 0,
            parts_bound: 0,
            parts_textured: 0,
            untextured: std::collections::HashSet::new(),
            bounds: None,
            colliders: std::collections::HashMap::new(),
            last_position: Vec3::splat(-1.0e9),
            still_for: 0.0,
            last_load: 0.0,
        });
        commands.insert_resource(MapGrey(grey.clone()));
    }

    // Lighting: overcast sun plus a soft ambient fill so grey reads clearly.
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.70, 0.81, 1.0),
        brightness: 550.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(1.0, 0.91, 0.78),
            illuminance: 18_000.0,
            shadow_maps_enabled: !map_mode,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 0.6, -0.9, 0.0)),
    ));
    // Second dim light from the opposite side to avoid pure black backsides.
    commands.spawn((
        DirectionalLight {
            illuminance: 2_000.0,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -2.4, -0.4, 0.0)),
    ));

    // Camera framing the decoded bounds.
    let center = (bevy_min + bevy_max) / 2.0;
    let size = (bevy_max - bevy_min).length().max(1.0);
    let azimuth = options.orbit_deg.to_radians();
    // Multi-cell scenes include the river arms, which stretch the bounds; frame tighter.
    // A player uses a third-person follow camera; otherwise frame the scene.
    let distance = if options.map_player {
        // Six metres behind a walker puts the eye inside the nearest wall; the default keeps
        // the camera above head height and clear of most facades, and it is adjustable.
        options.map_camera_distance.unwrap_or(9.0)
    } else if map_mode && instances.len() > 1 {
        size * 1.1
    } else {
        size * 1.6
    };
    let elevation = options.orbit_elevation;
    let eye = center + Vec3::new(azimuth.sin(), elevation, azimuth.cos()).normalize() * distance;
    eprintln!(
        "camera: center {center:?} size {size:.1} distance {distance:.1} eye {eye:?} elevation {elevation}"
    );
    // A pinned camera makes orientation checks reproducible, independent of the follow rig.
    let (eye, look) = match (options.camera_at, options.look_at) {
        (Some(at), look) => {
            commands.insert_resource(FixedCamera);
            (
                Vec3::from_array(at),
                Vec3::from_array(look.unwrap_or(center.to_array())),
            )
        }
        _ => (eye, center),
    };
    let mut camera = commands.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::srgb(0.46, 0.61, 0.78)),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: 40.0_f32.to_radians(),
            // City-scale scenes need a far plane well past the default.
            near: 0.5,
            far: 100_000.0,
            ..default()
        }),
        Transform::from_translation(eye).looking_at(look, Vec3::Y),
        InspectionCamera {
            center,
            azimuth,
            elevation: elevation.atan(),
            distance,
            initial: distance,
            actual: distance,
            report: distance,
            report_desired: distance,
            allowed: distance,
            probe_after: 0.0,
        },
    ));
    if let Some(mut capture) = capture {
        let (width, height) = options.size;
        let image = Image::new_target_texture(
            width,
            height,
            TextureFormat::Rgba8UnormSrgb,
            Some(TextureFormat::Rgba8UnormSrgb),
        );
        let handle = images.add(image);
        capture.image = Some(handle.clone());
        camera.insert(RenderTarget::Image(handle.into()));
        println!("capture target: offscreen {width}x{height} Rgba8UnormSrgb");
    }
}

/// Unit vector from the player towards the camera for a given orbit angle.
fn boom_direction(azimuth: f32, elevation: f32) -> Vec3 {
    Vec3::new(
        azimuth.sin() * elevation.cos(),
        elevation.sin(),
        azimuth.cos() * elevation.cos(),
    )
}

/// Distance along `dir` at which the ray from `origin` enters the box, if it does.
fn ray_box(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let mut near = 0.0f32;
    let mut far = f32::INFINITY;
    for axis in 0..3 {
        let o = origin[axis];
        let d = dir[axis];
        if d.abs() < 1.0e-6 {
            if o < min[axis] || o > max[axis] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / d;
        let mut t0 = (min[axis] - o) * inv;
        let mut t1 = (max[axis] - o) * inv;
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        near = near.max(t0);
        far = far.min(t1);
        if near > far {
            return None;
        }
    }
    // A coarse AABB can enclose the player even though no surface is there; only a real
    // surface ahead blocks the boom.
    if far <= 0.0 || near <= 0.5 {
        return None;
    }
    Some(near)
}

/// Prints measured frame rate every two seconds while streaming is active, so performance
/// claims are numbers rather than impressions.
fn report_fps(
    time: Res<Time>,
    streams: Query<&Streaming>,
    cameras: Query<&InspectionCamera>,
    mut since: Local<f32>,
    mut frames: Local<u32>,
    mut worst_ms: Local<f32>,
    mut hitches: Local<u32>,
) {
    if streams.is_empty() {
        return;
    }
    *frames += 1;
    let frame_ms = time.delta_secs() * 1000.0;
    if frame_ms > *worst_ms {
        *worst_ms = frame_ms;
    }
    if frame_ms > 100.0 {
        *hitches += 1;
    }
    let now = time.elapsed_secs();
    if now - *since < 2.0 {
        return;
    }
    let span = now - *since;
    let boom = cameras
        .iter()
        .next()
        .map(|camera| (camera.report, camera.report_desired))
        .unwrap_or((0.0, 0.0));
    eprintln!(
        "fps: {:.1} over {span:.1} s, camera {:.1} m (target {:.1} m), worst frame {:.0} ms, {} hitches",
        *frames as f32 / span,
        boom.0,
        boom.1,
        *worst_ms,
        *hitches
    );
    *frames = 0;
    *worst_ms = 0.0;
    *hitches = 0;
    *since = now;
}

fn request_capture(mut commands: Commands, mut capture: ResMut<Capture>) {
    if capture.requested {
        return;
    }
    capture.rendered += 1;
    if capture.rendered < capture.frames {
        return;
    }
    let Some(image) = capture.image.clone() else {
        eprintln!("capture: offscreen target missing (internal error)");
        std::process::exit(4);
    };
    capture.requested = true;
    println!(
        "capture: saving {} after {} frames",
        capture.path.display(),
        capture.rendered
    );
    commands
        .spawn(Screenshot::image(image))
        .observe(save_to_disk(capture.path.clone()))
        .observe(finish_after_save);
}

fn finish_after_save(_screenshot: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>) {
    println!("capture: saved, exiting");
    exit.write(AppExit::Success);
}

fn capture_timeout(mut capture: ResMut<Capture>, mut exit: MessageWriter<AppExit>) {
    if !capture.requested {
        return;
    }
    capture.waited += 1;
    if capture.waited > CAPTURE_DEADLINE_FRAMES {
        eprintln!("capture: timed out waiting for the screenshot");
        exit.write(AppExit::error());
    }
}

#[cfg(test)]
mod controller_camera_tests {
    use super::*;
    #[test]
    fn right_stick_changes_both_camera_axes_without_move_input() {
        let mut app = App::new();
        let mut time = Time::<()>::default();
        time.advance_by(Duration::from_millis(100));
        app.insert_resource(time)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<controls::Controller>()
            .add_message::<AppExit>()
            .add_systems(Update, inspect_camera);
        app.world_mut().resource_mut::<controls::Controller>().look = Vec2::ONE;
        let entity = app
            .world_mut()
            .spawn((
                Transform::default(),
                InspectionCamera {
                    center: Vec3::ZERO,
                    azimuth: 0.0,
                    elevation: 0.5,
                    distance: 5.0,
                    actual: 5.0,
                    report: 5.0,
                    report_desired: 5.0,
                    allowed: 5.0,
                    probe_after: 0.0,
                },
            ))
            .id();
        app.update();
        let orbit = app.world().get::<InspectionCamera>(entity).unwrap();
        assert!((orbit.azimuth + 0.24).abs() < 1e-5);
        assert!((orbit.elevation - 0.32).abs() < 1e-5);
        let transform = app.world().get::<Transform>(entity).unwrap();
        assert!(transform.translation.x < 0.0);
        assert!((transform.translation.length() - 5.0).abs() < 1e-4);
    }
}
mod motion;
