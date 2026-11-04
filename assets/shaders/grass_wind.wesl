// Grass cards swaying in the wind: Bevy's standard mesh vertex shader, with
// the tip of each card leaned downwind.
//
// - The second UV says how much a vertex moves: `x` from 0 at the base to 1 at
//   the tip, `y` its height above the base in mesh units, turned into metres
//   by the entity's scale.
// - The lean is a fraction of that height (`wind.z` in a full gust). Gusts a
//   few tens of metres long roll across the field downwind, a slow swell
//   rises and falls under them, and a quick small flutter rides on top.
// - The tip also drops a little, so a leaning card keeps about its length.
// - With `wind.w` set (the near grass), tips sink to the ground over the last
//   `SINK` metres before `wind.w` metres from the eye, so the near carpet
//   ends without a dithered edge.
//
// Only the main pass sways: every prepass (shadows, and depth, normal or
// motion-vector prepasses) keeps the still cards. Shadows are off by a few
// centimetres at most. The game's camera has no depth prepass; adding one
// (for SSAO, TAA or deferred) would need a matching prepass vertex shader,
// or the swaying cards would fail the depth test against their still copy.

import bevy_pbr::render::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput, decompress_vertex},
    view_transformations::position_world_to_clip,
    mesh_view_bindings::{globals, view},
};

// x, y: the wind's heading on the world's ground plane (world x, world z);
// z: the lean of a tip in a full gust, as a fraction of its height;
// w: where the grass has sunk to the ground, in metres from the eye (0: never).
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> wind: vec4<f32>;

// Over how many metres before `wind.w` the tips sink.
const SINK: f32 = 12.0;
// How fast the gusts roll downwind, in metres a second.
const GUST_SPEED: f32 = 6.0;
// Gusts: about 30 m from one crest to the next downwind, their fronts bent
// over about 70 m across the wind.
const GUST_WAVE: f32 = 0.21;
const GUST_BEND: vec2<f32> = vec2<f32>(1.3, 0.09);
// The swell under them: some 110 m from crest to crest, rising and falling
// every 18 s.
const SWELL_WAVE: vec2<f32> = vec2<f32>(0.047, 0.031);
const SWELL_RATE: f32 = 0.35;
// The flutter on top: about 0.8 times a second (5.1 rad/s), a tenth of a
// gust.
const FLUTTER_RATE: f32 = 5.1;
const FLUTTER_WAVE: f32 = 0.6;
const FLUTTER: f32 = 0.12;
// The lean between gusts, as a share of a full gust.
const CALM: f32 = 0.35;

// How far a tip at `p` (world x, z) leans at time `t`, as a fraction of its
// height.
fn lean(p: vec2<f32>, t: f32) -> f32 {
    let along = dot(p, wind.xy);
    let across = dot(p, vec2<f32>(-wind.y, wind.x));
    let gust = 0.5 + 0.5 * sin((along - t * GUST_SPEED) * GUST_WAVE + GUST_BEND.x * sin(across * GUST_BEND.y));
    let swell = 0.5 + 0.5 * sin(along * SWELL_WAVE.x + across * SWELL_WAVE.y - t * SWELL_RATE);
    let flutter = FLUTTER * sin(t * FLUTTER_RATE + (along + across) * FLUTTER_WAVE);
    return wind.z * (CALM + (1.0 - CALM) * gust * mix(0.6, 1.0, swell) + flutter);
}

@vertex
fn vertex(vertex_in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let vertex = decompress_vertex(vertex_in, vertex_in.instance_index);
    let world_from_local = mesh_functions::get_world_from_local(vertex_in.instance_index);

    @if(VERTEX_NORMALS) {
        out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex_in.instance_index);
    }

    var world = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    @if(VERTEX_UVS_B) {
        let height = vertex.uv_b.y * length(world_from_local[1].xyz);
        let bend = lean(world.xz, globals.time) * vertex.uv_b.x;
        var sunk = 0.0;
        if (wind.w > 0.0) {
            sunk = smoothstep(wind.w - SINK, wind.w, distance(world.xyz, view.world_position));
        }
        world = vec4<f32>(
            world.x + wind.x * bend * height,
            world.y - (0.5 * bend * bend + sunk * vertex.uv_b.x) * height,
            world.z + wind.y * bend * height,
            world.w,
        );
    }
    out.world_position = world;
    out.position = position_world_to_clip(world.xyz);

    @if(VERTEX_UVS_A) {
        out.uv = vertex.uv;
    }
    @if(VERTEX_UVS_B) {
        out.uv_b = vertex.uv_b;
    }
    @if(VERTEX_TANGENTS) {
        out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, vertex.tangent, vertex_in.instance_index);
    }
    @if(VERTEX_COLORS) {
        out.color = vertex.color;
    }
    @if(VERTEX_OUTPUT_INSTANCE_INDEX) {
        out.instance_index = vertex_in.instance_index;
    }
    @if(VISIBILITY_RANGE_DITHER) {
        out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
            vertex_in.instance_index, world_from_local[3]);
    }
    return out;
}
