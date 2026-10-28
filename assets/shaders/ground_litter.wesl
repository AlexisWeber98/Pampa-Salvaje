// The terrain's fragment shader: Bevy's standard one, with leaf litter under
// the monte. The vertex colour's alpha is the litter weight (0 in the open
// pampa, on roads and in towns, 1 in the heart of a monte patch), so it
// feathers across the triangles. The litter tile is a brightness-and-hue
// ratio toned in `ground.rs`; the ground colour (detail texture times vertex
// paint) is multiplied by it, then lit as usual. One extra texture sample.

import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var litter_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var litter_sampler: sampler;

// Litter tiles per ground tile (`LITTER_TILING` in `ground.rs`).
const LITTER_TILING: f32 = 3.0;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let litter = textureSample(litter_texture, litter_sampler, in.uv * LITTER_TILING).rgb;
    pbr_input.material.base_color = vec4<f32>(
        pbr_input.material.base_color.rgb * mix(vec3<f32>(1.0), litter, in.color.a),
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
