//! A brief fade to black and back over the whole screen, to cover a jump in
//! time (skipping a train ride, waiting for the train).
//!
//! The game starts it ([`ScreenFade::start`]) and ticks it
//! ([`ScreenFade::tick`]), which says on which frame the screen is fully
//! black: the moment to jump. Render only draws it, as a black UI layer over
//! everything.

use bevy::prelude::*;

/// The fade's state: `None` while the screen is clear.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct ScreenFade(Option<f32>);

impl ScreenFade {
    /// Seconds to fade to black.
    pub const OUT: f32 = 0.35;
    /// Seconds black before fading back.
    pub const HOLD: f32 = 0.15;
    /// Seconds to fade back in.
    pub const IN: f32 = 0.35;

    /// Starts a fade; `false` (and nothing changes) while one is running.
    pub fn start(&mut self) -> bool {
        if self.0.is_some() {
            return false;
        }
        self.0 = Some(0.0);
        true
    }

    /// Whether a fade is running.
    pub fn active(&self) -> bool {
        self.0.is_some()
    }

    /// Runs the fade for `dt` seconds; `true` on the one tick that reaches
    /// full black. It clears once faded back in.
    pub fn tick(&mut self, dt: f32) -> bool {
        let Some(t) = self.0 else {
            return false;
        };
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let next = t + dt;
        self.0 = (next < Self::OUT + Self::HOLD + Self::IN).then_some(next);
        t < Self::OUT && next >= Self::OUT
    }

    /// Opacity of the black layer, 0 (clear) to 1 (black).
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

/// The black layer over the screen.
#[derive(Component, Debug)]
pub struct FadeLayer;

/// Draws the [`ScreenFade`].
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
        // Over the HUD and the minimap.
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
        // A long frame still goes black once, then clears.
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
