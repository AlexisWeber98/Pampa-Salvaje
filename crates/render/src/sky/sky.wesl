// The sky dome: gradient, twilight, sun, moon with phases, stars, the Milky
// Way, the Southern Cross, cumulus and cirrus, all in one fragment shader.
// Colors are linear. The dome is drawn inside out around the camera; its
// horizon color is the fog color, so distant ground melts into it.

import bevy_pbr::render::forward_io::VertexOutput;

struct Sky {
    // xyz: direction toward the sun; w: how visible its glow is (0..1).
    sun: vec4<f32>,
    // xyz: direction toward the moon; w: how visible its disc is (0..1).
    moon: vec4<f32>,
    // xyz: direction from the moon toward its light; w: earthshine.
    moon_light: vec4<f32>,
    // rgb: color overhead; w: star fade (0 by day).
    zenith: vec4<f32>,
    // rgb: color at the horizon (the fog); w: twilight strength.
    horizon: vec4<f32>,
    // rgb: color of the sunlight; w: daylight.
    sun_color: vec4<f32>,
    // rgb: sunlit cloud color; w: cumulus coverage (0..1).
    cloud_lit: vec4<f32>,
    // rgb: cloud shade color; w: real seconds.
    cloud_shade: vec4<f32>,
    // xy: cloud drift in noise units; zw: unit wind direction.
    drift: vec4<f32>,
    // Rows taking a world direction to galactic coordinates.
    star_x: vec4<f32>,
    star_y: vec4<f32>,
    star_z: vec4<f32>,
    // xyz: where the camera is.
    center: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> sky: Sky;
// The star map: an equirectangular map in galactic coordinates (Solar System
// Scope, CC BY 4.0). Longitude grows to the left; the top row is latitude -90.
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var star_map: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var star_sampler: sampler;

// How bright the map shows: it is dark, to be seen under a night sky.
const STAR_GAIN: f32 = 5.0;

const SUN_RADIUS: f32 = 0.017;
const MOON_RADIUS: f32 = 0.016;

fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(123.34, 456.21));
    q += dot(q, q + 45.32);
    return fract(q.x * q.y);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm4(p_in: vec2<f32>) -> f32 {
    var p = p_in;
    var sum = 0.0;
    var amp = 0.5;
    for (var i = 0; i < 4; i++) {
        sum += amp * value_noise(p);
        p = p * 2.03 + vec2<f32>(17.1, 9.2);
        amp *= 0.5;
    }
    return sum;
}

fn fbm3(p_in: vec2<f32>) -> f32 {
    var p = p_in;
    var sum = 0.0;
    var amp = 0.5;
    for (var i = 0; i < 3; i++) {
        sum += amp * value_noise(p);
        p = p * 2.1 + vec2<f32>(5.3, 1.7);
        amp *= 0.5;
    }
    return sum;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let d = normalize(in.world_position.xyz - sky.center.xyz);
    let up = clamp(d.y, 0.0, 1.0);
    let horizon = sky.horizon.rgb;
    let twilight = sky.horizon.w;
    let star_fade = sky.zenith.w;
    let time = sky.cloud_shade.w;
    let above = smoothstep(-0.01, 0.02, d.y);

    // The star map, sampled in uniform control flow. The longitude wraps, so
    // the derivatives come from whichever of two seamless copies is smoother.
    let g = vec3<f32>(dot(sky.star_x.xyz, d), dot(sky.star_y.xyz, d), dot(sky.star_z.xyz, d));
    let u = 0.5 - atan2(g.y, g.x) / 6.2831853;
    let v = 0.5 + asin(clamp(g.z, -1.0, 1.0)) / 3.1415927;
    let u_alt = fract(u + 0.5);
    let ddx_u = select(dpdx(u_alt), dpdx(u), abs(dpdx(u)) <= abs(dpdx(u_alt)));
    let ddy_u = select(dpdy(u_alt), dpdy(u), abs(dpdy(u)) <= abs(dpdy(u_alt)));
    let stars = textureSampleGrad(star_map, star_sampler, vec2<f32>(u, v), vec2<f32>(ddx_u, dpdx(v)), vec2<f32>(ddy_u, dpdy(v))).rgb;

    // Gradient: haze at the horizon rising into the zenith's blue.
    var col = mix(sky.zenith.rgb, horizon, pow(1.0 - up, 3.6));

    // Twilight: orange where the sun went down, the pink belt opposite, a
    // violet band above.
    let sun_h = normalize(sky.sun.xz + vec2<f32>(0.0001, 0.0));
    let d_h = normalize(d.xz + vec2<f32>(0.0001, 0.0));
    let toward = smoothstep(-0.3, 1.0, dot(d_h, sun_h));
    let low_band = exp(-up * 5.0);
    col += twilight * toward * toward * low_band * vec3<f32>(1.0, 0.40, 0.16) * 0.8;
    col = mix(col, vec3<f32>(0.62, 0.30, 0.40), twilight * (1.0 - toward) * 0.38 * exp(-pow((up - 0.09) / 0.10, 2.0)));
    col = mix(col, vec3<f32>(0.20, 0.12, 0.36), twilight * toward * 0.30 * smoothstep(0.08, 0.35, up) * (1.0 - smoothstep(0.35, 0.9, up)));

    // Stars and the Milky Way, behind everything else: none by day, in
    // after the sun is well under the horizon (the sun's own height here,
    // as well as the CPU's fade, so they cannot disagree).
    let star_vis = min(star_fade, 1.0 - smoothstep(-0.3146, -0.06, sky.sun.y));
    if star_vis > 0.002 && d.y > 0.0 {
        let moon_wash = 1.0 - 0.55 * sky.moon.w * pow(max(dot(d, sky.moon.xyz), 0.0), 6.0);
        col += stars * STAR_GAIN * star_vis * smoothstep(0.0, 0.2, d.y) * moon_wash;
    }

    // Sun glow and disc.
    let mu = dot(d, sky.sun.xyz);
    let sun_vis = sky.sun.w;
    let glow = 0.16 * pow(max(mu, 0.0), 5.0) + 0.55 * pow(max(mu, 0.0), 40.0) + 0.9 * pow(max(mu, 0.0), 400.0);
    col += sky.sun_color.rgb * glow * sun_vis * (0.6 + 0.4 * twilight);
    let disc = smoothstep(cos(SUN_RADIUS * 1.15), cos(SUN_RADIUS * 0.85), mu) * above * sun_vis;
    col = mix(col, vec3<f32>(1.0, 0.96, 0.85) * 1.6 + sky.sun_color.rgb * 0.2, disc);

    // Clouds: cumulus lit from the sun's side, cirrus streaks above them.
    let wind = sky.drift.zw;
    let across = vec2<f32>(-wind.y, wind.x);
    let fade = smoothstep(0.02, 0.24, d.y);
    var cloud_alpha = 0.0;
    if d.y > 0.01 {
        let plane = d.xz / (d.y + 0.10);
        let p = plane * 0.55 + sky.drift.xy;
        let n = fbm4(p);
        let cover = sky.cloud_lit.w;
        // `cover` is about the share of the sky under cumulus.
        let edge = 0.72 - 0.9 * cover;
        let body = smoothstep(edge, edge + 0.12, n);
        let toward_sun = sun_h * 0.07;
        let n2 = fbm4(p + toward_sun);
        let lit = clamp(0.55 + (n - n2) * 7.0, 0.0, 1.0);
        let core = smoothstep(edge + 0.06, edge + 0.22, n);
        let sunlit = clamp(lit * (1.0 - 0.45 * core) + 0.25 * (1.0 - core), 0.0, 1.0);
        var cloud = mix(sky.cloud_shade.rgb, sky.cloud_lit.rgb, sunlit);
        // Sunset: the sun-facing sides and undersides glow.
        cloud += vec3<f32>(1.0, 0.42, 0.15) * twilight * toward * toward * (1.0 - core * 0.6) * 0.7;
        let a = body * fade;
        col = mix(col, cloud, a);
        cloud_alpha = a;

        let q = d.xz / (d.y + 0.04);
        let s = vec2<f32>(dot(q, wind) * 0.30, dot(q, across) * 1.9) + sky.drift.xy * 0.5;
        let streak = smoothstep(0.58, 0.88, fbm3(s));
        let ca = streak * 0.32 * smoothstep(0.03, 0.30, d.y);
        col = mix(col, sky.cloud_lit.rgb * 1.05 + vec3<f32>(0.9, 0.4, 0.2) * twilight * toward * 0.4, ca);
        cloud_alpha = max(cloud_alpha, ca);
    }

    // Moon with its phase, in front of the stars, behind the clouds.
    let m = sky.moon.xyz;
    let mc = dot(d, m);
    if mc > 0.9 {
        let r = normalize(cross(m, vec3<f32>(0.0, 1.0, 0.0)));
        let u = cross(r, m);
        let x = dot(d, r) / MOON_RADIUS;
        let y = dot(d, u) / MOON_RADIUS;
        let q = x * x + y * y;
        let vis = sky.moon.w * above;
        let halo = pow(max(mc, 0.0), 120.0) * 0.22 * vis * (1.0 - cloud_alpha * 0.6);
        col += vec3<f32>(0.62, 0.68, 0.90) * halo;
        if q < 1.0 {
            let z = sqrt(1.0 - q);
            let normal = r * x + u * y - m * z;
            let lit = smoothstep(-0.03, 0.08, dot(normal, sky.moon_light.xyz));
            let maria = 0.62 + 0.38 * fbm3(vec2<f32>(x, y) * 2.4 + vec2<f32>(3.0, 7.0));
            let shade = lit + sky.moon_light.w * (1.0 - lit);
            let disc_col = vec3<f32>(1.0, 0.96, 0.84) * maria * shade;
            let coverage = smoothstep(1.0, 0.9, q) * vis * (1.0 - cloud_alpha * 0.75);
            col = mix(col, disc_col, coverage);
        }
    }

    // Below the horizon the dome is plain haze, like the fog.
    col = mix(horizon, col, above);
    return vec4<f32>(col, 1.0);
}
