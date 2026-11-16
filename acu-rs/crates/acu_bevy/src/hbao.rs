//! ACU's max-quality ambient occlusion: NVIDIA HBAO+ with the parameters ACU passes to
//! `GFSDK_SSAO.win64.dll` (`acu_assets::hbao`, finding ACU-RE-0105).
//!
//! Bevy's `ScreenSpaceAmbientOcclusion` stays on the camera for its texture and the
//! `diffuse_occlusion` binding every `StandardMaterial` extension reads; this pass runs before
//! the main opaque pass and overwrites that texture (`--ao bevy` keeps Bevy's GTAO instead,
//! `--ao off` removes AO). The kernel is in `shaders/hbao.wgsl`.
//!
//! Adapted (reviewed import, no path dependency) from For Honor's port,
//! `ForHonorRustRewrite/fh-rs/crates/fh_bevy/src/fh_hbao.rs` at commit `546a3776`: the same
//! Bevy 0.19.1 render-graph wiring, three compute passes (AO, blur x, blur y) and the uniform
//! layout, with ACU's constants and blur radius.

use acu_assets::hbao::HbaoParams;
use bevy::asset::{AssetServer, Handle, load_embedded_asset};
use bevy::core_pipeline::core_3d::main_opaque_pass_3d;
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::core_pipeline::schedule::{Core3d, Core3dSystems};
use bevy::ecs::prelude::*;
use bevy::pbr::ScreenSpaceAmbientOcclusionResources;
use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_resource::binding_types::{
    texture_2d, texture_depth_2d, texture_storage_2d, uniform_buffer_sized,
};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderAdapter, RenderContext, ViewQuery};
use bevy::render::texture::TextureCache;
use bevy::render::view::ExtractedView;
use bevy::render::{RenderApp, RenderStartup};
use bevy::shader::{Shader, ShaderDefVal};

/// Camera component: run ACU's HBAO+ pass with these parameters.
#[derive(Component, Clone, Copy, ExtractComponent)]
pub struct AcuHbao(pub HbaoParams);

/// Which AO the viewer renders (`--ao`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AoMode {
    /// ACU's HBAO+ (max settings; the default).
    Hbao,
    /// Bevy's own GTAO (negative control).
    Bevy,
    Off,
}

impl AoMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "hbao" => Some(Self::Hbao),
            "bevy" => Some(Self::Bevy),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
}

pub struct AcuHbaoPlugin;

impl Plugin for AcuHbaoPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "shaders/hbao.wgsl");
        app.add_plugins(ExtractComponentPlugin::<AcuHbao>::default());
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .add_systems(RenderStartup, init_pipelines)
            .add_systems(
                Core3d,
                acu_hbao
                    .before(main_opaque_pass_3d)
                    .in_set(Core3dSystems::MainPass),
            );
    }
}

#[derive(Resource)]
struct Pipelines {
    layout: BindGroupLayoutDescriptor,
    hbao: CachedComputePipelineId,
    blur_x: CachedComputePipelineId,
    blur_y: CachedComputePipelineId,
}

fn init_pipelines(
    mut commands: Commands,
    cache: Res<PipelineCache>,
    adapter: Res<RenderAdapter>,
    assets: Res<AssetServer>,
) {
    // Bevy's SSAO texture format choice (`bevy_pbr` ssao `SsaoPipelines::from_world`).
    let r16 = adapter
        .get_texture_format_features(TextureFormat::R16Float)
        .allowed_usages
        .contains(TextureUsages::STORAGE_BINDING);
    let final_format = if r16 {
        TextureFormat::R16Float
    } else {
        TextureFormat::R32Float
    };
    let layout = BindGroupLayoutDescriptor::new(
        "acu_hbao_layout",
        &BindGroupLayoutEntries::with_indices(
            ShaderStages::COMPUTE,
            (
                (0, uniform_buffer_sized(false, None)),
                (1, texture_depth_2d()),
                (
                    2,
                    texture_2d(TextureSampleType::Float { filterable: false }),
                ),
                (
                    3,
                    texture_storage_2d(TextureFormat::Rg32Float, StorageTextureAccess::WriteOnly),
                ),
                (
                    4,
                    texture_storage_2d(final_format, StorageTextureAccess::WriteOnly),
                ),
            ),
        ),
    );
    let shader: Handle<Shader> = load_embedded_asset!(assets.as_ref(), "shaders/hbao.wgsl");
    let defs: Vec<ShaderDefVal> = if r16 {
        vec!["USE_R16FLOAT".into()]
    } else {
        vec![]
    };
    let queue = |entry: &'static str| {
        cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some(format!("acu_hbao_{entry}").into()),
            layout: vec![layout.clone()],
            shader: shader.clone(),
            shader_defs: defs.clone(),
            entry_point: Some(entry.into()),
            ..default()
        })
    };
    commands.insert_resource(Pipelines {
        hbao: queue("hbao"),
        blur_x: queue("blur_x"),
        blur_y: queue("blur_y"),
        layout,
    });
}

/// The `Params` uniform of `hbao.wgsl`. `MetersToViewSpaceUnits` is 1: ACU's InputData reads
/// 1.0 (INFERRED field) and the viewer keeps native units as metres.
fn uniform(h: &HbaoParams, view: &ExtractedView, size: UVec2) -> Vec<u8> {
    // clip_from_view.y_axis.y = 1 / tan(fovy / 2).
    let c = h.constants(1.0 / view.clip_from_view.y_axis.y, size.y as f32, 1.0);
    let blur_radius = h.blur.map_or(0.0, |b| b.radius as f32);
    let mut f: Vec<f32> = view.clip_from_view.inverse().to_cols_array().to_vec();
    f.extend([
        size.x as f32,
        size.y as f32,
        c.radius_to_screen,
        c.neg_inv_r2,
    ]);
    f.extend([c.ndotv_bias, c.detail_amount, c.coarse_amount, c.power]);
    f.extend([c.blur_sharpness, c.blur_falloff, blur_radius, 0.0]);
    f.iter().flat_map(|v| v.to_le_bytes()).collect()
}

#[allow(clippy::too_many_arguments)]
fn acu_hbao(
    view: ViewQuery<(
        &ExtractedCamera,
        &ExtractedView,
        &ViewPrepassTextures,
        &ScreenSpaceAmbientOcclusionResources,
        &AcuHbao,
    )>,
    pipelines: Option<Res<Pipelines>>,
    cache: Res<PipelineCache>,
    mut textures: ResMut<TextureCache>,
    mut ctx: RenderContext,
) {
    let (camera, extracted, prepass, ssao, hbao) = view.into_inner();
    let (Some(p), Some(size), Some(depth)) = (
        pipelines,
        camera.physical_viewport_size,
        prepass.depth_view(),
    ) else {
        return;
    };
    let (Some(pass_ao), Some(pass_x), Some(pass_y)) = (
        cache.get_compute_pipeline(p.hbao),
        cache.get_compute_pipeline(p.blur_x),
        cache.get_compute_pipeline(p.blur_y),
    ) else {
        return;
    };
    let device = ctx.render_device().clone();
    let scratch = |textures: &mut TextureCache, label: &'static str| {
        textures.get(
            &device,
            TextureDescriptor {
                label: Some(label),
                size: Extent3d {
                    width: size.x,
                    height: size.y,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rg32Float,
                usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        )
    };
    let a = scratch(&mut textures, "acu_hbao_a");
    let b = scratch(&mut textures, "acu_hbao_b");
    let params = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("acu_hbao_params"),
        contents: &uniform(&hbao.0, extracted, size),
        usage: BufferUsages::UNIFORM,
    });
    let out = &ssao.screen_space_ambient_occlusion_texture.default_view;
    let layout = cache.get_bind_group_layout(&p.layout);
    // (src, ao_out) per pass: hbao writes A, blur_x reads A into B, blur_y reads B into `out`.
    let groups = [(&b, &a), (&a, &b), (&b, &a)].map(|(src, dst)| {
        device.create_bind_group(
            "acu_hbao",
            &layout,
            &BindGroupEntries::with_indices((
                (0, params.as_entire_binding()),
                (1, depth),
                (2, &src.default_view),
                (3, &dst.default_view),
                (4, out),
            )),
        )
    });
    let encoder = ctx.command_encoder();
    encoder.push_debug_group("acu_hbao");
    for (pipeline, group) in [pass_ao, pass_x, pass_y].into_iter().zip(&groups) {
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("acu_hbao"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.dispatch_workgroups(size.x.div_ceil(8), size.y.div_ceil(8), 1);
    }
    encoder.pop_debug_group();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ao_modes_parse() {
        assert_eq!(AoMode::parse("hbao"), Some(AoMode::Hbao));
        assert_eq!(AoMode::parse("bevy"), Some(AoMode::Bevy));
        assert_eq!(AoMode::parse("off"), Some(AoMode::Off));
        assert_eq!(AoMode::parse("gtao"), None);
    }
}
