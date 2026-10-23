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
//! path gives `map_material` — diffuse map, normal map and alpha cut-out — bound
//! through an `AsBindGroup` material at Bevy's material bind group index.

use std::mem::size_of;

use bevy::{
    camera::visibility::NoFrustumCulling,
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
        mesh::{allocator::MeshAllocator, RenderMesh, RenderMeshBufferInfo},
        render_asset::RenderAssets,
        render_phase::{
            AddRenderCommand, DrawFunctions, PhaseItem, PhaseItemExtraIndex, RenderCommand,
            RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewSortedRenderPhases,
        },
        render_resource::*,
        renderer::RenderDevice,
        sync_component::SyncComponent,
        sync_world::MainEntity,
        view::ExtractedView,
        Render, RenderApp, RenderStartup, RenderSystems,
    },
};

use bevy::shader::ShaderDefVal;

use crate::load;

/// Embedded shader for the instanced map path.
const SHADER_ASSET_PATH: &str = "embedded://acu_bevy/shaders/map_instanced.wgsl";

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

/// Every placement of one (piece, part), for the renderer to draw instanced.
#[derive(Component, Deref)]
pub struct InstanceMaterialData(pub Vec<InstanceData>);

impl SyncComponent for InstanceMaterialData {
    type Target = Self;
}

impl ExtractComponent for InstanceMaterialData {
    type QueryData = &'static InstanceMaterialData;
    type QueryFilter = ();
    type Out = Self;

    fn extract_component(item: QueryItem<'_, '_, Self::QueryData>) -> Option<Self> {
        Some(InstanceMaterialData(item.0.clone()))
    }
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
}

/// What one [`spawn_map_instanced`] call produced, for the run summary.
#[derive(Debug, Clone, Copy)]
pub struct InstancedSpawned {
    /// Draw entities: one per (piece, part) that has at least one placement.
    pub entities: usize,
    /// Placements accepted (after the radius test).
    pub placements: usize,
    /// Unique (piece, part) entries in the scene.
    pub parts: usize,
    /// Placement x part pairs, i.e. the number of rendered instances.
    pub instances: usize,
}

/// Spawn the map as one entity per unique part carrying all of its placements.
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

    let mut buckets: Vec<Vec<Vec<InstanceData>>> = pieces
        .iter()
        .map(|piece| piece.parts.iter().map(|_| Vec::new()).collect())
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
        for bucket in &mut buckets[instance.piece] {
            bucket.push(data);
        }
    }

    let mut entities = 0usize;
    let mut rendered_instances = 0usize;
    for (piece, parts) in buckets.iter().enumerate() {
        for (part, data) in parts.iter().enumerate() {
            if data.is_empty() {
                continue;
            }
            entities += 1;
            rendered_instances += data.len();
            // `part.material` indexes the scene material table, the same table
            // `bound_materials` is built from; a part with none keeps the
            // white-fallback default, like an untextured `map_material`.
            let material = pieces[piece].parts[part]
                .material
                .and_then(|index| part_materials.get(index).cloned())
                .unwrap_or_default();
            commands.spawn((
                Mesh3d(part_meshes[piece][part].clone()),
                material,
                InstanceMaterialData(data.clone()),
                // The instance matrices are not part of the entity's `Aabb`, so
                // built-in frustum culling would drop the whole part at once.
                NoFrustumCulling,
                Transform::default(),
                Visibility::default(),
            ));
        }
    }

    InstancedSpawned {
        entities,
        placements,
        parts: buckets.iter().map(Vec::len).sum(),
        instances: rendered_instances,
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
        app.sub_app_mut(RenderApp)
            .add_render_command::<Transparent3d, DrawMapInstanced>()
            .init_resource::<SpecializedMeshPipelines<MapInstancedMaterial>>()
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
    commands.insert_resource(MapInstancedMaterial {
        shader: asset_server.load(SHADER_ASSET_PATH),
        mesh_pipeline: mesh_pipeline.clone(),
        part_layout,
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
        let bind_group_def = ShaderDefVal::UInt(
            "MATERIAL_BIND_GROUP".into(),
            MATERIAL_BIND_GROUP_INDEX as u32,
        );
        descriptor.vertex.shader_defs.push(bind_group_def.clone());
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader_defs.push(bind_group_def);
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
    material_meshes: Query<(Entity, &MainEntity), With<InstanceMaterialData>>,
    mut transparent_render_phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    views: Query<&ExtractedView>,
    view_key_cache: Res<ViewKeyCache>,
) {
    let draw_custom = transparent_3d_draw_functions.read().id::<DrawMapInstanced>();

    for view in &views {
        let Some(transparent_phase) = transparent_render_phases.get_mut(&view.retained_view_entity)
        else {
            continue;
        };

        let Some(&view_key) = view_key_cache.get(&view.retained_view_entity) else {
            continue;
        };

        for (entity, main_entity) in &material_meshes {
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
        }
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
/// map, so entities that already carry a bind group are skipped instead of
/// rebuilding thousands of bind groups every frame.
fn prepare_map_part_bind_groups(
    mut commands: Commands,
    query: Query<(Entity, &MapPartMaterial, Option<&PreparedMapPart>)>,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    mut param: StaticSystemParam<<MapPartMaterial as AsBindGroup>::Param>,
) {
    let descriptor = MapPartMaterial::bind_group_layout_descriptor(&render_device);
    for (entity, material, prepared) in &query {
        if prepared.is_some() {
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
    DrawMeshInstanced,
);

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
