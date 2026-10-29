//! Pájaros en el cielo: chimangos planeando en círculos de día, bandadas de
//! palomas y teros que cruzan, y bandadas que van a dormir al anochecer;
//! ninguno de noche.
//!
//! Cada pájaro es una entidad con una mesh de silueta oscura y sin iluminación
//! que alterna entre unos pocos frames de aleteo (7 meshes pequeñas
//! compartidas, un material), así que como mucho [`MAX_BIRDS`] draw calls y
//! unos 250 triángulos. Sus trayectorias son analíticas: un círculo, o una
//! línea recta con un lugar en una V. Cada uno se envuelve en un cuadrado
//! [`TORUS`] alrededor de la cámara, así que siempre hay pájaros cerca sin
//! simular ninguno, y cada uno vuela al menos [`MIN_ABOVE_CAMERA`] metros por
//! encima de la cámara, nunca cerca de la cara del jugador. Un pájaro aparece o
//! se oculta solo mientras está más allá de [`HIDE_RANGE`], donde la niebla de
//! polvo ya lo tapó, así que ninguno aparece de golpe a la vista; uno que sigue
//! a la vista cuando se cierra su compuerta se oculta después de [`GATE_GRACE`]
//! segundos, así que ninguno vuela toda la noche.

use super::main_camera;
use crate::atmosphere::{SUNRISE, SUNSET, WorldClock};
use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// La mayor cantidad de pájaros que llega a haber.
pub const MAX_BIRDS: usize = 48;
/// Lado del cuadrado alrededor de la cámara en el que se envuelven los pájaros,
/// en metros.
pub const TORUS: f32 = 440.0;
/// La altura mínima a la que vuela un pájaro por encima de la cámara, en
/// metros.
pub const MIN_ABOVE_CAMERA: f32 = 30.0;
/// Más allá de esta cantidad de metros la niebla oculta a un pájaro: la única
/// distancia a la que puede aparecer o desaparecer.
pub const HIDE_RANGE: f32 = 200.0;
/// Cuánto tiempo puede seguir volando un pájaro todavía a la vista después de
/// que se cerró su compuerta, en segundos; luego se oculta aunque esté a la
/// vista.
pub const GATE_GRACE: f32 = 45.0;
/// Frames de aleteo en un ciclo; la mesh que les sigue es el planeo.
pub const FLAP_FRAMES: usize = 6;
/// El rumbo de todas las bandadas que van a dormir (hacia el sudeste), como
/// `(x, z)`, de longitud unitaria.
pub const ROOST_HEADING: Vec2 = Vec2::new(0.8, 0.6);

/// Qué tipo de vuelo tiene un pájaro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Un chimango planeando en círculos amplios sobre las térmicas del día.
    Soarer,
    /// Una bandada de palomas o teros que cruza de día.
    Crosser,
    /// Una bandada que vuela baja hacia el dormidero al anochecer.
    Roost,
}

/// Por dónde vuela un pájaro y cómo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BirdSpec {
    pub group: Group,
    /// Centro del círculo (los que planean) o posición en el tiempo cero (las
    /// bandadas), `(x, z)`.
    pub anchor: Vec2,
    /// Radio del círculo, en metros (los que planean).
    pub radius: f32,
    /// Velocidad angular, en radianes por segundo; negativa gira en el otro
    /// sentido.
    pub rate: f32,
    /// Rumbo unitario de la bandada (bandadas).
    pub heading: Vec2,
    /// Velocidad, en metros por segundo (bandadas).
    pub speed: f32,
    /// Altura sobre el nivel del mar, en metros.
    pub height: f32,
    /// Lugar en la V de la bandada: a lo largo y después a lo ancho, en metros
    /// (bandadas).
    pub slot: Vec2,
    /// Multiplicador de tamaño de la silueta unitaria (de alrededor de 1.1 m de
    /// envergadura).
    pub scale: f32,
    /// Aleteos por segundo.
    pub flap_hz: f32,
    /// Desfase dentro del círculo o del ciclo de aleteo, en vueltas.
    pub phase: f32,
}

/// Si los pájaros de `group` andan afuera a la hora `hour`.
pub fn gate_open(group: Group, hour: f32) -> bool {
    let hour = hour.rem_euclid(24.0);
    let (from, to) = match group {
        Group::Soarer => (SUNRISE + 2.0, SUNSET - 1.5),
        Group::Crosser => (SUNRISE + 0.3, SUNSET - 2.0),
        Group::Roost => (SUNSET - 2.0, SUNSET + 0.5),
    };
    (from..to).contains(&hour)
}

/// Un número repetible en `[0, 1)` para `seed` y `k`.
fn unit(seed: u32, k: u32) -> f32 {
    let mut x = seed
        .wrapping_mul(0x9E37_79B1)
        .wrapping_add(k.wrapping_mul(0x85EB_CA6B));
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

fn between(seed: u32, k: u32, lo: f32, hi: f32) -> f32 {
    lo + (hi - lo) * unit(seed, k)
}

fn flock(specs: &mut Vec<BirdSpec>, group: Group, seed: u32, size: usize) {
    let angle = if group == Group::Roost {
        ROOST_HEADING.to_angle()
    } else {
        between(seed, 1, 0.0, std::f32::consts::TAU)
    };
    let heading = Vec2::from_angle(angle);
    let anchor = Vec2::new(
        between(seed, 2, -2000.0, 2000.0),
        between(seed, 3, -2000.0, 2000.0),
    );
    let (speed, height, flap_hz) = if group == Group::Roost {
        (
            between(seed, 4, 11.0, 13.0),
            between(seed, 5, 35.0, 70.0),
            4.0,
        )
    } else {
        (
            between(seed, 4, 12.0, 15.0),
            between(seed, 5, 50.0, 110.0),
            5.5,
        )
    };
    for i in 0..size {
        // Una V: primero el líder, y después pares que se van quedando atrás a
        // cada lado.
        let rank = i.div_ceil(2) as f32;
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let wobble = |k| between(seed, 10 + i as u32 * 4 + k, -0.6, 0.6);
        specs.push(BirdSpec {
            group,
            anchor,
            radius: 0.0,
            rate: 0.0,
            heading,
            speed,
            height: height + wobble(0),
            slot: Vec2::new(-rank * 2.4 + wobble(1), side * rank * 2.0 + wobble(2)),
            scale: 1.4,
            flap_hz: flap_hz * (1.0 + 0.04 * wobble(3)),
            phase: unit(seed, 100 + i as u32),
        });
    }
}

/// Todos los pájaros que puede haber: seis chimangos planeando, tres bandadas
/// de siete que cruzan y tres bandadas de seis que van a dormir. La misma lista
/// en cada ejecución.
pub fn bird_specs() -> Vec<BirdSpec> {
    let mut specs = Vec::with_capacity(MAX_BIRDS);
    for i in 0..6u32 {
        let seed = 1000 + i;
        let dir = if i % 2 == 0 { 1.0 } else { -1.0 };
        specs.push(BirdSpec {
            group: Group::Soarer,
            anchor: Vec2::new(
                between(seed, 1, -2000.0, 2000.0),
                between(seed, 2, -2000.0, 2000.0),
            ),
            radius: between(seed, 3, 25.0, 70.0),
            rate: dir * between(seed, 4, 0.12, 0.2),
            heading: Vec2::X,
            speed: 0.0,
            height: between(seed, 5, 70.0, 140.0),
            slot: Vec2::ZERO,
            scale: 1.7,
            flap_hz: 0.0,
            phase: unit(seed, 6),
        });
    }
    for i in 0..3 {
        flock(&mut specs, Group::Crosser, 2000 + i, 7);
    }
    for i in 0..3 {
        flock(&mut specs, Group::Roost, 3000 + i, 6);
    }
    specs
}

/// Envuelve `p` en el cuadrado [`TORUS`] centrado en `centre`.
pub fn wrap_around(p: Vec2, centre: Vec2) -> Vec2 {
    centre + (p - centre + Vec2::splat(TORUS / 2.0)).rem_euclid(Vec2::splat(TORUS))
        - Vec2::splat(TORUS / 2.0)
}

/// Dónde está un pájaro a los `seconds`, y hacia dónde mira y se inclina, para
/// una cámara en `camera`.
pub fn bird_pose(spec: &BirdSpec, seconds: f32, camera: Vec3) -> (Vec3, Vec3, f32) {
    let centre = Vec2::new(camera.x, camera.z);
    let (xz, heading, bank) = match spec.group {
        Group::Soarer => {
            let turn = std::f32::consts::TAU * spec.phase + spec.rate * seconds;
            let ring = Vec2::from_angle(turn) * spec.radius;
            let tangent = ring.perp() * spec.rate.signum();
            // Inclinarse hacia el círculo: el centro del círculo está a la
            // izquierda de un pájaro que gira en sentido antihorario.
            let lean = -0.35 * spec.rate.signum();
            (
                // Se envuelve el pájaro, no su círculo: un centro envuelto lo
                // llevaría a cruzar a la vista, más cerca que HIDE_RANGE.
                wrap_around(spec.anchor + ring, centre),
                tangent.normalize_or(Vec2::X),
                lean,
            )
        }
        Group::Crosser | Group::Roost => {
            let fore = spec.heading;
            let side = fore.perp();
            let at =
                spec.anchor + fore * spec.speed * seconds + fore * spec.slot.x + side * spec.slot.y;
            (wrap_around(at, centre), fore, 0.0)
        }
    };
    let bob = if spec.group == Group::Soarer {
        2.0 * (0.3 * seconds + 6.0 * spec.phase).sin()
    } else {
        0.0
    };
    let y = (spec.height + bob).max(camera.y + MIN_ABOVE_CAMERA);
    (
        Vec3::new(xz.x, y, xz.y),
        Vec3::new(heading.x, 0.0, heading.y),
        bank,
    )
}

/// La mesh que muestra un pájaro a los `seconds`: un frame de aleteo, o el
/// planeo (el último índice) para los que planean.
pub fn frame_at(spec: &BirdSpec, seconds: f32) -> usize {
    if spec.group == Group::Soarer {
        return FLAP_FRAMES;
    }
    let turns = (spec.phase + spec.flap_hz * seconds).rem_euclid(1.0);
    ((turns * FLAP_FRAMES as f32) as usize).min(FLAP_FRAMES - 1)
}

/// Si un pájaro se muestra a continuación: la compuerta decide mientras la
/// niebla lo oculta, y a la vista solo una vez que estuvo cerrada durante
/// [`GATE_GRACE`] segundos.
pub fn next_visible(shown: bool, gate: bool, distance: f32, closed_for: f32) -> bool {
    if distance > HIDE_RANGE {
        gate
    } else {
        // Si no, un pájaro que circula dentro del rango volaría toda la noche.
        shown && (gate || closed_for < GATE_GRACE)
    }
}

/// Una silueta de pájaro, de 1.1 m de envergadura en reposo, con el pico hacia
/// -Z y las alas levantadas `angle` radianes (negativo: bajadas).
fn silhouette(angle: f32) -> Mesh {
    let (c, s) = (angle.cos(), angle.sin());
    let mut positions: Vec<[f32; 3]> =
        vec![[0.0, 0.0, -0.24], [0.045, 0.0, 0.2], [-0.045, 0.0, 0.2]];
    let mut indices: Vec<u32> = vec![0, 2, 1];
    for side in [1.0_f32, -1.0] {
        let base = positions.len() as u32;
        let reach = 0.05 + 0.5 * c;
        positions.extend([
            [side * 0.04, 0.0, -0.08],
            [side * 0.04, 0.0, 0.11],
            [side * reach, 0.5 * s, 0.07],
            [side * reach, 0.5 * s, -0.03],
        ]);
        if side > 0.0 {
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        } else {
            indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
        }
    }
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_indices(Indices::U32(indices))
}

/// Las siluetas y el material compartidos de todos los pájaros.
#[derive(Resource, Debug, Clone)]
pub struct BirdKit {
    /// Los [`FLAP_FRAMES`] frames de aleteo, y después el planeo.
    pub frames: Vec<Handle<Mesh>>,
    pub material: Handle<StandardMaterial>,
}

/// Un pájaro en el cielo: un índice en [`bird_specs`].
#[derive(Component, Debug, Clone, Copy)]
pub struct Bird {
    pub index: usize,
    /// Si la compuerta ya se aplicó una vez, para que el primer frame no
    /// necesite distancia.
    settled: bool,
    /// Segundos que lleva cerrada la compuerta, cero mientras está abierta.
    closed_for: f32,
}

/// Crea los pájaros y los hace volar.
pub struct BirdsPlugin;

impl Plugin for BirdsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_birds)
            .add_systems(Update, fly_birds);
    }
}

fn spawn_birds(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let flap = |k: usize| {
        let turn = k as f32 / FLAP_FRAMES as f32 * std::f32::consts::TAU;
        0.15 + 0.65 * turn.sin()
    };
    let mut frames: Vec<Handle<Mesh>> = (0..FLAP_FRAMES)
        .map(|k| meshes.add(silhouette(flap(k))))
        .collect();
    frames.push(meshes.add(silhouette(0.2)));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.07, 0.065, 0.06),
        unlit: true,
        cull_mode: None,
        double_sided: true,
        ..default()
    });
    for (index, spec) in bird_specs().into_iter().enumerate() {
        commands.spawn((
            Name::new("Bird"),
            Bird {
                index,
                settled: false,
                closed_for: 0.0,
            },
            Mesh3d(frames[frame_at(&spec, 0.0)].clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_scale(Vec3::splat(spec.scale)),
            Visibility::Hidden,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
    commands.insert_resource(BirdKit { frames, material });
}

fn fly_birds(
    clock: Option<Res<WorldClock>>,
    time: Res<Time<Real>>,
    kit: Option<Res<BirdKit>>,
    cameras: Query<(&GlobalTransform, &Camera), With<Camera3d>>,
    mut birds: Query<(&mut Bird, &mut Transform, &mut Visibility, &mut Mesh3d)>,
) {
    let Some(kit) = kit else {
        return;
    };
    let camera = main_camera(&cameras);
    let hour = clock.map_or(crate::atmosphere::DEFAULT_HOUR, |c| c.0.hour());
    let seconds = time.elapsed_secs();
    let dt = time.delta_secs();
    let specs = bird_specs();
    for (mut bird, mut transform, mut visibility, mut mesh) in &mut birds {
        let spec = &specs[bird.index];
        let (position, heading, bank) = bird_pose(spec, seconds, camera);
        let distance = position.distance(camera);
        let gate = gate_open(spec.group, hour);
        bird.closed_for = if gate { 0.0 } else { bird.closed_for + dt };
        let shown = *visibility == Visibility::Inherited;
        let show = if bird.settled {
            next_visible(shown, gate, distance, bird.closed_for)
        } else {
            bird.settled = true;
            gate
        };
        let wanted = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
        if !show {
            continue;
        }
        *transform = Transform::from_translation(position)
            .looking_to(heading, Vec3::Y)
            .with_scale(Vec3::splat(spec.scale));
        transform.rotate_local_z(bank);
        let frame = &kit.frames[frame_at(spec, seconds)];
        if mesh.0 != *frame {
            mesh.0 = frame.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pampa_core::{GameClock, TimeOfDay};

    #[test]
    fn there_are_forty_five_birds_in_three_groups() {
        let specs = bird_specs();
        assert_eq!(specs.len(), 45);
        assert!(specs.len() <= MAX_BIRDS);
        let count = |g| specs.iter().filter(|s| s.group == g).count();
        assert_eq!(count(Group::Soarer), 6);
        assert_eq!(count(Group::Crosser), 21);
        assert_eq!(count(Group::Roost), 18);
        assert_eq!(specs, bird_specs(), "the same every run");
    }

    #[test]
    fn birds_are_out_by_day_and_never_at_night() {
        let any = |hour| {
            [Group::Soarer, Group::Crosser, Group::Roost]
                .into_iter()
                .any(|g| gate_open(g, hour))
        };
        for hour in [20.5, 22.0, 0.0, 3.0, 5.5, 6.5] {
            assert!(!any(hour), "{hour}");
        }
        assert!(gate_open(Group::Soarer, 13.0));
        assert!(gate_open(Group::Crosser, 10.0));
        assert!(!gate_open(Group::Roost, 13.0));
        assert!(!gate_open(Group::Roost, 15.5));
        // Anochecer: las bandadas van a dormir mientras los que planean ya
        // bajaron.
        assert!(gate_open(Group::Roost, 19.0));
        assert!(!gate_open(Group::Soarer, 19.0));
        assert!(!gate_open(Group::Crosser, 19.0));
    }

    #[test]
    fn every_bird_stays_within_the_wrap_and_far_from_the_camera() {
        let specs = bird_specs();
        for camera in [Vec3::new(0.0, 2.0, 0.0), Vec3::new(913.0, 14.0, -2210.0)] {
            for spec in &specs {
                for k in 0..400 {
                    let (p, _, _) = bird_pose(spec, k as f32 * 1.7, camera);
                    let dx = (p.x - camera.x).abs();
                    let dz = (p.z - camera.z).abs();
                    // Los que planean giran alrededor de un centro envuelto,
                    // así que su radio puede llegar más allá del cuadrado.
                    let slack = spec.radius + 0.5;
                    assert!(dx <= 220.0 + slack && dz <= 220.0 + slack);
                    assert!(p.y >= camera.y + 30.0 - 1e-3);
                    assert!(p.distance(camera) >= 30.0 - 1e-3);
                }
            }
        }
    }

    #[test]
    fn a_camera_high_up_still_has_birds_above_it() {
        let spec = bird_specs()[0];
        let (p, _, _) = bird_pose(&spec, 0.0, Vec3::new(0.0, 500.0, 0.0));
        assert!(p.y >= 530.0);
    }

    #[test]
    fn soarers_circle_and_flocks_fly_straight() {
        let specs = bird_specs();
        let soarer = specs.iter().find(|s| s.group == Group::Soarer).unwrap();
        let camera = Vec3::ZERO;
        for k in 0..50 {
            let (p, _, _) = bird_pose(soarer, k as f32 * 3.0, camera);
            // El pájaro está envuelto, así que se compara módulo el toro.
            let d = Vec2::new(p.x, p.z) - soarer.anchor;
            let d = d - TORUS * (d / TORUS).round();
            let r = d.length();
            assert!((r - soarer.radius).abs() < 0.01, "{r}");
        }
        let flock = specs.iter().find(|s| s.group == Group::Crosser).unwrap();
        let (a, ha, _) = bird_pose(flock, 0.0, Vec3::new(10000.0, 0.0, 10000.0));
        let (b, _, _) = bird_pose(flock, 1.0, Vec3::new(10000.0, 0.0, 10000.0));
        let step = Vec2::new(b.x - a.x, b.z - a.z);
        assert!((step.length() - flock.speed).abs() < 0.05, "{step:?}");
        assert!(step.normalize().dot(Vec2::new(ha.x, ha.z)) > 0.999);
    }

    #[test]
    fn roost_flocks_all_head_the_same_way() {
        for spec in bird_specs().iter().filter(|s| s.group == Group::Roost) {
            assert!(spec.heading.distance(Vec2::new(0.8, 0.6)) < 1e-5);
            assert!(spec.height < 71.0, "low, to the trees");
        }
    }

    #[test]
    fn wings_beat_through_every_flap_frame_and_soarers_glide() {
        let specs = bird_specs();
        let flock = specs.iter().find(|s| s.group == Group::Crosser).unwrap();
        let mut seen = [false; 6];
        for k in 0..400 {
            let f = frame_at(flock, k as f32 * 0.01);
            assert!(f < 6);
            seen[f] = true;
        }
        assert!(seen.iter().all(|s| *s));
        let soarer = specs.iter().find(|s| s.group == Group::Soarer).unwrap();
        assert_eq!(frame_at(soarer, 12.3), 6);
    }

    #[test]
    fn a_bird_changes_visibility_only_out_of_sight() {
        assert!(next_visible(true, false, 100.0, 0.0), "stays while in view");
        assert!(!next_visible(false, true, 100.0, 0.0), "does not pop in");
        assert!(next_visible(true, true, 100.0, 0.0));
        assert!(
            !next_visible(true, false, 250.0, 0.0),
            "gone once fogged out"
        );
        assert!(next_visible(false, true, 250.0, 0.0));
    }

    #[test]
    fn no_bird_in_view_jumps_when_the_camera_crosses_a_wrap_boundary() {
        let specs = bird_specs();
        let seconds = 123.0;
        let mut worst = 0.0_f32;
        // Se mueve la cámara 500 m a lo largo de x, y después a lo largo de z,
        // en pasos de medio metro.
        for axis in [Vec3::X, Vec3::Z] {
            for spec in &specs {
                let at = |k: usize| Vec3::new(37.0, 2.0, -11.0) + axis * (k as f32 * 0.5);
                for k in 0..1000 {
                    let (cam_a, cam_b) = (at(k), at(k + 1));
                    let (a, _, _) = bird_pose(spec, seconds, cam_a);
                    let (b, _, _) = bird_pose(spec, seconds, cam_b);
                    if a.distance(cam_a) < HIDE_RANGE || b.distance(cam_b) < HIDE_RANGE {
                        worst = worst.max(a.distance(b));
                    }
                }
            }
        }
        assert!(worst < 2.0, "a bird in view jumped {worst} m");
    }

    #[test]
    fn a_bird_still_in_range_is_hidden_once_the_gate_has_been_closed_a_while() {
        assert!(next_visible(true, false, 100.0, 0.0), "grace period");
        assert!(next_visible(true, false, 100.0, GATE_GRACE - 1.0));
        assert!(!next_visible(true, false, 100.0, GATE_GRACE), "then gone");
        assert!(next_visible(true, true, 100.0, GATE_GRACE * 2.0));
        assert!(!next_visible(false, true, 100.0, 0.0), "still no pop-in");
    }

    #[test]
    fn a_soarer_circling_a_still_camera_does_not_fly_through_the_night() {
        use bevy::time::TimeUpdateStrategy;
        let mut app = app_at(13);
        app.add_plugins(TransformPlugin);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_secs(10),
        ));
        let soarer = bird_specs()[0];
        // La cámara justo debajo del círculo del que planea: siempre dentro del
        // rango.
        app.world_mut().spawn((
            Camera3d::default(),
            Transform::from_xyz(soarer.anchor.x, 0.0, soarer.anchor.y),
        ));
        for _ in 0..3 {
            app.update();
        }
        let mut q = app.world_mut().query::<(&Bird, &Visibility)>();
        let visible = |app: &App, q: &mut QueryState<(&Bird, &Visibility)>| {
            q.iter(app.world())
                .find(|(b, _)| b.index == 0)
                .map(|(_, v)| *v == Visibility::Inherited)
                .unwrap()
        };
        assert!(visible(&app, &mut q), "out by day");
        app.world_mut().resource_mut::<WorldClock>().0 = GameClock::at(TimeOfDay::hm(22, 0));
        app.update();
        assert!(visible(&app, &mut q), "the grace period");
        for _ in 0..(GATE_GRACE as usize / 10 + 2) {
            app.update();
        }
        assert!(
            !visible(&app, &mut q),
            "gone by the end of the grace period"
        );
    }

    #[test]
    fn the_silhouettes_are_small_and_the_wings_move() {
        let flat = silhouette(0.0);
        let up = silhouette(0.8);
        assert_eq!(flat.count_vertices(), 11);
        assert_eq!(flat.indices().unwrap().len(), 15);
        let top = |m: &Mesh| {
            m.attribute(Mesh::ATTRIBUTE_POSITION)
                .unwrap()
                .as_float3()
                .unwrap()
                .iter()
                .map(|p| p[1])
                .fold(f32::MIN, f32::max)
        };
        assert!(top(&flat).abs() < 1e-6);
        assert!(top(&up) > 0.3);
    }

    fn app_at(hour: u32) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(WorldClock(GameClock::at(TimeOfDay::hm(hour, 0))))
            .add_plugins(BirdsPlugin);
        app.update();
        app.update();
        app
    }

    fn shown(app: &mut App) -> usize {
        app.world_mut()
            .query::<(&Bird, &Visibility)>()
            .iter(app.world())
            .filter(|(_, v)| **v == Visibility::Inherited)
            .count()
    }

    #[test]
    fn the_plugin_spawns_the_flock_and_shows_it_by_the_hour() {
        let mut noon = app_at(13);
        let all = noon.world_mut().query::<&Bird>().iter(noon.world()).count();
        assert_eq!(all, 45);
        assert_eq!(shown(&mut noon), 27);
        assert_eq!(shown(&mut app_at(18)), 18, "dusk: only the roost flocks");
        assert_eq!(shown(&mut app_at(0)), 0);
        let draws = noon
            .world_mut()
            .query::<&Mesh3d>()
            .iter(noon.world())
            .count();
        assert!(draws <= MAX_BIRDS);
        assert_eq!(
            noon.world().resource::<BirdKit>().frames.len(),
            FLAP_FRAMES + 1
        );
    }
}
