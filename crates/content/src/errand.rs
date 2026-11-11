//! La historia del capítulo 1 como pasos ([`Step`]): qué pide cada uno al
//! jugador, el objetivo que muestra el HUD y la escena que se reproduce al
//! cumplirlo (líneas de quien hable y acciones narradas). También el encuentro
//! con el Viejo Amadeo, fuera de la historia (una escena sola), lo que dice el
//! jugador ante un caballo que todavía no es suyo, y el pueblo de la
//! comisaría. Se leen de `assets/dialogue/errand.ron`.

use std::path::Path;

use pampa_core::{Action, Beat, Goal, Item, Money, Npc, Speaker, Step};
use serde::Deserialize;

use crate::ContentError;

/// Dónde está la historia, relativo a la carpeta de assets.
pub const ERRAND_FILE: &str = "dialogue/errand.ron";

/// Línea más larga de la historia, en caracteres. Se muestra como subtítulo,
/// no en un globo, así que puede ser más larga que [`crate::MAX_LINE_CHARS`];
/// con este largo cada línea queda en pantalla todo el tiempo de leerla sin
/// pasar el máximo de un subtítulo (9 s).
pub const STORY_LINE_CHARS: usize = 100;

/// Objetivo más largo, en caracteres, con el pueblo de la comisaría ya puesto:
/// una línea del HUD.
pub const OBJECTIVE_CHARS: usize = 48;

/// La historia del encargo, ya interpretada.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrandLines {
    /// El jugador, junto al caballo de Don Hidalgo antes de que se lo preste.
    pub not_your_horse: Vec<String>,
    /// El pueblo a cuyo comisario la historia envía al jugador, por nombre (el
    /// otro pueblo más cercano con comisaría cuando ese pueblo no tiene una).
    /// Por defecto, [`DEFAULT_POLICE_TOWN`].
    pub police_town: String,
    /// Los pasos de la historia, en orden.
    pub steps: Vec<Step>,
    /// La escena del encuentro con el Viejo Amadeo, que sucede una sola vez,
    /// la primera vez que el jugador pasa por su puesto, fuera de la historia
    /// (ver `pampa_core::Errand::hail`). Vacía: no hay encuentro.
    pub amadeo: Vec<Beat>,
}

/// El pueblo de la comisaría del encargo cuando el archivo no nombra uno.
pub const DEFAULT_POLICE_TOWN: &str = "Eduardo Castex";

fn default_police_town() -> String {
    DEFAULT_POLICE_TOWN.to_string()
}

/// El archivo tal como se escribe. Los campos desconocidos se ignoran, como en
/// el archivo de choques.
#[derive(Deserialize)]
struct File {
    not_your_horse: Vec<String>,
    #[serde(default = "default_police_town")]
    police_town: String,
    steps: Vec<StepFile>,
    #[serde(default)]
    amadeo: Vec<BeatFile>,
}

#[derive(Deserialize)]
struct StepFile {
    goal: GoalFile,
    /// Puede faltar: un paso sin aviso (ver [`Step::objective`]). Escrito, es
    /// un texto, sin `Some(...)`.
    #[serde(default, deserialize_with = "written")]
    objective: Option<String>,
    #[serde(default)]
    scene: Vec<BeatFile>,
}

/// Un campo opcional que, cuando está escrito, es el valor mismo.
fn written<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}

#[derive(Deserialize)]
enum NpcFile {
    Hidalgo,
    Farach,
    Bairoletto,
    Amadeo,
}

impl From<NpcFile> for Npc {
    fn from(npc: NpcFile) -> Self {
        match npc {
            NpcFile::Hidalgo => Npc::Hidalgo,
            NpcFile::Farach => Npc::Farach,
            NpcFile::Bairoletto => Npc::Bairoletto,
            NpcFile::Amadeo => Npc::Amadeo,
        }
    }
}

#[derive(Deserialize)]
enum GoalFile {
    Talk(NpcFile),
    TakeHorse,
}

/// Quién dice una línea: el protagonista por su nombre, o un personaje.
#[derive(Deserialize)]
enum Who {
    Lisandro,
    Hidalgo,
    Farach,
    Bairoletto,
    Amadeo,
}

#[derive(Deserialize)]
enum ItemFile {
    Letter,
    Cheese,
}

#[derive(Deserialize)]
enum BeatFile {
    Say(Who, String),
    Earn {
        #[serde(default)]
        pesos: u32,
        #[serde(default)]
        centavos: u32,
    },
    Receive(ItemFile),
    HandOver(ItemFile),
}

fn item(item: ItemFile) -> Item {
    match item {
        ItemFile::Letter => Item::Letter,
        ItemFile::Cheese => Item::Cheese,
    }
}

impl From<BeatFile> for Beat {
    fn from(beat: BeatFile) -> Self {
        match beat {
            BeatFile::Say(who, text) => Beat::Line {
                speaker: match who {
                    Who::Lisandro => Speaker::Protagonist,
                    Who::Hidalgo => Speaker::Npc(Npc::Hidalgo),
                    Who::Farach => Speaker::Npc(Npc::Farach),
                    Who::Bairoletto => Speaker::Npc(Npc::Bairoletto),
                    Who::Amadeo => Speaker::Npc(Npc::Amadeo),
                },
                text,
            },
            // Los centavos de más se validan después (ver `validate`).
            BeatFile::Earn { pesos, centavos } => Beat::Act(Action::Earn(Money(
                pesos.saturating_mul(100).saturating_add(centavos),
            ))),
            BeatFile::Receive(i) => Beat::Act(Action::Receive(item(i))),
            BeatFile::HandOver(i) => Beat::Act(Action::HandOver(item(i))),
        }
    }
}

impl From<StepFile> for Step {
    fn from(step: StepFile) -> Self {
        let goal = match step.goal {
            GoalFile::Talk(npc) => Goal::Talk(npc.into()),
            GoalFile::TakeHorse => Goal::TakeHorse,
        };
        Self {
            goal,
            objective: step.objective,
            scene: step.scene.into_iter().map(Beat::from).collect(),
        }
    }
}

impl ErrandLines {
    /// Interpreta y valida el texto RON de un archivo de historia.
    pub fn parse(text: &str) -> Result<Self, ContentError> {
        let file: File = ron::from_str(text).map_err(ContentError::Parse)?;
        let lines = Self {
            not_your_horse: file.not_your_horse,
            police_town: file.police_town,
            steps: file.steps.into_iter().map(Step::from).collect(),
            amadeo: file.amadeo.into_iter().map(Beat::from).collect(),
        };
        lines.validate()?;
        Ok(lines)
    }

    /// Lee, interpreta y valida el archivo de historia en `path`.
    pub fn load(path: &Path) -> Result<Self, ContentError> {
        let text = std::fs::read_to_string(path).map_err(|source| ContentError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text)
    }

    /// La historia compilada en el juego, que se usa cuando el archivo falta o
    /// no es válido: los mismos pasos, con menos líneas.
    pub fn builtin() -> Self {
        let say = |npc, text: &str| Beat::Line {
            speaker: Speaker::Npc(npc),
            text: text.into(),
        };
        Self {
            not_your_horse: vec!["Este caballo no es mío.".into()],
            police_town: default_police_town(),
            steps: vec![
                Step {
                    goal: Goal::Talk(Npc::Hidalgo),
                    objective: Some("Buscá a Don Hidalgo en la plaza".into()),
                    scene: vec![
                        say(
                            Npc::Hidalgo,
                            "En estos pagos quien manda es Farach, el comisario de Castex.",
                        ),
                        say(Npc::Hidalgo, "Llévele esta carta y tome mi caballo."),
                        Beat::Act(Action::Receive(Item::Letter)),
                        Beat::Act(Action::Earn(Money::new(3, 0))),
                    ],
                },
                Step {
                    goal: Goal::TakeHorse,
                    objective: Some("Tomá el caballo de Don Hidalgo".into()),
                    scene: vec![],
                },
                Step {
                    goal: Goal::Talk(Npc::Farach),
                    objective: Some("Presentate al comisario Farach en {town}".into()),
                    scene: vec![
                        Beat::Act(Action::HandOver(Item::Letter)),
                        say(
                            Npc::Farach,
                            "Quedate cerca que a la noche te devuelvo tus papeles firmados.",
                        ),
                    ],
                },
                Step {
                    goal: Goal::Talk(Npc::Bairoletto),
                    objective: Some("Andá a la pulpería La Colonia".into()),
                    scene: vec![
                        say(
                            Npc::Bairoletto,
                            "¿Queres ganarte unos pesos? Necesito transporte para esta noche.",
                        ),
                        say(
                            Npc::Bairoletto,
                            "Yo soy Bairoletto, pero me dicen Tano. Nos vemos a la noche en esta pulpería.",
                        ),
                    ],
                },
            ],
            amadeo: vec![
                say(
                    Npc::Amadeo,
                    "¡Aparcero! Venga, acérquese, ¿Es nuevo en el pueblo?",
                ),
                say(
                    Npc::Amadeo,
                    "No se haga problema joven, tome, llévese un pedazo,",
                ),
                Beat::Act(Action::Receive(Item::Cheese)),
            ],
        }
    }

    /// Comprueba que haya pasos y que `not_your_horse` tenga líneas; que
    /// ninguna línea, objetivo escrito ni el nombre del pueblo esté en blanco
    /// (un paso sin objetivo es un paso sin aviso); que
    /// cada línea tenga como máximo [`STORY_LINE_CHARS`] caracteres y cada
    /// objetivo (con el pueblo puesto) como máximo [`OBJECTIVE_CHARS`]; que
    /// la plata dada tenga menos de 100 centavos sueltos; y que ningún paso
    /// pida hablar con el Viejo Amadeo (su encuentro no es un paso, y nunca se
    /// cumpliría). La escena del Viejo Amadeo se valida igual que las de los
    /// pasos.
    pub fn validate(&self) -> Result<(), ContentError> {
        let fail = |part: String, reason: String| Err(ContentError::Story { part, reason });
        if self.police_town.trim().is_empty() {
            return fail("police_town".into(), "is blank".into());
        }
        if self.not_your_horse.is_empty() {
            return fail("not_your_horse".into(), "has no lines".into());
        }
        if self.steps.is_empty() {
            return fail("steps".into(), "has no steps".into());
        }
        for (index, line) in self.not_your_horse.iter().enumerate() {
            check_line(line)
                .or_else(|reason| fail(format!("not_your_horse line {index}"), reason))?;
        }
        for (s, step) in self.steps.iter().enumerate() {
            if step.goal == Goal::Talk(Npc::Amadeo) {
                return fail(
                    format!("step {s} goal"),
                    "Amadeo is an encounter (`amadeo`), not a step".into(),
                );
            }
            let Some(objective) = step.objective_in(&self.police_town) else {
                check_scene(&format!("step {s}"), &step.scene)?;
                continue;
            };
            let chars = objective.chars().count();
            if objective.trim().is_empty() {
                return fail(format!("step {s} objective"), "is blank".into());
            }
            if chars > OBJECTIVE_CHARS {
                return fail(
                    format!("step {s} objective"),
                    format!("has {chars} characters (max {OBJECTIVE_CHARS})"),
                );
            }
            check_scene(&format!("step {s}"), &step.scene)?;
        }
        check_scene("amadeo", &self.amadeo)
    }

    /// Cada carácter distinto de cualquier texto que se muestra (líneas,
    /// objetivos y el nombre del pueblo) que `supported` rechaza, en orden de
    /// primera aparición.
    pub fn unsupported_chars(&self, supported: impl Fn(char) -> bool) -> Vec<char> {
        let mut found = Vec::new();
        for c in self.texts().flat_map(str::chars) {
            if !supported(c) && !found.contains(&c) {
                found.push(c);
            }
        }
        found
    }

    /// Cada texto que se muestra, en orden.
    fn texts(&self) -> impl Iterator<Item = &str> {
        let steps = self.steps.iter().flat_map(|step| {
            step.objective
                .as_deref()
                .into_iter()
                .chain(step.scene.iter().filter_map(|beat| match beat {
                    Beat::Line { text, .. } => Some(text.as_str()),
                    Beat::Act(_) => None,
                }))
        });
        let amadeo = self.amadeo.iter().filter_map(|beat| match beat {
            Beat::Line { text, .. } => Some(text.as_str()),
            Beat::Act(_) => None,
        });
        std::iter::once(self.police_town.as_str())
            .chain(self.not_your_horse.iter().map(String::as_str))
            .chain(steps)
            .chain(amadeo)
    }
}

/// Comprueba la escena `scene` (de `part`, para el error): cada línea se puede
/// mostrar y ninguna paga nada.
fn check_scene(part: &str, scene: &[Beat]) -> Result<(), ContentError> {
    for (b, beat) in scene.iter().enumerate() {
        let fail = |reason: String| {
            Err(ContentError::Story {
                part: format!("{part} beat {b}"),
                reason,
            })
        };
        match beat {
            Beat::Line { text, .. } => check_line(text).or_else(fail)?,
            Beat::Act(Action::Earn(amount)) if amount.0 == 0 => {
                return fail("earns nothing".into());
            }
            Beat::Act(_) => {}
        }
    }
    Ok(())
}

/// Por qué una línea no se puede mostrar, si no se puede.
fn check_line(line: &str) -> Result<(), String> {
    let chars = line.chars().count();
    if line.trim().is_empty() {
        Err("is blank".into())
    } else if chars > STORY_LINE_CHARS {
        Err(format!("has {chars} characters (max {STORY_LINE_CHARS})"))
    } else {
        Ok(())
    }
}

/// Carga el archivo de historia en `path` y recurre a [`ErrandLines::builtin`]
/// si falla. El error, si lo hay, se devuelve para que quien llama pueda
/// registrarlo.
pub fn load_errand_or_builtin(path: &Path) -> (ErrandLines, Option<ContentError>) {
    match ErrandLines::load(path) {
        Ok(lines) => (lines, None),
        Err(error) => (ErrandLines::builtin(), Some(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_path;

    const SHIPPED: &str = include_str!("../../../assets/dialogue/errand.ron");

    /// Las líneas de la escena de `step`, con el nombre de quien las dice.
    fn lines_of(step: &Step) -> Vec<(&'static str, &str)> {
        step.scene
            .iter()
            .filter_map(|beat| match beat {
                Beat::Line { speaker, text } => Some((speaker.name(), text.as_str())),
                Beat::Act(_) => None,
            })
            .collect()
    }

    /// Un archivo mínimo con `steps` como pasos.
    fn file(steps: &str) -> String {
        format!(r#"(not_your_horse: ["b"], steps: [{steps}])"#)
    }

    const STEP: &str = r#"(goal: Talk(Hidalgo), objective: "o", scene: [Say(Hidalgo, "a")])"#;

    #[test]
    fn the_shipped_file_tells_acts_two_to_four_in_order_and_amadeo_apart() {
        let lines = ErrandLines::parse(SHIPPED).expect("shipped errand.ron is valid");
        let goals: Vec<Goal> = lines.steps.iter().map(|s| s.goal).collect();
        assert_eq!(
            goals,
            [
                Goal::Talk(Npc::Hidalgo),
                Goal::TakeHorse,
                Goal::Talk(Npc::Farach),
                Goal::Talk(Npc::Bairoletto),
            ]
        );
        assert_eq!(lines.police_town, "Eduardo Castex");
        assert!(!lines.not_your_horse.is_empty());
        // Acto 2: no hay policía en Monte Nievas; quien manda es Farach, en la
        // comisaría de Castex; el caballo de Don Hidalgo está en el almacén.
        let hidalgo = lines_of(&lines.steps[0]);
        assert!(hidalgo.iter().all(|(who, _)| *who == "Don Hidalgo"));
        let said: String = hidalgo
            .iter()
            .map(|(_, t)| *t)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(said.contains("en estos pagos quien manda es Farach, el comisario de Castex"));
        for needed in ["comisaría", "caballo", "almacén", "carta"] {
            assert!(said.contains(needed), "Hidalgo never says {needed}");
        }
        assert!(lines.steps[1].scene.is_empty());
        // Acto 4, palabra por palabra, en la pulpería de Castex.
        assert_eq!(
            lines.steps[3].objective_in(&lines.police_town).as_deref(),
            Some("Andá a la pulpería La Colonia")
        );
        assert_eq!(
            lines_of(&lines.steps[3]),
            [
                (
                    "Bairoletto",
                    "Che pibe, ese caballo que está afuera…¿Es tuyo?"
                ),
                ("Lisandro", "Si, es de mi tìo."),
                (
                    "Bairoletto",
                    "¿Queres ganarte unos pesos? Necesito transporte para esta noche."
                ),
                ("Lisandro", "si ¿hasta donde necesita ir?"),
                ("Bairoletto", "A la noche te digo ¿Cómo te llamas?"),
                ("Lisandro", "Lisandro."),
                (
                    "Bairoletto",
                    "Yo soy Bairoletto, pero me dicen Tano. Nos vemos a la noche en esta pulpería."
                ),
            ]
        );
        assert_eq!(lines.steps[3].scene.len(), 7, "only lines");
        // Acto 5, palabra por palabra, fuera de la historia (no es un paso): el
        // Viejo Amadeo llama al jugador desde su puesto y le regala un pedazo
        // de queso. Las dos líneas largas, cortadas sin cambiar ninguna
        // palabra.
        let amadeo = Step {
            goal: Goal::Talk(Npc::Amadeo),
            objective: None,
            scene: lines.amadeo.clone(),
        };
        assert_eq!(
            lines_of(&amadeo),
            [
                (
                    "Viejo Amadeo",
                    "¡Aparcero! Venga, acérquese, ¿Es nuevo en el pueblo?"
                ),
                (
                    "Viejo Amadeo",
                    "¿Le gustaría probar el mejor queso de todo el pago?"
                ),
                ("Lisandro", "Buenas, ¿los hace uste’?"),
                (
                    "Viejo Amadeo",
                    "Me extraña m’hijo, ¡pero claro! Leche sacada de mis propias vacas, ordeñadas con estas mismas manos."
                ),
                (
                    "Lisandro",
                    "Se ven muy buenos, pero mejor otro día señor, ando medio corto de plata."
                ),
                (
                    "Viejo Amadeo",
                    "No se haga problema joven, tome, llévese un pedazo,"
                ),
                (
                    "Viejo Amadeo",
                    "y si le gusta vuelva cuando quiera que aquí voy a estar."
                ),
                ("Lisandro", "Le agradezco mucho señor."),
            ]
        );
        // Le da el queso al decir "tome, llévese un pedazo".
        assert_eq!(amadeo.scene.len(), 9);
        assert_eq!(amadeo.scene[6], Beat::Act(Action::Receive(Item::Cheese)));
        for line in lines.texts() {
            assert_eq!(line.trim(), line, "no stray spaces: {line:?}");
        }
    }

    #[test]
    fn the_police_town_defaults_when_the_file_does_not_name_it() {
        let lines = ErrandLines::parse(&file(STEP)).unwrap();
        assert_eq!(lines.police_town, DEFAULT_POLICE_TOWN);
        let named = ErrandLines::parse(&format!(
            r#"(police_town: "General Pico", not_your_horse: ["b"], steps: [{STEP}])"#
        ))
        .unwrap();
        assert_eq!(named.police_town, "General Pico");
        let blank = ErrandLines::parse(&format!(
            r#"(police_town: " ", not_your_horse: ["b"], steps: [{STEP}])"#
        ))
        .expect_err("a blank town");
        assert!(blank.to_string().contains("police_town"), "{blank}");
    }

    #[test]
    fn the_shipped_file_is_found_and_loaded_from_disk() {
        let path = asset_path(ERRAND_FILE);
        assert!(path.is_file(), "{path:?}");
        let (lines, error) = load_errand_or_builtin(&path);
        assert!(error.is_none(), "{error:?}");
        assert_eq!(lines, ErrandLines::parse(SHIPPED).unwrap());
    }

    #[test]
    fn a_step_reads_its_goal_objective_lines_and_actions() {
        let lines = ErrandLines::parse(&file(
            r#"(goal: Talk(Farach), objective: "Ver a Farach en {town}", scene: [
                Say(Lisandro, "Buenas"), Say(Farach, "¿Y vos?"), Receive(Letter),
                Earn(pesos: 2, centavos: 50), HandOver(Letter)]),
               (goal: TakeHorse, objective: "Caballo")"#,
        ))
        .unwrap();
        let step = &lines.steps[0];
        assert_eq!(step.goal, Goal::Talk(Npc::Farach));
        assert_eq!(
            step.objective_in("Castex").as_deref(),
            Some("Ver a Farach en Castex")
        );
        assert_eq!(
            lines_of(step),
            [("Lisandro", "Buenas"), ("Farach", "¿Y vos?")]
        );
        assert_eq!(
            step.scene[2..],
            [
                Beat::Act(Action::Receive(Item::Letter)),
                Beat::Act(Action::Earn(Money::new(2, 50))),
                Beat::Act(Action::HandOver(Item::Letter)),
            ]
        );
        assert_eq!(lines.steps[1].goal, Goal::TakeHorse);
        assert!(lines.steps[1].scene.is_empty());
    }

    #[test]
    fn a_step_without_objective_has_no_hud_line() {
        // Ya no es el del Viejo Amadeo (S18: su encuentro no es un paso), pero
        // un paso sin aviso sigue siendo posible.
        let lines = ErrandLines::parse(&file(
            r#"(goal: Talk(Farach), scene: [Say(Farach, "¿Y vos?"), Receive(Cheese)])"#,
        ))
        .unwrap();
        let step = &lines.steps[0];
        assert_eq!(
            (step.goal, step.objective_in("Castex")),
            (Goal::Talk(Npc::Farach), None)
        );
        assert_eq!(lines_of(step), [("Farach", "¿Y vos?")]);
        assert_eq!(step.scene[1], Beat::Act(Action::Receive(Item::Cheese)));
        // Su escena se valida igual.
        let error = ErrandLines::parse(&file(r#"(goal: Talk(Farach), scene: [Say(Farach, " ")])"#))
            .expect_err("a blank line");
        assert!(
            error.to_string().contains("`step 0 beat 0`: is blank"),
            "{error}"
        );
    }

    #[test]
    fn invalid_content_is_rejected() {
        let long = "x".repeat(STORY_LINE_CHARS + 1);
        let ok = "y".repeat(STORY_LINE_CHARS);
        let say = |text: &str| {
            file(&format!(
                r#"(goal: TakeHorse, objective: "o", scene: [Say(Lisandro, "{text}")])"#
            ))
        };
        assert!(ErrandLines::parse(&say(&ok)).is_ok());
        let cases = [
            (file(""), "has no steps"),
            (
                format!(r#"(not_your_horse: [], steps: [{STEP}])"#),
                "`not_your_horse`: has no lines",
            ),
            (say(" "), "`step 0 beat 0`: is blank"),
            (say(&long), "has 101 characters (max 100)"),
            (
                file(r#"(goal: TakeHorse, objective: "")"#),
                "`step 0 objective`: is blank",
            ),
            (
                file(
                    r#"(goal: TakeHorse, objective: "Presentate al comisario Farach en {town} ya")"#,
                ),
                "characters (max 48)",
            ),
            (
                file(r#"(goal: TakeHorse, objective: "o", scene: [Earn()])"#),
                "earns nothing",
            ),
            (
                file(r#"(goal: Talk(Lisandro), objective: "o")"#),
                "invalid RON",
            ),
            (
                file(r#"(goal: Talk(Amadeo), scene: [Say(Amadeo, "a")])"#),
                "`step 0 goal`: Amadeo is an encounter",
            ),
            (
                format!(r#"(not_your_horse: ["b"], steps: [{STEP}], amadeo: [Say(Amadeo, " ")])"#),
                "`amadeo beat 0`: is blank",
            ),
            (String::new(), "invalid RON"),
        ];
        for (text, message) in cases {
            let error = ErrandLines::parse(&text).expect_err(&text);
            assert!(error.to_string().contains(message), "{text}: {error}");
        }
    }

    #[test]
    fn missing_or_broken_files_fall_back_to_the_builtin_story() {
        let builtin = ErrandLines::builtin();
        builtin.validate().expect("the builtin story is valid");
        let (lines, error) = load_errand_or_builtin(Path::new("no/such/dir/errand.ron"));
        assert_eq!(lines, builtin);
        assert!(matches!(error, Some(ContentError::Io { .. })));
        let (lines, error) = load_errand_or_builtin(&asset_path(crate::BUMPS_FILE));
        assert_eq!(lines, builtin, "a bumps file is not a story file");
        assert!(matches!(error, Some(ContentError::Parse(_))));
    }

    #[test]
    fn reports_characters_outside_a_supported_set() {
        let lines = ErrandLines::parse(
            r##"(not_your_horse: ["a"], steps: [(goal: TakeHorse, objective: "o~",
                scene: [Say(Farach, "ab#")])])"##,
        )
        .unwrap();
        assert_eq!(
            // El nombre del pueblo de la comisaría por defecto tiene un
            // espacio.
            lines.unsupported_chars(|c| c.is_ascii_alphabetic() || c == ' '),
            ['~', '#']
        );
        assert!(lines.unsupported_chars(|_| true).is_empty());
    }
}
