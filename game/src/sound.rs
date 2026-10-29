//! La configuración de sonido del jugador: un volumen general y el de la
//! música, que se ajustan en la pantalla Sonido de los menús y se guardan entre
//! ejecuciones en `settings.ron`, en el directorio de configuración del usuario
//! (`$XDG_CONFIG_HOME/pampa-salvaje`, `~/.config/pampa-salvaje`, o
//! `%APPDATA%\pampa-salvaje` en Windows). El directorio era `pampa-seca` hasta
//! que se renombró el juego (2026-10-28, antes de cualquier lanzamiento); ese
//! archivo viejo no se lee.
//!
//! Cada volumen es un paso de 0 a [`MAX_STEP`]. Lo que percibe el oído crece
//! con el cuadrado del paso de la amplitud, así que
//! [`SoundSettings::music_gain`] eleva al cuadrado el producto de ambos: el
//! paso 4 de 10 suena al 16 % de amplitud. Los sonidos ambientales y los
//! efectos tendrán sus propios volúmenes cuando lleguen.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// El paso más fuerte de un volumen.
pub const MAX_STEP: u8 = 10;

/// Los volúmenes, en pasos de un décimo.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundSettings {
    /// Todo lo que reproduce el juego.
    pub general: u8,
    /// La música de los menús.
    pub music: u8,
}

impl Default for SoundSettings {
    /// Volumen general al máximo y la música bastante por debajo (al máximo
    /// sonaba demasiado fuerte, usuario 2026-10-28).
    fn default() -> Self {
        SoundSettings {
            general: MAX_STEP,
            music: 4,
        }
    }
}

impl SoundSettings {
    /// La amplitud de la música, de 0 a 1.
    pub fn music_gain(&self) -> f32 {
        let loudness = fraction(self.general) * fraction(self.music);
        loudness * loudness
    }

    /// La configuración de `path`, o los valores por defecto cuando falta o no
    /// se puede leer (el error se devuelve para el log).
    pub fn load(path: &Path) -> (Self, Option<String>) {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return (Self::default(), None);
            }
            Err(e) => return (Self::default(), Some(format!("{}: {e}", path.display()))),
        };
        match ron::from_str::<SoundSettings>(&text) {
            Ok(settings) => (settings.clamped(), None),
            Err(e) => (Self::default(), Some(format!("{}: {e}", path.display()))),
        }
    }

    /// Escribe la configuración en `path` y crea su directorio. Primero el
    /// texto va a un archivo temporal a su lado, que luego se renombra sobre
    /// `path`, así que un cuelgue o un disco lleno a mitad de escritura nunca
    /// dejan un archivo truncado.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(std::io::Error::other)?;
        let mut temporary = path.as_os_str().to_owned();
        temporary.push(".tmp");
        let temporary = PathBuf::from(temporary);
        // Se escribe y se vuelca a disco antes de renombrar, para que un corte
        // de luz nunca deje un archivo vacío con el nombre nuevo; cualquier
        // fallo en el camino elimina el archivo temporal.
        let written = (|| {
            use std::io::Write;
            let mut file = std::fs::File::create(&temporary)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&temporary, path)
        })();
        if written.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        written
    }

    fn clamped(self) -> Self {
        SoundSettings {
            general: self.general.min(MAX_STEP),
            music: self.music.min(MAX_STEP),
        }
    }
}

fn fraction(step: u8) -> f32 {
    f32::from(step.min(MAX_STEP)) / f32::from(MAX_STEP)
}

/// Dónde está la configuración de este usuario, si se conoce un directorio de
/// configuración.
pub fn settings_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }?;
    Some(base.join("pampa-salvaje").join("settings.ron"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un directorio nuevo bajo el directorio temporal del sistema, que se
    /// elimina al soltarlo.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("pampa-sound-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_music_starts_well_under_full_volume() {
        let gain = SoundSettings::default().music_gain();
        assert!(gain > 0.0 && gain <= 0.2, "{gain}");
    }

    #[test]
    fn the_music_gain_follows_both_volumes() {
        let full = SoundSettings {
            general: MAX_STEP,
            music: MAX_STEP,
        };
        assert_eq!(full.music_gain(), 1.0);
        let half = SoundSettings { music: 5, ..full };
        assert_eq!(half.music_gain(), 0.25);
        let quiet = SoundSettings { general: 5, ..half };
        assert_eq!(quiet.music_gain(), 0.0625);
        assert_eq!(SoundSettings { general: 0, ..full }.music_gain(), 0.0);
        assert_eq!(SoundSettings { music: 0, ..full }.music_gain(), 0.0);
    }

    #[test]
    fn settings_are_saved_and_read_back() {
        let scratch = Scratch::new("round-trip");
        let path = scratch.0.join("nested").join("settings.ron");
        let settings = SoundSettings {
            general: 7,
            music: 2,
        };
        settings.save(&path).expect("saved");
        assert_eq!(SoundSettings::load(&path), (settings, None));
    }

    #[test]
    fn saving_replaces_the_old_file_whole_and_leaves_nothing_beside_it() {
        let scratch = Scratch::new("atomic");
        let path = scratch.0.join("settings.ron");
        SoundSettings::default().save(&path).expect("first save");
        let settings = SoundSettings {
            general: 3,
            music: 9,
        };
        settings.save(&path).expect("second save");
        assert_eq!(SoundSettings::load(&path), (settings, None));
        let files: Vec<_> = std::fs::read_dir(&scratch.0)
            .expect("dir")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(files, ["settings.ron"]);
    }

    #[test]
    fn a_failed_save_leaves_no_temporary_file_behind() {
        let scratch = Scratch::new("failed");
        // Un directorio no vacío donde debería ir el archivo: el renombrado
        // falla.
        let path = scratch.0.join("settings.ron");
        std::fs::create_dir_all(path.join("in-the-way")).expect("dir");
        assert!(SoundSettings::default().save(&path).is_err());
        let mut temporary = path.as_os_str().to_owned();
        temporary.push(".tmp");
        assert!(!PathBuf::from(temporary).exists());
        assert!(
            path.join("in-the-way").is_dir(),
            "what was there is untouched"
        );
    }

    #[test]
    fn a_missing_file_gives_the_defaults_quietly() {
        let scratch = Scratch::new("missing");
        let (settings, error) = SoundSettings::load(&scratch.0.join("settings.ron"));
        assert_eq!(settings, SoundSettings::default());
        assert_eq!(error, None);
    }

    #[test]
    fn a_broken_file_gives_the_defaults_and_says_why() {
        let scratch = Scratch::new("broken");
        let path = scratch.0.join("settings.ron");
        std::fs::create_dir_all(&scratch.0).expect("dir");
        std::fs::write(&path, "not ron at all {").expect("written");
        let (settings, error) = SoundSettings::load(&path);
        assert_eq!(settings, SoundSettings::default());
        assert!(error.is_some_and(|e| e.contains("settings.ron")));
    }

    #[test]
    fn out_of_range_and_missing_volumes_are_mended() {
        let scratch = Scratch::new("mended");
        let path = scratch.0.join("settings.ron");
        std::fs::create_dir_all(&scratch.0).expect("dir");
        std::fs::write(&path, "(general: 40)").expect("written");
        let (settings, error) = SoundSettings::load(&path);
        assert_eq!(error, None);
        assert_eq!(
            settings,
            SoundSettings {
                general: MAX_STEP,
                music: SoundSettings::default().music,
            }
        );
    }
}
