// Diagnostic surface: original diffuse/alpha + inferred RG normal XY.
// Positive Z reconstruction and tangent orientation are presentation choices.
// Packed B/A channels and original material template equations remain unknown.
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
}
struct Surface { params: vec4<f32>, }
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var packed_normal: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var<uniform> settings: Surface;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    var surface = pbr_input_from_standard_material(in, front);
    surface.material.base_color = alpha_discard(surface.material, surface.material.base_color);
#ifdef VERTEX_TANGENTS
#ifdef VERTEX_UVS_A
    if settings.params.x > 0.5 {
        let packed = textureSample(packed_normal, normal_sampler, in.uv);
        let xy = (packed.rg * 2.0 - 1.0) * vec2<f32>(1.0, settings.params.y);
        let normal = normalize(vec3<f32>(xy, sqrt(max(0.0, 1.0 - dot(xy, xy)))));
        let n = normalize(surface.N);
        let t = normalize(in.world_tangent.xyz - n * dot(n, in.world_tangent.xyz));
        let b = cross(n, t) * in.world_tangent.w;
        surface.N = normalize(t * normal.x + b * normal.y + n * normal.z);
    }
#endif
#endif
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(surface);
    out.color = main_pass_post_lighting_processing(surface, out.color);
    return out;
}
