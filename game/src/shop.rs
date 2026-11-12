//! Comprarles al pulpero y al almacenero ([`pampa_core::shop`]).
//!
//! Adentro del local, frente al mostrador (a [`SHOP_RANGE`] metros o menos de
//! quien atiende, que está del otro lado), se rotula "F: hablar con el
//! pulpero" (o "con el almacenero"). F lo saluda: si el protagonista todavía
//! no sabe comerciar ([`Ability::Trade`]), contesta [`NOT_YET`] y nada más; si
//! sabe, abre la lista de lo que vende ([`ShopView`]) y el protagonista queda
//! quieto en el mostrador ([`ControlLock`]). W y S (o las flechas) eligen, F
//! compra una de lo elegido con la plata del HUD y la guarda en el inventario,
//! y Esc cierra la lista sin abrir el menú. Quien atiende contesta cada compra
//! como subtítulo.
//!
//! F es la tecla de contexto compartida ([`INTERACT_KEY`]): hablar la toma solo
//! cuando ni la persona del encargo ni un caballo al alcance la reclaman
//! ([`FClaims`]), y consume la pulsación.

use bevy::input::InputSystems;
use bevy::prelude::*;
use pampa_core::shop::{NOT_YET, Refusal, Shop};
use pampa_core::{Ability, Travel, Vec2 as GroundVec2};
use pampa_render::hud::{LINE_PX, ink_font, panel_colors};
use pampa_render::subtitle::duration;
use pampa_render::{ShopContents, ShopRow, ShopView, Subtitle, SubtitleLine, Typefaces};

use crate::abilities::PlayerAbilities;
use crate::errand::{FClaims, INTERACT_KEY};
use crate::menu::MenuKeys;
use crate::player::{ControlLock, Player};
use crate::pulperia::{ALMACENERO, PULPERO, PulperiaDoor, PulperiaDoors};
use crate::ride::OnTrain;
use crate::wallet::{PlayerItems, PlayerWallet};

/// Qué tan cerca de quien atiende se le habla, en metros: el mostrador queda
/// en el medio (quien atiende está a 1,35 m de su canto).
pub const SHOP_RANGE: f32 = 2.5;
/// Lo que dice quien atiende al vender.
pub const SOLD: &str = "Ahí tiene.";
/// Lo que dice cuando no alcanza la plata.
pub const SHORT: &str = "No le alcanza la plata, amigo.";
/// Lo que dice cuando le piden un arma a quien no sabe usarla.
pub const NO_ARMS: &str = "Armas no le vendo a cualquiera.";
/// La tecla que cierra la lista.
pub const LEAVE_KEY: KeyCode = KeyCode::Escape;

/// El aviso de que F habla con quien atiende el local.
pub fn talk_hint(almacen: bool) -> &'static str {
    if almacen {
        "F: hablar con el almacenero"
    } else {
        "F: hablar con el pulpero"
    }
}

/// El mostrador donde el protagonista está comprando, si hay uno.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct ShopOpen(pub Option<Counter>);

/// Un mostrador abierto: el local (por pueblo y edificio, como
/// [`PulperiaDoor`]), lo que vende y el renglón elegido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counter {
    pub town: usize,
    pub building: usize,
    pub shop: Shop,
    pub selected: usize,
}

/// El aviso que se rotula, si hay uno.
#[derive(Resource, Debug, Default, PartialEq, Eq)]
pub struct ShopHint(pub Option<&'static str>);

#[derive(Component, Debug)]
struct ShopPrompt;

/// El subtítulo que está diciendo quien atiende, y por cuánto más.
#[derive(Resource, Debug, Default)]
struct Saying {
    left: f32,
    line: Option<SubtitleLine>,
}

pub struct ShopPlugin;

impl Plugin for ShopPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(pampa_render::ShopPanelPlugin)
            .init_resource::<ShopOpen>()
            .init_resource::<ShopHint>()
            .init_resource::<ShopView>()
            .init_resource::<Saying>()
            .init_resource::<Subtitle>()
            .init_resource::<Typefaces>()
            .add_systems(Startup, spawn_hint)
            .add_systems(
                PreUpdate,
                leave.after(InputSystems).before(MenuKeys),
            )
            .add_systems(
                Update,
                (talk, choose, show)
                    .chain()
                    .before(crate::errand::interact)
                    .before(crate::ride::ride_keys),
            )
            .add_systems(Update, hold_still.after(crate::errand::frame_scene))
            .add_systems(PostUpdate, draw_hint);
    }
}

fn shop_of(door: &PulperiaDoor) -> Shop {
    if door.almacen { Shop::Almacen } else { Shop::Pulperia }
}

fn keeper_of(shop: Shop) -> &'static str {
    match shop {
        Shop::Pulperia => PULPERO,
        Shop::Almacen => ALMACENERO,
    }
}

/// El local abierto en cuyo mostrador está parado `at`, adentro y al alcance
/// de quien atiende.
fn counter_at(doors: &PulperiaDoors, at: GroundVec2) -> Option<&PulperiaDoor> {
    doors.0.iter().find(|door| {
        !door.shut
            && door.inside.holds(at)
            && at.distance(door.inside.keeper.position) <= SHOP_RANGE
    })
}

fn say(saying: &mut Saying, subtitle: &mut Subtitle, shop: Shop, text: &str) {
    let line = SubtitleLine {
        speaker: keeper_of(shop).to_string(),
        text: text.to_string(),
    };
    saying.left = duration(text);
    saying.line = Some(line.clone());
    subtitle.0 = Some(line);
}

/// F frente al mostrador: saluda y, si el protagonista sabe comerciar, abre
/// la lista. También borra el subtítulo de quien atiende cuando pasó su tiempo
/// y decide el aviso.
#[allow(clippy::too_many_arguments)]
fn talk(
    time: Res<Time>,
    keys: Option<ResMut<ButtonInput<KeyCode>>>,
    doors: Res<PulperiaDoors>,
    claims: FClaims,
    abilities: Option<Res<PlayerAbilities>>,
    players: Query<(&Player, Has<OnTrain>)>,
    (mut open, mut lock, mut hint): (ResMut<ShopOpen>, ResMut<ControlLock>, ResMut<ShopHint>),
    (mut saying, mut subtitle): (ResMut<Saying>, ResMut<Subtitle>),
) {
    if saying.left > 0.0 {
        saying.left -= time.delta_secs();
        if saying.left <= 0.0 && subtitle.0 == saying.line {
            subtitle.0 = None;
        }
    }
    if open.0.is_some() {
        hint.set_if_neq(ShopHint(None));
        return;
    }
    let door = players.single().ok().and_then(|(player, aboard)| {
        let rider = &player.rider;
        let free = !aboard
            && rider.travel() == Travel::OnFoot
            && !lock.0
            && !claims.taken(rider.position);
        free.then(|| counter_at(&doors, rider.position)).flatten()
    });
    hint.set_if_neq(ShopHint(door.map(|d| talk_hint(d.almacen))));
    let Some(door) = door else {
        return;
    };
    let mut keys = keys;
    let Some(keys) = keys.as_mut().filter(|k| k.just_pressed(INTERACT_KEY)) else {
        return;
    };
    keys.clear_just_pressed(INTERACT_KEY);
    let shop = shop_of(door);
    if !abilities.is_some_and(|a| a.0.has(Ability::Trade)) {
        say(&mut saying, &mut subtitle, shop, NOT_YET);
        return;
    }
    say(&mut saying, &mut subtitle, shop, shop.greeting());
    open.0 = Some(Counter {
        town: door.town,
        building: door.building,
        shop,
        selected: 0,
    });
    lock.0 = true;
}

/// Con la lista abierta: W y S eligen, F compra una de lo elegido.
fn choose(
    keys: Option<ResMut<ButtonInput<KeyCode>>>,
    abilities: Option<Res<PlayerAbilities>>,
    (mut open, mut wallet, mut items): (
        ResMut<ShopOpen>,
        Option<ResMut<PlayerWallet>>,
        Option<ResMut<PlayerItems>>,
    ),
    (mut saying, mut subtitle): (ResMut<Saying>, ResMut<Subtitle>),
) {
    let (Some(counter), Some(mut keys)) = (open.0.as_mut(), keys) else {
        return;
    };
    let wares = counter.shop.wares();
    if keys.any_just_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) {
        counter.selected = counter.selected.checked_sub(1).unwrap_or(wares.len() - 1);
    }
    if keys.any_just_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) {
        counter.selected = (counter.selected + 1) % wares.len();
    }
    if !keys.just_pressed(INTERACT_KEY) {
        return;
    }
    keys.clear_just_pressed(INTERACT_KEY);
    let (Some(wallet), Some(items)) = (wallet.as_mut(), items.as_mut()) else {
        return;
    };
    let abilities = abilities.map(|a| a.0).unwrap_or_default();
    let line = match wares[counter.selected].buy(abilities, &mut wallet.0, &mut items.0) {
        Ok(()) => SOLD,
        Err(Refusal::Short) => SHORT,
        Err(Refusal::Needs(_)) => NO_ARMS,
        Err(Refusal::NoTrade) => NOT_YET,
    };
    let shop = counter.shop;
    say(&mut saying, &mut subtitle, shop, line);
}

/// Con la lista abierta, el protagonista se queda en el mostrador: vuelve a
/// bloquear las teclas después de que el encargo decide el bloqueo de cada
/// frame según su escena ([`crate::errand::frame_scene`]), que sin escena lo
/// suelta, y antes de que se mueva el jugador en el frame siguiente.
fn hold_still(open: Res<ShopOpen>, mut lock: ResMut<ControlLock>) {
    if open.0.is_some() {
        lock.set_if_neq(ControlLock(true));
    }
}

/// Esc con la lista abierta: la cierra, y el menú no se abre.
fn leave(
    keys: Option<ResMut<ButtonInput<KeyCode>>>,
    mut open: ResMut<ShopOpen>,
    mut lock: ResMut<ControlLock>,
) {
    let Some(mut keys) = keys else {
        return;
    };
    if open.0.is_some() && keys.just_pressed(LEAVE_KEY) {
        keys.clear_just_pressed(LEAVE_KEY);
        open.0 = None;
        lock.0 = false;
    }
}

/// Escribe la lista abierta en [`ShopView`] para render.
fn show(
    open: Res<ShopOpen>,
    abilities: Option<Res<PlayerAbilities>>,
    wallet: Option<Res<PlayerWallet>>,
    mut view: ResMut<ShopView>,
) {
    let want = open.0.map(|counter| {
        let money = wallet.as_ref().map(|w| w.0).unwrap_or_default();
        let abilities = abilities.as_ref().map(|a| a.0).unwrap_or_default();
        ShopContents {
            title: match counter.shop {
                Shop::Pulperia => "Pulpería",
                Shop::Almacen => "Almacén de ramos generales",
            }
            .to_string(),
            money: money.to_string(),
            rows: counter
                .shop
                .wares()
                .iter()
                .map(|w| ShopRow {
                    name: w.item.name().to_string(),
                    price: w.price.to_string(),
                    available: w.check(abilities, money).is_ok(),
                })
                .collect(),
            selected: counter.selected,
        }
    });
    if view.0 != want {
        view.0 = want;
    }
}

/// El aviso, oculto hasta que [`ShopHint`] pide uno.
fn spawn_hint(mut commands: Commands, faces: Res<Typefaces>) {
    let (border, background) = panel_colors();
    commands
        .spawn((
            Name::new("Shop prompt"),
            ShopPrompt,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                bottom: Val::Px(48.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_child((
            Node {
                border: UiRect::all(Val::Px(3.0)),
                padding: UiRect::all(Val::Px(8.0)),
                ..default()
            },
            border,
            background,
            children![(Text::new(""), ink_font(&faces.body, LINE_PX))],
        ));
}

/// Muestra, rotula u oculta el aviso cuando cambia [`ShopHint`].
fn draw_hint(
    hint: Res<ShopHint>,
    mut prompts: Query<(&mut Node, &Children), With<ShopPrompt>>,
    frames: Query<&Children>,
    mut texts: Query<&mut Text>,
) {
    if !hint.is_changed() {
        return;
    }
    for (mut node, children) in &mut prompts {
        node.display = if hint.0.is_some() { Display::Flex } else { Display::None };
        let line = children
            .first()
            .and_then(|frame| frames.get(*frame).ok())
            .and_then(|c| c.first().copied());
        if let (Some(text), Some(line)) = (hint.0, line)
            && let Ok(mut t) = texts.get_mut(line)
        {
            t.0 = text.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{PlayerPlugin, PlayerSpawn};
    use crate::pulperia::PulperiaPlugin;
    use bevy::time::TimeUpdateStrategy;
    use pampa_core::{Abilities, GameClock, Heading, Item, Items, Money, Terrain, TimeOfDay, TownLayout, World};
    use pampa_render::{WorldClock, WorldTowns};
    use std::time::Duration;

    use crate::player::GameWorld;

    /// Castex, a mediodía, el jugador a pie frente al mostrador del local
    /// (`almacen`), a `away` metros de quien atiende, con `money` y
    /// `abilities`.
    fn app(almacen: bool, away: f32, money: Money, abilities: Abilities) -> App {
        build(almacen, away, money, abilities, false, false)
    }

    /// Como [`app`]; con `real`, el teclado y el menú de verdad.
    fn build(
        almacen: bool,
        away: f32,
        money: Money,
        abilities: Abilities,
        real: bool,
        errand: bool,
    ) -> App {
        let town = TownLayout::generate(&pampa_core::town::TownConfig::castex()).unwrap();
        let (_, _, inside) = town
            .rooms()
            .find(|(_, b, _)| (b.kind == pampa_core::BuildingKind::GeneralStore) == almacen)
            .unwrap();
        let keeper = inside.keeper;
        let at = keeper.position + keeper.facing.forward() * away;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        if errand {
            app.add_plugins(crate::errand::ErrandPlugin);
        }
        if real {
            app.add_plugins(bevy::input::InputPlugin)
                .add_plugins(crate::menu::MenuPlugin { title: false, settings: None });
        } else {
            app.init_resource::<ButtonInput<KeyCode>>();
        }
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(1.0 / 60.0)))
            .insert_resource(GameWorld(World::with_town(Terrain::default(), &town)))
            .insert_resource(WorldTowns(vec![town.clone()]))
            .insert_resource(WorldClock(GameClock::at(TimeOfDay::hm(12, 0))))
            .insert_resource(PlayerSpawn {
                position: at,
                heading: Heading::from_radians(0.0),
                travel: Travel::OnFoot,
            })
            .insert_resource(PlayerWallet(money))
            .insert_resource(PlayerItems(Items::default()))
            .insert_resource(PlayerAbilities(abilities))
            .add_plugins((PlayerPlugin, PulperiaPlugin, ShopPlugin));
        app.update();
        app.update();
        app
    }

    fn trader() -> Abilities {
        let mut a = Abilities::default();
        a.unlock(Ability::Trade);
        a
    }

    fn press(app: &mut App, key: KeyCode) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
        app.update();
    }

    fn said(app: &App) -> Option<(String, String)> {
        app.world()
            .resource::<Subtitle>()
            .0
            .clone()
            .map(|l| (l.speaker, l.text))
    }

    fn view(app: &App) -> Option<ShopContents> {
        app.world().resource::<ShopView>().0.clone()
    }

    fn locked(app: &App) -> bool {
        app.world().resource::<ControlLock>().0
    }

    fn hint(app: &App) -> Option<&'static str> {
        app.world().resource::<ShopHint>().0
    }

    #[test]
    fn without_trade_the_pulpero_greets_but_does_not_sell() {
        let mut app = app(false, 1.9, Money::new(3, 0), Abilities::default());
        assert_eq!(hint(&app), Some("F: hablar con el pulpero"));
        press(&mut app, INTERACT_KEY);
        assert_eq!(
            said(&app),
            Some(("Pulpero".into(), "Volvé cuando tengas con qué.".into()))
        );
        assert_eq!(view(&app), None);
        assert!(!locked(&app));
    }

    #[test]
    fn with_trade_f_opens_the_list_w_s_choose_f_buys_and_esc_leaves() {
        let mut app = app(false, 1.9, Money::new(0, 30), trader());
        press(&mut app, INTERACT_KEY);
        assert_eq!(
            said(&app),
            Some(("Pulpero".into(), "Buenas. ¿Qué le sirvo?".into()))
        );
        let list = view(&app).expect("the list opens");
        assert_eq!(list.title, "Pulpería");
        assert_eq!(list.money, "$ 0,30");
        assert_eq!(list.rows.len(), 7);
        assert_eq!(list.selected, 0);
        assert_eq!((list.rows[0].name.as_str(), list.rows[0].price.as_str()), ("Vaso de caña", "$ 0,10"));
        assert!(locked(&app));
        assert_eq!(hint(&app), None, "no hint while buying");

        // S baja al vaso de ginebra; F lo compra.
        press(&mut app, KeyCode::KeyS);
        assert_eq!(view(&app).unwrap().selected, 1);
        press(&mut app, INTERACT_KEY);
        assert_eq!(said(&app), Some(("Pulpero".into(), "Ahí tiene.".into())));
        assert_eq!(app.world().resource::<PlayerWallet>().0, Money::new(0, 15));
        assert_eq!(app.world().resource::<PlayerItems>().0.count(Item::GinGlass), 1);
        assert_eq!(view(&app).unwrap().money, "$ 0,15");

        // La botella de caña ($ 0,80) no alcanza: apagada, y F no la compra.
        press(&mut app, KeyCode::KeyS);
        press(&mut app, KeyCode::KeyS);
        let list = view(&app).unwrap();
        assert_eq!(list.selected, 3);
        assert!(!list.rows[3].available && list.rows[0].available);
        press(&mut app, INTERACT_KEY);
        assert_eq!(said(&app), Some(("Pulpero".into(), "No le alcanza la plata, amigo.".into())));
        assert_eq!(app.world().resource::<PlayerWallet>().0, Money::new(0, 15));
        assert_eq!(app.world().resource::<PlayerItems>().0.count(Item::CaneBottle), 0);

        // W desde el primero vuelve al último.
        for _ in 0..4 {
            press(&mut app, KeyCode::KeyW);
        }
        assert_eq!(view(&app).unwrap().selected, 6);

        // Esc cierra la lista y suelta al jugador; el menú no se entera.
        press(&mut app, LEAVE_KEY);
        assert_eq!(view(&app), None);
        assert!(!locked(&app));
        assert_eq!(hint(&app), Some("F: hablar con el pulpero"));
    }

    #[test]
    fn the_almacenero_sells_arms_only_to_who_can_use_them() {
        let mut app = app(true, 1.9, Money::new(100, 0), trader());
        assert_eq!(hint(&app), Some("F: hablar con el almacenero"));
        press(&mut app, INTERACT_KEY);
        let list = view(&app).unwrap();
        assert_eq!(list.title, "Almacén de ramos generales");
        assert_eq!(list.rows.len(), 15);
        let arms: Vec<bool> = list.rows[12..].iter().map(|r| r.available).collect();
        assert_eq!(arms, [false, false, false]);
        for _ in 0..12 {
            press(&mut app, KeyCode::KeyS);
        }
        press(&mut app, INTERACT_KEY);
        assert_eq!(said(&app), Some(("Almacenero".into(), "Armas no le vendo a cualquiera.".into())));
        assert_eq!(app.world().resource::<PlayerItems>().0.count(Item::Revolver), 0);
    }

    #[test]
    fn esc_leaves_the_counter_without_opening_the_menu() {
        use bevy::input::ButtonState;
        use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
        let mut app = build(false, 1.9, Money::new(3, 0), trader(), true, false);
        let real = |app: &mut App, key: KeyCode| {
            for state in [ButtonState::Pressed, ButtonState::Released] {
                app.world_mut().write_message(KeyboardInput {
                    key_code: key,
                    logical_key: Key::Unidentified(NativeKey::Unidentified),
                    state,
                    text: None,
                    repeat: false,
                    window: Entity::PLACEHOLDER,
                });
                app.update();
            }
        };
        let menu = |app: &App| app.world().resource::<crate::menu::Menu>().screen.is_some();
        real(&mut app, INTERACT_KEY);
        assert!(view(&app).is_some());
        real(&mut app, LEAVE_KEY);
        assert_eq!(view(&app), None);
        assert!(!menu(&app), "Esc only left the counter");
        // Afuera del mostrador, Esc vuelve a abrir el menú.
        real(&mut app, LEAVE_KEY);
        assert!(menu(&app));
    }

    #[test]
    fn the_player_stays_at_the_counter_while_the_list_is_open() {
        // Con el encargo andando, que también decide el bloqueo de las teclas.
        let mut app = build(false, 1.9, Money::new(3, 0), trader(), false, true);
        let at = |app: &mut App| {
            let mut q = app.world_mut().query::<&Player>();
            q.single(app.world()).unwrap().rider.position
        };
        press(&mut app, INTERACT_KEY);
        assert!(view(&app).is_some());
        let before = at(&mut app);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyS);
        for _ in 0..30 {
            app.update();
        }
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::KeyS);
        assert!(at(&mut app).distance(before) < 1e-3, "walked away from the counter");
        assert!(locked(&app));
        assert!(view(&app).is_some());
        // Al salir, el jugador vuelve a moverse.
        press(&mut app, LEAVE_KEY);
        assert!(!locked(&app));
    }

    #[test]
    fn away_from_the_counter_there_is_nothing_to_say() {
        let mut app = app(false, 4.0, Money::new(3, 0), trader());
        assert_eq!(hint(&app), None);
        press(&mut app, INTERACT_KEY);
        assert_eq!(said(&app), None);
        assert_eq!(view(&app), None);
    }
}
