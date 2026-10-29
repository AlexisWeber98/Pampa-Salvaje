//! Las líneas del encargo: lo que dicen Don Hidalgo y el comisario Farach, y lo
//! que dice el jugador ante un caballo que todavía no es suyo. Se leen de
//! `assets/dialogue/errand.ron` y se validan como las líneas de los choques.

use std::path::Path;

use serde::Deserialize;

use crate::{ContentError, check_pools, unsupported_in};

/// Dónde están las líneas del encargo, relativo a la carpeta de assets.
pub const ERRAND_FILE: &str = "dialogue/errand.ron";

/// Lo que se dice durante el encargo; cada una es una breve secuencia de líneas
/// que se muestran una tras otra.
///
/// Los campos desconocidos del archivo se ignoran, como en el archivo de
/// choques.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ErrandLines {
    /// Don Hidalgo, en la plaza: manda al jugador a ver a Farach y le presta el
    /// caballo.
    pub hidalgo: Vec<String>,
    /// El jugador, junto al caballo de Don Hidalgo antes de hablar con él.
    pub not_your_horse: Vec<String>,
    /// El comisario Farach, cuando el jugador se presenta.
    pub farach: Vec<String>,
    /// El pueblo a cuyo comisario el encargo envía al jugador, por nombre (el
    /// otro pueblo más cercano con comisaría cuando ese pueblo no tiene una).
    /// Por defecto, [`DEFAULT_POLICE_TOWN`].
    #[serde(default = "default_police_town")]
    pub police_town: String,
}

/// El pueblo de la comisaría del encargo cuando el archivo no nombra uno.
pub const DEFAULT_POLICE_TOWN: &str = "Eduardo Castex";

fn default_police_town() -> String {
    DEFAULT_POLICE_TOWN.to_string()
}

impl ErrandLines {
    /// Interpreta y valida el texto RON de un archivo de encargo.
    pub fn parse(text: &str) -> Result<Self, ContentError> {
        let lines: Self = ron::from_str(text).map_err(ContentError::Parse)?;
        lines.validate()?;
        Ok(lines)
    }

    /// Lee, interpreta y valida el archivo de encargo en `path`.
    pub fn load(path: &Path) -> Result<Self, ContentError> {
        let text = std::fs::read_to_string(path).map_err(|source| ContentError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text)
    }

    /// Las líneas compiladas en el juego, que se usan cuando el archivo falta o
    /// no es válido.
    pub fn builtin() -> Self {
        let own = |lines: &[&str]| lines.iter().map(|l| l.to_string()).collect();
        Self {
            hidalgo: own(&[
                "Acá en Monte Nievas no hay comisaría.",
                "Preséntese en la comisaría de Castex.",
                "Lo espera el comisario Elías Farach.",
                "Llévese mi caballo, el del almacén.",
            ]),
            not_your_horse: own(&["Este caballo no es mío."]),
            farach: own(&["¿Lo manda Don Hidalgo?", "Bienvenido a Castex."]),
            police_town: default_police_town(),
        }
    }

    /// Comprueba que cada secuencia tenga líneas, y que cada línea (y el nombre
    /// del pueblo de la comisaría) no esté en blanco y tenga como máximo
    /// [`crate::MAX_LINE_CHARS`] caracteres.
    pub fn validate(&self) -> Result<(), ContentError> {
        check_pools(&self.pools())
    }

    /// Cada carácter distinto de cualquier línea que `supported` rechaza, en
    /// orden de primera aparición.
    pub fn unsupported_chars(&self, supported: impl Fn(char) -> bool) -> Vec<char> {
        unsupported_in(&self.pools(), supported)
    }

    /// Cada secuencia con el nombre que tiene en el archivo.
    fn pools(&self) -> [(&'static str, &[String]); 4] {
        [
            ("hidalgo", &self.hidalgo),
            ("not_your_horse", &self.not_your_horse),
            ("farach", &self.farach),
            ("police_town", std::slice::from_ref(&self.police_town)),
        ]
    }
}

/// Carga el archivo de encargo en `path` y recurre a [`ErrandLines::builtin`]
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
    use crate::{MAX_LINE_CHARS, asset_path};

    const SHIPPED: &str = include_str!("../../../assets/dialogue/errand.ron");

    fn every_line(lines: &ErrandLines) -> impl Iterator<Item = &String> {
        lines
            .hidalgo
            .iter()
            .chain(&lines.not_your_horse)
            .chain(&lines.farach)
    }

    #[test]
    fn the_shipped_file_tells_the_errand_in_short_lines() {
        let lines = ErrandLines::parse(SHIPPED).expect("shipped errand.ron is valid");
        assert!(
            (3..=8).contains(&lines.hidalgo.len()),
            "{:?}",
            lines.hidalgo
        );
        assert!((2..=6).contains(&lines.farach.len()), "{:?}", lines.farach);
        assert!(!lines.not_your_horse.is_empty());
        for line in every_line(&lines) {
            assert!(line.chars().count() <= MAX_LINE_CHARS, "{line}");
            assert_eq!(line.trim(), line, "no stray spaces: {line:?}");
            for banned in ['"', ';', '(', ')'] {
                assert!(!line.contains(banned), "{line}");
            }
        }
        // La historia: no hay policía en Monte Nievas; hay que presentarse ante
        // Farach en la comisaría de Castex, en el caballo de Don Hidalgo, del
        // almacén de ramos generales.
        let hidalgo = lines.hidalgo.join(" ");
        for needed in ["comisaría", "Castex", "Farach", "caballo", "almacén"] {
            assert!(hidalgo.contains(needed), "Hidalgo never says {needed}");
        }
        assert!(lines.farach.join(" ").contains("Hidalgo"));
        assert_eq!(lines.police_town, "Eduardo Castex");
    }

    #[test]
    fn the_police_town_defaults_when_the_file_does_not_name_it() {
        let lines = ErrandLines::parse(r#"(hidalgo: ["a"], not_your_horse: ["b"], farach: ["c"])"#)
            .unwrap();
        assert_eq!(lines.police_town, DEFAULT_POLICE_TOWN);
        let named = ErrandLines::parse(
            r#"(hidalgo: ["a"], not_your_horse: ["b"], farach: ["c"], police_town: "General Pico")"#,
        )
        .unwrap();
        assert_eq!(named.police_town, "General Pico");
        let blank = ErrandLines::parse(
            r#"(hidalgo: ["a"], not_your_horse: ["b"], farach: ["c"], police_town: " ")"#,
        )
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
    fn invalid_content_is_rejected() {
        let long = "x".repeat(MAX_LINE_CHARS + 1);
        let ok = "y".repeat(MAX_LINE_CHARS);
        let file = |h: &str, n: &str, f: &str| {
            format!("(hidalgo: [{h}], not_your_horse: [{n}], farach: [{f}])")
        };
        assert!(ErrandLines::parse(&file(&format!("\"{ok}\""), "\"b\"", "\"c\"")).is_ok());
        let cases = [
            (file("", "\"b\"", "\"c\""), "`hidalgo` has no lines"),
            (file("\"a\"", "", "\"c\""), "`not_your_horse` has no lines"),
            (file("\"a\"", "\"b\"", ""), "`farach` has no lines"),
            (
                file("\"a\"", "\"b\"", "\"c\", \" \""),
                "line 1 of `farach` is blank",
            ),
            (
                file(&format!("\"a\", \"{long}\""), "\"b\"", "\"c\""),
                "41 characters",
            ),
            ("(hidalgo: [\"a\"])".to_string(), "invalid RON"),
            (String::new(), "invalid RON"),
        ];
        for (text, message) in cases {
            let error = ErrandLines::parse(&text).expect_err(&text);
            assert!(error.to_string().contains(message), "{text}: {error}");
        }
    }

    #[test]
    fn missing_or_broken_files_fall_back_to_the_builtin_lines() {
        let builtin = ErrandLines::builtin();
        builtin.validate().expect("builtin lines are valid");
        let (lines, error) = load_errand_or_builtin(Path::new("no/such/dir/errand.ron"));
        assert_eq!(lines, builtin);
        assert!(matches!(error, Some(ContentError::Io { .. })));
        let (lines, error) = load_errand_or_builtin(&asset_path(crate::BUMPS_FILE));
        assert_eq!(lines, builtin, "a bumps file is not an errand file");
        assert!(matches!(error, Some(ContentError::Parse(_))));
    }

    #[test]
    fn reports_characters_outside_a_supported_set() {
        let lines =
            ErrandLines::parse(r#"(hidalgo: ["ab#"], not_your_horse: ["a"], farach: ["~b#"])"#)
                .unwrap();
        assert_eq!(
            // El nombre del pueblo de la comisaría por defecto tiene un
            // espacio.
            lines.unsupported_chars(|c| c.is_ascii_alphabetic() || c == ' '),
            ['#', '~']
        );
        assert!(lines.unsupported_chars(|_| true).is_empty());
    }
}
