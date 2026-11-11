//! El puesto de quesos del Viejo Amadeo, en la vereda: una mesa de caballetes
//! de tablas gastadas, con unas hormas de queso redondas encima, una cuña
//! cortada, unos salamines (uno empezado) y dos panes de campo, y un cajón de
//! madera abajo. Todo procedural, con las superficies de madera de las
//! fachadas ([`FacadeMaterials`]) y materiales propios para lo que se vende:
//! la pasta clara y la cáscara del queso, la tripa del salamín y su corte, y
//! la corteza del pan.
//!
//! La lógica pone una entidad [`CheeseStall`] con su `Transform` (la mesa
//! apoyada en el suelo, con su +X local hacia donde mira el puestero, a través
//! de la mesa, y su largo a lo largo de Z); este módulo la viste una vez, con
//! sus piezas como hijas, que se van con ella.

use bevy::prelude::*;
use pampa_core::Stall;

use crate::facade::shape::Shape;
use crate::facade::{FacadeMaterials, Surface};

/// Un puesto de quesos a vestir, apoyado en el suelo en su `Transform`.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CheeseStall;

/// La madera de los caballetes y del cajón: durmiente viejo, oscuro.
const FRAME: Surface = Surface::Sleeper;
/// Las tablas de la mesa.
const TOP: Surface = Surface::Barnwood;
/// El cajón de abajo.
const CRATE: Surface = Surface::Planks;

/// Grosor de las tablas de la mesa, en metros.
const BOARD: f32 = 0.04;
/// Radio y alto de una horma de queso, en metros: una horma de campo de unos
/// cuatro kilos.
const WHEEL_RADIUS: f32 = 0.15;
const WHEEL_HEIGHT: f32 = 0.09;
/// Radio y largo de un salamín casero, en metros.
const SALAMI_RADIUS: f32 = 0.025;
const SALAMI_LENGTH: f32 = 0.3;
/// Radio y alto de un pan de campo redondo, en metros.
const LOAF_RADIUS: f32 = 0.11;
const LOAF_HEIGHT: f32 = 0.09;

/// Las piezas de un puesto: la madera, por superficie, y lo que se vende,
/// cada material en su propio [`Shape`] (de una sola superficie que no se usa
/// para el material): el queso, partido en lo que lleva cáscara y lo que
/// muestra la pasta cortada; los salamines, su tripa y la cara del corte; y
/// los panes.
pub(crate) struct StallShapes {
    pub(crate) wood: Shape,
    pub(crate) rind: Shape,
    pub(crate) paste: Shape,
    pub(crate) salami: Shape,
    pub(crate) salami_cut: Shape,
    pub(crate) bread: Shape,
}

/// El puesto entero, en su espacio local: la mesa de [`Stall::LENGTH`] por
/// [`Stall::DEPTH`] con la tabla a [`Stall::HEIGHT`] del suelo.
pub(crate) fn stall_shapes() -> StallShapes {
    let mut wood = Shape::default();
    let (half_x, half_z, top) = (Stall::DEPTH / 2.0, Stall::LENGTH / 2.0, Stall::HEIGHT);
    // Las tablas: cuatro a lo largo, con una rendija entre ellas.
    let boards = 4;
    let gap = 0.008;
    let width = (Stall::DEPTH - gap * (boards - 1) as f32) / boards as f32;
    for k in 0..boards {
        let x0 = -half_x + k as f32 * (width + gap);
        wood.block(
            TOP,
            Vec3::new(x0, top - BOARD, -half_z),
            Vec3::new(x0 + width, top, half_z),
        );
    }
    // Dos caballetes, cada uno con sus patas abiertas en A y un travesaño
    // arriba, unidos por un larguero bajo.
    let under = top - BOARD;
    for z in [-half_z + 0.22, half_z - 0.22] {
        wood.block(
            FRAME,
            Vec3::new(-half_x + 0.04, under - 0.07, z - 0.035),
            Vec3::new(half_x - 0.04, under, z + 0.035),
        );
        for side in [-1.0, 1.0] {
            wood.beam(
                FRAME,
                Vec3::new(side * (half_x - 0.04), 0.0, z),
                Vec3::new(side * (half_x - 0.14), under - 0.07, z),
                0.05,
            );
        }
        wood.beam(
            FRAME,
            Vec3::new(-half_x + 0.1, 0.28, z),
            Vec3::new(half_x - 0.1, 0.28, z),
            0.035,
        );
    }
    wood.beam(
        FRAME,
        Vec3::new(0.0, 0.28, -half_z + 0.22),
        Vec3::new(0.0, 0.28, half_z - 0.22),
        0.04,
    );
    // Un cajón de tablas abajo, del lado del puestero.
    wood.block(
        CRATE,
        Vec3::new(-half_x + 0.05, 0.0, -0.3),
        Vec3::new(-half_x + 0.45, 0.3, 0.18),
    );

    let (mut rind, mut paste) = (Shape::default(), Shape::default());
    // Dos hormas apiladas en una punta, una sola al medio, y del lado de la
    // gente, en la otra punta, una horma con la cuña cortada al lado.
    let on_top =
        |x: f32, z: f32, level: u32| Transform::from_xyz(x, top + level as f32 * WHEEL_HEIGHT, z);
    for place in [
        on_top(-0.12, -0.58, 0),
        on_top(-0.1, -0.56, 1),
        on_top(0.1, -0.12, 0),
        on_top(0.12, 0.42, 0),
    ] {
        wheel(&mut rind, place);
    }
    wedge(&mut rind, &mut paste, Vec3::new(0.16, top, 0.7));

    // Tres salamines tendidos a lo largo, del lado de la gente, junto a las
    // hormas apiladas; el del medio, empezado, más corto y con el corte a la
    // vista.
    let (mut salami, mut salami_cut) = (Shape::default(), Shape::default());
    for (x, z, length, cut) in [
        (0.24, -0.78, SALAMI_LENGTH, false),
        (0.30, -0.74, SALAMI_LENGTH * 0.7, true),
        (0.36, -0.8, SALAMI_LENGTH, false),
    ] {
        sausage(
            &mut salami,
            &mut salami_cut,
            Vec3::new(x, top, z),
            length,
            cut,
        );
    }
    // Dos panes de campo, del lado del puestero, entre las hormas.
    let mut bread = Shape::default();
    for at in [Vec3::new(-0.24, top, 0.15), Vec3::new(-0.25, top, 0.52)] {
        loaf(&mut bread, at);
    }
    StallShapes {
        wood,
        rind,
        paste,
        salami,
        salami_cut,
        bread,
    }
}

/// Un salamín tendido sobre la tabla a lo largo de +Z, con su punta trasera
/// en `at` (apoyado en `at.y`): un cilindro fino de puntas redondeadas por el
/// atado; uno empezado (`cut`) termina en una cara plana con el corte.
fn sausage(casing: &mut Shape, cut_face: &mut Shape, at: Vec3, length: f32, cut: bool) {
    let r = SALAMI_RADIUS;
    let place = Transform::from_translation(at + Vec3::Y * r)
        .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
    let mut profile = vec![
        (0.0, 0.0),
        (r * 0.45, 0.004),
        (r * 0.85, 0.014),
        (r, 0.03),
        (r * 0.97, length * 0.5),
    ];
    if cut {
        profile.extend([(r, length), (0.0, length)]);
    } else {
        profile.extend([
            (r, length - 0.03),
            (r * 0.85, length - 0.014),
            (r * 0.45, length - 0.004),
            (0.0, length),
        ]);
    }
    casing.lathe(Surface::Lime, place, &profile, 14);
    if cut {
        // El corte, apenas por fuera de la tapa de la tripa.
        let face = place * Transform::from_xyz(0.0, length + 0.001, 0.0);
        cut_face.disc(Surface::Lime, face, r * 0.96, 14);
    }
}

/// Un pan de campo redondo apoyado en `at`: base chata y copete abombado.
fn loaf(crust: &mut Shape, at: Vec3) {
    let (r, h) = (LOAF_RADIUS, LOAF_HEIGHT);
    let profile = [
        (0.0, 0.0),
        (r * 0.92, 0.0),
        (r, h * 0.15),
        (r * 0.98, h * 0.38),
        (r * 0.86, h * 0.66),
        (r * 0.62, h * 0.88),
        (r * 0.32, h * 0.98),
        (0.0, h),
    ];
    crust.lathe(Surface::Lime, Transform::from_translation(at), &profile, 20);
}

/// Una horma de queso apoyada en el origen de `place`: un cilindro bajo con
/// los cantos redondeados.
fn wheel(rind: &mut Shape, place: Transform) {
    let (r, h, round) = (WHEEL_RADIUS, WHEEL_HEIGHT, 0.018);
    let profile = [
        (0.0, 0.0),
        (r - round, 0.0),
        (r - round * 0.3, round * 0.3),
        (r, round),
        (r, h - round),
        (r - round * 0.3, h - round * 0.3),
        (r - round, h),
        (0.0, h),
    ];
    rind.lathe(Surface::Lime, place, &profile, 24);
}

/// Una cuña de queso tendida con su punta en `at`, mirando hacia +X: tapa y
/// base y el lomo curvo con cáscara; las dos caras del corte, de pasta.
fn wedge(rind: &mut Shape, paste: &mut Shape, at: Vec3) {
    let (r, h) = (WHEEL_RADIUS, WHEEL_HEIGHT);
    let arc = 0.7_f32;
    let steps = 6;
    let point = |a: f32, y: f32| at + Vec3::new(r * a.cos(), y, r * a.sin());
    let angles: Vec<f32> = (0..=steps)
        .map(|k| -arc / 2.0 + arc * k as f32 / steps as f32)
        .collect();
    for w in angles.windows(2) {
        let (a, b) = (w[0], w[1]);
        // La tapa y la base.
        rind.face(Surface::Lime, &[at + Vec3::Y * h, point(b, h), point(a, h)]);
        rind.face(Surface::Lime, &[at, point(a, 0.0), point(b, 0.0)]);
        // El lomo.
        rind.face(
            Surface::Lime,
            &[point(a, 0.0), point(a, h), point(b, h), point(b, 0.0)],
        );
    }
    let (first, last) = (angles[0], angles[steps]);
    paste.face(
        Surface::Lime,
        &[at, at + Vec3::Y * h, point(first, h), point(first, 0.0)],
    );
    paste.face(
        Surface::Lime,
        &[at, point(last, 0.0), point(last, h), at + Vec3::Y * h],
    );
}

/// Las meshes y los materiales de un puesto, creados una vez.
struct StallKit {
    wood: Vec<(Surface, Handle<Mesh>)>,
    rind: Vec<Handle<Mesh>>,
    paste: Vec<Handle<Mesh>>,
    salami: Vec<Handle<Mesh>>,
    salami_cut: Vec<Handle<Mesh>>,
    bread: Vec<Handle<Mesh>>,
    rind_material: Handle<StandardMaterial>,
    paste_material: Handle<StandardMaterial>,
    casing_material: Handle<StandardMaterial>,
    cut_material: Handle<StandardMaterial>,
    crust_material: Handle<StandardMaterial>,
}

impl StallKit {
    fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        let shapes = stall_shapes();
        let add = |meshes: &mut Assets<Mesh>, shape: Shape| -> Vec<Handle<Mesh>> {
            shape
                .into_meshes()
                .into_iter()
                .map(|(_, mesh)| meshes.add(mesh))
                .collect()
        };
        let food = |rgb: [f32; 3], roughness: f32| StandardMaterial {
            base_color: Color::srgb(rgb[0], rgb[1], rgb[2]),
            perceptual_roughness: roughness,
            reflectance: 0.3,
            ..default()
        };
        Self {
            wood: shapes
                .wood
                .into_meshes()
                .into_iter()
                .map(|(surface, mesh)| (surface, meshes.add(mesh)))
                .collect(),
            rind: add(meshes, shapes.rind),
            paste: add(meshes, shapes.paste),
            salami: add(meshes, shapes.salami),
            salami_cut: add(meshes, shapes.salami_cut),
            bread: add(meshes, shapes.bread),
            // Cáscara de horma de campo, encerada y dorada por el oreo; la
            // pasta, amarillo claro y algo mate.
            rind_material: materials.add(food([0.74, 0.55, 0.24], 0.55)),
            paste_material: materials.add(food([0.93, 0.85, 0.58], 0.8)),
            // La tripa del salamín, rojo oscuro con la flor blanquecina del
            // estacionado; el corte, rojo carne salpicado de grasa.
            casing_material: materials.add(food([0.36, 0.17, 0.14], 0.75)),
            cut_material: materials.add(food([0.62, 0.24, 0.22], 0.6)),
            // La corteza del pan de campo, dorada y enharinada.
            crust_material: materials.add(food([0.71, 0.49, 0.26], 0.9)),
        }
    }
}

/// Viste cada puesto nuevo con sus piezas, como hijas.
fn dress_stalls(
    mut commands: Commands,
    surfaces: Option<Res<FacadeMaterials>>,
    (mut meshes, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>),
    mut kit: Local<Option<StallKit>>,
    stalls: Query<Entity, Added<CheeseStall>>,
) {
    let Some(surfaces) = surfaces else {
        return;
    };
    if stalls.is_empty() {
        return;
    }
    let kit = kit.get_or_insert_with(|| StallKit::new(&mut meshes, &mut materials));
    for stall in &stalls {
        commands
            .entity(stall)
            .insert(Visibility::default())
            .with_children(|parent| {
                for (surface, mesh) in &kit.wood {
                    parent.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(surfaces.get(*surface))));
                }
                for (meshes, material) in [
                    (&kit.rind, &kit.rind_material),
                    (&kit.paste, &kit.paste_material),
                    (&kit.salami, &kit.casing_material),
                    (&kit.salami_cut, &kit.cut_material),
                    (&kit.bread, &kit.crust_material),
                ] {
                    for mesh in meshes {
                        parent.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone())));
                    }
                }
            });
    }
}

/// Viste los puestos de la vereda ([`CheeseStall`]).
pub struct StallPlugin;

impl Plugin for StallPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, dress_stalls);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points(shape: &Shape) -> Vec<Vec3> {
        shape.surfaces().flat_map(|s| shape.points(s)).collect()
    }

    #[test]
    fn the_stall_is_a_waist_high_trestle_table_with_cheese_on_top() {
        let shapes = stall_shapes();
        let wood = points(&shapes.wood);
        let (half_x, half_z) = (Stall::DEPTH / 2.0, Stall::LENGTH / 2.0);
        // Las puntas de las patas inclinadas se pasan apenas de las cotas.
        let eps = 0.01;
        // Dentro de su huella, del suelo a la tabla, a 80 cm.
        for p in &wood {
            assert!(
                p.x.abs() <= half_x + eps && p.z.abs() <= half_z + eps,
                "{p}"
            );
            assert!(p.y >= -eps && p.y <= Stall::HEIGHT + eps, "{p}");
        }
        let highest = wood.iter().map(|p| p.y).fold(f32::MIN, f32::max);
        assert!((highest - Stall::HEIGHT).abs() < eps, "{highest}");
        assert!(wood.iter().any(|p| p.y.abs() < eps), "legs on the ground");
        let used: Vec<Surface> = shapes.wood.surfaces().collect();
        for surface in [FRAME, TOP, CRATE] {
            assert!(used.contains(&surface), "{surface:?}");
        }
        // Las hormas y la cuña, apoyadas sobre la tabla, sin salirse.
        let cheese: Vec<Vec3> = points(&shapes.rind)
            .into_iter()
            .chain(points(&shapes.paste))
            .collect();
        assert!(!points(&shapes.paste).is_empty(), "the cut shows the paste");
        let lowest = cheese.iter().map(|p| p.y).fold(f32::MAX, f32::min);
        assert!((lowest - Stall::HEIGHT).abs() < eps, "{lowest}");
        for p in &cheese {
            assert!(
                p.x.abs() <= half_x + eps && p.z.abs() <= half_z + eps,
                "{p}"
            );
            assert!(p.y <= Stall::HEIGHT + 2.0 * WHEEL_HEIGHT + eps, "{p}");
        }
    }

    #[test]
    fn beside_the_cheese_lie_salamis_and_country_bread() {
        let shapes = stall_shapes();
        let (half_x, half_z, eps) = (Stall::DEPTH / 2.0, Stall::LENGTH / 2.0, 0.01);
        let cheese: Vec<Vec3> = points(&shapes.rind)
            .into_iter()
            .chain(points(&shapes.paste))
            .collect();
        for (what, goods, tallest) in [
            ("salami", &shapes.salami, 2.0 * SALAMI_RADIUS),
            ("cut salami", &shapes.salami_cut, 2.0 * SALAMI_RADIUS),
            ("bread", &shapes.bread, LOAF_HEIGHT),
        ] {
            let at = points(goods);
            assert!(!at.is_empty(), "no {what}");
            // Apoyados en la tabla, dentro de la mesa.
            let lowest = at.iter().map(|p| p.y).fold(f32::MAX, f32::min);
            assert!((lowest - Stall::HEIGHT).abs() < eps, "{what} at {lowest}");
            for p in &at {
                assert!(
                    p.x.abs() <= half_x - eps && p.z.abs() <= half_z - eps,
                    "{what} {p}"
                );
                assert!(p.y <= Stall::HEIGHT + tallest + eps, "{what} {p}");
            }
            // Sin meterse en las hormas: lejos del eje de cada una.
            for p in &at {
                let inside = cheese.iter().any(|c| {
                    Vec2::new(c.x - p.x, c.z - p.z).length() < 0.02 && (c.y - p.y).abs() < 0.02
                });
                assert!(!inside, "{what} {p} runs into the cheese");
            }
        }
    }
}
