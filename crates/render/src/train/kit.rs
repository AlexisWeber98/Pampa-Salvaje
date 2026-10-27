//! The train's materials and its wheel meshes, made once and shared by
//! every car.

use std::collections::HashMap;

use bevy::prelude::*;

use super::wheels::{WheelStyle, wheel_centre, wheel_tyre};
use crate::facade::{FacadeMaterials, Livery, Surface};

/// What a part of a car is made of: a surface of the towns' materials, or
/// a plain finish of the train's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Mat {
    Surface(Surface),
    /// Clear window glass.
    Glass,
    /// A lamp's glowing glass.
    Lamp,
    /// Coal heaped in the tender.
    Coal,
    /// Upholstery.
    Leather,
    /// Cream lining and beading.
    Cream,
    /// The aisle runner.
    Runner,
    /// The coaches' painted canvas roofs.
    Canvas,
    /// Dark glazing: the cab's windows, a clerestory's panes.
    Dark,
}

impl Mat {
    /// Worn black enamel: boiler, tender, wheels, underframes.
    pub const BLACK: Mat = Mat::Surface(Surface::Livery(Livery::Black));
    /// Tread plate: running plates, footplates, steps.
    pub const TREAD: Mat = Mat::Surface(Surface::Livery(Livery::Tread));
    /// Dark red buffer beams.
    pub const BUFFER_RED: Mat = Mat::Surface(Surface::Livery(Livery::BufferRed));
    /// Maroon coach boards.
    pub const MAROON: Mat = Mat::Surface(Surface::Livery(Livery::Maroon));
    /// Varnished brown wood.
    pub const VARNISH: Mat = Mat::Surface(Surface::Livery(Livery::Varnish));
    /// Oxide red freight boards.
    pub const OXIDE: Mat = Mat::Surface(Surface::Livery(Livery::Oxide));
    /// Rusty steel: frames, hoppers.
    pub const STEEL: Mat = Mat::Surface(Surface::RailSteel);
    /// The polished running face of a tyre.
    pub const TYRE: Mat = Mat::Surface(Surface::RailHead);
    /// Plain weathered boards.
    pub const PLANKS: Mat = Mat::Surface(Surface::Planks);
    /// Brass and copper.
    pub const BRASS: Mat = Mat::Surface(Surface::Bronze);
    /// Weathered corrugated iron.
    pub const OLD_IRON: Mat = Mat::Surface(Surface::OldIron);
}

/// The wheels' radii (meters) and kinds.
pub const WHEELS: [(f32, WheelStyle); 3] = [
    (super::DRIVER_RADIUS, WheelStyle::Spoked(12)),
    (SMALL_RADIUS, WheelStyle::Spoked(8)),
    (BOGIE_RADIUS, WheelStyle::Plate),
];

/// The leading truck's and the tender's wheels.
pub const SMALL_RADIUS: f32 = 0.5;
/// The coaches' and freight cars' wheels.
pub const BOGIE_RADIUS: f32 = 0.45;

/// A wheel's radius, style and side, and its tyre and centre meshes.
type KitWheel = (f32, WheelStyle, f32, Handle<Mesh>, Handle<Mesh>);

#[derive(Clone)]
pub struct Kit {
    surfaces: FacadeMaterials,
    plain: HashMap<Mat, Handle<StandardMaterial>>,
    pub puff: Handle<Mesh>,
    pub smoke: Handle<StandardMaterial>,
    /// `(radius, style, side)` to the wheel's steel tyre and its enamelled
    /// centre.
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

    /// The material of `mat`.
    pub fn material(&self, mat: Mat) -> Handle<StandardMaterial> {
        match mat {
            Mat::Surface(surface) => self.surfaces.get(surface),
            plain => self.plain[&plain].clone(),
        }
    }

    /// A wheel's tyre and centre meshes.
    pub fn wheel(&self, radius: f32, style: WheelStyle, side: f32) -> (Handle<Mesh>, Handle<Mesh>) {
        let (.., tyre, centre) = self
            .wheels
            .iter()
            .find(|w| w.0 == radius && w.1 == style && w.2 == side)
            .expect("a kit wheel");
        (tyre.clone(), centre.clone())
    }
}
