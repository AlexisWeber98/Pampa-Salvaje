// The terrain's fragment shader: Bevy's standard one, with the ground drawn
// from two photographed layers instead of the standard material's maps.
//
// - Grass (Poly Haven grass_ground, CC0, 2.5 m a tile) and bare soil (Poly
//   Haven dry_mud_field_001, CC0, 3 m a tile), each colour, normal and an ORM
//   map whose blue channel is the scan's height (`textures_to_ktx2.sh` with
//   `height.jpg`).
// - Each layer is sampled at offsets picked by a slow noise, twice only across
//   the seams between them (Inigo Quilez's texture-repetition technique), so
//   neither repeats every few metres. `textureSampleGrad` keeps the mip level of the
//   continuous UV across the offset jumps.
// - Soil shows through where the ground is cleared (towns, road verges, lagoon
//   mud: the mesh's second UV, `x`) and in blots over the open pampa, more of
//   them where the grass is dry. The two layers meet by their heights: grass
//   holds on in the soil's hollows, clods break through the thin grass.
// - A slow variation of tone, and centimetre hummocks that tilt the normal
//   and darken their hollows, break the flatness of the 5 m mesh.
// - Cost on an integrated GPU: a layer that cannot show is not read, and the
//   normal maps and hummocks fade out a few tens of metres from the eye.
// - The vertex colour still sets the grass tone (dryness, slope shade), and
//   its alpha the leaf litter under the monte, as before.
//
// Without the layer maps (`layers.x` is 0) the standard material draws the
// plain vertex-painted ground, with the litter on top.

import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, calculate_tbn_mikktspace, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    mesh_view_bindings::view,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var litter_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var litter_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var grass_color: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var layer_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(104) var grass_normal: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(105) var grass_orm: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(106) var soil_color: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(107) var soil_normal: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(108) var soil_orm: texture_2d<f32>;
// x: 1 when the layer maps are loaded.
@group(constants::MATERIAL_BIND_GROUP) @binding(109) var<uniform> layers: vec4<f32>;

// Litter tiles per ground UV tile (`LITTER_TILING` in `ground.rs`).
const LITTER_TILING: f32 = 3.0;
// Metres a tile of each scan covers (Poly Haven's stated dimensions).
const GRASS_TILE: f32 = 2.5;
const SOIL_TILE: f32 = 3.0;
// Mean linear luma of each colour map, so the layers multiply as ratios.
const GRASS_LUMA: f32 = 0.123;
const SOIL_LUMA: f32 = 0.109;
// How much of each scan's own hue survives: little for the grass, whose tone
// the vertex colour sets; more for the soil, a real pale loess.
const GRASS_SATURATION: f32 = 0.35;
const SOIL_SATURATION: f32 = 0.45;
// How much each scan's own light and shade is deepened: blades and their
// shadows, clods and cracks, as crisp as they look from a horse.
const GRASS_CONTRAST: f32 = 1.35;
const SOIL_CONTRAST: f32 = 1.2;
// Mean height and spread of each scan's height map.
const GRASS_HEIGHT: vec2<f32> = vec2<f32>(0.34, 0.15);
const SOIL_HEIGHT: vec2<f32> = vec2<f32>(0.42, 0.25);
// Bare loess of the open pampa (linear), lit by the vertex colour's own shade.
const EARTH: vec3<f32> = vec3<f32>(0.27, 0.225, 0.165);
// The linear luma of the mid grass tone, against which that shade is read
// (between `GREEN_GRASS` and `DRY_GRASS` in `crates/render/src/terrain.rs`).
const GRASS_REFERENCE: f32 = 0.28;
// The vertex paint's red over green, from the green pampa to the dry one
// (`GREEN_GRASS` and `DRY_GRASS` in terrain.rs, linear): how dry the grass is.
const DRY_RATIO: vec2<f32> = vec2<f32>(0.81, 1.30);
// Half the width of the edge between grass and soil, in the blend's units.
const BLEND_EDGE: f32 = 0.2;
// The seam between a layer's two offsets: where it crosses (`SEAM.x` to
// `SEAM.y` of the patch fraction), and the band outside it where one offset
// alone is read. The crossing's own shift (a tenth of the colours'
// difference) stays inside that band for scan colours.
const SEAM: vec2<f32> = vec2<f32>(0.3, 0.7);
const SEAM_BAND: f32 = 0.05;
// The hummocks' two scales: amplitude in metres and frequency per metre.
const HUMMOCK_BIG: vec2<f32> = vec2<f32>(0.035, 0.8);
const HUMMOCK_SMALL: vec2<f32> = vec2<f32>(0.012, 2.6);
// Where the normal maps and hummocks fade out, in metres from the eye.
const DETAIL_NEAR: f32 = 30.0;
const DETAIL_FAR: f32 = 45.0;
// How far the layers' heights can move the edge between them.
const HEIGHT_PUSH: f32 = 0.5;
const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

// A hash of a 2D point to [0, 1) (Dave Hoskins' hash12).
fn hash(p: vec2<f32>) -> f32 {
    var q = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

// A unit gradient for the lattice point `i`.
fn gradient(i: vec2<f32>) -> vec2<f32> {
    let angle = hash(i) * 6.2831853;
    return vec2<f32>(cos(angle), sin(angle));
}

// Gradient (Perlin) noise, remapped to about [0, 1].
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let a = dot(gradient(i), f);
    let b = dot(gradient(i + vec2<f32>(1.0, 0.0)), f - vec2<f32>(1.0, 0.0));
    let c = dot(gradient(i + vec2<f32>(0.0, 1.0)), f - vec2<f32>(0.0, 1.0));
    let d = dot(gradient(i + vec2<f32>(1.0, 1.0)), f - vec2<f32>(1.0, 1.0));
    return clamp(mix(mix(a, b, u.x), mix(c, d, u.x), u.y) * 0.75 + 0.5, 0.0, 1.0);
}

// `noise` with its gradient: (value, d/dx, d/dy), unclamped.
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

// Three octaves of `noise`, each turned against the last, in about [0, 1].
fn fbm(p: vec2<f32>) -> f32 {
    let turn = mat2x2<f32>(0.8, 0.6, -0.6, 0.8);
    let q = turn * p * 2.03 + 17.1;
    let r = turn * q * 2.01 + 41.7;
    return (0.5 * noise(p) + 0.25 * noise(q) + 0.125 * noise(r)) / 0.875;
}

// The hummocks, a few centimetres over half a metre to a metre and a half:
// (height in metres, its slope).
fn hummocks(p: vec2<f32>) -> vec3<f32> {
    let big = noise_d(p * HUMMOCK_BIG.y + 3.7);
    let small = noise_d(p * HUMMOCK_SMALL.y + 11.3);
    return vec3<f32>(
        HUMMOCK_BIG.x * big.x + HUMMOCK_SMALL.x * small.x,
        HUMMOCK_BIG.x * HUMMOCK_BIG.y * big.yz + HUMMOCK_SMALL.x * HUMMOCK_SMALL.y * small.yz,
    );
}

// Where to sample a layer: the two offsets of its UV and how far across from
// the first to the second, from a slow noise over the ground.
struct Patch {
    a: vec2<f32>,
    b: vec2<f32>,
    f: f32,
}

fn patch_at(p: vec2<f32>, seed: f32) -> Patch {
    let k = noise(p * 0.19 + seed) * 8.0;
    let i = floor(k);
    return Patch(
        sin(vec2<f32>(3.0, 7.0) * (i + seed)),
        sin(vec2<f32>(3.0, 7.0) * (i + 1.0 + seed)),
        fract(k),
    );
}

// What a layer gives at one point: sampled at one offset, blended across a
// seam, or a placeholder for a layer that is not read (`no_layer`).
struct Layer {
    color: vec3<f32>,
    // Tangent-space normal.
    normal: vec3<f32>,
    ao: f32,
    roughness: f32,
    height: f32,
}

fn decode(n: vec3<f32>) -> vec3<f32> {
    let xy = n.xy * 2.0 - 1.0;
    return vec3<f32>(xy, sqrt(max(1.0 - dot(xy, xy), 0.0)));
}

const FLAT: vec3<f32> = vec3<f32>(0.0, 0.0, 1.0);

// One layer at one offset; the normal map is read only with `detail`.
fn sample_layer(
    color_map: texture_2d<f32>,
    normal_map: texture_2d<f32>,
    orm_map: texture_2d<f32>,
    uv: vec2<f32>,
    ddx: vec2<f32>,
    ddy: vec2<f32>,
    detail: bool,
) -> Layer {
    let color = textureSampleGrad(color_map, layer_sampler, uv, ddx, ddy).rgb;
    var normal = FLAT;
    if (detail) {
        normal = decode(textureSampleGrad(normal_map, layer_sampler, uv, ddx, ddy).rgb);
    }
    let orm = textureSampleGrad(orm_map, layer_sampler, uv, ddx, ddy).rgb;
    return Layer(color, normal, orm.r, orm.g, orm.b);
}

// One layer, sampled at both offsets only across their seam; elsewhere one
// offset alone, so most of the ground reads each map once.
fn layer(
    color_map: texture_2d<f32>,
    normal_map: texture_2d<f32>,
    orm_map: texture_2d<f32>,
    uv: vec2<f32>,
    ddx: vec2<f32>,
    ddy: vec2<f32>,
    at: Patch,
    detail: bool,
) -> Layer {
    if (at.f < SEAM.x - SEAM_BAND) {
        return sample_layer(color_map, normal_map, orm_map, uv + at.a, ddx, ddy, detail);
    }
    if (at.f > SEAM.y + SEAM_BAND) {
        return sample_layer(color_map, normal_map, orm_map, uv + at.b, ddx, ddy, detail);
    }
    let a = sample_layer(color_map, normal_map, orm_map, uv + at.a, ddx, ddy, detail);
    let b = sample_layer(color_map, normal_map, orm_map, uv + at.b, ddx, ddy, detail);
    // Cross where the two samples look most alike, not on a straight line.
    let w = smoothstep(SEAM.x, SEAM.y, at.f - 0.1 * dot(a.color - b.color, vec3<f32>(1.0)));
    return Layer(
        mix(a.color, b.color, w),
        normalize(mix(a.normal, b.normal, w)),
        mix(a.ao, b.ao, w),
        mix(a.roughness, b.roughness, w),
        mix(a.height, b.height, w),
    );
}

// A layer that is not drawn: what `layer` gives where the other covers all.
fn no_layer(height: f32) -> Layer {
    return Layer(vec3<f32>(0.0), FLAT, 1.0, 1.0, height);
}

// A colour map as a ratio around 1: its own hue kept by `saturation`, its
// light and shade deepened by `contrast`.
fn ratio(c: vec3<f32>, mean: f32, saturation: f32, contrast: f32) -> vec3<f32> {
    let y = max(dot(c, LUMA), 1e-4);
    let hue = mix(vec3<f32>(y), c, saturation) / y;
    return hue * pow(y / mean, contrast);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let p = in.world_position.xz;

    // Derivatives first, in uniform control flow.
    let grass_uv = p / GRASS_TILE;
    let soil_uv = p / SOIL_TILE;
    let grass_dx = dpdx(grass_uv);
    let grass_dy = dpdy(grass_uv);
    let soil_dx = dpdx(soil_uv);
    let soil_dy = dpdy(soil_uv);

    var paint = pbr_input.material.base_color.rgb;
    var litter_weight = 0.0;
    @if(VERTEX_COLORS) {
        litter_weight = in.color.a;
    }

    if (layers.x > 0.5) {
        // x: cleared ground; y: wet ground.
        var cover = vec2<f32>(0.0);
        @if(VERTEX_UVS_B) {
            cover = in.uv_b;
        }
        let cleared = clamp(cover.x, 0.0, 1.0);
        let wet = clamp(cover.y, 0.0, 1.0);
        // Normal maps and hummocks only near the eye, faded out between
        // `DETAIL_NEAR` and `DETAIL_FAR`: past that they are finer than a
        // pixel.
        let fade = 1.0 - smoothstep(DETAIL_NEAR, DETAIL_FAR, distance(in.world_position.xyz, view.world_position));
        let detail = fade > 0.0;

        // Dry grass is yellower: its red over green rises from about 0.8
        // (green pampa) to 1.3 (dry).
        let dry = clamp((paint.r / max(paint.g, 1e-3) - DRY_RATIO.x) / (DRY_RATIO.y - DRY_RATIO.x), 0.0, 1.0);
        // Bare blots in the open pampa, a few metres across.
        let blots = fbm(p * 0.33 + 7.0);
        let open = smoothstep(0.66 - 0.1 * dry, 0.8 - 0.1 * dry, blots);
        // Cleared ground is bare but for tufts that hold on along its edges.
        var mask = open;
        if (cleared > 0.0) {
            let tufts = smoothstep(0.3, 0.5, fbm(p * 0.45 + 23.0));
            mask = max(open, cleared * mix(0.75, 1.0, tufts));
        }
        // The layers meet by height (below), which moves the edge by at most
        // `HEIGHT_PUSH`: past that only one layer shows and the other is not
        // read.
        let edge = (mask - 0.5) * 2.0;
        var grass = no_layer(GRASS_HEIGHT.x);
        var soil = no_layer(SOIL_HEIGHT.x);
        if (edge < BLEND_EDGE + HEIGHT_PUSH) {
            grass = layer(grass_color, grass_normal, grass_orm, grass_uv, grass_dx, grass_dy, patch_at(p, 0.0), detail);
        }
        if (edge > -BLEND_EDGE - HEIGHT_PUSH) {
            soil = layer(soil_color, soil_normal, soil_orm, soil_uv, soil_dx, soil_dy, patch_at(p, 5.3), detail);
        }
        // Meet by height: each scan's height, normalised, pushes its layer.
        let hg = (grass.height - GRASS_HEIGHT.x) / GRASS_HEIGHT.y;
        let hs = (soil.height - SOIL_HEIGHT.x) / SOIL_HEIGHT.y;
        let push = clamp(0.3 * (hs - hg), -HEIGHT_PUSH, HEIGHT_PUSH);
        let s = smoothstep(-BLEND_EDGE, BLEND_EDGE, edge + push);

        let shade = sqrt(clamp(dot(paint, LUMA) / GRASS_REFERENCE, 0.4, 1.6));
        let earth = mix(EARTH * shade, paint, cleared);
        // Tussocks a metre or two across: greener ones among the straw.
        let tussock = noise(p * 0.55 + 41.0);
        let grass_tint = mix(vec3<f32>(1.05, 1.0, 0.86), vec3<f32>(0.9, 1.03, 1.0), tussock);
        let grass_albedo = paint * grass_tint * ratio(grass.color, GRASS_LUMA, GRASS_SATURATION, GRASS_CONTRAST);
        let soil_albedo = earth * ratio(soil.color, SOIL_LUMA, SOIL_SATURATION, SOIL_CONTRAST) * mix(1.0, 0.62, wet);
        var color = mix(grass_albedo, soil_albedo, s);

        // A slow variation of tone, so the scans never read as one carpet.
        color *= mix(0.88, 1.1, fbm(p * 0.045 + 2.0));
        color *= vec3<f32>(1.0, mix(0.97, 1.03, noise(p * 0.11 + 9.0)), 1.0);

        // Hummocks: their slope tilts the normal, their hollows hold shade.
        var hollow = 0.5;
        if (detail) {
            let hummock = hummocks(p);
            hollow = mix(0.5, clamp(hummock.x / (HUMMOCK_BIG.x + HUMMOCK_SMALL.x), 0.0, 1.0), fade);
            let normal = normalize(mix(FLAT, mix(grass.normal, soil.normal, s), fade));
            @if(VERTEX_TANGENTS) {
                let tbn = calculate_tbn_mikktspace(pbr_input.world_normal, in.world_tangent);
                pbr_input.N = normalize(tbn * normal);
            }
            pbr_input.N = normalize(pbr_input.N - fade * vec3<f32>(hummock.y, 0.0, hummock.z));
        }

        pbr_input.material.base_color = vec4<f32>(color, pbr_input.material.base_color.a);
        let roughness = mix(grass.roughness, soil.roughness, s);
        pbr_input.material.perceptual_roughness = clamp(mix(roughness, 0.45, wet), 0.45, 1.0);
        let ao = mix(grass.ao, soil.ao, s) * mix(0.85, 1.0, hollow);
        pbr_input.diffuse_occlusion *= vec3<f32>(ao);
        paint = color;
    }

    let litter = textureSample(litter_texture, litter_sampler, in.uv * LITTER_TILING).rgb;
    pbr_input.material.base_color = vec4<f32>(
        paint * mix(vec3<f32>(1.0), litter, litter_weight),
        pbr_input.material.base_color.a,
    );

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
