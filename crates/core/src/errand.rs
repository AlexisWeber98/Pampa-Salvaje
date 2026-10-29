//! El primer encargo: Don Hidalgo, en la plaza del pueblo donde el jugador baja
//! del tren, lo manda a presentarse ante el comisario Elías Farach en la
//! comisaría de otro pueblo, y le presta el caballo atado en el almacén de
//! ramos generales para el viaje.
//!
//! Reglas puras: dónde está cada personaje, las etapas en orden y qué
//! interacción está al alcance. Cada etapa es una variante de [`ErrandStage`],
//! así que un encargo nunca puede estar a la vez en "habló con Hidalgo" y "no
//! habló", y cada transición ocurre una sola vez: solo sucede desde la etapa
//! anterior a ella.

use crate::{Aabb, Heading, TetheredHorse, TownLayout, Vec2};

/// Quién participa en el encargo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Npc {
    /// Don Hidalgo, parado en la plaza: da el encargo.
    Hidalgo,
    /// El comisario Elías Farach, en la puerta de su comisaría.
    Farach,
}

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

/// Qué tan avanzado está el encargo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrandStage {
    /// Encontrar a Don Hidalgo en la plaza y hablar con él.
    MeetHidalgo,
    /// Tomar el caballo de Don Hidalgo, atado en el almacén de ramos generales.
    TakeHorse,
    /// Cabalgar hasta el otro pueblo y presentarse ante el comisario Farach.
    ReportToFarach,
    /// Presentado: no queda nada por hacer.
    Done,
}

impl ErrandStage {
    /// El objetivo de una línea que se muestra en pantalla para esta etapa, en
    /// español, para un encargo ante el comisario de `police_town`; `None` una
    /// vez terminado.
    pub fn objective(self, police_town: &str) -> Option<String> {
        match self {
            Self::MeetHidalgo => Some("Buscá a Don Hidalgo en la plaza".into()),
            Self::TakeHorse => Some("Tomá el caballo de Don Hidalgo".into()),
            Self::ReportToFarach => {
                Some(format!("Presentate al comisario Farach en {police_town}"))
            }
            Self::Done => None,
        }
    }
}

/// Algo que hizo avanzar el encargo. Cada uno ocurre como mucho una vez.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrandEvent {
    /// Habló con Don Hidalgo: mandó al jugador a ver a Farach y le prestó el
    /// caballo.
    TalkedToHidalgo,
    /// Montó el caballo de Don Hidalgo.
    MountedHorse,
    /// Se presentó ante el comisario Farach: el encargo está terminado.
    ReportedToFarach,
}

/// Lo que hizo apretar la tecla de interacción.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Interaction {
    /// El encargo avanzó.
    Event(ErrandEvent),
    /// Junto al caballo de Don Hidalgo antes de hablar con él: no le
    /// corresponde al jugador llevárselo.
    NotYourHorse,
    /// No hay nada al alcance con lo que interactuar.
    Nothing,
}

/// El encargo: quién está dónde, el caballo y la etapa actual.
#[derive(Debug, Clone, PartialEq)]
pub struct Errand {
    stage: ErrandStage,
    hidalgo: Standing,
    farach: Standing,
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
    /// A qué distancia a la izquierda de Farach está Bairoletto, en metros.
    pub const BAIROLETTO_GAP: f32 = 1.1;

    /// El encargo desde el pueblo `giver` (Don Hidalgo en su plaza, su caballo
    /// en su almacén de ramos generales) hasta el pueblo `police` (Farach en la
    /// puerta de su primera comisaría). `None` si falta alguno de los pueblos,
    /// si son el mismo pueblo o si `police` no tiene comisaría.
    pub fn new(towns: &[TownLayout], giver: usize, police: usize) -> Option<Self> {
        if giver == police {
            return None;
        }
        let (home, away) = (towns.get(giver)?, towns.get(police)?);
        let (door, facing) = away.police_door()?;
        let (spot, heading) = home.plaza_spot();
        Some(Self {
            stage: ErrandStage::MeetHidalgo,
            hidalgo: Standing {
                town: giver,
                position: spot,
                heading,
            },
            farach: Standing {
                town: police,
                position: door,
                heading: facing,
            },
            horse: (giver, home.hitching_post.horse),
            police_name: away.name.clone(),
        })
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

    pub fn stage(&self) -> ErrandStage {
        self.stage
    }

    /// Nombre del pueblo al que el encargo manda al jugador, el de Farach.
    pub fn police_name(&self) -> &str {
        &self.police_name
    }

    /// El objetivo de la etapa actual (ver [`ErrandStage::objective`]).
    pub fn objective(&self) -> Option<String> {
        self.stage.objective(&self.police_name)
    }

    /// Dónde está `npc`.
    pub fn standing(&self, npc: Npc) -> Standing {
        match npc {
            Npc::Hidalgo => self.hidalgo,
            Npc::Farach => self.farach,
        }
    }

    /// Dónde está Juan Bautista Bairoletto: a la izquierda de Farach junto a la
    /// puerta de la comisaría, mirando hacia el mismo lado. Todavía no tiene
    /// ningún papel en el encargo.
    pub fn bairoletto(&self) -> Standing {
        let forward = self.farach.heading.forward();
        let left = Vec2::new(-forward.y, forward.x);
        Standing {
            position: self.farach.position + left * Self::BAIROLETTO_GAP,
            ..self.farach
        }
    }

    /// El caballo que presta Don Hidalgo, y su pueblo.
    pub fn horse(&self) -> (usize, TetheredHorse) {
        self.horse
    }

    /// Adónde manda al jugador la etapa actual: a Don Hidalgo, después a su
    /// caballo y después a Farach; `None` una vez terminado.
    pub fn target(&self) -> Option<Vec2> {
        match self.stage {
            ErrandStage::MeetHidalgo => Some(self.hidalgo.position),
            ErrandStage::TakeHorse => Some(self.horse.1.position),
            ErrandStage::ReportToFarach => Some(self.farach.position),
            ErrandStage::Done => None,
        }
    }

    /// Huellas de colisión de las personas que están en el encargo: Hidalgo,
    /// Farach y Bairoletto.
    pub fn obstacles(&self) -> [Aabb; 3] {
        [
            self.hidalgo.footprint(),
            self.farach.footprint(),
            self.bairoletto().footprint(),
        ]
    }

    /// Habla con `npc` desde `at`: solo si está al alcance, y solo en la etapa
    /// que lo espera.
    pub fn talk(&mut self, npc: Npc, at: Vec2) -> Option<ErrandEvent> {
        let (due, range, next, event) = match npc {
            Npc::Hidalgo => (
                ErrandStage::MeetHidalgo,
                Self::TALK_RANGE,
                ErrandStage::TakeHorse,
                ErrandEvent::TalkedToHidalgo,
            ),
            Npc::Farach => (
                ErrandStage::ReportToFarach,
                Self::REPORT_RANGE,
                ErrandStage::Done,
                ErrandEvent::ReportedToFarach,
            ),
        };
        let near = at.distance(self.standing(npc).position) <= range;
        self.advance(due, near, next, event)
    }

    /// Monta el caballo desde `at`: solo si está al alcance, y solo una vez que
    /// Don Hidalgo lo prestó.
    pub fn mount(&mut self, at: Vec2) -> Option<ErrandEvent> {
        let near = self.near_horse(at);
        self.advance(
            ErrandStage::TakeHorse,
            near,
            ErrandStage::ReportToFarach,
            ErrandEvent::MountedHorse,
        )
    }

    /// Aprieta la tecla de interacción en `at`: habla o monta, lo que esté al
    /// alcance y corresponda.
    pub fn interact(&mut self, at: Vec2) -> Interaction {
        let event = match self.stage {
            ErrandStage::MeetHidalgo => self.talk(Npc::Hidalgo, at),
            ErrandStage::TakeHorse => self.mount(at),
            ErrandStage::ReportToFarach => self.talk(Npc::Farach, at),
            ErrandStage::Done => None,
        };
        match event {
            Some(event) => Interaction::Event(event),
            None if self.stage == ErrandStage::MeetHidalgo && self.near_horse(at) => {
                Interaction::NotYourHorse
            }
            None => Interaction::Nothing,
        }
    }

    fn near_horse(&self, at: Vec2) -> bool {
        at.distance(self.horse.1.position) <= Self::MOUNT_RANGE
    }

    /// Pasa de la etapa `due` a `next` cuando `near`, informando `event`; desde
    /// cualquier otra etapa no pasa nada, así que cada evento ocurre una sola
    /// vez.
    fn advance(
        &mut self,
        due: ErrandStage,
        near: bool,
        next: ErrandStage,
        event: ErrandEvent,
    ) -> Option<ErrandEvent> {
        if self.stage != due || !near {
            return None;
        }
        self.stage = next;
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

    fn errand() -> (Vec<TownLayout>, Errand) {
        let towns = towns();
        let errand = Errand::new(&towns, 0, 1).expect("a valid errand");
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
        for (npc, s) in people
            .into_iter()
            .chain([("Bairoletto".to_owned(), e.bairoletto())])
        {
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
    fn bairoletto_stands_beside_farach_without_touching_him() {
        let (_, e) = errand();
        let (farach, him) = (e.standing(Npc::Farach), e.bairoletto());
        assert_eq!((him.town, him.heading), (farach.town, farach.heading));
        let gap = him.position.distance(farach.position);
        assert!((gap - Errand::BAIROLETTO_GAP).abs() < 1e-4);
        assert!(gap > Errand::PERSON_FOOTPRINT);
        // A su lado, no adelante ni atrás.
        let along = (him.position - farach.position).dot(farach.heading.forward());
        assert!(along.abs() < 1e-4);
        assert!(e.obstacles()[2].contains_point(him.position));
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
            Some(ErrandEvent::TalkedToHidalgo)
        );
        assert_eq!(e.stage(), ErrandStage::TakeHorse);
        assert_eq!(e.target(), Some(horse));
        assert_eq!(e.talk(Npc::Hidalgo, hidalgo), None, "only once");
        assert_eq!(e.talk(Npc::Farach, farach), None);

        assert_eq!(e.mount(east(horse, 2.0)), Some(ErrandEvent::MountedHorse));
        assert_eq!(e.stage(), ErrandStage::ReportToFarach);
        assert_eq!(e.target(), Some(farach));
        assert_eq!(e.mount(horse), None, "only once");

        assert_eq!(
            e.talk(Npc::Farach, east(farach, 3.5)),
            Some(ErrandEvent::ReportedToFarach)
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
        assert_eq!(e.stage(), ErrandStage::ReportToFarach);
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
            Interaction::Event(ErrandEvent::TalkedToHidalgo)
        );
        assert_eq!(e.interact(hidalgo), Interaction::Nothing);
        assert_eq!(
            e.interact(horse),
            Interaction::Event(ErrandEvent::MountedHorse)
        );
        assert_eq!(e.interact(horse), Interaction::Nothing);
        assert_eq!(
            e.interact(farach),
            Interaction::Event(ErrandEvent::ReportedToFarach)
        );
        assert_eq!(e.interact(farach), Interaction::Nothing);
        assert_eq!(e.stage(), ErrandStage::Done);
    }

    #[test]
    fn every_stage_but_the_last_has_a_short_spanish_objective() {
        use ErrandStage::*;
        let town = "Eduardo Castex";
        for stage in [MeetHidalgo, TakeHorse, ReportToFarach] {
            let text = stage.objective(town).expect("an objective");
            assert!(
                !text.trim().is_empty() && text.chars().count() <= 48,
                "{text}"
            );
        }
        assert!(MeetHidalgo.objective(town).unwrap().contains("Hidalgo"));
        assert!(TakeHorse.objective(town).unwrap().contains("caballo"));
        assert!(ReportToFarach.objective(town).unwrap().contains("Farach"));
        assert_eq!(Done.objective(town), None);
    }

    #[test]
    fn the_report_objective_names_the_errands_own_police_town() {
        let mut towns = towns();
        towns[1].name = "General Pico".into();
        let mut e = Errand::new(&towns, 0, 1).unwrap();
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
