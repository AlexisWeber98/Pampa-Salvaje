//! El primer encargo: Don Hidalgo, en la plaza del pueblo donde el jugador baja
//! del tren, lo manda a presentarse ante el comisario Elías Farach en la
//! comisaría de otro pueblo, y le presta el caballo atado en el palenque
//! principal del pueblo ([`crate::town::hitched_store`]: el almacén, o la
//! pulpería donde no hay almacén) para el viaje. En ese pueblo, Bairoletto espera sentado en
//! la pulpería, y a una cuadra de ella el Viejo Amadeo vende quesos en su
//! puesto de la vereda ([`Stall`]).
//!
//! Reglas puras: dónde está cada personaje, los pasos de la historia en orden
//! ([`Step`], escritos como datos por el guion) y qué interacción está al
//! alcance. Hay un solo paso pendiente a la vez, y cada paso se cumple una sola
//! vez: solo su objetivo lo hace avanzar. El Viejo Amadeo no es un paso: lo
//! encuentra el jugador una sola vez, la primera vez que pasa por su puesto, en
//! cualquier momento del juego ([`Errand::hail`]).

use crate::pulperia::Door;
use crate::pulperia::FurnitureKind;
use crate::story::{Beat, Cue, Goal, Step};
use crate::{Aabb, Heading, Side, TetheredHorse, TownLayout, Travel, Vec2};

/// Quién participa en el encargo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Npc {
    /// Don Hidalgo, parado en la plaza: da el encargo.
    Hidalgo,
    /// El comisario Elías Farach, en la puerta de su comisaría.
    Farach,
    /// Juan Bautista Bairoletto, "el Tano": sentado a una mesa de la pulpería
    /// del pueblo de Farach, golpeado, tomando algo.
    Bairoletto,
    /// El Viejo Amadeo, un puestero que vende quesos en la vereda, a una cuadra
    /// de esa pulpería: llama al jugador la primera vez que pasa cerca de su
    /// puesto, fuera de la historia.
    Amadeo,
}

impl Npc {
    /// El nombre con que lo rotula un subtítulo.
    pub fn name(self) -> &'static str {
        match self {
            Self::Hidalgo => "Don Hidalgo",
            Self::Farach => "Farach",
            Self::Bairoletto => "Bairoletto",
            Self::Amadeo => "Viejo Amadeo",
        }
    }
}

/// El nombre del protagonista, con que lo rotula un subtítulo.
pub const PROTAGONIST: &str = "Lisandro";

/// Dónde está un personaje del encargo, de forma permanente.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Standing {
    /// Índice (dentro de los pueblos del mundo) del pueblo en el que está.
    pub town: usize,
    pub position: Vec2,
    pub heading: Heading,
}

impl Standing {
    /// La huella de colisión de una persona parada aquí.
    pub fn footprint(&self) -> Aabb {
        let side = Errand::PERSON_FOOTPRINT;
        Aabb::from_center_size(self.position, Vec2::new(side, side))
    }
}

/// El puesto de quesos del Viejo Amadeo: una mesa de caballetes sobre la
/// mitad de la vereda del lado del cordón, con él parado detrás, del lado por
/// el que pasa la gente, mirando hacia la calle por encima de la mesa.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stall {
    /// Índice del pueblo en el que está.
    pub town: usize,
    /// El centro de la mesa.
    pub position: Vec2,
    /// Hacia dónde mira el puestero, a través de la mesa: el eje corto de la
    /// mesa, siempre a lo largo de un eje del mundo.
    pub facing: Heading,
}

impl Stall {
    /// Largo de la mesa, a lo largo de la vereda, en metros.
    pub const LENGTH: f32 = 1.8;
    /// Ancho de la mesa, a lo ancho de la vereda, en metros.
    pub const DEPTH: f32 = 0.8;
    /// Alto de la tabla de la mesa sobre el suelo, en metros.
    pub const HEIGHT: f32 = 0.8;

    /// La huella de colisión de la mesa.
    pub fn footprint(&self) -> Aabb {
        let f = self.facing.forward();
        let size = if f.x.abs() > f.y.abs() {
            Vec2::new(Self::DEPTH, Self::LENGTH)
        } else {
            Vec2::new(Self::LENGTH, Self::DEPTH)
        };
        Aabb::from_center_size(self.position, size)
    }
}

/// Qué pide el paso actual de la historia (ver [`Goal`]), o que la historia
/// terminó.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrandStage {
    /// Hablar con alguien.
    Talk(Npc),
    /// Montar el caballo de Don Hidalgo.
    TakeHorse,
    /// Volver de noche, a caballo, a la puerta de la pulpería donde espera
    /// Bairoletto (acto 6).
    Rendezvous,
    /// Perder a la milicada en el campo con Bairoletto en ancas (acto 6).
    Hideout,
    /// No queda ningún paso: el gancho para los actos siguientes, que se
    /// agregan como pasos en el archivo de la historia.
    Done,
}

#[allow(non_upper_case_globals)]
impl ErrandStage {
    /// El primer paso del capítulo 1: buscar a Don Hidalgo en la plaza.
    pub const MeetHidalgo: Self = Self::Talk(Npc::Hidalgo);
}

impl From<Option<Goal>> for ErrandStage {
    fn from(goal: Option<Goal>) -> Self {
        match goal {
            Some(Goal::Talk(npc)) => Self::Talk(npc),
            Some(Goal::TakeHorse) => Self::TakeHorse,
            Some(Goal::Rendezvous) => Self::Rendezvous,
            Some(Goal::Hideout) => Self::Hideout,
            None => Self::Done,
        }
    }
}

/// Dónde está alguien del encargo, y si cayó al piso (ver [`Errand::farach`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Whereabouts {
    pub at: Standing,
    pub fallen: bool,
}

/// Algo que hizo avanzar el encargo: se cumplió el objetivo de un paso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrandEvent {
    /// Habló con alguien.
    Talked(Npc),
    /// Montó el caballo de Don Hidalgo: desde entonces es del jugador.
    MountedHorse,
    /// Llegó de noche, a caballo, a la puerta donde lo espera Bairoletto.
    Rendezvous,
    /// Perdió a la milicada en el campo.
    ShookOff,
}

/// Lo que hizo apretar la tecla de interacción.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Interaction {
    /// El encargo avanzó un paso (su escena está en [`Errand::last_scene`]).
    Event(ErrandEvent),
    /// Junto al caballo de Don Hidalgo antes de que lo preste: no le
    /// corresponde al jugador llevárselo.
    NotYourHorse,
    /// No hay nada al alcance con lo que interactuar.
    Nothing,
}

/// La última escena que se jugó: la de un paso de la historia, o el encuentro
/// con el Viejo Amadeo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Played {
    Step(usize),
    Amadeo,
}

/// El encargo: quién está dónde, el caballo y los pasos de la historia.
#[derive(Debug, Clone, PartialEq)]
pub struct Errand {
    steps: Vec<Step>,
    /// Qué pasos ya se cumplieron, uno por paso (siempre los primeros).
    done: Vec<bool>,
    /// La última escena jugada.
    last: Option<Played>,
    /// La escena del encuentro con el Viejo Amadeo (vacía: no hay encuentro),
    /// y si ya sucedió.
    amadeo_scene: Vec<Beat>,
    met_amadeo: bool,
    hidalgo: Standing,
    farach: Standing,
    bairoletto: Standing,
    /// El asiento de Bairoletto en la pulpería del pueblo de Farach
    /// ([`crate::pulperia::Interior::seats`]); `None` si ese pueblo no tiene
    /// pulpería y espera parado junto a Farach.
    bairoletto_seat: Option<usize>,
    /// El piso de esa pulpería: se le habla a Bairoletto desde adentro.
    bairoletto_room: Option<Aabb>,
    /// La puerta de esa pulpería, por la que entra Lisandro
    /// ([`Self::walk_in`]), y el lugar junto a la cabecera de la mesa de
    /// Bairoletto desde el que hablan ([`Self::place_to_talk`]).
    bairoletto_entry: Option<(Door, Vec2)>,
    /// Dónde está Bairoletto cuando el juego lo movió ([`Self::place`]): en
    /// la vereda de [`Self::porch`] de noche, junto al caballo donde perdieron
    /// a la milicada. Manda sobre su asiento.
    bairoletto_moved: Option<Standing>,
    /// Dónde está Farach la noche del acto 6: en el mostrador de esa
    /// pulpería, del lado de los clientes
    /// ([`crate::pulperia::Interior::at_counter`]); `None` sin pulpería.
    farach_counter: Option<Standing>,
    /// Si ya se oyó el disparo que lo deja tendido ([`Self::shoot_farach`]).
    farach_shot: bool,
    /// Pueblo del caballo, y el caballo.
    horse: (usize, TetheredHorse),
    /// Nombre del pueblo en el que está Farach.
    police_name: String,
    /// El puesto del Viejo Amadeo, y él detrás.
    stall: Stall,
    amadeo: Standing,
}

impl Errand {
    /// Distancia máxima a la que puede estar el jugador de Don Hidalgo para
    /// hablarle, en metros.
    pub const TALK_RANGE: f32 = 3.0;
    /// Distancia máxima a la que puede estar el jugador del caballo para
    /// montarlo, en metros.
    pub const MOUNT_RANGE: f32 = 3.0;
    /// Distancia máxima a la que puede estar el jugador (a pie o montado) de
    /// Farach para presentarse, en metros.
    pub const REPORT_RANGE: f32 = 4.0;
    /// Lado de la huella de colisión cuadrada de una persona parada, en metros.
    pub const PERSON_FOOTPRINT: f32 = 0.8;
    /// A qué distancia a la izquierda de Farach espera Bairoletto cuando el
    /// pueblo de Farach no tiene pulpería, en metros.
    pub const BAIROLETTO_GAP: f32 = 1.1;
    /// Distancia máxima a la que puede estar el jugador de Bairoletto, sentado,
    /// para hablarle, en metros: alcanza desde el paso que viene de la puerta,
    /// junto a su banco. Además hay que estar adentro, para no hablarle a
    /// través de la pared.
    pub const SEATED_TALK_RANGE: f32 = 4.5;
    /// Distancia máxima a la que el Viejo Amadeo llama al jugador que pasa (a
    /// pie o montado) junto a su puesto, en metros: alcanza a un jinete que
    /// pasa por la mitad de la calle (Amadeo está a 1,5 m del cordón y la calle
    /// tiene 12 m), que después frena y vuelve al tranco ([`Self::hail`]).
    pub const HAIL_RANGE: f32 = 8.0;
    /// Cuánto entra Lisandro, desde la puerta de La Colonia, antes de
    /// detenerse a mirar el lugar, en metros (la puerta está en la cara de
    /// afuera de la pared, de 0,45 m).
    pub const ENTRY_DEPTH: f32 = 2.0;
    /// Cuánto se aparta Lisandro de la cabecera de la mesa de Bairoletto para
    /// hablarle parado, en metros: Bairoletto lo ve girando la cabeza menos de
    /// [`Self::GAZE_MAX`].
    pub const BESIDE_TABLE: f32 = 0.7;
    /// Lo más que gira la cabeza alguien sentado para mirar a quien le habla,
    /// en radianes (unos 70°): más sería forzado, así que quien está parado se
    /// pone donde se lo vea.
    pub const GAZE_MAX: f32 = 1.22;
    /// A qué distancia al costado del Viejo Amadeo, sobre la vereda, se para
    /// Lisandro a pie para hablarle, en metros.
    pub const BESIDE_VENDOR: f32 = 1.2;
    /// A qué distancia del cordón, en la calle, se para el caballo de Lisandro
    /// para hablarle a Amadeo por encima de la mesa, en metros: la cabeza del
    /// caballo no llega al puesto.
    pub const FROM_CURB: f32 = 2.0;
    /// Hasta qué distancia del punto de la vereda frente a la puerta de la
    /// pulpería de Bairoletto (a [`Self::PORCH`] de ella) entra Lisandro solo,
    /// en metros: llegando a pie por la vereda.
    pub const DOOR_RANGE: f32 = 2.0;
    /// A qué distancia de la puerta, hacia la calle, empieza Lisandro a entrar
    /// solo, en metros.
    pub const PORCH: f32 = 1.0;
    /// A qué distancia de la puerta de la pulpería (o de la comisaría, sin
    /// pulpería) está el puesto del Viejo Amadeo, en metros: una cuadra de
    /// Castex, una manzana de 40 m y una calle de 12 m.
    pub const STALL_DISTANCE: f32 = 52.0;
    /// Espacio libre mínimo entre el puesto (o su puestero) y cualquier
    /// obstáculo del pueblo, en metros.
    pub const STALL_CLEARANCE: f32 = 1.0;
    /// Desde qué hora espera Bairoletto en la puerta de la pulpería, de noche
    /// (acto 6), hasta que amanece ([`crate::pulperia::OPENS_AT`]).
    pub const NIGHT_FROM: f32 = 20.0;
    /// A qué distancia de la puerta, sobre la vereda, espera Bairoletto
    /// parado, en metros.
    pub const DOORSTEP: f32 = 0.8;
    /// Hasta qué distancia de Bairoletto, en la puerta, llega el jugador a
    /// caballo para que empiece el acto 6, en metros: el caballo frena solo.
    pub const MEET_RANGE: f32 = 7.0;

    /// El encargo desde el pueblo `giver` (Don Hidalgo en su plaza, su caballo
    /// en su almacén de ramos generales) hasta el pueblo `police` (Farach en la
    /// puerta de su primera comisaría, Bairoletto sentado en su pulpería, en
    /// [`crate::pulperia::Interior::front_seat`], y el Viejo Amadeo en su puesto,
    /// a una cuadra de la pulpería: ver [`Self::stall`]). `None` si falta alguno
    /// de los pueblos, si son el mismo pueblo o si `police` no tiene comisaría
    /// (o veredas).
    ///
    /// Empieza sin pasos (terminado); [`Self::with_story`] le da la historia.
    pub fn new(towns: &[TownLayout], giver: usize, police: usize) -> Option<Self> {
        if giver == police {
            return None;
        }
        let (home, away) = (towns.get(giver)?, towns.get(police)?);
        let (door, facing) = away.police_door()?;
        let (spot, heading) = home.plaza_spot();
        let farach = Standing {
            town: police,
            position: door,
            heading: facing,
        };
        let from = away
            .pulperia()
            .map_or(door, |(_, inside)| inside.door.center);
        let (stall, amadeo) = cheese_stall(away, police, from)?;
        let farach_counter = away
            .pulperia()
            .and_then(|(_, inside)| inside.at_counter())
            .map(|(position, heading)| Standing {
                town: police,
                position,
                heading,
            });
        let (bairoletto, bairoletto_seat, bairoletto_room, bairoletto_entry) = match away.pulperia()
        {
            Some((_, inside)) => {
                let seat = inside.front_seat();
                let at = inside.seats[seat];
                let standing = Standing {
                    town: police,
                    position: at.position,
                    heading: at.facing,
                };
                (
                    standing,
                    Some(seat),
                    Some(inside.floor),
                    Some((inside.door, beside_table(&inside, seat))),
                )
            }
            None => {
                let forward = facing.forward();
                let left = Vec2::new(-forward.y, forward.x);
                let standing = Standing {
                    position: door + left * Self::BAIROLETTO_GAP,
                    ..farach
                };
                (standing, None, None, None)
            }
        };
        Some(Self {
            steps: Vec::new(),
            done: Vec::new(),
            last: None,
            amadeo_scene: Vec::new(),
            met_amadeo: false,
            hidalgo: Standing {
                town: giver,
                position: spot,
                heading,
            },
            farach,
            bairoletto,
            bairoletto_seat,
            bairoletto_room,
            bairoletto_entry,
            bairoletto_moved: None,
            farach_counter,
            farach_shot: false,
            horse: (giver, home.hitching_post.horse),
            police_name: away.name.clone(),
            stall,
            amadeo,
        })
    }

    /// El encargo con los pasos `steps`, desde el primero.
    pub fn with_story(self, steps: Vec<Step>) -> Self {
        Self {
            done: vec![false; steps.len()],
            last: None,
            steps,
            ..self
        }
    }

    /// El encargo con `scene` como el encuentro con el Viejo Amadeo, todavía
    /// sin suceder ([`Self::hail`]). Sin escena, no hay encuentro.
    pub fn with_amadeo(self, scene: Vec<Beat>) -> Self {
        Self {
            amadeo_scene: scene,
            met_amadeo: false,
            ..self
        }
    }

    /// Pone a Bairoletto en `at`: en la vereda de [`Self::porch`] cuando
    /// espera de noche, junto al caballo al bajarse de las ancas en el campo.
    /// Hasta entonces está en su asiento (también durante la escena del acto
    /// 4, aunque el paso pendiente ya sea el encuentro de la noche).
    pub fn place(&mut self, at: Standing) {
        self.bairoletto_moved = Some(at);
    }

    /// Se oyó el disparo de la escena del acto 6: Farach cae tendido donde
    /// estaba, en el mostrador de La Colonia ([`Self::farach`]).
    pub fn shoot_farach(&mut self) {
        self.farach_shot = true;
    }

    /// Si en la escena de la puerta del acto 6, `scene`, jugada hasta antes
    /// del momento `played`, ya se oyó el disparo: su primer subtítulo
    /// ([`Cue::Caption`]), después de que Bairoletto entra a la pulpería.
    pub fn shot_heard(scene: &[Beat], played: usize) -> bool {
        scene
            .iter()
            .position(|b| matches!(b, Beat::Cue(Cue::Caption(_))))
            .is_some_and(|i| i < played)
    }

    /// Dónde está Farach a la hora `time` (del día, en horas), y si está
    /// tendido. Parado en la puerta de su comisaría ([`Self::standing`]),
    /// salvo en el acto 6: de noche mientras Bairoletto espera
    /// ([`Self::waits_at_door`]) y durante la escena de la puerta está parado
    /// en el mostrador de La Colonia; desde el disparo
    /// ([`Self::shoot_farach`]) está tendido ahí, toda la huida. Terminada la
    /// historia después del disparo ya no está en ninguna parte: `None`.
    pub fn farach(&self, time: f32) -> Option<Whereabouts> {
        let inside = self.waits_at_door(time) || self.stage() == ErrandStage::Hideout;
        match (self.farach_counter, self.farach_shot) {
            (Some(_), true) if self.stage() == ErrandStage::Done => None,
            (Some(at), true) => Some(Whereabouts { at, fallen: true }),
            (Some(at), false) if inside => Some(Whereabouts { at, fallen: false }),
            _ => Some(Whereabouts {
                at: self.farach,
                fallen: false,
            }),
        }
    }

    /// La puerta de la pulpería de Bairoletto; `None` si ese pueblo no tiene.
    pub fn pulperia_door(&self) -> Option<Door> {
        self.bairoletto_entry.map(|(door, _)| door)
    }

    /// Dónde están atados, de noche, los caballos de la milicada que está
    /// adentro de La Colonia en el acto 6: hasta `count` lugares frente a la
    /// pulpería, a [`Self::TIE_OUT`] de la fachada y a los costados de la
    /// puerta donde haya lugar (sin tapar la entrada), mirando hacia ella y
    /// libre de los obstáculos de `world`; los más cercanos a la puerta
    /// primero. Vacío si ese pueblo no tiene pulpería.
    pub fn milicada_horses(&self, world: &crate::World, count: usize) -> Vec<(Vec2, Heading)> {
        let Some(door) = self.pulperia_door() else {
            return Vec::new();
        };
        let along = Vec2::new(-door.outward.y, door.outward.x);
        let facing = Heading::from_radians((-door.outward.y).atan2(-door.outward.x));
        let half = Vec2::new(Self::TIE_ROOM, Self::TIE_ROOM) * 0.5;
        (0..16)
            .flat_map(|k| {
                let d = Self::TIE_CLEAR + Self::TIE_SPACING * k as f32;
                [d, -d]
            })
            .map(|lateral| door.center + door.outward * Self::TIE_OUT + along * lateral)
            .filter(|&at| {
                let room = Aabb::new(at - half, at + half);
                world.obstacles.near(room).is_empty()
            })
            .take(count)
            .map(|at| (at, facing))
            .collect()
    }

    /// Dónde espera Bairoletto de noche en el acto 6: parado en la vereda, a
    /// [`Self::DOORSTEP`] de la puerta de su pulpería, mirando a la calle.
    /// `None` si ese pueblo no tiene pulpería.
    pub fn porch(&self) -> Option<Standing> {
        let door = self.pulperia_door()?;
        Some(Standing {
            town: self.bairoletto.town,
            position: door.center + door.outward * Self::DOORSTEP,
            heading: Heading::from_radians(door.outward.y.atan2(door.outward.x)),
        })
    }

    /// Si a la hora `time` (del día, en horas) es de noche para el acto 6:
    /// desde [`Self::NIGHT_FROM`] hasta que amanece.
    pub fn is_night(time: f32) -> bool {
        time >= Self::NIGHT_FROM || time < crate::pulperia::OPENS_AT
    }

    /// A cuántos metros de la fachada de La Colonia se atan los caballos de
    /// la milicada.
    pub const TIE_OUT: f32 = 3.5;
    /// Cuánto se apartan de la puerta, a cada lado, como mínimo, en metros:
    /// la entrada queda libre.
    pub const TIE_CLEAR: f32 = 2.5;
    /// La separación entre dos caballos atados, en metros.
    pub const TIE_SPACING: f32 = 1.8;
    /// El lado del cuadrado que ocupa un caballo atado, en metros.
    pub const TIE_ROOM: f32 = 2.6;

    /// Si Bairoletto espera en la puerta a la hora `time`: el paso pendiente
    /// es el encuentro del acto 6, es de noche y su pueblo tiene pulpería.
    pub fn waits_at_door(&self, time: f32) -> bool {
        self.stage() == ErrandStage::Rendezvous && Self::is_night(time) && self.porch().is_some()
    }

    /// El jugador llega a `at` a la hora `time`, a pie o montado (`travel`):
    /// si Bairoletto espera en la puerta ([`Self::waits_at_door`]) y el
    /// jugador llega a caballo a menos de [`Self::MEET_RANGE`] de él, se cumple
    /// el encuentro y su escena se juega. A pie no: necesita el caballo.
    pub fn rendezvous(&mut self, at: Vec2, travel: Travel, time: f32) -> Option<ErrandEvent> {
        let porch = self.porch()?;
        let near = self.waits_at_door(time)
            && travel == Travel::Riding
            && at.distance(porch.position) <= Self::MEET_RANGE;
        self.advance(Goal::Rendezvous, near, ErrandEvent::Rendezvous)
    }

    /// Si Bairoletto, esperando en la puerta ([`Self::waits_at_door`]), le
    /// pide el caballo al jugador que llega a pie a menos de
    /// [`Self::MEET_RANGE`] de él, a `at` y a la hora `time`.
    pub fn asks_for_horse(&self, at: Vec2, travel: Travel, time: f32) -> bool {
        self.stage() == ErrandStage::Rendezvous
            && travel == Travel::OnFoot
            && self.waits_at_door(time)
            && self
                .porch()
                .is_some_and(|p| at.distance(p.position) <= Self::MEET_RANGE)
    }

    /// El jugador está en `at`: si el paso pendiente es la huida, la milicada
    /// ya lo perdió (`lost`) y él está en el campo, en cualquier parte fuera
    /// del pueblo de Farach (`town`, su extensión), se cumple y se juega su
    /// escena. No hay un lugar fijo adonde llegar.
    pub fn shake_off(&mut self, at: Vec2, town: Aabb, lost: bool) -> Option<ErrandEvent> {
        let free = lost && !town.contains_point(at);
        self.advance(Goal::Hideout, free, ErrandEvent::ShookOff)
    }

    /// El pueblo al que un encargo desde el pueblo `giver` manda al jugador: el
    /// pueblo llamado `name` cuando tiene comisaría o, si no, el otro pueblo
    /// más cercano (según el centro de la cuadrícula) que tenga una.
    pub fn police_town(towns: &[TownLayout], giver: usize, name: &str) -> Option<usize> {
        let from = towns.get(giver)?.grid().center();
        let candidates = || {
            towns
                .iter()
                .enumerate()
                .filter(move |&(i, t)| i != giver && t.police_door().is_some())
        };
        candidates()
            .find(|(_, t)| t.name == name)
            .or_else(|| {
                candidates().min_by(|(_, a), (_, b)| {
                    let (da, db) = (
                        a.grid().center().distance(from),
                        b.grid().center().distance(from),
                    );
                    da.total_cmp(&db)
                })
            })
            .map(|(i, _)| i)
    }

    /// Qué pide el paso pendiente.
    pub fn stage(&self) -> ErrandStage {
        self.step().map(|s| s.goal).into()
    }

    /// El paso pendiente; `None` una vez terminada la historia.
    pub fn step(&self) -> Option<&Step> {
        self.pending().map(|i| &self.steps[i])
    }

    /// El índice del paso pendiente: el primero sin cumplir.
    fn pending(&self) -> Option<usize> {
        self.done.iter().position(|d| !d)
    }

    /// La escena jugada por última vez: la del último paso cumplido o la del
    /// encuentro con el Viejo Amadeo (vacía antes de la primera).
    pub fn last_scene(&self) -> &[Beat] {
        match self.last {
            Some(Played::Step(i)) => self.steps.get(i).map_or(&[], |s| &s.scene),
            Some(Played::Amadeo) => &self.amadeo_scene,
            None => &[],
        }
    }

    /// Nombre del pueblo al que el encargo manda al jugador, el de Farach.
    pub fn police_name(&self) -> &str {
        &self.police_name
    }

    /// El objetivo del paso pendiente, con el pueblo de la comisaría
    /// en lugar de `{town}`; `None` una vez terminada la historia o mientras
    /// ese paso es sin aviso.
    pub fn objective(&self) -> Option<String> {
        self.step()?.objective_in(&self.police_name)
    }

    /// Dónde está `npc`.
    pub fn standing(&self, npc: Npc) -> Standing {
        match npc {
            Npc::Hidalgo => self.hidalgo,
            Npc::Farach => self.farach,
            Npc::Bairoletto => self.bairoletto_moved.unwrap_or(self.bairoletto),
            Npc::Amadeo => self.amadeo,
        }
    }

    /// El puesto de quesos del Viejo Amadeo: en una vereda del pueblo de
    /// Farach, a media cuadra, sobre la mitad del lado del cordón, a unos
    /// [`Self::STALL_DISTANCE`] de la puerta de la pulpería (o de la comisaría,
    /// si ese pueblo no tiene pulpería), apartado de todo obstáculo del pueblo.
    pub fn stall(&self) -> Stall {
        self.stall
    }

    /// El asiento de Bairoletto en la pulpería del pueblo de Farach; `None` si
    /// ese pueblo no tiene pulpería y Bairoletto espera parado junto a Farach.
    pub fn bairoletto_seat(&self) -> Option<usize> {
        self.bairoletto_seat
    }

    /// Si ya se cumplió un paso que pedía hablar con `npc` (con el Viejo
    /// Amadeo: si ya sucedió su encuentro).
    pub fn talked_to(&self, npc: Npc) -> bool {
        if npc == Npc::Amadeo {
            return self.met_amadeo;
        }
        self.steps
            .iter()
            .zip(&self.done)
            .any(|(s, &done)| done && s.goal == Goal::Talk(npc))
    }

    /// El caballo que presta Don Hidalgo, y su pueblo.
    pub fn horse(&self) -> (usize, TetheredHorse) {
        self.horse
    }

    /// Adónde manda al jugador el paso pendiente: a quien tiene que
    /// hablarle o al caballo; `None` una vez terminada la historia o mientras
    /// ese paso es sin aviso.
    pub fn target(&self) -> Option<Vec2> {
        let step = self.step()?;
        step.objective.as_ref()?;
        match step.goal {
            Goal::Talk(npc) => Some(self.standing(npc).position),
            Goal::TakeHorse => Some(self.horse.1.position),
            Goal::Rendezvous => self.porch().map(|p| p.position),
            // En cualquier parte del campo: no hay adónde mandar.
            Goal::Hideout => None,
        }
    }

    /// Huellas de colisión de las personas que están paradas en el encargo
    /// (Hidalgo, Farach y el Viejo Amadeo) y del puesto de quesos. Bairoletto
    /// no tiene: sentado, su banco ya es un obstáculo, y después se va.
    pub fn obstacles(&self) -> [Aabb; 4] {
        [
            self.hidalgo.footprint(),
            self.farach.footprint(),
            self.amadeo.footprint(),
            self.stall.footprint(),
        ]
    }

    /// Hasta dónde se le puede hablar a `npc`, en metros: a Farach también
    /// desde la montura.
    pub fn talk_range(npc: Npc) -> f32 {
        match npc {
            Npc::Hidalgo => Self::TALK_RANGE,
            Npc::Farach => Self::REPORT_RANGE,
            Npc::Bairoletto => Self::SEATED_TALK_RANGE,
            Npc::Amadeo => Self::HAIL_RANGE,
        }
    }

    /// El jugador pasa por `at`, a pie o montado (`travel`): la primera vez
    /// que está a menos de [`Self::HAIL_RANGE`] del Viejo Amadeo, en cualquier
    /// momento del juego (sea cual sea el paso pendiente), Amadeo lo llama, y
    /// el camino por el que Lisandro se le acerca: hasta su lugar para hablar
    /// ([`Self::place_to_talk`]). Al llegar, [`Self::talk`] juega el
    /// encuentro. Nadie más llama al jugador, y Amadeo una sola vez. Durante
    /// la huida del acto 6 no llama: el encuentro queda para después.
    pub fn hail(&self, at: Vec2, travel: Travel) -> Option<Vec<Vec2>> {
        let him = self.amadeo.position;
        let escaping = self.stage() == ErrandStage::Hideout;
        let due = !self.met_amadeo && !self.amadeo_scene.is_empty() && !escaping;
        (due && at.distance(him) <= Self::HAIL_RANGE)
            .then(|| self.place_to_talk(Npc::Amadeo, at, travel))
            .flatten()
            .map(|spot| vec![spot])
    }

    /// Dónde se para Lisandro, viniendo desde `from` a pie o montado
    /// (`travel`), para hablar con `npc` de frente y con naturalidad, cuando
    /// importa: junto a la cabecera de la mesa de Bairoletto sentado, del lado
    /// del paso que viene de la puerta (él lo ve girando la cabeza menos de
    /// [`Self::GAZE_MAX`]); junto al Viejo Amadeo, sobre la vereda, del lado
    /// por el que llega, a pie; o en la calle frente a su puesto, a
    /// [`Self::FROM_CURB`] del cordón, montado. `None` para quien está parado
    /// en otro lado (se le habla desde donde se esté: se da vuelta él).
    pub fn place_to_talk(&self, npc: Npc, from: Vec2, travel: Travel) -> Option<Vec2> {
        match npc {
            Npc::Bairoletto => self.bairoletto_entry.map(|(_, beside)| beside),
            Npc::Amadeo => {
                let street = self.stall.facing.forward();
                Some(match travel {
                    Travel::Riding => {
                        self.stall.position + street * (TABLE_FROM_CURB + Self::FROM_CURB)
                    }
                    Travel::OnFoot => {
                        let along = Vec2::new(-street.y, street.x);
                        let him = self.amadeo.position;
                        let side = if (from - him).dot(along) < 0.0 {
                            -1.0
                        } else {
                            1.0
                        };
                        him + along * (side * Self::BESIDE_VENDOR)
                    }
                })
            }
            Npc::Hidalgo | Npc::Farach => None,
        }
    }

    /// El jugador llega a `at`: si el paso pendiente pide hablar con
    /// Bairoletto, sentado en su pulpería, y `at` está en la vereda frente a
    /// la puerta (a menos de [`Self::DOOR_RANGE`] del punto a [`Self::PORCH`]
    /// de ella), el camino por el que Lisandro entra solo, tímido: ese punto
    /// de la vereda, y después derecho por la puerta hasta
    /// [`Self::ENTRY_DEPTH`] adentro, por piso libre, donde se detiene a mirar
    /// el lugar. Desde ahí [`Self::talk`] lo alcanza (Bairoletto lo ve y lo
    /// llama), y después va hasta su mesa ([`Self::place_to_talk`]). Si la
    /// puerta está abierta y si va a pie lo decide quien llama.
    pub fn walk_in(&self, at: Vec2) -> Option<Vec<Vec2>> {
        let (door, _) = self.bairoletto_entry?;
        let porch = door.center + door.outward * Self::PORCH;
        let inside = door.center - door.outward * Self::ENTRY_DEPTH;
        let near = at.distance(porch) <= Self::DOOR_RANGE;
        let due = self.stage() == ErrandStage::Talk(Npc::Bairoletto);
        (near && due).then(|| vec![porch, inside])
    }

    /// Habla con `npc` desde `at`: solo si está al alcance (a Bairoletto
    /// sentado, desde adentro de la pulpería), y solo si el paso pendiente lo
    /// pide; o, con el Viejo Amadeo, si su encuentro todavía no sucedió.
    pub fn talk(&mut self, npc: Npc, at: Vec2) -> Option<ErrandEvent> {
        let inside = match (npc, self.bairoletto_room) {
            (Npc::Bairoletto, Some(room)) => room.contains_point(at),
            _ => true,
        };
        let near = inside && at.distance(self.standing(npc).position) <= Self::talk_range(npc);
        if npc == Npc::Amadeo {
            let due = !self.met_amadeo && !self.amadeo_scene.is_empty();
            if !(due && near) {
                return None;
            }
            self.met_amadeo = true;
            self.last = Some(Played::Amadeo);
            return Some(ErrandEvent::Talked(npc));
        }
        self.advance(Goal::Talk(npc), near, ErrandEvent::Talked(npc))
    }

    /// Monta el caballo desde `at`: solo si está al alcance, y solo si un paso
    /// pendiente lo pide (Don Hidalgo ya lo prestó).
    pub fn mount(&mut self, at: Vec2) -> Option<ErrandEvent> {
        let near = self.near_horse(at);
        self.advance(Goal::TakeHorse, near, ErrandEvent::MountedHorse)
    }

    /// Aprieta la tecla de interacción en `at`: habla o monta, lo que pida el
    /// paso pendiente y esté al alcance. Al Viejo Amadeo no: él llama antes
    /// ([`Self::hail`]).
    pub fn interact(&mut self, at: Vec2) -> Interaction {
        let event = self.step().map(|s| s.goal).and_then(|goal| match goal {
            Goal::Talk(npc) => self.talk(npc, at),
            Goal::TakeHorse => self.mount(at),
            // Empiezan solos, al llegar ([`Self::rendezvous`],
            // [`Self::shake_off`]).
            Goal::Rendezvous | Goal::Hideout => None,
        });
        match event {
            Some(event) => Interaction::Event(event),
            None if self.horse_still_lent_later() && self.near_horse(at) => {
                Interaction::NotYourHorse
            }
            None => Interaction::Nothing,
        }
    }

    /// Si algún paso que todavía no llegó (no el pendiente) pide tomar el
    /// caballo: hasta entonces no es del jugador.
    fn horse_still_lent_later(&self) -> bool {
        let pending = self.pending();
        (0..self.steps.len())
            .any(|i| !self.done[i] && pending != Some(i) && self.steps[i].goal == Goal::TakeHorse)
    }

    fn near_horse(&self, at: Vec2) -> bool {
        at.distance(self.horse.1.position) <= Self::MOUNT_RANGE
    }

    /// Cumple el paso pendiente si pide `due` y `near`, informando `event`;
    /// si no, no pasa nada, así que cada paso se cumple una sola vez.
    fn advance(&mut self, due: Goal, near: bool, event: ErrandEvent) -> Option<ErrandEvent> {
        if !near {
            return None;
        }
        let i = self.pending().filter(|&i| self.steps[i].goal == due)?;
        self.done[i] = true;
        self.last = Some(Played::Step(i));
        Some(event)
    }
}

/// A qué distancia del cordón está el centro de la mesa del puesto, en
/// metros: a veinte centímetros del cordón.
const TABLE_FROM_CURB: f32 = 0.2 + Stall::DEPTH / 2.0;

/// El lugar junto a la cabecera de la mesa del asiento `seat` de `inside`, del
/// lado del paso que viene de la puerta, a [`Errand::BESIDE_TABLE`] de ella y a
/// la profundidad de la mesa: desde ahí habla parado quien llega con quien está
/// sentado ahí.
fn beside_table(inside: &crate::pulperia::Interior, seat: usize) -> Vec2 {
    let door = inside.door;
    let along = Vec2::new(-door.outward.y, door.outward.x);
    let table = inside
        .furniture
        .iter()
        .filter(|f| f.kind == FurnitureKind::Table)
        .nth(inside.seats[seat].table)
        .map(|f| f.footprint)
        .expect("every seat has its table");
    let center = table.center();
    let size = table.size();
    let half = (size.x * along.x.abs() + size.y * along.y.abs()) / 2.0;
    let toward_aisle = if (center - door.center).dot(along) > 0.0 {
        -1.0
    } else {
        1.0
    };
    center + along * (toward_aisle * (half + Errand::BESIDE_TABLE))
}

/// El puesto del Viejo Amadeo en `town` (de índice `index`) y él detrás: el
/// lugar a media cuadra, sobre la mitad del cordón de una vereda, cuya
/// distancia a `from` está más cerca de [`Errand::STALL_DISTANCE`], entre los
/// que dejan a ambos a más de [`Errand::STALL_CLEARANCE`] de todo obstáculo del
/// pueblo (o entre todos, si ninguno lo hace). Determinista; `None` solo en un
/// pueblo sin veredas.
fn cheese_stall(town: &TownLayout, index: usize, from: Vec2) -> Option<(Stall, Standing)> {
    // Del cordón hacia adentro: la mesa a veinte centímetros, el puestero
    // después, del lado por el que pasa la gente.
    const TABLE: f32 = TABLE_FROM_CURB;
    const VENDOR: f32 = TABLE + Stall::DEPTH / 2.0 + 0.1 + Errand::PERSON_FOOTPRINT / 2.0;
    // Corrimientos a lo largo de la vereda desde la mitad de la cuadra.
    const SHIFTS: [f32; 5] = [0.0, -4.0, 4.0, -8.0, 8.0];
    let obstacles = town.obstacles();
    let candidates = town.sidewalks().iter().flat_map(|walk| {
        let out = match walk.curbs.first().copied().unwrap_or(Side::South) {
            Side::North => Vec2::new(0.0, 1.0),
            Side::South => Vec2::new(0.0, -1.0),
            Side::East => Vec2::new(1.0, 0.0),
            Side::West => Vec2::new(-1.0, 0.0),
        };
        let along = Vec2::new(out.y.abs(), out.x.abs());
        let (c, half) = (walk.area.center(), walk.area.size() * 0.5);
        let edge = c + out * (half.x * out.x.abs() + half.y * out.y.abs());
        SHIFTS.into_iter().map(move |t| {
            let vendor = edge - out * VENDOR + along * t;
            let facing = Heading::from_radians(out.y.atan2(out.x));
            let stall = Stall {
                town: index,
                position: edge - out * TABLE + along * t,
                facing,
            };
            let amadeo = Standing {
                town: index,
                position: vendor,
                heading: facing,
            };
            (stall, amadeo)
        })
    });
    let clear = |(stall, amadeo): &(Stall, Standing)| {
        let table = stall.footprint().expanded(Errand::STALL_CLEARANCE);
        let him = amadeo.footprint().expanded(Errand::STALL_CLEARANCE);
        obstacles
            .iter()
            .all(|o| !o.intersects(&table) && !o.intersects(&him))
    };
    let off = |(stall, _): &(Stall, Standing)| {
        (stall.position.distance(from) - Errand::STALL_DISTANCE).abs()
    };
    let best =
        |all: Vec<(Stall, Standing)>| all.into_iter().min_by(|a, b| off(a).total_cmp(&off(b)));
    let all: Vec<_> = candidates.collect();
    let fits: Vec<_> = all.iter().copied().filter(clear).collect();
    best(fits).or_else(|| best(all))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Terrain, TownConfig, World};

    /// Un pueblo que da el encargo y, lejos hacia el este, un pueblo con
    /// comisaría.
    fn towns() -> Vec<TownLayout> {
        let giver = TownLayout::generate(&TownConfig {
            name: "Monte Nievas".into(),
            seed: 7,
            blocks: (4, 3),
            ..TownConfig::castex()
        })
        .unwrap();
        let police = TownLayout::generate(&TownConfig {
            name: "Eduardo Castex".into(),
            origin: Vec2::new(-2500.0, -1000.0),
            seed: 2,
            ..TownConfig::castex()
        })
        .unwrap();
        vec![giver, police]
    }

    /// La historia del capítulo 1 en pocas palabras: hablar con Hidalgo (que
    /// dice algo y da plata), tomar su caballo y presentarse ante Farach.
    fn story() -> Vec<Step> {
        use crate::story::{Action, Speaker};
        let line = |npc, text: &str| Beat::Line {
            speaker: Speaker::Npc(npc),
            text: text.into(),
        };
        vec![
            Step {
                goal: Goal::Talk(Npc::Hidalgo),
                objective: Some("Buscá a Don Hidalgo".into()),
                scene: vec![
                    line(Npc::Hidalgo, "Hola"),
                    Beat::Act(Action::Earn(crate::Money::new(2, 0))),
                ],
            },
            Step {
                goal: Goal::TakeHorse,
                objective: Some("Tomá el caballo".into()),
                scene: vec![],
            },
            Step {
                goal: Goal::Talk(Npc::Farach),
                objective: Some("Presentate al comisario Farach en {town}".into()),
                scene: vec![line(Npc::Farach, "Buenas")],
            },
        ]
    }

    fn errand() -> (Vec<TownLayout>, Errand) {
        let towns = towns();
        let errand = Errand::new(&towns, 0, 1)
            .expect("a valid errand")
            .with_story(story());
        (towns, errand)
    }

    /// Un punto a `d` metros al este de `p`.
    fn east(p: Vec2, d: f32) -> Vec2 {
        p + Vec2::new(d, 0.0)
    }

    #[test]
    fn the_cast_stands_where_the_towns_say() {
        let (towns, e) = errand();
        let (at, heading) = towns[0].plaza_spot();
        assert_eq!(
            e.standing(Npc::Hidalgo),
            Standing {
                town: 0,
                position: at,
                heading
            }
        );
        let (door, facing) = towns[1].police_door().unwrap();
        assert_eq!(
            e.standing(Npc::Farach),
            Standing {
                town: 1,
                position: door,
                heading: facing
            }
        );
        assert_eq!(e.horse(), (0, towns[0].hitching_post.horse));
        assert_eq!(e.stage(), ErrandStage::MeetHidalgo);
        for (npc, o) in [Npc::Hidalgo, Npc::Farach].into_iter().zip(e.obstacles()) {
            assert!(o.contains_point(e.standing(npc).position));
            assert!((o.size().x - Errand::PERSON_FOOTPRINT).abs() < 1e-3);
        }
    }

    #[test]
    fn people_standing_in_the_errand_are_clear_of_their_towns() {
        let (towns, e) = errand();
        let people = [Npc::Hidalgo, Npc::Farach, Npc::Amadeo]
            .map(|npc| (format!("{npc:?}"), e.standing(npc)));
        for (npc, s) in people {
            let footprint = s.footprint();
            for o in towns[s.town].obstacles() {
                assert!(
                    o.closest_point(s.position).distance(s.position) > 1.0,
                    "{npc} too close to {o:?}"
                );
                assert!(!o.contains_point(footprint.center()));
            }
        }
    }

    #[test]
    fn bairoletto_waits_seated_at_a_front_table_of_the_police_towns_pulperia() {
        let (towns, e) = errand();
        let (_, inside) = towns[1].pulperia().unwrap();
        let seat = inside.front_seat();
        assert_eq!(e.bairoletto_seat(), Some(seat));
        assert_eq!(
            e.standing(Npc::Bairoletto),
            Standing {
                town: 1,
                position: inside.seats[seat].position,
                heading: inside.seats[seat].facing
            }
        );
        assert_eq!(Npc::Bairoletto.name(), "Bairoletto");
        // Sentado no estorba a nadie: el banco ya es un obstáculo.
        let him = e.standing(Npc::Bairoletto).footprint();
        assert!(e.obstacles().iter().all(|o| *o != him));
    }

    /// Si `stall` y el Viejo Amadeo están en una vereda de su pueblo, a media
    /// cuadra, a algo más de un metro de todo obstáculo, con la mesa sobre la
    /// mitad del cordón y él del lado de adentro, mirando hacia la calle;
    /// devuelve la distancia de la mesa a `from`.
    fn assert_stall_on_a_sidewalk(town: &TownLayout, e: &Errand, from: Vec2) -> f32 {
        let stall = e.stall();
        let him = e.standing(Npc::Amadeo);
        assert_eq!(stall.town, him.town);
        let table = stall.footprint();
        let walk = town
            .sidewalks()
            .iter()
            .find(|w| w.area.contains(&table))
            .expect("the stall stands on a sidewalk");
        assert!(
            walk.area.contains(&him.footprint()),
            "{him:?} off its sidewalk"
        );
        // A media cuadra: lejos de las esquinas (y de sus faroles).
        let size = walk.area.size();
        let (along, ends) = if size.x > size.y {
            (table.center().x, (walk.area.min.x, walk.area.max.x))
        } else {
            (table.center().y, (walk.area.min.y, walk.area.max.y))
        };
        assert!(
            along - ends.0 > 12.0 && ends.1 - along > 12.0,
            "{along} in {ends:?}"
        );
        // Del lado de la vereda por el que pasa la gente, mirando hacia la
        // calle, con la mesa entre él y el cordón, sobre la mitad del cordón.
        let street = crate::places::side_normal(walk.curbs[0]);
        assert!(
            him.heading.forward().dot(street) > 0.99,
            "{him:?} faces the street"
        );
        let toward = table.center() - him.position;
        assert!(
            toward.dot(street) > Stall::DEPTH / 2.0,
            "the table between him and the curb: {him:?} {table:?}"
        );
        assert_eq!(stall.facing, him.heading);
        let from_middle = |p: Vec2| (p - walk.area.center()).dot(street);
        assert!(from_middle(table.center()) > 0.0, "the curb half");
        for o in town.obstacles() {
            assert!(
                !o.intersects(&table.expanded(1.0)),
                "the stall too close to {o:?}"
            );
            assert!(
                o.closest_point(him.position).distance(him.position) > 1.0,
                "Amadeo too close to {o:?}"
            );
        }
        // Son obstáculos del encargo.
        assert!(e.obstacles().contains(&table));
        assert!(e.obstacles().contains(&him.footprint()));
        table.center().distance(from)
    }

    #[test]
    fn amadeos_cheese_stall_stands_on_a_sidewalk_a_block_from_la_colonia() {
        let (towns, e) = errand();
        let (_, inside) = towns[1].pulperia().unwrap();
        assert_eq!(e.stall().town, 1);
        let far = assert_stall_on_a_sidewalk(&towns[1], &e, inside.door.center);
        assert!((40.0..=64.0).contains(&far), "{far} m from the door");
        assert_eq!(Npc::Amadeo.name(), "Viejo Amadeo");
        // Un árbol justo ahí: el puesto se corre adonde no estorbe.
        let mut shaded = towns.clone();
        shaded[1].trees.push(e.stall().position);
        let moved = Errand::new(&shaded, 0, 1).unwrap();
        assert_ne!(moved.stall(), e.stall());
        assert_stall_on_a_sidewalk(&shaded[1], &moved, inside.door.center);
        // Sin pulpería, a una cuadra de la comisaría.
        let mut plain = towns.clone();
        plain[1]
            .buildings
            .retain(|b| b.kind != crate::BuildingKind::Pulperia);
        let e = Errand::new(&plain, 0, 1).unwrap();
        let (door, _) = plain[1].police_door().unwrap();
        let far = assert_stall_on_a_sidewalk(&plain[1], &e, door);
        assert!((40.0..=64.0).contains(&far), "{far} m from the comisaría");
    }

    /// El encuentro con el Viejo Amadeo, en pocas palabras: le da el queso.
    fn cheese() -> Vec<Beat> {
        vec![Beat::Act(crate::story::Action::Receive(
            crate::Item::Cheese,
        ))]
    }

    /// El ángulo, en radianes, entre hacia dónde mira alguien parado o
    /// sentado en `from` (`heading`) y `to`.
    fn turn_to(from: Vec2, heading: Heading, to: Vec2) -> f32 {
        let toward = to - from;
        let cos = heading.forward().dot(toward) / toward.length();
        cos.clamp(-1.0, 1.0).acos()
    }

    #[test]
    fn amadeo_hails_the_first_time_the_player_passes_at_any_point_and_never_again() {
        let (_, e) = errand();
        let him = e.standing(Npc::Amadeo).position;
        // Un jinete al galope por la mitad de la calle (Amadeo está a 1,5 m
        // del cordón; la calle tiene 12 m) pasa a su alcance.
        const { assert!(Errand::HAIL_RANGE >= 7.5) };
        let near = east(him, Errand::HAIL_RANGE - 0.2);
        let far = east(him, Errand::HAIL_RANGE + 0.2);
        // Sin escena no hay encuentro.
        assert_eq!(e.hail(near, Travel::OnFoot), None);
        // Antes de que empiece el encargo, aun sin ningún paso que lo pida.
        let mut e = e.with_amadeo(cheese());
        assert_eq!(e.hail(far, Travel::Riding), None);
        let way = e.hail(near, Travel::Riding).expect("he hails");
        assert!(
            !e.talked_to(Npc::Amadeo),
            "not yet: Lisandro comes up first"
        );
        let spot = *way.last().unwrap();
        assert_eq!(
            e.talk(Npc::Amadeo, spot),
            Some(ErrandEvent::Talked(Npc::Amadeo))
        );
        assert_eq!(e.last_scene(), &cheese()[..]);
        assert!(e.talked_to(Npc::Amadeo));
        // La historia no se movió, ni tiene objetivo ni destino de él.
        assert_eq!(e.stage(), ErrandStage::MeetHidalgo);
        assert_eq!(e.target(), Some(e.standing(Npc::Hidalgo).position));
        // Una sola vez, ni llamando ni con F.
        assert_eq!(e.hail(near, Travel::OnFoot), None);
        assert_eq!(e.talk(Npc::Amadeo, spot), None);
        assert_eq!(e.interact(spot), Interaction::Nothing);
        // La historia sigue como siempre, y termina sin él.
        let mut e = e.with_story(story());
        let hidalgo = e.standing(Npc::Hidalgo).position;
        assert_eq!(
            e.interact(hidalgo),
            Interaction::Event(ErrandEvent::Talked(Npc::Hidalgo))
        );
        // Ya avanzada la historia, también llama.
        let (_, later) = errand();
        let mut later = later.with_story(story()).with_amadeo(cheese());
        later.talk(Npc::Hidalgo, hidalgo).unwrap();
        assert!(later.hail(near, Travel::OnFoot).is_some());
    }

    #[test]
    fn lisandro_comes_up_to_amadeo_mounted_in_the_street_or_on_foot_beside_him() {
        let (towns, e) = errand();
        let e = e.with_amadeo(cheese());
        let him = e.standing(Npc::Amadeo);
        let street = him.heading.forward();
        let along = Vec2::new(-street.y, street.x);
        let mut obstacles = towns[him.town].obstacles();
        obstacles.extend(e.obstacles());
        for (travel, radius) in [
            (Travel::Riding, crate::Rider::COLLIDER_RADIUS),
            (Travel::OnFoot, crate::Rider::PERSON_RADIUS),
        ] {
            for side in [-1.0, 1.0] {
                let from = him.position + along * (side * 6.0) + street * 3.0;
                let way = e.hail(from, travel).expect("in range");
                let spot = *way.last().unwrap();
                let apart = spot.distance(him.position);
                assert!((1.0..=4.0).contains(&apart), "{travel:?}: {apart} m");
                assert!(apart <= Errand::talk_range(Npc::Amadeo));
                for o in &obstacles {
                    let gap = o.closest_point(spot).distance(spot);
                    assert!(gap >= radius, "{travel:?} {spot:?} by {o:?}");
                }
                let out = (spot - him.position).dot(street);
                match travel {
                    // En la calle, frente al puesto, la mesa entre los dos.
                    Travel::Riding => {
                        assert!(out > 2.5, "{out}");
                        assert!((spot - him.position).dot(along).abs() < 0.1);
                    }
                    // En la vereda, a su lado, del lado por el que llega.
                    Travel::OnFoot => {
                        assert!(out.abs() < 0.1, "{out}");
                        assert!((spot - him.position).dot(along) * side > 1.0);
                    }
                }
            }
        }
    }

    #[test]
    fn bairoletto_is_spoken_to_from_the_aisle_but_not_through_the_wall() {
        let (towns, e) = errand();
        let mut e = e.with_story(vec![Step {
            goal: Goal::Talk(Npc::Bairoletto),
            objective: Some("Andá a la pulpería".into()),
            scene: vec![],
        }]);
        let (_, inside) = towns[1].pulperia().unwrap();
        let door = inside.door;
        // Afuera, frente a la puerta (cerrada de noche) o contra la pared
        // frente a él: no llega.
        let him = e.standing(Npc::Bairoletto).position;
        let wall = him + door.outward * ((door.center - him).dot(door.outward) + 0.4);
        assert!(wall.distance(him) <= Errand::SEATED_TALK_RANGE);
        for outside in [door.center + door.outward * 0.5, wall] {
            assert_eq!(e.interact(outside), Interaction::Nothing);
        }
        assert!(!e.talked_to(Npc::Bairoletto));
        // En el paso desde la puerta, junto a su banco.
        let aisle = inside.way_out(inside.front_seat())[2];
        assert_eq!(
            e.interact(aisle),
            Interaction::Event(ErrandEvent::Talked(Npc::Bairoletto))
        );
        assert!(e.talked_to(Npc::Bairoletto));
        assert!(!e.talked_to(Npc::Farach));
    }

    #[test]
    fn at_the_door_of_la_colonia_lisandro_walks_in_stops_inside_and_then_goes_to_the_table() {
        let (towns, e) = errand();
        let (_, inside) = towns[1].pulperia().unwrap();
        let door = inside.door;
        let along = Vec2::new(-door.outward.y, door.outward.x);
        let seat = inside.seats[inside.front_seat()];
        let tano = Step {
            goal: Goal::Talk(Npc::Bairoletto),
            objective: Some("Andá a la pulpería".into()),
            scene: vec![],
        };
        let mut e = e.with_story(vec![tano.clone()]);
        // Llegando por la vereda, a un lado de la puerta.
        let sidewalk = door.center + door.outward * 1.5 + along * 1.0;
        let way = e.walk_in(sidewalk).expect("walks in");
        // Entra derecho por la puerta y se detiene unos pasos adentro.
        let stop = *way.last().unwrap();
        assert_eq!(way.len(), 2);
        assert!(!inside.floor.contains_point(way[0]));
        assert!(way[0].distance(door.center) < 1.5);
        assert!(inside.floor.contains_point(stop));
        let doorway = inside.doorway();
        let clear = |from: Vec2, to: Vec2| {
            (0..=40).all(|k| {
                let p = from + (to - from) * (k as f32 / 40.0);
                inside
                    .obstacles()
                    .all(|o| o.closest_point(p).distance(p) > crate::Rider::PERSON_RADIUS)
            })
        };
        assert!(
            (0..=20).any(|k| doorway.contains_point(way[0] + (stop - way[0]) * (k as f32 / 20.0))),
            "{way:?} through the door"
        );
        assert!(clear(way[0], stop), "{way:?}");
        // Desde ahí Bairoletto lo ve sin girar más la cabeza de lo natural,
        // y lo alcanza.
        assert!(turn_to(seat.position, seat.facing, stop) <= Errand::GAZE_MAX);
        assert!(stop.distance(seat.position) <= Errand::SEATED_TALK_RANGE);
        // Después va hasta la cabecera de su mesa, donde Bairoletto lo mira de
        // frente, girando apenas la cabeza.
        let beside = e
            .place_to_talk(Npc::Bairoletto, stop, Travel::OnFoot)
            .unwrap();
        assert!(inside.floor.contains_point(beside));
        assert!(clear(stop, beside), "{stop:?} to {beside:?}");
        assert!(turn_to(seat.position, seat.facing, beside) <= Errand::GAZE_MAX);
        assert!((0.8..=2.0).contains(&beside.distance(seat.position)));
        // Lejos de la puerta, o ya adentro: nada.
        assert_eq!(
            e.walk_in(door.center + door.outward * 1.5 + along * 4.0),
            None
        );
        assert_eq!(e.walk_in(stop), None);
        // Desde donde se detiene, la escena se cumple como con F.
        assert_eq!(
            e.talk(Npc::Bairoletto, stop),
            Some(ErrandEvent::Talked(Npc::Bairoletto))
        );
        assert_eq!(e.walk_in(sidewalk), None, "only while his step is due");
        // Antes de su paso, tampoco.
        let (_, early) = errand();
        let early = early.with_story(vec![story()[0].clone(), tano]);
        assert_eq!(early.walk_in(sidewalk), None);
        // A los que están parados se les habla desde donde se esté.
        assert_eq!(e.place_to_talk(Npc::Farach, stop, Travel::OnFoot), None);
    }

    /// El acto 6: el encuentro de noche en la puerta y la huida por el campo.
    fn escape(e: Errand) -> Errand {
        let step = |goal, objective: &str| Step {
            goal,
            objective: Some(objective.into()),
            scene: vec![],
        };
        e.with_story(vec![
            step(Goal::Rendezvous, "Volvé de noche a La Colonia"),
            step(Goal::Hideout, "Perdé a la milicada en el campo"),
        ])
    }

    #[test]
    fn at_night_bairoletto_waits_at_the_door_and_the_act_starts_only_on_horseback() {
        let (towns, e) = errand();
        let (_, inside) = towns[1].pulperia().unwrap();
        let door = inside.door;
        let mut e = escape(e);
        assert_eq!(e.stage(), ErrandStage::Rendezvous);
        // Parado en la vereda, frente a la puerta, mirando a la calle; ahí
        // apunta el mapa. Hasta que el juego lo pone ahí sigue en su asiento.
        let seat = e.standing(Npc::Bairoletto);
        assert!(inside.floor.contains_point(seat.position));
        let him = e.porch().unwrap();
        e.place(him);
        assert_eq!(e.standing(Npc::Bairoletto), him);
        assert_eq!(him.town, 1);
        assert!(
            him.position
                .distance(door.center + door.outward * Errand::DOORSTEP)
                < 1e-4
        );
        assert!(him.heading.forward().dot(door.outward) > 0.999);
        assert!(!inside.floor.contains_point(him.position));
        assert_eq!(e.target(), Some(him.position));
        assert_eq!(
            e.objective().as_deref(),
            Some("Volvé de noche a La Colonia")
        );
        // De noche desde las 20 hasta que amanece.
        assert!(!e.waits_at_door(19.9));
        assert!(e.waits_at_door(20.0) && e.waits_at_door(23.5) && e.waits_at_door(5.9));
        assert!(!e.waits_at_door(6.0));
        let near = him.position + door.outward * (Errand::MEET_RANGE - 0.5);
        let far = him.position + door.outward * (Errand::MEET_RANGE + 0.5);
        // De día, a pie, o lejos, no pasa nada; F tampoco.
        assert_eq!(e.rendezvous(near, Travel::Riding, 15.0), None);
        assert_eq!(e.rendezvous(near, Travel::OnFoot, 21.0), None);
        assert_eq!(e.rendezvous(far, Travel::Riding, 21.0), None);
        assert_eq!(e.interact(near), Interaction::Nothing);
        assert_eq!(e.stage(), ErrandStage::Rendezvous);
        // A pie, de noche y cerca, le pide el caballo; a caballo, de día o
        // lejos, no.
        assert!(e.asks_for_horse(near, Travel::OnFoot, 21.0));
        assert!(!e.asks_for_horse(near, Travel::Riding, 21.0));
        assert!(!e.asks_for_horse(near, Travel::OnFoot, 15.0));
        assert!(!e.asks_for_horse(far, Travel::OnFoot, 21.0));
        // Los caballos de la milicada, atados frente a la pulpería, al costado
        // de la puerta sin taparla, mirándola, sin pisar nada.
        let world = World::with_town(Terrain::default(), &towns[1]);
        let tied = e.milicada_horses(&world, 4);
        assert_eq!(tied.len(), 4);
        let along = Vec2::new(-door.outward.y, door.outward.x);
        for (k, &(at, heading)) in tied.iter().enumerate() {
            let from = at - door.center;
            assert!((from.dot(door.outward) - Errand::TIE_OUT).abs() < 1e-3);
            assert!(from.dot(along).abs() >= Errand::TIE_CLEAR - 1e-3, "{k}");
            assert!(heading.forward().dot(door.outward) < -0.999);
            let half = Vec2::new(Errand::TIE_ROOM, Errand::TIE_ROOM) * 0.5;
            assert!(
                world
                    .obstacles
                    .near(Aabb::new(at - half, at + half))
                    .is_empty()
            );
            for &(other, _) in &tied[..k] {
                assert!(other.distance(at) >= Errand::TIE_SPACING - 1e-3);
            }
        }
        // De noche, a caballo y cerca: empieza el acto, una sola vez.
        assert_eq!(
            e.rendezvous(near, Travel::Riding, 21.0),
            Some(ErrandEvent::Rendezvous)
        );
        assert_eq!(e.rendezvous(near, Travel::Riding, 21.0), None);
        assert_eq!(e.stage(), ErrandStage::Hideout);
    }

    #[test]
    fn amadeo_does_not_hail_during_the_escape_and_keeps_his_meeting_for_later() {
        let (_, e) = errand();
        let mut e = escape(e).with_amadeo(cheese());
        let him = e.standing(Npc::Amadeo).position;
        let near = east(him, Errand::HAIL_RANGE - 0.2);
        let door = e.porch().unwrap().position;
        let town = Aabb::from_center_size(door, Vec2::new(400.0, 400.0));
        assert!(e.rendezvous(door, Travel::Riding, 21.0).is_some());
        // Huyendo con Bairoletto en ancas, pasa por su puesto y no lo llama.
        assert_eq!(e.hail(near, Travel::Riding), None);
        assert!(!e.talked_to(Npc::Amadeo));
        // Terminada la huida, el encuentro sigue pendiente.
        let field = east(door, 1000.0);
        assert!(e.shake_off(field, town, true).is_some());
        assert!(e.hail(near, Travel::Riding).is_some());
    }

    #[test]
    fn farach_keeps_his_door_until_the_night_of_act_six_then_falls_shot_in_la_colonia_for_good() {
        let (towns, e) = errand();
        let (_, inside) = towns[1].pulperia().unwrap();
        let (spot, facing) = inside.at_counter().unwrap();
        let door = e.standing(Npc::Farach);
        let counter = Standing {
            town: 1,
            position: spot,
            heading: facing,
        };
        let up = |at| Some(Whereabouts { at, fallen: false });
        let down = Some(Whereabouts {
            at: counter,
            fallen: true,
        });
        // Antes del acto 6, de día y de noche, en la puerta de su comisaría.
        assert_eq!((e.farach(15.0), e.farach(21.0)), (up(door), up(door)));
        let mut e = escape(e);
        // La noche del acto 6, parado en el mostrador de La Colonia; de día
        // sigue en su puerta.
        assert_eq!((e.farach(15.0), e.farach(21.0)), (up(door), up(counter)));
        let porch = e.porch().unwrap().position;
        assert!(e.rendezvous(porch, Travel::Riding, 21.0).is_some());
        assert_eq!(e.farach(21.0), up(counter));
        // Desde el disparo, tendido ahí, aunque amanezca durante la huida.
        e.shoot_farach();
        assert_eq!((e.farach(21.0), e.farach(7.0)), (down, down));
        // Terminado el acto ya no está en ninguna parte (herido), tampoco en
        // su puerta.
        let town = Aabb::from_center_size(porch, Vec2::new(400.0, 400.0));
        assert!(e.shake_off(east(porch, 1000.0), town, true).is_some());
        assert_eq!(e.stage(), ErrandStage::Done);
        assert_eq!((e.farach(23.0), e.farach(12.0)), (None, None));
    }

    #[test]
    fn the_shot_is_heard_with_the_first_caption_of_the_door_scene() {
        let caption = |t: &str| Beat::Cue(Cue::Caption(t.into()));
        let door = [
            Beat::Line {
                speaker: crate::story::Speaker::Npc(Npc::Bairoletto),
                text: "Esperame dos minutos".into(),
            },
            Beat::Cue(Cue::Enter),
            Beat::Cue(Cue::Pause(4)),
            caption("[Un disparo]"),
            caption("[Gritos y tumulto]"),
            Beat::Cue(Cue::RunToHorse),
        ];
        // Jugados hasta la pausa, no; desde el primer subtítulo, sí.
        assert!(!Errand::shot_heard(&door, 3));
        assert!(Errand::shot_heard(&door, 4) && Errand::shot_heard(&door, 6));
        // Lo que se repite al atraparlo, desde que sale corriendo, no dispara.
        assert!(!Errand::shot_heard(&door[5..], 1));
    }

    #[test]
    fn the_escape_ends_anywhere_in_the_country_once_the_milicada_lost_him() {
        let (_, e) = errand();
        let mut e = escape(e);
        let door = e.porch().unwrap().position;
        let town = Aabb::from_center_size(door, Vec2::new(400.0, 400.0));
        assert!(e.rendezvous(door, Travel::Riding, 21.0).is_some());
        // No hay un lugar fijo: ni marca en el mapa.
        assert_eq!(e.target(), None);
        assert_eq!(
            e.objective().as_deref(),
            Some("Perdé a la milicada en el campo")
        );
        let field = east(door, 1000.0);
        // Con la milicada detrás no se cumple, ni apretando la tecla.
        assert_eq!(e.shake_off(field, town, false), None);
        assert_eq!(e.interact(field), Interaction::Nothing);
        // Perdida en el pueblo tampoco: Bairoletto no se baja en la calle.
        assert_eq!(e.shake_off(door, town, true), None);
        assert_eq!(e.stage(), ErrandStage::Hideout);
        // Perdida en cualquier parte del campo, se cumple, una sola vez.
        assert_eq!(e.shake_off(field, town, true), Some(ErrandEvent::ShookOff));
        assert_eq!(e.shake_off(field, town, true), None);
        assert_eq!(e.stage(), ErrandStage::Done);
        assert_eq!((e.target(), e.objective()), (None, None));
        // Se bajó junto al caballo: ahí está, para mirarse al hablar.
        let beside = Standing {
            town: 1,
            position: field,
            heading: Heading::from_radians(0.0),
        };
        e.place(beside);
        assert_eq!(e.standing(Npc::Bairoletto), beside);
    }

    #[test]
    fn there_is_no_errand_without_two_towns_and_a_police_station() {
        let towns = towns();
        assert!(Errand::new(&towns, 0, 0).is_none(), "same town");
        assert!(Errand::new(&towns, 0, 5).is_none(), "missing town");
        assert!(Errand::new(&towns, 5, 1).is_none(), "missing town");
        let mut no_police = towns.clone();
        no_police[1]
            .buildings
            .retain(|b| b.kind != crate::BuildingKind::PoliceStation);
        assert!(Errand::new(&no_police, 0, 1).is_none());
    }

    #[test]
    fn the_police_town_is_found_by_name_or_else_the_nearest() {
        let mut towns = towns();
        assert_eq!(Errand::police_town(&towns, 0, "Eduardo Castex"), Some(1));
        let far = TownLayout::generate(&TownConfig {
            name: "Lejos".into(),
            origin: Vec2::new(6000.0, 5000.0),
            seed: 3,
            ..TownConfig::castex()
        })
        .unwrap();
        towns.push(far);
        assert_eq!(Errand::police_town(&towns, 0, "Lejos"), Some(2));
        // Nombre desconocido: el otro pueblo más cercano.
        assert_eq!(Errand::police_town(&towns, 0, "Atlantis"), Some(1));
        // Nunca el propio pueblo del que da el encargo.
        assert_eq!(Errand::police_town(&towns, 0, "Monte Nievas"), Some(1));
        assert_eq!(Errand::police_town(&towns[..1], 0, "Eduardo Castex"), None);
    }

    #[test]
    fn the_errand_runs_in_order_each_step_once() {
        let (_, mut e) = errand();
        let hidalgo = e.standing(Npc::Hidalgo).position;
        let horse = e.horse().1.position;
        let farach = e.standing(Npc::Farach).position;

        assert_eq!(e.target(), Some(hidalgo));
        // Fuera de orden: el caballo todavía no fue prestado y Farach no espera
        // al jugador.
        assert_eq!(e.mount(horse), None);
        assert_eq!(e.talk(Npc::Farach, farach), None);
        assert_eq!(e.stage(), ErrandStage::MeetHidalgo);

        assert_eq!(
            e.talk(Npc::Hidalgo, east(hidalgo, 2.5)),
            Some(ErrandEvent::Talked(Npc::Hidalgo))
        );
        assert_eq!(e.stage(), ErrandStage::TakeHorse);
        assert_eq!(e.target(), Some(horse));
        assert_eq!(e.talk(Npc::Hidalgo, hidalgo), None, "only once");
        assert_eq!(e.talk(Npc::Farach, farach), None);

        assert_eq!(e.mount(east(horse, 2.0)), Some(ErrandEvent::MountedHorse));
        assert_eq!(e.stage(), ErrandStage::Talk(Npc::Farach));
        assert_eq!(e.target(), Some(farach));
        assert_eq!(e.mount(horse), None, "only once");

        assert_eq!(
            e.talk(Npc::Farach, east(farach, 3.5)),
            Some(ErrandEvent::Talked(Npc::Farach))
        );
        assert_eq!(e.stage(), ErrandStage::Done);
        assert_eq!(e.target(), None);
        assert_eq!(e.talk(Npc::Farach, farach), None, "only once");
        assert_eq!(e.talk(Npc::Hidalgo, hidalgo), None);
        assert_eq!(e.mount(horse), None);
    }

    #[test]
    fn every_step_needs_the_player_in_range() {
        let (_, mut e) = errand();
        let hidalgo = e.standing(Npc::Hidalgo).position;
        assert_eq!(
            e.talk(Npc::Hidalgo, east(hidalgo, Errand::TALK_RANGE + 0.1)),
            None
        );
        assert_eq!(e.stage(), ErrandStage::MeetHidalgo);
        assert!(
            e.talk(Npc::Hidalgo, east(hidalgo, Errand::TALK_RANGE - 0.1))
                .is_some()
        );

        let horse = e.horse().1.position;
        assert_eq!(e.mount(east(horse, Errand::MOUNT_RANGE + 0.1)), None);
        assert_eq!(e.stage(), ErrandStage::TakeHorse);
        assert!(e.mount(east(horse, Errand::MOUNT_RANGE - 0.1)).is_some());

        let farach = e.standing(Npc::Farach).position;
        assert_eq!(
            e.talk(Npc::Farach, east(farach, Errand::REPORT_RANGE + 0.1)),
            None
        );
        assert_eq!(e.stage(), ErrandStage::Talk(Npc::Farach));
        assert!(
            e.talk(Npc::Farach, east(farach, Errand::REPORT_RANGE - 0.1))
                .is_some()
        );
        assert_eq!(e.stage(), ErrandStage::Done);
    }

    #[test]
    fn the_interact_key_does_what_is_in_range_and_due() {
        let (_, mut e) = errand();
        let hidalgo = e.standing(Npc::Hidalgo).position;
        let horse = e.horse().1.position;
        let farach = e.standing(Npc::Farach).position;
        let nowhere = Vec2::new(-7000.0, 7000.0);

        assert_eq!(e.interact(nowhere), Interaction::Nothing);
        assert_eq!(e.interact(farach), Interaction::Nothing);
        assert_eq!(e.interact(horse), Interaction::NotYourHorse);
        assert_eq!(e.stage(), ErrandStage::MeetHidalgo);
        assert_eq!(
            e.interact(hidalgo),
            Interaction::Event(ErrandEvent::Talked(Npc::Hidalgo))
        );
        assert_eq!(e.interact(hidalgo), Interaction::Nothing);
        assert_eq!(
            e.interact(horse),
            Interaction::Event(ErrandEvent::MountedHorse)
        );
        assert_eq!(e.interact(horse), Interaction::Nothing);
        assert_eq!(
            e.interact(farach),
            Interaction::Event(ErrandEvent::Talked(Npc::Farach))
        );
        assert_eq!(e.interact(farach), Interaction::Nothing);
        assert_eq!(e.stage(), ErrandStage::Done);
    }

    #[test]
    fn each_step_plays_its_own_scene_and_without_steps_there_is_nothing_to_do() {
        let (towns, mut e) = errand();
        assert_eq!(e.last_scene(), &[]);
        let hidalgo = e.standing(Npc::Hidalgo).position;
        e.talk(Npc::Hidalgo, hidalgo);
        assert_eq!(e.last_scene(), &story()[0].scene[..]);
        e.mount(e.horse().1.position);
        assert_eq!(e.last_scene(), &[], "taking the horse has no scene");
        e.talk(Npc::Farach, e.standing(Npc::Farach).position);
        assert_eq!(e.last_scene(), &story()[2].scene[..]);
        assert_eq!((e.step(), e.objective()), (None, None));

        let mut bare = Errand::new(&towns, 0, 1).unwrap();
        assert_eq!(bare.stage(), ErrandStage::Done);
        assert_eq!(bare.interact(hidalgo), Interaction::Nothing);
        assert_eq!(bare.interact(bare.horse().1.position), Interaction::Nothing);
    }

    #[test]
    fn the_report_objective_names_the_errands_own_police_town() {
        let mut towns = towns();
        towns[1].name = "General Pico".into();
        let mut e = Errand::new(&towns, 0, 1).unwrap().with_story(story());
        assert_eq!(e.police_name(), "General Pico");
        assert!(e.objective().unwrap().contains("Hidalgo"));
        let hidalgo = e.standing(Npc::Hidalgo).position;
        e.talk(Npc::Hidalgo, hidalgo);
        e.mount(e.horse().1.position);
        let text = e.objective().unwrap();
        assert!(
            text.contains("Farach") && text.contains("General Pico"),
            "{text}"
        );
        assert!(!text.contains("Castex"), "{text}");
    }

    #[test]
    fn the_errand_people_block_the_way_once_added_to_the_world() {
        let (towns, e) = errand();
        let mut world = World::with_towns(Default::default(), Terrain::default(), &towns);
        world.obstacles.extend(e.obstacles());
        let hidalgo = e.standing(Npc::Hidalgo).position;
        let c = crate::Circle::new(crate::Rider::PERSON_RADIUS);
        let end = world.resolve(c, hidalgo, |_| ());
        assert!(
            end.distance(hidalgo) >= Errand::PERSON_FOOTPRINT / 2.0,
            "{end:?}"
        );
    }
}
