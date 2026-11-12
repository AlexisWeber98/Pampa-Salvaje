//! El cuero de los paneles de mano (el inventario y la lista del pulpero o el
//! almacenero): una tapa de cuero marrón curtido ("Brown Leather", Poly Haven,
//! CC0) con su costura clara a un dedo del borde, y la letra estampada encima:
//! crema con sombra para el texto, dorada para los títulos.

use bevy::prelude::*;

/// La textura del cuero, bajo `assets/`.
pub const LEATHER_TEXTURE: &str = "textures/ui/leather/brown_leather_albedo_1k.jpg";

/// El cuero cuando la textura todavía no cargó (o no hay `AssetServer`): su
/// tono medio, y el del borde.
const LEATHER: Color = Color::srgb(0.30, 0.17, 0.08);
const LEATHER_EDGE: Color = Color::srgb(0.10, 0.06, 0.03);
/// El hilo de la costura.
const STITCH: Color = Color::srgba(0.86, 0.74, 0.52, 0.75);
/// La textura se oscurece un poco para que la letra se lea.
const TANNED: Color = Color::srgb(0.78, 0.74, 0.70);

/// La letra sobre el cuero: crema, y dorada en los títulos.
pub const CREAM: Color = Color::srgb(0.95, 0.89, 0.76);
pub const GOLD: Color = Color::srgb(0.88, 0.68, 0.34);
/// Lo que no se puede elegir ahora: la letra crema, apagada.
pub const FADED: Color = Color::srgba(0.95, 0.89, 0.76, 0.42);
/// La sombra que hunde la letra en el cuero.
pub const PRESSED: TextShadow = TextShadow {
    offset: Vec2::new(1.5, 1.5),
    color: Color::srgba(0.0, 0.0, 0.0, 0.8),
};

/// La textura del cuero; sin `AssetServer` queda vacía y se ve el tono plano.
#[derive(Resource, Debug, Clone, Default)]
pub struct Leather(pub Handle<Image>);

/// Carga la textura del cuero, si hay con qué.
pub fn load_leather(server: Option<Res<AssetServer>>, mut leather: ResMut<Leather>) {
    if let Some(server) = server
        && leather.0 == Handle::default()
    {
        leather.0 = server.load(LEATHER_TEXTURE);
    }
}

/// La tapa de cuero: el fondo de un panel, con su borde oscuro y su sombra.
/// Lleva adentro la [`stitched`] hoja.
pub fn cover(leather: &Leather) -> impl Bundle {
    (
        Node {
            border: UiRect::all(Val::Px(2.0)),
            padding: UiRect::all(Val::Px(8.0)),
            ..default()
        },
        BackgroundColor(LEATHER),
        ImageNode::new(leather.0.clone())
            .with_mode(NodeImageMode::Stretch)
            .with_color(TANNED),
        BorderColor::all(LEATHER_EDGE),
        BoxShadow::new(
            Color::srgba(0.0, 0.0, 0.0, 0.6),
            Val::Px(6.0),
            Val::Px(8.0),
            Val::Px(0.0),
            Val::Px(14.0),
        ),
    )
}

/// El marco de la costura, dentro de la tapa, donde va lo escrito.
pub fn stitched(node: Node) -> impl Bundle {
    (
        Node {
            border: UiRect::all(Val::Px(1.0)),
            ..node
        },
        BorderColor::all(STITCH),
    )
}

/// La letra de `face` de `px` píxeles en `color`, hundida en el cuero.
pub fn stamped(face: &Handle<Font>, px: f32, color: Color) -> impl Bundle {
    (
        TextFont {
            font: face.into(),
            font_size: FontSize::Px(px),
            ..default()
        },
        TextColor(color),
        PRESSED,
    )
}
