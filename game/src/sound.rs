//! The player's sound settings: a general volume and the music's, set in
//! the menus' Sonido screen and kept between runs in `settings.ron` under
//! the user's config directory (`$XDG_CONFIG_HOME/pampa-seca`,
//! `~/.config/pampa-seca`, or `%APPDATA%\pampa-seca` on Windows).
//!
//! Each volume is a step from 0 to [`MAX_STEP`]. What the ear hears grows
//! with the square of the amplitude's step, so [`SoundSettings::music_gain`]
//! squares the product of the two: step 4 of 10 plays at 16 % amplitude.
//! Ambient sounds and effects get their own volumes when they come.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// The loudest step of a volume.
pub const MAX_STEP: u8 = 10;

/// The volumes, in steps of a tenth.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundSettings {
    /// Everything the game plays.
    pub general: u8,
    /// The menus' music.
    pub music: u8,
}

impl Default for SoundSettings {
    /// Full general volume, the music well under it (it was too loud at
    /// full, user 2026-10-28).
    fn default() -> Self {
        SoundSettings {
            general: MAX_STEP,
            music: 4,
        }
    }
}

impl SoundSettings {
    /// The music's amplitude, from 0 to 1.
    pub fn music_gain(&self) -> f32 {
        let loudness = fraction(self.general) * fraction(self.music);
        loudness * loudness
    }

    /// The settings in `path`, or the defaults when it is missing or
    /// unreadable (the error is returned for the log).
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

    /// Writes the settings to `path`, making its directory. The text goes
    /// to a temporary file beside it first and is renamed over `path`, so a
    /// crash or a full disk mid-write never leaves a truncated file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(std::io::Error::other)?;
        let mut temporary = path.as_os_str().to_owned();
        temporary.push(".tmp");
        let temporary = PathBuf::from(temporary);
        std::fs::write(&temporary, text)?;
        std::fs::rename(&temporary, path).inspect_err(|_| {
            let _ = std::fs::remove_file(&temporary);
        })
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

/// Where the settings live for this user, if a config directory is known.
pub fn settings_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }?;
    Some(base.join("pampa-seca").join("settings.ron"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory under the system temp dir, removed on drop.
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
