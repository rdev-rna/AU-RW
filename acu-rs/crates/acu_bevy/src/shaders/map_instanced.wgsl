// Instanced map geometry: one draw per unique (piece, part), one instance per
// placement.
//
// The instance data is the placement's native-space world matrix baked with the
// scene's Z-up -> Y-up presentation rotation, so the vertex stage needs nothing
// from the mesh's own transform: only the instance matrix and the view
// projection. Normals ride the same matrix (placements are rigid).
//
// The fragment stage takes the part's diffuse map, alpha cut-out and lighting
// from the material bind group at `MATERIAL_BIND_GROUP` (group 3), the same
// group index Bevy's own `Material` pipeline uses.

#import bevy_pbr::view_transformations::position_world_to_clip

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Mesh attribute 2: `Mesh::ATTRIBUTE_UV_0`.
    @location(2) uv: vec2<f32>,
};

struct InstanceInput {
    // Locations 0..4 are the mesh attributes (position, normal, uv, tangent),
    // so the four matrix columns live at 8..12.
    @location(8) column_0: vec4<f32>,
    @location(9) column_1: vec4<f32>,
    @location(10) column_2: vec4<f32>,
    @location(11) column_3: vec4<f32>,
};

struct MapVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal_world: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var base_color_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var base_color_sampler: sampler;
// Bound for the next step (normal mapping); not sampled yet.
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var normal_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var normal_sampler: sampler;
// `x` = alpha cutoff (0.0 = opaque), `y` = 1.0 when a normal map is bound.
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<uniform> part_params: vec4<f32>;

// The scene's real lights (`MapLights` in `instanced.rs`), extracted from the
// app's `GlobalAmbientLight` and `DirectionalLight`s every frame. `ambient.rgb`
// is colour * brightness / LIGHT_UNIT, `ambient.a` the live light count;
// `direction.xyz` points towards each light and `colour.rgb` is
// colour * illuminance / LIGHT_UNIT.
struct MapLights {
    ambient: vec4<f32>,
    direction: array<vec4<f32>, 4>,
    colour: array<vec4<f32>, 4>,
};

@group(#{MAP_LIGHT_BIND_GROUP}) @binding(0) var<uniform> map_lights: MapLights;

// The single global calibration constant for this shader: the app's
// exposure/tonemapping chain, measured by matching the per-placement
// `StandardMaterial` reference (`light-ref.png` vs `light-inst.png`,
// `--map-cell 4884`). It scales the whole irradiance, so it can neither tint
// nor clamp an albedo.
//
// The diffuse BRDF differs between the two terms, mirroring Bevy: direct light
// is divided by PI (`pbr_lighting.wgsl`), while `GlobalAmbientLight` is applied
// as `diffuse_color * ambient_color` with no PI (`pbr_ambient.wgsl`). Folding
// PI into the ambient term instead is what made the first attempt both dark and
// too warm.
const MAP_LIGHT_EXPOSURE: f32 = 5.25;
const INVERSE_PI: f32 = 0.31830988618;
// The ambient term is scaled separately: `StandardMaterial` carries a cool sheen
// the direct sum does not reproduce (the reference street frame is RGB
// 135.7/153.9/174.0 against 152.4/167.0/180.8 before this split). Multiplying the
// ambient irradiance — which is the blue `GlobalAmbientLight` — moves the ratio
// back without tinting the albedo.
const MAP_AMBIENT_GAIN: f32 = 1.3;

@vertex
fn vertex(vertex: VertexInput, instance: InstanceInput) -> MapVertexOutput {
    let model = mat4x4<f32>(
        instance.column_0,
        instance.column_1,
        instance.column_2,
        instance.column_3,
    );
    var out: MapVertexOutput;
    out.clip_position = position_world_to_clip((model * vec4<f32>(vertex.position, 1.0)).xyz);
    out.normal_world = normalize((model * vec4<f32>(vertex.normal, 0.0)).xyz);
    out.uv = vertex.uv;
    return out;
}

@fragment
fn fragment(in: MapVertexOutput) -> @location(0) vec4<f32> {
    // The diffuse is uploaded with an sRGB format (`Bc*UnormSrgb` /
    // `Rgba8UnormSrgb`), so the sampler already decodes it to linear light;
    // converting it again here would darken every texture twice.
    let diffuse = textureSample(base_color_texture, base_color_sampler, in.uv);

    // Alpha cut-out: foliage and railings are mask-tested in the original
    // materials, so their transparent texels must not render as solid quads.
    let cutoff = part_params.x;
    if (cutoff > 0.0 && diffuse.a < cutoff) {
        discard;
    }

    // Two-sided parts (the pipeline has `cull_mode: None`): the per-light
    // `max(dot(n, l), 0)` clamp leaves a back face on the ambient term instead
    // of lighting it with a flipped normal, matching the double-sided standard
    // material closely enough for a static map.
    let n = normalize(in.normal_world);

    var direct = vec3<f32>(0.0);
    let light_count = u32(map_lights.ambient.a);
    for (var index: u32 = 0u; index < light_count; index += 1u) {
        let towards_light = map_lights.direction[index].xyz;
        let n_dot_l = max(dot(n, towards_light), 0.0);
        direct += map_lights.colour[index].rgb * n_dot_l;
    }

    // Ambient is the scene's `GlobalAmbientLight` (colour * brightness), direct
    // light is each `DirectionalLight` (colour * illuminance * N.L), exactly the
    // quantities `StandardMaterial` consumes.
    let irradiance = map_lights.ambient.rgb * MAP_AMBIENT_GAIN + direct * INVERSE_PI;
    return vec4<f32>(diffuse.rgb * irradiance * MAP_LIGHT_EXPOSURE, 1.0);
}
