//! Las tipografías del juego, cargadas una sola vez para toda UI que escriba
//! texto.
//!
//! - Títulos: Rye (Sorkin Type Co, SIL OFL 1.1), la letra de madera de un
//!   cartel de 1919, para los encabezados ("SE BUSCA", "Pampa Salvaje").
//! - Cuerpo: IM Fell English (Igino Marini, SIL OFL 1.1), impresión de libro
//!   antiguo, en redonda y cursiva, para todo lo demás.
//!
//! Las fuentes están en `assets/fonts/`, cada una con su licencia (`OFL.txt`);
//! ver `CREDITS.md`. Sin un servidor de assets (tests headless) los handles
//! quedan por defecto y no se carga nada.

use bevy::prelude::*;

/// Rye, relativa a la raíz de assets.
pub const DISPLAY_FONT: &str = "fonts/rye/Rye-Regular.ttf";
/// IM Fell English redonda, relativa a la raíz de assets.
pub const BODY_FONT: &str = "fonts/im_fell_english/IMFeENrm28P.ttf";
/// IM Fell English cursiva, relativa a la raíz de assets.
pub const ITALIC_FONT: &str = "fonts/im_fell_english/IMFeENit28P.ttf";

/// Handles de las tipografías del juego.
#[derive(Resource, Debug, Clone, Default)]
pub struct Typefaces {
    pub display: Handle<Font>,
    pub body: Handle<Font>,
    pub italic: Handle<Font>,
}

/// Carga [`Typefaces`] antes de `Startup`, para que toda UI creada allí pueda
/// usarlas.
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
