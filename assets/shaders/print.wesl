// A short-lived print in the earth (crates/render/src/prints.rs), multiplied
// over the ground under it. The print cannot know the ground's colour, so it
// only says how much darker each spot is than flat ground in the same light:
// the print's relief (its normal map) is lit as the ground would be, and
// divided by the light on a flat patch here. The wall facing away from the sun
// falls into shade; the one facing it stays as bright as flat ground (an 8-bit
// target cannot brighten under a multiply). The baked colour adds what no light
// direction explains: the deep floor and the crumbs.
import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::apply_pbr_lighting,
};

// The darkest a flat-lit spot may get against flat ground: keeps a wall turned
// fully away from a low sun from going black.
const FLOOR: f32 = 0.35;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let shade = pbr_input.material.base_color;
    // White albedo: the ratio below is light only, whatever the texture.
    pbr_input.material.base_color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
    let relief = apply_pbr_lighting(pbr_input).rgb;
    pbr_input.N = normalize(pbr_input.world_normal);
    let flat = apply_pbr_lighting(pbr_input).rgb;
    let light = dot(relief, vec3<f32>(0.2126, 0.7152, 0.0722))
        / max(dot(flat, vec3<f32>(0.2126, 0.7152, 0.0722)), 1e-5);
    let c = shade.rgb * clamp(light, FLOOR, 1.0);
    // The multiply blend is dst * src + (1 - src_alpha) * dst: premultiplied.
    var out: FragmentOutput;
    out.color = vec4<f32>(c * shade.a, shade.a);
    return out;
}
