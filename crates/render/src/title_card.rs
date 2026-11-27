//! Un cartel sobre toda la pantalla: fundido a negro, una línea de texto que
//! queda sobre el negro y la vuelta a la imagen. Para "Te agarraron", cuando
//! la milicada atrapa al jugador en el acto 6.
//!
//! Como [`crate::ScreenFade`], el juego lo inicia ([`TitleCard::start`]) y lo
//! hace avanzar ([`TitleCard::tick`]), que indica el único frame en que la
//! pantalla queda negra: el momento de rehacer lo que hay detrás. Render solo
//! lo dibuja.

use bevy::prelude::*;

use crate::typeface::Typefaces;

/// El cartel en curso: `None` mientras la pantalla está despejada.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct TitleCard(Option<(String, f32)>);

impl TitleCard {
    /// Segundos para fundir a negro.
    pub const OUT: f32 = 0.8;
    /// Segundos que queda el texto sobre el negro.
    pub const HOLD: f32 = 3.0;
    /// Segundos para volver a la imagen.
    pub const IN: f32 = 0.8;

    /// Inicia un cartel con `text`; `false` (y no cambia nada) mientras hay
    /// uno en curso.
    pub fn start(&mut self, text: impl Into<String>) -> bool {
        if self.0.is_some() {
            return false;
        }
        self.0 = Some((text.into(), 0.0));
        true
    }

    /// Si hay un cartel en curso.
    pub fn active(&self) -> bool {
        self.0.is_some()
    }

    /// El texto del cartel en curso.
    pub fn text(&self) -> Option<&str> {
        self.0.as_ref().map(|(text, _)| text.as_str())
    }

    /// Hace avanzar el cartel `dt` segundos; `true` en el único tick que llega
    /// al negro total. Se borra una vez que vuelve la imagen.
    pub fn tick(&mut self, dt: f32) -> bool {
        let Some((_, t)) = self.0.as_mut() else {
            return false;
        };
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let (before, next) = (*t, *t + dt);
        *t = next;
        if next >= Self::OUT + Self::HOLD + Self::IN {
            self.0 = None;
        }
        before < Self::OUT && next >= Self::OUT
    }

    /// Opacidad de la capa negra y de su texto, de 0 (despejada) a 1.
    pub fn alpha(&self) -> f32 {
        let Some((_, t)) = self.0 else {
            return 0.0;
        };
        if t < Self::OUT {
            t / Self::OUT
        } else if t < Self::OUT + Self::HOLD {
            1.0
        } else {
            (1.0 - (t - Self::OUT - Self::HOLD) / Self::IN).clamp(0.0, 1.0)
        }
    }
}

/// La capa negra del cartel.
#[derive(Component, Debug)]
pub struct TitleCardLayer;

/// El texto del cartel.
#[derive(Component, Debug)]
pub struct TitleCardText;

/// Color del texto: un blanco cálido, apagado.
const INK: Color = Color::srgb(0.93, 0.89, 0.80);

/// Dibuja el [`TitleCard`].
pub struct TitleCardPlugin;

impl Plugin for TitleCardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Typefaces>()
            .init_resource::<TitleCard>()
            .add_systems(Startup, spawn_card)
            .add_systems(PostUpdate, show_card);
    }
}

fn spawn_card(mut commands: Commands, faces: Res<Typefaces>) {
    commands
        .spawn((
            Name::new("Title card"),
            TitleCardLayer,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::NONE),
            // Por encima del HUD, del minimapa y del fundido; debajo del menú.
            GlobalZIndex(150),
        ))
        .with_children(|layer| {
            layer.spawn((
                TitleCardText,
                Text::default(),
                TextFont {
                    font: (&faces.display).into(),
                    font_size: FontSize::Vh(9.0),
                    ..default()
                },
                TextColor(INK.with_alpha(0.0)),
            ));
        });
}

fn show_card(
    card: Res<TitleCard>,
    mut layers: Query<(&mut Node, &mut BackgroundColor), With<TitleCardLayer>>,
    mut texts: Query<(&mut Text, &mut TextColor), With<TitleCardText>>,
) {
    let alpha = card.alpha();
    let display = if card.active() {
        Display::Flex
    } else {
        Display::None
    };
    for (mut node, mut color) in &mut layers {
        if node.display != display {
            node.display = display;
        }
        color.set_if_neq(BackgroundColor(Color::srgba(0.0, 0.0, 0.0, alpha)));
    }
    let text = card.text().unwrap_or_default();
    for (mut line, mut color) in &mut texts {
        if line.0 != text {
            line.0 = text.to_owned();
        }
        color.set_if_neq(TextColor(INK.with_alpha(alpha)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fades_to_black_holds_the_text_and_clears() {
        let mut card = TitleCard::default();
        assert!(!card.tick(0.1), "nothing running");
        assert!(card.start("Te agarraron"));
        assert!(!card.start("otra"), "one at a time");
        assert_eq!(card.text(), Some("Te agarraron"));
        let (mut black, mut held) = (0, 0.0);
        while card.active() {
            if card.tick(0.05) {
                black += 1;
                assert_eq!(card.alpha(), 1.0);
            }
            if card.alpha() == 1.0 {
                held += 0.05;
            }
        }
        assert_eq!(black, 1, "black exactly once");
        assert!(held >= 2.9, "the text is held about three seconds: {held}");
        assert_eq!((card.alpha(), card.text()), (0.0, None));
    }

    #[test]
    fn the_layer_shows_the_text_on_black_and_hides_when_done() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(TitleCardPlugin);
        app.update();
        let seen = |app: &mut App| {
            let mut layers = app
                .world_mut()
                .query_filtered::<(&Node, &BackgroundColor), With<TitleCardLayer>>();
            let (node, back) = layers.single(app.world()).unwrap();
            let (display, back) = (node.display, back.0.alpha());
            let mut texts = app
                .world_mut()
                .query_filtered::<(&Text, &TextColor), With<TitleCardText>>();
            let (text, color) = texts.single(app.world()).unwrap();
            (display, back, text.0.clone(), color.0.alpha())
        };
        assert_eq!(seen(&mut app).0, Display::None);
        let mut card = TitleCard::default();
        card.start("Te agarraron");
        card.tick(TitleCard::OUT);
        app.insert_resource(card);
        app.update();
        assert_eq!(
            seen(&mut app),
            (Display::Flex, 1.0, "Te agarraron".to_owned(), 1.0)
        );
        app.insert_resource(TitleCard::default());
        app.update();
        assert_eq!(seen(&mut app).0, Display::None);
    }
}
