//! Contenido editable del juego para Pampa Salvaje, leído de archivos RON bajo
//! `assets/`.
//!
//! El contenido son datos, no código: los guionistas editan
//! `assets/dialogue/*.ron` (líneas) y `assets/world/towns.ron` (pueblos), y el
//! juego toma los cambios en su siguiente inicio, sin recompilar. Este crate
//! solo interpreta y valida; no sabe nada de cómo se dibuja cada cosa y nunca
//! debe depender de Bevy.
//!
//! Qué caracteres puede usar una línea lo decide la tipografía del cuerpo de
//! texto en `pampa-render`. Este crate no puede ver la tipografía, así que
//! ofrece [`BumpLines::unsupported_chars`] con un predicado que aporta quien
//! llama; el juego comprueba el archivo incluido contra la tipografía real en
//! sus tests y avisa en tiempo de ejecución.

use std::fmt;
use std::path::{Path, PathBuf};

use pampa_core::{BumpSeverity, PlacementError, WorldConfigError};
use serde::Deserialize;

pub mod errand;
pub mod slots;
pub mod towns;

pub use errand::{DEFAULT_POLICE_TOWN, ERRAND_FILE, ErrandLines, load_errand_or_builtin};
pub use slots::{ModelSlots, SLOTS_FILE, SlotFit, SlotRotor, load_model_slots};
pub use towns::{TOWNS_FILE, WorldContent, load_towns, load_towns_or_builtin, parse_towns};

/// Línea más larga, en caracteres, para que entre en un globo de diálogo.
pub const MAX_LINE_CHARS: usize = 40;

/// Dónde están las quejas por choques, relativo a la carpeta de assets.
pub const BUMPS_FILE: &str = "dialogue/bumps.ron";

/// Lo que dicen los vecinos cuando el jinete los choca, según la gravedad.
///
/// Los campos desconocidos del archivo se ignoran, así que los archivos más
/// nuevos (con grupos agregados después, p. ej. por aspecto de peatón) siguen
/// cargando en versiones anteriores.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BumpLines {
    /// Empujado al paso.
    pub walk: Vec<String>,
    /// Atropellado al galope.
    pub gallop: Vec<String>,
}

impl BumpLines {
    /// Interpreta y valida el texto RON de un archivo de choques.
    pub fn parse(text: &str) -> Result<Self, ContentError> {
        let lines: Self = ron::from_str(text).map_err(ContentError::Parse)?;
        lines.validate()?;
        Ok(lines)
    }

    /// Lee, interpreta y valida el archivo de choques en `path`.
    pub fn load(path: &Path) -> Result<Self, ContentError> {
        let text = std::fs::read_to_string(path).map_err(|source| ContentError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text)
    }

    /// Un pequeño grupo compilado en el juego, que se usa cuando el archivo
    /// falta o no es válido.
    pub fn builtin() -> Self {
        let own = |lines: &[&str]| lines.iter().map(|l| l.to_string()).collect();
        Self {
            walk: own(&["¡Eh, mirá por dónde andás!", "¡Cuidado, che!"]),
            gallop: own(&["¡Casi me pisa el pingo, che!", "¡Más despacio, bárbaro!"]),
        }
    }

    /// Comprueba que cada grupo tenga líneas, y que cada línea no esté en
    /// blanco y tenga como máximo [`MAX_LINE_CHARS`] caracteres.
    pub fn validate(&self) -> Result<(), ContentError> {
        check_pools(&self.pools())
    }

    /// Las líneas para un choque de gravedad `severity`.
    pub fn pool(&self, severity: BumpSeverity) -> &[String] {
        match severity {
            BumpSeverity::Walk => &self.walk,
            BumpSeverity::Gallop => &self.gallop,
        }
    }

    /// Cada carácter distinto de cualquier línea que `supported` rechaza, en
    /// orden de primera aparición.
    pub fn unsupported_chars(&self, supported: impl Fn(char) -> bool) -> Vec<char> {
        unsupported_in(&self.pools(), supported)
    }

    /// Cada grupo con el nombre que tiene en el archivo.
    fn pools(&self) -> [(&'static str, &[String]); 2] {
        [("walk", &self.walk), ("gallop", &self.gallop)]
    }
}

/// Comprueba que cada grupo con nombre tenga líneas, y que cada línea no esté
/// en blanco y tenga como máximo [`MAX_LINE_CHARS`] caracteres.
pub(crate) fn check_pools(pools: &[(&'static str, &[String])]) -> Result<(), ContentError> {
    for &(pool, lines) in pools {
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

/// Cada carácter distinto de cualquier línea de `pools` que `supported`
/// rechaza, en orden de primera aparición.
pub(crate) fn unsupported_in(
    pools: &[(&'static str, &[String])],
    supported: impl Fn(char) -> bool,
) -> Vec<char> {
    let mut found = Vec::new();
    for (_, lines) in pools {
        for c in lines.iter().flat_map(|l| l.chars()) {
            if !supported(c) && !found.contains(&c) {
                found.push(c);
            }
        }
    }
    found
}

/// Carga el archivo de choques en `path` y recurre a [`BumpLines::builtin`] si
/// falla. El error, si lo hay, se devuelve para que quien llama pueda
/// registrarlo.
pub fn load_bumps_or_builtin(path: &Path) -> (BumpLines, Option<ContentError>) {
    match BumpLines::load(path) {
        Ok(lines) => (lines, None),
        Err(error) => (BumpLines::builtin(), Some(error)),
    }
}

/// Ruta de `relative` dentro de la carpeta `assets/` del workspace.
///
/// Prueba primero la carpeta junto a las fuentes de este workspace (para que
/// `cargo run` funcione desde cualquier directorio) y luego `assets/` bajo el
/// directorio actual (un juego distribuido junto a sus assets). Devuelve la
/// primera que existe, o la ruta del directorio actual cuando no existe
/// ninguna.
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

/// Por qué no se pudo usar el contenido.
#[derive(Debug)]
pub enum ContentError {
    /// No se pudo leer el archivo.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// El archivo no es RON válido para este contenido.
    Parse(ron::error::SpannedError),
    /// Un grupo no tiene líneas.
    EmptyPool { pool: &'static str },
    /// Una línea está vacía o solo tiene espacios en blanco.
    BlankLine { pool: &'static str, index: usize },
    /// Una línea supera los [`MAX_LINE_CHARS`] caracteres.
    LineTooLong {
        pool: &'static str,
        index: usize,
        chars: usize,
    },
    /// Un pueblo no se puede construir, o no entra en el mundo junto a los
    /// pueblos anteriores.
    Town(PlacementError),
    /// La sección `world` no se puede usar: se usa el mundo por defecto.
    World(WorldConfigError),
    /// No queda ningún pueblo válido: se usa el incorporado.
    NoTowns,
    /// Un pueblo pide más de [`pampa_core::TownConfig::MAX_TOWNSFOLK`]
    /// personas: en su lugar se usa la cantidad de gente de su categoría de
    /// tamaño.
    Townsfolk {
        town: String,
        wanted: usize,
        max: usize,
    },
    /// Una entrada de ranura de modelo no se puede usar: su ranura es
    /// desconocida, o un número está fuera de rango. No se usa ningún ajuste de
    /// ranuras.
    Slot { slot: String, reason: &'static str },
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
            Self::Townsfolk { town, wanted, max } => write!(
                f,
                "town `{town}`: townsfolk {wanted} is more than a town holds (max {max}); \
                 using its size's crowd"
            ),
            Self::Slot { slot, reason } => write!(f, "model slot `{slot}`: {reason}"),
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
        // Los grupos dicen cosas distintas.
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
        // La longitud cuenta caracteres, no bytes: los acentos están bien.
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
