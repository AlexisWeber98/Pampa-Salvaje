//! The townsfolk's models: five outfits, one per look (see
//! [`FigureKind::Townsfolk`] and `assets/characters/townsfolk_*`), each
//! person's clothes tinted from a small 1919 palette, and the arms of the
//! women in long skirts held a little out so their hands rest on the skirt
//! instead of sinking into it.
//!
//! A townsperson's `look` (from logic) picks the outfit (`look % 5`) and,
//! for each tintable garment, one of [`VARIANTS`] colours, the first being
//! the model's own. A tinted material is a copy of the model's, made once
//! per (material, colour) and shared by everyone who wears it
//! ([`TintCache`]), never one per person.

use bevy::gltf::GltfMaterialName;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use super::figure::{FigureKind, Rgb, TOWNSFOLK_LOOKS};

/// Each look's model folder, in look order: the paisano, the peón, the
/// Volga German farmhand, the countrywoman in a headscarf and the young
/// townswoman with her hair in a bun.
pub const OUTFITS: [&str; TOWNSFOLK_LOOKS as usize] = [
    "townsfolk_paisano",
    "townsfolk_peon",
    "townsfolk_volga",
    "townsfolk_headscarf",
    "townsfolk_bun",
];

/// How many colours each tintable garment comes in.
pub const VARIANTS: usize = 4;

/// The model folder of a townsperson of `look`.
pub fn outfit(look: u8) -> &'static str {
    OUTFITS[(look % TOWNSFOLK_LOOKS) as usize]
}

/// A garment (by its glTF material's name) and its colours (sRGB), the
/// first the model's own.
type Garment = (&'static str, [Rgb; VARIANTS]);

// Muted 1919 cloth (see `odd/setting-1919.md`): dust, dun, faded blue,
// brown, grey and black wool, off-white cotton; colour only in the
// accents (the faja, the pañuelo).
#[rustfmt::skip]
const PAISANO: &[Garment] = &[
    ("shirt", [[214, 207, 186], [190, 196, 198], [200, 186, 156], [226, 222, 210]]),
    ("bombacha", [[51, 51, 56], [70, 56, 44], [92, 86, 72], [38, 38, 40]]),
    ("sash", [[128, 33, 28], [36, 44, 72], [30, 28, 28], [70, 82, 52]]),
    ("neckerchief", [[158, 41, 31], [224, 220, 208], [46, 60, 104], [120, 40, 60]]),
    ("boina", [[33, 38, 56], [26, 26, 28], [40, 40, 46], [56, 44, 36]]),
];
#[rustfmt::skip]
const PEON: &[Garment] = &[
    ("shirt", [[168, 156, 130], [200, 196, 184], [150, 160, 170], [180, 170, 140]]),
    ("waistcoat", [[84, 64, 46], [46, 44, 46], [70, 72, 60], [96, 80, 60]]),
    ("bombacha", [[97, 87, 69], [60, 58, 60], [78, 66, 50], [110, 104, 88]]),
    ("sash", [[56, 43, 33], [110, 30, 26], [40, 44, 60], [30, 28, 28]]),
    ("hat_felt", [[92, 77, 59], [60, 52, 44], [120, 108, 88], [40, 36, 34]]),
];
#[rustfmt::skip]
const VOLGA: &[Garment] = &[
    ("shirt", [[158, 148, 122], [196, 190, 176], [140, 146, 150], [176, 160, 128]]),
    ("waistcoat", [[38, 36, 33], [70, 56, 42], [50, 54, 62], [60, 60, 56]]),
    ("trousers", [[66, 61, 54], [48, 46, 46], [84, 72, 56], [56, 58, 66]]),
    ("hat_felt", [[61, 54, 46], [38, 36, 34], [84, 74, 60], [52, 50, 48]]),
    // The poncho's woven texture is coloured: these tint it.
    ("poncho", [[158, 133, 107], [110, 120, 140], [170, 150, 120], [130, 110, 100]]),
];
#[rustfmt::skip]
const HEADSCARF: &[Garment] = &[
    ("shirt", [[107, 122, 138], [150, 140, 120], [120, 92, 84], [180, 176, 166]]),
    ("skirt", [[69, 48, 36], [38, 40, 50], [56, 56, 54], [80, 64, 44]]),
    ("apron", [[199, 191, 173], [170, 160, 140], [210, 206, 196], [150, 150, 140]]),
    ("shawl", [[51, 43, 38], [80, 30, 34], [40, 46, 60], [90, 80, 66]]),
    ("headscarf", [[33, 31, 33], [200, 196, 184], [60, 44, 40], [50, 60, 90]]),
];
#[rustfmt::skip]
const BUN: &[Garment] = &[
    ("shirt", [[219, 214, 199], [196, 188, 170], [176, 184, 196], [210, 190, 186]]),
    ("collar", [[219, 214, 199], [196, 188, 170], [176, 184, 196], [210, 190, 186]]),
    ("skirt", [[31, 36, 56], [40, 34, 32], [70, 40, 40], [48, 52, 44]]),
    ("shawl", [[84, 84, 87], [110, 90, 70], [60, 40, 50], [140, 128, 108]]),
];

/// Each outfit's tintable garments, in look order.
pub const GARMENTS: [&[Garment]; TOWNSFOLK_LOOKS as usize] = [PAISANO, PEON, VOLGA, HEADSCARF, BUN];

/// Which of its [`VARIANTS`] colours the `k`-th garment of a person of
/// `look` wears: drawn from the look's high part (the low part picks the
/// outfit), each garment by its own hash, so clothes mix.
pub fn variant(look: u8, k: usize) -> usize {
    let mut h = (look / TOWNSFOLK_LOOKS) as u32;
    h = h.wrapping_mul(0x9E37_79B1) ^ (k as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h % VARIANTS as u32) as usize
}

/// The colour a townsperson of `look` wears for the glTF material
/// `material`, when it differs from the model's own: None for a garment
/// that is not tinted or a person in its first colour.
pub fn tint(look: u8, material: &str) -> Option<Rgb> {
    let garments = GARMENTS[(look % TOWNSFOLK_LOOKS) as usize];
    let k = garments.iter().position(|(name, _)| *name == material)?;
    let v = variant(look, k);
    (v > 0).then_some(garments[k].1[v])
}

/// The tinted copies of the townsfolk's materials, one per (model
/// material, colour), shared by everyone wearing it.
#[derive(Resource, Debug, Default)]
pub struct TintCache {
    materials: HashMap<(AssetId<StandardMaterial>, Rgb), Handle<StandardMaterial>>,
}

impl TintCache {
    /// Tinted copies made so far.
    pub fn len(&self) -> usize {
        self.materials.len()
    }

    /// Whether none was made yet.
    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }

    /// `material` coloured `rgb`: the shared copy, made the first time.
    pub fn tinted(
        &mut self,
        material: &Handle<StandardMaterial>,
        rgb: Rgb,
        materials: &mut Assets<StandardMaterial>,
    ) -> Handle<StandardMaterial> {
        self.materials
            .entry((material.id(), rgb))
            .or_insert_with(|| {
                let mut m = materials.get(material).cloned().unwrap_or_default();
                let alpha = m.base_color.alpha();
                m.base_color = Color::srgb_u8(rgb[0], rgb[1], rgb[2]).with_alpha(alpha);
                materials.add(m)
            })
            .clone()
    }
}

/// What [`dress_townsperson`] reads of a model's mesh.
pub(super) type Worn = (
    &'static MeshMaterial3d<StandardMaterial>,
    &'static GltfMaterialName,
);

/// Gives the meshes of a townsperson of `look` their tinted materials.
/// Returns whether the model wears a long skirt (a material named
/// `skirt`): its arms are then held out a little ([`SKIRT_ARMS_OUT`]).
pub(super) fn dress_townsperson<'a>(
    commands: &mut Commands,
    (cache, materials): (&mut TintCache, &mut Assets<StandardMaterial>),
    kind: FigureKind,
    meshes: impl IntoIterator<
        Item = (
            Entity,
            (&'a MeshMaterial3d<StandardMaterial>, &'a GltfMaterialName),
        ),
    >,
) -> bool {
    let mut skirt = false;
    for (entity, (material, name)) in meshes {
        skirt |= name.0 == "skirt";
        let FigureKind::Townsfolk(look) = kind else {
            continue;
        };
        if let Some(rgb) = tint(look, &name.0) {
            let copy = cache.tinted(&material.0, rgb, materials);
            commands.entity(entity).try_insert(MeshMaterial3d(copy));
        }
    }
    skirt
}

/// How far (radians) the upper arms of a model in a long skirt are turned
/// out from the body, on top of the clips: the clips were made on a man,
/// and a woman's hips (with the skirt over them) are wider, so her
/// hanging hands would sink up to 4–5 cm into the skirt. At 0.14 they
/// keep about 2 cm off it through the idle, the walk and the talk.
pub const SKIRT_ARMS_OUT: f32 = 0.14;

/// The local rotation `local` of an upper arm turned out by `angle`
/// (radians) about the body's front (+X), away from the body on the left
/// (-Z) for a positive angle; `frame` is its parent's rest orientation in
/// the body frame.
pub fn arm_out(local: Quat, frame: Quat, angle: f32) -> Quat {
    (frame.inverse() * Quat::from_rotation_x(angle) * frame * local).normalize()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::character::cpu;

    /// How far, at worst, the model `model` (posed as drawn) has a leg
    /// out through its skirt, and an arm in it, meters (negative: clear by
    /// that much). Each vertex of the legs (thighs, shins, feet, and the
    /// boots on them) and of the arms (by its strongest joint) is seen along
    /// a level ray out from the pelvis through it: where the ray meets the
    /// skirt, a leg must lie inside its outermost crossing and an arm
    /// outside it. Points above or below the skirt meet none and count for
    /// nothing. None for a model without a skirt.
    pub(crate) fn skirt_pokes(world: &mut World, model: Entity) -> Option<(f32, f32)> {
        let drawn = cpu::drawn(world, Some(model));
        let named = |e: Entity| world.get::<Name>(e).map(|n| n.as_str().to_owned());
        let pelvis = {
            let mut stack = vec![model];
            let mut found = None;
            while let Some(e) = stack.pop() {
                if named(e).as_deref() == Some("pelvis") {
                    found = world.get::<GlobalTransform>(e).map(|g| g.translation());
                    break;
                }
                stack.extend(world.get::<Children>(e).into_iter().flatten().copied());
            }
            found?
        };
        let skirt: Vec<[Vec3; 3]> = drawn
            .iter()
            .filter(|d| d.name == "skirt")
            .flat_map(|d| {
                d.triangles
                    .iter()
                    .map(|t| t.map(|i| d.positions[i as usize]))
            })
            .collect();
        if skirt.is_empty() {
            return None;
        }
        let limb = |joint: Option<Entity>| {
            let name = joint.and_then(named)?;
            let base = name.split('_').next()?.to_owned();
            match base.as_str() {
                "thigh" | "calf" | "foot" | "ball" => Some(true),
                "upperarm" | "lowerarm" | "hand" | "thumb" | "index" | "middle" | "ring"
                | "pinky" => Some(false),
                _ => None,
            }
        };
        let (mut legs, mut arms) = (-1.0f32, -1.0f32);
        for d in drawn
            .iter()
            .filter(|d| d.name != "skirt" && d.name != "apron")
        {
            for (p, joint) in d.positions.iter().zip(&d.strongest) {
                let Some(leg) = limb(*joint) else { continue };
                let out = Vec3::new(p.x - pelvis.x, 0.0, p.z - pelvis.z);
                let r = out.length();
                if r < 1e-4 {
                    continue;
                }
                let dir = out / r;
                let from = Vec3::new(pelvis.x, p.y, pelvis.z);
                let far = skirt
                    .iter()
                    .filter(|t| {
                        let (lo, hi) = (
                            t[0].y.min(t[1].y).min(t[2].y),
                            t[0].y.max(t[1].y).max(t[2].y),
                        );
                        (lo..=hi).contains(&p.y)
                    })
                    .filter_map(|t| crossing(from, dir, t))
                    .fold(None, |a: Option<f32>, t| Some(a.map_or(t, |a| a.max(t))));
                let Some(far) = far else { continue };
                if leg {
                    legs = legs.max(r - far);
                } else {
                    arms = arms.max(far - r);
                }
            }
        }
        Some((legs, arms))
    }

    /// Where a ray from `from` along `dir` crosses the triangle `t`
    /// (Möller–Trumbore): the distance along it, if ahead.
    fn crossing(from: Vec3, dir: Vec3, t: &[Vec3; 3]) -> Option<f32> {
        let (e1, e2) = (t[1] - t[0], t[2] - t[0]);
        let h = dir.cross(e2);
        let det = e1.dot(h);
        if det.abs() < 1e-12 {
            return None;
        }
        let f = 1.0 / det;
        let s = from - t[0];
        let u = f * s.dot(h);
        let q = s.cross(e1);
        let v = f * dir.dot(q);
        let along = f * e2.dot(q);
        ((0.0..=1.0).contains(&u) && v >= 0.0 && u + v <= 1.0 && along > 0.0).then_some(along)
    }

    #[test]
    fn a_level_ray_meets_a_skirts_outer_wall_where_it_is() {
        // A square tube of half-width 1 round the y axis, from y 0 to 1.
        let c = [
            Vec3::new(-1.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(-1.0, 0.0, 1.0),
        ];
        let up = Vec3::Y;
        let walls: Vec<[Vec3; 3]> = (0..4)
            .flat_map(|k| {
                let (a, b) = (c[k], c[(k + 1) % 4]);
                [[a, b, b + up], [a, b + up, a + up]]
            })
            .collect();
        let far = |p: Vec3| {
            let dir = Vec3::new(p.x, 0.0, p.z).normalize();
            walls
                .iter()
                .filter_map(|t| crossing(Vec3::new(0.0, p.y, 0.0), dir, t))
                .fold(0.0f32, f32::max)
        };
        assert!((far(Vec3::new(0.3, 0.5, 0.0)) - 1.0).abs() < 1e-5);
        assert!((far(Vec3::new(2.0, 0.5, 2.0)) - 2f32.sqrt()).abs() < 1e-5);
        assert_eq!(far(Vec3::new(0.3, 1.5, 0.0)), 0.0, "above it: none");
    }
    use crate::character::model::{model_name, model_path};
    use crate::gltf::tests::root;

    #[test]
    fn each_look_wears_its_own_outfit_and_every_one_is_shipped() {
        let mut seen = std::collections::HashSet::new();
        for look in 0..=255u8 {
            let name = model_name(FigureKind::townsfolk(look)).unwrap();
            assert_eq!(name, OUTFITS[(look % TOWNSFOLK_LOOKS) as usize]);
            seen.insert(name);
        }
        assert_eq!(seen.len(), OUTFITS.len());
        for name in OUTFITS {
            assert!(root().0.join(model_path(name)).is_file(), "{name}");
        }
    }

    /// The material names of each outfit's shipped glTF, loaded as the
    /// game loads it.
    fn material_names() -> Vec<Vec<String>> {
        use crate::gltf::tests::unfinished_app;
        let mut app = unfinished_app(&root());
        app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>();
        app.finish();
        app.cleanup();
        let server = app.world().resource::<AssetServer>().clone();
        let handles: Vec<Handle<bevy::gltf::Gltf>> =
            OUTFITS.iter().map(|n| server.load(model_path(n))).collect();
        for _ in 0..2000 {
            let gltfs = app.world().resource::<Assets<bevy::gltf::Gltf>>();
            if handles.iter().all(|h| gltfs.contains(h)) {
                return handles
                    .iter()
                    .map(|h| {
                        let gltf = gltfs.get(h).unwrap();
                        gltf.named_materials.keys().map(|k| k.to_string()).collect()
                    })
                    .collect();
            }
            app.update();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the outfits never loaded");
    }

    #[test]
    fn every_tinted_garment_is_a_material_of_its_model_starting_from_its_own_colour() {
        let all = material_names();
        for ((name, garments), names) in OUTFITS.iter().zip(GARMENTS).zip(&all) {
            for (garment, colours) in garments {
                assert!(names.iter().any(|n| n == garment), "{name}: {garment}");
                // Each colour differs, and stays muted: no garment brighter
                // than off-white or more saturated than the faja's red.
                for (k, c) in colours.iter().enumerate() {
                    assert!(!colours[..k].contains(c), "{name} {garment}: {c:?} twice");
                    let (lo, hi) = (*c.iter().min().unwrap(), *c.iter().max().unwrap());
                    assert!(hi <= 230 && hi - lo <= 130, "{name} {garment}: {c:?}");
                }
            }
        }
    }

    #[test]
    fn a_look_picks_each_garments_colour_the_same_every_time_and_varies_across_people() {
        // The first colour is the model's own: no tint.
        let mut tinted = 0;
        let mut differ = 0;
        for look in 0..=255u8 {
            let garments = GARMENTS[(look % TOWNSFOLK_LOOKS) as usize];
            for (k, (garment, colours)) in garments.iter().enumerate() {
                let v = variant(look, k);
                assert_eq!(variant(look, k), v);
                assert_eq!(tint(look, garment), (v > 0).then_some(colours[v]));
                tinted += usize::from(v > 0);
            }
            // The same outfit, the next person: something else on.
            if let Some(next) = look.checked_add(TOWNSFOLK_LOOKS) {
                differ +=
                    usize::from((0..garments.len()).any(|k| variant(look, k) != variant(next, k)));
            }
        }
        assert!(tinted > 600, "{tinted}");
        assert!(differ > 240, "{differ}");
        // Every colour of every garment is worn by someone.
        for (o, garments) in GARMENTS.iter().enumerate() {
            for k in 0..garments.len() {
                let worn: std::collections::HashSet<usize> = (0..=255u8)
                    .filter(|l| (l % TOWNSFOLK_LOOKS) as usize == o)
                    .map(|l| variant(l, k))
                    .collect();
                assert_eq!(worn.len(), VARIANTS, "outfit {o} garment {k}");
            }
        }
        assert_eq!(tint(0, "skin"), None);
    }

    #[test]
    fn the_cache_makes_one_copy_per_material_and_colour() {
        let mut materials = Assets::<StandardMaterial>::default();
        let mut cache = TintCache::default();
        let cloth = materials.add(StandardMaterial {
            perceptual_roughness: 0.9,
            ..default()
        });
        let wool = materials.add(StandardMaterial::default());
        let red = cache.tinted(&cloth, [128, 33, 28], &mut materials);
        assert_eq!(cache.tinted(&cloth, [128, 33, 28], &mut materials), red);
        let blue = cache.tinted(&cloth, [36, 44, 72], &mut materials);
        let other = cache.tinted(&wool, [128, 33, 28], &mut materials);
        assert!(red != blue && red != other);
        assert_eq!(cache.len(), 3);
        let m = materials.get(&red).unwrap();
        assert_eq!(m.base_color, Color::srgb_u8(128, 33, 28));
        assert_eq!(m.perceptual_roughness, 0.9, "the rest is the model's");
    }

    #[test]
    fn an_arm_turned_out_swings_its_hand_away_from_the_body_on_its_own_side() {
        // A parent frame turned about the body's up, as a clavicle's is: the
        // turn still goes about the body's front.
        let frame = Quat::from_rotation_y(0.7) * Quat::from_rotation_z(0.3);
        let local = frame.inverse(); // the arm hangs straight down in the body frame
        let hand = |q: Quat| (frame * q) * Vec3::NEG_Y;
        assert!(hand(local).distance(Vec3::NEG_Y) < 1e-5);
        let left = hand(arm_out(local, frame, SKIRT_ARMS_OUT));
        let right = hand(arm_out(local, frame, -SKIRT_ARMS_OUT));
        // The body's left is -Z: out, not forward or back.
        assert!(left.z < -0.13 && right.z > 0.13, "{left} {right}");
        assert!(left.x.abs() < 1e-5 && right.x.abs() < 1e-5);
        assert!((left.angle_between(Vec3::NEG_Y) - SKIRT_ARMS_OUT).abs() < 1e-5);
    }
}
