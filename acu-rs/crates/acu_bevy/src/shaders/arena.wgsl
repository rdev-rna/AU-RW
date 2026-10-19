// Authored synthetic one-unit grid floor. Bevy 0.19.1 PBR extension interface:
// https://github.com/bevyengine/bevy/blob/v0.19.1/assets/shaders/extended_material.wgsl
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
struct Arena { grid: vec4<f32>, }
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> arena: Arena;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    var surface = pbr_input_from_standard_material(in, front);
    let p = surface.world_position.xz * arena.grid.x;
    let width = max(fwidth(p), vec2<f32>(0.0001));
    let cell = abs(fract(p - 0.5) - 0.5) / width;
    let line = 1.0 - clamp(min(cell.x, cell.y), 0.0, 1.0);
    let checker = abs((i32(floor(p.x)) + i32(floor(p.y))) % 2);
    let base = mix(vec3<f32>(0.25, 0.28, 0.30), vec3<f32>(0.30, 0.33, 0.35), f32(checker));
    surface.material.base_color = vec4<f32>(mix(base, vec3<f32>(0.46, 0.49, 0.51), line * arena.grid.y), 1.0);
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(surface);
    out.color = main_pass_post_lighting_processing(surface, out.color);
    return out;
}
