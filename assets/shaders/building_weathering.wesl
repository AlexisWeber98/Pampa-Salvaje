// The weathering of the town and settlement buildings: Bevy's standard
// fragment shader, with each wall dirtied by where it stands. The building
// mesh's second UV carries the height of the point above the ground under the
// building (x, metres) and the building's weather (y): how worn it is in
// hundredths in the integer part (0 for fresh lime to 100 for decades without a
// coat) and how much damp rises in it in the fraction (0 for most buildings;
// halved on the street front). The material's uniform says how much of each
// kind of dirt its surface takes (`Surface::weathering` in
// `crates/render/src/facade/materials.rs`).
//
// Four kinds, placed where they happen on a 1919 pampa wall:
// - splash-back: rain bouncing off the bare earth of the streets and yards
//   throws mud up the foot of the wall, densest at the ground and petering out
//   in drops some 20-50 cm up, stronger on some stretches than others;
// - the dark foot: where the wall meets the ground it wicks the soil's damp;
// - rising damp, only where the building has it: the wall sucks ground water
//   up to an irregular, blotchy tide line, darker below, with a broken brown
//   mark and patches of pale salt where it dries out, more on the shaded
//   south-facing walls;
// - dust: the loess of the pampa settles on sills, cornices and ledges.
// A mud-and-straw wall (the surface's `breakup.w`) also gets its own network
// of shrinkage and settlement cracks, a rain-washed foot and patches where the
// last coat of mud fell off (`mud_cracks`).
// Rain streaks under sills and cornices are decals, not this shader
// (`crates/render/src/facade/grime.rs`). The noise is in world space, so every
// building and every stretch of wall gets its own pattern. Meshes without the
// second UV (none should use this material) are drawn as the plain standard
// material.

import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
};

// Splash-back, rising damp, the dark foot and dust, 0 to 1.
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> amounts: vec4<f32>;
// How much the surface's own tiling is broken up, 0 to 1: a macro variation of
// its tone, patches of a fresher tone (new mud daubed over old) and lumps that
// bend the light (a hand-packed mud wall, a thatch).
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var<uniform> breakup: vec4<f32>;

// Linear multipliers and albedos. The pampa's loess is a pale brown.
const MUD: vec3<f32> = vec3<f32>(0.42, 0.33, 0.24);
const DAMP: vec3<f32> = vec3<f32>(0.46, 0.41, 0.35);
const TIDE: vec3<f32> = vec3<f32>(0.50, 0.42, 0.33);
const SALT: vec3<f32> = vec3<f32>(0.60, 0.58, 0.53);
const DUST: vec3<f32> = vec3<f32>(0.40, 0.33, 0.25);

// A hash of a 2D point to [0, 1) (Dave Hoskins' hash12).
fn hash(p: vec2<f32>) -> f32 {
    var q = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

// Gradient (Perlin) noise, remapped to about [0, 1]: no grid in its blots,
// unlike value noise.
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

// A unit gradient for the lattice point `i`.
fn gradient(i: vec2<f32>) -> vec2<f32> {
    let angle = hash(i) * 6.2831853;
    return vec2<f32>(cos(angle), sin(angle));
}

// Three octaves of `noise`, each turned against the last, in about [0, 1].
fn fbm(p: vec2<f32>) -> f32 {
    let turn = mat2x2<f32>(0.8, 0.6, -0.6, 0.8);
    let q = turn * p * 2.03 + 17.1;
    let r = turn * q * 2.01 + 41.7;
    return (0.5 * noise(p) + 0.25 * noise(q) + 0.125 * noise(r)) / 0.875;
}

// Mud drops thrown up the wall: on a jittered grid of `size`-metre cells, one
// round drop in a share `density` of the cells, 1 inside it.
fn drops(p: vec2<f32>, size: f32, density: f32) -> f32 {
    let q = p / size;
    let cell = floor(q);
    let centre = vec2<f32>(hash(cell), hash(cell + 7.3)) * 0.6 + 0.2;
    let radius = 0.12 + 0.18 * hash(cell + 3.1);
    let inside = 1.0 - smoothstep(radius * 0.6, radius, length(fract(q) - centre));
    return inside * step(hash(cell + 11.9), density);
}

// The distance along a wall of normal `n` through `p`, in metres: the noise
// runs along it.
fn along_wall(p: vec3<f32>, n: vec3<f32>) -> f32 {
    let side = vec2<f32>(-n.z, n.x);
    return select(p.x + p.z, dot(p.xz, side / max(length(side), 1e-4)), length(side) > 0.05);
}

// The height of the lumps of a hand-packed surface at (`along`, `h`).
fn lumps(along: f32, h: f32) -> f32 {
    return fbm(vec2<f32>(along * 2.2, h * 2.2) + 23.0) + 0.4 * noise(vec2<f32>(along * 7.0, h * 7.0));
}

// The distance from `q` to the nearest border between the cells of a jittered
// grid (Inigo Quilez's Voronoi borders), and a hash of the nearest cell.
fn borders(q: vec2<f32>) -> vec2<f32> {
    let cell = floor(q);
    let f = fract(q);
    var nearest = vec2<f32>(0.0);
    var to_nearest = vec2<f32>(8.0);
    var best = 8.0;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let g = vec2<f32>(f32(i), f32(j));
            let o = vec2<f32>(hash(cell + g), hash(cell + g + 19.7)) * 0.8 + 0.1;
            let r = g + o - f;
            let d = dot(r, r);
            if (d < best) {
                best = d;
                to_nearest = r;
                nearest = cell + g;
            }
        }
    }
    var border = 8.0;
    for (var j = -2; j <= 2; j++) {
        for (var i = -2; i <= 2; i++) {
            let g = nearest - cell + vec2<f32>(f32(i), f32(j));
            let o = vec2<f32>(hash(cell + g), hash(cell + g + 19.7)) * 0.8 + 0.1;
            let r = g + o - f;
            let gap = r - to_nearest;
            if (dot(gap, gap) > 1e-5) {
                border = min(border, dot(0.5 * (to_nearest + r), normalize(gap)));
            }
        }
    }
    return vec2<f32>(border, hash(nearest + 5.1));
}

// A hand-packed mud-and-straw wall as the years left it, at `along` metres
// along the wall and `h` over the ground; `seed` moves the pattern for each
// building. Returns how dark the point is (0 none, 1 an open crack) in x,
// and the slope of that darkening along the wall and up it (for the normal)
// in y and z, and the wear (eroded, pitted mud) in w.
//
// Two crack networks, both in world space so no two walls share one: the
// fine shrinkage cracks of the drying mud, polygons of 15 to 40 cm, only
// along some of their borders and wider where the wall dried faster; and a
// few long settlement cracks that wander up the wall. The foot is washed
// and pitted by the rain bounced off the yard, and here and there the last
// coat of mud has fallen off in a patch.
fn mud_cracks(along: f32, h: f32, seed: f32) -> vec4<f32> {
    let p = vec2<f32>(along, h) + vec2<f32>(seed, seed * 0.37);
    let warp = vec2<f32>(fbm(p * 1.7 + 3.0), fbm(p * 1.7 + 9.0)) - 0.5;
    // Fine shrinkage cracks, a little taller than wide.
    let fine = borders((p + warp * 0.18) * vec2<f32>(4.2, 3.4));
    // Most of the wall stays whole: the cracks open where it dried fast,
    // more on some walls than on others.
    let density = 0.55 + 0.15 * (noise(p * 0.15 + seed) - 0.5);
    let open_fine = smoothstep(density, density + 0.12, fbm(p * 1.1 + 13.0));
    let width = mix(0.006, 0.022, noise(p * 3.0 + 1.0)) * open_fine;
    let fine_crack = (1.0 - smoothstep(width * 0.3, width, fine.x)) * open_fine;
    // Long settlement cracks: a coarse grid stretched up the wall.
    let coarse = borders((p + warp * 0.5) * vec2<f32>(0.9, 0.35));
    let long_open = smoothstep(0.6, 0.72, fbm(p * vec2<f32>(0.6, 0.25) + 41.0));
    let long_width = mix(0.008, 0.02, noise(p * 2.0 + 7.0));
    let long_crack = (1.0 - smoothstep(long_width * 0.3, long_width, coarse.x)) * long_open;
    let crack = max(fine_crack, long_crack);
    // Rain-washed, pitted foot, and patches where the last coat fell off.
    let foot = (1.0 - smoothstep(0.15, 0.55 + 0.25 * noise(p * vec2<f32>(1.5, 0.0)), h));
    let spall = smoothstep(0.62, 0.7, fbm(p * 1.1 + 27.0));
    let wear = max(foot * mix(0.4, 1.0, noise(p * 9.0)), spall);
    return vec4<f32>(crack, fine.y, coarse.y, wear);
}

// What the weathering does to a point: a multiplier on its colour (rgb), and
// how far it goes towards salt (in the tide line) and dust (on ledges), each an
// albedo it is mixed with.
struct Weather {
    tint: vec3<f32>,
    salt: f32,
    dust: f32,
}

fn weather(p: vec3<f32>, n: vec3<f32>, h: f32, wear: f32, damp: f32) -> Weather {
    var out: Weather;
    out.tint = vec3<f32>(1.0);
    // How much of a wall the face is (cornice tops and sills are not).
    let wall = 1.0 - smoothstep(0.3, 0.6, abs(n.y));
    let along = along_wall(p, n);

    // Splash-back.
    let top = 0.25 + 0.3 * noise(vec2<f32>(along * 0.8, 5.3));
    // A dense, uneven film at the foot, then single drops thinning out
    // upwards.
    let foot = (1.0 - smoothstep(0.0, top, h)) * mix(0.5, 1.0, noise(vec2<f32>(along * 3.0, h * 6.0)));
    let thinning = (1.0 - smoothstep(top * 0.4, top * 1.4, h)) * smoothstep(0.3, 0.6, noise(vec2<f32>(along * 1.7, 2.9)));
    let spots = max(drops(vec2<f32>(along, h), 0.035, 0.5), drops(vec2<f32>(along, h) + 0.4, 0.06, 0.3));
    let spatter = max(foot * foot, spots * thinning);
    // Stronger on some stretches and buildings than on others.
    let fling = mix(0.45, 1.15, noise(p.xz * 0.07 + 3.3));
    let splash = spatter * smoothstep(-0.3, 0.0, h) * amounts.x * wall * fling;
    out.tint *= mix(vec3<f32>(1.0), MUD, splash * mix(0.6, 1.0, wear));
    // Where the wall meets the ground it wicks the soil's damp: a dark foot.
    let contact = (1.0 - smoothstep(0.0, 0.16, h)) * smoothstep(-0.3, 0.0, h) * amounts.z * wall;
    out.tint *= mix(vec3<f32>(1.0), MUD * 0.8, contact * mix(0.5, 0.85, wear));

    // Rising damp, only on the buildings that have it (`damp` > 0, a
    // minority) and more on the shaded, south-facing walls of the southern
    // hemisphere (world +Z). Darkest at the foot, fading up to a tide line
    // whose height wanders at several scales and is warped by the stain
    // itself, so it reads as blotches, not a drawn line.
    // From a faint shadow to a marked stain: any damp at all reads.
    let strength = select(0.0, mix(0.45, 1.0, damp), damp > 0.001);
    let shade = mix(0.65, 1.0, smoothstep(-0.6, 0.6, n.z));
    let wet = strength * shade * amounts.y * wall * smoothstep(-0.3, 0.0, h);
    let reach = mix(0.4, 1.1, damp) * (0.45 + 0.6 * fbm(vec2<f32>(along * 0.45, 1.7))
        + 0.35 * fbm(vec2<f32>(along * 1.9, 4.3)));
    let warp = 0.35 * (fbm(vec2<f32>(along * 3.7, h * 3.3) + 11.0) - 0.5)
        + 0.15 * (noise(vec2<f32>(along * 9.0, h * 8.0)) - 0.5);
    let rise = clamp(h / max(reach, 0.05) + warp, 0.0, 1.6);
    let blotch = smoothstep(0.4, 0.6, fbm(vec2<f32>(along * 2.2, h * 2.6) + 5.0));
    let below = (1.0 - smoothstep(0.35, 1.0, rise)) * mix(0.3, 1.0, blotch);
    out.tint *= mix(vec3<f32>(1.0), DAMP, below * wet);
    // The drying front leaves a soft brownish mark under the line, broken
    // where the wall dried evenly, and the salt blooms just over it in
    // patches.
    let mark = (rise - 0.95) / 0.12;
    let unbroken = smoothstep(0.35, 0.65, fbm(vec2<f32>(along * 0.9, 7.0)));
    out.tint *= mix(vec3<f32>(1.0), TIDE, exp(-mark * mark) * unbroken * wet * 0.6);
    // The salt: a thin, crusty, mottled band only just above the line, where
    // the water evaporates, dim and broken.
    let band = smoothstep(0.98, 1.03, rise) * (1.0 - smoothstep(1.05, 1.15, rise));
    let crust = smoothstep(0.4, 0.8, fbm(vec2<f32>(along * 9.0, h * 9.0) + 2.0))
        * smoothstep(0.35, 0.65, fbm(vec2<f32>(along * 1.3, 9.1)));
    out.salt = band * crust * wet * 0.3;

    // Dust on the ledges above the splash.
    let ledge = smoothstep(0.7, 0.95, n.y) * smoothstep(0.25, 0.6, h);
    out.dust = ledge * amounts.w * mix(0.2, 0.5, wear) * mix(0.6, 1.0, noise(p.xz * 3.0));
    return out;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    @if(VERTEX_UVS_B) {
        // The building's wear in hundredths and its damp in the fraction.
        let wear = floor(in.uv_b.y) / 100.0;
        let damp = fract(in.uv_b.y);
        let w = weather(in.world_position.xyz, normalize(in.world_normal), in.uv_b.x, wear, damp);
        var color = pbr_input.material.base_color.rgb * w.tint;
        // Break the scan's tiling: a slow variation of its tone, and patches
        // of a fresher, yellower tone.
        let p = in.world_position.xyz;
        let n = normalize(in.world_normal);
        let along = along_wall(p, n);
        let slow = fbm(vec2<f32>(along * 0.3, in.uv_b.x * 0.45) + p.xz * 0.05);
        color *= mix(1.0, mix(0.8, 1.15, slow), breakup.x);
        let fresh = smoothstep(0.6, 0.68, fbm(vec2<f32>(along * 0.5, in.uv_b.x * 0.7) + 31.0));
        color = mix(color, color * vec3<f32>(1.18, 1.1, 0.9), fresh * breakup.y);
        // Lumps: tilt the normal down the slope of the lumps' height.
        if (breakup.z > 0.0) {
            let e = 0.04;
            let h = in.uv_b.x;
            let d_along = (lumps(along + e, h) - lumps(along - e, h)) / (2.0 * e);
            let d_up = (lumps(along, h + e) - lumps(along, h - e)) / (2.0 * e);
            let side = normalize(vec3<f32>(-n.z, 0.0, n.x) + vec3<f32>(1e-4, 0.0, 0.0));
            pbr_input.N = normalize(pbr_input.N - (side * d_along + vec3<f32>(0.0, 1.0, 0.0) * d_up) * 0.07 * breakup.z);
        }
        // The mud-and-straw wall's own cracks and wear (breakup.w).
        if (breakup.w > 0.0) {
            // The noise runs in world space, so every wall has its own
            // pattern; each building's wear and damp shift it further.
            let seed = fract(in.uv_b.y * 7.31) * 97.0;
            let m = mud_cracks(along, in.uv_b.x, seed);
            let wall_face = 1.0 - smoothstep(0.3, 0.6, abs(n.y));
            let crack = m.x * breakup.w * wall_face;
            // Open cracks are deep and dark; the eroded mud is darker and
            // browner where the straw-bound inner coat shows.
            color *= mix(vec3<f32>(1.0), vec3<f32>(0.42, 0.36, 0.3), crack);
            color *= mix(vec3<f32>(1.0), vec3<f32>(0.86, 0.8, 0.72), m.w * breakup.w * wall_face);
            if (breakup.z > 0.0) {
                // The cracks bend the light at their lips: tilt the normal
                // into them, and pit the worn mud.
                let e = 0.01;
                let ca = mud_cracks(along + e, in.uv_b.x, seed).x - m.x;
                let cu = mud_cracks(along, in.uv_b.x + e, seed).x - m.x;
                let side = normalize(vec3<f32>(-n.z, 0.0, n.x) + vec3<f32>(1e-4, 0.0, 0.0));
                let pits = noise(vec2<f32>(along, in.uv_b.x) * 40.0) - 0.5;
                pbr_input.N = normalize(pbr_input.N + (side * ca + vec3<f32>(0.0, 1.0, 0.0) * cu) * 2.0 * wall_face
                    + side * pits * 0.25 * m.w * wall_face);
            }
        }
        color = mix(color, SALT, w.salt);
        color = mix(color, DUST, w.dust);
        pbr_input.material.base_color = vec4<f32>(color, pbr_input.material.base_color.a);
    }

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
