//! Un recorrido de capturas solo para desarrollo: `PAMPA_SHOTS=<file>` toma las
//! capturas que enumera el archivo, un PNG cada una, y sale. Sin la variable,
//! nada de esto se ejecuta.
//!
//! El archivo tiene una captura por línea (se omiten las líneas en blanco y los
//! comentarios con `#`):
//!
//! ```text
//! name  eye_x eye_y eye_height  target_x target_y target_height  [HH:MM]
//! ```
//!
//! `x`/`y` son coordenadas del suelo (este, norte); las alturas son metros
//! sobre el suelo bajo ese punto. Una hora del día fija el reloj para la
//! captura (el cielo y el sol la siguen, y cada tren avanza junto con el salto,
//! así que un tren queda donde lo pone el horario; el reloj solo avanza, hasta
//! la próxima vez que marque esa hora, y conviene listar las capturas de un
//! recorrido con horas en orden creciente); sin ella, el reloj corre como
//! corre. Para cada captura, la cámara se mantiene en el ojo mirando al
//! objetivo (se anula la cámara de seguimiento) y el mundo hace streaming a su
//! alrededor; cuando dejan de llegar chunks y todas las texturas están
//! cargadas, se asientan unos frames más y se guarda `<name>.png` en
//! `PAMPA_SHOTS_DIR` (por defecto `shots`). `PAMPA_SHOTS_WAIT` fija esos frames
//! de asentamiento (por defecto 90).

use std::path::PathBuf;

use bevy::asset::LoadState;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::transform::TransformSystems;
use pampa_core::{BASE_RATE, TimeOfDay, Vec2 as GroundVec2};
use pampa_render::{
    StreamFocus, TerrainChunk, WorldClock, WorldTerrain, WorldTrains, world_from_ground,
};

use crate::camera::FollowCamera;

/// Una captura del recorrido.
#[derive(Debug, Clone, PartialEq)]
pub struct Shot {
    pub name: String,
    pub eye: (GroundVec2, f32),
    pub target: (GroundVec2, f32),
    pub time: Option<TimeOfDay>,
}

/// Interpreta una lista de capturas (ver la documentación del módulo).
pub fn parse_shots(text: &str) -> Result<Vec<Shot>, String> {
    let mut shots = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let at = |why: &str| format!("line {}: {why}", n + 1);
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.len() != 7 && words.len() != 8 {
            return Err(at("want `name ex ey eh tx ty th [HH:MM]`"));
        }
        let mut num = [0.0_f32; 6];
        for (slot, word) in num.iter_mut().zip(&words[1..7]) {
            *slot = word
                .parse()
                .map_err(|_| at(&format!("`{word}` is no number")))?;
        }
        let time = match words.get(7) {
            None => None,
            Some(word) => {
                let (h, m) = word.split_once(':').ok_or_else(|| at("time is HH:MM"))?;
                match (h.parse::<u32>(), m.parse::<u32>()) {
                    (Ok(h), Ok(m)) if h < 24 && m < 60 => Some(TimeOfDay::hm(h, m)),
                    _ => return Err(at("time is HH:MM")),
                }
            }
        };
        shots.push(Shot {
            name: words[0].to_owned(),
            eye: (GroundVec2::new(num[0], num[1]), num[2]),
            target: (GroundVec2::new(num[3], num[4]), num[5]),
            time,
        });
    }
    Ok(shots)
}

/// Hace el recorrido de `PAMPA_SHOTS`, si la variable está definida.
pub struct ShotsPlugin;

impl ShotsPlugin {
    /// El plugin, cuando `PAMPA_SHOTS` nombra una lista de capturas legible y
    /// válida.
    pub fn from_env() -> Option<Tour> {
        let file = std::env::var_os("PAMPA_SHOTS")?;
        let text = std::fs::read_to_string(&file)
            .map_err(|e| error!("PAMPA_SHOTS {file:?}: {e}"))
            .ok()?;
        let shots = parse_shots(&text)
            .map_err(|e| error!("PAMPA_SHOTS {file:?}: {e}"))
            .ok()?;
        let dir = std::env::var_os("PAMPA_SHOTS_DIR").map_or_else(|| "shots".into(), PathBuf::from);
        let wait = std::env::var("PAMPA_SHOTS_WAIT")
            .ok()
            .and_then(|w| w.parse().ok())
            .unwrap_or(90);
        Some(Tour {
            shots,
            dir,
            wait,
            index: 0,
            chunks: 0,
            stable: 0,
            waited: 0,
            requested: false,
            jumped: None,
        })
    }
}

/// El recorrido en curso.
#[derive(Resource, Debug)]
pub struct Tour {
    shots: Vec<Shot>,
    dir: PathBuf,
    /// Frames que el mundo debe mantenerse quieto antes de una captura.
    wait: u32,
    index: usize,
    /// Los chunks presentes en el frame anterior, y durante cuántos frames se
    /// mantuvo eso.
    chunks: usize,
    stable: u32,
    waited: u32,
    requested: bool,
    /// La captura a cuya hora se llevó el reloj.
    jumped: Option<usize>,
}

impl Plugin for Tour {
    fn build(&self, app: &mut App) {
        std::fs::create_dir_all(&self.dir).ok();
        app.insert_resource(Tour {
            shots: self.shots.clone(),
            dir: self.dir.clone(),
            wait: self.wait,
            index: 0,
            chunks: 0,
            stable: 0,
            waited: 0,
            requested: false,
            jumped: None,
        })
        .add_systems(
            Update,
            (aim_focus.after(crate::player::follow_player), take_shot).chain(),
        )
        .add_systems(PostUpdate, aim_camera.before(TransformSystems::Propagate));
    }
}

/// Hace streaming del mundo alrededor del ojo de la captura actual y fija su
/// hora.
fn aim_focus(
    mut tour: ResMut<Tour>,
    mut focus: ResMut<StreamFocus>,
    mut clock: Option<ResMut<WorldClock>>,
    mut trains: Option<ResMut<WorldTrains>>,
) {
    let Some(shot) = tour.shots.get(tour.index) else {
        return;
    };
    focus.0 = shot.eye.0;
    let (time, index, shot_name) = (shot.time, tour.index, shot.name.clone());
    if let (Some(time), Some(clock)) = (time, clock.as_deref_mut())
        && tour.jumped != Some(index)
    {
        tour.jumped = Some(index);
        // Los trenes recorren el salto como una espera omitida.
        let from = clock.0.elapsed();
        let skipped = clock.0.advance_until(time);
        for (i, train) in trains.iter_mut().flat_map(|t| t.0.iter_mut()).enumerate() {
            train.advance(from, skipped, BASE_RATE);
            // Dónde están la locomotora y el último vagón de cada tren, para
            // apuntar.
            let cars = train.placed_cars();
            if let (Some(first), Some(last)) = (cars.first(), cars.last()) {
                info!(
                    "shot {shot_name}: train {i} from ({:.0}, {:.0}) to ({:.0}, {:.0}), \
                     heading ({:.1}, {:.1})",
                    first.center.x,
                    first.center.y,
                    last.center.x,
                    last.center.y,
                    first.direction.x,
                    first.direction.y
                );
            }
        }
    }
}

/// Mantiene la cámara en el ojo de la captura actual, sobre el objetivo.
fn aim_camera(
    tour: Res<Tour>,
    terrain: Res<WorldTerrain>,
    mut cameras: Query<&mut Transform, With<FollowCamera>>,
) {
    let Some(shot) = tour.shots.get(tour.index) else {
        return;
    };
    let place = |(p, h): (GroundVec2, f32)| world_from_ground(p, terrain.0.height(p.x, p.y) + h);
    for mut camera in &mut cameras {
        *camera =
            Transform::from_translation(place(shot.eye)).looking_at(place(shot.target), Vec3::Y);
    }
}

/// Una vez que el mundo alrededor de la captura se asentó, guarda la imagen;
/// pasa a la siguiente cuando el archivo está; sale después de la última.
fn take_shot(
    mut commands: Commands,
    mut tour: ResMut<Tour>,
    chunks: Query<(), With<TerrainChunk>>,
    materials: Res<Assets<StandardMaterial>>,
    server: Res<AssetServer>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(shot) = tour.shots.get(tour.index) else {
        exit.write(AppExit::Success);
        return;
    };
    let path = tour.dir.join(format!("{}.png", shot.name));
    if tour.requested {
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > 0) {
            info!("shot {} saved", path.display());
            tour.index += 1;
            tour.requested = false;
            tour.stable = 0;
        }
        return;
    }
    let count = chunks.iter().count();
    let loading = materials.iter().any(|(_, m)| {
        [
            &m.base_color_texture,
            &m.normal_map_texture,
            &m.metallic_roughness_texture,
        ]
        .into_iter()
        .flatten()
        .any(|t| matches!(server.load_state(t.id()), LoadState::Loading))
    });
    if tour.waited.is_multiple_of(120) {
        info!(
            "shot {}: {count} chunks, textures loading: {loading}",
            shot.name
        );
    }
    tour.waited += 1;
    if count == tour.chunks && count > 0 && !loading {
        tour.stable += 1;
    } else {
        tour.stable = 0;
        tour.chunks = count;
    }
    if tour.stable >= tour.wait {
        // Una imagen dejada por un recorrido anterior no debe pasar por la de
        // este.
        std::fs::remove_file(&path).ok();
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
        tour.requested = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shot_list_gives_names_eyes_targets_and_optional_times() {
        let shots =
            parse_shots("# the bay\nbay 10 20 6  30 40 1.5\n\n  dusk -5.5 0 12 1 2 3 # comment\n")
                .unwrap();
        assert_eq!(shots.len(), 2);
        assert_eq!(shots[0].name, "bay");
        assert_eq!(shots[0].eye, (GroundVec2::new(10.0, 20.0), 6.0));
        assert_eq!(shots[0].target, (GroundVec2::new(30.0, 40.0), 1.5));
        assert_eq!(shots[0].time, None);
        let shots = parse_shots("noon 0 0 1 1 1 1 12:30").unwrap();
        assert_eq!(shots[0].time, Some(TimeOfDay::hm(12, 30)));
    }

    #[test]
    fn a_bad_shot_list_names_the_line() {
        for (text, line) in [
            ("a 1 2 3", "line 1"),
            ("\nb 1 2 x 4 5 6", "line 2"),
            ("c 1 2 3 4 5 6 24:00", "line 1"),
            ("c 1 2 3 4 5 6 noon", "line 1"),
        ] {
            let error = parse_shots(text).unwrap_err();
            assert!(error.starts_with(line), "{error}");
        }
    }
}
