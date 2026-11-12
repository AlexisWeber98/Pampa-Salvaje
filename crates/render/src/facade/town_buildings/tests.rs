use pampa_core::{Aabb, Vec2 as GroundVec2};

use super::pulperia::ROOM_FLOOR;
use super::*;

/// Un lote para un edificio de tipo `kind`, de `w` a lo largo de su frente, `d`
/// de profundidad y `h` de alto, mirando al sur (su propio `+Z`), con semilla
/// `variant`.
fn lot(kind: BuildingKind, (w, d, h): (f32, f32, f32), variant: u32) -> Lot {
    Lot {
        building: Building {
            kind,
            footprint: Aabb::from_center_size(GroundVec2::ZERO, GroundVec2::new(w, d)),
            height: h,
            front: Side::South,
        },
        variant,
        outskirts: false,
        by_rail: false,
        store: StoreStyle::GeneralStore,
        veranda: 3.0,
    }
}

/// Los puntos de todas las superficies de `shape`.
fn points(shape: &Shape) -> Vec<(Surface, Vec3)> {
    shape
        .surfaces()
        .flat_map(|s| shape.points(s).into_iter().map(move |p| (s, p)))
        .collect()
}

/// Los edificios de una calle, a sus tamaños de core, con unas pocas semillas
/// cada uno.
fn street() -> Vec<Lot> {
    let mut lots = Vec::new();
    for v in 0..48u32 {
        let v = v.wrapping_mul(0x9e37_79b9) ^ v;
        lots.push(lot(
            BuildingKind::House,
            (9.0 + (v % 3) as f32, 9.5, 3.8),
            v,
        ));
        lots.push(Lot {
            outskirts: true,
            by_rail: true,
            ..lot(BuildingKind::House, (8.0, 8.5, 3.6), v)
        });
    }
    for store in [StoreStyle::GeneralStore, StoreStyle::Pulperia] {
        for v in 0..6 {
            lots.push(Lot {
                store,
                ..lot(kind_of(store), (16.0, 14.0, 5.0), v)
            });
        }
    }
    lots.push(lot(BuildingKind::PoliceStation, (14.0, 12.0, 4.5), 3));
    lots.push(lot(BuildingKind::Church, (12.0, 22.0, 7.0), 5));
    lots.push(lot(BuildingKind::Bank, BANK, 4));
    lots.push(lot(BuildingKind::TownHall, TOWN_HALL, 6));
    lots
}

/// Un banco y la intendencia a sus tamaños de core en una manzana de 40 m.
const BANK: (f32, f32, f32) = (12.0, 16.0, 6.2);
const TOWN_HALL: (f32, f32, f32) = (20.0, 16.0, 8.0);

/// Los edificios del cuadro de la estación y los galpones del pueblo, a sus
/// tamaños de core: la estación, unos cuantos galpones de chapa y los dos
/// depósitos de granos.
fn yard() -> Vec<Lot> {
    let mut lots = vec![lot(BuildingKind::Station, (24.0, 9.0, 5.0), 0)];
    for v in 0..4 {
        lots.push(lot(BuildingKind::Shed, (25.0, 10.0, 4.5), v));
        lots.push(Lot {
            outskirts: true,
            ..lot(BuildingKind::Shed, (8.5 + v as f32, 9.0, 4.0), v)
        });
    }
    for (length, v) in [(40.0, 0), (35.0, 1)] {
        lots.push(lot(BuildingKind::GrainStore, (length, 12.0, 6.0), v));
    }
    lots
}

#[test]
fn masonry_fronts_stay_inside_their_footprints() {
    // Los colliders son las huellas de edificio y los carteles de fachada
    // cuelgan justo por fuera: ninguna pilastra, cornisa, alféizar, postigo ni
    // alero puede asomar ahí. Solo la galería de un almacén se extiende sobre
    // la vereda, y la marquesina del andén de la estación sobre el andén (la
    // marquesina de core).
    for lot in street().into_iter().chain(yard()) {
        let shape = building(&lot).expect("a building");
        let size = lot.size();
        let (half_w, half_d) = (size.x / 2.0 + 1e-3, size.y / 2.0 + 1e-3);
        let h = lot.building.height;
        for (surface, p) in points(&shape) {
            // La tierra juntada al pie de las paredes es un decal tendido
            // sobre el suelo, no algo con lo que se choque ni que tape un
            // cartel: se aleja de la pared hasta FOOT_GRIME.
            if surface == Surface::Grime && p.y < 0.1 {
                assert!(
                    p.x.abs() <= half_w + FOOT_GRIME && p.z.abs() <= half_d + FOOT_GRIME,
                    "{surface:?} at {p}"
                );
                continue;
            }
            let over_street = p.z > half_d;
            let store = matches!(
                lot.building.kind,
                BuildingKind::GeneralStore | BuildingKind::Pulperia
            );
            if store && over_street {
                // La galería: bajo la cornisa, dentro de la galería de core.
                assert!(p.y < h - 0.5, "{surface:?} at {p} over the sidewalk");
                assert!(p.z < half_d + lot.veranda + 0.1, "{surface:?} at {p}");
                assert!(p.x.abs() < half_w + 0.2, "{surface:?} at {p}");
                continue;
            }
            if lot.building.kind == BuildingKind::Station && p.z < -half_d {
                // La marquesina y sus postes: del lado de la vía, bajo la parte
                // superior de la marquesina, a lo largo de toda la estación.
                let top = Canopy::HEIGHT + Canopy::THICKNESS / 2.0 + 1e-3;
                assert!(p.y <= top, "{surface:?} at {p} over the platform");
                assert!(p.z >= -half_d - Canopy::DEPTH, "{surface:?} at {p}");
                assert!(p.x.abs() <= half_w, "{surface:?} at {p}");
                continue;
            }
            assert!(
                p.x.abs() <= half_w && p.z.abs() <= half_d,
                "{:?} {surface:?} at {p} outside {size}",
                lot.building.kind
            );
        }
    }
}

/// Si un triángulo de `shape` que mira hacia la calle cubre `(x, y)` dentro de
/// `reach` del frente.
fn front_covers(shape: &Shape, front: f32, (x, y): (f32, f32), reach: f32) -> bool {
    shape.surfaces().any(|s| {
        shape.triangles_of(s).into_iter().any(|[a, b, c]| {
            let n = (b - a).cross(c - a);
            if n.z <= 1e-6 || [a, b, c].iter().any(|p| (front - p.z).abs() > reach) {
                return false;
            }
            let (e1, e2, p) = (b - a, c - a, Vec3::new(x, y, 0.0) - a);
            let det = e1.x * e2.y - e2.x * e1.y;
            let u = (p.x * e2.y - e2.x * p.y) / det;
            let v = (e1.x * p.y - p.x * e1.y) / det;
            u >= -1e-4 && v >= -1e-4 && u + v <= 1.0 + 1e-4
        })
    })
}

#[test]
fn a_galpon_wall_stands_where_core_collides() {
    // Core detecta colisiones en las paredes de un galpón, no en su lote: las
    // paredes de chapa deben apoyarse en ese collider, y el techo no debe
    // llegar más lejos que el lote.
    assert!((BARN_REACH - pampa_core::Building::BARN_EAVES).abs() < 1e-6);
    let mut checked = 0;
    for lot in yard() {
        if !matches!(
            lot.building.kind,
            BuildingKind::Shed | BuildingKind::GrainStore
        ) {
            continue;
        }
        let shape = building(&lot).unwrap();
        let collider = lot.building.collider().size() * 0.5;
        let walls: Vec<Vec3> = [Surface::OldIron, Surface::Galvanised]
            .into_iter()
            .flat_map(|s| shape.points(s))
            // Sus pies, bajo el suelo: ningún techo baja hasta ahí.
            .filter(|p| p.y < 0.0)
            .collect();
        let reach = walls
            .iter()
            .fold(Vec2::ZERO, |m, p| m.max(Vec2::new(p.x.abs(), p.z.abs())));
        // Mirando al sur: la `x` del edificio es la del suelo, y `z` es la
        // transversal.
        let want = Vec2::new(collider.x, collider.y);
        assert!(
            (reach - want).abs().max_element() < 0.05,
            "{:?}: walls reach {reach}, collider {want}",
            lot.building.kind
        );
        checked += 1;
    }
    assert!(checked >= 6);
}

#[test]
fn a_store_parapet_stands_solid_behind_its_sign() {
    // `signs` cuelga el cartel del almacén entre 0.2 y 2.0 m por encima de la
    // altura de su pared, casi del ancho del frente: el parapeto debe quedar
    // entero detrás de él.
    for store in [StoreStyle::GeneralStore, StoreStyle::Pulperia] {
        let lot = Lot {
            store,
            ..lot(kind_of(store), (16.0, 14.0, 5.0), 1)
        };
        let shape = building(&lot).unwrap();
        let (w, d, h) = (16.0, 14.0, 5.0);
        for k in 0..=20 {
            let x = -w / 2.0 + 1.0 + (w - 2.0) * k as f32 / 20.0;
            for y in [h + 0.3, h + 1.1, h + 2.1] {
                assert!(
                    front_covers(&shape, d / 2.0, (x, y), 0.6),
                    "{store:?}: nothing behind the sign at ({x}, {y})"
                );
            }
        }
    }
}

#[test]
fn a_bank_and_the_intendencia_stand_solid_behind_their_signs() {
    // `signs` pinta el nombre del banco en su friso, de 10 m de ancho, y el de
    // la intendencia en el ático de su pabellón, de 7 m: detrás de cada cartel
    // hay pared, no una abertura.
    for (kind, (w, d, h), width, (y0, y1)) in [
        (
            BuildingKind::Bank,
            BANK,
            10.0,
            (
                h_of(BANK) - BANK_FRIEZE.0 + 0.1,
                h_of(BANK) - BANK_FRIEZE.1 - 0.1,
            ),
        ),
        (
            BuildingKind::TownHall,
            TOWN_HALL,
            7.0,
            (
                h_of(TOWN_HALL) + 0.2,
                h_of(TOWN_HALL) + TOWN_HALL_ATTIC - 0.1,
            ),
        ),
    ] {
        let shape = building(&lot(kind, (w, d, h), 1)).unwrap();
        for k in 0..=20 {
            let x = -width / 2.0 + width * k as f32 / 20.0;
            for y in [y0, (y0 + y1) / 2.0, y1] {
                assert!(
                    front_covers(&shape, d / 2.0, (x, y), 0.3),
                    "{kind:?}: nothing behind the sign at ({x}, {y})"
                );
            }
        }
    }
}

fn h_of((_, _, h): (f32, f32, f32)) -> f32 {
    h
}

#[test]
fn the_station_canopy_is_the_roof_core_gives_the_camera() {
    let (w, d, h) = (24.0, 9.0, 5.0);
    let shape = building(&lot(BuildingKind::Station, (w, d, h), 0)).unwrap();
    // La marquesina de core se extiende `DEPTH` hacia atrás desde la pared del
    // lado de la vía (la trasera), a lo largo de toda la pared, con su losa de
    // `THICKNESS` de grosor alrededor de `HEIGHT`.
    let slab: Vec<Vec3> = shape
        .points(Surface::Galvanised)
        .into_iter()
        .filter(|p| p.z < -d / 2.0 + 1e-3 && p.y < Canopy::HEIGHT + 0.5)
        .collect();
    let min = slab.iter().fold(Vec3::MAX, |a, p| a.min(*p));
    let max = slab.iter().fold(Vec3::MIN, |a, p| a.max(*p));
    let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
    assert!(close(min.z, -d / 2.0 - Canopy::DEPTH), "{min}");
    assert!(close(max.z, -d / 2.0), "{max}");
    assert!(
        close(min.x, -w / 2.0) && close(max.x, w / 2.0),
        "{min} {max}"
    );
    assert!(
        close(min.y, Canopy::HEIGHT - Canopy::THICKNESS / 2.0),
        "{min}"
    );
    assert!(
        close(max.y, Canopy::HEIGHT + Canopy::THICKNESS / 2.0),
        "{max}"
    );
}

/// La estación del cuadro, a su tamaño de core.
fn station_shape() -> Shape {
    building(&lot(BuildingKind::Station, (24.0, 9.0, 5.0), 0)).unwrap()
}

/// Los puntos de `surface` del lado de la vía de la estación (`z < -d / 2`).
fn track_side(shape: &Shape, surface: Surface) -> Vec<Vec3> {
    shape
        .points(surface)
        .into_iter()
        .filter(|p| p.z < -4.5 - 1e-3)
        .collect()
}

/// La x del pie de cada columna de la marquesina, desde el andén hacia arriba.
fn column_xs(iron: &[Vec3]) -> Vec<f32> {
    let mut xs: Vec<f32> = iron
        .iter()
        .filter(|p| (p.y - PLATFORM_TOP).abs() < 1e-3)
        .map(|p| p.x)
        .collect();
    xs.sort_by(f32::total_cmp);
    // Varios puntos por pie: se conserva uno por columna.
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    xs
}

#[test]
fn the_platform_top_is_the_height_core_gives_its_platform() {
    let town = pampa_core::TownLayout::castex_inspired();
    let mut platforms = 0;
    for platform in town
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::Platform)
    {
        assert_eq!(platform.height, PLATFORM_TOP, "{}", town.name);
        platforms += 1;
    }
    assert!(platforms > 0);
}

/// Qué profundidad le da core al andén de una estación, desde la pared trasera
/// de la estación hasta el borde de la vía (su lado corto; se extiende a lo
/// largo de la vía).
fn platform_depth() -> f32 {
    let town = pampa_core::TownLayout::castex_inspired();
    let platform = town
        .buildings
        .iter()
        .find(|b| b.kind == BuildingKind::Platform)
        .unwrap();
    let size = platform.footprint.size();
    size.x.min(size.y)
}

#[test]
fn the_canopy_columns_stand_on_the_platform_not_in_it() {
    let (w, d) = (24.0, 9.0);
    let shape = station_shape();
    let iron = track_side(&shape, Surface::Steel);
    // Nada de la herrería de la marquesina llega por debajo de la parte
    // superior del andén, y el pie de cada columna se apoya en él.
    let lowest = iron.iter().fold(f32::MAX, |a, p| a.min(p.y));
    assert!(
        (lowest - PLATFORM_TOP).abs() < 1e-3,
        "lowest iron at {lowest}"
    );
    let feet = column_xs(&iron);
    assert!(feet.len() >= 6, "{} columns", feet.len());
    // Espaciadas de forma pareja, de tres a cuatro metros entre sí, dentro del
    // ancho de la estación.
    let gaps: Vec<f32> = feet.windows(2).map(|p| p[1] - p[0]).collect();
    for gap in &gaps {
        assert!((3.0..=4.0).contains(gap), "gap {gap}");
        assert!((gap - gaps[0]).abs() < 0.2, "uneven {gaps:?}");
    }
    assert!(feet[0] > -w / 2.0 && feet[feet.len() - 1] < w / 2.0);
    // Sobre el andén: se extiende desde la pared trasera de la estación con la
    // profundidad que le da core, y las filas de columnas quedan bien adentro.
    let depth = platform_depth();
    for p in iron.iter().filter(|p| (p.y - PLATFORM_TOP).abs() < 1e-3) {
        assert!(p.z > -d / 2.0 - depth + 0.5 && p.z < -d / 2.0, "{p}");
    }
}

#[test]
fn the_canopy_wears_a_fascia_a_timber_valance_and_iron_brackets() {
    let shape = station_shape();
    let bottom = Canopy::HEIGHT - Canopy::THICKNESS / 2.0;
    let z0 = -4.5 - Canopy::DEPTH;
    let timber = track_side(&shape, Surface::Planks);
    // Una tabla de frente en el borde de la vía, y un faldón colgando bajo la
    // losa, todavía muy por encima de la cabeza de una persona en el andén.
    let hanging: Vec<&Vec3> = timber.iter().filter(|p| p.y < bottom - 0.2).collect();
    assert!(hanging.len() > 100, "{} valance points", hanging.len());
    assert!(
        hanging.iter().all(|p| p.z < z0 + 0.2),
        "valance on the edge"
    );
    let low = hanging.iter().fold(f32::MAX, |a, p| a.min(p.y));
    assert!(low > PLATFORM_TOP + 2.0, "valance at {low}");
    assert!(
        timber
            .iter()
            .any(|p| (p.y - (bottom + Canopy::THICKNESS)).abs() < 1e-3)
    );
    // Ménsulas: de hierro bajo la losa, junto a cada columna a lo largo del
    // borde del techo y hacia atrás, hacia la pared de la estación (una columna
    // mide 0.2 m de ancho).
    let iron = track_side(&shape, Surface::Steel);
    let columns = column_xs(&iron);
    let zc = z0 + 0.4;
    let under = |p: &&Vec3| p.y > bottom - 0.8 && p.y < bottom;
    let beside = iron
        .iter()
        .filter(under)
        .filter(|p| columns.iter().all(|x| (p.x - x).abs() > 0.3))
        .count();
    let toward_wall = iron.iter().filter(under).filter(|p| p.z > zc + 0.3).count();
    assert!(beside > 100, "{beside} side bracket points");
    assert!(toward_wall > 50, "{toward_wall} wall bracket points");
}

#[test]
fn the_station_has_a_pedimented_pavilion_on_both_fronts() {
    let (d, h) = (9.0, 5.0);
    let shape = station_shape();
    let above = |sign: f32| {
        shape
            .points(Surface::Stucco)
            .into_iter()
            .filter(|p| p.y > h + 1.2 && p.z * sign > 0.0)
            .collect::<Vec<_>>()
    };
    for sign in [1.0, -1.0] {
        let pediment = above(sign);
        assert!(pediment.len() > 8, "no pediment at {sign}");
        // Sobre el centro, más angosto que el frente entero, dentro de la
        // huella de edificio.
        let (lo, hi) = pediment
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
        assert!(lo > -8.0 && hi < 8.0 && hi - lo > 6.0, "{lo}..{hi}");
        // El vértice queda muy por encima de los aleros, sobre el centro.
        let apex = pediment.iter().fold(f32::MIN, |a, p| a.max(p.y));
        assert!(apex > h + 2.2, "apex {apex}");
        assert!(pediment.iter().all(|p| p.z.abs() <= d / 2.0 + 1e-3));
    }
}

#[test]
fn the_civic_buildings_are_kept_better_than_the_pulperia_and_the_outskirts() {
    // De la semilla, solo los bits altos mueven el desgaste dentro de su
    // rango: se prueban sus extremos.
    let range = |lot: Lot| {
        let wears: Vec<f32> = [0, 50 << 16, 100 << 16]
            .into_iter()
            .map(|v| wear(&Lot { variant: v, ..lot }).expect("a weathering building"))
            .collect();
        (wears[0], wears[2])
    };
    let size = (12.0, 12.0, 5.0);
    let bank = range(lot(BuildingKind::Bank, size, 0));
    let hall = range(lot(BuildingKind::TownHall, size, 0));
    let church = range(lot(BuildingKind::Church, size, 0));
    let police = range(lot(BuildingKind::PoliceStation, size, 0));
    let pulperia = range(Lot {
        store: StoreStyle::Pulperia,
        ..lot(BuildingKind::Pulperia, size, 0)
    });
    let outskirts = range(Lot {
        outskirts: true,
        ..lot(BuildingKind::House, size, 0)
    });
    for kept in [bank, hall] {
        for worn in [church, police] {
            assert!(kept.1 < worn.0, "{kept:?} vs {worn:?}");
        }
    }
    for worn in [church, police] {
        for older in [pulperia, outskirts] {
            assert!(worn.1 < older.0, "{worn:?} vs {older:?}");
        }
    }
    // Ninguno es una ruina.
    assert!(pulperia.1 <= 1.0 && outskirts.1 <= 1.0);
    for iron in [BuildingKind::WaterTank, BuildingKind::Windmill] {
        assert_eq!(wear(&lot(iron, size, 0)), None);
    }
}

#[test]
fn every_building_type_wears_its_own_finish_and_weathers() {
    let shape = |lot: Lot| building(&lot).expect("a building");
    let has = |shape: &Shape, surface: Surface| shape.surfaces().any(|s| s == surface);
    for (kind, size, wall) in [
        (BuildingKind::Bank, BANK, Surface::Ashlar),
        (
            BuildingKind::TownHall,
            TOWN_HALL,
            Surface::Render(Paint::Ochre),
        ),
        (
            BuildingKind::Church,
            (12.0, 22.0, 7.0),
            Surface::Render(Paint::LimeWhite),
        ),
        (
            BuildingKind::PoliceStation,
            (14.0, 12.0, 4.5),
            Surface::LimeBrick,
        ),
        (
            BuildingKind::Station,
            (24.0, 9.0, 5.0),
            Surface::StationBrick,
        ),
    ] {
        let s = shape(lot(kind, size, 3));
        assert!(has(&s, wall), "{kind:?} in {wall:?}");
        // Sobre un zócalo de cemento manchado, y gastado.
        assert!(has(&s, Surface::Plinth), "{kind:?}");
        let meshes = s.into_meshes();
        assert!(
            meshes
                .iter()
                .all(|(_, m)| crate::facade::materials::weathers(m)),
            "{kind:?}"
        );
    }
    // Una pulpería de cal vieja, de las que caen en ella.
    let pulperias: Vec<Shape> = (0..6)
        .map(|v| {
            shape(Lot {
                store: StoreStyle::Pulperia,
                ..lot(BuildingKind::Pulperia, (16.0, 14.0, 5.0), v)
            })
        })
        .collect();
    assert!(pulperias.iter().any(|s| has(s, Surface::OldLime)));
    // Persianas y puertas pintadas en los frentes del pueblo; la madera al
    // natural en la estación.
    let police = shape(lot(BuildingKind::PoliceStation, (14.0, 12.0, 4.5), 3));
    assert!(has(&police, Surface::Shutter) && !has(&police, Surface::Planks));
    let station = shape(lot(BuildingKind::Station, (24.0, 9.0, 5.0), 0));
    assert!(!has(&station, Surface::Shutter));
}

#[test]
fn every_front_door_has_a_threshold_and_every_window_a_vent() {
    let police = building(&lot(BuildingKind::PoliceStation, (14.0, 12.0, 4.5), 3)).unwrap();
    // El frente mira a +Z; su cara está a `d / 2 - PROUD`.
    let face = 12.0 / 2.0 - PROUD;
    let low = |surface: Surface, top: f32| -> Vec<Vec3> {
        police
            .points(surface)
            .into_iter()
            .filter(|p| p.z > face + 0.03 && p.y <= top + 1e-4)
            .collect()
    };
    let step = low(Surface::Plinth, THRESHOLD.1);
    assert!(
        step.iter().any(|p| (p.z - face - THRESHOLD.0).abs() < 1e-4),
        "a threshold out in front of the door"
    );
    let vents = low(Surface::RailSteel, 0.33);
    assert!(
        vents.iter().any(|p| (p.z - face - VENT_OUT).abs() < 1e-4),
        "a vent proud of the plinth"
    );
}

/// Una casilla: paredes y techo de chapa, sin revoque ni ladrillo.
fn casilla(shape: &Shape) -> bool {
    let all: Vec<Surface> = shape.surfaces().collect();
    all.contains(&Surface::Barnwood)
        && !all.iter().any(|x| {
            matches!(
                x,
                Surface::Adobe
                    | Surface::Whitewash
                    | Surface::WashedBrick
                    | Surface::OldLime
                    | Surface::Weatherboard
                    | Surface::MudStraw
            )
        })
}

#[test]
fn the_outskirts_have_casillas_only_by_the_railway_and_few_wooden_ranchos() {
    // La mitad lejos de la vía.
    let outskirts: Vec<(Lot, Shape)> = (0..200u32)
        .map(|i| {
            let v = i.wrapping_mul(0x9e37_79b9) ^ i;
            let lot = Lot {
                outskirts: true,
                by_rail: i % 2 == 0,
                ..lot(BuildingKind::House, (8.0, 8.5, 3.6), v)
            };
            (lot, building(&lot).unwrap())
        })
        .collect();
    for (lot, shape) in &outskirts {
        assert!(lot.by_rail || !casilla(shape), "{lot:?}");
    }
    let wooden = outskirts
        .iter()
        .filter(|(_, s)| s.surfaces().any(|x| x == Surface::Weatherboard))
        .count();
    assert!(
        wooden * 10 <= outskirts.len(),
        "{wooden} of {}",
        outskirts.len()
    );
}

#[test]
fn the_outskirts_have_wooden_ranchos_casillas_and_mud_and_straw_ranchos() {
    // Los ranchos de madera son raros: hacen falta más semillas que en
    // `street`. Todos junto a la vía, para que también haya casillas.
    let outskirts: Vec<Shape> = (0..200u32)
        .map(|v| {
            let v = v.wrapping_mul(0x9e37_79b9) ^ v;
            building(&Lot {
                outskirts: true,
                by_rail: true,
                ..lot(BuildingKind::House, (8.0, 8.5, 3.6), v)
            })
            .unwrap()
        })
        .collect();
    for wall in [Surface::Weatherboard, Surface::MudStraw] {
        let n = outskirts
            .iter()
            .filter(|s| s.surfaces().any(|x| x == wall))
            .count();
        assert!(n >= 1, "{wall:?}");
    }
    let casillas = outskirts.iter().filter(|s| casilla(s)).count();
    assert!(casillas >= 1);
    // Siguen siendo pocos: la mayoría de las afueras sigue de adobe o cal.
    let adobe = outskirts
        .iter()
        .filter(|s| s.surfaces().any(|x| x == Surface::Adobe))
        .count();
    assert!(
        adobe > outskirts.len() / 5,
        "{adobe} of {}",
        outskirts.len()
    );
}

#[test]
fn town_buildings_keep_to_their_triangle_budgets() {
    for lot in street() {
        let tris = building(&lot).unwrap().triangles();
        if lot.building.kind == BuildingKind::GeneralStore {
            // Su sala, aparte: estantes hasta el techo con mercadería en dos
            // paredes, el mostrador largo con balanza y vitrina, bolsas,
            // barriles, monturas, herramientas y el farol; 18.9k a 22.4k al
            // escribir esto, casi todo en la mercadería de los estantes.
            let room = interior(&lot).map_or(0, |s| s.triangles());
            assert!(
                (12000..=24000).contains(&room),
                "almacén {:#x}: {room} triangles inside",
                lot.variant
            );
        }
        if lot.building.kind == BuildingKind::Pulperia {
            // Su sala amueblada, aparte: mostrador con reja, estantes con
            // botellas, tres mesas, seis bancos, dos barriles, el farol, y la
            // caja registradora, vasos, botellas, frascos y naipes encima;
            // 8.2k a 9.7k al escribir esto, casi la mitad en botellas.
            let room = interior(&lot).map_or(0, |s| s.triangles());
            assert!(
                (4000..=12000).contains(&room),
                "pulpería {:#x}: {room} triangles inside",
                lot.variant
            );
        }
        let budget = match lot.building.kind {
            BuildingKind::Church => 2000..=6000,
            // Con persianas de tablillas, rejillas de hierro, puertas de
            // tableros y la mugre en decals: 5.5k la comisaría al escribir
            // esto.
            BuildingKind::GeneralStore | BuildingKind::Pulperia | BuildingKind::PoliceStation => {
                1500..=7000
            }
            // Almohadillado, dentículos, arcos y balaustrada: dos o tres por
            // pueblo, 4.3k al escribir esto.
            BuildingKind::Bank => 3000..=7000,
            // Una sola en el mundo, con su pabellón y su reloj: 7.4k.
            BuildingKind::TownHall => 5000..=10000,
            _ if lot.outskirts => 100..=4000,
            _ => 800..=4000,
        };
        assert!(
            budget.contains(&tris),
            "{:?} (variant {:#x}, outskirts {}): {tris} triangles",
            lot.building.kind,
            lot.variant,
            lot.outskirts
        );
    }
    // La estación y su marquesina (ocho columnas torneadas, un faldón de
    // rombos, dos frentes ornamentados): un hito, construido una vez por
    // pueblo, así que más costoso que un almacén; 7.2k al momento de escribir
    // esto; 10.1k con sus puertas de tableros, rejillas y chorreados.
    let tris = station_shape().triangles();
    assert!((3000..=11000).contains(&tris), "station: {tris} triangles");
}

#[test]
fn town_houses_vary_their_wash_roof_and_dressing() {
    let lots: Vec<Lot> = street()
        .into_iter()
        .filter(|l| l.building.kind == BuildingKind::House && !l.outskirts)
        .collect();
    // Cada encalado, pintura y ladrillo aparece en algún frente, y los pintados
    // por encima de la franja beige.
    // Cada nibble bajo (el acabado), con los demás bits mezclados.
    let seeded: Vec<Lot> = (0..64u32)
        .map(|v| v & 0xf | (v.wrapping_mul(0x9e37_79b9) & !0xf))
        .map(|v| lot(BuildingKind::House, (10.0, 9.5, 3.8), v))
        .collect();
    let worn: std::collections::BTreeSet<Surface> = seeded
        .iter()
        .flat_map(|l| building(l).unwrap().surfaces().collect::<Vec<_>>())
        .collect();
    let finishes = [
        Surface::Whitewash,
        Surface::Lime,
        Surface::Brick,
        Surface::WashedBrick,
        Surface::PatchedBrick,
        Surface::Band,
    ]
    .into_iter()
    .chain(Paint::ALL.map(Surface::Painted));
    for finish in finishes {
        assert!(worn.contains(&finish), "no front wears {finish:?}");
    }
    // Un pueblo joven: el revoque descascarándose del ladrillo es el frente más
    // raro.
    let fronts: Vec<Surface> = seeded.iter().map(|l| house_finish(l.variant).0).collect();
    let patched = fronts
        .iter()
        .filter(|&&s| s == Surface::PatchedBrick)
        .count();
    let painted = fronts
        .iter()
        .filter(|s| matches!(s, Surface::Painted(_)))
        .count();
    assert!(8 * patched <= fronts.len(), "{patched} of {}", fronts.len());
    assert!(
        painted > patched && 2 * painted < fronts.len(),
        "{painted} painted"
    );
    let iron = lots
        .iter()
        .filter(|l| {
            let shape = building(l).unwrap();
            shape
                .surfaces()
                .any(|s| s == Surface::OldIron || s == Surface::Galvanised)
        })
        .count();
    assert!(iron > 0 && iron < lots.len(), "{iron} of {}", lots.len());
    // Las casas de las afueras son en su mayoría casitas simples bajo chapa.
    let plain = street()
        .into_iter()
        .filter(|l| l.outskirts)
        .filter(|l| {
            !building(l)
                .unwrap()
                .surfaces()
                .any(|s| s == Surface::Stucco)
        })
        .count();
    assert!(plain > 24, "{plain} plain outskirts houses of 48");
}

/// Escribe la calle de vista previa en `PAMPA_PREVIEW` (una carpeta) para el
/// renderizador offline: `m <surface>` por mesh, después líneas
/// `v x y z nx ny nz u v` y líneas `f a b c`. Se ejecuta con
/// `PAMPA_PREVIEW=/some/dir cargo test -p pampa-render -- --ignored preview_town`.
#[test]
#[ignore = "writes preview files"]
fn preview_town_street() {
    use std::fmt::Write as _;
    let Some(dir) = std::env::var_os("PAMPA_PREVIEW") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let pulperia = Lot {
        store: StoreStyle::Pulperia,
        ..lot(BuildingKind::Pulperia, (14.0, 14.0, 5.0), 2)
    };
    // Una casa de cada acabado nuevo: las cinco pinturas, el ladrillo encalado
    // y el revoque descascarándose del ladrillo (el acabado es `variant % 16`).
    let finishes: Vec<Lot> = [6u32, 7, 9, 10, 11, 12, 14]
        .into_iter()
        .map(|nibble| lot(BuildingKind::House, (9.5, 9.5, 4.0), 0x0000_0080 | nibble))
        .collect();
    let streets: [(&str, Vec<Lot>); 4] = [
        (
            "street_a",
            vec![
                lot(BuildingKind::House, (10.0, 9.5, 3.9), 0x0000_0080),
                lot(BuildingKind::GeneralStore, (16.0, 14.0, 5.0), 1),
                lot(BuildingKind::House, (9.0, 9.5, 4.2), 0x0000_4782),
                lot(BuildingKind::PoliceStation, (14.0, 12.0, 4.5), 3),
                lot(BuildingKind::House, (10.0, 9.5, 3.7), 0x0000_2b03),
            ],
        ),
        (
            "street_b",
            vec![
                lot(BuildingKind::House, (9.5, 9.5, 4.0), 0x0000_0791),
                lot(BuildingKind::Church, (12.0, 22.0, 7.0), 5),
                lot(BuildingKind::House, (10.0, 9.0, 3.8), 0x0000_6c04),
                pulperia,
                Lot {
                    outskirts: true,
                    ..lot(BuildingKind::House, (8.0, 8.5, 3.6), 2)
                },
                Lot {
                    outskirts: true,
                    ..lot(BuildingKind::House, (8.0, 8.5, 3.6), 5)
                },
            ],
        ),
        (
            "street_c",
            vec![
                lot(BuildingKind::Station, (24.0, 9.0, 5.0), 0),
                lot(BuildingKind::Shed, (12.0, 10.0, 4.5), 1),
                lot(BuildingKind::GrainStore, (40.0, 12.0, 6.0), 2),
            ],
        ),
        ("street_d", finishes),
    ];
    for (name, lots) in streets {
        let mut text = String::new();
        let mut x = 0.0;
        for lot in &lots {
            let size = lot.size();
            let offset = Vec3::new(x + size.x / 2.0, 0.0, -size.y / 2.0);
            x += size.x + 1.5;
            let shape = building(lot).unwrap();
            for surface in shape.surfaces().collect::<Vec<_>>() {
                let _ = writeln!(text, "m {surface:?}");
                for (p, n, uv) in shape.vertices(surface) {
                    let p = p + offset;
                    let _ = writeln!(
                        text,
                        "v {:.4} {:.4} {:.4} {:.3} {:.3} {:.3} {:.4} {:.4}",
                        p.x, p.y, p.z, n.x, n.y, n.z, uv.x, uv.y
                    );
                }
                for t in shape.indices(surface).chunks(3) {
                    let _ = writeln!(text, "f {} {} {}", t[0], t[1], t[2]);
                }
            }
        }
        std::fs::write(dir.join(format!("{name}.txt")), text).expect("preview written");
    }
}

/// Si hay algo de `surface` en el vano de la puerta del frente centrada en `x`
/// (con la cara del frente en `face`): entre sus jambas, a la altura de una
/// persona (bajo la banderola) y dentro del espesor de la pared.
fn fills_door(shape: &Shape, surface: Surface, x: f32, face: f32) -> bool {
    shape.points(surface).iter().any(|p| {
        (p.x - x).abs() < 0.66 && p.y > -0.01 && p.y < 2.5 && p.z < face + 0.05 && p.z > face - 0.46
    })
}

#[test]
fn a_shops_middle_door_stands_open_and_every_other_door_keeps_its_leaves() {
    // Un frente de 16 m: cinco vanos, puertas en el 0, el 2 y el 4.
    let face = 14.0 / 2.0 - PROUD;
    let side = (16.0 - 2.0 * PROUD) * 2.0 / 5.0;
    for v in 0..6 {
        for store in [StoreStyle::GeneralStore, StoreStyle::Pulperia] {
            let shape = building(&Lot {
                store,
                ..lot(kind_of(store), (16.0, 14.0, 5.0), v)
            })
            .unwrap();
            // Se entra a los dos por la puerta del medio.
            for surface in [Surface::Recess, Surface::Door] {
                assert!(
                    !fills_door(&shape, surface, 0.0, face),
                    "{store:?} {v}: {surface:?} in the middle door"
                );
                for x in [-side, side] {
                    assert!(
                        fills_door(&shape, surface, x, face),
                        "{store:?} {v}: {surface:?} in the door at {x}"
                    );
                }
            }
            // El umbral de piedra sigue en su lugar.
            assert!(
                shape.points(Surface::Plinth).iter().any(|p| p.x.abs() < 0.8
                    && p.y <= THRESHOLD.1 + 1e-4
                    && (p.z - face - THRESHOLD.0).abs() < 1e-4),
                "{store:?} {v}: the middle threshold"
            );
        }
    }
}

/// Una pulpería de `w` por `d` mirando a `front`, con la semilla `v`.
fn pulperia_lot((w, d): (f32, f32), front: Side, v: u32) -> Lot {
    let size = match front {
        Side::North | Side::South => GroundVec2::new(w, d),
        Side::East | Side::West => GroundVec2::new(d, w),
    };
    let mut lot = Lot {
        store: StoreStyle::Pulperia,
        ..lot(BuildingKind::Pulperia, (w, d, 5.0), v)
    };
    lot.building.footprint = Aabb::from_center_size(GroundVec2::ZERO, size);
    lot.building.front = front;
    lot
}

/// La madera con la que se hacen los muebles de la pulpería.
const FURNITURE_WOOD: [Surface; 4] = [
    Surface::Door,
    Surface::Planks,
    Surface::Sleeper,
    Surface::Barnwood,
];

#[test]
fn a_pulperia_is_furnished_where_core_puts_its_furniture() {
    use pampa_core::pulperia::{FurnitureKind, Interior};
    for (w, d) in [(16.0, 14.0), (14.0, 14.0), (20.0, 16.0)] {
        // Mirando al sur, el `+Z` propio es el sur: el norte de core (`+y`)
        // queda en `-Z`.
        let lot = pulperia_lot((w, d), Side::South, 3);
        let room = Interior::of(&lot.building);
        let inside = interior(&lot).expect("a pulpería is furnished");
        let local = |a: Aabb| (Vec2::new(a.min.x, -a.max.y), Vec2::new(a.max.x, -a.min.y));
        let (lo, hi) = local(room.floor);
        // Todo queda en la sala, entre las caras de adentro de las paredes y
        // bajo el techo.
        for (surface, p) in points(&inside) {
            assert!(
                p.x >= lo.x - 1e-3
                    && p.x <= hi.x + 1e-3
                    && p.z >= lo.y - 1e-3
                    && p.z <= hi.y + 1e-3,
                "{surface:?} at {p} outside the room"
            );
            assert!(
                p.y >= 0.0 && p.y <= lot.building.height,
                "{surface:?} at {p}"
            );
        }
        let wood: Vec<Vec3> = FURNITURE_WOOD
            .iter()
            .flat_map(|&s| inside.points(s))
            .collect();
        let mut kinds = Vec::new();
        for f in &room.furniture {
            let (lo, hi) = local(f.footprint);
            let over = |p: &&Vec3| {
                p.x > lo.x - 0.02 && p.x < hi.x + 0.02 && p.z > lo.y - 0.02 && p.z < hi.y + 0.02
            };
            // Su tabla de arriba, a la altura que core le da.
            assert!(
                wood.iter()
                    .filter(over)
                    .any(|p| (p.y - ROOM_FLOOR - f.height).abs() < 0.03),
                "{w}x{d}: no top for {f:?}"
            );
            // Y de ese mueble, nada de madera más arriba (la reja del
            // mostrador es de hierro y las botellas, de vidrio).
            if f.kind != FurnitureKind::Shelves {
                let top = wood.iter().filter(over).map(|p| p.y).fold(0.0f32, f32::max);
                if top > ROOM_FLOOR + f.height + 0.03 {
                    // Solo puede ser el cielorraso, sobre todo.
                    assert!(
                        top > lot.building.height - 0.5,
                        "{w}x{d}: {f:?} reaches {top}"
                    );
                }
            }
            kinds.push(f.kind);
        }
        // La reja del mostrador sube del mostrador hacia el techo.
        let counter = room
            .furniture
            .iter()
            .find(|f| f.kind == FurnitureKind::Counter)
            .unwrap();
        let (clo, chi) = local(counter.footprint);
        assert!(
            inside.points(Surface::Steel).iter().any(|p| p.x > clo.x
                && p.x < chi.x
                && p.z > clo.y
                && p.z < chi.y
                && p.y > 2.5),
            "{w}x{d}: the grille over the counter"
        );
        assert!(inside.surfaces().any(|s| s == Surface::Glass), "bottles");
        assert!(kinds.contains(&FurnitureKind::Barrel));
    }
}

#[test]
fn a_pulperia_is_the_same_room_whichever_way_it_faces() {
    let south = interior(&pulperia_lot((16.0, 14.0), Side::South, 1)).unwrap();
    for front in [Side::North, Side::East, Side::West] {
        let turned = interior(&pulperia_lot((16.0, 14.0), front, 1)).unwrap();
        for surface in south.surfaces() {
            let (a, b) = (south.points(surface), turned.points(surface));
            assert_eq!(a.len(), b.len(), "{front:?} {surface:?}");
            for (p, q) in a.iter().zip(&b) {
                assert!(p.distance(*q) < 1e-3, "{front:?} {surface:?}: {p} vs {q}");
            }
        }
    }
}

#[test]
fn only_the_pulperia_and_the_almacen_have_an_interior() {
    for lot in street().into_iter().chain(yard()) {
        let walks_in = matches!(
            lot.building.kind,
            BuildingKind::Pulperia | BuildingKind::GeneralStore
        );
        assert_eq!(
            interior(&lot).is_some(),
            walks_in,
            "{:?}",
            lot.building.kind
        );
    }
}

/// La caja que encierra `points`.
fn bounds(points: &[Vec3]) -> (Vec3, Vec3) {
    points.iter().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    )
}

#[test]
fn the_pulperias_leaves_close_its_middle_door_like_the_others_and_swing_clear_of_it() {
    use super::super::italianate::FANLIGHT;
    // Un frente de 16 m: la puerta del medio, de 1,3 m, en `x = 0`.
    let face = 14.0 / 2.0 - PROUD;
    let back = 0.45 * 0.5;
    let top = (5.0f32 - 1.3).min(3.4) - FANLIGHT;
    // La puerta de la derecha, del mismo ancho, en el vano 4 de cinco.
    let side = (16.0 - 2.0 * PROUD) * 2.0 / 5.0;
    for v in 0..6 {
        let lot = pulperia_lot((16.0, 14.0), Side::South, v);
        let general = building(&Lot {
            store: StoreStyle::GeneralStore,
            ..lot
        })
        .unwrap();
        let doors = general.points(Surface::Door);
        let leaves = leaves(&lot).expect("a pulpería has its leaves");
        let mut shut = Vec::new();
        for leaf in &leaves {
            let points = leaf.shape.points(Surface::Door);
            assert!(!points.is_empty());
            // Las mismas hojas que las de la puerta de la derecha.
            for p in &points {
                let there = *p + Vec3::X * side;
                assert!(
                    doors.iter().any(|q| q.distance(there) < 1e-4),
                    "{v}: {p} is not a door leaf"
                );
            }
            shut.extend(points);
        }
        // Cerradas, de jamba a jamba y del umbral a la banderola.
        let (lo, hi) = bounds(&shut);
        assert!(
            (lo.x + 0.65).abs() < 1e-3 && (hi.x - 0.65).abs() < 1e-3,
            "{v}"
        );
        assert!(lo.y.abs() < 1e-3 && (hi.y - top).abs() < 1e-3, "{v}");
        // Abiertas, de canto contra sus jambas, del lado de la sala.
        for leaf in &leaves {
            let pose = leaf.hinge.pose(1.0);
            for p in leaf.shape.points(Surface::Door) {
                let q = pose.transform_point(p);
                assert!(q.x.abs() > 0.65 - 0.07, "{v}: open leaf in the door at {q}");
                assert!(
                    q.z < face - back + 1e-3 && q.z > face - back - 0.7,
                    "{v}: open leaf at {q}"
                );
            }
        }
    }
    // El almacén también se abre; el banco no tiene hojas que se muevan.
    let store = lot(BuildingKind::GeneralStore, (16.0, 14.0, 5.0), 1);
    assert!(leaves(&store).is_some(), "an almacén swings its leaves too");
    assert!(leaves(&lot(BuildingKind::Bank, BANK, 4)).is_none());
}

#[test]
fn the_lamps_flame_burns_on_its_brass_font_inside_its_glass_chimney() {
    for front in [Side::South, Side::East] {
        let lot = pulperia_lot((16.0, 14.0), front, 2);
        let flame = lamp_flame(&lot).expect("a pulpería has its lamp");
        let room = interior(&lot).unwrap();
        let near = |p: &Vec3| Vec2::new(p.x - flame.x, p.z - flame.z).length() < 0.1;
        let font: Vec<Vec3> = room
            .points(Surface::Bronze)
            .into_iter()
            .filter(|p| near(p) && p.y > flame.y - 0.3)
            .collect();
        assert!(!font.is_empty(), "{front:?}: the font under the flame");
        let (_, hi) = bounds(&font);
        assert!(
            (hi.y - flame.y).abs() < 0.01,
            "{front:?}: the wick at {}",
            hi.y
        );
        let tube = lamp_chimney().points(Surface::Glass);
        let (lo, hi) = bounds(&tube);
        assert!(lo.y < 0.0 && hi.y > 0.1 && lo.x > -0.05 && hi.x < 0.05);
    }
    assert!(lamp_flame(&lot(BuildingKind::GeneralStore, (16.0, 14.0, 5.0), 1)).is_some());
    assert!(lamp_flame(&lot(BuildingKind::Bank, (16.0, 14.0, 6.2), 1)).is_none());
}

/// El tipo de edificio de un comercio de estilo `store`.
fn kind_of(store: StoreStyle) -> BuildingKind {
    match store {
        StoreStyle::Pulperia => BuildingKind::Pulperia,
        StoreStyle::GeneralStore => BuildingKind::GeneralStore,
    }
}

#[test]
fn the_almacens_shelves_reach_the_ceiling_and_its_counter_has_no_grille() {
    for v in 0..6 {
        let almacen = lot(BuildingKind::GeneralStore, (16.0, 14.0, 5.0), v);
        let pulperia = pulperia_lot((16.0, 14.0), Side::South, v);
        let high = |lot: &Lot, surface| {
            let room = interior(lot).unwrap();
            room.points(surface)
                .into_iter()
                .filter(|p| p.y > 3.0)
                .collect::<Vec<_>>()
        };
        // La cornisa pintada de los estantes, arriba de todo; la pulpería no
        // tiene nada pintado tan alto.
        assert!(!high(&almacen, Surface::Door).is_empty(), "{v}");
        assert!(high(&pulperia, Surface::Door).is_empty(), "{v}");
        // De hierro tan alto, solo la varilla y los brazos del farol:
        // ninguna reja.
        let flame = lamp_flame(&almacen).unwrap();
        for p in high(&almacen, Surface::Steel) {
            let off = Vec2::new(p.x - flame.x, p.z - flame.z).length();
            assert!(off < 0.3, "{v}: iron at {p}");
        }
        assert!(
            high(&pulperia, Surface::Steel).len() > 20,
            "{v}: the pulpería keeps its grille"
        );
    }
}

#[test]
fn the_pulperias_counter_and_tables_are_set_with_a_register_glasses_and_cards() {
    use pampa_core::pulperia::{FurnitureKind, Interior};
    let mut cards = 0;
    for v in 0..6 {
        let lot = pulperia_lot((16.0, 14.0), Side::South, v);
        let room = Interior::of(&lot.building);
        let inside = interior(&lot).unwrap();
        let local = |a: Aabb| (Vec2::new(a.min.x, -a.max.y), Vec2::new(a.max.x, -a.min.y));
        let on = |f: &pampa_core::pulperia::Furniture, surface: Surface| {
            let (lo, hi) = local(f.footprint);
            let top = ROOM_FLOOR + f.height;
            inside
                .points(surface)
                .into_iter()
                .filter(|p| {
                    p.x > lo.x && p.x < hi.x && p.z > lo.y && p.z < hi.y && p.y > top - 1e-3
                })
                .count()
        };
        let counter = room
            .furniture
            .iter()
            .find(|f| f.kind == FurnitureKind::Counter)
            .unwrap();
        // La caja registradora de bronce y los vasos, sobre el mostrador.
        assert!(on(counter, Surface::Bronze) > 0, "{v}: no register");
        assert!(on(counter, Surface::Glass) > 0, "{v}: no glasses");
        // Los naipes, en la mesa del truco que eligió el interior y en
        // ninguna otra: ahí se sientan los que juegan.
        for (index, table) in room
            .furniture
            .iter()
            .filter(|f| f.kind == FurnitureKind::Table)
            .enumerate()
        {
            assert!(on(table, Surface::Glass) > 0, "{v}: a bare table");
            let dealt = on(table, Surface::Painted(Paint::LimeWhite)) > 0;
            assert_eq!(dealt, room.card_table == Some(index), "{v}: table {index}");
            cards += usize::from(dealt);
        }
    }
    assert_eq!(cards, 6, "someone plays truco in every pulpería");
}
