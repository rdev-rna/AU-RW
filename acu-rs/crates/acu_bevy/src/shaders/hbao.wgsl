// ACU's max-quality AO: NVIDIA HBAO+ as GFSDK_SSAO.win64.dll computes it (finding ACU-RE-0105),
// with the constants of `acu_assets::hbao::HbaoConstants`, written over Bevy's SSAO output so
// `diffuse_occlusion` keeps its binding.
//
// Adapted from For Honor's port (ForHonorRustRewrite fh-rs/crates/fh_bevy/src/fh_hbao.wgsl,
// commit 546a3776): view-space reconstruction, the normal from the nearer depth neighbour, the
// falloff and the coarse 8 x 4 kernel are the same algorithm in both SDKs (the CoarseAO DXBC
// matches opcode for opcode). Changed for ACU's older SDK, from its DXBC:
// - a detail term: 4 full-resolution taps at +-3 px in x and y, averaged (normal pass), added
//   to the coarse sum as `detail * DetailAO * 8` before the 1/16 normalisation;
// - one CoarseAO amount for all 32 samples, no foreground / background radius clamps;
// - the blur is a plain cross-bilateral one (no slope term), radius 8: taps 1..4 and bilinear
//   taps at 5.5 and 7.5 weighted as r = 5 and 7.
// UNVERIFIED: the per-layer jitter (a per-pixel hash stands in) and full-resolution evaluation
// instead of 16 deinterleaved quarter-resolution layers.

struct Params {
    view_from_clip: mat4x4<f32>,
    // size.xy, radius_to_screen, -1 / R^2
    a: vec4<f32>,
    // NdotV bias, detail amount, coarse amount, power exponent
    b: vec4<f32>,
    // blur sharpness, blur falloff, blur radius (0 = no blur), unused
    c: vec4<f32>,
}

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var depth: texture_depth_2d;
@group(0) @binding(2) var src: texture_2d<f32>;
@group(0) @binding(3) var ao_out: texture_storage_2d<rg32float, write>;
#ifdef USE_R16FLOAT
@group(0) @binding(4) var final_out: texture_storage_2d<r16float, write>;
#else
@group(0) @binding(4) var final_out: texture_storage_2d<r32float, write>;
#endif

const DIRECTIONS: u32 = 8u;
const STEPS: u32 = 4u;
const DETAIL_TAP: i32 = 3;
const PI: f32 = 3.14159265;
const SKY: f32 = 1.0e30;

// View-space position of pixel `px` (Bevy: right-handed, looking down -z, reverse-z depth).
fn view_pos(px: vec2<i32>) -> vec3<f32> {
    let size = vec2<i32>(p.a.xy);
    let q = clamp(px, vec2<i32>(0), size - 1);
    let z = textureLoad(depth, q, 0);
    let uv = (vec2<f32>(q) + 0.5) / p.a.xy;
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, max(z, 1.0e-7), 1.0);
    let v = p.view_from_clip * ndc;
    return v.xyz / v.w;
}

fn min_diff(a: vec3<f32>, b: vec3<f32>) -> vec3<f32> {
    return select(b, a, dot(a, a) < dot(b, b));
}

fn hash(px: vec2<u32>) -> vec2<f32> {
    let f = vec2<f32>(px);
    let r = fract(52.9829189 * fract(dot(f, vec2<f32>(0.06711056, 0.00583715))));
    let s = fract(52.9829189 * fract(dot(f + 17.0, vec2<f32>(0.00583715, 0.06711056))));
    return vec2<f32>(r, s);
}

// One sample: saturate(N.V / |V| - bias) * saturate(1 - |V|^2 / R^2).
fn falloff_ao(pos: vec3<f32>, n: vec3<f32>, s: vec3<f32>) -> f32 {
    let v = s - pos;
    let vv = dot(v, v);
    let nv = dot(n, v) * inverseSqrt(max(vv, 1.0e-12));
    return clamp(nv - p.b.x, 0.0, 1.0) * clamp(vv * p.a.w + 1.0, 0.0, 1.0);
}

@compute @workgroup_size(8, 8, 1)
fn hbao(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = vec2<u32>(p.a.xy);
    if (any(id.xy >= size)) {
        return;
    }
    let px = vec2<i32>(id.xy);
    if (textureLoad(depth, px, 0) <= 0.0) {
        textureStore(ao_out, px, vec4<f32>(1.0, SKY, 0.0, 0.0));
        return;
    }
    let pos = view_pos(px);
    let vz = -pos.z;
    let radius_px = p.a.z / vz;
    // CoarseAO returns 1 when `radius_to_screen / z` is below 1 (pixels, as For Honor reads it).
    if (radius_px < 1.0) {
        textureStore(ao_out, px, vec4<f32>(1.0, vz, 0.0, 0.0));
        return;
    }
    let dx = min_diff(view_pos(px + vec2<i32>(1, 0)) - pos, pos - view_pos(px - vec2<i32>(1, 0)));
    let dy = min_diff(view_pos(px + vec2<i32>(0, 1)) - pos, pos - view_pos(px - vec2<i32>(0, 1)));
    var n = normalize(cross(dx, dy));
    if (dot(n, -pos) < 0.0) {
        n = -n;
    }
    var detail = 0.0;
    if (p.b.y != 0.0) {
        for (var i = 0; i < 4; i++) {
            let axis = select(vec2<i32>(0, 1), vec2<i32>(1, 0), i < 2);
            let side = select(-1, 1, (i & 1) == 0);
            detail += falloff_ao(pos, n, view_pos(px + axis * side * DETAIL_TAP));
        }
        detail *= 0.25;
    }
    // Steps on the quarter-resolution grid: radius_px / 4 / 5 each, starting one texel out plus
    // jitter, rounded per sample, then scaled back by 4.
    let step_q = radius_px * 0.05;
    let rnd = hash(id.xy);
    var coarse = 0.0;
    for (var d = 0u; d < DIRECTIONS; d++) {
        let ang = (f32(d) + rnd.x) * (2.0 * PI / f32(DIRECTIONS));
        let dir = vec2<f32>(cos(ang), sin(ang));
        var ray = rnd.y * step_q + 1.0;
        for (var s = 0u; s < STEPS; s++) {
            let q = px + 4 * vec2<i32>(round(ray * dir));
            coarse += falloff_ao(pos, n, view_pos(q));
            ray += step_q;
        }
    }
    // saturate(1 - (coarse * CoarseAO' + detail * DetailAO' * 8) / 16); reinterleave: pow(, power).
    let occ = clamp(1.0 - (coarse * p.b.z + detail * p.b.y * 8.0) * 0.0625, 0.0, 1.0);
    textureStore(ao_out, px, vec4<f32>(pow(occ, p.b.w), vz, 0.0, 0.0));
}

fn tap(px: vec2<i32>, off: vec2<i32>) -> vec2<f32> {
    let size = vec2<i32>(p.a.xy);
    return textureLoad(src, clamp(px + off, vec2<i32>(0), size - 1), 0).xy;
}

// exp2(-r^2 falloff - ((z_tap - z_centre) sharpness)^2), as the DLL's blur shaders.
fn weight(s: vec2<f32>, zc: f32, r: f32) -> f32 {
    let d = (s.y - zc) * p.c.x;
    return exp2(-r * r * p.c.y - d * d);
}

// Cross-bilateral blur along `axis`. Radius 2: taps 1, 2. Radius 4 and 8: point taps 1..r/2,
// then bilinear taps (two texels averaged) at r/2 + 1.5, r/2 + 3.5, ... weighted as one less.
fn blur(px: vec2<i32>, axis: vec2<i32>) -> f32 {
    let c = textureLoad(src, px, 0).xy;
    let radius = i32(p.c.z);
    if (c.y >= SKY || radius == 0) {
        return c.x;
    }
    let points = select(radius / 2, 2, radius == 2);
    var sum = c.x;
    var wsum = 1.0;
    for (var side = -1; side <= 1; side += 2) {
        let a = axis * side;
        for (var r = 1; r <= points; r++) {
            let s = tap(px, a * r);
            let w = weight(s, c.y, f32(r));
            sum += s.x * w;
            wsum += w;
        }
        for (var r = points + 1; r < radius; r += 2) {
            let s = 0.5 * (tap(px, a * r) + tap(px, a * (r + 1)));
            let w = weight(s, c.y, f32(r));
            sum += s.x * w;
            wsum += w;
        }
    }
    return sum / wsum;
}

@compute @workgroup_size(8, 8, 1)
fn blur_x(@builtin(global_invocation_id) id: vec3<u32>) {
    if (any(id.xy >= vec2<u32>(p.a.xy))) {
        return;
    }
    let px = vec2<i32>(id.xy);
    let z = textureLoad(src, px, 0).y;
    textureStore(ao_out, px, vec4<f32>(blur(px, vec2<i32>(1, 0)), z, 0.0, 0.0));
}

@compute @workgroup_size(8, 8, 1)
fn blur_y(@builtin(global_invocation_id) id: vec3<u32>) {
    if (any(id.xy >= vec2<u32>(p.a.xy))) {
        return;
    }
    let px = vec2<i32>(id.xy);
    textureStore(final_out, px, vec4<f32>(blur(px, vec2<i32>(0, 1)), 0.0, 0.0, 0.0));
}
