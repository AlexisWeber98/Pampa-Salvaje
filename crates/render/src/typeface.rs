//! The game's typefaces, loaded once for every UI that letters text.
//!
//! - Display: Rye (Sorkin Type Co, SIL OFL 1.1), the wood type of a 1919
//!   poster, for headings ("SE BUSCA", "Pampa Seca").
//! - Body: IM Fell English (Igino Marini, SIL OFL 1.1), old book print, in
//!   roman and italic, for everything else.
//!
//! The fonts live in `assets/fonts/`, each with its licence (`OFL.txt`);
//! see `CREDITS.md`. Without an asset server (headless tests) the handles
//! stay default and nothing is loaded.

use bevy::prelude::*;

/// Rye, relative to the asset root.
pub const DISPLAY_FONT: &str = "fonts/rye/Rye-Regular.ttf";
/// IM Fell English roman, relative to the asset root.
pub const BODY_FONT: &str = "fonts/im_fell_english/IMFeENrm28P.ttf";
/// IM Fell English italic, relative to the asset root.
pub const ITALIC_FONT: &str = "fonts/im_fell_english/IMFeENit28P.ttf";

/// Handles to the game's typefaces.
#[derive(Resource, Debug, Clone, Default)]
pub struct Typefaces {
    pub display: Handle<Font>,
    pub body: Handle<Font>,
    pub italic: Handle<Font>,
}

/// Loads [`Typefaces`] before `Startup`, so every UI spawned there can use
/// them.
pub struct TypefacePlugin;

impl Plugin for TypefacePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Typefaces>()
            .add_systems(PreStartup, load_typefaces);
    }
}

fn load_typefaces(server: Option<Res<AssetServer>>, mut faces: ResMut<Typefaces>) {
    if let Some(server) = server {
        *faces = Typefaces {
            display: server.load(DISPLAY_FONT),
            body: server.load(BODY_FONT),
            italic: server.load(ITALIC_FONT),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_typefaces_ship_with_their_licences() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        for font in [DISPLAY_FONT, BODY_FONT, ITALIC_FONT] {
            let path = root.join(font);
            assert!(path.is_file(), "{font}");
            assert!(path.with_file_name("OFL.txt").is_file(), "{font} licence");
        }
    }
}
