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

mod load;
mod options;

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
}

fn inspect_camera(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut cameras: Query<(&mut Transform, &mut InspectionCamera)>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
    let dt = time.delta_secs().min(0.1);
    let axis = |positive, negative| {
        f32::from(u8::from(keys.pressed(positive))) - f32::from(u8::from(keys.pressed(negative)))
    };
    for (mut transform, mut orbit) in &mut cameras {
        orbit.azimuth += axis(KeyCode::ArrowRight, KeyCode::ArrowLeft) * dt;
        orbit.elevation =
            (orbit.elevation + axis(KeyCode::ArrowUp, KeyCode::ArrowDown) * dt).clamp(-0.1, 1.4);
        orbit.distance =
            (orbit.distance * (axis(KeyCode::KeyE, KeyCode::KeyQ) * dt).exp()).clamp(0.4, 20.0);
        let direction = Vec3::new(
            orbit.azimuth.sin() * orbit.elevation.cos(),
            orbit.elevation.sin(),
            orbit.azimuth.cos() * orbit.elevation.cos(),
        );
        *transform = Transform::from_translation(orbit.center + direction * orbit.distance)
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
                    title: "ACU synthetic test world (data read from the original install)".into(),
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
    app.add_plugins(plugins)
        .insert_resource(options)
        .add_systems(Startup, setup_world);
    if !headless {
        app.add_systems(Update, inspect_camera);
        println!(
            "Arrow keys: orbit; Q/E: zoom in/out; Esc: exit. Synthetic static inspection scene."
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
        .add_systems(Update, (request_capture, capture_timeout));
    }
    app.run();
}

fn setup_world(
    mut commands: Commands,
    options: Res<Options>,
    capture: Option<ResMut<Capture>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    println!("install dir: {}", options.game_dir.display());
    let loaded = load::load_all(&options).unwrap_or_else(|e| {
        eprintln!("asset load failed: {e}");
        std::process::exit(3);
    });
    if let Some(path) = &options.report {
        let report = serde_json::to_vec_pretty(&loaded.scene).expect("serialize scene report");
        std::fs::write(path, report).expect("write scene report");
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
            let format = match texture.dxgi_format {
                71 | 72 => TextureFormat::Bc1RgbaUnormSrgb,
                74 | 75 => TextureFormat::Bc2RgbaUnormSrgb,
                77 | 78 => TextureFormat::Bc3RgbaUnormSrgb,
                98 | 99 => TextureFormat::Bc7RgbaUnormSrgb,
                28 | 29 => TextureFormat::Rgba8UnormSrgb,
                87 | 91 => TextureFormat::Bgra8UnormSrgb,
                other => panic!(
                    "unsupported diffuse DXGI format {other} for {}",
                    texture.file_id
                ),
            };
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
            materials.add(StandardMaterial {
                base_color: Color::WHITE,
                base_color_texture: texture,
                alpha_mode: if material.alpha_test_enabled {
                    AlphaMode::Mask(f32::from(material.alpha_test_value) / 255.0)
                } else {
                    AlphaMode::Opaque
                },
                perceptual_roughness: 0.85,
                cull_mode: None,
                ..default()
            })
        })
        .collect();
    let pieces = loaded.pieces;

    let mut bevy_min = Vec3::splat(f32::INFINITY);
    let mut bevy_max = Vec3::splat(f32::NEG_INFINITY);
    let mut native_min = [f32::INFINITY; 3];
    let mut native_max = [f32::NEG_INFINITY; 3];
    for piece in &pieces {
        let (min, max) = piece.bounds();
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
    let ground_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.38, 0.38, 0.40),
        perceptual_roughness: 0.95,
        ..default()
    });

    // Synthetic arena: 24x24 grey ground plus block references at known sizes.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(24.0, 24.0))),
        MeshMaterial3d(ground_material),
    ));
    for (x, z, size) in [
        (-4.2f32, -3.0f32, 0.8f32),
        (4.4, 2.6, 1.2),
        (0.0, -6.0, 0.4),
        (6.0, -4.2, 0.6),
    ] {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size, size, size))),
            MeshMaterial3d(grey.clone()),
            Transform::from_xyz(x, size / 2.0, z),
        ));
    }

    // Arno pieces as children of one native-space root.
    commands
        .spawn((
            Transform::from_rotation(native_z_up_to_bevy_y_up()),
            Visibility::default(),
        ))
        .with_children(|parent| {
            for piece in &pieces {
                let mut positions = Vec::with_capacity(piece.vertices.len());
                let mut normals = Vec::with_capacity(piece.vertices.len());
                let mut uvs = Vec::with_capacity(piece.vertices.len());
                for v in &piece.vertices {
                    positions.push(v.position);
                    normals.push(v.normal);
                    uvs.push(v.uv0.unwrap_or([0.0, 0.0]));
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
                    .with_inserted_indices(bevy::mesh::Indices::U32(
                        part.triangles.iter().flatten().copied().collect(),
                    ));
                    let material = part
                        .material
                        .map(|i| bound_materials[i].clone())
                        .unwrap_or_else(|| grey.clone());
                    parent.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material)));
                }
            }
        });

    // Lighting: overcast sun plus a soft ambient fill so grey reads clearly.
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 300.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            illuminance: light_consts::lux::OVERCAST_DAY,
            shadow_maps_enabled: true,
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
    let distance = size * 1.6;
    let eye = center + Vec3::new(azimuth.sin(), 0.28, azimuth.cos()).normalize() * distance;
    let mut camera = commands.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::srgb(0.11, 0.12, 0.14)),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: 40.0_f32.to_radians(),
            ..default()
        }),
        Transform::from_translation(eye).looking_at(center, Vec3::Y),
        InspectionCamera {
            center,
            azimuth,
            elevation: 0.28_f32.atan(),
            distance,
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
