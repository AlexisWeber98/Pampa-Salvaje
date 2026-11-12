//! La lista de lo que vende el pulpero o el almacenero, abierta mientras el
//! protagonista le compra en el mostrador: el nombre del local, la plata del
//! protagonista arriba, y un renglón por cosa con su precio; lo elegido se
//! marca, y lo que no se puede comprar ahora (no alcanza la plata, o hace falta
//! saber usar armas) se muestra apagado.
//!
//! El juego la abre y la cierra mediante [`ShopView`]: con contenido se
//! muestra, `None` la oculta; se rehace cada vez que cambia. Es de cuero, como
//! el inventario ([`crate::leather`]), a la derecha de la pantalla para que se
//! vea a quien atiende.

use bevy::prelude::*;

use crate::hud::LINE_PX;
use crate::inventory::{TITLE_PX, rule};
use crate::leather::{CREAM, FADED, GOLD, Leather, cover, load_leather, stamped, stitched};
use crate::typeface::Typefaces;

/// Las teclas, al pie.
pub const SHOP_KEYS: &str = "W/S: elegir    F: comprar    Esc: salir";
/// El fondo del renglón elegido: un dorado traslúcido sobre el cuero.
const PICKED: Color = Color::srgba(0.88, 0.68, 0.34, 0.24);

/// Un renglón de la lista.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopRow {
    /// El nombre de la cosa ("Vaso de caña").
    pub name: String,
    /// Su precio, escrito como en el HUD ("$ 0,10").
    pub price: String,
    /// Si se puede comprar ahora; si no, el renglón va apagado.
    pub available: bool,
}

/// Lo que muestra la lista abierta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopContents {
    /// El local: "Pulpería" o "Almacén de ramos generales".
    pub title: String,
    /// La plata del protagonista, escrita como en el HUD.
    pub money: String,
    pub rows: Vec<ShopRow>,
    /// El renglón elegido, por índice en `rows`.
    pub selected: usize,
}

/// La lista en pantalla; `None` la oculta.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct ShopView(pub Option<ShopContents>);

/// La capa que ubica la lista; se oculta sin contenido.
#[derive(Component, Debug)]
pub struct ShopPanel;

/// Donde se escribe la lista; se rehace en cada cambio.
#[derive(Component, Debug)]
pub struct ShopSheet;

/// Un renglón de la lista; el elegido lleva [`Picked`].
#[derive(Component, Debug)]
pub struct ShopLine;

/// Marca el renglón elegido.
#[derive(Component, Debug)]
pub struct Picked;

/// Muestra la lista de [`ShopView`].
pub struct ShopPanelPlugin;

impl Plugin for ShopPanelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Typefaces>()
            .init_resource::<ShopView>()
            .init_resource::<Leather>()
            .add_systems(Startup, (load_leather, spawn_panel).chain())
            .add_systems(PostUpdate, update_panel);
    }
}

fn spawn_panel(mut commands: Commands, leather: Res<Leather>) {
    commands
        .spawn((
            Name::new("Shop list"),
            ShopPanel,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::End,
                align_items: AlignItems::Center,
                padding: UiRect::right(Val::Percent(6.0)),
                ..default()
            },
            // Como el inventario: sobre el HUD y los subtítulos, bajo el menú.
            GlobalZIndex(5),
        ))
        .with_child((
            cover(&leather),
            children![(
                ShopSheet,
                stitched(Node {
                    flex_direction: FlexDirection::Column,
                    min_width: Val::Px(380.0),
                    padding: UiRect::axes(Val::Px(20.0), Val::Px(14.0)),
                    row_gap: Val::Px(2.0),
                    ..default()
                }),
            )],
        ));
}

/// Rehace la lista cuando cambia [`ShopView`], y la muestra u oculta.
fn update_panel(
    view: Res<ShopView>,
    faces: Res<Typefaces>,
    mut commands: Commands,
    mut panels: Query<&mut Node, With<ShopPanel>>,
    sheets: Query<Entity, With<ShopSheet>>,
) {
    if !view.is_changed() {
        return;
    }
    for mut node in &mut panels {
        node.display = if view.0.is_some() { Display::Flex } else { Display::None };
    }
    let Some(contents) = &view.0 else {
        return;
    };
    for sheet in &sheets {
        commands.entity(sheet).despawn_related::<Children>();
        commands.entity(sheet).with_children(|sheet| {
            sheet.spawn((
                Text::new(contents.title.clone()),
                stamped(&faces.display, TITLE_PX * 0.85, GOLD),
                Node {
                    align_self: AlignSelf::Center,
                    ..default()
                },
            ));
            sheet.spawn((
                Text::new(format!("Plata: {}", contents.money)),
                stamped(&faces.italic, LINE_PX, CREAM),
                Node {
                    align_self: AlignSelf::Center,
                    ..default()
                },
            ));
            sheet.spawn(rule());
            for (k, row) in contents.rows.iter().enumerate() {
                let color = if row.available { CREAM } else { FADED };
                let mut line = sheet.spawn((
                    ShopLine,
                    Node {
                        justify_content: JustifyContent::SpaceBetween,
                        column_gap: Val::Px(24.0),
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                        ..default()
                    },
                    children![
                        (Text::new(row.name.clone()), stamped(&faces.body, LINE_PX, color)),
                        (Text::new(row.price.clone()), stamped(&faces.body, LINE_PX, color)),
                    ],
                ));
                if k == contents.selected {
                    line.insert((Picked, BackgroundColor(PICKED)));
                }
            }
            sheet.spawn((
                Text::new(SHOP_KEYS),
                stamped(&faces.italic, LINE_PX * 0.8, CREAM),
                Node {
                    align_self: AlignSelf::Center,
                    margin: UiRect::top(Val::Px(10.0)),
                    ..default()
                },
            ));
        });
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
            .add_plugins(ShopPanelPlugin);
        app.update();
        app
    }

    /// Cada renglón: nombre, precio, si está elegido y si la letra va apagada.
    fn rows(app: &mut App) -> Vec<(String, String, bool, bool)> {
        let world = app.world_mut();
        let mut lines: Vec<(Entity, bool)> = world
            .query::<(Entity, Has<Picked>, &ShopLine)>()
            .iter(world)
            .map(|(e, picked, _)| (e, picked))
            .collect();
        let sheet = world.query_filtered::<&Children, With<ShopSheet>>().single(world).unwrap();
        let order: Vec<Entity> = sheet.iter().collect();
        lines.sort_by_key(|(e, _)| order.iter().position(|o| o == e));
        lines
            .into_iter()
            .map(|(e, picked)| {
                let kids = world.entity(e).get::<Children>().unwrap();
                let text = |k: usize| world.entity(kids[k]).get::<Text>().unwrap().0.clone();
                let faded = world.entity(kids[0]).get::<TextColor>().unwrap().0 == FADED;
                (text(0), text(1), picked, faded)
            })
            .collect()
    }

    fn shown(app: &mut App) -> bool {
        let world = app.world_mut();
        world.query_filtered::<&Node, With<ShopPanel>>().single(world).unwrap().display
            == Display::Flex
    }

    #[test]
    fn the_list_shows_each_ware_with_its_price_the_choice_and_what_cannot_be_bought() {
        let mut app = app();
        assert!(!shown(&mut app), "hidden at first");
        let row = |name: &str, price: &str, available| ShopRow {
            name: name.into(),
            price: price.into(),
            available,
        };
        app.world_mut().resource_mut::<ShopView>().0 = Some(ShopContents {
            title: "Pulpería".into(),
            money: "$ 0,30".into(),
            rows: vec![row("Vaso de caña", "$ 0,10", true), row("Botella de caña", "$ 0,80", false)],
            selected: 1,
        });
        app.update();
        assert!(shown(&mut app));
        assert_eq!(
            rows(&mut app),
            [
                ("Vaso de caña".into(), "$ 0,10".into(), false, false),
                ("Botella de caña".into(), "$ 0,80".into(), true, true),
            ]
        );
        app.world_mut().resource_mut::<ShopView>().0 = None;
        app.update();
        assert!(!shown(&mut app), "hidden when closed");
    }
}
