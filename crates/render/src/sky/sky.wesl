// La cúpula del cielo: degradado, crepúsculo, sol, luna con sus fases,
// estrellas, la Vía Láctea, la Cruz del Sur, cúmulos y cirros, todo en un solo
// fragment shader. Los colores son lineales. La cúpula se dibuja invertida
// alrededor de la cámara; el color de su horizonte es el color de la niebla,
// así que el suelo lejano se funde con ella.

import bevy_pbr::render::forward_io::VertexOutput;

struct Sky {
    // xyz: dirección hacia el sol; w: qué tan visible es su resplandor (0..1).
    sun: vec4<f32>,
    // xyz: dirección hacia la luna; w: qué tan visible es su disco (0..1).
    moon: vec4<f32>,
    // xyz: dirección desde la luna hacia su luz; w: luz cenicienta.
    moon_light: vec4<f32>,
    // rgb: color en lo alto; w: desvanecimiento de las estrellas (0 de día).
    zenith: vec4<f32>,
    // rgb: color en el horizonte (la niebla); w: intensidad del crepúsculo.
    horizon: vec4<f32>,
    // rgb: color de la luz del sol; w: luz del día.
    sun_color: vec4<f32>,
    // rgb: color de las nubes al sol; w: cobertura de cúmulos (0..1).
    cloud_lit: vec4<f32>,
    // rgb: color de las nubes a la sombra; w: segundos reales.
    cloud_shade: vec4<f32>,
    // xy: desplazamiento de las nubes en unidades de ruido; zw: dirección
    // unitaria del viento.
    drift: vec4<f32>,
    // Filas que llevan una dirección del mundo a coordenadas galácticas.
    star_x: vec4<f32>,
    star_y: vec4<f32>,
    star_z: vec4<f32>,
    // xyz: dónde está la cámara.
    center: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> sky: Sky;
// El mapa de estrellas: un mapa equirrectangular en coordenadas galácticas
// (Solar System Scope, CC BY 4.0). La longitud crece hacia la izquierda; la
// fila superior es la latitud -90.
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var star_map: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var star_sampler: sampler;

// Qué tan brillante se ve el mapa: es oscuro, para verse bajo un cielo
// nocturno.
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

    // El mapa de estrellas, muestreado en flujo de control uniforme. La
    // longitud da la vuelta, así que las derivadas salen de la más suave de dos
    // copias sin costura.
    let g = vec3<f32>(dot(sky.star_x.xyz, d), dot(sky.star_y.xyz, d), dot(sky.star_z.xyz, d));
    let u = 0.5 - atan2(g.y, g.x) / 6.2831853;
    let v = 0.5 + asin(clamp(g.z, -1.0, 1.0)) / 3.1415927;
    let u_alt = fract(u + 0.5);
    let ddx_u = select(dpdx(u_alt), dpdx(u), abs(dpdx(u)) <= abs(dpdx(u_alt)));
    let ddy_u = select(dpdy(u_alt), dpdy(u), abs(dpdy(u)) <= abs(dpdy(u_alt)));
    let stars = textureSampleGrad(star_map, star_sampler, vec2<f32>(u, v), vec2<f32>(ddx_u, dpdx(v)), vec2<f32>(ddy_u, dpdy(v))).rgb;

    // Degradado: bruma en el horizonte que sube hacia el azul del cenit.
    var col = mix(sky.zenith.rgb, horizon, pow(1.0 - up, 3.6));

    // Crepúsculo: naranja donde se puso el sol, el cinturón rosado del lado
    // opuesto y una franja violeta encima.
    let sun_h = normalize(sky.sun.xz + vec2<f32>(0.0001, 0.0));
    let d_h = normalize(d.xz + vec2<f32>(0.0001, 0.0));
    let toward = smoothstep(-0.3, 1.0, dot(d_h, sun_h));
    let low_band = exp(-up * 5.0);
    col += twilight * toward * toward * low_band * vec3<f32>(1.0, 0.40, 0.16) * 0.8;
    col = mix(col, vec3<f32>(0.62, 0.30, 0.40), twilight * (1.0 - toward) * 0.38 * exp(-pow((up - 0.09) / 0.10, 2.0)));
    col = mix(col, vec3<f32>(0.20, 0.12, 0.36), twilight * toward * 0.30 * smoothstep(0.08, 0.35, up) * (1.0 - smoothstep(0.35, 0.9, up)));

    // Las estrellas y la Vía Láctea, detrás de todo lo demás: ninguna de día, y
    // aparecen después de que el sol está bien por debajo del horizonte (aquí,
    // la propia altura del sol, además del desvanecimiento de la CPU, para que
    // no puedan contradecirse).
    let star_vis = min(star_fade, 1.0 - smoothstep(-0.3146, -0.06, sky.sun.y));
    if star_vis > 0.002 && d.y > 0.0 {
        let moon_wash = 1.0 - 0.55 * sky.moon.w * pow(max(dot(d, sky.moon.xyz), 0.0), 6.0);
        col += stars * STAR_GAIN * star_vis * smoothstep(0.0, 0.2, d.y) * moon_wash;
    }

    // Resplandor y disco del sol.
    let mu = dot(d, sky.sun.xyz);
    let sun_vis = sky.sun.w;
    let glow = 0.16 * pow(max(mu, 0.0), 5.0) + 0.55 * pow(max(mu, 0.0), 40.0) + 0.9 * pow(max(mu, 0.0), 400.0);
    col += sky.sun_color.rgb * glow * sun_vis * (0.6 + 0.4 * twilight);
    let disc = smoothstep(cos(SUN_RADIUS * 1.15), cos(SUN_RADIUS * 0.85), mu) * above * sun_vis;
    col = mix(col, vec3<f32>(1.0, 0.96, 0.85) * 1.6 + sky.sun_color.rgb * 0.2, disc);

    // Nubes: cúmulos iluminados desde el lado del sol, y estrías de cirros por
    // encima.
    let wind = sky.drift.zw;
    let across = vec2<f32>(-wind.y, wind.x);
    let fade = smoothstep(0.02, 0.24, d.y);
    var cloud_alpha = 0.0;
    if d.y > 0.01 {
        let plane = d.xz / (d.y + 0.10);
        let p = plane * 0.55 + sky.drift.xy;
        let n = fbm4(p);
        let cover = sky.cloud_lit.w;
        // `cover` es aproximadamente la proporción del cielo bajo cúmulos.
        let edge = 0.72 - 0.9 * cover;
        let body = smoothstep(edge, edge + 0.12, n);
        let toward_sun = sun_h * 0.07;
        let n2 = fbm4(p + toward_sun);
        let lit = clamp(0.55 + (n - n2) * 7.0, 0.0, 1.0);
        let core = smoothstep(edge + 0.06, edge + 0.22, n);
        let sunlit = clamp(lit * (1.0 - 0.45 * core) + 0.25 * (1.0 - core), 0.0, 1.0);
        var cloud = mix(sky.cloud_shade.rgb, sky.cloud_lit.rgb, sunlit);
        // Atardecer: los lados y las caras inferiores que miran al sol brillan.
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

    // La luna con su fase, delante de las estrellas y detrás de las nubes.
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

    // Por debajo del horizonte la cúpula es bruma lisa, como la niebla.
    col = mix(horizon, col, above);
    return vec4<f32>(col, 1.0);
}
