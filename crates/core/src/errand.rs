//! El primer encargo: Don Hidalgo, en la plaza del pueblo donde el jugador baja
//! del tren, lo manda a presentarse ante el comisario Elías Farach en la
//! comisaría de otro pueblo, y le presta el caballo atado en el almacén de
//! ramos generales para el viaje. En ese pueblo, Bairoletto espera sentado en
//! la pulpería.
//!
//! Reglas puras: dónde está cada personaje, los pasos de la historia en orden
//! ([`Step`], escritos como datos por el guion) y qué interacción está al
//! alcance. Hay un solo paso pendiente a la vez, y cada paso se cumple una
//! sola vez: solo su objetivo lo hace avanzar al siguiente.

use crate::story::{Beat, Goal, Step};
use crate::{Aabb, Heading, TetheredHorse, TownLayout, Vec2};

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
}

impl Npc {
    /// El nombre con que lo rotula un subtítulo.
    pub fn name(self) -> &'static str {
        match self {
            Self::Hidalgo => "Don Hidalgo",
            Self::Farach => "Farach",
            Self::Bairoletto => "Bairoletto",
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

/// Qué pide el paso actual de la historia (ver [`Goal`]), o que la historia
/// terminó.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrandStage {
    /// Hablar con alguien.
    Talk(Npc),
    /// Montar el caballo de Don Hidalgo.
    TakeHorse,
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
            None => Self::Done,
        }
    }
}

/// Algo que hizo avanzar el encargo: se cumplió el objetivo de un paso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrandEvent {
    /// Habló con alguien.
    Talked(Npc),
    /// Montó el caballo de Don Hidalgo: desde entonces es del jugador.
    MountedHorse,
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

/// El encargo: quién está dónde, el caballo y los pasos de la historia.
#[derive(Debug, Clone, PartialEq)]
pub struct Errand {
    steps: Vec<Step>,
    /// Índice del paso pendiente; `steps.len()` una vez terminada la historia.
    at: usize,
    hidalgo: Standing,
    farach: Standing,
    bairoletto: Standing,
    /// El asiento de Bairoletto en la pulpería del pueblo de Farach
    /// ([`crate::pulperia::Interior::seats`]); `None` si ese pueblo no tiene
    /// pulpería y espera parado junto a Farach.
    bairoletto_seat: Option<usize>,
    /// El piso de esa pulpería: se le habla a Bairoletto desde adentro.
    bairoletto_room: Option<Aabb>,
    /// Pueblo del caballo, y el caballo.
    horse: (usize, TetheredHorse),
    /// Nombre del pueblo en el que está Farach.
    police_name: String,
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

    /// El encargo desde el pueblo `giver` (Don Hidalgo en su plaza, su caballo
    /// en su almacén de ramos generales) hasta el pueblo `police` (Farach en la
    /// puerta de su primera comisaría, Bairoletto sentado en su pulpería, en
    /// [`crate::pulperia::Interior::front_seat`]). `None` si falta alguno de los pueblos,
    /// si son el mismo pueblo o si `police` no tiene comisaría.
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
        let (bairoletto, bairoletto_seat, bairoletto_room) = match away.pulperia() {
            Some((_, inside)) => {
                let seat = inside.front_seat();
                let at = inside.seats[seat];
                let standing = Standing {
                    town: police,
                    position: at.position,
                    heading: at.facing,
                };
                (standing, Some(seat), Some(inside.floor))
            }
            None => {
                let forward = facing.forward();
                let left = Vec2::new(-forward.y, forward.x);
                let standing = Standing {
                    position: door + left * Self::BAIROLETTO_GAP,
                    ..farach
                };
                (standing, None, None)
            }
        };
        Some(Self {
            steps: Vec::new(),
            at: 0,
            hidalgo: Standing {
                town: giver,
                position: spot,
                heading,
            },
            farach,
            bairoletto,
            bairoletto_seat,
            bairoletto_room,
            horse: (giver, home.hitching_post.horse),
            police_name: away.name.clone(),
        })
    }

    /// El encargo con los pasos `steps`, desde el primero.
    pub fn with_story(self, steps: Vec<Step>) -> Self {
        Self {
            steps,
            at: 0,
            ..self
        }
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
        self.goal().into()
    }

    /// El paso pendiente; `None` una vez terminada la historia.
    pub fn step(&self) -> Option<&Step> {
        self.steps.get(self.at)
    }

    fn goal(&self) -> Option<Goal> {
        self.step().map(|s| s.goal)
    }

    /// La escena del último paso cumplido (vacía antes del primero).
    pub fn last_scene(&self) -> &[Beat] {
        self.at
            .checked_sub(1)
            .and_then(|i| self.steps.get(i))
            .map_or(&[], |s| &s.scene)
    }

    /// Nombre del pueblo al que el encargo manda al jugador, el de Farach.
    pub fn police_name(&self) -> &str {
        &self.police_name
    }

    /// El objetivo del paso pendiente, con el pueblo de la comisaría en lugar
    /// de `{town}`; `None` una vez terminada la historia.
    pub fn objective(&self) -> Option<String> {
        self.step().map(|s| s.objective_in(&self.police_name))
    }

    /// Dónde está `npc`.
    pub fn standing(&self, npc: Npc) -> Standing {
        match npc {
            Npc::Hidalgo => self.hidalgo,
            Npc::Farach => self.farach,
            Npc::Bairoletto => self.bairoletto,
        }
    }

    /// El asiento de Bairoletto en la pulpería del pueblo de Farach; `None` si
    /// ese pueblo no tiene pulpería y Bairoletto espera parado junto a Farach.
    pub fn bairoletto_seat(&self) -> Option<usize> {
        self.bairoletto_seat
    }

    /// Si ya se cumplió un paso que pedía hablar con `npc`.
    pub fn talked_to(&self, npc: Npc) -> bool {
        self.steps[..self.at.min(self.steps.len())]
            .iter()
            .any(|s| s.goal == Goal::Talk(npc))
    }

    /// El caballo que presta Don Hidalgo, y su pueblo.
    pub fn horse(&self) -> (usize, TetheredHorse) {
        self.horse
    }

    /// Adónde manda al jugador el paso pendiente: a quien tiene que hablarle o
    /// al caballo; `None` una vez terminada la historia.
    pub fn target(&self) -> Option<Vec2> {
        match self.goal()? {
            Goal::Talk(npc) => Some(self.standing(npc).position),
            Goal::TakeHorse => Some(self.horse.1.position),
        }
    }

    /// Huellas de colisión de las personas que están paradas en el encargo:
    /// Hidalgo y Farach. Bairoletto no tiene: sentado, su banco ya es un
    /// obstáculo, y después se va.
    pub fn obstacles(&self) -> [Aabb; 2] {
        [self.hidalgo.footprint(), self.farach.footprint()]
    }

    /// Hasta dónde se le puede hablar a `npc`, en metros: a Farach también
    /// desde la montura.
    pub fn talk_range(npc: Npc) -> f32 {
        match npc {
            Npc::Hidalgo => Self::TALK_RANGE,
            Npc::Farach => Self::REPORT_RANGE,
            Npc::Bairoletto => Self::SEATED_TALK_RANGE,
        }
    }

    /// Habla con `npc` desde `at`: solo si está al alcance (a Bairoletto
    /// sentado, desde adentro de la pulpería), y solo si el paso pendiente lo
    /// pide.
    pub fn talk(&mut self, npc: Npc, at: Vec2) -> Option<ErrandEvent> {
        let inside = match (npc, self.bairoletto_room) {
            (Npc::Bairoletto, Some(room)) => room.contains_point(at),
            _ => true,
        };
        let near = inside && at.distance(self.standing(npc).position) <= Self::talk_range(npc);
        self.advance(Goal::Talk(npc), near, ErrandEvent::Talked(npc))
    }

    /// Monta el caballo desde `at`: solo si está al alcance, y solo si el paso
    /// pendiente lo pide (Don Hidalgo ya lo prestó).
    pub fn mount(&mut self, at: Vec2) -> Option<ErrandEvent> {
        let near = self.near_horse(at);
        self.advance(Goal::TakeHorse, near, ErrandEvent::MountedHorse)
    }

    /// Aprieta la tecla de interacción en `at`: habla o monta, lo que esté al
    /// alcance y corresponda.
    pub fn interact(&mut self, at: Vec2) -> Interaction {
        let event = match self.goal() {
            Some(Goal::Talk(npc)) => self.talk(npc, at),
            Some(Goal::TakeHorse) => self.mount(at),
            None => None,
        };
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
        self.steps
            .iter()
            .skip(self.at + 1)
            .any(|s| s.goal == Goal::TakeHorse)
    }

    fn near_horse(&self, at: Vec2) -> bool {
        at.distance(self.horse.1.position) <= Self::MOUNT_RANGE
    }

    /// Pasa al paso siguiente cuando el pendiente pide `due` y `near`,
    /// informando `event`; si no, no pasa nada, así que cada paso se cumple una
    /// sola vez.
    fn advance(&mut self, due: Goal, near: bool, event: ErrandEvent) -> Option<ErrandEvent> {
        if self.goal() != Some(due) || !near {
            return None;
        }
        self.at += 1;
        Some(event)
    }
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
                objective: "Buscá a Don Hidalgo".into(),
                scene: vec![
                    line(Npc::Hidalgo, "Hola"),
                    Beat::Act(Action::Earn(crate::Money::new(2, 0))),
                ],
            },
            Step {
                goal: Goal::TakeHorse,
                objective: "Tomá el caballo".into(),
                scene: vec![],
            },
            Step {
                goal: Goal::Talk(Npc::Farach),
                objective: "Presentate al comisario Farach en {town}".into(),
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
        let people = [Npc::Hidalgo, Npc::Farach].map(|npc| (format!("{npc:?}"), e.standing(npc)));
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

    #[test]
    fn bairoletto_is_spoken_to_from_the_aisle_but_not_through_the_wall() {
        let (towns, e) = errand();
        let mut e = e.with_story(vec![Step {
            goal: Goal::Talk(Npc::Bairoletto),
            objective: "Andá a la pulpería".into(),
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
