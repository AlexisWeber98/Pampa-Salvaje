//! Poses de partes rígidas para personas: cuánto gira cada articulación en un
//! punto del ciclo de la marcha. El render rota las partes de una figura
//! low-poly según estos ángulos; la lógica nunca los necesita. Los caballos son
//! modelos animados: aquí solo se calcula su marcha ([`HorseGait`]).
//!
//! Los ángulos están en radianes alrededor del eje lateral del cuerpo,
//! positivos cuando el miembro que está debajo de la articulación se balancea
//! hacia adelante. Las personas no galopan: su [`AnimationState::Gallop`] es un
//! trote.

use crate::AnimationState;

/// Ángulos de las articulaciones y desplazamientos del cuerpo de una pose.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct JointAngles {
    /// Balanceo de la cadera (muslo), izquierda y derecha.
    pub hip: [f32; 2],
    /// Flexión de la rodilla, izquierda y derecha: la pierna se pliega hacia
    /// atrás (siempre `<= 0`).
    pub knee: [f32; 2],
    /// Balanceo del hombro (brazo), izquierdo y derecho.
    pub shoulder: [f32; 2],
    /// Flexión del codo, izquierdo y derecho: el antebrazo se pliega hacia
    /// adelante (siempre `>= 0`).
    pub elbow: [f32; 2],
    /// Inclinación del torso hacia adelante.
    pub lean: f32,
    /// Elevación de todo el cuerpo en metros (el rebote de la zancada).
    pub bob: f32,
    /// Elevación del torso en metros al respirar.
    pub breath: f32,
    /// Cabeceo (positivo hacia adelante) que estabiliza la mirada.
    pub nod: f32,
    /// Inclinación del sombrero, positiva con el ala hacia abajo por delante:
    /// el ala acompaña el paso.
    pub hat: f32,
    /// Balanceo adelante-atrás de un poncho, un chal o una pollera, positivo
    /// con el ruedo hacia adelante.
    pub drape: f32,
    /// Balanceo lateral de lo mismo, positivo con el ruedo hacia la derecha.
    pub drape_side: f32,
}

/// Respiraciones por segundo estando parado.
pub const BREATHS_PER_SECOND: f32 = 0.25;

/// Amplitud del balanceo (cadera, hombro, pliegue de rodilla, pliegue de codo),
/// inclinación, rebote.
fn gait(animation: AnimationState) -> (f32, f32, f32, f32, f32, f32) {
    match animation {
        AnimationState::Idle => (0.0, 0.0, 0.0, 0.08, 0.0, 0.0),
        AnimationState::Walk => (0.42, 0.38, 0.55, 0.15, 0.03, 0.025),
        AnimationState::Gallop => (0.5, 0.75, 1.1, 1.25, 0.2, 0.06),
    }
}

/// La pose de `animation` en `phase` (ciclos, ajustados a `0..1`).
///
/// Las piernas se balancean en fase opuesta, cada brazo contra la pierna de su
/// lado, y el cuerpo se eleva dos veces por ciclo, una por paso. Una pierna que
/// camina lleva su pie hacia atrás a ritmo parejo mientras está en el suelo
/// ([`foot_sweep`]); una que trota se balancea de forma pareja. En reposo solo
/// respira, igual de ambos lados.
pub fn pose(animation: AnimationState, phase: f32) -> JointAngles {
    use std::f32::consts::TAU;
    let phase = wrap(phase);
    let angle = phase * TAU;
    let (swing, arm, fold, elbow, lean, bob) = gait(animation);
    let (hat, drape, drape_side) = drape(animation, phase);
    if animation == AnimationState::Idle {
        return JointAngles {
            elbow: [elbow; 2],
            breath: 0.012 * angle.sin(),
            nod: 0.02 * angle.sin(),
            hat,
            drape,
            drape_side,
            ..JointAngles::default()
        };
    }
    let (s, c) = angle.sin_cos();
    let hip = |phase: f32| match animation {
        AnimationState::Walk => (foot_sweep(phase) * swing.sin()).asin(),
        _ => swing * (phase * TAU).sin(),
    };
    JointAngles {
        hip: [hip(phase), hip(phase + 0.5)],
        // Una pierna se pliega mientras se balancea hacia adelante (cuando sube
        // el ángulo de su cadera).
        knee: [-fold * c.max(0.0), -fold * (-c).max(0.0)],
        shoulder: [-arm * s, arm * s],
        elbow: [elbow + 0.1 * (-s).max(0.0), elbow + 0.1 * s.max(0.0)],
        lean,
        bob: bob * s.abs(),
        breath: 0.0,
        nod: -0.04 * (2.0 * angle).cos(),
        hat,
        drape,
        drape_side,
    }
}

/// Dónde está el pie de una pierna que camina, de `1` adelante a `-1` atrás, en
/// el punto `phase` de su ciclo (el de la pierna izquierda; la derecha va medio
/// ciclo adelantada). Desde adelante, en `0.25`, va hacia atrás a ritmo parejo
/// durante medio ciclo, de modo que un pie apoyado en el suelo se queda quieto
/// sobre él, y después vuelve suavemente hacia adelante.
pub fn foot_sweep(phase: f32) -> f32 {
    let p = wrap(phase - 0.25);
    if p < 0.5 {
        1.0 - 4.0 * p
    } else {
        let t = 2.0 * p - 1.0;
        -1.0 + 2.0 * t * t * (3.0 - 2.0 * t)
    }
}

/// Ciclos de marcha por segundo a velocidad `speed` sobre el suelo (m/s): un
/// ciclo son dos pasos, así que los pies acompañan al suelo. Parado, respira a
/// un ritmo fijo.
pub fn cadence(animation: AnimationState, speed: f32) -> f32 {
    let speed = if speed.is_finite() { speed.abs() } else { 0.0 };
    match animation {
        AnimationState::Idle => BREATHS_PER_SECOND,
        // Dos pasos de unos 0,7 m: el pie apoyado en el suelo se desplaza hacia
        // atrás tanto como avanza el cuerpo en un paso.
        AnimationState::Walk => speed.max(0.8) / 1.4,
        // Dos pasos con rebote de unos 1,35 m, con un momento en el aire: el
        // pie toca el suelo a mitad del balanceo y va hacia atrás tan rápido
        // como avanza el cuerpo.
        AnimationState::Gallop => speed.max(2.0) / 2.7,
    }
}

/// `phase` avanzada `dt` segundos de `animation` a velocidad `speed`, ajustada
/// a `0..1`.
pub fn advance(phase: f32, animation: AnimationState, speed: f32, dt: f32) -> f32 {
    let step = cadence(animation, speed) * dt.max(0.0);
    wrap(phase + step)
}

/// Cómo se mueve un caballo. La lógica solo conoce [`AnimationState`]; el
/// caballo trota en la parte más lenta de su rango de galope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum HorseGait {
    /// Parado: respirando, con el peso cargado sobre tres patas.
    #[default]
    Idle,
    /// El paso, de cuatro tiempos.
    Walk,
    /// El trote, de dos tiempos: pares diagonales de patas juntas.
    Trot,
    /// El galope, con un momento de suspensión.
    Gallop,
}

impl HorseGait {
    /// Todas las marchas, de la más lenta a la más rápida.
    pub const ALL: [Self; 4] = [Self::Idle, Self::Walk, Self::Trot, Self::Gallop];

    /// Velocidad (m/s) por encima de la cual un caballo en su rango de galope
    /// galopa en lugar de trotar.
    pub const TROT_MAX: f32 = 6.5;

    /// La marcha para el `animation` de la lógica a velocidad `speed` sobre el
    /// suelo (m/s).
    pub fn of(animation: AnimationState, speed: f32) -> Self {
        match animation {
            AnimationState::Idle => Self::Idle,
            AnimationState::Walk => Self::Walk,
            // No finita: la marcha más lenta.
            AnimationState::Gallop if speed.is_finite() && speed.abs() > Self::TROT_MAX => {
                Self::Gallop
            }
            AnimationState::Gallop => Self::Trot,
        }
    }
}

/// Movimiento secundario: cuánto se balancean el sombrero, un poncho o una
/// pollera en el punto `phase` de la `animation` de una persona (ver
/// [`JointAngles::hat`]). Van con retraso respecto del paso, así que se
/// balancean después del cuerpo.
pub fn drape(animation: AnimationState, phase: f32) -> (f32, f32, f32) {
    use std::f32::consts::TAU;
    let angle = wrap(phase) * TAU;
    // Amplitudes del ala, adelante-atrás y lateral.
    let (brim, fore, side) = match animation {
        AnimationState::Idle => return (0.0, 0.012 * angle.sin(), 0.0),
        AnimationState::Walk => (0.04, 0.07, 0.04),
        AnimationState::Gallop => (0.09, 0.15, 0.08),
    };
    (
        brim * (2.0 * angle - 0.6).sin(),
        fore * (2.0 * angle - 1.0).sin(),
        side * (angle - 0.8).sin(),
    )
}

/// Un hash entero barato para momentos deterministas.
fn mix(mut h: u32) -> u32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^ (h >> 16)
}

/// El reloj del movimiento secundario ([`glance`]) vuelve a cero después de
/// esta cantidad de segundos (7,7 h): un múltiplo de cada ciclo que usa
/// (6–10 s), así que cada uno empieza un ciclo nuevo, en reposo, justo al volver a
/// cero, y un reloj `f32` nunca pierde precisión.
pub const MOTION_CLOCK_PERIOD: f32 = 27_720.0;

/// El `clock` del movimiento secundario avanzado `dt` segundos, ajustado en
/// [`MOTION_CLOCK_PERIOD`]. Un reloj que no es finito vuelve a empezar en 0; un
/// paso que no es finito (o que es negativo) se ignora.
pub fn tick_clock(clock: f32, dt: f32) -> f32 {
    let clock = if clock.is_finite() { clock } else { 0.0 };
    let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
    (clock + dt).rem_euclid(MOTION_CLOCK_PERIOD)
}

/// En qué punto cae `time` dentro de una serie de ciclos de `period` segundos
/// de una `seed`, cada uno con un evento de `length` segundos en un momento
/// determinado por hash: el progreso `0..1` del evento mientras dura, y el hash
/// del ciclo.
fn moment(seed: u32, time: f32, period: f32, length: f32) -> Option<(f32, u32)> {
    if !time.is_finite() || time < 0.0 {
        return None;
    }
    let cycle = (time / period).floor();
    let h = mix(seed ^ mix(cycle as u32).wrapping_mul(0x9e37_79b9));
    let start = (h % 1000) as f32 / 1000.0 * (period - length);
    let s = (time - cycle * period - start) / length;
    (0.0..1.0).contains(&s).then_some((s, h))
}

/// Sube suavemente de 0 a 1 durante el primer tramo `edge` de `0..1`, y baja
/// durante el último.
fn plateau(s: f32, edge: f32) -> f32 {
    let smooth = |x: f32| {
        let x = x.clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    smooth(s / edge).min(smooth((1.0 - s) / edge))
}

/// Cuánto mira hacia un costado una persona en reposo (en radianes, positivo a
/// la izquierda) a los `time` segundos: de vez en cuando, de forma determinista
/// según `seed`, una mirada hacia un lado que entra suavemente, se sostiene y
/// vuelve suavemente.
pub fn glance(seed: u32, time: f32) -> f32 {
    let period = 6.0 + (mix(seed) % 5) as f32;
    let Some((s, h)) = moment(seed, time, period, 2.4) else {
        return 0.0;
    };
    let side = if h & 0x10000 == 0 { 1.0 } else { -1.0 };
    let amount = 0.3 + 0.25 * ((h >> 20) % 100) as f32 / 100.0;
    side * amount * plateau(s, 0.3)
}

/// `phase` ajustada a `0..1`, incluidas las fases negativas (`-0.25` es
/// `0.75`); cualquier valor que no sea finito es `0`.
fn wrap(phase: f32) -> f32 {
    let wrapped = phase.rem_euclid(1.0);
    // Una fase negativa muy pequeña se redondea hasta exactamente 1.0.
    if wrapped.is_finite() && wrapped < 1.0 {
        wrapped
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use AnimationState::*;

    fn samples() -> impl Iterator<Item = f32> {
        (0..40).map(|i| i as f32 / 40.0)
    }

    #[test]
    fn idle_is_symmetric_and_only_breathes() {
        let mut breathes = false;
        for t in samples() {
            let p = pose(Idle, t);
            assert_eq!(p.hip[0], p.hip[1]);
            assert_eq!(p.knee[0], p.knee[1]);
            assert_eq!(p.shoulder[0], p.shoulder[1]);
            assert_eq!(p.elbow[0], p.elbow[1]);
            assert_eq!(
                (p.hip[0], p.shoulder[0], p.bob, p.lean),
                (0.0, 0.0, 0.0, 0.0)
            );
            breathes |= p.breath.abs() > 0.005;
        }
        assert!(breathes, "the chest rises and falls");
    }

    #[test]
    fn walking_legs_swing_in_opposite_phase_and_arms_against_them() {
        let mut forward = [false; 2];
        for t in samples() {
            let p = pose(Walk, t);
            assert!(
                (p.hip[1] - pose(Walk, t + 0.5).hip[0]).abs() < 1e-6,
                "t {t}: {p:?}"
            );
            assert!(p.hip[0] * p.hip[1] <= 0.0, "t {t}: {p:?}");
            assert!(p.hip[0] * p.shoulder[0] <= 0.0, "left arm against left leg");
            assert!(
                p.hip[1] * p.shoulder[1] <= 0.0,
                "right arm against right leg"
            );
            assert!(p.knee.iter().all(|k| *k <= 0.0) && p.elbow.iter().all(|e| *e >= 0.0));
            assert!(p.bob >= 0.0);
            for (stepped, hip) in forward.iter_mut().zip(p.hip) {
                *stepped |= hip > 0.3;
            }
        }
        assert_eq!(forward, [true, true], "each leg steps forward");
        // La cabeza rebota: dos elevaciones por ciclo.
        assert!(pose(Walk, 0.25).bob > 0.02 && pose(Walk, 0.0).bob < 1e-6);
    }

    #[test]
    fn jogging_swings_wider_and_leans_forward() {
        let peak = |a| {
            samples()
                .map(|t| pose(a, t).hip[0].abs())
                .fold(0.0, f32::max)
        };
        assert!(peak(Gallop) > peak(Walk) + 0.05);
        assert!(pose(Gallop, 0.1).lean > pose(Walk, 0.1).lean);
        let bob = |a| samples().map(|t| pose(a, t).bob).fold(0.0, f32::max);
        assert!(bob(Gallop) > bob(Walk));
    }

    #[test]
    fn a_walking_foot_sweeps_back_evenly_while_its_leg_is_straight() {
        // Durante medio ciclo con la rodilla estirada, el pie (adelantado
        // `sin(hip)` del largo de la pierna) se mueve hacia atrás lo mismo en
        // cada momento.
        let at = |t: f32| pose(Walk, t).hip[0].sin();
        let mut straight = 0;
        for k in 0..200 {
            let t = k as f32 / 200.0;
            if [t - 0.01, t, t + 0.01]
                .iter()
                .any(|&t| pose(Walk, t).knee[0] < 0.0)
            {
                continue;
            }
            straight += 1;
            let (a, b, c) = (at(t - 0.01), at(t), at(t + 0.01));
            assert!(b < a && ((a - b) - (b - c)).abs() < 1e-4, "t {t}");
        }
        assert!(straight > 80, "{straight}");
    }

    #[test]
    fn a_foot_on_the_ground_keeps_pace_with_it() {
        // La pierna de una persona, de la cadera al tobillo, aproximadamente.
        const LEG: f32 = 0.85;
        for speed in [1.2, 6.0 / 3.6] {
            // Al caminar, una pierna estirada lleva al cuerpo a lo largo de un
            // paso entero: su pie va de un extremo del balanceo al otro.
            let swing = pose(Walk, 0.25).hip[0];
            let sweep = 2.0 * LEG * swing.sin();
            let step = speed / cadence(Walk, speed) / 2.0;
            assert!((sweep / step - 1.0).abs() < 0.06, "{sweep} {step}");
        }
        for speed in [3.0, 18.0 / 3.6] {
            // Al trotar, el pie toca el suelo a mitad del balanceo, cuando va
            // hacia atrás más rápido.
            let swing = pose(Gallop, 0.25).hip[0];
            let foot = swing * LEG * std::f32::consts::TAU * cadence(Gallop, speed);
            assert!((foot / speed - 1.0).abs() < 0.06, "{foot} {speed}");
        }
    }

    #[test]
    fn the_phase_advances_with_speed_whatever_the_frame_rate() {
        // Proporcional a la velocidad por encima de la zancada más lenta.
        let walk = |s| cadence(Walk, s);
        assert!((walk(2.4) - 2.0 * walk(1.2)).abs() < 1e-6);
        let jog = |s| cadence(Gallop, s);
        assert!((jog(6.0) - 2.0 * jog(3.0)).abs() < 1e-6);
        assert_eq!(cadence(Idle, 5.0), BREATHS_PER_SECOND);

        // Diez frames de 10 ms llegan a donde llega un frame de 100 ms.
        let mut small = 0.3;
        for _ in 0..10 {
            small = advance(small, Walk, 1.5, 0.01);
        }
        let big = advance(0.3, Walk, 1.5, 0.1);
        assert!((small - big).abs() < 1e-5, "{small} vs {big}");
        let wrapped = advance(0.9, Gallop, 4.0, 0.5);
        assert!((0.0..1.0).contains(&wrapped));
        assert_eq!(advance(0.4, Walk, 1.5, -1.0), 0.4, "time never runs back");
        // Una fase negativa también se ajusta a 0..1, al mismo punto del ciclo.
        let from_negative = advance(-0.25, Walk, 1.5, 0.0);
        assert!((from_negative - 0.75).abs() < 1e-6, "{from_negative}");
        assert!((0.0..1.0).contains(&advance(-3.6, Gallop, 4.0, 0.1)));
        assert!((0.0..1.0).contains(&advance(-1e-9, Walk, 1.5, 0.0)));
        assert_eq!(pose(Walk, -0.75), pose(Walk, 0.25));
    }

    #[test]
    fn the_motion_clock_wraps_seamlessly_and_never_loses_precision() {
        // Un reloj que queda corriendo durante semanas se mantiene chico y
        // sigue avanzando.
        let late = tick_clock(1.0e9, 0.016);
        assert!((0.0..MOTION_CLOCK_PERIOD).contains(&late), "{late}");
        assert!((tick_clock(late, 0.25) - late - 0.25).abs() < 1e-2);
        assert!((tick_clock(MOTION_CLOCK_PERIOD - 0.01, 0.02) - 0.01).abs() < 1e-2);
        // La mirada está en reposo al volver a cero, así que no hay saltos sea
        // cual sea la semilla.
        for seed in 0..64 {
            for t in [MOTION_CLOCK_PERIOD - 0.01, 0.0, 0.01] {
                assert!(glance(seed, t).abs() < 0.02, "{seed} {t}");
            }
        }
    }

    #[test]
    fn the_horse_trots_in_the_slow_part_of_its_gallop_range() {
        assert_eq!(HorseGait::of(Idle, 0.0), HorseGait::Idle);
        assert_eq!(HorseGait::of(Walk, 2.0), HorseGait::Walk);
        assert_eq!(HorseGait::of(Gallop, 4.0), HorseGait::Trot);
        assert_eq!(HorseGait::of(Gallop, -4.0), HorseGait::Trot);
        assert_eq!(HorseGait::of(Gallop, 11.0), HorseGait::Gallop);
    }

    fn times() -> impl Iterator<Item = f32> {
        (0..3000).map(|i| i as f32 * 0.02)
    }

    #[test]
    fn hats_and_drapes_follow_the_step_within_bounds() {
        for animation in [Idle, Walk, Gallop] {
            for t in samples() {
                let (hat, fore, side) = drape(animation, t);
                assert!(hat.abs() <= 0.12 && fore.abs() <= 0.2 && side.abs() <= 0.12);
                let next = drape(animation, t + 1.0);
                assert!(
                    (next.1 - fore).abs() < 1e-5 && (next.0 - hat).abs() < 1e-5,
                    "cyclic"
                );
                let p = pose(animation, t);
                assert_eq!((p.hat, p.drape, p.drape_side), (hat, fore, side));
            }
        }
        let swing = |a| samples().map(|t| drape(a, t).1.abs()).fold(0.0, f32::max);
        assert!(swing(Walk) > 0.03 && swing(Gallop) > swing(Walk));
        let brim = |a| samples().map(|t| drape(a, t).0.abs()).fold(0.0, f32::max);
        assert!(brim(Walk) > 0.02 && brim(Idle) == 0.0);
        // Va con retraso respecto del paso: la pollera no está en su punto
        // máximo cuando la pierna sí lo está.
        assert!(drape(Walk, 0.25).1.abs() < swing(Walk) * 0.95);
    }

    #[test]
    fn idle_people_glance_around_now_and_then() {
        for seed in [0, 7, 1234] {
            let looks: Vec<f32> = times().map(|t| glance(seed, t)).collect();
            assert!(looks.iter().all(|g| g.abs() <= 0.6), "bounded");
            let aside = looks.iter().filter(|g| g.abs() > 0.2).count();
            let ahead = looks.iter().filter(|g| g.abs() < 0.01).count();
            assert!(aside > 0, "seed {seed} glances");
            assert!(ahead > looks.len() / 2, "seed {seed} mostly looks ahead");
            assert_eq!(looks, times().map(|t| glance(seed, t)).collect::<Vec<_>>());
            // Suave: no hay saltos entre frames de 20 ms.
            assert!(looks.windows(2).all(|w| (w[1] - w[0]).abs() < 0.08));
        }
        let a: Vec<f32> = times().map(|t| glance(1, t)).collect();
        let b: Vec<f32> = times().map(|t| glance(2, t)).collect();
        assert_ne!(a, b, "people glance at their own moments");
    }
}
