//! Editable game content for Pampa Seca, read from RON files under `assets/`.
//!
//! Content is data, not code: writers edit `assets/dialogue/*.ron` (lines)
//! and `assets/world/towns.ron` (towns), and the game picks the changes up
//! on its next start, without recompiling. This crate only parses and
//! validates; it knows nothing about how anything is drawn and must never
//! depend on Bevy.
//!
//! Which characters a line may use is decided by the bitmap font in
//! `pampa-render`. This crate cannot see the font, so it offers
//! [`BumpLines::unsupported_chars`] with a caller-supplied predicate; the
//! game checks the shipped file against the real font in its tests and
//! warns at runtime.

use std::fmt;
use std::path::{Path, PathBuf};

use pampa_core::{BumpSeverity, PlacementError, WorldConfigError};
use serde::Deserialize;

pub mod towns;

pub use towns::{TOWNS_FILE, WorldContent, load_towns, load_towns_or_builtin, parse_towns};

/// Longest line, in characters, so it fits a speech bubble.
pub const MAX_LINE_CHARS: usize = 40;

/// Where the bump complaints live, relative to the assets folder.
pub const BUMPS_FILE: &str = "dialogue/bumps.ron";

/// What townsfolk say when the rider bumps into them, by severity.
///
/// Unknown fields in the file are ignored, so newer files (with pools added
/// later, e.g. per walker look) still load in older builds.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BumpLines {
    /// Shoved at a walking pace.
    pub walk: Vec<String>,
    /// Run into at a gallop.
    pub gallop: Vec<String>,
}

impl BumpLines {
    /// Parses and validates the RON text of a bumps file.
    pub fn parse(text: &str) -> Result<Self, ContentError> {
        let lines: Self = ron::from_str(text).map_err(ContentError::Parse)?;
        lines.validate()?;
        Ok(lines)
    }

    /// Reads, parses and validates the bumps file at `path`.
    pub fn load(path: &Path) -> Result<Self, ContentError> {
        let text = std::fs::read_to_string(path).map_err(|source| ContentError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text)
    }

    /// A small pool compiled into the game, used when the file is missing or
    /// invalid.
    pub fn builtin() -> Self {
        let own = |lines: &[&str]| lines.iter().map(|l| l.to_string()).collect();
        Self {
            walk: own(&["¡Eh, mirá por dónde andás!", "¡Cuidado, che!"]),
            gallop: own(&["¡Casi me pisa el pingo, che!", "¡Más despacio, bárbaro!"]),
        }
    }

    /// Checks every pool has lines, and every line is non-blank and at most
    /// [`MAX_LINE_CHARS`] long.
    pub fn validate(&self) -> Result<(), ContentError> {
        for (pool, lines) in self.pools() {
            if lines.is_empty() {
                return Err(ContentError::EmptyPool { pool });
            }
            for (index, line) in lines.iter().enumerate() {
                let chars = line.chars().count();
                if line.trim().is_empty() {
                    return Err(ContentError::BlankLine { pool, index });
                }
                if chars > MAX_LINE_CHARS {
                    return Err(ContentError::LineTooLong { pool, index, chars });
                }
            }
        }
        Ok(())
    }

    /// The lines for a bump of `severity`.
    pub fn pool(&self, severity: BumpSeverity) -> &[String] {
        match severity {
            BumpSeverity::Walk => &self.walk,
            BumpSeverity::Gallop => &self.gallop,
        }
    }

    /// Every distinct character in any line that `supported` rejects, in
    /// order of first appearance.
    pub fn unsupported_chars(&self, supported: impl Fn(char) -> bool) -> Vec<char> {
        let mut found = Vec::new();
        for (_, lines) in self.pools() {
            for c in lines.iter().flat_map(|l| l.chars()) {
                if !supported(c) && !found.contains(&c) {
                    found.push(c);
                }
            }
        }
        found
    }

    /// Every pool with the name it has in the file.
    fn pools(&self) -> [(&'static str, &[String]); 2] {
        [("walk", &self.walk), ("gallop", &self.gallop)]
    }
}

/// Loads the bumps file at `path`, falling back to [`BumpLines::builtin`].
/// The error, if any, is returned so the caller can log it.
pub fn load_bumps_or_builtin(path: &Path) -> (BumpLines, Option<ContentError>) {
    match BumpLines::load(path) {
        Ok(lines) => (lines, None),
        Err(error) => (BumpLines::builtin(), Some(error)),
    }
}

/// Path of `relative` inside the workspace `assets/` folder.
///
/// Tries the folder next to this workspace's sources first (so `cargo run`
/// works from any directory), then `assets/` under the current directory (a
/// game shipped next to its assets). Returns the first that exists, or the
/// current-directory path when neither does.
pub fn asset_path(relative: &str) -> PathBuf {
    let beside_sources = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .join(relative);
    if beside_sources.exists() {
        beside_sources
    } else {
        Path::new("assets").join(relative)
    }
}

/// Why content could not be used.
#[derive(Debug)]
pub enum ContentError {
    /// The file could not be read.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The file is not valid RON for this content.
    Parse(ron::error::SpannedError),
    /// A pool has no lines.
    EmptyPool { pool: &'static str },
    /// A line is empty or only whitespace.
    BlankLine { pool: &'static str, index: usize },
    /// A line is longer than [`MAX_LINE_CHARS`].
    LineTooLong {
        pool: &'static str,
        index: usize,
        chars: usize,
    },
    /// A town cannot be built, or does not fit the world next to the
    /// towns before it.
    Town(PlacementError),
    /// The `world` section is unusable: the default world is used.
    World(WorldConfigError),
    /// No valid town is left: the built-in one is used.
    NoTowns,
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "cannot read {}: {source}", path.display()),
            Self::Parse(e) => write!(f, "invalid RON: {e}"),
            Self::EmptyPool { pool } => write!(f, "pool `{pool}` has no lines"),
            Self::BlankLine { pool, index } => write!(f, "line {index} of `{pool}` is blank"),
            Self::LineTooLong { pool, index, chars } => write!(
                f,
                "line {index} of `{pool}` has {chars} characters (max {MAX_LINE_CHARS})"
            ),
            Self::Town(error) => write!(f, "{error}"),
            Self::World(error) => write!(f, "{error}; using the default world"),
            Self::NoTowns => write!(f, "no towns to build; using the built-in Eduardo Castex"),
        }
    }
}

impl std::error::Error for ContentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse(e) => Some(e),
            Self::Town(error) => Some(error),
            Self::World(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIPPED: &str = include_str!("../../../assets/dialogue/bumps.ron");

    #[test]
    fn the_shipped_file_parses_with_full_pools() {
        let lines = BumpLines::parse(SHIPPED).expect("shipped bumps.ron is valid");
        assert!(lines.walk.len() >= 8, "{}", lines.walk.len());
        assert!(lines.gallop.len() >= 8, "{}", lines.gallop.len());
        for line in lines.walk.iter().chain(&lines.gallop) {
            assert!(line.chars().count() <= MAX_LINE_CHARS, "{line}");
            assert_eq!(line.trim(), line, "no stray spaces: {line:?}");
        }
        // The pools say different things.
        assert!(lines.walk.iter().all(|l| !lines.gallop.contains(l)));
    }

    #[test]
    fn the_shipped_file_is_found_and_loaded_from_disk() {
        let path = asset_path(BUMPS_FILE);
        assert!(path.is_file(), "{path:?}");
        assert_eq!(
            BumpLines::load(&path).unwrap(),
            BumpLines::parse(SHIPPED).unwrap()
        );
        let (lines, error) = load_bumps_or_builtin(&path);
        assert!(error.is_none(), "{error:?}");
        assert_eq!(lines, BumpLines::parse(SHIPPED).unwrap());
    }

    #[test]
    fn pools_are_chosen_by_severity() {
        let lines = BumpLines::parse(r#"(walk: ["a"], gallop: ["b", "c"])"#).unwrap();
        assert_eq!(lines.pool(BumpSeverity::Walk), ["a"]);
        assert_eq!(lines.pool(BumpSeverity::Gallop), ["b", "c"]);
    }

    #[test]
    fn unknown_fields_and_comments_are_allowed() {
        let text = r#"
            // A comment.
            (
                walk: ["a"],
                gallop: ["b"],
                by_look: { 3: ["c"] },
            )
        "#;
        assert_eq!(BumpLines::parse(text).unwrap().walk, ["a"]);
    }

    #[test]
    fn invalid_content_is_rejected() {
        let long = "x".repeat(MAX_LINE_CHARS + 1);
        let ok = "y".repeat(MAX_LINE_CHARS);
        assert!(BumpLines::parse(&format!(r#"(walk: ["{ok}"], gallop: ["b"])"#)).is_ok());
        let cases = [
            (r#"(walk: [], gallop: ["b"])"#.to_string(), "no lines"),
            (r#"(walk: ["a"], gallop: [])"#.to_string(), "no lines"),
            (r#"(walk: ["a"], gallop: ["  "])"#.to_string(), "blank"),
            (
                format!(r#"(walk: ["{long}"], gallop: ["b"])"#),
                "41 characters",
            ),
            (r#"(walk: ["a"])"#.to_string(), "invalid RON"),
            (r#"(walk: ["a", gallop: ["b"])"#.to_string(), "invalid RON"),
            ("".to_string(), "invalid RON"),
        ];
        for (text, message) in cases {
            let error = BumpLines::parse(&text).expect_err(&text);
            assert!(error.to_string().contains(message), "{text}: {error}");
        }
        // Length counts characters, not bytes: accents are fine.
        let accented = "á".repeat(MAX_LINE_CHARS);
        assert!(BumpLines::parse(&format!(r#"(walk: ["{accented}"], gallop: ["b"])"#)).is_ok());
    }

    #[test]
    fn missing_or_broken_files_fall_back_to_the_builtin_pool() {
        let builtin = BumpLines::builtin();
        builtin.validate().expect("builtin pool is valid");
        assert!(!builtin.walk.is_empty() && !builtin.gallop.is_empty());

        let missing = Path::new("no/such/dir/bumps.ron");
        assert!(matches!(
            BumpLines::load(missing),
            Err(ContentError::Io { .. })
        ));
        let (lines, error) = load_bumps_or_builtin(missing);
        assert_eq!(lines, builtin);
        assert!(error.unwrap().to_string().contains("no/such/dir"));

        let dir = std::env::temp_dir().join(format!("pampa-content-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let broken = dir.join("bumps.ron");
        std::fs::write(&broken, "(walk: [").unwrap();
        let (lines, error) = load_bumps_or_builtin(&broken);
        assert_eq!(lines, builtin);
        assert!(matches!(error, Some(ContentError::Parse(_))));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reports_characters_outside_a_supported_set() {
        let lines = BumpLines::parse(r#"(walk: ["ab#c"], gallop: ["a~b#"])"#).unwrap();
        let unsupported = lines.unsupported_chars(|c| c.is_ascii_alphabetic());
        assert_eq!(unsupported, ['#', '~']);
        assert!(lines.unsupported_chars(|_| true).is_empty());
    }

    #[test]
    fn asset_paths_fall_back_to_the_current_directory() {
        let path = asset_path("dialogue/does-not-exist.ron");
        assert_eq!(
            path,
            Path::new("assets").join("dialogue/does-not-exist.ron")
        );
    }
}
