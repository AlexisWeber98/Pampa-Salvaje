//! Polvo y salpicaduras en las pisadas: los vasos de los caballos (montados,
//! atados o de tiro), las ruedas de los vehículos y los pies de la gente a pie
//! levantan polvo donde el suelo lo da, y barro o agua en la orilla de una
//! laguna.
//!
//! Es un sistema de partículas propio y liviano: un grupo fijo de bocanadas
//! ([`POOL`] entidades) que comparten una malla (un cuadrado que mira a la
//! cámara girando solo alrededor de la vertical, con la normal hacia arriba
//! para que el sol la ilumine como al suelo) y una textura de polvo grumoso
//! hecha al arrancar; cada bocanada lleva su propio material para su tinte y
//! su transparencia. Con el grupo lleno, se recicla la bocanada más gastada
//! (la que menos se ve).
//!
//! Cada fuente detecta su pisada después de propagar los transforms (la
//! posición mundial de un vaso solo se conoce ahí) y deja una [`Kick`] en
//! [`Kicks`]; en el frame siguiente, antes de propagar, [`stir_dust`] clasifica
//! el suelo bajo cada una ([`footing`]), larga sus bocanadas y mueve todas las
//! vivas. Solo patean las fuentes a [`EMIT_RADIUS`] metros de la cámara o
//! menos, y nunca un caballo congelado.
//!
//! El polvo sube, se hincha, deriva con el viento del pasto
//! ([`crate::grass::WIND`]) y se desvanece; una salpicadura son gotas rápidas,
//! bajas y oscuras que caen enseguida.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::VisibilitySystems;
use bevy::image::ImageSampler;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::transform::TransformSystems;
use pampa_core::town::Walk;
use pampa_core::{Terrain, TownLayout, Vec2 as GroundVec2};

use crate::character::gait::{Bone, HorseBones, PLANTED};
use crate::character::{FigureBody, HorseBody, HorseRig, VisualPose};
use crate::fauna::livestock::view_eye;
use crate::flora::jitter;
use crate::grass::WIND;
use crate::terrain::{WorldTerrain, hash2, lagoon_wetness, road_wear, town_weight};
use crate::town::WorldTowns;
use crate::vehicle::{OfVehicle, Rolled, VehicleWheel};

/// Cuántas bocanadas hay en total: el tope de partículas a la vista, pensado
/// para la iGPU Renoir (un galope por un camino tiene unas 25 vivas).
pub const POOL: usize = 160;

/// Hasta qué distancia de la cámara patea una fuente, en metros: más lejos,
/// una bocanada ya no se distingue.
pub const EMIT_RADIUS: f32 = 70.0;

/// A qué altura sobre su apoyo en reposo cuenta un vaso como apoyado, en
/// metros: la holgura con la que la marcha lo deja bajar ([`PLANTED`]).
const TOUCH: f32 = PLANTED;
/// A qué altura sobre su apoyo un vaso cuenta como levantado otra vez, en
/// metros: la histéresis evita contar dos pisadas en un vaso que tiembla.
const LIFT: f32 = 0.06;

/// Metros que rueda una rueda entre dos bocanadas.
const WHEEL_SPACING: f32 = 1.4;
/// Un salto de lo rodado mayor que esto (en metros) es un vehículo que
/// apareció o se movió de golpe, no una rueda que rodó: no patea.
const WHEEL_JUMP: f32 = 6.0;

/// Velocidad a la que un caballo patea con toda su fuerza, en m/s: el galope
/// tendido del jinete (49 km/h).
const HOOF_FULL: f32 = 13.6;
/// Velocidad a la que una rueda levanta todo su polvo, en m/s.
const WHEEL_FULL: f32 = 8.0;
/// Velocidad a la que un pie levanta todo su polvo, en m/s.
const FOOT_FULL: f32 = 5.0;
/// El polvo mínimo del suelo para que un pie lo levante (de 0 a 1): la gente
/// solo levanta polvo en la tierra suelta, no en el pasto.
const FOOT_DUST: f32 = 0.5;
/// Separación lateral entre los dos pies de una persona, en metros.
const FOOT_SPREAD: f32 = 0.11;

/// Polvo de un camino o de una calle de tierra (de 0 a 1).
const ROAD_DUST: f32 = 1.0;
/// Polvo de una vereda de tierra apisonada.
const EARTH_WALK_DUST: f32 = 0.6;
/// Polvo del resto del suelo nivelado de un pueblo (baldíos, patios).
const TOWN_DUST: f32 = 0.75;
/// Polvo del pasto verde de la pampa, y cuánto más da el pasto del todo seco.
const GRASS_DUST: [f32; 2] = [0.08, 0.3];
/// Humedad a partir de la cual la orilla de una laguna es barro que salpica
/// (de 0 a 1, ver [`lagoon_wetness`]); más seca, solo apaga el polvo.
const MUD_WET: f32 = 0.35;

/// Bocanadas de polvo por pisada con toda la fuerza y todo el polvo.
const DUST_PER_KICK: f32 = 1.5;
/// Gotas por salpicadura en el barro más mojado o en el agua, sin fuerza y
/// con toda; el barro menos mojado salpica en proporción a su humedad.
const DROPS: [f32; 2] = [3.0, 9.0];
/// Cuántas tiradas al azar tiene cada partícula de una pisada.
const DRAWS: u32 = 10;

/// Freno del aire sobre el empuje inicial de una bocanada de polvo, en 1/s.
const DRAG: f32 = 2.5;
/// Cuánto sube una bocanada de polvo por sí sola, en m/s.
const RISE: f32 = 0.04;
/// Cuánto la arrastra el viento, en m/s.
const DRIFT: f32 = 0.5;
/// La gravedad sobre una gota, en m/s².
const GRAVITY: f32 = 9.8;
/// Qué fracción de su vida tarda una bocanada en aparecer del todo.
const FADE_IN: f32 = 0.08;

/// El polvo seco de la tierra (sRGB, iluminado como el suelo): algo más claro
/// que la tierra que lo dio, porque el polvo en el aire dispersa la luz.
const DUST_TINT: Vec3 = Vec3::new(0.86, 0.78, 0.66);
/// El polvo del pasto: más gris y oscuro que el de la tierra.
const GRASS_TINT: Vec3 = Vec3::new(0.74, 0.70, 0.61);
/// El polvo del suelo (de 0 a 1) desde el que una bocanada tiene el color de
/// la tierra; entre el pasto verde y esto, pasa del color del pasto al de la
/// tierra.
const EARTH_TINT_DUST: f32 = 0.7;
/// El barro mojado de la orilla de una laguna.
const MUD_TINT: Vec3 = Vec3::new(0.20, 0.16, 0.11);
/// El agua turbia de una laguna pampeana.
const WATER_TINT: Vec3 = Vec3::new(0.62, 0.64, 0.60);

/// Lado de la textura de polvo, en píxeles.
const TEXTURE_SIZE: usize = 128;
/// Cuántas texturas de polvo distintas se reparten las bocanadas.
const TEXTURES: usize = 3;

/// Lo que un punto del suelo levanta cuando lo pisan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Footing {
    /// Suelo firme que no levanta nada: ladrillo, senderos de plaza, césped
    /// regado.
    Firm,
    /// Tierra suelta o pasto que levanta polvo, cuánto de 0 a 1.
    Dust(f32),
    /// El barro de la orilla de una laguna, qué tan mojado de 0 a 1.
    Mud(f32),
    /// El agua baja de una laguna.
    Water,
}

/// Qué levanta una pisada en `g`: el agua y el barro de las lagunas primero,
/// después lo firme de un pueblo (veredas de ladrillo, plazas), la tierra de
/// sus calles, veredas de tierra y baldíos, la de los caminos y, en el resto,
/// el pasto, con más polvo cuanto más seco ([`grass_dryness`](pampa_core::flora::grass_dryness)).
pub fn footing(terrain: &Terrain, towns: &[TownLayout], g: GroundVec2) -> Footing {
    if terrain
        .lagoons
        .iter()
        .any(|l| l.touches(g) && l.offshore(g) < 0.0)
    {
        return Footing::Water;
    }
    let wet = lagoon_wetness(terrain, g);
    if wet >= MUD_WET {
        return Footing::Mud(wet);
    }
    let dryness = pampa_core::flora::grass_dryness(terrain, g);
    let mut dust = GRASS_DUST[0] + GRASS_DUST[1] * dryness;
    let town = town_weight(terrain, g);
    if town > 0.0 {
        match town_footing(towns, g) {
            Some(Footing::Dust(d)) => dust = dust.max(d),
            Some(other) => return other,
            None => dust = dust.max(TOWN_DUST * town),
        }
    }
    dust = dust.max(ROAD_DUST * road_wear(terrain, g));
    Footing::Dust(dust * (1.0 - wet / MUD_WET))
}

/// Lo que hay en `g` dentro de un pueblo: las plazas (sus senderos y su
/// césped) y las veredas de ladrillo son firmes; las veredas de tierra y las
/// calles dan polvo; `None` fuera de todo eso.
fn town_footing(towns: &[TownLayout], g: GroundVec2) -> Option<Footing> {
    for town in towns {
        if town.plazas().any(|p| p.contains_point(g)) {
            return Some(Footing::Firm);
        }
        if let Some(walk) = town.sidewalks().iter().find(|s| s.area.contains_point(g)) {
            return Some(match walk.walk {
                Walk::Brick => Footing::Firm,
                Walk::Earth => Footing::Dust(EARTH_WALK_DUST),
            });
        }
        if town.streets.iter().any(|s| s.contains_point(g)) {
            return Some(Footing::Dust(ROAD_DUST));
        }
    }
    None
}

/// Qué pisa: cuánto suelo mueve y cómo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Hoof,
    Wheel,
    Foot,
}

impl Source {
    /// El tamaño de lo que levanta, relativo a un vaso.
    fn size(self) -> f32 {
        match self {
            Self::Hoof => 1.0,
            Self::Wheel => 1.25,
            Self::Foot => 0.45,
        }
    }
}

/// Una pisada: dónde (en el mundo), hacia dónde va lo que pisa (unitario,
/// horizontal) y con cuánta fuerza, de 0 a 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Kick {
    pub source: Source,
    pub at: Vec3,
    pub travel: Vec3,
    pub strength: f32,
}

/// Las pisadas de este frame, que [`stir_dust`] convierte en bocanadas en el
/// siguiente.
#[derive(Resource, Debug, Default)]
pub struct Kicks {
    pending: Vec<Kick>,
    /// Cuenta todas las pisadas: la semilla de la suerte de cada una.
    count: u32,
}

impl Kicks {
    /// Agrega una pisada.
    pub fn push(&mut self, kick: Kick) {
        self.pending.push(kick);
    }
}

/// Cómo se mueve una partícula.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// Una bocanada de polvo o de niebla: se frena, sube, se hincha y deriva.
    Cloud,
    /// Una gota: vuela y cae.
    Drop,
}

/// Una partícula viva (o gastada, con `age >= life`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Puff {
    pub motion: Motion,
    /// Dónde nació, en el mundo (el centro de la bocanada o de la gota).
    pub from: Vec3,
    /// Su empuje inicial, en m/s.
    pub velocity: Vec3,
    /// Segundos desde que nació, y cuántos vive.
    pub age: f32,
    pub life: f32,
    /// Su lado al nacer y al final, en metros.
    pub size: [f32; 2],
    /// Su opacidad plena, de 0 a 1.
    pub opacity: f32,
    /// Su color (sRGB).
    pub tint: Vec3,
}

impl Puff {
    /// Una partícula ya gastada, para llenar el grupo.
    pub const SPENT: Self = Self {
        motion: Motion::Cloud,
        from: Vec3::ZERO,
        velocity: Vec3::ZERO,
        age: 1.0,
        life: 1.0,
        size: [0.0; 2],
        opacity: 0.0,
        tint: Vec3::ZERO,
    };

    /// Qué fracción de su vida gastó: 1 o más si ya se desvaneció.
    pub fn spent(&self) -> f32 {
        if self.life <= 0.0 {
            1.0
        } else {
            self.age / self.life
        }
    }

    /// Dónde está, su lado y su opacidad ahora.
    pub fn place(&self) -> (Vec3, f32, f32) {
        let t = self.age;
        let u = self.spent().clamp(0.0, 1.0);
        match self.motion {
            Motion::Cloud => {
                let push = self.velocity * (1.0 - (-DRAG * t).exp()) / DRAG;
                let wind = Vec3::new(WIND.x, 0.0, WIND.y) * DRIFT * t;
                let at = self.from + push + wind + Vec3::Y * RISE * t;
                let grow = 1.0 - (1.0 - u) * (1.0 - u);
                let size = self.size[0] + (self.size[1] - self.size[0]) * grow;
                let alpha = self.opacity * (u / FADE_IN).min(1.0) * (1.0 - u).powf(1.6);
                (at, size, alpha)
            }
            Motion::Drop => {
                let mut at = self.from + self.velocity * t - Vec3::Y * 0.5 * GRAVITY * t * t;
                // En el suelo se queda: una gota no se hunde en el barro.
                at.y = at.y.max(self.from.y - 0.02);
                (at, self.size[0], self.opacity * (1.0 - u).sqrt())
            }
        }
    }
}

/// Qué partícula del grupo usar para una nueva: la más gastada (una ya
/// desvanecida si la hay, la primera de ellas), la que menos se nota al
/// desaparecer.
pub fn recycle(spent: &[f32]) -> usize {
    let mut best = 0;
    for (i, &s) in spent.iter().enumerate() {
        if s >= 1.0 {
            return i;
        }
        if s > spent[best] {
            best = i;
        }
    }
    best
}

/// Las partículas que levanta `kick` sobre `footing`; `salt` elige su suerte.
pub fn plume(kick: &Kick, footing: Footing, salt: u32) -> Vec<Puff> {
    let size = kick.source.size();
    let s = kick.strength.clamp(0.0, 1.0);
    let r = |i: u32| jitter(salt, i);
    // Un desvío horizontal al azar de hasta `reach` metros; usa dos tiradas,
    // `i` e `i + 1`. Cada partícula tiene [`DRAWS`] tiradas propias, desde
    // `k`, y ninguna se usa para dos cosas.
    let scatter = |i: u32, reach: f32| {
        let (sin, cos) = (r(i) * std::f32::consts::TAU).sin_cos();
        Vec3::new(cos, 0.0, sin) * reach * r(i + 1).sqrt()
    };
    match footing {
        Footing::Firm => Vec::new(),
        Footing::Dust(dust) => {
            if kick.source == Source::Foot && dust < FOOT_DUST {
                return Vec::new();
            }
            let earth = (dust - GRASS_DUST[0]) / (EARTH_TINT_DUST - GRASS_DUST[0]);
            let tint = GRASS_TINT.lerp(DUST_TINT, earth.clamp(0.0, 1.0));
            let count = (dust * s * size * DUST_PER_KICK + r(0)).floor() as u32;
            (0..count)
                .map(|n| {
                    let k = DRAWS * (n + 1);
                    let start = (0.12 + 0.08 * r(k)) * size;
                    let back = -kick.travel * (0.3 + 0.9 * s) * (0.5 + r(k + 1));
                    let up = Vec3::Y * (0.1 + 0.35 * s) * (0.5 + r(k + 2));
                    Puff {
                        motion: Motion::Cloud,
                        from: kick.at + scatter(k + 3, 0.15 * size) + Vec3::Y * start * 0.4,
                        velocity: back + up + scatter(k + 5, 0.3 * s),
                        age: 0.0,
                        life: (0.8 + 1.0 * s) * (0.7 + 0.6 * r(k + 7)),
                        size: [start, (0.35 + 0.6 * s) * size * (0.7 + 0.6 * r(k + 8))],
                        opacity: (0.25 + 0.35 * dust) * (0.6 + 0.4 * s),
                        tint: tint * (0.92 + 0.16 * r(k + 9)),
                    }
                })
                .collect()
        }
        Footing::Mud(_) | Footing::Water if kick.source == Source::Foot => Vec::new(),
        Footing::Mud(_) | Footing::Water => {
            let water = footing == Footing::Water;
            let tint = if water { WATER_TINT } else { MUD_TINT };
            // El barro más mojado salpica más; el agua, como el barro más mojado.
            let wet = match footing {
                Footing::Mud(wet) => wet.clamp(0.0, 1.0),
                _ => 1.0,
            };
            let count = ((DROPS[0] + (DROPS[1] - DROPS[0]) * s * size) * wet + r(0)).floor() as u32;
            let mut puffs: Vec<Puff> = (0..count)
                .map(|n| {
                    let k = DRAWS * (n + 1);
                    let out = scatter(k, 1.0).normalize_or_zero() * (0.4 + 1.4 * s) * r(k + 2)
                        - kick.travel * 0.6 * s;
                    Puff {
                        motion: Motion::Drop,
                        from: kick.at + scatter(k + 3, 0.12 * size) + Vec3::Y * 0.04,
                        velocity: out + Vec3::Y * (1.0 + 2.2 * s) * (0.5 + r(k + 5)),
                        age: 0.0,
                        life: 0.35 + 0.35 * r(k + 6),
                        size: [(0.04 + 0.05 * r(k + 7)) * size; 2],
                        opacity: if water { 0.55 } else { 0.95 },
                        tint: tint * (0.85 + 0.3 * r(k + 8)),
                    }
                })
                .collect();
            // Una nube baja de rocío (agua) o de barro fino (barro), que se
            // asienta enseguida.
            puffs.push(Puff {
                motion: Motion::Cloud,
                from: kick.at + Vec3::Y * 0.1,
                velocity: -kick.travel * 0.6 * s + Vec3::Y * 0.3 * s,
                age: 0.0,
                life: 0.5 + 0.4 * s,
                size: [0.2 * size, (0.5 + 0.6 * s) * size],
                opacity: if water { 0.3 } else { 0.25 },
                tint: if water {
                    WATER_TINT * 1.15
                } else {
                    MUD_TINT * 1.6
                },
            });
            puffs
        }
    }
}

/// Qué vasos tocaron el suelo recién: cada `height` es la altura del extremo
/// de una pata sobre su apoyo en reposo; un vaso apoyado (`planted`) se
/// levanta pasando [`LIFT`] y toca el suelo al bajar a [`TOUCH`] o menos.
/// Actualiza `planted`.
pub fn touchdowns(planted: &mut [bool; 4], height: [f32; 4]) -> [bool; 4] {
    let mut touched = [false; 4];
    for leg in 0..4 {
        if planted[leg] {
            planted[leg] = height[leg] <= LIFT;
        } else if height[leg] <= TOUCH {
            planted[leg] = true;
            touched[leg] = true;
        }
    }
    touched
}

/// Las pisadas de un caballo en este frame: `height` como en [`touchdowns`],
/// o `None` mientras no patea (congelado, sin empezar, lejos de la cámara, o
/// sin sus huesos a mano en este frame).
/// Mientras no patea se olvida `planted`, y la primera vez que vuelve a
/// patear lo siembra sin pisar: un vaso que bajó a la vista no es una pisada.
pub fn hoof_step(planted: &mut Option<[bool; 4]>, height: Option<[f32; 4]>) -> [bool; 4] {
    match (height, planted.as_mut()) {
        (None, _) => {
            *planted = None;
            [false; 4]
        }
        (Some(height), None) => {
            let mut seeded = [false; 4];
            touchdowns(&mut seeded, height);
            *planted = Some(seeded);
            [false; 4]
        }
        (Some(height), Some(planted)) => touchdowns(planted, height),
    }
}

/// Si el andar de una persona pasó por una pisada entre la fase `was` y `now`
/// (en ciclos, con vuelta en 1): pisa en la fase 0 y en la 0.5, hacia adelante
/// o hacia atrás. Basta con ver si cambió de medio ciclo: un paso que da la
/// vuelta cruza la fase 0, que también es una pisada.
pub fn stepped(was: f32, now: f32) -> bool {
    (was * 2.0).floor() != (now * 2.0).floor()
}

/// Si una rueda patea ahora, con `last` lo que llevaba rodado su vehículo en
/// su última bocanada y `rolled` lo que lleva rodado ahora: patea cada
/// [`WHEEL_SPACING`] metros; un salto mayor que [`WHEEL_JUMP`] reinicia la
/// cuenta sin patear. Devuelve el nuevo `last` y si patea.
pub fn rolled_kick(last: f32, rolled: f32) -> (f32, bool) {
    let d = (rolled - last).abs();
    if d > WHEEL_JUMP {
        (rolled, false)
    } else if d >= WHEEL_SPACING {
        (rolled, true)
    } else {
        (last, false)
    }
}

/// Las partículas del grupo, en el orden en que se crearon.
#[derive(Resource, Debug, Default)]
struct DustPool(Vec<Entity>);

/// Una partícula del grupo y su material propio.
#[derive(Component, Debug, Clone)]
struct DustPuff {
    puff: Puff,
    material: Handle<StandardMaterial>,
}

/// Si los vasos de un caballo están apoyados, en el orden de
/// [`LEGS`](crate::character::gait::LEGS), o `None` mientras no patea (ver
/// [`hoof_step`]); en la raíz de su modelo.
#[derive(Component, Debug, Clone, Copy)]
struct Hooves(Option<[bool; 4]>);

/// La fase del andar de una persona en el frame anterior; en su cuerpo.
#[derive(Component, Debug, Clone, Copy)]
struct Stride(f32);

/// Lo que llevaba rodado el vehículo de una rueda en su última bocanada; en
/// la rueda.
#[derive(Component, Debug, Clone, Copy)]
struct Track(f32);

/// Polvo y salpicaduras en las pisadas (ver el módulo).
pub struct DustPlugin;

impl Plugin for DustPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Kicks>()
            .init_resource::<DustPool>()
            .add_systems(Startup, spawn_pool)
            .add_systems(PostUpdate, stir_dust.before(TransformSystems::Propagate))
            .add_systems(
                PostUpdate,
                (kick_hooves, kick_wheels, kick_feet)
                    .after(TransformSystems::Propagate)
                    .before(VisibilitySystems::VisibilityPropagate),
            );
    }
}

/// Crea el grupo de partículas, ocultas, con su malla y su textura
/// compartidas.
fn spawn_pool(
    mut commands: Commands,
    mut pool: ResMut<DustPool>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut quad = Mesh::from(Rectangle::new(1.0, 1.0));
    // La normal hacia arriba: el sol la ilumina como al suelo, se mire desde
    // donde se mire.
    quad.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 4]);
    let mesh = meshes.add(quad);
    let textures: Vec<Handle<Image>> = (0..TEXTURES)
        .map(|k| images.add(dust_image(k as f32 * 17.3)))
        .collect();
    pool.0 = (0..POOL)
        .map(|i| {
            let material = materials.add(StandardMaterial {
                base_color: Color::NONE,
                base_color_texture: Some(textures[i % TEXTURES].clone()),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 1.0,
                reflectance: 0.1,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            commands
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    DustPuff {
                        puff: Puff::SPENT,
                        material,
                    },
                    Transform::default(),
                    Visibility::Hidden,
                    NotShadowCaster,
                    NotShadowReceiver,
                ))
                .id()
        })
        .collect();
}

/// Una mota de polvo grumosa, para teñirla en el material: un borde suave y
/// grumos de ruido de valor de tres octavas (sembrado por `salt`) en su
/// opacidad, y algo más oscura abajo y entre los grumos, donde la nube se hace
/// sombra a sí misma.
fn dust_image(salt: f32) -> Image {
    let n = TEXTURE_SIZE;
    let lattice = |x: f32, y: f32, cell: f32, salt: f32| {
        let (gx, gy) = (x / cell, y / cell);
        let (ix, iy) = (gx.floor(), gy.floor());
        let (fx, fy) = (gx - ix, gy - iy);
        let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
        let h = |a: f32, b: f32| hash2(ix + a + salt, iy + b - salt);
        let top = h(0.0, 0.0) + (h(1.0, 0.0) - h(0.0, 0.0)) * sx;
        let bottom = h(0.0, 1.0) + (h(1.0, 1.0) - h(0.0, 1.0)) * sx;
        top + (bottom - top) * sy
    };
    let mut data = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let half = n as f32 / 2.0;
            let r = ((px - half).hypot(py - half) / half).min(1.0);
            let edge = 1.0 - r * r * (3.0 - 2.0 * r);
            let grain = 0.5 * lattice(px, py, 24.0, 3.1 + salt)
                + 0.3 * lattice(px, py, 10.0, 7.7 + salt)
                + 0.2 * lattice(px, py, 4.0, 1.9 + salt);
            let lumps = ((grain - 0.2) / 0.6).clamp(0.0, 1.0);
            let alpha = (edge.powf(1.3) * (0.25 + 1.1 * lumps)).clamp(0.0, 1.0);
            let shade = (0.78 + 0.12 * lumps + 0.1 * (1.0 - py / n as f32)) * 255.0;
            let shade = shade.min(255.0) as u8;
            data.extend_from_slice(&[shade, shade, shade, (alpha * 255.0) as u8]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: n as u32,
            height: n as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image
}

/// Larga las bocanadas de las pisadas del frame anterior sobre el suelo que
/// pisaron, y mueve, gira hacia la cámara y desvanece las vivas.
#[allow(clippy::too_many_arguments)]
fn stir_dust(
    time: Res<Time>,
    terrain: Option<Res<WorldTerrain>>,
    towns: Option<Res<WorldTowns>>,
    mut kicks: ResMut<Kicks>,
    pool: Res<DustPool>,
    cameras: Query<(Entity, &GlobalTransform, &Camera), With<Camera3d>>,
    mut puffs: Query<(&mut DustPuff, &mut Transform, &mut Visibility)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    let mut spent: Vec<f32> = pool
        .0
        .iter()
        .map(|&e| puffs.get(e).map_or(0.0, |(p, ..)| p.puff.spent()))
        .collect();
    let towns: &[TownLayout] = towns.as_deref().map_or(&[], |t| &t.0);
    let pending = std::mem::take(&mut kicks.pending);
    // Sin terreno todavía no hay suelo que pisar: las pisadas se pierden.
    if let Some(world) = terrain.as_deref() {
        for kick in &pending {
            kicks.count = kicks.count.wrapping_add(1);
            let g = GroundVec2::new(kick.at.x, -kick.at.z);
            let born = plume(kick, footing(&world.0, towns, g), kicks.count);
            for puff in born {
                let slot = recycle(&spent);
                spent[slot] = 0.0;
                if let Ok((mut p, ..)) = puffs.get_mut(pool.0[slot]) {
                    // Recién nacida: su paso de este frame la mueve.
                    p.puff = Puff { age: -dt, ..puff };
                }
            }
        }
    }
    kicks.pending = pending;
    kicks.pending.clear();
    let eye = view_eye(cameras);
    for (mut p, mut transform, mut visibility) in &mut puffs {
        if p.puff.spent() >= 1.0 {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        p.puff.age += dt;
        let (at, size, alpha) = p.puff.place();
        if p.puff.spent() >= 1.0 || alpha <= 0.0 {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        let toward = eye.map_or(Vec3::Z, |e| e - at);
        transform.translation = at;
        transform.rotation = Quat::from_rotation_y(toward.x.atan2(toward.z));
        transform.scale = Vec3::splat(size);
        visibility.set_if_neq(Visibility::Inherited);
        if let Some(mut material) = materials.get_mut(&p.material) {
            let t = p.puff.tint;
            material.base_color = Color::srgba(t.x, t.y, t.z, alpha);
        }
    }
}

/// Si `at` está lo bastante cerca de la cámara para patear.
fn near_eye(eye: Option<Vec3>, at: Vec3) -> bool {
    eye.is_some_and(|e| e.distance_squared(at) <= EMIT_RADIUS * EMIT_RADIUS)
}

/// Las pisadas de los vasos de cada caballo con su modelo animado (montado,
/// atado o de tiro) cerca de la cámara: cada vaso que vuelve a apoyarse.
#[allow(clippy::type_complexity)]
fn kick_hooves(
    mut commands: Commands,
    mut kicks: ResMut<Kicks>,
    cameras: Query<(Entity, &GlobalTransform, &Camera), With<Camera3d>>,
    mut rigs: Query<(
        Entity,
        &HorseRig,
        &HorseBones,
        &GlobalTransform,
        &ChildOf,
        Option<&mut Hooves>,
    )>,
    bodies: Query<(&HorseBody, &GlobalTransform)>,
    globals: Query<&GlobalTransform>,
) {
    let eye = view_eye(cameras);
    for (entity, rig, bones, root, child_of, hooves) in &mut rigs {
        let Some(mut hooves) = hooves else {
            commands.entity(entity).insert(Hooves(None));
            continue;
        };
        let Ok((body, body_at)) = bodies.get(child_of.parent()) else {
            continue;
        };
        let kicking = !rig.frozen && rig.started && near_eye(eye, root.translation());
        let ends = globals
            .get_many(std::array::from_fn::<_, 4, _>(|leg| {
                bones.bones[Bone::End(leg).index()]
            }))
            .map(|e| e.map(GlobalTransform::translation))
            .ok()
            .filter(|_| kicking);
        let into_body = root.affine().inverse();
        let height = ends.map(|ends| {
            std::array::from_fn(|leg| into_body.transform_point3(ends[leg]).y - bones.standing[leg])
        });
        let touched = hoof_step(&mut hooves.0, height);
        let Some(ends) = ends else {
            continue;
        };
        if body.flight.is_some() {
            continue;
        }
        let travel = horizontal(body_at.rotation() * Vec3::X);
        let strength = (0.12 + body.speed.abs() / HOOF_FULL).min(1.0);
        for leg in (0..4).filter(|&l| touched[l]) {
            kicks.push(Kick {
                source: Source::Hoof,
                at: ends[leg],
                travel: travel * body.speed.signum(),
                strength,
            });
        }
    }
}

/// La dirección horizontal unitaria de `v` (el este si es vertical).
fn horizontal(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z).try_normalize().unwrap_or(Vec3::X)
}

/// Las bocanadas de las ruedas de los vehículos que se mueven cerca de la
/// cámara: una cada [`WHEEL_SPACING`] metros rodados, donde la rueda toca el
/// suelo.
fn kick_wheels(
    mut commands: Commands,
    mut kicks: ResMut<Kicks>,
    cameras: Query<(Entity, &GlobalTransform, &Camera), With<Camera3d>>,
    mut wheels: Query<(
        Entity,
        &VehicleWheel,
        &OfVehicle,
        &GlobalTransform,
        Option<&mut Track>,
    )>,
    vehicles: Query<(&VisualPose, &Rolled)>,
) {
    let eye = view_eye(cameras);
    for (entity, wheel, of, at, track) in &mut wheels {
        let Ok((pose, rolled)) = vehicles.get(of.0) else {
            continue;
        };
        let Some(mut track) = track else {
            commands.entity(entity).insert(Track(rolled.0));
            continue;
        };
        let (last, kick) = rolled_kick(track.0, rolled.0);
        track.0 = last;
        let contact = at.translation() - Vec3::Y * wheel.radius * at.scale().y;
        if !kick || pose.speed == 0.0 || !near_eye(eye, contact) {
            continue;
        }
        let forward = crate::world_from_ground(pose.heading.forward(), 0.0);
        kicks.push(Kick {
            source: Source::Wheel,
            at: contact,
            travel: forward * pose.speed.signum(),
            strength: (pose.speed.abs() / WHEEL_FULL).clamp(0.15, 1.0),
        });
    }
}

/// Las pisadas de la gente a pie cerca de la cámara, en las fases 0 y 0.5 de
/// su andar (un jinete no tiene [`FigureBody`]).
fn kick_feet(
    mut commands: Commands,
    mut kicks: ResMut<Kicks>,
    cameras: Query<(Entity, &GlobalTransform, &Camera), With<Camera3d>>,
    mut bodies: Query<(
        Entity,
        &FigureBody,
        &ChildOf,
        &GlobalTransform,
        Option<&mut Stride>,
    )>,
    poses: Query<&VisualPose>,
) {
    let eye = view_eye(cameras);
    for (entity, body, child_of, at, stride) in &mut bodies {
        let Some(mut stride) = stride else {
            commands.entity(entity).insert(Stride(body.phase));
            continue;
        };
        let was = std::mem::replace(&mut stride.0, body.phase);
        let Ok(pose) = poses.get(child_of.parent()) else {
            continue;
        };
        if pose.speed == 0.0 || !stepped(was, body.phase) || !near_eye(eye, at.translation()) {
            continue;
        }
        let forward = horizontal(at.rotation() * Vec3::X);
        // La fase 0 es un pie, la 0.5 el otro.
        let side = if (body.phase * 2.0).floor() as i32 % 2 == 0 {
            1.0
        } else {
            -1.0
        };
        let foot = at.translation() + forward.cross(Vec3::Y) * FOOT_SPREAD * side;
        kicks.push(Kick {
            source: Source::Foot,
            at: Vec3::new(foot.x, foot.y.min(at.translation().y), foot.z),
            travel: forward * pose.speed.signum(),
            strength: (pose.speed.abs() / FOOT_FULL).clamp(0.1, 1.0),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pampa_core::{Aabb, Lagoon, LagoonConfig, RoadNetwork, plaza_paths};
    use std::sync::Arc;

    /// Un pueblo nivelado, un camino recto lejos de él y una laguna.
    fn world() -> (Terrain, Vec<TownLayout>, Lagoon) {
        let town = TownLayout::castex_inspired();
        let base = Terrain::default();
        let lagoon = Lagoon::new(
            &LagoonConfig {
                name: "Laguna".into(),
                center: GroundVec2::new(3401.0, 841.0),
                size: GroundVec2::new(215.0, 145.0),
                seed: 5,
            },
            &base,
        );
        let road = pampa_core::Road {
            kind: pampa_core::RoadKind::Rural,
            from: 0,
            to: 0,
            path: vec![
                GroundVec2::new(-3000.0, -2000.0),
                GroundVec2::new(-2000.0, -2000.0),
            ],
            ends: [pampa_core::RoadEnd {
                at: GroundVec2::new(-3000.0, -2000.0),
                out: GroundVec2::new(-1.0, 0.0),
            }; 2],
            crossings: Vec::new(),
        };
        let terrain = base
            .clone()
            .with_flat_area(town.flat_area(&base))
            .with_lagoon(lagoon.clone())
            .with_roads(Arc::new(RoadNetwork::new(vec![road], &["Rural"])));
        (terrain, vec![town], lagoon)
    }

    fn dust(f: Footing) -> f32 {
        match f {
            Footing::Dust(d) => d,
            other => panic!("expected dust, got {other:?}"),
        }
    }

    #[test]
    fn the_lagoon_splashes_water_then_mud_then_dries_into_grass() {
        let (terrain, towns, lagoon) = world();
        for p in lagoon.ring(12, -2.0) {
            assert_eq!(footing(&terrain, &towns, p), Footing::Water);
        }
        for p in lagoon.ring(12, 0.5) {
            assert!(matches!(footing(&terrain, &towns, p), Footing::Mud(w) if w > 0.9));
        }
        for p in lagoon.ring(12, Lagoon::MUD_BAND + 1.0) {
            assert!(dust(footing(&terrain, &towns, p)) < 0.4);
        }
    }

    #[test]
    fn roads_and_streets_are_dusty_and_the_pampa_grass_barely() {
        let (terrain, towns, _) = world();
        let road = dust(footing(&terrain, &towns, GroundVec2::new(-2500.0, -2000.5)));
        assert_eq!(road, ROAD_DUST);
        let grass = dust(footing(&terrain, &towns, GroundVec2::new(-2500.0, -1950.0)));
        assert!((GRASS_DUST[0]..=GRASS_DUST[0] + GRASS_DUST[1]).contains(&grass));
        let street = towns[0].streets[0].center();
        assert_eq!(dust(footing(&terrain, &towns, street)), ROAD_DUST);
    }

    #[test]
    fn plazas_and_brick_walks_are_firm_and_earth_walks_dusty() {
        let (terrain, towns, _) = world();
        let town = &towns[0];
        let lawn = town.plaza.min + GroundVec2::new(2.0, 2.0);
        assert_eq!(footing(&terrain, &towns, lawn), Footing::Firm);
        let path = plaza_paths(town.plaza)[0].center();
        assert_eq!(footing(&terrain, &towns, path), Footing::Firm);
        // Un punto de cada clase de vereda que no caiga en una plaza.
        let walk = |kind: Walk| {
            town.sidewalks()
                .iter()
                .filter(|s| s.walk == kind)
                .map(|s| s.area.center())
                .find(|&c| !town.plazas().any(|p: Aabb| p.contains_point(c)))
        };
        let brick = walk(Walk::Brick).expect("a brick walk");
        assert_eq!(footing(&terrain, &towns, brick), Footing::Firm);
        let earth = walk(Walk::Earth).expect("an earth walk");
        assert_eq!(
            footing(&terrain, &towns, earth),
            Footing::Dust(EARTH_WALK_DUST)
        );
    }

    fn kick(source: Source, strength: f32) -> Kick {
        Kick {
            source,
            at: Vec3::ZERO,
            travel: Vec3::X,
            strength,
        }
    }

    #[test]
    fn a_gallop_kicks_more_dust_than_a_walk_and_firm_ground_none() {
        let count = |s: f32| -> usize {
            (0..64)
                .map(|salt| plume(&kick(Source::Hoof, s), Footing::Dust(1.0), salt).len())
                .sum()
        };
        assert!(
            count(1.0) > 3 * count(0.2),
            "{} vs {}",
            count(1.0),
            count(0.2)
        );
        assert!(plume(&kick(Source::Hoof, 1.0), Footing::Firm, 1).is_empty());
        // A pie, solo en la tierra suelta.
        let foot = |d: f32| -> usize {
            (0..64)
                .map(|salt| plume(&kick(Source::Foot, 1.0), Footing::Dust(d), salt).len())
                .sum()
        };
        assert_eq!(foot(0.3), 0);
        assert!(foot(1.0) > 0);
    }

    #[test]
    fn mud_splashes_falling_drops_that_dust_never_does() {
        let drops = plume(&kick(Source::Hoof, 1.0), Footing::Mud(1.0), 3);
        assert!(drops.iter().filter(|p| p.motion == Motion::Drop).count() >= DROPS[0] as usize);
        let dust = plume(&kick(Source::Hoof, 1.0), Footing::Dust(1.0), 3);
        assert!(dust.iter().all(|p| p.motion == Motion::Cloud));
        assert!(plume(&kick(Source::Foot, 1.0), Footing::Mud(1.0), 3).is_empty());
        // El barro más mojado salpica más.
        let splash = |wet: f32| -> usize {
            (0..64)
                .map(|salt| plume(&kick(Source::Hoof, 1.0), Footing::Mud(wet), salt).len())
                .sum()
        };
        assert!(
            splash(1.0) > splash(MUD_WET) * 2,
            "{} vs {}",
            splash(1.0),
            splash(MUD_WET)
        );
    }

    #[test]
    fn a_dust_cloud_fades_in_swells_rises_and_fades_out() {
        let mut puff = plume(&kick(Source::Hoof, 1.0), Footing::Dust(1.0), 2)[0];
        let at = |p: &Puff, age: f32| Puff { age, ..*p }.place();
        let (born, start, a0) = at(&puff, 0.0);
        let (_, mid, a1) = at(&puff, puff.life * 0.3);
        let (high, end, a2) = at(&puff, puff.life * 0.99);
        assert_eq!(a0, 0.0);
        assert!(a1 > a2 && a2 < 0.02, "{a1} {a2}");
        assert!(start < mid && mid < end);
        assert!(high.y > born.y);
        puff.age = puff.life;
        assert!(puff.spent() >= 1.0);
    }

    #[test]
    fn a_drop_flies_up_and_lands() {
        let drop = plume(&kick(Source::Hoof, 1.0), Footing::Mud(1.0), 5)[0];
        let at = |age: f32| Puff { age, ..drop }.place().0.y;
        assert!(at(0.1) > drop.from.y);
        assert!(at(2.0) >= drop.from.y - 0.02 && at(2.0) < drop.from.y);
    }

    #[test]
    fn a_full_pool_recycles_the_most_spent_puff_after_any_free_one() {
        assert_eq!(recycle(&[0.2, 1.0, 0.9, 1.3]), 1);
        assert_eq!(recycle(&[0.2, 0.95, 0.4]), 1);
        assert_eq!(recycle(&[0.0; 3]), 0);
    }

    #[test]
    fn a_hoof_touches_down_once_per_landing() {
        let mut planted = [true, false, false, true];
        let lifted = [0.2, 0.2, 0.0, 0.03];
        // Apoyado y quieto no cuenta; al bajar, una vez.
        assert_eq!(
            touchdowns(&mut planted, lifted),
            [false, false, true, false]
        );
        assert_eq!(touchdowns(&mut planted, lifted), [false; 4]);
        // Un temblor dentro de la histéresis no es otra pisada.
        assert_eq!(touchdowns(&mut planted, [0.2, 0.2, 0.04, 0.0]), [false; 4]);
        // Levantado del todo, vuelve a pisar.
        touchdowns(&mut planted, [0.2, 0.2, 0.3, 0.3]);
        assert_eq!(touchdowns(&mut planted, [0.0; 4]), [true, true, true, true]);
    }

    #[test]
    fn a_horse_back_in_view_does_not_kick_with_a_hoof_that_landed_unseen() {
        let (up, down) = ([0.3; 4], [0.0; 4]);
        let mut planted = None;
        // Recién visto, con los vasos abajo: se siembra sin pisar.
        assert_eq!(hoof_step(&mut planted, Some(down)), [false; 4]);
        hoof_step(&mut planted, Some(up));
        // Se aleja con los vasos en el aire y vuelve con ellos abajo.
        assert_eq!(hoof_step(&mut planted, None), [false; 4]);
        assert_eq!(hoof_step(&mut planted, Some(down)), [false; 4]);
        // Ya a la vista, la próxima pisada cuenta.
        hoof_step(&mut planted, Some(up));
        assert_eq!(hoof_step(&mut planted, Some(down)), [true; 4]);
    }

    #[test]
    fn a_walker_steps_at_each_half_cycle_and_across_the_wrap() {
        assert!(stepped(0.45, 0.55));
        assert!(stepped(0.95, 0.05));
        assert!(stepped(0.05, 0.95), "walking backwards");
        assert!(!stepped(0.1, 0.4));
        assert!(!stepped(0.6, 0.9));
        assert!(!stepped(0.4, 0.3), "a short step backwards");
    }

    #[test]
    fn a_wheel_kicks_every_spacing_and_not_on_a_teleport() {
        assert_eq!(rolled_kick(0.0, 0.5), (0.0, false));
        assert_eq!(rolled_kick(0.0, WHEEL_SPACING), (WHEEL_SPACING, true));
        assert_eq!(rolled_kick(0.0, -WHEEL_SPACING), (-WHEEL_SPACING, true));
        assert_eq!(rolled_kick(0.0, 100.0), (100.0, false));
    }
}
