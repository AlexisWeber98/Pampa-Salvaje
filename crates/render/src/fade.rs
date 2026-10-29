//! Un breve fundido a negro y de vuelta sobre toda la pantalla, para tapar un
//! salto en el tiempo (omitir un viaje en tren, esperar el tren).
//!
//! El juego lo inicia ([`ScreenFade::start`]) y lo hace avanzar
//! ([`ScreenFade::tick`]), que indica en qué frame la pantalla está
//! completamente negra: el momento de saltar. Render solo lo dibuja, como una
//! capa de UI negra por encima de todo.

use bevy::prelude::*;

/// El estado del fundido: `None` mientras la pantalla está despejada.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct ScreenFade(Option<f32>);

impl ScreenFade {
    /// Segundos para fundir a negro.
    pub const OUT: f32 = 0.35;
    /// Segundos en negro antes de volver.
    pub const HOLD: f32 = 0.15;
    /// Segundos para volver a la imagen.
    pub const IN: f32 = 0.35;

    /// Inicia un fundido; `false` (y no cambia nada) mientras hay uno en curso.
    pub fn start(&mut self) -> bool {
        if self.0.is_some() {
            return false;
        }
        self.0 = Some(0.0);
        true
    }

    /// Si hay un fundido en curso.
    pub fn active(&self) -> bool {
        self.0.is_some()
    }

    /// Hace avanzar el fundido `dt` segundos; `true` en el único tick que llega
    /// al negro total. Se borra una vez que vuelve la imagen.
    pub fn tick(&mut self, dt: f32) -> bool {
        let Some(t) = self.0 else {
            return false;
        };
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let next = t + dt;
        self.0 = (next < Self::OUT + Self::HOLD + Self::IN).then_some(next);
        t < Self::OUT && next >= Self::OUT
    }

    /// Opacidad de la capa negra, de 0 (despejada) a 1 (negra).
    pub fn alpha(&self) -> f32 {
        let Some(t) = self.0 else {
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

/// La capa negra sobre la pantalla.
#[derive(Component, Debug)]
pub struct FadeLayer;

/// Dibuja el [`ScreenFade`].
pub struct FadePlugin;

impl Plugin for FadePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScreenFade>()
            .add_systems(Startup, spawn_layer)
            .add_systems(PostUpdate, shade);
    }
}

fn spawn_layer(mut commands: Commands) {
    commands.spawn((
        Name::new("Screen fade"),
        FadeLayer,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
        // Por encima del HUD y del minimapa.
        GlobalZIndex(100),
    ));
}

fn shade(fade: Res<ScreenFade>, mut layers: Query<&mut BackgroundColor, With<FadeLayer>>) {
    for mut color in &mut layers {
        let wanted = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, fade.alpha()));
        color.set_if_neq(wanted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fades_out_holds_black_once_and_fades_back_in() {
        let mut fade = ScreenFade::default();
        assert_eq!(fade.alpha(), 0.0);
        assert!(!fade.tick(0.1), "nothing running");
        assert!(fade.start());
        assert!(!fade.start(), "one at a time");
        let mut black = 0;
        let mut alphas = Vec::new();
        for _ in 0..40 {
            if fade.tick(0.05) {
                black += 1;
                assert_eq!(fade.alpha(), 1.0);
            }
            alphas.push(fade.alpha());
        }
        assert_eq!(black, 1, "black exactly once");
        assert!(!fade.active());
        assert_eq!(fade.alpha(), 0.0);
        let peak = alphas.iter().position(|&a| a == 1.0).unwrap();
        assert!(alphas[..peak].windows(2).all(|w| w[0] <= w[1]));
        // Un frame largo igual pasa a negro una vez y luego se despeja.
        assert!(fade.start());
        assert!(fade.tick(1.0));
        assert!(!fade.active());
    }

    #[test]
    fn the_layer_follows_the_fade() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(FadePlugin);
        app.update();
        let alpha = |app: &mut App| {
            let mut q = app
                .world_mut()
                .query_filtered::<&BackgroundColor, With<FadeLayer>>();
            q.single(app.world()).unwrap().0.alpha()
        };
        assert_eq!(alpha(&mut app), 0.0);
        let mut fade = ScreenFade::default();
        fade.start();
        fade.tick(ScreenFade::OUT);
        app.insert_resource(fade);
        app.update();
        assert_eq!(alpha(&mut app), 1.0);
    }
}
