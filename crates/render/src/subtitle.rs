//! Subtítulos de película para el diálogo de la historia: una línea a la vez,
//! centrada cerca del borde inferior de la pantalla, con el nombre de quien
//! habla en color delante ("Don Hidalgo: ...") y el texto en blanco, los dos
//! con una sombra oscura para leerse sobre el cielo y el pasto.
//!
//! El juego fija la línea mediante [`Subtitle`]; `None` la oculta. Cuánto
//! queda en pantalla cada línea lo da [`duration`], según su largo.

use bevy::prelude::*;

use crate::typeface::Typefaces;

/// Tamaño de fuente de un subtítulo, en píxeles lógicos.
pub const SUBTITLE_PX: f32 = 26.0;
/// Distancia del subtítulo al borde inferior, en porcentaje de la altura.
const BOTTOM: f32 = 9.0;
/// Ancho máximo del subtítulo, en porcentaje del ancho de la pantalla.
const MAX_WIDTH: f32 = 70.0;
/// Color del nombre de quien habla: un ocre cálido, como en los westerns.
const SPEAKER: Color = Color::srgb(0.96, 0.78, 0.42);
/// Color del texto.
const TEXT: Color = Color::srgb(0.97, 0.96, 0.92);
/// La sombra de las letras.
const SHADOW: TextShadow = TextShadow {
    offset: Vec2::new(2.0, 2.0),
    color: Color::srgba(0.0, 0.0, 0.0, 0.85),
};

/// Caracteres por segundo que se le da a leer una línea.
const READING_RATE: f32 = 15.0;
/// Lo mínimo que queda en pantalla una línea, en segundos.
pub const MIN_SECONDS: f32 = 3.0;
/// Lo máximo que queda en pantalla una línea, en segundos.
pub const MAX_SECONDS: f32 = 9.0;

/// Una línea de diálogo: quién la dice y qué dice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleLine {
    pub speaker: String,
    pub text: String,
}

/// El subtítulo en pantalla; `None` lo oculta.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct Subtitle(pub Option<SubtitleLine>);

/// El nodo de texto del subtítulo: su raíz es el nombre de quien habla.
#[derive(Component, Debug)]
pub struct SubtitleSpeaker;

/// El tramo de texto del subtítulo que lleva la línea.
#[derive(Component, Debug)]
pub struct SubtitleText;

/// Segundos que queda en pantalla una línea de `text`: dos segundos más el tiempo
/// de leerla a [`READING_RATE`], entre [`MIN_SECONDS`] y [`MAX_SECONDS`].
pub fn duration(text: &str) -> f32 {
    let chars = text.chars().count() as f32;
    (2.0 + chars / READING_RATE).clamp(MIN_SECONDS, MAX_SECONDS)
}

/// Muestra el subtítulo de [`Subtitle`].
pub struct SubtitlePlugin;

impl Plugin for SubtitlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Typefaces>()
            .init_resource::<Subtitle>()
            .add_systems(Startup, spawn_subtitle)
            .add_systems(PostUpdate, update_subtitle);
    }
}

fn spawn_subtitle(mut commands: Commands, faces: Res<Typefaces>) {
    let font = |color: Color| {
        (
            TextFont {
                font: (&faces.body).into(),
                font_size: FontSize::Px(SUBTITLE_PX),
                ..default()
            },
            TextColor(color),
        )
    };
    commands
        .spawn((
            Name::new("Subtitle"),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                bottom: Val::Percent(BOTTOM),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                SubtitleSpeaker,
                Text::default(),
                font(SPEAKER),
                SHADOW,
                TextLayout::default().with_justify(Justify::Center),
                Node {
                    display: Display::None,
                    max_width: Val::Percent(MAX_WIDTH),
                    ..default()
                },
            ))
            .with_children(|line| {
                line.spawn((SubtitleText, TextSpan::default(), font(TEXT)));
            });
        });
}

/// Reescribe el subtítulo cuando cambia, y lo oculta sin línea.
fn update_subtitle(
    subtitle: Res<Subtitle>,
    mut speakers: Query<(&mut Text, &mut Node), With<SubtitleSpeaker>>,
    mut texts: Query<&mut TextSpan, With<SubtitleText>>,
) {
    if !subtitle.is_changed() {
        return;
    }
    let (speaker, text) = match &subtitle.0 {
        Some(line) => (format!("{}: ", line.speaker), line.text.clone()),
        None => (String::new(), String::new()),
    };
    for (mut root, mut node) in &mut speakers {
        node.display = if subtitle.0.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        root.0.clone_from(&speaker);
    }
    for mut span in &mut texts {
        span.0.clone_from(&text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::uuid_handle;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Typefaces {
                display: uuid_handle!("00000000-0000-0000-0000-000000000000"),
                body: uuid_handle!("00000000-0000-0000-0000-000000000001"),
                italic: uuid_handle!("00000000-0000-0000-0000-000000000002"),
            })
            .add_plugins(SubtitlePlugin);
        app
    }

    /// Lo que se ve: el nombre, el texto y si el subtítulo se muestra.
    fn shown(app: &mut App) -> (String, String, Display) {
        let mut q = app
            .world_mut()
            .query_filtered::<(&Text, &Node), With<SubtitleSpeaker>>();
        let (speaker, node) = q.single(app.world()).unwrap();
        let (speaker, display) = (speaker.0.clone(), node.display);
        let mut q = app
            .world_mut()
            .query_filtered::<&TextSpan, With<SubtitleText>>();
        let text = q.single(app.world()).unwrap().0.clone();
        (speaker, text, display)
    }

    #[test]
    fn a_line_shows_its_speaker_and_text_at_the_bottom_until_cleared() {
        let mut app = app();
        app.update();
        assert_eq!(shown(&mut app).2, Display::None, "hidden without a line");

        app.world_mut().resource_mut::<Subtitle>().0 = Some(SubtitleLine {
            speaker: "Farach".into(),
            text: "¿y vos quien sos?".into(),
        });
        app.update();
        assert_eq!(
            shown(&mut app),
            ("Farach: ".into(), "¿y vos quien sos?".into(), Display::Flex)
        );
        let mut q = app
            .world_mut()
            .query_filtered::<(&TextColor, &TextShadow), With<SubtitleSpeaker>>();
        let (color, shadow) = q.single(app.world()).unwrap();
        assert_eq!(color.0, SPEAKER);
        assert!(
            shadow.color.alpha() > 0.5,
            "a dark shadow behind the letters"
        );
        let mut q = app
            .world_mut()
            .query_filtered::<&Node, (Without<SubtitleSpeaker>, With<Children>)>();
        let root = q
            .iter(app.world())
            .find(|n| n.bottom == Val::Percent(BOTTOM))
            .expect("the subtitle sits near the bottom edge");
        assert_eq!(root.justify_content, JustifyContent::Center);

        app.world_mut().resource_mut::<Subtitle>().0 = None;
        app.update();
        assert_eq!(shown(&mut app).2, Display::None, "hidden once cleared");
    }

    #[test]
    fn a_line_stays_long_enough_to_read_within_bounds() {
        assert_eq!(duration("¡Sí!"), MIN_SECONDS);
        let short = "Le agradezco mucho señor.";
        let long =
            "Lisandro Benjamin Andrada, nacido en General Pico, desempleado, zoncera, zoncera…";
        assert!(duration(long) > duration(short));
        assert!((duration(short) - (2.0 + 25.0 / READING_RATE)).abs() < 1e-5);
        assert_eq!(duration(&"a".repeat(400)), MAX_SECONDS);
    }
}
