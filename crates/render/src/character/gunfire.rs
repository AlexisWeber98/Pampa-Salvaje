//! El revólver y el fogonazo de quien dispara a caballo en el acto 6
//! ([`Shooting`]): el arma aparece en la mano derecha, apuntando al blanco,
//! mientras el componente está puesto, y el fogonazo (una luz breve y un
//! destello en la boca del cañón) mientras dura su `flash`.
//!
//! El revólver es el "Colt Single Action Army .45" de Guy in a Poncho
//! (ponchoguy, Sketchfab, CC BY 4.0), convertido por `tools/revolver.py`
//! ([`REVOLVER`]; ver `assets/models/weapons/CREDITS.md`).
//!
//! El brazo lo estira [`super::rider::aim_shooters`] antes de que se calculen
//! los transforms. El arma y el fogonazo cuelgan del hueso de la mano: se
//! ubican después, con la mano ya calculada, en el espacio de la mano; así
//! siguen a la mano sin atraso aunque el caballo galope, y solo el giro hacia
//! el blanco llega un frame después, que no se ve.

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};

use super::Pillion;
use super::rider::{Behind, Riding, Seated, Shooting, right_hand, shooter};
use super::viewer::world_layers;

/// El revólver, en `assets/`.
pub const REVOLVER: &str = "models/weapons/colt_saa.glb";

/// Cuánto más allá de la muñeca, hacia el blanco, queda la culata en la mano,
/// en metros.
pub const GRIP_OUT: f32 = 0.07;

/// Dónde está la boca del cañón en el espacio del revólver (el cañón apunta a
/// -Z, arriba es +Y, el origen es el medio de la culata, en la mano): el nodo
/// `Muzzle` de [`REVOLVER`] (`tools/test_revolver.py` lo comprueba).
pub const MUZZLE: Vec3 = Vec3::new(0.0, 0.0741, -0.2721);

/// Cuánto ilumina el fogonazo, en lúmenes, y hasta dónde, en metros.
pub const FLASH_LUMENS: f32 = 300_000.0;
pub const FLASH_RANGE: f32 = 18.0;

/// El revólver y el destello del fogonazo, cargados una vez.
#[derive(Resource, Debug, Clone)]
pub(super) struct GunKit {
    revolver: Handle<WorldAsset>,
    flash: Handle<Mesh>,
    glare: Handle<StandardMaterial>,
}

/// El revólver de un personaje (`of`), colgado de su mano.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Revolver {
    of: Entity,
}

/// El fogonazo del revólver de un personaje (`of`), colgado de su mano.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MuzzleFlash {
    of: Entity,
}

/// Carga el revólver y arma el destello del fogonazo.
pub(super) fn make_gun_kit(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(server), Some(mut meshes), Some(mut materials)) = (server, meshes, materials) else {
        return;
    };
    commands.insert_resource(GunKit {
        revolver: server.load(GltfAssetLabel::Scene(0).from_asset(REVOLVER)),
        // Un destello estirado a lo largo del cañón.
        flash: meshes.add(
            Mesh::from(Sphere::new(0.05))
                .transformed_by(Transform::from_scale(Vec3::new(0.8, 0.8, 1.8))),
        ),
        glare: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.85, 0.5),
            emissive: LinearRgba::rgb(40.0, 22.0, 6.0),
            unlit: true,
            ..default()
        }),
    });
}

/// Pone el revólver en la mano derecha de cada uno que dispara, apuntando a su
/// blanco, y el fogonazo en la boca del cañón mientras dura; los saca cuando
/// deja de disparar.
#[allow(clippy::type_complexity)]
pub(super) fn hold_revolvers(
    mut commands: Commands,
    kit: Option<Res<GunKit>>,
    riders: Query<(&Riding, &Seated, Option<&Behind>)>,
    (shooting, parents): (Query<(&Shooting, Has<Pillion>)>, Query<&ChildOf>),
    hands: Query<&GlobalTransform, (Without<Revolver>, Without<MuzzleFlash>)>,
    mut guns: Query<(Entity, &Revolver, &mut Transform), Without<MuzzleFlash>>,
    mut flashes: Query<(Entity, &MuzzleFlash, &mut Transform, &mut Visibility), Without<Revolver>>,
) {
    let Some(kit) = kit else {
        return;
    };
    // Cada personaje que dispara: su mano, y el arma y la boca del cañón en el
    // espacio de la mano, y si se ve el fogonazo.
    let mut held: Vec<(Entity, Entity, Transform, Transform, bool)> = Vec::new();
    for (riding, seated, behind) in &riders {
        let doubled = |c: Entity| shooting.get(c).ok().map(|(_, d)| d);
        let Some(character) = shooter(riding, behind, &parents, doubled) else {
            continue;
        };
        let hand = right_hand(seated);
        let (Ok((shot, _)), Ok(at)) = (shooting.get(character), hands.get(hand)) else {
            continue;
        };
        let wrist = at.translation();
        let toward = (shot.at - wrist).normalize_or_zero();
        if toward == Vec3::ZERO {
            continue;
        }
        let gun =
            Transform::from_translation(wrist + toward * GRIP_OUT).looking_to(toward, Vec3::Y);
        let muzzle = gun.with_translation(gun.transform_point(MUZZLE));
        let local = |t: Transform| {
            Transform::from_matrix((at.affine().inverse() * t.compute_affine()).into())
        };
        held.push((character, hand, local(gun), local(muzzle), shot.flash > 0.0));
    }
    let find = |of: Entity| held.iter().find(|h| h.0 == of).copied();
    for (entity, revolver, mut transform) in &mut guns {
        match find(revolver.of) {
            Some((_, _, gun, ..)) => transform.set_if_neq(gun),
            None => {
                commands.entity(entity).despawn();
                continue;
            }
        };
    }
    for (entity, flash, mut transform, mut shown) in &mut flashes {
        match find(flash.of) {
            Some((.., muzzle, firing)) => {
                transform.set_if_neq(muzzle);
                shown.set_if_neq(if firing {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
            None => commands.entity(entity).despawn(),
        }
    }
    for (character, hand, gun, muzzle, firing) in held {
        if guns.iter().any(|(_, r, _)| r.of == character) {
            continue;
        }
        commands.spawn((
            Name::new("Revolver"),
            Revolver { of: character },
            WorldAssetRoot(kit.revolver.clone()),
            gun,
            ChildOf(hand),
        ));
        commands.spawn((
            Name::new("Muzzle flash"),
            MuzzleFlash { of: character },
            Mesh3d(kit.flash.clone()),
            MeshMaterial3d(kit.glare.clone()),
            NotShadowCaster,
            NotShadowReceiver,
            PointLight {
                color: Color::srgb(1.0, 0.78, 0.45),
                intensity: FLASH_LUMENS,
                range: FLASH_RANGE,
                radius: 0.05,
                shadow_maps_enabled: false,
                contact_shadows_enabled: false,
                ..default()
            },
            world_layers(),
            muzzle,
            if firing {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
            ChildOf(hand),
        ));
    }
}
