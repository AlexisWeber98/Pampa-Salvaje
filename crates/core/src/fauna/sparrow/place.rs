//! Dónde viven los gorriones y los chingolos (ver [`super`]): los comederos y
//! los posaderos de cada grupo, sembrados de forma determinista.
//!
//! - Los gorriones, por pueblo ([`town_sparrows`]): comen en las veredas
//!   alrededor de la plaza, frente a los almacenes de ramos generales (donde
//!   se cae el grano de los caballos del palenque), en otras veredas y en las
//!   calles; se posan en el pretil del frente de las casas cercanas
//!   ([`roof_perches`], las que tienen uno según el render).
//! - Los chingolos, por chunk ([`fence_sparrows`]): comen en el borde del
//!   campo o en la banquina, junto a un alambrado, y se posan en sus postes y
//!   en su alambre de arriba.

use super::{Patch, Perch, SparrowGroup, SparrowKind, Spot};
use crate::rng::Rng;
use crate::town::{BuildingKind, Side, Sidewalk, TownLayout};
use crate::{Aabb, ChunkCoord, FenceLayout, Heading, Terrain, Vec2};

/// La parte de arriba de la albardilla del pretil del frente de una casa del
/// pueblo sobre la altura de su pared, en metros: el pretil de 0.85 m y su
/// albardilla de 0.08 (`facade::town_buildings::house`).
pub const COPING_TOP: f32 = 0.93;
/// El medio de la albardilla, detrás del borde del frente de la huella, en
/// metros: la pared retirada 0.1 (`PROUD`) y la mitad de su albardilla de
/// 0.24.
pub const COPING_IN: f32 = 0.22;
/// Hasta cuánto de la línea media de la albardilla (0.34 m de fondo) se
/// posan, hacia adelante o hacia atrás, en metros.
pub const COPING_DEPTH: f32 = 0.07;
/// El pedestal sobre cada pilar del pretil, la mitad de su ancho con la
/// albardilla, en metros: los pájaros se posan entre ellos.
const PEDESTAL: f32 = 0.29;
/// Dónde está el pilar de cada extremo del pretil, desde su punta, en metros.
const END_PILLAR: f32 = 0.2;
/// La altura del alambre de arriba de un alambrado, en metros (el render,
/// `fence.rs`, con su grosor de 14 mm encima).
pub const TOP_WIRE: f32 = 1.187;
/// Hasta qué distancia de un comedero hay posaderos, en metros.
pub const PERCH_REACH: f32 = 30.0;
/// Cuántos posaderos tiene un grupo, como mucho.
const PERCHES: usize = 4;
/// El lado de un comedero, en metros.
const PATCH: f32 = 3.0;
/// La probabilidad de que un chunk con alambrados tenga chingolos.
pub const FENCE_CHANCE: f32 = 0.6;

/// La dirección hacia afuera de un lado, sobre el suelo.
pub fn outward(side: Side) -> Vec2 {
    match side {
        Side::North => Vec2::new(0.0, 1.0),
        Side::South => Vec2::new(0.0, -1.0),
        Side::East => Vec2::new(1.0, 0.0),
        Side::West => Vec2::new(-1.0, 0.0),
    }
}

fn heading_of(v: Vec2) -> Heading {
    Heading::from_radians(v.y.atan2(v.x))
}

/// Los pretiles del frente de las casas de `town` que tienen uno según
/// `coped` (que recibe el índice de la casa en `town.buildings`: el render
/// decide qué casas de las afueras son ranchos sin pretil): dos tramos por
/// casa, entre los pedestales de sus dos vanos de los extremos, lejos del
/// frontón que puede llevar sobre el medio.
pub fn roof_perches(
    town: &TownLayout,
    terrain: &Terrain,
    coped: impl Fn(usize) -> bool,
) -> Vec<Spot> {
    let mut spots = Vec::new();
    for (i, b) in town.buildings.iter().enumerate() {
        if b.kind != BuildingKind::House || !coped(i) {
            continue;
        }
        let c = b.footprint.center();
        let out = outward(b.front);
        let along = Vec2::new(out.y.abs(), out.x.abs());
        let size = b.footprint.size();
        let (width, depth) = if out.x == 0.0 {
            (size.x, size.y)
        } else {
            (size.y, size.x)
        };
        // Las paredes, retiradas 0.1 de la huella; sus vanos, de unos 2.8 m.
        let len = width - 0.2;
        let bays = ((len / 2.8) as usize).clamp(2, 5);
        let bay = len / bays as f32;
        // Lejos del frontón que puede llevar sobre el medio del pretil.
        let crest = (len * 0.18).clamp(1.0, 2.2);
        let (lo, hi) = (
            END_PILLAR + PEDESTAL,
            (bay - PEDESTAL).min(len / 2.0 - crest - 0.1),
        );
        let reach = (hi - lo) / 2.0 - 0.08;
        if reach < 0.1 {
            continue;
        }
        let mid = -len / 2.0 + (lo + hi) / 2.0;
        let line = c + out * (depth / 2.0 - COPING_IN);
        let y = terrain.height(c.x, c.y) + b.height + COPING_TOP;
        for offset in [mid, -mid] {
            spots.push(Spot {
                at: line + along * offset,
                y,
                perch: Perch::Roof,
                along,
                slope: 0.0,
                reach,
                facing: heading_of(out),
                depth: COPING_DEPTH,
            });
        }
    }
    spots
}

/// Los posaderos de `spots` a [`PERCH_REACH`] de `p` o menos, los
/// [`PERCHES`] más cercanos.
fn perches_near(spots: &[Spot], p: Vec2) -> Vec<Spot> {
    let mut near: Vec<Spot> = spots
        .iter()
        .filter(|s| s.at.distance(p) <= PERCH_REACH)
        .copied()
        .collect();
    near.sort_by(|a, b| a.at.distance(p).total_cmp(&b.at.distance(p)));
    near.truncate(PERCHES);
    near
}

/// La parte común de dos rectángulos, si tienen una.
fn overlap(a: &Aabb, b: &Aabb) -> Option<Aabb> {
    let min = Vec2::new(a.min.x.max(b.min.x), a.min.y.max(b.min.y));
    let max = Vec2::new(a.max.x.min(b.max.x), a.max.y.min(b.max.y));
    (min.x < max.x && min.y < max.y).then(|| Aabb::new(min, max))
}

/// Si un comedero de `town` en `area` está libre de árboles.
fn clear_of_trees(town: &TownLayout, area: &Aabb) -> bool {
    let wide = area.expanded(0.6);
    town.trees
        .iter()
        .chain(town.shade_trees.iter().map(|t| &t.at))
        .all(|t| !wide.contains_point(*t))
}

/// Un comedero en la vereda `walk` alrededor de `p`.
fn sidewalk_patch(town: &TownLayout, walk: &Sidewalk, p: Vec2) -> Option<Patch> {
    let area = overlap(
        &walk.area.expanded(-0.25),
        &Aabb::from_center_size(p, Vec2::new(PATCH, PATCH)),
    )?;
    let size = area.size();
    (size.x >= 1.0 && size.y >= 1.0 && clear_of_trees(town, &area)).then_some(Patch {
        area,
        perch: Perch::Sidewalk,
    })
}

/// Un comedero en la vereda `walk`, en algún lugar dentro de `around` libre
/// de árboles (unos pocos intentos).
fn walk_patch(town: &TownLayout, walk: &Sidewalk, around: &Aabb, rng: &mut Rng) -> Option<Patch> {
    let a = overlap(&walk.area, around)?;
    (0..12).find_map(|_| {
        let p = Vec2::new(rng.range(a.min.x, a.max.x), rng.range(a.min.y, a.max.y));
        sidewalk_patch(town, walk, p)
    })
}

/// Un comedero en el medio de la calle `street`, alrededor de `p`, lejos de
/// sus veredas.
fn street_patch(town: &TownLayout, street: &Aabb, p: Vec2) -> Option<Patch> {
    let size = street.size();
    let keep = Sidewalk::WIDTH + 0.4;
    let middle = if size.x < size.y {
        Aabb::new(
            Vec2::new(street.min.x + keep, street.min.y),
            Vec2::new(street.max.x - keep, street.max.y),
        )
    } else {
        Aabb::new(
            Vec2::new(street.min.x, street.min.y + keep),
            Vec2::new(street.max.x, street.max.y - keep),
        )
    };
    let area = overlap(&middle, &Aabb::from_center_size(p, Vec2::new(PATCH, PATCH)))?;
    let s = area.size();
    let off_walks = town.sidewalks().iter().all(|w| !w.area.intersects(&area));
    (s.x >= 1.0 && s.y >= 1.0 && off_walks && clear_of_trees(town, &area)).then_some(Patch {
        area,
        perch: Perch::Street,
    })
}

/// Las bandadas de gorriones del pueblo `town` (el `index` de la lista de
/// pueblos, para su identidad): una cada unas cuatro casas con pretil (dos
/// tramos por casa en `roofs`, de [`roof_perches`]), entre 4 y 16, en las
/// veredas de la plaza, frente a los almacenes, en otras veredas y en las
/// calles, cada una con pretiles a [`PERCH_REACH`] de su comedero.
pub fn town_sparrows(
    town: &TownLayout,
    index: usize,
    roofs: &[Spot],
    terrain: &Terrain,
    seed: u32,
) -> Vec<SparrowGroup> {
    if roofs.is_empty() {
        return Vec::new();
    }
    let mut rng = Rng::new(seed ^ 0x6022_1019, town.seed ^ index as u32);
    let want = (roofs.len() / 8).clamp(4, 16);
    let walks = town.sidewalks();
    let mut patches: Vec<Patch> = Vec::new();
    // La plaza: las veredas que la rodean (entre los árboles de su anillo).
    let plaza = town.plaza.expanded(0.5);
    for walk in walks.iter().filter(|w| w.area.intersects(&plaza)).take(4) {
        patches.extend(walk_patch(town, walk, &walk.area, &mut rng));
    }
    // Frente a los almacenes.
    for b in town
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::GeneralStore)
        .take(3)
    {
        let out = outward(b.front);
        let size = b.footprint.size();
        let reach = (if out.x == 0.0 { size.y } else { size.x }) / 2.0;
        let p = b.footprint.center() + out * (reach + Sidewalk::WIDTH / 2.0);
        if let Some(walk) = walks.iter().find(|w| w.area.contains_point(p)) {
            let front = Aabb::from_center_size(p, Vec2::new(8.0, 8.0));
            patches.extend(walk_patch(town, walk, &front, &mut rng));
        }
    }
    // Otras veredas y las calles.
    for _ in 0..want * 12 {
        if patches.len() >= want {
            break;
        }
        let patch = if rng.unit() < 0.65 && !walks.is_empty() {
            let walk = &walks[rng.below(walks.len())];
            walk_patch(town, walk, &walk.area, &mut rng)
        } else if !town.streets.is_empty() {
            let s = town.streets[rng.below(town.streets.len())];
            let p = Vec2::new(rng.range(s.min.x, s.max.x), rng.range(s.min.y, s.max.y));
            street_patch(town, &s, p)
        } else {
            None
        };
        if let Some(patch) = patch
            && patches
                .iter()
                .all(|q| q.area.center().distance(patch.area.center()) > 12.0)
        {
            patches.push(patch);
        }
    }
    let ground = |p: Vec2| terrain.height(p.x, p.y);
    patches
        .into_iter()
        .enumerate()
        .filter_map(|(k, patch)| {
            let perches = perches_near(roofs, patch.area.center());
            (!perches.is_empty()).then(|| {
                let id = ((index as u64) << 32) | k as u64;
                SparrowGroup::new(
                    SparrowKind::Gorrion,
                    id,
                    patch,
                    perches,
                    rng.next(),
                    &ground,
                )
            })
        })
        .collect()
}

/// Los chingolos del chunk `coord` (de lado `size`): con
/// [`FENCE_CHANCE`], uno o dos grupos junto a sus alambrados, cada uno con su
/// comedero a un par de metros del alambre, de un lado o del otro (el borde
/// del potrero o la banquina), donde `clear` lo permite (ni en una laguna ni
/// en un asentamiento), y sus posaderos en los postes y el alambre de arriba
/// de ese tramo y de sus vecinos.
pub fn fence_sparrows(
    fences: &FenceLayout,
    coord: ChunkCoord,
    size: f32,
    terrain: &Terrain,
    seed: u32,
    clear: impl Fn(Vec2) -> bool,
) -> Vec<SparrowGroup> {
    let mut rng = Rng::new(seed ^ 0xc41_6010, coord.seed(seed));
    if rng.unit() >= FENCE_CHANCE {
        return Vec::new();
    }
    let runs = fences.runs_in(coord.bounds(size));
    let spans: Vec<(usize, usize)> = runs
        .iter()
        .enumerate()
        .flat_map(|(r, run)| {
            run.posts
                .windows(2)
                .enumerate()
                .filter(|(_, w)| w[0].distance(w[1]) >= 4.0)
                .map(move |(i, _)| (r, i))
        })
        .collect();
    if spans.is_empty() {
        return Vec::new();
    }
    let ground = |p: Vec2| terrain.height(p.x, p.y);
    let groups = 1 + rng.below(2);
    let mut out = Vec::new();
    for k in 0..groups {
        let (r, i) = spans[rng.below(spans.len())];
        let posts = &runs[r].posts;
        let (a, b) = (posts[i], posts[i + 1]);
        let len = a.distance(b);
        let dir = (b - a) * (1.0 / len);
        let perp = Vec2::new(-dir.y, dir.x);
        let side = if rng.unit() < 0.5 { 1.0 } else { -1.0 };
        let c = a + dir * (len * rng.range(0.25, 0.75)) + perp * (side * rng.range(1.8, 2.8));
        let area = Aabb::from_center_size(c, Vec2::new(2.2, 2.2));
        let corners = [
            area.min,
            area.max,
            Vec2::new(area.min.x, area.max.y),
            Vec2::new(area.max.x, area.min.y),
            c,
        ];
        if !corners.iter().all(|&p| clear(p)) {
            continue;
        }
        let mut perches = Vec::new();
        let facing = heading_of(perp);
        for &p in &posts[i.saturating_sub(1)..=(i + 2).min(posts.len() - 1)] {
            perches.push(Spot {
                at: p,
                y: ground(p) + FenceLayout::HEIGHT,
                perch: Perch::Post,
                along: dir,
                slope: 0.0,
                reach: 0.0,
                facing,
                depth: 0.0,
            });
        }
        for j in i.saturating_sub(1)..=(i + 1).min(posts.len() - 2) {
            let (p, q) = (posts[j], posts[j + 1]);
            let l = p.distance(q);
            if l < 2.0 {
                continue;
            }
            let (gp, gq) = (ground(p), ground(q));
            perches.push(Spot {
                at: (p + q) * 0.5,
                y: (gp + gq) / 2.0 + TOP_WIRE,
                perch: Perch::Wire,
                along: (q - p) * (1.0 / l),
                slope: (gq - gp) / l,
                reach: l / 2.0 - 0.5,
                facing: heading_of(Vec2::new(-(q - p).y, (q - p).x)),
                depth: 0.0,
            });
        }
        let id = (u64::from(coord.x as u32) << 34) ^ (u64::from(coord.y as u32) << 2) ^ k as u64;
        let patch = Patch {
            area,
            perch: Perch::Verge,
        };
        out.push(SparrowGroup::new(
            SparrowKind::Chingolo,
            id | (1 << 63),
            patch,
            perches,
            rng.next(),
            &ground,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FenceRun;
    use crate::town::TownConfig;

    #[test]
    fn gorriones_feed_on_the_town_s_sidewalks_and_streets_and_perch_on_house_copings() {
        let terrain = Terrain::default();
        let town = TownLayout::generate(&TownConfig::castex()).unwrap();
        let roofs = roof_perches(&town, &terrain, |_| true);
        assert!(roofs.len() >= 20, "{}", roofs.len());
        // Solo las casas que tienen pretil según `coped`.
        let some = roof_perches(&town, &terrain, |i| i % 2 == 0);
        assert!(!some.is_empty() && some.len() < roofs.len());
        assert!(some.iter().all(|s| {
            town.buildings
                .iter()
                .position(|b| b.footprint.contains_point(s.at))
                .is_some_and(|i| i % 2 == 0)
        }));
        for spot in &roofs {
            // Sobre el frente de una casa.
            let house = town
                .buildings
                .iter()
                .find(|b| b.footprint.contains_point(spot.at))
                .expect("a coping lies on its house");
            assert_eq!(house.kind, BuildingKind::House);
            let c = house.footprint.center();
            let top = terrain.height(c.x, c.y) + house.height + COPING_TOP;
            assert!((spot.y - top).abs() < 1e-4);
            for end in [-spot.reach, spot.reach] {
                assert!(house.footprint.contains_point(spot.place(end).0));
            }
            // Mirando a la calle.
            let out = outward(house.front);
            assert!(spot.facing.forward().dot(out) > 0.99);
        }
        let groups = town_sparrows(&town, 0, &roofs, &terrain, 7);
        assert!((4..=16).contains(&groups.len()), "{}", groups.len());
        let walks = town.sidewalks();
        let plaza = town.plaza.expanded(Sidewalk::WIDTH + 1.0);
        assert!(groups.iter().any(|g| plaza.intersects(&g.patch.area)));
        let stores: Vec<Aabb> = town
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::GeneralStore)
            .map(|b| b.footprint.expanded(Sidewalk::WIDTH + 1.0))
            .collect();
        assert!(
            groups
                .iter()
                .any(|g| stores.iter().any(|s| s.intersects(&g.patch.area)))
        );
        for g in &groups {
            assert_eq!(g.kind, SparrowKind::Gorrion);
            let (lo, hi) = SparrowKind::Gorrion.group_size();
            assert!((lo..=hi).contains(&g.birds.len()));
            let area = g.patch.area;
            let corners = [area.min, area.max];
            match g.patch.perch {
                Perch::Sidewalk => assert!(
                    walks
                        .iter()
                        .any(|w| corners.iter().all(|&c| w.area.contains_point(c)))
                ),
                Perch::Street => {
                    assert!(walks.iter().all(|w| !w.area.intersects(&area)));
                    assert!(
                        town.streets
                            .iter()
                            .any(|s| corners.iter().all(|&c| s.contains_point(c)))
                    );
                }
                other => panic!("{other:?}"),
            }
            assert!(
                town.buildings
                    .iter()
                    .all(|b| !b.footprint.intersects(&area))
            );
            assert!(!g.perches.is_empty());
            for p in &g.perches {
                assert_eq!(p.perch, Perch::Roof);
                assert!(p.at.distance(area.center()) <= PERCH_REACH);
            }
            for b in &g.birds {
                let on = b.perch.unwrap();
                assert!(on == g.patch.perch || on == Perch::Roof, "{on:?}");
            }
        }
        // Deterministas.
        assert_eq!(groups, town_sparrows(&town, 0, &roofs, &terrain, 7));
    }

    #[test]
    fn chingolos_feed_beside_a_fence_and_perch_on_its_posts_and_top_wire() {
        let terrain = Terrain::default();
        let size = 128.0;
        // Un alambrado este–oeste de postes cada 10 m a través del chunk.
        let posts: Vec<Vec2> = (0..=14)
            .map(|k| Vec2::new(-5.0 + 10.0 * k as f32, 60.0))
            .collect();
        let fences = FenceLayout {
            runs: vec![FenceRun {
                posts,
                roadside: false,
            }],
            ..FenceLayout::default()
        };
        let chunks = [ChunkCoord::new(0, 0)];
        let mut seen = 0;
        for seed in 0..40 {
            for &coord in &chunks {
                for g in fence_sparrows(&fences, coord, size, &terrain, seed, |_| true) {
                    seen += 1;
                    assert_eq!(g.kind, SparrowKind::Chingolo);
                    assert!((1..=2).contains(&g.birds.len()));
                    assert_eq!(g.patch.perch, Perch::Verge);
                    // Al lado del alambre, sin tocarlo.
                    let off = (g.patch.area.center().y - 60.0).abs();
                    assert!((1.8..=2.8).contains(&off), "{off}");
                    assert!(g.patch.area.min.y > 60.0 || g.patch.area.max.y < 60.0);
                    assert!(g.perches.iter().any(|p| p.perch == Perch::Post));
                    assert!(g.perches.iter().any(|p| p.perch == Perch::Wire));
                    for p in &g.perches {
                        let ground = terrain.height(p.at.x, p.at.y);
                        match p.perch {
                            Perch::Post => {
                                assert!((p.y - ground - FenceLayout::HEIGHT).abs() < 1e-4);
                                assert_eq!(p.reach, 0.0);
                            }
                            Perch::Wire => {
                                assert!((p.at.y - 60.0).abs() < 1e-4);
                                for end in [-p.reach, p.reach] {
                                    let (q, y) = p.place(end);
                                    let ground = terrain.height(q.x, q.y);
                                    // Sobre el alambre de arriba, a lo largo
                                    // del tramo (el suelo, en línea recta).
                                    assert!((y - ground - TOP_WIRE).abs() < 0.05, "{y} {ground}");
                                }
                            }
                            other => panic!("{other:?}"),
                        }
                    }
                }
            }
        }
        // FENCE_CHANCE y uno o dos grupos.
        assert!((20..=70).contains(&seen), "{seen}");
        // Donde `clear` no lo permite (una laguna, un asentamiento), ninguno.
        for seed in 0..40 {
            assert!(
                fence_sparrows(&fences, ChunkCoord::new(0, 0), size, &terrain, seed, |_| {
                    false
                })
                .is_empty()
            );
        }
        // Sin alambrados, ninguno.
        let none = FenceLayout::default();
        assert!(
            fence_sparrows(&none, ChunkCoord::new(0, 0), size, &terrain, 1, |_| true).is_empty()
        );
    }
}
