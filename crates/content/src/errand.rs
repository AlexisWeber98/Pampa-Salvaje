//! The errand's lines: what Don Hidalgo and comisario Farach say, and what
//! the player says at a horse that is not theirs yet. Read from
//! `assets/dialogue/errand.ron`, validated like the bump lines.

use std::path::Path;

use serde::Deserialize;

use crate::{ContentError, check_pools, unsupported_in};

/// Where the errand lines live, relative to the assets folder.
pub const ERRAND_FILE: &str = "dialogue/errand.ron";

/// What is said during the errand, each a short sequence of lines shown one
/// after the other.
///
/// Unknown fields in the file are ignored, like in the bumps file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ErrandLines {
    /// Don Hidalgo, in the plaza: sends the player to Farach and lends the
    /// horse.
    pub hidalgo: Vec<String>,
    /// The player, at Don Hidalgo's horse before talking to him.
    pub not_your_horse: Vec<String>,
    /// Comisario Farach, when the player reports.
    pub farach: Vec<String>,
    /// The town whose comisario the errand sends the player to, by name
    /// (the nearest other town with a police station when no such town
    /// has one). Defaults to [`DEFAULT_POLICE_TOWN`].
    #[serde(default = "default_police_town")]
    pub police_town: String,
}

/// The errand's police town when the file does not name one.
pub const DEFAULT_POLICE_TOWN: &str = "Eduardo Castex";

fn default_police_town() -> String {
    DEFAULT_POLICE_TOWN.to_string()
}

impl ErrandLines {
    /// Parses and validates the RON text of an errand file.
    pub fn parse(text: &str) -> Result<Self, ContentError> {
        let lines: Self = ron::from_str(text).map_err(ContentError::Parse)?;
        lines.validate()?;
        Ok(lines)
    }

    /// Reads, parses and validates the errand file at `path`.
    pub fn load(path: &Path) -> Result<Self, ContentError> {
        let text = std::fs::read_to_string(path).map_err(|source| ContentError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text)
    }

    /// The lines compiled into the game, used when the file is missing or
    /// invalid.
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

    /// Checks every sequence has lines, and every line (and the police
    /// town's name) is non-blank and at most [`crate::MAX_LINE_CHARS`] long.
    pub fn validate(&self) -> Result<(), ContentError> {
        check_pools(&self.pools())
    }

    /// Every distinct character in any line that `supported` rejects, in
    /// order of first appearance.
    pub fn unsupported_chars(&self, supported: impl Fn(char) -> bool) -> Vec<char> {
        unsupported_in(&self.pools(), supported)
    }

    /// Every sequence with the name it has in the file.
    fn pools(&self) -> [(&'static str, &[String]); 4] {
        [
            ("hidalgo", &self.hidalgo),
            ("not_your_horse", &self.not_your_horse),
            ("farach", &self.farach),
            ("police_town", std::slice::from_ref(&self.police_town)),
        ]
    }
}

/// Loads the errand file at `path`, falling back to
/// [`ErrandLines::builtin`]. The error, if any, is returned so the caller
/// can log it.
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
        // The story: no police in Monte Nievas, report to Farach at the
        // Castex comisaría, on Don Hidalgo's horse from the general store.
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
            // The default police town's name has a space.
            lines.unsupported_chars(|c| c.is_ascii_alphabetic() || c == ' '),
            ['#', '~']
        );
        assert!(lines.unsupported_chars(|_| true).is_empty());
    }
}
