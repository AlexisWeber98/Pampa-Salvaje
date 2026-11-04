// The road surface's fragment shader: Bevy's standard one (Ground 102 earth,
// tinted by the vertex colour) worn into a country road of 1919.
//
// The mesh's second UV is (across, along): the signed distance from the
// centre line (positive to the left) and the distance along the road, in
// metres. From it:
// - Two cart lanes, one each way, each a pair of wheel ruts 1.56 m apart that
//   wander a little along the road and run deeper in some stretches. The
//   lanes sit at the same fraction of the half width on a country road and
//   on a wide town street (`shape.x`), so one shader wears both. The ruts
//   are compacted (darker, smoother) and tilt the normal down their walls.
// - Between the ruts of a lane, where the horses walk, the earth is churned
//   into small clods.
// - The crown between the lanes stands a few centimetres higher, with some
//   dry grass on it, and grass creeps in from the verges.
// - Patches of dry cracked mud (Poly Haven dry_mud_field_001, the ground's
//   soil layer), more of them in the ruts, where water stood.
// - The edge wanders in and out instead of running straight, and fades into
//   the worn ground under it. The mesh's own feather (the vertex alpha) still
//   bounds it, so the strip is always clear at its outer rows.
//
// Fine detail (clods, cracks' relief) fades out a few tens of metres from the
// eye, like the ground's.

import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, calculate_tbn_mikktspace, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    mesh_view_bindings::view,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var mud_color: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var mud_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var mud_normal: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var mud_orm: texture_2d<f32>;
// x: 1 when the mud maps are loaded.
@group(constants::MATERIAL_BIND_GROUP) @binding(104) var<uniform> layers: vec4<f32>;
// x: half the road's width (`RoadNetwork::HALF_WIDTH`), in metres.
@group(constants::MATERIAL_BIND_GROUP) @binding(105) var<uniform> shape: vec4<f32>;
// Where each lane runs, as a fraction of the half width from the centre line
// (1.55 m on a 6.5 m road), and half a cart's gauge in metres.
const LANE: f32 = 0.477;
const GAUGE: f32 = 0.78;
// A rut's half width and depth, and the crown's rise, in metres.
const RUT_WIDTH: f32 = 0.16;
const RUT_DEPTH: f32 = 0.05;
const CROWN: f32 = 0.06;
// Metres a tile of the mud scan covers, and its mean linear luma.
const MUD_TILE: f32 = 3.0;
const MUD_LUMA: f32 = 0.109;
// Dry grass on the crown and the verges (linear).
const GRASS: vec3<f32> = vec3<f32>(0.21, 0.19, 0.09);
// Where the fine detail fades out, in metres from the eye.
const DETAIL_NEAR: f32 = 30.0;
const DETAIL_FAR: f32 = 50.0;
const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

// A hash of a 2D point to [0, 1) (Dave Hoskins' hash12).
fn hash(p: vec2<f32>) -> f32 {
    var q = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn gradient(i: vec2<f32>) -> vec2<f32> {
    let angle = hash(i) * 6.2831853;
    return vec2<f32>(cos(angle), sin(angle));
}

// Gradient noise with its gradient: (value in about [0, 1], d/dx, d/dy).
fn noise_d(p: vec2<f32>) -> vec3<f32> {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let du = 30.0 * f * f * (f * (f - 2.0) + 1.0);
    let ga = gradient(i);
    let gb = gradient(i + vec2<f32>(1.0, 0.0));
    let gc = gradient(i + vec2<f32>(0.0, 1.0));
    let gd = gradient(i + vec2<f32>(1.0, 1.0));
    let a = dot(ga, f);
    let b = dot(gb, f - vec2<f32>(1.0, 0.0));
    let c = dot(gc, f - vec2<f32>(0.0, 1.0));
    let d = dot(gd, f - vec2<f32>(1.0, 1.0));
    let k = a - b - c + d;
    let value = a + (b - a) * u.x + (c - a) * u.y + k * u.x * u.y;
    let slope = ga + (gb - ga) * u.x + (gc - ga) * u.y + (ga - gb - gc + gd) * u.x * u.y
        + du * vec2<f32>(b - a + k * u.y, c - a + k * u.x);
    return vec3<f32>(value * 0.75 + 0.5, slope * 0.75);
}

fn noise(p: vec2<f32>) -> f32 {
    return clamp(noise_d(p).x, 0.0, 1.0);
}

fn fbm(p: vec2<f32>) -> f32 {
    let turn = mat2x2<f32>(0.8, 0.6, -0.6, 0.8);
    let q = turn * p * 2.03 + 17.1;
    return (0.6 * noise(p) + 0.4 * noise(q));
}

fn decode(n: vec3<f32>) -> vec3<f32> {
    let xy = n.xy * 2.0 - 1.0;
    return vec3<f32>(xy, sqrt(max(1.0 - dot(xy, xy), 0.0)));
}

// A bell of unit height and width: `exp(-x²)`. Squared by hand, since `pow`
// is undefined for a negative base in WGSL.
fn bell(x: f32) -> f32 {
    return exp(-x * x);
}

// One rut at `centre`: (how much of it is here, the slope of its floor
// across the road).
fn rut(across: f32, centre: f32, depth: f32) -> vec2<f32> {
    let x = (across - centre) / RUT_WIDTH;
    let b = bell(x);
    return vec2<f32>(b, depth * 2.0 * x / RUT_WIDTH * b);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let p = in.world_position.xz;
    var lane = vec2<f32>(0.0);
    @if(VERTEX_UVS_B) {
        lane = in.uv_b;
    }
    let across = lane.x;
    let along = lane.y;
    let half = shape.x;

    // Derivatives first, in uniform control flow: the world gradient of
    // `across` on the surface (Mikkelsen's surface gradient), and the mud's
    // UV footprint.
    let dpx = dpdx(in.world_position.xyz);
    let dpy = dpdy(in.world_position.xyz);
    let dax = dpdx(across);
    let day = dpdy(across);
    let mud_uv = p / MUD_TILE;
    let mud_dx = dpdx(mud_uv);
    let mud_dy = dpdy(mud_uv);
    let n0 = normalize(pbr_input.world_normal);
    let r1 = cross(dpy, n0);
    let r2 = cross(n0, dpx);
    let det = dot(dpx, r1);
    var across_grad = vec3<f32>(0.0);
    if (abs(det) > 1e-8) {
        across_grad = (dax * r1 + day * r2) / det;
    }

    let fade = 1.0 - smoothstep(DETAIL_NEAR, DETAIL_FAR, distance(in.world_position.xyz, view.world_position));

    // The two lanes, each wandering on its own, its ruts deeper in some
    // stretches than others.
    let left = LANE * half + 0.22 * (noise(vec2<f32>(along * 0.031, 3.7)) - 0.5) * 2.0;
    let right = -LANE * half + 0.22 * (noise(vec2<f32>(along * 0.029, 9.1)) - 0.5) * 2.0;
    let deep_left = RUT_DEPTH * mix(0.35, 1.3, noise(vec2<f32>(along * 0.07, 1.3)));
    let deep_right = RUT_DEPTH * mix(0.35, 1.3, noise(vec2<f32>(along * 0.07, 5.9)));
    let ruts = array<vec2<f32>, 4>(
        rut(across, left - GAUGE, deep_left),
        rut(across, left + GAUGE, deep_left),
        rut(across, right - GAUGE, deep_right),
        rut(across, right + GAUGE, deep_right),
    );
    var worn = 0.0;
    var slope = -2.0 * CROWN * across / (half * half);
    for (var i = 0; i < 4; i++) {
        worn = max(worn, ruts[i].x);
        slope += ruts[i].y;
    }
    let depth = mix(deep_right, deep_left, step(0.0, across)) / RUT_DEPTH;
    worn *= clamp(depth, 0.0, 1.0);

    // The hoof path down the middle of each lane.
    let hooves = max(
        bell((across - left) / 0.38),
        bell((across - right) / 0.38),
    );
    let clods = noise_d(p * 4.3 + 2.1);

    var color = pbr_input.material.base_color.rgb;
    color *= mix(1.0, 0.8, worn);
    color *= 1.0 - 0.16 * hooves * smoothstep(0.45, 0.8, clods.x) * fade;

    // Grass on the crown and creeping in from the verges.
    let crown = bell(across / 0.5);
    let verge = smoothstep(half - 0.9, half - 0.2, abs(across));
    let tufts = smoothstep(0.5, 0.75, fbm(p * 0.9 + 31.0));
    let grassy = clamp(max(crown * mix(0.3, 1.0, tufts), verge * tufts), 0.0, 1.0) * (1.0 - worn);
    let shade = dot(color, LUMA) / max(dot(GRASS, LUMA), 1e-4);
    color = mix(color, GRASS * clamp(shade, 0.6, 1.4), 0.75 * grassy);

    var roughness = mix(pbr_input.material.perceptual_roughness, 0.72, worn);
    // Ground 102's own relief, already mapped by the standard material.
    var n = pbr_input.N;

    // Dry cracked mud in the low patches, and in the ruts where water stood.
    let low = smoothstep(0.58, 0.7, fbm(p * 0.045 + 13.0));
    let puddled = worn * smoothstep(0.5, 0.62, fbm(p * 0.11 + 5.0));
    let mud = max(low, puddled) * layers.x * (1.0 - grassy);
    if (mud > 0.0) {
        let c = textureSampleGrad(mud_color, mud_sampler, mud_uv, mud_dx, mud_dy).rgb;
        let orm = textureSampleGrad(mud_orm, mud_sampler, mud_uv, mud_dx, mud_dy).rgb;
        let y = max(dot(c, LUMA), 1e-4);
        let cracked = color * (mix(vec3<f32>(y), c, 0.4) / y) * pow(y / MUD_LUMA, 1.15);
        color = mix(color, cracked, mud);
        roughness = mix(roughness, orm.g, mud);
        pbr_input.diffuse_occlusion *= vec3<f32>(mix(1.0, orm.r, mud));
        @if(VERTEX_TANGENTS) {
            let tbn = calculate_tbn_mikktspace(pbr_input.world_normal, in.world_tangent);
            let mud_n = tbn * decode(textureSampleGrad(mud_normal, mud_sampler, mud_uv, mud_dx, mud_dy).rgb);
            n = normalize(mix(n, mud_n, mud * fade));
        }
    }

    // The ruts' walls and the crown tilt the normal across the road; the clods
    // roughen the hoof path.
    n = normalize(n - slope * across_grad - 0.12 * hooves * fade * vec3<f32>(clods.y, 0.0, clods.z));
    pbr_input.N = n;
    pbr_input.material.perceptual_roughness = clamp(roughness, 0.45, 1.0);

    // A wandering edge, fading into the worn ground, within the mesh's
    // feather.
    let feather = pbr_input.material.base_color.a;
    let edge = half + 0.45 * (noise(vec2<f32>(along * 0.12, sign(across) * 7.0)) - 0.5) * 2.0;
    let alpha = feather * (1.0 - smoothstep(edge - 0.35, edge + 0.35, abs(across)));
    pbr_input.material.base_color = vec4<f32>(color, alpha);

    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color,
    );

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
