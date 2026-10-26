use pampa_core::{Aabb, Vec2 as GroundVec2};

use super::*;

/// A lot for a building of `kind`, `w` along its front and `d` deep, `h`
/// tall, facing south (its own `+Z`), seeded `variant`.
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
        store: StoreStyle::GeneralStore,
        veranda: 3.0,
    }
}

/// Every surface's points of `shape`.
fn points(shape: &Shape) -> Vec<(Surface, Vec3)> {
    shape
        .surfaces()
        .flat_map(|s| shape.points(s).into_iter().map(move |p| (s, p)))
        .collect()
}

/// The buildings of a street, at their core sizes, a few seeds each.
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
            ..lot(BuildingKind::House, (8.0, 8.5, 3.6), v)
        });
    }
    for store in [StoreStyle::GeneralStore, StoreStyle::Pulperia] {
        for v in 0..6 {
            lots.push(Lot {
                store,
                ..lot(BuildingKind::GeneralStore, (16.0, 14.0, 5.0), v)
            });
        }
    }
    lots.push(lot(BuildingKind::PoliceStation, (14.0, 12.0, 4.5), 3));
    lots.push(lot(BuildingKind::Church, (12.0, 22.0, 7.0), 5));
    lots
}

/// The station yard's buildings and the town's sheds, at their core
/// sizes: the station, a few iron sheds and the two grain stores.
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
    // Colliders are the footprints and the facade signs hang just outside
    // them: no pilaster, cornice, sill, shutter or eave may reach out
    // there. Only a store's galería reaches over the sidewalk, and the
    // station's platform canopy over the platform (core's canopy).
    for lot in street().into_iter().chain(yard()) {
        let shape = building(&lot).expect("a building");
        let size = lot.size();
        let (half_w, half_d) = (size.x / 2.0 + 1e-3, size.y / 2.0 + 1e-3);
        let h = lot.building.height;
        for (surface, p) in points(&shape) {
            let over_street = p.z > half_d;
            if lot.building.kind == BuildingKind::GeneralStore && over_street {
                // The galería: under the cornice, within the veranda.
                assert!(p.y < h - 0.5, "{surface:?} at {p} over the sidewalk");
                assert!(p.z < half_d + lot.veranda + 0.1, "{surface:?} at {p}");
                assert!(p.x.abs() < half_w + 0.2, "{surface:?} at {p}");
                continue;
            }
            if lot.building.kind == BuildingKind::Station && p.z < -half_d {
                // The canopy and its posts: on the track side, under the
                // canopy's top, the station's length.
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

/// Whether a triangle of `shape` facing the street covers `(x, y)` within
/// `reach` of the front.
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
    // Core collides at a galpón's walls, not at its lot: the iron walls
    // must stand on that collider, and the roof reach no farther than the
    // lot.
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
            // Their feet, under the ground: no roof comes down there.
            .filter(|p| p.y < 0.0)
            .collect();
        let reach = walls
            .iter()
            .fold(Vec2::ZERO, |m, p| m.max(Vec2::new(p.x.abs(), p.z.abs())));
        // Facing south: the building's `x` is the ground's, `z` across.
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
    // `signs` hangs the store's board 0.2–2.0 m over its wall height,
    // nearly the front's width: the parapet must stand whole behind it.
    for store in [StoreStyle::GeneralStore, StoreStyle::Pulperia] {
        let lot = Lot {
            store,
            ..lot(BuildingKind::GeneralStore, (16.0, 14.0, 5.0), 1)
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
fn the_station_canopy_is_the_roof_core_gives_the_camera() {
    let (w, d, h) = (24.0, 9.0, 5.0);
    let shape = building(&lot(BuildingKind::Station, (w, d, h), 0)).unwrap();
    // Core's canopy reaches `DEPTH` back from the track-side (back) wall,
    // the whole wall long, its slab `THICKNESS` thick round `HEIGHT`.
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

#[test]
fn town_buildings_keep_to_their_triangle_budgets() {
    for lot in street() {
        let tris = building(&lot).unwrap().triangles();
        let budget = match lot.building.kind {
            BuildingKind::Church => 2000..=6000,
            BuildingKind::GeneralStore | BuildingKind::PoliceStation => 1500..=5000,
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
}

#[test]
fn town_houses_vary_their_wash_roof_and_dressing() {
    let lots: Vec<Lot> = street()
        .into_iter()
        .filter(|l| l.building.kind == BuildingKind::House && !l.outskirts)
        .collect();
    // Each wash, paint and brick appears on some front, painted ones over
    // the beige band.
    // Every low nibble (the finish), the other bits mixed.
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
    // A young town: plaster flaking off the brick is the rarest front.
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
    // Outskirts houses are mostly plain cottages under iron.
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

/// Writes the preview street to `PAMPA_PREVIEW` (a folder) for the
/// offline renderer: `m <surface>` per mesh, then `v x y z nx ny nz u v`
/// and `f a b c` lines. Run with
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
        ..lot(BuildingKind::GeneralStore, (14.0, 14.0, 5.0), 2)
    };
    // One house of each new finish: the five paints, whitewashed brick
    // and plaster flaking off brick (the finish is `variant % 16`).
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
