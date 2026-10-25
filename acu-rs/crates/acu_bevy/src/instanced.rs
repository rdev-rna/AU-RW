//! One instanced draw entity per unique (piece, part) instead of one entity per
//! placement.
//!
//! The original engine draws one entity = one world matrix x one mesh, and the
//! per-placement spawn path in `main.rs` mirrors that with one Bevy entity per
//! placement: a whole-map run produced 1,064,087 entities for 486,454
//! placements and exhausted an 8 GB GPU. Halving the placements only cut
//! triangles by 6.3%, so the cost is per entity, not per triangle. Here each
//! unique part is drawn once with one instance per placement, following Bevy's
//! `shader_advanced/custom_shader_instancing` example (instance buffer in vertex
//! slot 1, `draw_indexed`, `NoIndirectDrawing` on the camera).
//!
//! Each (piece, part) entity carries the same material inputs the per-placement
//! path gives `map_material` - diffuse map, normal map and alpha cut-out - bound
//! through an `AsBindGroup` material at Bevy's material bind group index.
//!
//! Like the native `ClusterCullingNoOcclusion` pass, the placements of a part
//! are split into spatial clusters of [`CLUSTER_SIZE`] metres and each cluster
//! is one draw entity with an explicit [`Aabb`]. The phase test in
//! [`queue_map_instanced`] then rejects whole clusters against the view frustum
//! instead of submitting every instance in the city for every view.

use std::collections::HashMap;
use std::mem::size_of;
use std::time::{Duration, Instant};

use bevy::{
    camera::primitives::Aabb,
    core_pipeline::core_3d::{Transparent3d, TransparentSortingInfo3d},
    ecs::{
        query::QueryItem,
        system::{
            lifetimeless::{Read, SRes},
            StaticSystemParam, SystemParamItem,
        },
    },
    mesh::{MeshVertexBufferLayoutRef, VertexBufferLayout},
    pbr::{
        self, MeshInputUniform, MeshPipeline, MeshPipelineKey, MeshPipelineSystems, MeshUniform,
        RenderMeshInstances, SetMeshBindGroup, SetMeshViewBindingArrayBindGroup,
        SetMeshViewBindGroup, ViewKeyCache, MATERIAL_BIND_GROUP_INDEX,
    },
    prelude::*,
    render::{
        batching::gpu_preprocessing::BatchedInstanceBuffers,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        mesh::{allocator::MeshAllocator, RenderMesh, RenderMeshBufferInfo},
        render_asset::RenderAssets,
        render_phase::{
            AddRenderCommand, DrawFunctions, PhaseItem, PhaseItemExtraIndex, RenderCommand,
            RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewSortedRenderPhases,
        },
        render_resource::*,
        renderer::{RenderDevice, RenderQueue},
        sync_component::SyncComponent,
        sync_world::MainEntity,
        view::ExtractedView,
        Render, RenderApp, RenderStartup, RenderSystems,
    },
};

use bevy::shader::ShaderDefVal;

use crate::load;
use crate::texture_stream::TextureGeneration;

/// Embedded shader for the instanced map path.
const SHADER_ASSET_PATH: &str = "embedded://acu_bevy/shaders/map_instanced.wgsl";

/// Illuminance the light uniform is normalised by. Every `colour * illuminance`
/// and the ambient `colour * brightness` are divided by this, so the shader sees
/// O(1) irradiances and keeps a single readable calibration constant. It is the
/// key `DirectionalLight` irradiance from `main.rs` (18 000 lux) — a unit, not a
/// tint or a clamp.
const LIGHT_UNIT: f32 = 18_000.0;

/// Directional lights the uniform carries. `main.rs` spawns two; the cap keeps
/// the uniform a plain array instead of a storage buffer.
const MAX_MAP_LIGHTS: usize = 4;

/// Bind group holding [`MapLights`], next to the per-part group at
/// `MATERIAL_BIND_GROUP_INDEX` (3), which the mesh pipeline fills 0..=2 and the
/// material adds at 3.
pub const MAP_LIGHT_BIND_GROUP_INDEX: usize = MATERIAL_BIND_GROUP_INDEX + 1;

/// The scene's real lights, flattened for the instanced map shader.
///
/// Filled from `GlobalAmbientLight` and every `DirectionalLight` each frame in
/// `Update`, then mirrored into the render world by `ExtractResourcePlugin`,
/// exactly like [`TextureGeneration`]. Nothing here is hardcoded.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, ExtractResource)]
pub struct MapLights {
    /// `rgb` = ambient colour (linear) * brightness / [`LIGHT_UNIT`],
    /// `a` = number of live entries in the two arrays below.
    pub ambient: [f32; 4],
    /// `xyz` = world-space unit vector pointing *towards* the light, the vector
    /// the shader dots with the surface normal.
    pub direction: [[f32; 4]; MAX_MAP_LIGHTS],
    /// `rgb` = light colour (linear) * illuminance / [`LIGHT_UNIT`].
    pub colour: [[f32; 4]; MAX_MAP_LIGHTS],
}

impl MapLights {
    /// Bytes for the uniform buffer: 9 `vec4<f32>`, matching `MapLights` in
    /// `map_instanced.wgsl`.
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity((1 + 2 * MAX_MAP_LIGHTS) * 16);
        for vector in std::iter::once(&self.ambient)
            .chain(self.direction.iter())
            .chain(self.colour.iter())
        {
            for value in *vector {
                bytes.extend_from_slice(&value.to_ne_bytes());
            }
        }
        bytes
    }
}

/// Reads the scene's real lights into [`MapLights`] every frame, so a light that
/// a capture script moves or recolours is picked up on the next frame.
fn update_map_lights(
    ambient: Option<Res<bevy::light::GlobalAmbientLight>>,
    lights: Query<(
        &bevy::prelude::DirectionalLight,
        &bevy::prelude::GlobalTransform,
    )>,
    mut map_lights: ResMut<MapLights>,
) {
    let mut next = MapLights::default();
    if let Some(ambient) = ambient {
        let colour = ambient.color.to_linear();
        next.ambient = [
            colour.red * ambient.brightness / LIGHT_UNIT,
            colour.green * ambient.brightness / LIGHT_UNIT,
            colour.blue * ambient.brightness / LIGHT_UNIT,
            0.0,
        ];
    }
    let mut count = 0usize;
    for (light, transform) in &lights {
        if count == MAX_MAP_LIGHTS {
            break;
        }
        // A `DirectionalLight` shines down its local -Z, so the vector towards
        // it is the light's world +Z.
        let towards = (transform.rotation() * Vec3::Z).normalize_or_zero();
        let colour = light.color.to_linear();
        next.direction[count] = [towards.x, towards.y, towards.z, 0.0];
        next.colour[count] = [
            colour.red * light.illuminance / LIGHT_UNIT,
            colour.green * light.illuminance / LIGHT_UNIT,
            colour.blue * light.illuminance / LIGHT_UNIT,
            0.0,
        ];
        count += 1;
    }
    next.ambient[3] = count as f32;
    if *map_lights != next {
        *map_lights = next;
    }
}

/// The uniform buffer and bind group carrying [`MapLights`] for every draw.
#[derive(Resource)]
struct MapLightBindings {
    buffer: Buffer,
    bind_group: BindGroup,
    /// Last values written, so a static scene does not rewrite the buffer.
    uploaded: Option<MapLights>,
}

fn map_light_bind_group_layout() -> BindGroupLayoutDescriptor {
    BindGroupLayoutDescriptor::new(
        "map lights bind group layout",
        &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    )
}

/// Uploads [`MapLights`] into the render-world uniform buffer.
fn prepare_map_lights(
    map_lights: Option<Res<MapLights>>,
    bindings: Option<ResMut<MapLightBindings>>,
    render_queue: Res<RenderQueue>,
) {
    let (Some(map_lights), Some(mut bindings)) = (map_lights, bindings) else {
        return;
    };
    if bindings.uploaded.as_ref() == Some(&*map_lights) {
        return;
    }
    render_queue.write_buffer(&bindings.buffer, 0, &map_lights.to_bytes());
    bindings.uploaded = Some(*map_lights);
}

/// One placement: its world matrix, already baked into Bevy space.
///
/// 64 bytes, all `f32`, no padding, so the buffer can be written column by
/// column without a `Pod` derive (`bytemuck` is not a dependency of this crate).
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct InstanceData {
    /// Column-major 4x4, the same layout `MapInstance::matrix` arrives in.
    pub model: [f32; 16],
}

/// Every placement of one (piece, part) cluster, for the renderer to draw
/// instanced.
///
/// The [`Aabb`] is the world-space union of the instance bounds in this
/// cluster. It travels with the instance list rather than as a separate
/// component because the streaming path replaces the list wholesale; one
/// extracted value can then never describe the previous frame's placements.
#[derive(Component, Deref)]
pub struct InstanceMaterialData(#[deref] pub Vec<InstanceData>, pub Aabb);

impl SyncComponent for InstanceMaterialData {
    type Target = Self;
}

impl ExtractComponent for InstanceMaterialData {
    type QueryData = &'static InstanceMaterialData;
    type QueryFilter = ();
    type Out = Self;

    fn extract_component(item: QueryItem<'_, '_, Self::QueryData>) -> Option<Self> {
        Some(InstanceMaterialData(item.0.clone(), item.1))
    }
}

/// Edge of one culling cluster, in native metres of the horizontal plane.
///
/// Native space is Z-up, so `x`/`y` are the ground plane and clusters are
/// columns of this width. The native renderer culls clusters rather than
/// individual instances (`ClusterCullingNoOcclusion`); a few hundred metres
/// keeps the per-cluster [`Aabb`] tight enough for frustum rejection to bite
/// while leaving few enough clusters that the per-frame cost stays linear in
/// clusters, not placements.
pub const CLUSTER_SIZE: f32 = 200.0;

/// Grid cell of one cluster: native `(x, y)` divided by [`CLUSTER_SIZE`].
pub type ClusterKey = (i32, i32);

/// The cluster a placement belongs to, from its native instance matrix.
pub fn cluster_key(native: &[f32; 16]) -> ClusterKey {
    (
        (native[12] / CLUSTER_SIZE).floor() as i32,
        (native[13] / CLUSTER_SIZE).floor() as i32,
    )
}

/// World-space [`Aabb`] covering every instance of a cluster.
///
/// The piece's eight local bounds corners are transformed by each instance
/// matrix and unioned; the mesh's own AABB describes a single instance at the
/// origin, which would cull a whole street block from its first placement.
/// Empty local bounds (a piece with no vertices) give a box that can never be
/// culled - it draws nothing, so that costs nothing either.
pub fn cluster_aabb(local_min: Vec3, local_max: Vec3, instances: &[InstanceData]) -> Aabb {
    if instances.is_empty() || !local_min.is_finite() || !local_max.is_finite() {
        return Aabb::from_min_max(Vec3::splat(-1.0e7), Vec3::splat(1.0e7));
    }
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for instance in instances {
        let model = Mat4::from_cols_array(&instance.model);
        for corner in 0..8 {
            let local = Vec3::new(
                if corner & 1 == 0 { local_min.x } else { local_max.x },
                if corner & 2 == 0 { local_min.y } else { local_max.y },
                if corner & 4 == 0 { local_min.z } else { local_max.z },
            );
            let world = model.transform_point3(local);
            min = min.min(world);
            max = max.max(world);
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return Aabb::from_min_max(Vec3::splat(-1.0e7), Vec3::splat(1.0e7));
    }
    Aabb::from_min_max(min, max)
}

/// How many cluster draw entities the instanced map currently has, mirrored to
/// the render world so the `cull:` line can report `submitted` against the
/// whole set rather than guessing from the view.
#[derive(Resource, Clone, Copy, Debug, Default, ExtractResource)]
pub struct MapClusterStats {
    /// Unique (piece, part) pairs that have at least one placement.
    pub parts: usize,
    /// Draw entities, i.e. one per (piece, part, cluster).
    pub clusters: usize,
}

/// Material inputs for one (piece, part), matching what `map_material` feeds
/// `CharacterMaterial` on the per-placement path.
///
/// This is not a Bevy `Material` asset: the instanced path owns its pipeline
/// instead of going through `MaterialPlugin`, so the parts are per-entity
/// bind groups built from this component with the `AsBindGroup` derive.
#[derive(Component, Clone, Debug, Default, AsBindGroup)]
pub struct MapPartMaterial {
    /// Diffuse map. `None` binds Bevy's fallback image, i.e. white, which is
    /// what `map_material` renders for a material with no uploaded diffuse
    /// (`base_color: Color::WHITE`).
    #[texture(0)]
    #[sampler(1)]
    pub base_color_texture: Option<Handle<Image>>,
    /// Normal map: bound for the next step, not sampled yet.
    #[texture(2)]
    #[sampler(3)]
    pub normal_texture: Option<Handle<Image>>,
    /// `x` = alpha cutoff (`alpha_test_value / 255.0`, 0.0 when the material is
    /// opaque), `y` = 1.0 when a normal map is bound.
    #[uniform(4)]
    pub params: Vec4,
}

impl SyncComponent for MapPartMaterial {
    type Target = Self;
}

impl ExtractComponent for MapPartMaterial {
    type QueryData = &'static MapPartMaterial;
    type QueryFilter = ();
    type Out = Self;

    fn extract_component(item: QueryItem<'_, '_, Self::QueryData>) -> Option<Self> {
        Some(item.clone())
    }
}

/// Bind group built from a [`MapPartMaterial`], for the draw command to bind.
#[derive(Component)]
struct PreparedMapPart {
    bind_group: BindGroup,
    /// [`TextureGeneration`] this bind group was built against. A later upload
    /// bumps the counter and forces this part to be rebuilt against the real
    /// `Image`; the placeholder view must not be kept.
    generation: u32,
    /// Keeps the views, sampler and uniform buffer alive with the bind group.
    _bindings: BindingResources,
}

/// The map's instanced material: one shader for every part of the scene.
///
/// This stands in for a `Material` impl because the instanced path does not go
/// through `MaterialPlugin`; it owns the specialised `MeshPipeline` instead
/// (the example's `CustomPipeline`). Per-part data lives in the entity's
/// [`MapPartMaterial`] bind group, added to the pipeline layout at
/// `MATERIAL_BIND_GROUP_INDEX`, exactly like `MaterialPipelineSpecializer`.
#[derive(Resource)]
pub struct MapInstancedMaterial {
    shader: Handle<Shader>,
    mesh_pipeline: MeshPipeline,
    /// The per-part bind group layout descriptor, added to the pipeline layout
    /// exactly like `MaterialPipelineSpecializer` adds a material's.
    part_layout: BindGroupLayoutDescriptor,
    /// The scene lights layout, appended at [`MAP_LIGHT_BIND_GROUP_INDEX`].
    light_layout: BindGroupLayoutDescriptor,
}

/// What one [`spawn_map_instanced`] call produced, for the run summary.
#[derive(Debug, Clone, Copy)]
pub struct InstancedSpawned {
    /// Draw entities: one per (piece, part, cluster) with at least one placement.
    pub entities: usize,
    /// Placements accepted (after the radius test).
    pub placements: usize,
    /// Unique (piece, part) entries in the scene that have placements.
    pub parts: usize,
    /// Placement x part pairs, i.e. the number of rendered instances.
    pub instances: usize,
    /// Spatial clusters, one draw entity each. Equal to `entities`.
    pub clusters: usize,
}

/// Spawn the map as one entity per (unique part, spatial cluster).
///
/// `root` is the Z-up -> Y-up presentation rotation; it is baked into each
/// instance matrix, so these entities are spawned at the scene root rather than
/// under a rotated parent.
pub fn spawn_map_instanced(
    commands: &mut Commands,
    pieces: &[load::LoadedPiece],
    instances: &[load::MapInstance],
    part_materials: &[MapPartMaterial],
    mesh_assets: &mut Assets<Mesh>,
    root: Mat4,
    radius: Option<f32>,
    origin: Option<Vec3>,
) -> InstancedSpawned {
    // One mesh asset per unique piece part, shared by every placement.
    let part_meshes: Vec<Vec<Handle<Mesh>>> = pieces
        .iter()
        .map(|piece| {
            piece
                .parts
                .iter()
                .map(|part| mesh_assets.add(crate::part_mesh(piece, part)))
                .collect()
        })
        .collect();

    // Local bounds per piece, transformed by every instance matrix in a cluster
    // to give that cluster's world `Aabb`.
    let piece_bounds: Vec<(Vec3, Vec3)> = pieces
        .iter()
        .map(|piece| {
            let (min, max) = piece.bounds();
            (Vec3::from_array(min), Vec3::from_array(max))
        })
        .collect();

    let mut buckets: Vec<Vec<HashMap<ClusterKey, Vec<InstanceData>>>> = pieces
        .iter()
        .map(|piece| piece.parts.iter().map(|_| HashMap::new()).collect())
        .collect();

    let mut placements = 0usize;
    for instance in instances {
        if let (Some(radius), Some(origin)) = (radius, origin) {
            // Native (x, y, z) -> Bevy (x, z, -y) for the distance test, the
            // same convention the per-placement path uses.
            let at = Vec3::new(instance.matrix[12], instance.matrix[14], -instance.matrix[13]);
            if at.distance(origin) > radius {
                continue;
            }
        }
        placements += 1;
        let model = root * Mat4::from_cols_array(&instance.matrix);
        let data = InstanceData {
            model: model.to_cols_array(),
        };
        let key = cluster_key(&instance.matrix);
        for bucket in &mut buckets[instance.piece] {
            bucket.entry(key).or_default().push(data);
        }
    }

    let mut entities = 0usize;
    let mut parts = 0usize;
    let mut rendered_instances = 0usize;
    for (piece, part_buckets) in buckets.iter().enumerate() {
        let (local_min, local_max) = piece_bounds[piece];
        for (part, clusters) in part_buckets.iter().enumerate() {
            if clusters.is_empty() {
                continue;
            }
            parts += 1;
            // `part.material` indexes the scene material table, the same table
            // `bound_materials` is built from; a part with none keeps the
            // white-fallback default, like an untextured `map_material`.
            let material = pieces[piece].parts[part]
                .material
                .and_then(|index| part_materials.get(index).cloned())
                .unwrap_or_default();
            // Sorted keys keep the spawn order (and so the transparent phase's
            // tie-break) deterministic between runs.
            let mut keys: Vec<ClusterKey> = clusters.keys().copied().collect();
            keys.sort_unstable();
            for key in keys {
                let data = &clusters[&key];
                let aabb = cluster_aabb(local_min, local_max, data);
                entities += 1;
                rendered_instances += data.len();
                commands.spawn((
                    Mesh3d(part_meshes[piece][part].clone()),
                    material.clone(),
                    InstanceMaterialData(data.clone(), aabb),
                    // The instance matrices are not part of the mesh's `Aabb`,
                    // which describes one instance at the origin, so the cluster
                    // carries its own; the phase test below reads the same box.
                    aabb,
                    Transform::default(),
                    Visibility::default(),
                ));
            }
        }
    }

    InstancedSpawned {
        entities,
        placements,
        parts,
        instances: rendered_instances,
        clusters: entities,
    }
}

/// Marks a draw entity whose instance list changed and whose buffer must be
/// re-uploaded. The preload path never sets it (its instance lists are fixed);
/// the streaming path sets it whenever a cell changes the part's placements.
#[derive(Component)]
pub struct InstancesDirty;

/// Instance buffer uploaded for one draw entity, in vertex slot 1.
#[derive(Component)]
struct InstanceBuffer {
    buffer: Buffer,
    length: usize,
}

/// Registers the instanced map draw: extraction, buffer preparation, pipeline
/// specialisation and the phase item for every view.
pub struct MapInstancedPlugin;

impl Plugin for MapInstancedPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractComponentPlugin::<InstanceMaterialData>::default());
        app.add_plugins(ExtractComponentPlugin::<MapPartMaterial>::default());
        // The streamer's upload counter, mirrored into the render world so
        // `prepare_map_part_bind_groups` can see that a placeholder went stale.
        app.init_resource::<crate::texture_stream::TextureGeneration>();
        app.add_plugins(ExtractResourcePlugin::<crate::texture_stream::TextureGeneration>::default());
        // Cluster counts, so the render world can report submitted vs clusters.
        app.init_resource::<MapClusterStats>();
        app.add_plugins(ExtractResourcePlugin::<MapClusterStats>::default());
        // The scene's real lights: read from the ECS each frame, then mirrored
        // into the render world exactly like the texture generation counter.
        app.init_resource::<MapLights>();
        app.add_systems(bevy::app::Update, update_map_lights);
        app.add_plugins(ExtractResourcePlugin::<MapLights>::default());
        app.sub_app_mut(RenderApp)
            .add_render_command::<Transparent3d, DrawMapInstanced>()
            .init_resource::<SpecializedMeshPipelines<MapInstancedMaterial>>()
            .init_resource::<CullReport>()
            .add_systems(
                RenderStartup,
                init_map_instanced_pipeline.after(MeshPipelineSystems),
            )
            .add_systems(
                Render,
                (
                    queue_map_instanced.in_set(RenderSystems::QueueMeshes),
                    prepare_instance_buffers.in_set(RenderSystems::PrepareResources),
                    prepare_map_part_bind_groups.in_set(RenderSystems::PrepareResources),
                    prepare_map_lights.in_set(RenderSystems::PrepareResources),
                ),
            );
    }
}

fn init_map_instanced_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mesh_pipeline: Res<MeshPipeline>,
    render_device: Res<RenderDevice>,
) {
    // The layout of the per-part material bind group. `RenderPipelineDescriptor`
    // takes layout *descriptors*; the pipeline cache resolves them to the same
    // layout `as_bind_group` uses, which is what wgpu requires for a match.
    let part_layout = MapPartMaterial::bind_group_layout_descriptor(&render_device);
    // Scene lights: one uniform, one bind group, shared by every draw.
    let light_layout = map_light_bind_group_layout();
    let light_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("map lights uniform"),
        contents: &MapLights::default().to_bytes(),
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });
    let light_bind_group = render_device.create_bind_group(
        Some("map lights bind group"),
        &render_device.create_bind_group_layout(Some("map lights layout"), &light_layout.entries),
        &[BindGroupEntry {
            binding: 0,
            resource: light_buffer.as_entire_binding(),
        }],
    );
    commands.insert_resource(MapLightBindings {
        buffer: light_buffer,
        bind_group: light_bind_group,
        uploaded: None,
    });
    commands.insert_resource(MapInstancedMaterial {
        shader: asset_server.load(SHADER_ASSET_PATH),
        mesh_pipeline: mesh_pipeline.clone(),
        part_layout,
        light_layout,
    });
}

impl SpecializedMeshPipeline for MapInstancedMaterial {
    type Key = MeshPipelineKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayoutRef,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mut descriptor = self.mesh_pipeline.specialize(key, layout)?;

        // Per-part material at group 3, the index Bevy's own material pipeline
        // uses. The mesh pipeline's layout holds groups 0..3 (view, view
        // binding array, mesh), so this appends the fourth.
        descriptor
            .layout
            .insert(MATERIAL_BIND_GROUP_INDEX, self.part_layout.clone());
        descriptor.layout.push(self.light_layout.clone());
        let bind_group_def = ShaderDefVal::UInt(
            "MATERIAL_BIND_GROUP".into(),
            MATERIAL_BIND_GROUP_INDEX as u32,
        );
        let light_group_def = ShaderDefVal::UInt(
            "MAP_LIGHT_BIND_GROUP".into(),
            MAP_LIGHT_BIND_GROUP_INDEX as u32,
        );
        descriptor.vertex.shader_defs.push(bind_group_def.clone());
        descriptor.vertex.shader_defs.push(light_group_def.clone());
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader_defs.push(bind_group_def);
            fragment.shader_defs.push(light_group_def);
        }

        descriptor.vertex.shader = self.shader.clone();
        descriptor.vertex.buffers.push(VertexBufferLayout {
            array_stride: size_of::<InstanceData>() as u64,
            step_mode: VertexStepMode::Instance,
            attributes: vec![
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: 0,
                    // 0..4 are the mesh attributes (position, normal, uv, tangent).
                    shader_location: 8,
                },
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 9,
                },
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: 32,
                    shader_location: 10,
                },
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: 48,
                    shader_location: 11,
                },
            ],
        });
        // Map parts are two-sided in the original data.
        descriptor.primitive.cull_mode = None;
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader = self.shader.clone();
        }
        Ok(descriptor)
    }
}

/// Per-frame culling counters, printed once a second so a run's log shows how
/// much of the city the view actually submitted.
#[derive(Resource, Default)]
struct CullReport {
    last: Option<Instant>,
    submitted: usize,
    culled: usize,
}

/// True when all eight corners of `aabb` fall outside one clip plane, i.e. the
/// box cannot contribute a pixel to this view.
///
/// Six plane tests from `clip_from_world`, eight transforms, no allocation, and
/// conservative in the two directions that matter: corners behind the eye
/// (`w <= 0`) never count as outside a plane (their sign flips the comparison),
/// and a box that straddles the eye plane is kept.
fn aabb_outside_view(clip_from_world: &Mat4, aabb: &Aabb) -> bool {
    let center = Vec3::new(aabb.center.x, aabb.center.y, aabb.center.z);
    let half = Vec3::new(
        aabb.half_extents.x,
        aabb.half_extents.y,
        aabb.half_extents.z,
    );
    // x < -w, x > w, y < -w, y > w, z < 0, z > w.
    let mut outside = [true; 6];
    let mut any_front = false;
    for corner in 0..8 {
        let local = center
            + Vec3::new(
                if corner & 1 == 0 { -half.x } else { half.x },
                if corner & 2 == 0 { -half.y } else { half.y },
                if corner & 4 == 0 { -half.z } else { half.z },
            );
        let clip = *clip_from_world * local.extend(1.0);
        let front = clip.w > 0.0;
        any_front |= front;
        let hits = [
            clip.x < -clip.w,
            clip.x > clip.w,
            clip.y < -clip.w,
            clip.y > clip.w,
            clip.z < 0.0,
            clip.z > clip.w,
        ];
        for (slot, hit) in outside.iter_mut().zip(hits) {
            if !front || !hit {
                *slot = false;
            }
        }
    }
    any_front && outside.iter().any(|&out| out)
}

fn queue_map_instanced(
    transparent_3d_draw_functions: Res<DrawFunctions<Transparent3d>>,
    custom_pipeline: Res<MapInstancedMaterial>,
    mut pipelines: ResMut<SpecializedMeshPipelines<MapInstancedMaterial>>,
    pipeline_cache: Res<PipelineCache>,
    meshes: Res<RenderAssets<RenderMesh>>,
    render_mesh_instances: Res<RenderMeshInstances>,
    maybe_batched_instance_buffers: Option<
        Res<BatchedInstanceBuffers<MeshUniform, MeshInputUniform>>,
    >,
    material_meshes: Query<(Entity, &MainEntity, &InstanceMaterialData)>,
    mut transparent_render_phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    views: Query<&ExtractedView>,
    view_key_cache: Res<ViewKeyCache>,
    cluster_stats: Option<Res<MapClusterStats>>,
    mut report: ResMut<CullReport>,
) {
    let draw_custom = transparent_3d_draw_functions.read().id::<DrawMapInstanced>();

    let mut submitted = 0usize;
    let mut culled = 0usize;

    for view in &views {
        let Some(transparent_phase) = transparent_render_phases.get_mut(&view.retained_view_entity)
        else {
            continue;
        };

        let Some(&view_key) = view_key_cache.get(&view.retained_view_entity) else {
            continue;
        };

        let clip_from_world = view.clip_from_world.unwrap_or_else(|| {
            view.clip_from_view * Mat4::from(view.world_from_view.affine().inverse())
        });

        for (entity, main_entity, instance_data) in &material_meshes {
            // Cluster culling: skip the whole draw entity when its instance
            // bounds fall outside the view.
            if aabb_outside_view(&clip_from_world, &instance_data.1) {
                culled += 1;
                continue;
            }
            let Some(mesh_instance) = render_mesh_instances.render_mesh_queue_data(*main_entity)
            else {
                continue;
            };
            let Some(mesh) = meshes.get(mesh_instance.mesh_asset_id()) else {
                continue;
            };
            let key = view_key
                | MeshPipelineKey::from_primitive_topology_and_strip_index(
                    mesh.primitive_topology(),
                    mesh.index_format(),
                );
            let pipeline = pipelines
                .specialize(&pipeline_cache, &custom_pipeline, key, &mesh.layout)
                .unwrap();
            transparent_phase.add_retained(Transparent3d {
                sorting_info: TransparentSortingInfo3d::Sorted {
                    mesh_center: pbr::get_mesh_instance_world_from_local(
                        *main_entity,
                        mesh_instance.current_uniform_index,
                        &render_mesh_instances,
                        maybe_batched_instance_buffers.as_deref(),
                    )
                    .transform_point3(mesh.aabb_center),
                    depth_bias: 0.0,
                },
                entity: (entity, *main_entity),
                pipeline,
                draw_function: draw_custom,
                distance: 0.0,
                batch_range: 0..1,
                extra_index: PhaseItemExtraIndex::None,
                indexed: true,
            });
            submitted += 1;
        }
    }

    report.submitted = submitted;
    report.culled = culled;
    let last = *report.last.get_or_insert_with(Instant::now);
    if last.elapsed() >= Duration::from_secs(1) {
        let stats = cluster_stats.as_deref().copied().unwrap_or_default();
        println!(
            "cull: parts={} clusters={} submitted={} culled={}",
            stats.parts, stats.clusters, report.submitted, report.culled
        );
        report.last = Some(Instant::now());
    }
}

/// One GPU buffer per draw entity. On the preload path the instance data is
/// immutable for the life of the placement, so entities that already carry
/// their buffer are skipped: re-uploading the whole map every frame would be
/// tens of megabytes of churn. The streaming path marks the parts it rebuilt
/// with [`InstancesDirty`], which forces exactly those buffers to be rewritten.
fn prepare_instance_buffers(
    mut commands: Commands,
    query: Query<(
        Entity,
        &InstanceMaterialData,
        Option<&InstanceBuffer>,
        Has<InstancesDirty>,
    )>,
    render_device: Res<RenderDevice>,
) {
    for (entity, instance_data, existing, dirty) in &query {
        if existing.is_some() && !dirty {
            continue;
        }
        let mut bytes = Vec::with_capacity(instance_data.len() * size_of::<InstanceData>());
        for instance in &instance_data.0 {
            for value in instance.model {
                bytes.extend_from_slice(&value.to_ne_bytes());
            }
        }
        let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("map instance data buffer"),
            contents: &bytes,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        });
        commands.entity(entity).insert(InstanceBuffer {
            buffer,
            length: instance_data.len(),
        });
        commands.entity(entity).remove::<InstancesDirty>();
    }
}

/// One bind group per draw entity, built from its [`MapPartMaterial`].
///
/// Like the instance buffers, the inputs are immutable for the life of the
/// map, so entities whose bind group was built against the current
/// [`TextureGeneration`] are skipped instead of rebuilding thousands of bind
/// groups every frame. When the streamer uploads a texture it bumps that
/// generation, and every part whose bind group still points at the 1x1
/// placeholder view is rebuilt against the real `Image` on that frame.
fn prepare_map_part_bind_groups(
    mut commands: Commands,
    query: Query<(Entity, &MapPartMaterial, Option<&PreparedMapPart>)>,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    generation: Option<Res<TextureGeneration>>,
    mut param: StaticSystemParam<<MapPartMaterial as AsBindGroup>::Param>,
) {
    let descriptor = MapPartMaterial::bind_group_layout_descriptor(&render_device);
    let current = generation.as_deref().copied().unwrap_or_default().0;
    for (entity, material, prepared) in &query {
        if prepared.is_some_and(|prepared| prepared.generation == current) {
            continue;
        }
        match material.as_bind_group(
            &descriptor,
            &render_device,
            &pipeline_cache,
            &mut *param,
        ) {
            Ok(prepared) => {
                commands.entity(entity).insert(PreparedMapPart {
                    bind_group: prepared.bind_group,
                    generation: current,
                    _bindings: prepared.bindings,
                });
            }
            // A texture is still uploading; retry next frame.
            Err(AsBindGroupError::RetryNextUpdate) => {}
            Err(error) => {
                eprintln!("note: instanced part {entity} has no bind group: {error}");
            }
        }
    }
}

type DrawMapInstanced = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetMeshBindGroup<2>,
    SetMapPartBindGroup<{ MATERIAL_BIND_GROUP_INDEX }>,
    SetMapLightBindGroup,
    DrawMeshInstanced,
);

/// Binds the scene-light uniform at [`MAP_LIGHT_BIND_GROUP_INDEX`]. The bind
/// group is a single render-world resource, so nothing per-entity is needed.
struct SetMapLightBindGroup;

impl<P: PhaseItem> RenderCommand<P> for SetMapLightBindGroup {
    type Param = SRes<MapLightBindings>;
    type ViewQuery = ();
    type ItemQuery = ();

    #[inline]
    fn render<'w>(
        _item: &P,
        _view: (),
        _item_query: Option<()>,
        bindings: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        pass.set_bind_group(
            MAP_LIGHT_BIND_GROUP_INDEX,
            &bindings.into_inner().bind_group,
            &[],
        );
        RenderCommandResult::Success
    }
}

/// Binds the entity's per-part material bind group at `I`.
struct SetMapPartBindGroup<const I: usize>;

impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetMapPartBindGroup<I> {
    type Param = ();
    type ViewQuery = ();
    type ItemQuery = Read<PreparedMapPart>;

    #[inline]
    fn render<'w>(
        _item: &P,
        _view: (),
        prepared: Option<&'w PreparedMapPart>,
        _param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(prepared) = prepared else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(I, &prepared.bind_group, &[]);
        RenderCommandResult::Success
    }
}

struct DrawMeshInstanced;

impl<P: PhaseItem> RenderCommand<P> for DrawMeshInstanced {
    type Param = (
        SRes<RenderAssets<RenderMesh>>,
        SRes<RenderMeshInstances>,
        SRes<MeshAllocator>,
    );
    type ViewQuery = ();
    type ItemQuery = Read<InstanceBuffer>;

    #[inline]
    fn render<'w>(
        item: &P,
        _view: (),
        instance_buffer: Option<&'w InstanceBuffer>,
        (meshes, render_mesh_instances, mesh_allocator): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        // A borrow check workaround.
        let mesh_allocator = mesh_allocator.into_inner();

        let Some(mesh_instance) = render_mesh_instances.render_mesh_queue_data(item.main_entity())
        else {
            return RenderCommandResult::Skip;
        };
        let Some(gpu_mesh) = meshes.into_inner().get(mesh_instance.mesh_asset_id()) else {
            return RenderCommandResult::Skip;
        };
        let Some(instance_buffer) = instance_buffer else {
            return RenderCommandResult::Skip;
        };
        let Some(vertex_buffer_slice) =
            mesh_allocator.mesh_vertex_slice(&mesh_instance.mesh_asset_id())
        else {
            return RenderCommandResult::Skip;
        };

        pass.set_vertex_buffer(0, vertex_buffer_slice.buffer.slice(..));
        pass.set_vertex_buffer(1, instance_buffer.buffer.slice(..));

        match &gpu_mesh.buffer_info {
            RenderMeshBufferInfo::Indexed {
                index_format,
                count,
            } => {
                let Some(index_buffer_slice) =
                    mesh_allocator.mesh_index_slice(&mesh_instance.mesh_asset_id())
                else {
                    return RenderCommandResult::Skip;
                };

                pass.set_index_buffer(index_buffer_slice.buffer.slice(..), *index_format);
                pass.draw_indexed(
                    index_buffer_slice.range.start..(index_buffer_slice.range.start + count),
                    vertex_buffer_slice.range.start as i32,
                    0..instance_buffer.length as u32,
                );
            }
            RenderMeshBufferInfo::NonIndexed => {
                pass.draw(vertex_buffer_slice.range, 0..instance_buffer.length as u32);
            }
        }
        RenderCommandResult::Success
    }
}
