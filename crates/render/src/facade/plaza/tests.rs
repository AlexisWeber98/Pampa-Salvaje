use std::collections::HashSet;

use pampa_core::town::{Bench, Paving, PlazaFurniture};
use pampa_core::{Aabb, PLAZA_MONUMENT, Side, Vec2 as GroundVec2};

use super::*;

/// A plaza `side` meters square round the origin, furnished as `paving`.
fn furnished(paving: Paving, side: f32, main: bool) -> PlazaFurniture {
    let area = Aabb::from_center_size(GroundVec2::new(0.0, 0.0), GroundVec2::new(side, side));
    PlazaFurniture::of(area, paving, main, &[])
}

const TIERS: [Paving; 3] = [Paving::None, Paving::Plaza, Paving::Everywhere];

/// A world point's place on the ground, for comparing with core's boxes.
fn ground(p: Vec3) -> GroundVec2 {
    GroundVec2::new(p.x, -p.z)
}

#[test]
fn each_tier_dresses_its_paths_as_the_sidewalk_rule_does() {
    // Brick in the city, earth with a brick curb in a town, plain earth in
    // a village: the same split as the sidewalks (`Paving`).
    assert_eq!(
        path_surfaces(Paving::Everywhere),
        (Surface::Paving, Some(Surface::Stucco))
    );
    assert_eq!(
        path_surfaces(Paving::Plaza),
        (Surface::Earth, Some(Surface::Brick))
    );
    assert_eq!(path_surfaces(Paving::None), (Surface::Earth, None));
    for paving in TIERS {
        let f = furnished(paving, 40.0, true);
        let s = plaza(&f);
        let (path, curb) = path_surfaces(paving);
        let surfaces: HashSet<Surface> = s.ground.surfaces().collect();
        assert!(surfaces.contains(&path), "{paving:?}");
        // The paths lie at their old height, the curbs 3 cm over them.
        let top = s
            .ground
            .points(path)
            .iter()
            .map(|p| p.y)
            .fold(f32::MIN, f32::max);
        assert!((top - PATH_TOP).abs() < 1e-5, "{paving:?}: {top}");
        match curb {
            Some(curb) => {
                // The paths alone: the city's curbs share stucco with its
                // monument.
                let mut alone = Shape::default();
                paths(&mut alone, &f, f.area.center());
                let points = alone.points(curb);
                let top = points.iter().map(|p| p.y).fold(f32::MIN, f32::max);
                assert!((top - PATH_CURB_TOP).abs() < 1e-5, "{paving:?}: {top}");
                // Curbs lie beside the paths, never on them.
                for p in points.iter().filter(|p| p.y > PATH_TOP) {
                    let g = ground(*p);
                    let on_x = g.y.abs() < PLAZA_PATH_WIDTH / 2.0 - 1e-4
                        && g.x.abs() > f.crossing.max.x + 1e-4;
                    let on_y = g.x.abs() < PLAZA_PATH_WIDTH / 2.0 - 1e-4
                        && g.y.abs() > f.crossing.max.y + 1e-4;
                    assert!(!on_x && !on_y, "{paving:?}: a curb on a path at {g:?}");
                }
            }
            None => assert!(!surfaces.contains(&Surface::Brick), "{paving:?}"),
        }
        // Only the city has a railing: elsewhere the furniture is the
        // benches alone.
        let mut one = Shape::default();
        bench(&mut one);
        let bench_iron = one.points(Surface::Steel).len();
        let railing = s.furniture.points(Surface::Steel).len() > f.benches.len() * bench_iron;
        assert_eq!(railing, paving == Paving::Everywhere, "{paving:?}");
    }
}

#[test]
fn the_monument_grows_with_the_town_and_stays_on_its_plinth() {
    let heights: Vec<f32> = TIERS.iter().map(|&p| Proportions::of(p).height()).collect();
    assert!(heights.windows(2).all(|w| w[0] + 1.5 < w[1]), "{heights:?}");
    assert!(heights[0] < 4.6 && heights[2] > 8.5, "{heights:?}");
    for paving in TIERS {
        let p = Proportions::of(paving);
        // Its lowest step is core's collider, inside the old plinth.
        let side = PlazaFurniture::monument_side(paving);
        assert!((p.steps[0].0 - side).abs() < 1e-6, "{paving:?}");
        assert!(side <= PLAZA_MONUMENT);
        // Each step, the pedestal and the shaft narrow upward.
        assert!(p.steps.windows(2).all(|w| w[0].0 > w[1].0));
        let last = p.steps.last().expect("a step").0;
        assert!(p.pedestal.0 < last && p.cornice.0 < last + 0.01);
        assert!(p.shaft.0 > p.shaft.1 && p.shaft.0 < p.die.0);
        let mut shape = Shape::default();
        monument(&mut shape, paving, PATH_TOP);
        let points: Vec<Vec3> = shape
            .surfaces()
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|s| shape.points(s))
            .collect();
        let half = side / 2.0 + 1e-4;
        assert!(
            points
                .iter()
                .all(|p| p.x.abs() <= half && p.z.abs() <= half)
        );
        let top = points.iter().map(|p| p.y).fold(f32::MIN, f32::max);
        assert!(
            (top - PATH_TOP - p.height()).abs() < 0.01,
            "{paving:?}: {top}"
        );
        assert!(points.iter().all(|p| p.y >= PATH_TOP - 0.05 - 1e-4));
    }
}

#[test]
fn a_bench_keeps_within_its_footprint_and_faces_its_path() {
    let mut shape = Shape::default();
    bench(&mut shape);
    let (half, d) = (Bench::LENGTH / 2.0, Bench::DEPTH / 2.0);
    for s in [Surface::Steel, Surface::Planks] {
        for p in shape.points(s) {
            assert!(
                p.x.abs() <= half + 1e-3 && p.z.abs() <= d + 1e-3,
                "{s:?}: {p}"
            );
            assert!((-0.01..1.0).contains(&p.y), "{s:?}: {p}");
        }
    }
    // Its back leans behind the seat, away from the front (+z).
    let mean = |pts: Vec<Vec3>| pts.iter().sum::<Vec3>() / pts.len() as f32;
    let planks = shape.points(Surface::Planks);
    let (seat, back): (Vec<Vec3>, Vec<Vec3>) = planks.into_iter().partition(|p| p.y < SEAT + 0.1);
    assert!(mean(back).z < mean(seat).z - 0.1);
    // Placed in a plaza, every bench's back is farther from the paths
    // than its seat, and it stays upright.
    for paving in TIERS {
        let f = furnished(paving, 40.0, true);
        for b in &f.benches {
            let mut one = Shape::default();
            bench(&mut one);
            let mut placed = Shape::default();
            placed.merge(
                one,
                Transform::from_translation(world_from_ground(b.at, LAWN_TOP))
                    .with_rotation(facing(b.facing)),
            );
            let planks = placed.points(Surface::Planks);
            assert!(planks.iter().all(|p| p.y > LAWN_TOP), "{b:?} upside down");
            let to_path = |p: &Vec3| {
                let g = ground(*p);
                g.x.abs().min(g.y.abs())
            };
            let (seat, back): (Vec<Vec3>, Vec<Vec3>) = planks
                .into_iter()
                .partition(|p| p.y < LAWN_TOP + SEAT + 0.1);
            let far = |v: &[Vec3]| v.iter().map(to_path).sum::<f32>() / v.len() as f32;
            assert!(far(&back) > far(&seat) + 0.1, "{b:?} faces away");
            // And within core's collider.
            let footprint = b.footprint().expanded(1e-3);
            for p in placed.points(Surface::Steel) {
                assert!(footprint.contains_point(ground(p)), "{b:?}: {p}");
            }
        }
    }
    // A half turn is about `y`: facing south stays upright.
    let south = facing(Side::South) * Vec3::Y;
    assert!(south.distance(Vec3::Y) < 1e-5);
}

#[test]
fn the_railing_stands_within_its_colliders_open_at_the_mouths() {
    {
        let paving = Paving::Everywhere;
        let f = furnished(paving, 40.0, true);
        let s = plaza(&f);
        let benches: Vec<Aabb> = f.benches.iter().map(|b| b.footprint()).collect();
        let solid: Vec<Aabb> = f
            .railing
            .iter()
            .chain(&benches)
            .map(|a| a.expanded(0.01))
            .collect();
        for surface in s.furniture.surfaces().collect::<Vec<_>>() {
            for p in s.furniture.points(surface) {
                let g = ground(p);
                assert!(
                    solid.iter().any(|a| a.contains_point(g)),
                    "{paving:?} {surface:?}: {g:?} outside every collider"
                );
            }
        }
        // Piers or posts at the four corners and both sides of each mouth.
        assert_eq!(stands(&f, 0.1).len(), 4 + 8);
        let top = s
            .furniture
            .points(Surface::Steel)
            .iter()
            .map(|p| p.y)
            .fold(f32::MIN, f32::max);
        assert!(top < 1.25, "{paving:?}: a low railing, {top}");
    }
}

#[test]
fn a_plaza_keeps_to_its_triangle_budget() {
    // General Pico's main plaza (2 x 2 blocks of 40 m and a 10 m street)
    // is the biggest: every tier stays under 30k triangles a plaza.
    const BUDGET: usize = 30_000;
    for paving in TIERS {
        for (side, main) in [(90.0, true), (40.0, true), (40.0, false)] {
            let s = plaza(&furnished(paving, side, main));
            let n = s.ground.triangles() + s.furniture.triangles();
            assert!(n < BUDGET, "{paving:?} {side}: {n} triangles");
        }
    }
    let s = plaza(&furnished(Paving::Everywhere, 90.0, true));
    let mut monument_only = Shape::default();
    monument(&mut monument_only, Paving::Everywhere, PATH_TOP);
    assert!(monument_only.triangles() < 1_500);
    let mut one = Shape::default();
    bench(&mut one);
    assert!(one.triangles() < 250, "{}", one.triangles());
    println!(
        "Pico main plaza: {} ground + {} furniture triangles",
        s.ground.triangles(),
        s.furniture.triangles()
    );
}
