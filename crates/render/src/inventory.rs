//! El inventario: un panel en el centro de la pantalla, encima del juego (que
//! sigue andando detrás), con lo que lleva el protagonista agrupado por las
//! categorías del documento `mecánicas` ("Alimentos", "Objetos de misión") y
//! la plata. Cada cosa lleva su dibujo y su nombre.
//!
//! El panel es de cuero ([`crate::leather`]): la tapa con su costura y la letra
//! estampada, crema, con los títulos dorados; el título en Rye y el resto en IM
//! Fell English ([`Typefaces`]). Cada cosa y la plata llevan su ícono
//! renderizado de un modelo 3D por `tools/item_icons.py` ([`icon_file`],
//! `assets/textures/ui/items/`); sin los archivos (en las pruebas) quedan los
//! grabados pintados aquí mismo, en tinta sobre colores planos: una cuña de
//! queso de campo, un sobre lacrado, dos pesos de plata y un paquete atado.
//!
//! El juego abre y cierra el panel mediante [`InventoryView`]: con contenido se
//! muestra, `None` lo oculta; el panel se rehace cada vez que cambia.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use pampa_core::{Item, Items};

use crate::hud::LINE_PX;
use crate::leather::{CREAM, GOLD, Leather, cover, load_leather, stamped, stitched};
use crate::typeface::Typefaces;

/// Tamaño de fuente del título del panel, en píxeles lógicos.
pub const TITLE_PX: f32 = 30.0;
/// Lado de un dibujo en pantalla, en píxeles lógicos.
const ICON_PX: f32 = 52.0;
/// Lado de la textura de un dibujo, en píxeles.
const ICON_SIZE: usize = 96;
/// La pista de tecla al pie.
pub const CLOSE_HINT: &str = "I: cerrar";
/// El título del renglón de la plata.
pub const MONEY_HEADING: &str = "Plata";

/// Lo que muestra el inventario abierto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryContents {
    /// La plata, ya escrita como en el HUD ("$ 3,00").
    pub money: String,
    /// Las cosas que lleva.
    pub items: Items,
}

/// El inventario en pantalla; `None` lo oculta.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct InventoryView(pub Option<InventoryContents>);

/// La capa que centra el panel; se oculta sin contenido.
#[derive(Component, Debug)]
pub struct InventoryPanel;

/// La hoja donde se escriben las secciones; se rehace en cada cambio.
#[derive(Component, Debug)]
pub struct InventorySheet;

/// El título de una sección ("Alimentos", "Plata").
#[derive(Component, Debug)]
pub struct InventoryHeading;

/// El nombre de una cosa o la plata escrita.
#[derive(Component, Debug)]
pub struct InventoryEntry;

/// El ícono renderizado de `item` (por `tools/item_icons.py`, de un modelo 3D),
/// bajo `assets/`.
pub fn icon_file(item: Item) -> &'static str {
    match item {
        Item::Letter => "textures/ui/items/letter.png",
        Item::Cheese => "textures/ui/items/cheese.png",
        Item::CaneGlass => "textures/ui/items/cane_glass.png",
        Item::GinGlass => "textures/ui/items/gin_glass.png",
        Item::WineGlass => "textures/ui/items/wine_glass.png",
        Item::CaneBottle => "textures/ui/items/cane_bottle.png",
        Item::Tobacco => "textures/ui/items/tobacco.png",
        Item::Matches => "textures/ui/items/matches.png",
        Item::Biscuit => "textures/ui/items/biscuit.png",
        Item::Yerba => "textures/ui/items/yerba.png",
        Item::Sugar => "textures/ui/items/sugar.png",
        Item::Jerky => "textures/ui/items/jerky.png",
        Item::Flour => "textures/ui/items/flour.png",
        Item::Candles => "textures/ui/items/candles.png",
        Item::Knife => "textures/ui/items/knife.png",
        Item::Axe => "textures/ui/items/axe.png",
        Item::Shovel => "textures/ui/items/shovel.png",
        Item::Maneador => "textures/ui/items/maneador.png",
        Item::Lasso => "textures/ui/items/lasso.png",
        Item::Alpargatas => "textures/ui/items/alpargatas.png",
        Item::Poncho => "textures/ui/items/poncho.png",
        Item::Revolver => "textures/ui/items/revolver.png",
        Item::Winchester => "textures/ui/items/winchester.png",
        Item::Cartridges => "textures/ui/items/cartridges.png",
    }
}

/// El ícono renderizado de la plata: dos pesos de plata.
pub const COINS_FILE: &str = "textures/ui/items/coins.png";

/// Las texturas de los dibujos: los íconos renderizados si hay un
/// `AssetServer`, si no los grabados pintados; por defecto (sin
/// `Assets<Image>`) quedan vacías.
#[derive(Resource, Debug, Clone, Default)]
pub struct InventoryIcons {
    /// Los íconos renderizados ([`icon_file`]), cargados del disco.
    pub files: std::collections::HashMap<Item, Handle<Image>>,
    pub cheese: Handle<Image>,
    pub letter: Handle<Image>,
    pub coins: Handle<Image>,
    /// Un paquete de papel atado: por ahora, lo comprado en la pulpería y el
    /// almacén.
    pub parcel: Handle<Image>,
}

impl InventoryIcons {
    fn of(&self, item: Item) -> Handle<Image> {
        if let Some(file) = self.files.get(&item) {
            return file.clone();
        }
        match item {
            Item::Cheese => self.cheese.clone(),
            Item::Letter => self.letter.clone(),
            _ => self.parcel.clone(),
        }
    }
}

/// Muestra el inventario de [`InventoryView`].
pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Typefaces>()
            .init_resource::<InventoryView>()
            .init_resource::<InventoryIcons>()
            .init_resource::<Leather>()
            .add_systems(Startup, (paint_icons, load_leather, spawn_panel).chain())
            .add_systems(PostUpdate, update_panel);
    }
}

fn icon_image(pixels: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: ICON_SIZE as u32,
            height: ICON_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image
}

fn paint_icons(
    images: Option<ResMut<Assets<Image>>>,
    server: Option<Res<AssetServer>>,
    mut icons: ResMut<InventoryIcons>,
) {
    let Some(mut images) = images else {
        return;
    };
    let files = server
        .as_ref()
        .map(|server| {
            Item::ALL
                .iter()
                .map(|&item| (item, server.load(icon_file(item))))
                .collect()
        })
        .unwrap_or_default();
    let coins = match &server {
        Some(server) => server.load(COINS_FILE),
        None => images.add(icon_image(paint(ICON_SIZE, coins))),
    };
    *icons = InventoryIcons {
        files,
        cheese: images.add(icon_image(paint(ICON_SIZE, cheese))),
        letter: images.add(icon_image(paint(ICON_SIZE, letter))),
        coins,
        parcel: images.add(icon_image(paint(ICON_SIZE, parcel))),
    };
}

fn spawn_panel(mut commands: Commands, leather: Res<Leather>) {
    commands
        .spawn((
            Name::new("Inventory"),
            InventoryPanel,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            // Por encima del HUD y los subtítulos, por debajo del menú.
            GlobalZIndex(5),
        ))
        .with_child((
            cover(&leather),
            children![(
                InventorySheet,
                stitched(Node {
                    flex_direction: FlexDirection::Column,
                    min_width: Val::Px(340.0),
                    padding: UiRect::axes(Val::Px(22.0), Val::Px(16.0)),
                    row_gap: Val::Px(6.0),
                    ..default()
                }),
            )],
        ));
}

/// Una línea dorada de lado a lado, bajo el título.
pub(crate) fn rule() -> impl Bundle {
    (
        Node {
            height: Val::Px(1.0),
            margin: UiRect::vertical(Val::Px(4.0)),
            ..default()
        },
        BackgroundColor(GOLD),
    )
}

/// Rehace la hoja cuando cambia [`InventoryView`], y la muestra u oculta.
fn update_panel(
    view: Res<InventoryView>,
    icons: Res<InventoryIcons>,
    faces: Res<Typefaces>,
    mut commands: Commands,
    mut panels: Query<&mut Node, With<InventoryPanel>>,
    sheets: Query<Entity, With<InventorySheet>>,
) {
    if !view.is_changed() {
        return;
    }
    for mut node in &mut panels {
        node.display = if view.0.is_some() {
            Display::Flex
        } else {
            Display::None
        };
    }
    let Some(contents) = &view.0 else {
        return;
    };
    let heading = |text: &str| {
        (
            InventoryHeading,
            Text::new(text),
            stamped(&faces.italic, LINE_PX, GOLD),
            Node {
                margin: UiRect::top(Val::Px(8.0)),
                ..default()
            },
        )
    };
    let entry = |icon: Handle<Image>, text: &str| {
        (
            Node {
                align_items: AlignItems::Center,
                column_gap: Val::Px(12.0),
                padding: UiRect::left(Val::Px(10.0)),
                ..default()
            },
            children![
                (
                    ImageNode::new(icon),
                    Node {
                        width: Val::Px(ICON_PX),
                        height: Val::Px(ICON_PX),
                        ..default()
                    },
                ),
                (
                    InventoryEntry,
                    Text::new(text),
                    stamped(&faces.body, LINE_PX, CREAM)
                ),
            ],
        )
    };
    for sheet in &sheets {
        commands.entity(sheet).despawn_related::<Children>();
        commands.entity(sheet).with_children(|sheet| {
            sheet.spawn((
                Text::new("Inventario"),
                stamped(&faces.display, TITLE_PX, GOLD),
                Node {
                    align_self: AlignSelf::Center,
                    ..default()
                },
            ));
            sheet.spawn(rule());
            for (category, items) in contents.items.by_category() {
                sheet.spawn(heading(category.name()));
                for (item, count) in items {
                    sheet.spawn(entry(icons.of(item), &counted(item.name(), count)));
                }
            }
            sheet.spawn(heading(MONEY_HEADING));
            sheet.spawn(entry(icons.coins.clone(), &contents.money));
            sheet.spawn((
                Text::new(CLOSE_HINT),
                stamped(&faces.italic, LINE_PX * 0.8, CREAM),
                Node {
                    align_self: AlignSelf::End,
                    margin: UiRect::top(Val::Px(10.0)),
                    ..default()
                },
            ));
        });
    }
}

/// El renglón de una cosa: su nombre, y cuántas si lleva más de una.
pub fn counted(name: &str, count: u32) -> String {
    if count > 1 {
        format!("{name} × {count}")
    } else {
        name.to_string()
    }
}

// --- Los dibujos -----------------------------------------------------------

/// Un color RGBA lineal-en-sRGB, de 0 a 1.
type Rgba = [f32; 4];

const INK: Rgba = [0.16, 0.12, 0.09, 1.0];

fn rgb(r: u8, g: u8, b: u8) -> Rgba {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

/// Pinta un dibujo de `size` píxeles de lado: `shade` da el color de cada
/// punto del cuadrado unitario (y hacia abajo), o `None` donde no hay nada.
/// Cada píxel promedia 4×4 muestras, para bordes suaves.
fn paint(size: usize, shade: fn(Vec2) -> Option<Rgba>) -> Vec<u8> {
    const N: usize = 4;
    let mut pixels = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let mut sum = [0.0f32; 4];
            for sy in 0..N {
                for sx in 0..N {
                    let p = Vec2::new(
                        (x as f32 + (sx as f32 + 0.5) / N as f32) / size as f32,
                        (y as f32 + (sy as f32 + 0.5) / N as f32) / size as f32,
                    );
                    if let Some(c) = shade(p) {
                        for i in 0..3 {
                            sum[i] += c[i] * c[3];
                        }
                        sum[3] += c[3];
                    }
                }
            }
            let alpha = sum[3] / (N * N) as f32;
            for channel in &sum[..3] {
                let c = if sum[3] > 0.0 { channel / sum[3] } else { 0.0 };
                pixels.push((c.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            pixels.push((alpha.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    pixels
}

/// Distancia de `p` al segmento `a`–`b`.
fn to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Si `p` está dentro del polígono convexo `poly` (en cualquier sentido).
fn inside(p: Vec2, poly: &[Vec2]) -> bool {
    let mut sign = 0.0f32;
    for (i, a) in poly.iter().enumerate() {
        let b = poly[(i + 1) % poly.len()];
        let cross = (b - *a).perp_dot(p - *a);
        if cross != 0.0 {
            if sign != 0.0 && cross.signum() != sign {
                return false;
            }
            sign = cross.signum();
        }
    }
    true
}

/// Distancia de `p` al borde del polígono `poly`.
fn to_outline(p: Vec2, poly: &[Vec2]) -> f32 {
    (0..poly.len())
        .map(|i| to_segment(p, poly[i], poly[(i + 1) % poly.len()]))
        .fold(f32::MAX, f32::min)
}

/// Rayado de grabado: líneas diagonales de `pitch` de separación.
fn hatched(p: Vec2, pitch: f32) -> bool {
    ((p.x + p.y) / pitch).fract() < 0.32
}

fn darker(c: Rgba, k: f32) -> Rgba {
    [c[0] * k, c[1] * k, c[2] * k, c[3]]
}

/// Grosor de la línea de tinta del contorno.
const LINE: f32 = 0.022;

/// Una cuña de queso de campo, de pasta amarilla pálida con unos pocos ojos y
/// cáscara ocre, vista desde arriba y de costado.
fn cheese(p: Vec2) -> Option<Rgba> {
    let tip = Vec2::new(0.08, 0.56);
    let back_left = Vec2::new(0.56, 0.24);
    let back_right = Vec2::new(0.93, 0.40);
    let drop = Vec2::new(0.0, 0.27);
    let top = [tip, back_left, back_right];
    let side = [tip, back_right, back_right + drop, tip + drop];
    let rind_side = [
        back_right + Vec2::new(-0.07, 0.0),
        back_right,
        back_right + drop,
        back_right + drop + Vec2::new(-0.07, 0.0),
    ];
    let paste = rgb(240, 214, 140);
    let rind = rgb(196, 132, 52);
    if to_outline(p, &top) < LINE || to_outline(p, &side) < LINE {
        return Some(INK);
    }
    if inside(p, &top) {
        // La cáscara corre a lo largo del borde de atrás.
        if to_segment(p, back_left, back_right) < 0.06 {
            return Some(rind);
        }
        return Some(paste);
    }
    if inside(p, &side) {
        if inside(p, &rind_side) {
            return Some(darker(rind, 0.82));
        }
        // Unos pocos ojos, los agujeritos del queso criollo.
        for (c, r) in [
            (Vec2::new(0.30, 0.66), 0.030),
            (Vec2::new(0.52, 0.71), 0.022),
            (Vec2::new(0.70, 0.60), 0.026),
        ] {
            if ((p - c) * Vec2::new(1.0, 1.4)).length() < r {
                return Some(rgb(178, 138, 70));
            }
        }
        let face = darker(paste, 0.86);
        // Sombra rayada en la mitad baja del costado.
        let below = (p.y
            - (tip.y + (p.x - tip.x) * (back_right.y - tip.y) / (back_right.x - tip.x)))
            / drop.y;
        if below > 0.55 && hatched(p, 0.045) {
            return Some(darker(face, 0.72));
        }
        return Some(face);
    }
    None
}

/// Un sobre de papel crema, visto de espaldas, cerrado con lacre rojo.
fn letter(p: Vec2) -> Option<Rgba> {
    let (l, r, t, b) = (0.08, 0.92, 0.22, 0.78);
    let body = [
        Vec2::new(l, t),
        Vec2::new(r, t),
        Vec2::new(r, b),
        Vec2::new(l, b),
    ];
    let seal = Vec2::new(0.5, 0.55);
    let paper = rgb(246, 238, 214);
    let wax = rgb(150, 26, 22);
    let d = p - seal;
    // El lacre, algo irregular en el borde, con su anillo estampado.
    let ragged = 0.095 + 0.008 * (d.y.atan2(d.x) * 7.0).sin();
    if d.length() < ragged {
        if (d.length() - 0.055).abs() < 0.008 {
            return Some(darker(wax, 0.6));
        }
        if d.length() < 0.03 {
            return Some(darker(wax, 1.25));
        }
        return Some(wax);
    }
    if to_outline(p, &body) < LINE {
        return Some(INK);
    }
    if !inside(p, &body) {
        return None;
    }
    // Las solapas: del borde de arriba al lacre, y de las esquinas de abajo.
    let flap = [Vec2::new(l, t), Vec2::new(r, t), seal];
    for (a, z) in [
        (Vec2::new(l, t), seal),
        (Vec2::new(r, t), seal),
        (Vec2::new(l, b), Vec2::new(0.5, 0.48)),
        (Vec2::new(r, b), Vec2::new(0.5, 0.48)),
    ] {
        if to_segment(p, a, z) < LINE * 0.6 {
            return Some(INK);
        }
    }
    if inside(p, &flap) {
        return Some(darker(paper, 0.9));
    }
    if p.y > 0.62 && hatched(p, 0.05) {
        return Some(darker(paper, 0.82));
    }
    Some(paper)
}

/// Una moneda de plata: canto oscuro, gráfila de puntos y el campo brillante
/// arriba a la izquierda.
fn coin(p: Vec2, center: Vec2, radius: f32) -> Option<Rgba> {
    let d = p - center;
    let r = d.length() / radius;
    if r > 1.0 {
        return None;
    }
    if r > 1.0 - LINE / radius * 1.6 {
        return Some(INK);
    }
    let silver = rgb(206, 208, 206);
    // Luz desde arriba a la izquierda.
    let lit = 1.0 - 0.28 * ((d.x + d.y) / radius * 0.5 + 0.5);
    if r > 0.86 {
        return Some(darker(silver, 0.78 * lit));
    }
    // La gráfila: un anillo de puntitos.
    let beads = 26.0;
    let angle = d.y.atan2(d.x);
    if (0.72..0.82).contains(&r) {
        let phase = (angle / std::f32::consts::TAU * beads).rem_euclid(1.0);
        if (phase - 0.5).abs() < 0.22 {
            return Some(darker(silver, 0.55 * lit));
        }
    }
    // El valor en el campo, como una cifra "1" grabada.
    let one = [
        (Vec2::new(-0.02, -0.32), Vec2::new(-0.02, 0.30)),
        (Vec2::new(-0.02, -0.32), Vec2::new(-0.16, -0.18)),
        (Vec2::new(-0.14, 0.30), Vec2::new(0.12, 0.30)),
    ];
    if one
        .iter()
        .any(|(a, b)| to_segment(d / radius, *a, *b) < 0.06)
    {
        return Some(darker(silver, 0.5));
    }
    Some(darker(silver, lit))
}

/// Dos pesos de plata, uno asomando detrás del otro.
fn coins(p: Vec2) -> Option<Rgba> {
    // La de atrás, algo en sombra.
    coin(p, Vec2::new(0.40, 0.42), 0.32)
        .or_else(|| coin(p, Vec2::new(0.64, 0.62), 0.30).map(|c| darker(c, 0.82)))
}

/// Un paquete de papel de estraza, atado en cruz con hilo, visto de arriba y
/// de costado.
fn parcel(p: Vec2) -> Option<Rgba> {
    let top = [
        Vec2::new(0.14, 0.40),
        Vec2::new(0.52, 0.24),
        Vec2::new(0.88, 0.38),
        Vec2::new(0.50, 0.54),
    ];
    let front = [
        Vec2::new(0.14, 0.40),
        Vec2::new(0.50, 0.54),
        Vec2::new(0.50, 0.80),
        Vec2::new(0.14, 0.66),
    ];
    let side = [
        Vec2::new(0.50, 0.54),
        Vec2::new(0.88, 0.38),
        Vec2::new(0.88, 0.64),
        Vec2::new(0.50, 0.80),
    ];
    let paper = rgb(196, 160, 112);
    let string = rgb(236, 226, 196);
    // El hilo, en cruz por el medio de cada cara.
    let strings = [
        (Vec2::new(0.33, 0.32), Vec2::new(0.69, 0.46)),
        (Vec2::new(0.70, 0.31), Vec2::new(0.32, 0.47)),
        (Vec2::new(0.32, 0.47), Vec2::new(0.32, 0.73)),
        (Vec2::new(0.69, 0.46), Vec2::new(0.69, 0.72)),
    ];
    if [&top[..], &front[..], &side[..]]
        .iter()
        .any(|f| to_outline(p, f) < LINE)
    {
        return Some(INK);
    }
    let face = if inside(p, &top) {
        paper
    } else if inside(p, &front) {
        darker(paper, 0.88)
    } else if inside(p, &side) {
        if hatched(p, 0.045) {
            darker(paper, 0.62)
        } else {
            darker(paper, 0.76)
        }
    } else {
        return None;
    };
    if strings.iter().any(|(a, b)| to_segment(p, *a, *b) < 0.014) {
        return Some(string);
    }
    Some(face)
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
            .add_plugins(InventoryPlugin);
        app.update();
        app
    }

    /// Lo que se lee en la hoja, en orden: los títulos entre corchetes y las
    /// entradas tal cual; y si el panel se muestra.
    fn read(app: &mut App) -> (Vec<String>, Display) {
        let world = app.world_mut();
        let display = world
            .query_filtered::<&Node, With<InventoryPanel>>()
            .single(world)
            .unwrap()
            .display;
        let sheet = world
            .query_filtered::<Entity, With<InventorySheet>>()
            .single(world)
            .unwrap();
        let mut lines = Vec::new();
        let mut stack = vec![sheet];
        while let Some(e) = stack.pop() {
            let entity = world.entity(e);
            if let Some(text) = entity.get::<Text>() {
                if entity.contains::<InventoryHeading>() {
                    lines.push(format!("[{}]", text.0));
                } else if entity.contains::<InventoryEntry>() {
                    lines.push(text.0.clone());
                }
            }
            if let Some(children) = entity.get::<Children>() {
                stack.extend(children.iter().rev());
            }
        }
        (lines, display)
    }

    fn show(app: &mut App, money: &str, items: &[Item]) {
        let mut carried = Items::default();
        for item in items {
            carried.receive(*item);
        }
        app.world_mut().resource_mut::<InventoryView>().0 = Some(InventoryContents {
            money: money.into(),
            items: carried,
        });
        app.update();
    }

    #[test]
    fn the_panel_lists_each_thing_under_its_category_and_the_money() {
        let mut app = app();
        assert_eq!(read(&mut app), (vec![], Display::None), "hidden at first");

        show(&mut app, "$ 3,00", &[Item::Letter, Item::Cheese]);
        assert_eq!(
            read(&mut app),
            (
                vec![
                    "[Alimentos]".into(),
                    "Queso".into(),
                    "[Objetos de misión]".into(),
                    "Carta de presentación".into(),
                    "[Plata]".into(),
                    "$ 3,00".into(),
                ],
                Display::Flex
            )
        );

        app.world_mut().resource_mut::<InventoryView>().0 = None;
        app.update();
        assert_eq!(read(&mut app).1, Display::None, "hidden when closed");
    }

    #[test]
    fn every_rendered_icon_is_in_the_assets() {
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let files: Vec<&str> = Item::ALL.into_iter().map(icon_file).chain([COINS_FILE]).collect();
        let unique: std::collections::HashSet<&&str> = files.iter().collect();
        assert_eq!(unique.len(), Item::ALL.len() + 1, "one icon each: {files:?}");
        for file in files {
            let path = assets.join(file);
            assert!(path.is_file(), "{}", path.display());
        }
    }

    #[test]
    fn a_category_with_nothing_in_it_is_not_shown() {
        let mut app = app();
        show(&mut app, "$ 0,00", &[Item::Letter]);
        assert_eq!(
            read(&mut app).0,
            vec![
                "[Objetos de misión]".to_string(),
                "Carta de presentación".into(),
                "[Plata]".into(),
                "$ 0,00".into(),
            ]
        );
    }
}
