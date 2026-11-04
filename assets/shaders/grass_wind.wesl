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
// Only the main pass sways: shadows keep the still cards, a few centimetres
// off at most.

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
const SINK: f32 = 8.0;
// How fast the gusts roll downwind, in metres a second.
const GUST_SPEED: f32 = 6.0;

// How far a tip at `p` (world x, z) leans at time `t`, as a fraction of its
// height.
fn lean(p: vec2<f32>, t: f32) -> f32 {
    let along = dot(p, wind.xy);
    let across = dot(p, vec2<f32>(-wind.y, wind.x));
    let gust = 0.5 + 0.5 * sin((along - t * GUST_SPEED) * 0.21 + 1.3 * sin(across * 0.09));
    let swell = 0.5 + 0.5 * sin(along * 0.047 + across * 0.031 - t * 0.35);
    let flutter = 0.12 * sin(t * 5.1 + (along + across) * 0.6);
    return wind.z * (0.35 + 0.65 * gust * mix(0.6, 1.0, swell) + flutter);
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
