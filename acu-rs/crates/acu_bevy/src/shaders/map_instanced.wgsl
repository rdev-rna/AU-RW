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

    // Two-sided parts: `abs` lights back faces like Bevy's double-sided
    // standard material instead of leaving them at the ambient floor.
    let n = normalize(in.normal_world);
    let light = normalize(vec3<f32>(0.35, 0.86, 0.37));
    // Presentation lighting only (no shadow maps, no GI). Once the buildings are
    // correctly placed, a street is a canyon: nearly every visible face points
    // away from `light`, so a 0.35 floor left it unreadable. Split the term into
    // an ambient floor, a cheap hemisphere (sky above / bounce below) and a
    // weaker directional lobe, summed to stay in [0.45, 1.05] so lit faces do not
    // blow out.
    // The street diffuse samples are themselves very dark (the shaded modal texel
    // is ~0.05 sRGB), so a pure multiplier cannot lift an enclosed street: 0.45
    // only moved the modal from 13 to 16. Second pass therefore uses a taller
    // multiplier (0.62..1.14) plus a small additive sky-bounce term in linear
    // light, which raises the floor without washing out sun-facing stone.
    let ambient = 0.62;
    let hemi = 0.22 * (0.5 + 0.5 * n.y);
    let direct = 0.30 * abs(dot(n, light));
    let sky_bounce = vec3<f32>(0.018, 0.020, 0.024);
    let shade = ambient + hemi + direct;
    return vec4<f32>(diffuse.rgb * shade + sky_bounce, 1.0);
}
