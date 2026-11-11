//! Una persona que se desvanece ([`Fading`]): quien se va de la escena se
//! difumina en lugar de desaparecer de golpe. Los materiales de las personas
//! se comparten entre todas las que tienen el mismo aspecto (el kit de figuras,
//! los de cada modelo y los teñidos de [`TintCache`](super::TintCache)), así
//! que la que se desvanece recibe sus propias copias, con mezcla alfa, y las
//! demás siguen opacas.

use bevy::prelude::*;

/// Qué tan visible queda la persona de esta entidad, de 1 (opaca) a 0
/// (invisible). Mientras la lleva, cada malla dibujada bajo ella usa su propia
/// copia del material, con mezcla alfa; quitarla no devuelve los materiales
/// compartidos (se usa para irse).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Fading(pub f32);

/// La copia propia del material de una malla bajo una persona que se
/// desvanece, y el alfa que tenía el original.
#[derive(Component, Debug, Clone)]
pub(super) struct OwnFade {
    material: Handle<StandardMaterial>,
    alpha: f32,
}

/// Da a cada malla bajo una persona que se desvanece su propia copia del
/// material, con mezcla alfa (también a las que aparecen después, como un
/// modelo que termina de cargar), y le pone el alfa de [`Fading`].
pub(super) fn fade_people(
    mut commands: Commands,
    fading: Query<(Entity, &Fading)>,
    children: Query<&Children>,
    mut meshes: Query<(&mut MeshMaterial3d<StandardMaterial>, Option<&OwnFade>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (root, fade) in &fading {
        let visible = fade.0.clamp(0.0, 1.0);
        for entity in children.iter_descendants(root) {
            let Ok((mut material, own)) = meshes.get_mut(entity) else {
                continue;
            };
            let own = match own {
                Some(own) if own.material == material.0 => own.clone(),
                _ => {
                    let Some(mut copy) = materials.get(&material.0).cloned() else {
                        continue;
                    };
                    let alpha = copy.base_color.alpha();
                    copy.alpha_mode = AlphaMode::Blend;
                    let own = OwnFade {
                        material: materials.add(copy),
                        alpha,
                    };
                    material.0 = own.material.clone();
                    commands.entity(entity).try_insert(own.clone());
                    own
                }
            };
            if let Some(mut copy) = materials.get_mut(&own.material) {
                copy.base_color.set_alpha(own.alpha * visible);
            }
        }
    }
}
