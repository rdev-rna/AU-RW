// Character surface. Every material: original diffuse/alpha + RG normal XY
// (positive Z reconstruction, tangent orientation and green sign are
// presentation choices). Materials on CN_Cloth_Tile_ColorMasked_AlphaTest
// (flags.z = 1) add the template's G-buffer surface, re-derived from its
// pixel shader (ACU-RE-0084):
//   - mask G -> band weights metal / cloth / mid / base;
//   - detail normal+gloss maps t36 / t38 / t39 blended by band, combined
//     with the base normal by reoriented normal mapping;
//   - gloss = base gloss (normal alpha) and detail gloss with specular powers
//     2^(19 g) added as reciprocals; GGX alpha^2 = 2 / (2^(19 g) + 2);
//   - specular reflectance from albedo hue/saturation and band intensities;
//   - specular occlusion pow(mask B, 2.2).
// Not ported: the velvet lobe blend (t_MiscProps.x), back-light
// translucency, the dissolve, global wetness. ACU's own deferred BRDF is
// replaced by Bevy's.
// CN_Skin_Wrinkle (flags.z = 2): saturation matrix, tint lerp by mask B,
// F0 0.04, specular occlusion pow(mask B, 2.2); base gloss SYNTHETIC
// (engine-bound t35). Wrinkle normals need a facial-animation weight: off.
// CN_Hair (flags.z = 3): tint then saturation matrix; gloss = normal alpha
// and F0 = pow(0.8 * mask1 R, 2.2) (texture-set maps, inferred binding);
// Bevy anisotropy across the strand stands in for ACU's two-lobe hair path.
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
}
struct Params {
    flags: vec4<f32>,
    detail: vec4<f32>,
    specular: vec4<f32>,
    bands: vec4<f32>,
    detail_uv: vec4<f32>,
    matrix_r: vec4<f32>,
    matrix_g: vec4<f32>,
    matrix_b: vec4<f32>,
    tint: vec4<f32>,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var packed_normal: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var<uniform> settings: Params;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var mask_map: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var mask_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var detail_metal: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var detail_metal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var detail_cloth: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var detail_cloth_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(109) var detail_base: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(110) var detail_base_sampler: sampler;

// Detail texel -> (normal - (0, 0, 1), gloss - 1), as the template blends them.
fn detail(t: vec4<f32>) -> vec4<f32> {
    let xy = t.xy * 2.0 - 1.0;
    return vec4<f32>(xy, sqrt(max(0.0, 1.0 - dot(xy, xy))) - 1.0, t.w - 1.0);
}

// Hue (turns) and saturation; both 0 for grey.
fn hue_saturation(c: vec3<f32>) -> vec2<f32> {
    let mx = max(c.r, max(c.g, c.b));
    let range = mx - min(c.r, min(c.g, c.b));
    if range == 0.0 {
        return vec2<f32>(0.0);
    }
    let d = (vec3<f32>(mx) - c.bgr) / range; // (max-b, max-g, max-r)
    let db = d.x;
    let dg = d.y;
    let dr = d.z;
    let is_max = vec3<f32>(c >= vec3<f32>(mx));
    let h = max(max(is_max.r * (db - dg + 6.0), is_max.b * (dg - dr + 4.0)), is_max.g * (dr - db + 2.0));
    return vec2<f32>(fract(h / 6.0), range / mx);
}

fn gloss_to_perceptual_roughness(g: f32) -> f32 {
    return pow(2.0 / (exp2(19.0 * g) + 2.0), 0.25);
}

fn saturation(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(dot(settings.matrix_r.xyz, c), dot(settings.matrix_g.xyz, c), dot(settings.matrix_b.xyz, c));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    var surface = pbr_input_from_standard_material(in, front);
    surface.material.base_color = alpha_discard(surface.material, surface.material.base_color);
    let surface_kind = settings.flags.z;
#ifdef VERTEX_UVS_A
    if surface_kind > 1.5 && surface_kind < 2.5 {
        // skin
        var mask_b = 1.0;
        if settings.flags.w > 0.5 {
            mask_b = textureSample(mask_map, mask_sampler, in.uv).b;
        }
        let w = saturate(mask_b + settings.tint.w);
        let albedo = saturation(surface.material.base_color.rgb);
        surface.material.base_color = vec4<f32>(mix(settings.tint.rgb, albedo, w), surface.material.base_color.a);
        let occlusion = pow(saturate(mask_b), 2.2);
        surface.material.perceptual_roughness = gloss_to_perceptual_roughness(settings.specular.x);
        surface.material.metallic = 0.0;
        surface.material.reflectance = vec3<f32>(sqrt(0.04 * occlusion / 0.16));
        surface.specular_occlusion = occlusion;
    } else if surface_kind > 2.5 {
        // hair
        let albedo = saturation(surface.material.base_color.rgb * settings.tint.rgb);
        surface.material.base_color = vec4<f32>(albedo, surface.material.base_color.a);
        var mask_r = 1.0;
        if settings.flags.w > 0.5 {
            mask_r = textureSample(mask_map, mask_sampler, in.uv).r;
        }
        surface.material.metallic = 0.0;
        surface.material.reflectance = vec3<f32>(sqrt(pow(0.8 * saturate(mask_r), 2.2) / 0.16));
    }
#endif
#ifdef VERTEX_TANGENTS
#ifdef VERTEX_UVS_A
    if settings.flags.x > 0.5 {
        let packed = textureSample(packed_normal, normal_sampler, in.uv);
        let xy = packed.rg * 2.0 - 1.0;
        var normal = vec3<f32>(xy, sqrt(max(0.0, 1.0 - dot(xy, xy))));
        if surface_kind > 0.5 && surface_kind < 1.5 {
            let m = textureSample(mask_map, mask_sampler, in.uv).rgb;
            let metal = saturate((0.247059 - m.g) * 4.047619);
            let cloth = saturate((m.g - 0.752941) * 4.047619);
            let mid = (1.0 - cloth) * saturate(m.g - settings.bands.z) * 8.0;
            let base = 1.0 - min(metal + mid + cloth, 1.0);
            let duv = in.uv * settings.detail_uv.xy;
            let bits = u32(settings.detail_uv.z + 0.5);
            var d = vec4<f32>(0.0, 0.0, 4.0, 4.0);
            if (bits & 2u) != 0u {
                let w = cloth * settings.detail.z + mid * settings.detail.y;
                d += w * detail(textureSample(detail_cloth, detail_cloth_sampler, duv));
            }
            if (bits & 4u) != 0u {
                d += base * settings.detail.x * detail(textureSample(detail_base, detail_base_sampler, duv));
            }
            if (bits & 1u) != 0u {
                d += metal * settings.detail.w * detail(textureSample(detail_metal, detail_metal_sampler, duv));
            }
            // reoriented normal mapping in the stored tangent space
            let t = normal + vec3<f32>(0.0, 0.0, 1.0);
            let u = d.xyz * vec3<f32>(-1.0, -1.0, 1.0);
            normal = t * dot(t, u) / t.z - u;
            // gloss and roughness
            let wet = settings.bands.w;
            let inv = exp2(-19.0 * packed.a) + exp2(-19.0 * d.w);
            var g = saturate(log2(1.0 / inv) / 19.0);
            g = g + wet * (max(g, 0.1) * 1.5 - g);
            surface.material.perceptual_roughness = pow(2.0 / (exp2(19.0 * g) + 2.0), 0.25);
            // specular reflectance
            let hs = hue_saturation(surface.material.base_color.rgb);
            let sat = (metal * settings.specular.w + cloth * settings.specular.z) * hs.y;
            let h = hs.x * ((cloth + mid) * settings.bands.y + metal * settings.bands.x);
            let hue_rgb = saturate(vec3<f32>(
                abs(h * 6.0 - 3.0) - 1.0,
                2.0 - abs(h * 6.0 - 2.0),
                2.0 - abs(h * 6.0 - 4.0),
            ));
            let intensity = base * 0.04 + cloth * settings.specular.x + mid * settings.specular.y + metal * 0.9;
            let occlusion = pow(saturate(m.b), 2.2);
            let f0 = ((hue_rgb - 1.0) * sat + 1.0) * intensity * occlusion;
            surface.material.metallic = 0.0;
            surface.material.reflectance = sqrt(f0 / 0.16);
            surface.specular_occlusion = occlusion;
        }
        if surface_kind > 2.5 {
            // hair gloss: normal-map alpha, wetness towards 1.5 * max(g, 0.1)
            let wet = settings.bands.w;
            var g = packed.a;
            g = g + wet * (max(g, 0.1) * 1.5 - g);
            surface.material.perceptual_roughness = gloss_to_perceptual_roughness(g);
        }
        normal.y *= settings.flags.y;
        let n = normalize(surface.N);
        let tangent = normalize(in.world_tangent.xyz - n * dot(n, in.world_tangent.xyz));
        let bitangent = cross(n, tangent) * in.world_tangent.w;
        surface.N = normalize(tangent * normal.x + bitangent * normal.y + n * normal.z);
        if surface_kind > 2.5 {
            // strand along the tangent (ACU's x axis); stretch the highlight across it
            let strand = normalize(tangent - surface.N * dot(surface.N, tangent));
            surface.anisotropy_strength = settings.tint.w;
            surface.anisotropy_T = normalize(cross(surface.N, strand));
            surface.anisotropy_B = strand;
        }
    }
#endif
#endif
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(surface);
    out.color = main_pass_post_lighting_processing(surface, out.color);
    return out;
}
