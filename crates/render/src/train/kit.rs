//! Los materiales del tren y las meshes de sus ruedas, hechos una sola vez y
//! compartidos por todos los vagones.

use std::collections::HashMap;

use bevy::prelude::*;

use super::wheels::{WheelStyle, wheel_centre, wheel_tyre};
use crate::facade::{FacadeMaterials, Livery, Surface};

/// De qué está hecha una parte de un vagón: una superficie de los materiales de
/// los pueblos, o un acabado simple propio del tren.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Mat {
    Surface(Surface),
    /// Vidrio transparente de ventana.
    Glass,
    /// El vidrio encendido de un farol.
    Lamp,
    /// Carbón apilado en el ténder.
    Coal,
    /// Tapizado.
    Leather,
    /// Forro y molduras color crema.
    Cream,
    /// La alfombra del pasillo.
    Runner,
    /// Los techos de lona pintada de los coches.
    Canvas,
    /// Vidriado oscuro: las ventanas de la cabina, los paneles de una linterna.
    Dark,
}

impl Mat {
    /// Esmalte negro gastado: caldera, ténder, ruedas, bastidores.
    pub const BLACK: Mat = Mat::Surface(Surface::Livery(Livery::Black));
    /// Chapa antideslizante: plataformas laterales, planchas del maquinista,
    /// estribos.
    pub const TREAD: Mat = Mat::Surface(Surface::Livery(Livery::Tread));
    /// Testeros rojo oscuro.
    pub const BUFFER_RED: Mat = Mat::Surface(Surface::Livery(Livery::BufferRed));
    /// Tablas granate de los coches.
    pub const MAROON: Mat = Mat::Surface(Surface::Livery(Livery::Maroon));
    /// Madera marrón barnizada.
    pub const VARNISH: Mat = Mat::Surface(Surface::Livery(Livery::Varnish));
    /// Tablas de carga rojo óxido.
    pub const OXIDE: Mat = Mat::Surface(Surface::Livery(Livery::Oxide));
    /// Acero oxidado: bastidores, tolvas.
    pub const STEEL: Mat = Mat::Surface(Surface::RailSteel);
    /// La cara de rodadura pulida de una llanta.
    pub const TYRE: Mat = Mat::Surface(Surface::RailHead);
    /// Tablas curtidas comunes.
    pub const PLANKS: Mat = Mat::Surface(Surface::Planks);
    /// Bronce y cobre.
    pub const BRASS: Mat = Mat::Surface(Surface::Bronze);
    /// Chapa acanalada curtida.
    pub const OLD_IRON: Mat = Mat::Surface(Surface::OldIron);
}

/// Los radios de las ruedas (en metros) y sus tipos.
pub const WHEELS: [(f32, WheelStyle); 3] = [
    (super::DRIVER_RADIUS, WheelStyle::Spoked(12)),
    (SMALL_RADIUS, WheelStyle::Spoked(8)),
    (BOGIE_RADIUS, WheelStyle::Plate),
];

/// Las ruedas del bogie delantero y del ténder.
pub const SMALL_RADIUS: f32 = 0.5;
/// Las ruedas de los coches y de los vagones de carga.
pub const BOGIE_RADIUS: f32 = 0.45;

/// El radio, el estilo y el lado de una rueda, y las meshes de su llanta y su
/// centro.
type KitWheel = (f32, WheelStyle, f32, Handle<Mesh>, Handle<Mesh>);

#[derive(Clone)]
pub struct Kit {
    surfaces: FacadeMaterials,
    plain: HashMap<Mat, Handle<StandardMaterial>>,
    pub puff: Handle<Mesh>,
    pub smoke: Handle<StandardMaterial>,
    /// `(radius, style, side)` a la llanta de acero de la rueda y a su centro
    /// esmaltado.
    wheels: Vec<KitWheel>,
}

impl Kit {
    pub fn new(
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        surfaces: FacadeMaterials,
    ) -> Self {
        let mut paint = |mat: Mat, [r, g, b]: [f32; 3], metallic: f32, roughness: f32| {
            let material = StandardMaterial {
                base_color: Color::srgb(r, g, b),
                metallic,
                perceptual_roughness: roughness,
                ..default()
            };
            (mat, materials.add(material))
        };
        let mut plain: HashMap<Mat, Handle<StandardMaterial>> = [
            paint(Mat::Coal, [0.07, 0.07, 0.07], 0.0, 1.0),
            paint(Mat::Leather, [0.30, 0.09, 0.07], 0.0, 0.55),
            paint(Mat::Cream, [0.78, 0.70, 0.52], 0.0, 0.6),
            paint(Mat::Runner, [0.20, 0.07, 0.06], 0.0, 0.9),
            paint(Mat::Canvas, [0.55, 0.54, 0.49], 0.0, 0.9),
            paint(Mat::Dark, [0.05, 0.045, 0.04], 0.0, 0.4),
        ]
        .into();
        plain.insert(
            Mat::Glass,
            materials.add(StandardMaterial {
                base_color: Color::srgba(0.70, 0.78, 0.80, 0.18),
                perceptual_roughness: 0.1,
                reflectance: 0.6,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
        );
        plain.insert(
            Mat::Lamp,
            materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.85, 0.55),
                emissive: LinearRgba::rgb(3.0, 2.2, 1.0),
                ..default()
            }),
        );
        let smoke = materials.add(StandardMaterial {
            base_color: Color::srgba(0.55, 0.53, 0.50, 0.35),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 1.0,
            ..default()
        });
        let mut wheel_meshes = Vec::new();
        for (radius, style) in WHEELS {
            for side in [-1.0, 1.0] {
                wheel_meshes.push((
                    radius,
                    style,
                    side,
                    meshes.add(wheel_tyre(radius, style, side).into_mesh()),
                    meshes.add(wheel_centre(radius, style, side).into_mesh()),
                ));
            }
        }
        Self {
            surfaces,
            plain,
            puff: meshes.add(Sphere::new(0.5).mesh().ico(1).expect("a low sphere")),
            smoke,
            wheels: wheel_meshes,
        }
    }

    /// El material de `mat`.
    pub fn material(&self, mat: Mat) -> Handle<StandardMaterial> {
        match mat {
            Mat::Surface(surface) => self.surfaces.get(surface),
            plain => self.plain[&plain].clone(),
        }
    }

    /// Las meshes de la llanta y el centro de una rueda.
    pub fn wheel(&self, radius: f32, style: WheelStyle, side: f32) -> (Handle<Mesh>, Handle<Mesh>) {
        let (.., tyre, centre) = self
            .wheels
            .iter()
            .find(|w| w.0 == radius && w.1 == style && w.2 == side)
            .expect("a kit wheel");
        (tyre.clone(), centre.clone())
    }
}
