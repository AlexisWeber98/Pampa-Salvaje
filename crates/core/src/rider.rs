//! Modelo cinemático del jinete (caballo + forajido).

use crate::{AnimationState, Body, Circle, Ground, Heading, StaticObstacles, Travel, Vec2, World};

/// El paso más largo que el juego le pasa a la simulación, en segundos, para
/// que un tirón nunca teletransporte nada. De todos modos, [`Rider::step`]
/// sigue siendo seguro con pasos más largos.
pub const MAX_STEP: f32 = 1.0 / 20.0;

/// Límite de subpasos de colisión por paso, para que un `dt` enorme no pueda
/// trabar un frame.
const MAX_SUBSTEPS: usize = 32;

/// Ajuste del modelo cinemático del jinete. Velocidades en m/s, tasas por
/// segundo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RiderParams {
    /// Velocidad de giro en radianes por segundo con la entrada de giro al
    /// máximo.
    pub turn_rate: f32,
    /// Aceleración hacia adelante con el acelerador a fondo.
    pub acceleration: f32,
    /// Desaceleración al acelerar en contra del movimiento actual.
    pub braking: f32,
    /// Desaceleración pasiva sin acelerar, o por encima de la velocidad máxima
    /// de la marcha.
    pub friction: f32,
    pub walk_max: f32,
    pub gallop_max: f32,
    pub reverse_max: f32,
}

impl Default for RiderParams {
    fn default() -> Self {
        Self {
            turn_rate: 1.8,
            acceleration: 4.0,
            braking: 8.0,
            friction: 3.0,
            walk_max: 3.0,
            // 49 km/h (usuario, 2026-10-29; antes era 44).
            gallop_max: 49.0 / 3.6,
            reverse_max: 1.2,
        }
    }
}

/// Intención del jugador o de la IA para un paso de simulación.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RiderInput {
    /// `-1.0..=1.0`: acelerador hacia adelante; los valores negativos frenan y
    /// después hacen retroceder.
    pub throttle: f32,
    /// `-1.0..=1.0`: los valores positivos giran a la izquierda (sentido
    /// antihorario).
    pub turn: f32,
    /// Mantener presionado para permitir velocidades de galope.
    pub gallop: bool,
}

/// Estado lógico del jinete.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rider {
    pub position: Vec2,
    /// Altura del suelo bajo el jinete ([`World::ground_height`]: el terreno o
    /// un andén), sincronizada a medida que se mueve.
    pub height: f32,
    pub heading: Heading,
    /// Velocidad con signo en la dirección de la orientación, en m/s.
    pub speed: f32,
    pub collider: Circle,
    /// A qué altura está un caballo que salta por encima de [`Self::height`],
    /// en metros; 0 en el suelo.
    pub air: f32,
    /// Velocidad hacia arriba durante el salto, en m/s (negativa al bajar); 0
    /// en el suelo.
    pub lift: f32,
}

impl Rider {
    /// Radio de colisión de un caballo con su jinete visto desde arriba, en
    /// metros.
    pub const COLLIDER_RADIUS: f32 = 0.9;

    /// Lo más lejos que se mueve el jinete en un subpaso de colisión, en
    /// metros.
    ///
    /// Un círculo no puede atravesar un rectángulo mientras cada movimiento sea
    /// más corto que su radio más el medio ancho del rectángulo; la mitad del
    /// radio del jinete deja un margen amplio incluso para el obstáculo más
    /// delgado (un tronco de árbol de 0,7 m).
    pub const MAX_SUBSTEP_DISTANCE: f32 = Self::COLLIDER_RADIUS * 0.5;

    /// Velocidad hacia arriba con la que despega un caballo, en m/s: el arco
    /// alcanza su punto más alto a unos 1,55 m, por encima de los alambres de
    /// un alambrado ([`crate::FenceLayout::HEIGHT`]). El usuario pidió el
    /// 2026-10-29 un salto un 4 % más alto que los 5,4 m/s (1,49 m) iniciales.
    pub const JUMP_SPEED: f32 = 5.51;

    /// Aceleración hacia abajo en el aire, en m/s².
    pub const GRAVITY: f32 = 9.81;

    /// Fracción de la velocidad máxima del galope ([`RiderParams::gallop_max`])
    /// a la que debe ir un caballo para saltar: solo a galope tendido (85 %,
    /// elección del usuario el 2026-10-29).
    pub const FULL_GALLOP: f32 = 0.85;

    /// Cuántos subpasos de colisión necesita un paso de `dt` segundos para que
    /// ningún subpaso avance más que [`Self::MAX_SUBSTEP_DISTANCE`].
    pub fn substeps(&self, params: &RiderParams, dt: f32) -> usize {
        // Cota superior de la velocidad alcanzada durante el paso.
        let fastest = self.speed.abs() + params.acceleration.max(params.braking) * dt;
        let reach = fastest * dt;
        let n = (reach / Self::MAX_SUBSTEP_DISTANCE).ceil();
        if n.is_finite() {
            (n as usize).clamp(1, MAX_SUBSTEPS)
        } else {
            1
        }
    }

    /// Un jinete a caballo en reposo en `position`, parado sobre `ground`
    /// (pasar el [`World`] para que cuenten los andenes).
    pub fn new(position: Vec2, heading: Heading, ground: &(impl Ground + ?Sized)) -> Self {
        Self {
            position,
            height: ground.ground_height(position),
            heading,
            speed: 0.0,
            collider: Circle::new(Self::COLLIDER_RADIUS),
            air: 0.0,
            lift: 0.0,
        }
    }

    /// Hace saltar a un caballo a galope tendido ([`Self::FULL_GALLOP`] de la
    /// velocidad máxima de `params`); sube a [`Self::JUMP_SPEED`] y supera los
    /// obstáculos bajos por encima de los cuales llega
    /// ([`World::resolve_above`]). Devuelve si despegó: nunca a pie, más
    /// despacio o si ya está en el aire.
    pub fn jump(&mut self, params: &RiderParams) -> bool {
        let takes_off = self.travel() == Travel::Riding
            && self.speed >= Self::FULL_GALLOP * params.gallop_max
            && !self.airborne();
        if takes_off {
            self.lift = Self::JUMP_SPEED;
        }
        takes_off
    }

    /// Si un salto lo tiene despegado del suelo.
    pub fn airborne(&self) -> bool {
        self.air > 0.0 || self.lift > 0.0
    }

    /// Qué parte del vuelo de su salto lleva recorrida, `0..=1` (nada en el
    /// suelo): el tiempo desde el despegue sobre la duración del vuelo, para
    /// que la animación siga el mismo arco que recorre la física.
    pub fn flight(&self) -> Option<f32> {
        if !self.airborne() {
            return None;
        }
        let aloft = (Self::JUMP_SPEED - self.lift) / Self::GRAVITY;
        Some((aloft / (2.0 * Self::JUMP_SPEED / Self::GRAVITY)).clamp(0.0, 1.0))
    }

    /// A qué altura sobre el suelo está a efectos de la colisión: la altura de
    /// un salto o, en el suelo, [`StaticObstacles::ON_GROUND`] (toda huella lo
    /// bloquea).
    fn clearance(&self) -> f32 {
        if self.airborne() {
            self.air
        } else {
            StaticObstacles::ON_GROUND
        }
    }

    /// La altura a la que está: la del suelo más la de cualquier salto.
    pub fn elevation(&self) -> f32 {
        self.height + self.air
    }

    /// Hace avanzar al jinete `dt` segundos dentro de `world`.
    ///
    /// Una entrada de giro o de aceleración no finita cuenta como ausencia de
    /// entrada, y un `dt` no finito o no positivo deja al jinete sin cambios.
    pub fn step(&mut self, input: RiderInput, params: &RiderParams, world: &World, dt: f32) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        let input = RiderInput {
            throttle: finite_or_zero(input.throttle).clamp(-1.0, 1.0),
            turn: finite_or_zero(input.turn).clamp(-1.0, 1.0),
            gallop: input.gallop,
        };
        // Subpasos cortos para que los movimientos rápidos no puedan saltearse
        // obstáculos delgados.
        let n = self.substeps(params, dt);
        let sub_dt = dt / n as f32;
        for _ in 0..n {
            self.substep(input, params, world, sub_dt);
        }
        self.height = world.ground_height(self.position);
    }

    /// Un subpaso de colisión con la entrada ya saneada.
    ///
    /// En el aire el caballo mantiene su orientación y su velocidad (solo un
    /// obstáculo lo detiene) y cae bajo [`Self::GRAVITY`], aterrizando en 0.
    fn substep(&mut self, input: RiderInput, params: &RiderParams, world: &World, dt: f32) {
        if self.airborne() {
            self.air += (self.lift - Self::GRAVITY * dt / 2.0) * dt;
            self.lift -= Self::GRAVITY * dt;
            if self.air <= 0.0 {
                self.air = 0.0;
                self.lift = 0.0;
            }
        } else {
            self.heading = self.heading.rotated(input.turn * params.turn_rate * dt);
            self.speed = next_speed(self.speed, input.throttle, input.gallop, params, dt);
        }
        self.position += self.heading.forward() * (self.speed * dt);

        self.resolve_obstacles(world);
        let contained = world.bounds.contain(self.position, self.collider.radius);
        self.position = contained.position;
        for push in [contained.push_x, contained.push_y].into_iter().flatten() {
            self.block(push);
        }
    }

    /// Saca al jinete de todos los obstáculos con los que se superpone y que no
    /// está saltando, deslizándolo a lo largo de las paredes.
    fn resolve_obstacles(&mut self, world: &World) {
        let mut normals = Vec::new();
        self.position = world.resolve_above(self.collider, self.position, self.clearance(), |n| {
            normals.push(n);
        });
        for normal in normals {
            self.block(normal);
        }
    }

    /// Quita la parte de la velocidad del jinete que apunta en contra de
    /// `normal` (la dirección unitaria en la que una pared empujó al jinete),
    /// para que se deslice a lo largo de las paredes en lugar de quedarse
    /// pegado.
    fn block(&mut self, normal: Vec2) {
        let along_normal = self.heading.forward().dot(normal);
        if self.speed * along_normal < 0.0 {
            // v' = v - (v.n) n, proyectada de nuevo sobre la orientación (sin
            // cambios).
            self.speed -= self.speed * along_normal * along_normal;
        }
    }

    /// Dónde está este jinete, como obstáculo para los demás.
    pub fn body(&self) -> Body {
        Body {
            position: self.position,
            collider: self.collider,
        }
    }

    /// Saca al jinete de cualquiera de los `bodies` con los que todavía se
    /// superpone (personas atrapadas contra una pared que no pudieron
    /// apartarse), deteniendo su movimiento hacia ellas. Un caballo que salta
    /// se mantiene lejos de lo que está saltando.
    pub fn keep_clear_of(&mut self, world: &World, bodies: &[Body]) {
        let mut normals = Vec::new();
        let (collider, position) = (self.collider, self.position);
        let clearance = self.clearance();
        self.position = world.resolve_among_above(collider, position, bodies, clearance, |n| {
            normals.push(n);
        });
        for normal in normals {
            self.block(normal);
        }
        self.height = world.ground_height(self.position);
    }

    pub fn animation(&self, params: &RiderParams) -> AnimationState {
        AnimationState::from_speed(self.speed, params.walk_max)
    }
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// Mueve `value` hacia `target` como mucho `max_delta`, sin pasarse nunca.
fn approach(value: f32, target: f32, max_delta: f32) -> f32 {
    if value < target {
        (value + max_delta).min(target)
    } else {
        (value - max_delta).max(target)
    }
}

fn next_speed(speed: f32, throttle: f32, gallop: bool, p: &RiderParams, dt: f32) -> f32 {
    let top = if gallop { p.gallop_max } else { p.walk_max };
    if throttle > 0.0 {
        if speed < 0.0 {
            approach(speed, 0.0, p.braking * throttle * dt)
        } else if speed > top {
            // Al salir de un galope: bajar suavemente en lugar de cambiar de
            // golpe.
            approach(speed, top, p.friction * dt)
        } else {
            approach(speed, top, p.acceleration * throttle * dt)
        }
    } else if throttle < 0.0 {
        let amount = -throttle;
        if speed > 0.0 {
            approach(speed, 0.0, p.braking * amount * dt)
        } else {
            approach(speed, -p.reverse_max, p.acceleration * amount * dt)
        }
    } else {
        approach(speed, 0.0, p.friction * dt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Aabb, Terrain};
    use std::f32::consts::FRAC_PI_4;

    const DT: f32 = 1.0 / 60.0;

    fn rider() -> (Rider, RiderParams, World) {
        let world = World::default();
        let r = Rider::new(Vec2::new(3.0, -2.0), Heading::from_radians(0.0), &world);
        (r, RiderParams::default(), world)
    }

    fn run(r: &mut Rider, input: RiderInput, p: &RiderParams, t: &World, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            r.step(input, p, t, DT);
        }
    }

    const GALLOP: RiderInput = RiderInput {
        throttle: 1.0,
        turn: 0.0,
        gallop: true,
    };
    const WALK: RiderInput = RiderInput {
        throttle: 1.0,
        turn: 0.0,
        gallop: false,
    };

    /// El mundo por defecto con un alambrado de 1,3 m de alto que corta el
    /// camino hacia el este, con su centro en `x`.
    fn fenced_at(x: f32) -> World {
        let mut world = World::default();
        let span = Aabb::from_center_size(Vec2::new(x, -2.0), Vec2::new(0.15, 40.0));
        world.obstacles.push_low(span, crate::FenceLayout::HEIGHT);
        world
    }

    /// Cabalga hacia el este con `input` desde (3, -2) hasta `x`, salta ahí
    /// (cuando `jump`) y después sigue cabalgando durante `seconds`.
    fn ride_at_fence(world: &World, input: RiderInput, x: f32, jump: bool, seconds: f32) -> Rider {
        let p = RiderParams::default();
        let mut r = Rider::new(Vec2::new(3.0, -2.0), Heading::from_radians(0.0), world);
        while r.position.x < x {
            r.step(input, &p, world, DT);
        }
        if jump {
            assert!(r.jump(&p));
        }
        run(&mut r, input, &p, world, seconds);
        r
    }

    #[test]
    fn the_flight_share_runs_from_takeoff_through_the_peak_to_landing() {
        let (mut r, p, t) = rider();
        assert_eq!(r.flight(), None, "on the ground");
        run(&mut r, GALLOP, &p, &t, 3.5);
        assert!(r.jump(&p));
        assert!(r.flight().unwrap() < 1e-6, "takeoff");
        let mut last = 0.0;
        let mut at_peak = None;
        let mut peak = 0.0;
        while r.airborne() {
            r.step(RiderInput::default(), &p, &t, DT);
            if let Some(f) = r.flight() {
                assert!(f >= last, "never goes back");
                last = f;
                if r.air > peak {
                    peak = r.air;
                    at_peak = Some(f);
                }
            }
        }
        assert!((at_peak.unwrap() - 0.5).abs() < 0.02, "{at_peak:?}");
        assert!(last > 0.95, "ends near landing: {last}");
        assert_eq!(r.flight(), None, "landed");
    }

    #[test]
    fn a_galloping_horse_jumps_up_and_lands_again() {
        let (mut r, p, t) = rider();
        run(&mut r, GALLOP, &p, &t, 3.5);
        let speed = r.speed;
        assert!(r.jump(&p));
        assert!(r.airborne());
        // Un salto por vez.
        assert!(!r.jump(&p));
        let mut peak: f32 = 0.0;
        for _ in 0..(1.3 / DT) as usize {
            r.step(RiderInput::default(), &p, &t, DT);
            peak = peak.max(r.air);
            assert!(r.air >= 0.0);
            assert!((r.elevation() - r.height - r.air).abs() < 1e-6);
        }
        assert!((1.5..1.6).contains(&peak), "{peak}");
        // Otra vez abajo, en el suelo, y frenando una vez que aterriza.
        assert!(!r.airborne());
        assert_eq!((r.air, r.lift), (0.0, 0.0));
        assert!(r.speed < speed);
        run(&mut r, GALLOP, &p, &t, 1.0);
        assert!(r.jump(&p), "landed, ready to jump again at full gallop");
    }

    #[test]
    fn only_a_horse_at_full_gallop_jumps() {
        let (mut r, p, t) = rider();
        let full = Rider::FULL_GALLOP * p.gallop_max;
        for (speed, why) in [
            (0.0, "standing still"),
            (-1.0, "backing up"),
            (p.walk_max, "walking"),
            (6.5, "trotting"),
            (full - 0.05, "short of full gallop"),
        ] {
            r.speed = speed;
            assert!(!r.jump(&p), "{why}");
            assert!(!r.airborne(), "{why}");
        }
        r.speed = full;
        assert!(r.jump(&p), "at full gallop");
        let mut person = Rider::on_foot(Vec2::new(3.0, -2.0), Heading::from_radians(0.0), &t);
        person.speed = p.gallop_max;
        assert!(!person.jump(&p), "on foot");
        assert!(!person.airborne());
    }

    #[test]
    fn in_the_air_the_horse_holds_its_line_and_its_rider_stays_on() {
        let (mut r, p, t) = rider();
        run(&mut r, GALLOP, &p, &t, 3.5);
        assert!(r.jump(&p));
        // Lo bastante lento como para bajarse, si estuviera en el suelo.
        r.speed = 0.8;
        let (heading, speed) = (r.heading, r.speed);
        let turn_and_brake = RiderInput {
            throttle: -1.0,
            turn: 1.0,
            gallop: false,
        };
        r.step(turn_and_brake, &p, &t, DT);
        assert!(r.airborne());
        assert_eq!((r.heading, r.speed), (heading, speed));
        assert!(r.dismount(&t).is_none());
    }

    #[test]
    fn a_galloping_jump_clears_a_fence() {
        let world = fenced_at(40.0);
        // Despegue más o menos donde el arco alcanza su punto más alto sobre
        // los alambres.
        let r = ride_at_fence(&world, GALLOP, 40.0 - 6.7, true, 1.5);
        assert!(r.position.x > 42.0, "{:?}", r.position);
    }

    #[test]
    fn a_fence_stops_a_horse_that_does_not_jump_or_cannot_at_a_walk() {
        let world = fenced_at(40.0);
        let r = ride_at_fence(&world, GALLOP, 40.0 - 6.7, false, 3.0);
        assert!(r.position.x < 40.0, "no jump: {:?}", r.position);
        // Al paso, Espacio no hace nada: el alambrado lo detiene.
        let mut r = ride_at_fence(&world, WALK, 40.0 - 1.65, false, 0.0);
        let p = RiderParams::default();
        assert!(!r.jump(&p));
        run(&mut r, WALK, &p, &world, 3.0);
        assert!(r.position.x < 40.0, "walking: {:?}", r.position);
    }

    #[test]
    fn keeping_clear_of_others_in_the_air_still_passes_over_a_fence() {
        let world = fenced_at(30.0);
        let over = Vec2::new(30.0, -2.0);
        let mut r = Rider::new(over, Heading::from_radians(0.0), &world);
        (r.air, r.lift) = (1.4, 0.5);
        r.keep_clear_of(&world, &[]);
        assert_eq!(r.position, over);
        // En el suelo, el alambrado lo empuja hacia afuera como siempre.
        (r.air, r.lift) = (0.0, 0.0);
        r.keep_clear_of(&world, &[]);
        assert!(r.position.x < 30.0 - 0.9, "{:?}", r.position);
    }

    #[test]
    fn a_jump_never_clears_a_tall_obstacle() {
        let mut world = World::default();
        let wall = Aabb::from_center_size(Vec2::new(40.0, -2.0), Vec2::new(0.6, 40.0));
        world.obstacles.push(wall);
        let r = ride_at_fence(&world, GALLOP, 40.0 - 6.7, true, 3.0);
        assert!(r.position.x < 40.0, "{:?}", r.position);
    }

    #[test]
    fn new_rider_rests_on_terrain() {
        let (r, p, t) = rider();
        assert_eq!(r.speed, 0.0);
        assert_eq!(r.height, t.terrain.height(3.0, -2.0));
        assert_eq!(r.animation(&p), AnimationState::Idle);
    }

    #[test]
    fn idle_rider_stays_put() {
        let (mut r, p, t) = rider();
        run(&mut r, RiderInput::default(), &p, &t, 2.0);
        assert_eq!(r.position, Vec2::new(3.0, -2.0));
        assert_eq!(r.animation(&p), AnimationState::Idle);
    }

    #[test]
    fn throttle_accelerates_to_walk_cap() {
        let (mut r, p, t) = rider();
        let walk = RiderInput {
            throttle: 1.0,
            ..Default::default()
        };
        r.step(walk, &p, &t, DT);
        assert!(r.speed > 0.0);
        run(&mut r, walk, &p, &t, 5.0);
        assert!((r.speed - p.walk_max).abs() < 1e-4);
        assert_eq!(r.animation(&p), AnimationState::Walk);
    }

    #[test]
    fn gallop_reaches_higher_cap_and_drops_back_when_released() {
        let (mut r, p, t) = rider();
        let gallop = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        run(&mut r, gallop, &p, &t, 6.0);
        assert!((r.speed - p.gallop_max).abs() < 1e-4);
        assert_eq!(r.animation(&p), AnimationState::Gallop);

        let walk = RiderInput {
            gallop: false,
            ..gallop
        };
        run(&mut r, walk, &p, &t, 6.0);
        assert!((r.speed - p.walk_max).abs() < 1e-4);
    }

    #[test]
    fn friction_stops_without_reversing() {
        let (mut r, p, t) = rider();
        r.speed = 5.0;
        run(&mut r, RiderInput::default(), &p, &t, 5.0);
        assert_eq!(r.speed, 0.0);
    }

    #[test]
    fn negative_throttle_brakes_then_backs_up() {
        let (mut r, p, t) = rider();
        r.speed = 2.0;
        let back = RiderInput {
            throttle: -1.0,
            ..Default::default()
        };
        r.step(back, &p, &t, DT);
        assert!(r.speed < 2.0 && r.speed > 0.0);
        run(&mut r, back, &p, &t, 5.0);
        assert!((r.speed + p.reverse_max).abs() < 1e-4);
    }

    #[test]
    fn moves_along_heading() {
        let (mut r, p, t) = rider();
        r.heading = Heading::from_radians(std::f32::consts::FRAC_PI_2);
        r.speed = 2.0;
        r.step(
            RiderInput {
                throttle: 1.0,
                ..Default::default()
            },
            &p,
            &t,
            DT,
        );
        assert!((r.position.x - 3.0).abs() < 1e-4);
        assert!(r.position.y > -2.0);
    }

    #[test]
    fn turning_is_continuous_not_snapped() {
        let (mut r, p, t) = rider();
        let left = RiderInput {
            turn: 1.0,
            ..Default::default()
        };
        r.step(left, &p, &t, DT);
        let expected = p.turn_rate * DT;
        assert!((r.heading.radians() - expected).abs() < 1e-5);
        assert!((r.heading.radians() % FRAC_PI_4).abs() > 1e-3);

        let right = RiderInput {
            turn: -1.0,
            ..Default::default()
        };
        r.step(right, &p, &t, DT);
        r.step(right, &p, &t, DT);
        assert!(r.heading.radians() > 6.0, "should wrap below zero");
    }

    #[test]
    fn sticks_to_terrain_while_moving() {
        let (mut r, p, t) = rider();
        let gallop = RiderInput {
            throttle: 1.0,
            turn: 0.3,
            gallop: true,
        };
        run(&mut r, gallop, &p, &t, 3.0);
        assert_eq!(r.height, t.terrain.height(r.position.x, r.position.y));
    }

    #[test]
    fn inputs_are_clamped() {
        let (mut r, p, t) = rider();
        let wild = RiderInput {
            throttle: 50.0,
            turn: 50.0,
            gallop: false,
        };
        r.step(wild, &p, &t, DT);
        assert!(r.speed <= p.acceleration * DT + 1e-6);
        assert!(r.heading.radians() <= p.turn_rate * DT + 1e-6);
    }

    #[test]
    fn bad_dt_is_a_no_op() {
        let (mut r, p, t) = rider();
        r.speed = 3.0;
        let start = r;
        let input = RiderInput {
            throttle: 1.0,
            turn: 1.0,
            gallop: true,
        };
        for dt in [-DT, 0.0] {
            r.step(input, &p, &t, dt);
            assert_eq!(r, start, "dt {dt} must not change the rider");
        }
    }

    #[test]
    fn cannot_leave_world_bounds() {
        let (mut r, p, t) = rider();
        let gallop = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        // Un minuto de galope desde 200 m antes del borde este.
        r.position = Vec2::new(t.bounds.max.x - 200.0, 0.0);
        run(&mut r, gallop, &p, &t, 60.0);
        let edge = t.bounds.max.x - Rider::COLLIDER_RADIUS;
        assert!((r.position.x - edge).abs() < 1e-3, "x = {}", r.position.x);
        assert!(r.speed.abs() < 1e-3, "head-on wall stops the rider");
        assert_eq!(r.height, t.terrain.height(r.position.x, r.position.y));
    }

    #[test]
    fn slides_along_world_edge() {
        let (mut r, p, t) = rider();
        r.position = Vec2::new(t.bounds.max.x - Rider::COLLIDER_RADIUS, 0.0);
        // Orientación 30 grados al norte del este: hacia el borde este, en parte
        // a lo largo de él.
        r.heading = Heading::from_radians(30f32.to_radians());
        r.speed = 4.0;
        r.step(RiderInput::default(), &p, &t, DT);
        assert!(r.position.x <= t.bounds.max.x - Rider::COLLIDER_RADIUS);
        assert!(r.position.y > 0.0, "keeps moving along the edge");
        assert!(
            r.speed > 0.0 && r.speed < 4.0,
            "loses only the part into the wall"
        );
    }

    fn walled_world() -> World {
        let mut world = World::default();
        world
            .obstacles
            .push(Aabb::new(Vec2::new(10.0, -10.0), Vec2::new(14.0, 10.0)));
        world
    }

    #[test]
    fn cannot_ride_through_a_building() {
        let (mut r, p, _) = rider();
        let world = walled_world();
        let gallop = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        run(&mut r, gallop, &p, &world, 10.0);
        let face = 10.0 - Rider::COLLIDER_RADIUS;
        assert!((r.position.x - face).abs() < 1e-3, "x = {}", r.position.x);
        assert!(r.speed.abs() < 1e-3, "head-on stops the rider");
    }

    #[test]
    fn slides_along_a_building() {
        let (mut r, p, _) = rider();
        let world = walled_world();
        r.position = Vec2::new(10.0 - Rider::COLLIDER_RADIUS, 0.0);
        r.heading = Heading::from_radians(-30f32.to_radians());
        r.speed = 4.0;
        r.step(RiderInput::default(), &p, &world, DT);
        assert!(r.position.x <= 10.0 - Rider::COLLIDER_RADIUS + 1e-4);
        assert!(r.position.y < 0.0, "keeps moving along the wall");
        assert!(r.speed > 0.0 && r.speed < 4.0);
    }

    fn galloping_into(trunk: Vec2, params: RiderParams, world: &World) -> Rider {
        let mut r = Rider::new(
            trunk - Vec2::new(10.0, 0.0),
            Heading::from_radians(0.0),
            world,
        );
        r.speed = params.gallop_max;
        let gallop = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        let near_face = trunk.x - crate::town::TREE_TRUNK_RADIUS - Rider::COLLIDER_RADIUS;
        for frame in 0..(4.0 / MAX_STEP) as usize {
            r.step(gallop, &params, world, MAX_STEP);
            assert!(
                r.position.x <= near_face + 1e-3,
                "frame {frame}: tunneled to x = {} (face {near_face})",
                r.position.x
            );
        }
        r
    }

    #[test]
    fn galloping_head_on_into_a_plaza_tree_stays_on_the_near_side() {
        let town = crate::TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let trunk = town.trees[0];
        let r = galloping_into(trunk, RiderParams::default(), &world);
        assert!(r.speed.abs() < 1e-3, "head-on stops the rider");
        assert!((r.position.y - trunk.y).abs() < 1e-3);
    }

    #[test]
    fn even_a_much_faster_horse_cannot_tunnel_through_a_trunk() {
        let trunk = Vec2::new(20.0, 0.0);
        let mut world = World::default();
        let size = Vec2::new(2.0, 2.0) * crate::town::TREE_TRUNK_RADIUS;
        world.obstacles.push(Aabb::from_center_size(trunk, size));
        // 40 m/s avanza 2 m por paso máximo: más que radio + medio ancho.
        let params = RiderParams {
            gallop_max: 40.0,
            ..RiderParams::default()
        };
        let r = galloping_into(trunk, params, &world);
        assert!(r.speed.abs() < 1e-3);
    }

    #[test]
    fn substeps_keep_every_move_under_the_bound() {
        let (mut r, p, _) = rider();
        r.speed = 0.0;
        assert_eq!(r.substeps(&p, DT), 1);
        for (speed, dt) in [(p.gallop_max, MAX_STEP), (40.0, MAX_STEP), (-3.0, 0.25)] {
            r.speed = speed;
            let n = r.substeps(&p, dt);
            let reach = (speed.abs() + p.acceleration.max(p.braking) * dt) * dt;
            assert!(reach / n as f32 <= Rider::MAX_SUBSTEP_DISTANCE + 1e-6);
            assert!(n < 64, "bounded work: {n}");
        }
    }

    /// Galopa por un recorrido errante desde el punto de llegada de `town`
    /// durante `seconds`, girando `rate` radianes de fase por paso y
    /// comprobando que el jinete nunca termine un paso dentro de algo; devuelve
    /// cuántos pasos terminaron tocando un obstáculo.
    fn wander_turning(town: &crate::TownLayout, seconds: f32, rate: f32) -> usize {
        let world = World::with_town(Terrain::default(), town);
        let p = RiderParams::default();
        let (start, heading) = town.arrival();
        let mut r = Rider::new(start, heading, &world);
        let reach = Vec2::new(1.0, 1.0) * (2.0 * Rider::COLLIDER_RADIUS + 0.2);
        let mut contacts = 0;
        for frame in 0..(seconds / DT) as usize {
            // Deambular: galopar con un giro que cambia lentamente.
            let turn = ((frame as f32) * rate).sin() * 0.6;
            let input = RiderInput {
                throttle: 1.0,
                turn,
                gallop: true,
            };
            r.step(input, &p, &world, DT);
            for o in world
                .obstacles
                .near(Aabb::from_center_size(r.position, reach))
            {
                let gap = r.position.distance(o.closest_point(r.position));
                assert!(
                    gap >= Rider::COLLIDER_RADIUS - 1e-3,
                    "{}, frame {frame}: inside {o:?} at {:?}",
                    town.name,
                    r.position
                );
                if gap < Rider::COLLIDER_RADIUS + 1e-2 {
                    contacts += 1;
                }
            }
            assert!(r.position.x.is_finite() && r.position.y.is_finite());
        }
        contacts
    }

    #[test]
    fn never_ends_a_step_inside_the_buildings_of_any_town() {
        // Una velocidad de giro cuyo recorrido por Castex choca contra
        // edificios.
        let castex = wander_turning(&crate::TownLayout::castex_inspired(), 240.0, 0.003);
        assert!(castex > 0, "the wander path must actually hit buildings");
        let mut contacts = 0;
        for town in crate::town::tests::sample_towns() {
            contacts += wander_turning(&town, 90.0, 0.004);
        }
        assert!(contacts > 0, "the wander paths must actually hit buildings");
    }
}
