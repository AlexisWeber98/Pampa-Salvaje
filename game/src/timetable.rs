//! The timetable board at each station: stand near the painted board on the
//! station wall, facing it, and F opens a panel with every train that stops
//! there ([`pampa_core::station_board`]); F again, or walking away, closes
//! it.
//!
//! F is the one context key ([`INTERACT_KEY`]), shared with the errand, the
//! horse and the train. The board goes by what is faced: it takes F when the
//! player stands in reach looking at it, ahead of boarding or waiting for a
//! train on the platform (the player looks at the wall, not the train), but
//! not when the errand's person or a horse is in reach, which keep F. The
//! board system runs first and eats the press when it acts.
//!
//! The panel and the "F: horarios" prompt are lettered in the HUD's bitmap
//! font on its paper panels ([`pampa_render::hud`]).

use bevy::prelude::*;
use pampa_core::places::side_normal;
use pampa_core::{
    Ability, BoardRow, Bound, Errand, Interaction, Service, TownLayout, Travel, Vec2 as GroundVec2,
    station_board,
};
use pampa_render::hud::{panel_colors, text_image};
use pampa_render::{ScreenFade, WorldTowns, WorldTrains};

use crate::abilities::PlayerAbilities;
use crate::errand::{ErrandRun, INTERACT_KEY, LooseHorse, Mountable};
use crate::player::Player;
use crate::ride::OnTrain;

/// How near the board the player must stand to read it, in meters.
pub const BOARD_RANGE: f32 = 3.5;
/// How far off the panel stays open, in meters.
pub const KEEP_RANGE: f32 = 5.5;
/// How squarely the player must face the board: the cosine of the widest
/// angle between where they look and the board (about 50 degrees).
pub const FACING: f32 = 0.64;

/// Pixel scale of the panel's lettering.
const SCALE: usize = 2;

/// The boards the player stands at, and the panel.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Timetables {
    /// The town whose board is in reach and faced, if any.
    pub near: Option<usize>,
    /// The town whose panel is open, if any.
    pub open: Option<usize>,
    /// The panel's lines as drawn (empty while closed).
    pub lines: Vec<String>,
}

#[derive(Component)]
struct BoardPanel;

#[derive(Component)]
struct BoardPrompt;

pub struct TimetablePlugin;

impl Plugin for TimetablePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Timetables>()
            .add_systems(Startup, spawn_ui)
            .add_systems(Update, read_board.before(crate::ride::ride_keys))
            .add_systems(PostUpdate, draw_ui);
    }
}

/// The town of the board the person at `at` facing `heading` reads, if any:
/// within [`BOARD_RANGE`] of it, on the side it faces, looking at it.
pub fn board_in_reach(
    towns: &[TownLayout],
    at: GroundVec2,
    heading: pampa_core::Heading,
) -> Option<usize> {
    towns
        .iter()
        .enumerate()
        .filter_map(|(i, town)| {
            // The nearest of the station's three boards the person reads.
            town.timetable_boards()
                .into_iter()
                .filter_map(|sign| {
                    let off = at - sign.anchor;
                    let (reach, front) = (off.length(), side_normal(sign.facing));
                    let reads = reach <= BOARD_RANGE
                        && off.dot(front) > 0.0
                        && heading.forward().dot(off) <= -FACING * reach;
                    reads.then_some((i, reach))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Whether the errand has somebody or something to do at `at` on F.
fn errand_wants_f(errand: &Errand, at: GroundVec2) -> bool {
    errand.clone().interact(at) != Interaction::Nothing
}

/// One row of the board as the panel letters it, in Spanish.
pub fn row_text(row: &BoardRow) -> String {
    let service = match row.service {
        Service::Line => "Tren",
        Service::Shuttle => "Ramal",
    };
    let bound = match &row.bound {
        Bound::Towards(to) => format!("a {to}"),
        Bound::From(from) => format!("de {from}"),
    };
    let stops = match (row.arrive, row.depart) {
        (Some(arrive), Some(depart)) => format!("Llega {arrive}  Sale {depart}"),
        (None, Some(depart)) => format!("Sale {depart}"),
        (Some(arrive), None) => format!("Llega {arrive}"),
        (None, None) => String::new(),
    };
    format!("{} {service} {bound}  {stops}", row.at())
}

/// The panel's lines for the station of `town`: its title, then a row per
/// stop of every train, earliest first.
pub fn panel_lines(
    trains: &[pampa_core::LineTrain],
    towns: &[TownLayout],
    town: usize,
) -> Vec<String> {
    let name = towns.get(town).map_or("", |t| t.name.as_str());
    let mut lines = vec![format!("Horarios - Estación {name}"), String::new()];
    let rows = station_board(trains, towns, town);
    if rows.is_empty() {
        lines.push("Sin trenes".to_string());
    }
    lines.extend(rows.iter().map(row_text));
    lines
}

/// Works out the board in reach, and opens or closes the panel on F.
#[allow(clippy::too_many_arguments)]
fn read_board(
    keys: Option<ResMut<ButtonInput<KeyCode>>>,
    towns: Option<Res<WorldTowns>>,
    trains: Option<Res<WorldTrains>>,
    errand: Option<Res<ErrandRun>>,
    fade: Option<Res<ScreenFade>>,
    (loose, abilities): (Option<Res<LooseHorse>>, Option<Res<PlayerAbilities>>),
    horses: Query<&Mountable>,
    players: Query<(&Player, Option<&OnTrain>)>,
    mut state: ResMut<Timetables>,
) {
    let (Some(towns), Some(trains)) = (towns, trains) else {
        return;
    };
    let Ok((player, aboard)) = players.single() else {
        return;
    };
    let rider = &player.rider;
    let walking =
        aboard.is_none() && rider.travel() == Travel::OnFoot && !fade.is_some_and(|f| f.active());
    let near = walking
        .then(|| board_in_reach(&towns.0, rider.position, rider.heading))
        .flatten();
    state.set_if_neq(Timetables {
        near,
        ..state.clone()
    });
    // Walking away, or onto a horse or a train, closes the panel.
    if let Some(open) = state.open {
        let away = towns
            .0
            .get(open)
            .map(TownLayout::timetable_boards)
            .is_none_or(|boards| {
                !walking
                    || boards
                        .iter()
                        .all(|sign| rider.position.distance(sign.anchor) > KEEP_RANGE)
            });
        if away {
            *state = Timetables {
                open: None,
                lines: Vec::new(),
                ..state.clone()
            };
        }
    }
    let mut keys = keys;
    let pressed = keys.as_ref().is_some_and(|k| k.just_pressed(INTERACT_KEY));
    if !pressed {
        return;
    }
    if state.open.is_some() {
        state.open = None;
        state.lines = Vec::new();
        if let Some(keys) = keys.as_mut() {
            keys.clear_just_pressed(INTERACT_KEY);
        }
        return;
    }
    let Some(town) = near else {
        return;
    };
    // F is for the errand, or a horse, when either is in reach.
    let at = rider.position;
    let horse_in_reach = loose
        .and_then(|l| l.0)
        .is_some_and(|h| at.distance(h.position) <= Errand::MOUNT_RANGE)
        || abilities.is_some_and(|a| a.0.has(Ability::Ride))
            && horses
                .iter()
                .any(|h| at.distance(h.0.position) <= Errand::MOUNT_RANGE);
    if horse_in_reach || errand.is_some_and(|e| errand_wants_f(&e.0, at)) {
        return;
    }
    state.open = Some(town);
    state.lines = panel_lines(&trains.0, &towns.0, town);
    if let Some(keys) = keys.as_mut() {
        keys.clear_just_pressed(INTERACT_KEY);
    }
}

/// The prompt and the panel, hidden, in the HUD's panel style: the prompt
/// at the bottom middle, the panel in the middle of the screen. Each is a
/// full-width wrapper (shown or hidden) around one framed column of lines.
fn spawn_ui(mut commands: Commands) {
    for panel in [false, true] {
        let mut root = Node {
            display: Display::None,
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            ..default()
        };
        if panel {
            root.top = Val::Px(0.0);
            root.height = Val::Percent(100.0);
            root.align_items = AlignItems::Center;
        } else {
            root.bottom = Val::Px(48.0);
        }
        let (border, background) = panel_colors();
        let mut entity = commands.spawn((
            Name::new(if panel { "Board panel" } else { "Board prompt" }),
            root,
        ));
        if panel {
            entity.insert(BoardPanel);
        } else {
            entity.insert(BoardPrompt);
        }
        entity.with_child((
            Node {
                flex_direction: FlexDirection::Column,
                border: UiRect::all(Val::Px(3.0)),
                padding: UiRect::all(Val::Px(8.0)),
                row_gap: Val::Px(4.0),
                ..default()
            },
            border,
            background,
        ));
    }
}

/// The prompt's and the panel's wrapper nodes.
type Roots<'w, 's> = Query<
    'w,
    's,
    (&'static mut Node, &'static Children, Has<BoardPanel>),
    Or<(With<BoardPanel>, With<BoardPrompt>)>,
>;

/// Shows the prompt while a board is in reach and the panel is shut, and
/// the panel while it is open, relettering them when they change.
fn draw_ui(
    mut commands: Commands,
    state: Res<Timetables>,
    images: Option<ResMut<Assets<Image>>>,
    mut roots: Roots,
) {
    let Some(mut images) = images else {
        return;
    };
    if !state.is_changed() {
        return;
    }
    for (mut node, children, panel) in &mut roots {
        let lines: Vec<String> = if panel {
            state.lines.clone()
        } else if state.open.is_none() && state.near.is_some() {
            vec!["F: horarios".to_string()]
        } else {
            Vec::new()
        };
        node.display = if lines.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        let Some(&frame) = children.first() else {
            continue;
        };
        commands.entity(frame).despawn_related::<Children>();
        for line in &lines {
            let image = text_image(line, SCALE);
            let size = image.size_f32();
            commands.entity(frame).with_child((
                ImageNode::new(images.add(image)),
                Node {
                    width: Val::Px(size.x),
                    height: Val::Px(size.y),
                    ..default()
                },
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ride::testing::{press, shipped_app_at};
    use pampa_core::{GameClock, Heading, Npc, TetheredHorse, TimeOfDay};
    use pampa_render::font;

    /// The shipped towns at midday, the player on foot on the first town's
    /// platform.
    fn app() -> App {
        let mut app = shipped_app_at(Some(GameClock::at(TimeOfDay::hm(12, 0))));
        app.init_resource::<Assets<Image>>();
        app.update();
        app
    }

    fn town_named(app: &App, name: &str) -> usize {
        let towns = &app.world().resource::<WorldTowns>().0;
        towns.iter().position(|t| t.name == name).expect(name)
    }

    /// Puts the player `meters` out from the middle of `town`'s board,
    /// looking at it (`toward`) or the other way.
    fn stand_at_board(app: &mut App, town: usize, meters: f32, toward: bool) {
        stand_at_nth_board(app, town, 0, meters, toward);
    }

    /// [`stand_at_board`] for the station's `n`th board (0 on the track
    /// side, 1 and 2 on the street front).
    fn stand_at_nth_board(app: &mut App, town: usize, n: usize, meters: f32, toward: bool) {
        let sign = app.world().resource::<WorldTowns>().0[town].timetable_boards()[n];
        let out = side_normal(sign.facing);
        let look = if toward { out * -1.0 } else { out };
        let mut q = app.world_mut().query::<&mut Player>();
        let mut p = q.single_mut(app.world_mut()).unwrap();
        p.rider.position = sign.anchor + out * meters;
        p.rider.heading = Heading::from_radians(look.y.atan2(look.x));
        p.rider.speed = 0.0;
    }

    fn open(app: &App) -> Option<usize> {
        app.world().resource::<Timetables>().open
    }

    /// An hour and a minute.
    type Hm = (u32, u32);

    fn row(service: Service, bound: Bound, at: (Option<Hm>, Option<Hm>)) -> BoardRow {
        let time = |t: Option<Hm>| t.map(|(h, m)| TimeOfDay::hm(h, m));
        BoardRow {
            train: 0,
            service,
            bound,
            arrive: time(at.0),
            depart: time(at.1),
        }
    }

    #[test]
    fn a_row_reads_in_spanish_with_what_it_has() {
        let to = Bound::Towards("Eduardo Castex".into());
        let stop = row(Service::Line, to, (Some((7, 12)), Some((7, 25))));
        assert_eq!(
            row_text(&stop),
            "07:25 Tren a Eduardo Castex  Llega 07:12  Sale 07:25"
        );
        let leaves = row(
            Service::Shuttle,
            Bound::Towards("Caleufú".into()),
            (None, Some((8, 5))),
        );
        assert_eq!(row_text(&leaves), "08:05 Ramal a Caleufú  Sale 08:05");
        let arrives = row(
            Service::Shuttle,
            Bound::From("Caleufú".into()),
            (Some((9, 40)), None),
        );
        assert_eq!(row_text(&arrives), "09:40 Ramal de Caleufú  Llega 09:40");
        // Everything the panel can say, the HUD font can letter.
        for text in [row_text(&stop), row_text(&leaves), row_text(&arrives)] {
            assert!(text.chars().all(font::supports), "{text}");
        }
    }

    #[test]
    fn metileos_panel_lists_the_line_train_and_the_shuttle_from_the_trains() {
        let app = app();
        let metileo = town_named(&app, "Metileo");
        let towns = &app.world().resource::<WorldTowns>().0;
        let trains = &app.world().resource::<WorldTrains>().0;
        let lines = panel_lines(trains, towns, metileo);
        assert!(
            lines.iter().all(|l| l.chars().all(font::supports)),
            "{lines:?}"
        );
        // Taken from the trains' own stop times: the line train's two runs
        // and the shuttle's two round trips, earliest first.
        assert_eq!(
            lines,
            [
                "Horarios - Estación Metileo",
                "",
                "08:41 Ramal de Caleufú  Llega 08:41",
                "09:04 Tren a Eduardo Castex  Llega 08:51  Sale 09:04",
                "09:14 Ramal a Caleufú  Sale 09:14",
                "19:10 Ramal de Caleufú  Llega 19:10",
                "19:33 Tren a General Pico  Llega 19:20  Sale 19:33",
                "19:43 Ramal a Caleufú  Sale 19:43",
            ]
        );
    }

    #[test]
    fn near_the_board_f_opens_the_panel_and_f_again_closes_it() {
        let mut app = app();
        let metileo = town_named(&app, "Metileo");
        stand_at_board(&mut app, metileo, 2.0, true);
        app.update();
        assert_eq!(app.world().resource::<Timetables>().near, Some(metileo));
        assert_eq!(open(&app), None, "the prompt only, until F");

        press(&mut app, INTERACT_KEY);
        assert_eq!(open(&app), Some(metileo));
        let lines = app.world().resource::<Timetables>().lines.clone();
        assert_eq!(lines[0], "Horarios - Estación Metileo");
        // Both trains' rows: the line train's and the shuttle's.
        assert!(lines.iter().any(|l| l.contains(" Tren a ")), "{lines:?}");
        assert!(
            lines.iter().any(|l| l.contains(" Ramal a Caleufú")),
            "{lines:?}"
        );

        press(&mut app, INTERACT_KEY);
        assert_eq!(open(&app), None);
        assert!(app.world().resource::<Timetables>().lines.is_empty());
    }

    #[test]
    fn far_away_or_looking_away_f_opens_nothing() {
        // A fresh game each time: an F out of reach starts waiting for the
        // train, and the screen fades for a while.
        for (meters, toward) in [(8.0, true), (2.0, false)] {
            let mut app = app();
            let metileo = town_named(&app, "Metileo");
            stand_at_board(&mut app, metileo, meters, toward);
            app.update();
            assert_eq!(
                app.world().resource::<Timetables>().near,
                None,
                "{meters} m, facing it: {toward}"
            );
            press(&mut app, INTERACT_KEY);
            assert_eq!(open(&app), None, "{meters} m, facing it: {toward}");
        }
    }

    #[test]
    fn behind_the_wall_nothing_is_read() {
        let app = app();
        let metileo = town_named(&app, "Metileo");
        let towns = &app.world().resource::<WorldTowns>().0;
        let sign = towns[metileo].timetable_boards()[0];
        let out = side_normal(sign.facing);
        let heading = |d: GroundVec2| Heading::from_radians(d.y.atan2(d.x));
        // In front, looking at it: read. The same spot from inside the
        // station, looking at the same board: not.
        let read = |at: GroundVec2, look: GroundVec2| board_in_reach(towns, at, heading(look));
        assert_eq!(read(sign.anchor + out * 2.0, out * -1.0), Some(metileo));
        assert_eq!(read(sign.anchor - out * 2.0, out), None);
    }

    #[test]
    fn f_at_a_street_side_board_opens_the_same_panel() {
        let mut app = app();
        let metileo = town_named(&app, "Metileo");
        let track = {
            stand_at_board(&mut app, metileo, 2.0, true);
            press(&mut app, INTERACT_KEY);
            app.world().resource::<Timetables>().lines.clone()
        };
        assert_eq!(track[0], "Horarios - Estación Metileo");
        for n in [1, 2] {
            // A fresh game: the F above may have left a fade running.
            let mut app = super::tests::app();
            stand_at_nth_board(&mut app, metileo, n, 2.0, true);
            app.update();
            assert_eq!(app.world().resource::<Timetables>().near, Some(metileo));
            press(&mut app, INTERACT_KEY);
            assert_eq!(open(&app), Some(metileo), "street board {n}");
            assert_eq!(app.world().resource::<Timetables>().lines, track);
            press(&mut app, INTERACT_KEY);
            assert_eq!(open(&app), None);
        }
    }

    #[test]
    fn walking_away_closes_the_panel() {
        let mut app = app();
        let metileo = town_named(&app, "Metileo");
        stand_at_board(&mut app, metileo, 2.0, true);
        press(&mut app, INTERACT_KEY);
        assert_eq!(open(&app), Some(metileo));
        // Turning in place does not close it; walking off does.
        stand_at_board(&mut app, metileo, 2.0, false);
        app.update();
        assert_eq!(open(&app), Some(metileo));
        stand_at_board(&mut app, metileo, 9.0, false);
        app.update();
        assert_eq!(open(&app), None);
    }

    #[test]
    fn f_is_left_to_a_horse_in_reach_and_to_the_errand() {
        let mut app = app();
        let metileo = town_named(&app, "Metileo");
        stand_at_board(&mut app, metileo, 2.0, true);
        // A horse left beside the player: F is the horse's, not the board's.
        let beside = {
            let mut q = app.world_mut().query::<&Player>();
            q.single(app.world()).unwrap().rider.position + GroundVec2::new(1.5, 0.0)
        };
        app.world_mut().resource_mut::<LooseHorse>().0 =
            Some(TetheredHorse::standing(beside, Heading::from_radians(0.0)));
        app.update();
        press(&mut app, INTERACT_KEY);
        assert_eq!(open(&app), None);

        // The errand's person in reach wants F too, and the board is not
        // anywhere near him.
        let errand = app.world().resource::<ErrandRun>().0.clone();
        let hidalgo = errand.standing(Npc::Hidalgo).position;
        assert!(errand_wants_f(&errand, hidalgo));
        let towns = &app.world().resource::<WorldTowns>().0;
        let sign = towns[metileo].timetable_boards()[0];
        assert!(!errand_wants_f(&errand, sign.anchor));
    }

    #[test]
    fn f_goes_to_the_errand_when_its_person_stands_at_the_board() {
        let mut app = app();
        let errand = app.world().resource::<ErrandRun>().0.clone();
        let hidalgo = errand.standing(Npc::Hidalgo).position;
        // Move the first town's station so that Don Hidalgo stands right
        // in front of its track-side board.
        let (town, out) = {
            let mut towns = app.world_mut().resource_mut::<WorldTowns>();
            let town = &mut towns.0[0];
            let sign = town.timetable_boards()[0];
            let out = side_normal(sign.facing);
            let delta = hidalgo - out * 1.5 - sign.anchor;
            let station = town
                .buildings
                .iter_mut()
                .find(|b| b.kind == pampa_core::BuildingKind::Station)
                .unwrap();
            station.footprint.min += delta;
            station.footprint.max += delta;
            (0, out)
        };
        let mut q = app.world_mut().query::<&mut Player>();
        {
            let mut p = q.single_mut(app.world_mut()).unwrap();
            p.rider.position = hidalgo;
            p.rider.heading = Heading::from_radians((out * -1.0).y.atan2((out * -1.0).x));
            p.rider.speed = 0.0;
        }
        app.update();
        assert_eq!(app.world().resource::<Timetables>().near, Some(town));
        let before = errand.stage();
        press(&mut app, INTERACT_KEY);
        assert_eq!(open(&app), None, "the errand has F");
        assert_ne!(app.world().resource::<ErrandRun>().0.stage(), before);
    }

    #[test]
    fn the_panel_and_prompt_are_lettered_when_shown() {
        let mut app = app();
        let metileo = town_named(&app, "Metileo");
        let shown = |app: &mut App| -> (Vec<Display>, usize) {
            let mut q = app.world_mut().query_filtered::<
                (&Node, &Children, Has<BoardPanel>),
                Or<(With<BoardPanel>, With<BoardPrompt>)>,
            >();
            let roots: Vec<(Display, usize, bool)> = q
                .iter(app.world())
                .map(|(n, c, panel)| (n.display, c.len(), panel))
                .collect();
            let frames: Vec<Entity> = {
                let mut q = app
                    .world_mut()
                    .query_filtered::<&Children, With<BoardPanel>>();
                q.iter(app.world()).flat_map(|c| c.iter()).collect()
            };
            let lines = frames
                .iter()
                .map(|f| app.world().get::<Children>(*f).map_or(0, |c| c.len()))
                .sum();
            (roots.iter().map(|r| r.0).collect(), lines)
        };
        let (display, lines) = shown(&mut app);
        assert_eq!(lines, 0);
        assert!(display.iter().all(|d| *d == Display::None));
        stand_at_board(&mut app, metileo, 2.0, true);
        press(&mut app, INTERACT_KEY);
        app.update();
        let (display, lines) = shown(&mut app);
        // The panel is shown and the prompt (shown only before F) is not.
        assert_eq!(display.iter().filter(|d| **d == Display::Flex).count(), 1);
        assert_eq!(lines, app.world().resource::<Timetables>().lines.len());
        assert!(lines > 3);
    }
}
