//! Lagunas: las "lagunas" poco profundas de agua dulce de la llanura.
//!
//! Una [`Lagoon`] es una cuenca hundida en el [`Terrain`] alrededor de una
//! orilla irregular, que contiene una lámina plana de agua. Su orilla es un
//! rectángulo redondeado (una superelipse, como la real Laguna La Arocena, de
//! forma casi cuadrada), irregularizado por unos pocos armónicos pequeños del
//! radio, con semilla, de modo que sigue siendo estrellada respecto del centro;
//! su fondo baja en pendiente desde la orilla y su barranca vuelve suavemente a
//! la pampa. El agua queda un poco por debajo del punto más bajo de la
//! barranca, así que nunca desborda. Las personas y los caballos vadean las
//! partes bajas y se detienen donde el agua se vuelve profunda
//! ([`Lagoon::push_out`]); los alambrados, los asentamientos, los caminos y los
//! árboles se mantienen fuera de su rectángulo [`Lagoon::keep_out`].

use std::f32::consts::TAU;
use std::fmt;

use crate::rng::Rng;
use crate::{Aabb, RailLine, RoadNetwork, Terrain, TownLayout, Vec2, WorldConfig};

/// Cuántos armónicos irregularizan la orilla (del tercero al sexto).
const HARMONICS: usize = 4;

/// Una laguna tal como se configura: dónde está y qué tamaño tiene.
#[derive(Debug, Clone, PartialEq)]
pub struct LagoonConfig {
    /// Su nombre, para las advertencias y el mapa.
    pub name: String,
    /// Centro del agua, en metros.
    pub center: Vec2,
    /// Tamaño total del rectángulo redondeado que sigue la orilla: primero
    /// este-oeste y después norte-sur, en metros.
    pub size: Vec2,
    /// Varía la ondulación de la orilla.
    pub seed: u32,
}

/// Por qué no se puede ubicar una laguna.
#[derive(Debug, Clone, PartialEq)]
pub enum LagoonError {
    /// Un tamaño fuera del rango de [`Lagoon::MIN_SIZE`] a
    /// [`Lagoon::MAX_SIZE`], o un número que no es finito.
    BadSize { name: String },
    /// Su barranca se sale del borde del mundo.
    OutsideWorld { name: String },
    /// Su barranca toca la franja nivelada de un pueblo.
    Town { name: String, town: String },
    /// La vía férrea atraviesa su barranca.
    Railway { name: String },
    /// Un camino atraviesa su barranca.
    Road { name: String },
}

impl fmt::Display for LagoonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LagoonError::BadSize { name } => write!(
                f,
                "lagoon {name}: size must be between {} and {} m each way",
                Lagoon::MIN_SIZE,
                Lagoon::MAX_SIZE
            ),
            LagoonError::OutsideWorld { name } => {
                write!(f, "lagoon {name}: its bank reaches past the world's edge")
            }
            LagoonError::Town { name, town } => {
                write!(f, "lagoon {name}: its bank touches the town {town}")
            }
            LagoonError::Railway { name } => {
                write!(f, "lagoon {name}: the railway runs across its bank")
            }
            LagoonError::Road { name } => write!(f, "lagoon {name}: a road runs across its bank"),
        }
    }
}

impl std::error::Error for LagoonError {}

/// Una laguna poco profunda hundida en el terreno (ver la documentación del
/// módulo).
#[derive(Debug, Clone, PartialEq)]
pub struct Lagoon {
    pub name: String,
    pub center: Vec2,
    /// Medios lados del rectángulo redondeado que sigue la orilla, en metros.
    pub radii: Vec2,
    /// Amplitud (una fracción del radio) y fase de cada armónico.
    wobble: [(f32, f32); HARMONICS],
    /// Altura de la superficie del agua, en metros.
    pub level: f32,
}

impl Lagoon {
    /// Profundidad del agua en el medio, en metros: una laguna de llanura es
    /// poco profunda.
    pub const DEPTH: f32 = 1.6;
    /// A qué distancia de la orilla hacia adentro el fondo alcanza su
    /// profundidad máxima, en metros.
    pub const SHELF: f32 = 30.0;
    /// Ancho de la barranca donde el suelo pasa suavemente del borde del agua a
    /// la pampa, en metros.
    pub const BANK: f32 = 30.0;
    /// Hasta qué distancia de la orilla hacia adentro pueden vadear las
    /// personas y los caballos, en metros (alrededor de medio metro de agua);
    /// más allá, el agua es demasiado profunda.
    pub const WADE: f32 = 5.0;
    /// Altura mínima de la barranca sobre el agua, en metros.
    pub const FREEBOARD: f32 = 0.6;
    /// Margen más allá de la barranca del que se mantienen alejados los
    /// alambrados, los asentamientos, los caminos y los árboles, en metros.
    pub const MARGIN: f32 = 15.0;
    /// Tamaño mínimo y máximo en cada dirección, en metros.
    pub const MIN_SIZE: f32 = 40.0;
    pub const MAX_SIZE: f32 = 1500.0;
    /// Suma máxima de las amplitudes de los armónicos: la orilla se mantiene
    /// dentro de esta fracción de su rectángulo redondeado.
    const MAX_WOBBLE: f32 = 0.05;
    /// Exponente de la superelipse de la orilla: 2 es una elipse; un valor más
    /// alto es más cuadrado, con esquinas suavemente redondeadas.
    const SQUARENESS: f32 = 5.0;
    /// Ancho del barro húmedo a lo largo de la línea del agua, en metros: más
    /// allá vuelve a crecer el pasto ([`Self::is_bare`]).
    pub const MUD_BAND: f32 = 3.0;

    /// La laguna que describe `config`, con su agua justo por debajo del punto
    /// natural más bajo de su barranca en `terrain`.
    pub fn new(config: &LagoonConfig, terrain: &Terrain) -> Self {
        let mut rng = Rng::new(config.seed ^ LAGOON_SEED, 0);
        let mut wobble = [(0.0, 0.0); HARMONICS];
        for (k, w) in wobble.iter_mut().enumerate() {
            // Los armónicos más bajos curvan más la orilla: primero una bahía
            // poco profunda, después ensenadas.
            let share = Self::MAX_WOBBLE * [0.34, 0.28, 0.22, 0.16][k];
            *w = (share * rng.range(0.55, 1.0), rng.unit() * TAU);
        }
        let mut lagoon = Self {
            name: config.name.clone(),
            center: config.center,
            radii: config.size * 0.5,
            wobble,
            level: 0.0,
        };
        let lowest = lagoon
            .bank_samples()
            .map(|p| terrain.natural_height(p.x, p.y))
            .fold(f32::MAX, f32::min);
        lagoon.level = lowest - Self::FREEBOARD;
        lagoon
    }

    /// Puntos alrededor de la barranca, desde la orilla hasta su borde
    /// exterior: donde se compara el nivel del agua con el terreno.
    fn bank_samples(&self) -> impl Iterator<Item = Vec2> + '_ {
        const ANGLES: usize = 96;
        (0..ANGLES).flat_map(move |k| {
            let angle = k as f32 * TAU / ANGLES as f32;
            let dir = Vec2::new(angle.cos(), angle.sin());
            let r = self.shore_radius(angle);
            (0..=6).map(move |s| self.center + dir * (r + Self::BANK * s as f32 / 6.0))
        })
    }

    /// Distancia desde el centro hasta la orilla en `angle` (radianes desde el
    /// este).
    pub fn shore_radius(&self, angle: f32) -> f32 {
        let rectangle = self.rounded_rectangle(angle);
        let wobble: f32 = self
            .wobble
            .iter()
            .enumerate()
            .map(|(k, &(amp, phase))| amp * ((k + 3) as f32 * angle + phase).sin())
            .sum();
        rectangle * (1.0 + wobble)
    }

    /// Distancia desde el centro hasta el rectángulo redondeado suave (la
    /// superelipse `|x/a|^n + |y/b|^n = 1`) en `angle`, antes de que los
    /// armónicos lo irregularicen.
    pub fn rounded_rectangle(&self, angle: f32) -> f32 {
        let n = Self::SQUARENESS;
        let (c, s) = (angle.cos().abs(), angle.sin().abs());
        ((c / self.radii.x).powf(n) + (s / self.radii.y).powf(n)).powf(-1.0 / n)
    }

    /// Si `p` está en el agua o en el barro húmedo pelado justo por encima de
    /// ella, donde no crece pasto.
    pub fn is_bare(&self, p: Vec2) -> bool {
        self.offshore(p) < Self::MUD_BAND
    }

    /// A qué distancia más allá de la orilla está `p`, en metros, sobre la
    /// línea desde el centro: negativa en el agua.
    pub fn offshore(&self, p: Vec2) -> f32 {
        let d = p - self.center;
        d.length() - self.shore_radius(d.y.atan2(d.x))
    }

    /// Si `p` está en el agua.
    pub fn is_water(&self, p: Vec2) -> bool {
        self.offshore(p) < 0.0
    }

    /// Si `p` está en agua demasiado profunda para vadear.
    pub fn is_deep(&self, p: Vec2) -> bool {
        self.offshore(p) < -Self::WADE
    }

    /// `n` puntos a lo largo de la orilla, en sentido antihorario desde el
    /// este.
    pub fn shore(&self, n: usize) -> Vec<Vec2> {
        self.ring(n, 0.0)
    }

    /// `n` puntos a `offset` metros más allá de la orilla (negativo: en el
    /// agua), en sentido antihorario desde el este.
    pub fn ring(&self, n: usize, offset: f32) -> Vec<Vec2> {
        (0..n)
            .map(|k| {
                let angle = k as f32 * TAU / n as f32;
                let r = (self.shore_radius(angle) + offset).max(0.0);
                self.center + Vec2::new(angle.cos(), angle.sin()) * r
            })
            .collect()
    }

    /// Rectángulo alrededor de toda el agua.
    pub fn bounds(&self) -> Aabb {
        let reach = 1.0 + Self::MAX_WOBBLE;
        Aabb::from_center_size(self.center, self.radii * (2.0 * reach))
    }

    /// Rectángulo alrededor del agua, su barranca y [`Self::MARGIN`]: aquello
    /// de lo que se mantienen fuera los alambrados, los asentamientos, los
    /// caminos y los árboles.
    pub fn keep_out(&self) -> Aabb {
        self.bounds().expanded(Self::BANK + Self::MARGIN)
    }

    /// Profundidad del agua en `p`, en metros: cero en tierra firme.
    pub fn depth_at(&self, p: Vec2) -> f32 {
        let u = self.offshore(p);
        if u >= 0.0 {
            return 0.0;
        }
        let t = (-u / Self::SHELF).min(1.0);
        // Más empinado en la orilla, plano en el medio.
        Self::DEPTH * (1.0 - (1.0 - t) * (1.0 - t))
    }

    /// El suelo en `p`, dado el `ground` que lo rodea sin la laguna: su fondo
    /// bajo el agua, su barranca volviendo suavemente a `ground` a lo largo de
    /// [`Self::BANK`] metros, y el propio `ground` más allá de la barranca.
    pub fn shape(&self, p: Vec2, ground: f32) -> f32 {
        let u = self.offshore(p);
        if u < 0.0 {
            return self.level - self.depth_at(p);
        }
        if u >= Self::BANK {
            return ground;
        }
        let s = u / Self::BANK;
        // Sube desde el borde del agua y se une suavemente con el terreno en lo
        // alto.
        let ease = 1.0 - (1.0 - s) * (1.0 - s);
        self.level + (ground - self.level) * ease
    }

    /// Si la laguna puede llegar a cambiar el suelo en `p`.
    pub fn touches(&self, p: Vec2) -> bool {
        self.bounds().expanded(Self::BANK).contains_point(p)
    }

    /// Cuánto hay que mover un cuerpo parado en `center` para sacarlo del agua
    /// profunda y llevarlo de vuelta a donde todavía puede vadear: `None`
    /// cuando no está en agua profunda.
    pub fn push_out(&self, center: Vec2) -> Option<Vec2> {
        let d = center - self.center;
        let r = d.length();
        let angle = d.y.atan2(d.x);
        let wade = self.shore_radius(angle) - Self::WADE;
        if r >= wade {
            return None;
        }
        let dir = if r > 1e-4 {
            d * (1.0 / r)
        } else {
            Vec2::new(1.0, 0.0)
        };
        Some(dir * (wade - r + 1e-3))
    }
}

/// Sal de la semilla de la orilla de cada laguna.
const LAGOON_SEED: u32 = 0x1a6_0a5e;

/// Ubica cada laguna de `configs` en `terrain`, dejando fuera (con un error
/// para cada una) las demasiado chicas o demasiado grandes, las que se salen
/// del borde del mundo o aquellas cuya barranca toca la franja nivelada de un
/// pueblo, la vía férrea `line` o un camino.
pub fn place_lagoons(
    config: &WorldConfig,
    terrain: &Terrain,
    towns: &[TownLayout],
    line: &RailLine,
    roads: &RoadNetwork,
    configs: &[LagoonConfig],
) -> (Vec<Lagoon>, Vec<LagoonError>) {
    let bounds = config.bounds();
    let (mut laid, mut errors) = (Vec::new(), Vec::new());
    for c in configs {
        let name = c.name.clone();
        let sized = [c.center.x, c.center.y, c.size.x, c.size.y]
            .iter()
            .all(|v| v.is_finite())
            && [c.size.x, c.size.y]
                .iter()
                .all(|s| (Lagoon::MIN_SIZE..=Lagoon::MAX_SIZE).contains(s));
        if !sized {
            errors.push(LagoonError::BadSize { name });
            continue;
        }
        let lagoon = Lagoon::new(c, terrain);
        let keep = lagoon.keep_out();
        let rails = line
            .segments()
            .chain(towns.iter().filter_map(|t| t.railway))
            .any(|r| keep.segment_overlap(r.start, r.end).is_some());
        let error = if !Aabb::new(bounds.min, bounds.max).contains(&keep) {
            Some(LagoonError::OutsideWorld { name })
        } else if let Some(town) = towns.iter().find(|t| t.leveled().intersects(&keep)) {
            Some(LagoonError::Town {
                name,
                town: town.name.clone(),
            })
        } else if rails {
            Some(LagoonError::Railway { name })
        } else if roads
            .segments()
            .any(|(a, b)| keep.segment_overlap(a, b).is_some())
        {
            Some(LagoonError::Road { name })
        } else {
            None
        };
        match error {
            Some(error) => errors.push(error),
            None => laid.push(lagoon),
        }
    }
    (laid, errors)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn arocena_config() -> LagoonConfig {
        LagoonConfig {
            name: "Laguna La Arocena".into(),
            center: Vec2::new(3401.0, 841.0),
            size: Vec2::new(215.0, 145.0),
            seed: 5,
        }
    }

    fn arocena() -> (Terrain, Lagoon) {
        let terrain = Terrain::default();
        let lagoon = Lagoon::new(&arocena_config(), &terrain);
        let terrain = terrain.with_lagoon(lagoon.clone());
        (terrain, lagoon)
    }

    #[test]
    fn the_shore_is_a_slightly_rough_rounded_rectangle_and_is_seeded() {
        let (_, lagoon) = arocena();
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for k in 0..360 {
            let angle = (k as f32).to_radians();
            let share = lagoon.shore_radius(angle) / lagoon.rounded_rectangle(angle);
            lo = lo.min(share);
            hi = hi.max(share);
            assert!(lagoon.bounds().contains_point(lagoon.shore(360)[k]));
        }
        // Un poco irregular, pero sigue siendo el rectángulo con el que se
        // configuró.
        assert!(hi - lo > 0.02, "{lo}..{hi}");
        assert!(lo > 0.94 && hi < 1.06, "{lo}..{hi}");
        // Casi cuadrada: llena la mayor parte del rectángulo que rodea su
        // orilla (una elipse llena el 79 %).
        let shore = lagoon.shore(720);
        let (min, max) = shore.iter().fold(
            (Vec2::new(f32::MAX, f32::MAX), Vec2::new(f32::MIN, f32::MIN)),
            |(lo, hi), p| {
                (
                    Vec2::new(lo.x.min(p.x), lo.y.min(p.y)),
                    Vec2::new(hi.x.max(p.x), hi.y.max(p.y)),
                )
            },
        );
        let (mut inside, mut total) = (0, 0);
        for i in 0..200 {
            for j in 0..200 {
                let p = Vec2::new(
                    min.x + (max.x - min.x) * (i as f32 + 0.5) / 200.0,
                    min.y + (max.y - min.y) * (j as f32 + 0.5) / 200.0,
                );
                total += 1;
                inside += usize::from(lagoon.is_water(p));
            }
        }
        let fill = inside as f32 / total as f32;
        assert!(fill > 0.85, "{fill}");
        // Alrededor de 1,5 a 1, más larga de este a oeste.
        let aspect = (max.x - min.x) / (max.y - min.y);
        assert!((1.35..1.65).contains(&aspect), "{aspect}");
        assert_eq!(Lagoon::new(&arocena_config(), &Terrain::default()), lagoon);
        let other = LagoonConfig {
            seed: 6,
            ..arocena_config()
        };
        assert_ne!(
            Lagoon::new(&other, &Terrain::default()).shore(8),
            lagoon.shore(8)
        );
    }

    #[test]
    fn only_a_narrow_band_of_mud_is_bare() {
        let (_, lagoon) = arocena();
        for p in lagoon.ring(64, Lagoon::MUD_BAND - 0.5) {
            assert!(lagoon.is_bare(p));
        }
        for p in lagoon.ring(64, Lagoon::MUD_BAND + 0.5) {
            assert!(!lagoon.is_bare(p));
        }
        const { assert!(Lagoon::MUD_BAND <= 5.0) };
    }

    #[test]
    fn the_water_is_flat_below_its_whole_bank_and_deepest_inside() {
        let (terrain, lagoon) = arocena();
        // La barranca queda por encima del agua en todo el contorno: nunca
        // desborda.
        for p in lagoon.ring(128, 0.5) {
            assert!(terrain.height(p.x, p.y) > lagoon.level, "{p:?}");
        }
        for p in lagoon.ring(128, Lagoon::BANK * 0.5) {
            assert!(terrain.height(p.x, p.y) >= lagoon.level + Lagoon::FREEBOARD * 0.5);
        }
        // El fondo está bajo el agua en todos los puntos dentro de la orilla, y
        // es más profundo en el medio.
        for p in lagoon.ring(128, -0.5).into_iter().chain([lagoon.center]) {
            let h = terrain.height(p.x, p.y);
            assert!(h < lagoon.level, "{p:?}: {h} vs {}", lagoon.level);
        }
        let middle = terrain.height(lagoon.center.x, lagoon.center.y);
        assert!((lagoon.level - middle - Lagoon::DEPTH).abs() < 1e-3);
        // Más allá de la barranca la pampa está intacta.
        for p in lagoon.ring(64, Lagoon::BANK + 1.0) {
            assert_eq!(terrain.height(p.x, p.y), terrain.natural_height(p.x, p.y));
        }
    }

    #[test]
    fn the_basin_is_continuous_and_gentle() {
        let (terrain, lagoon) = arocena();
        let step = 0.25;
        for k in 0..24 {
            let angle = k as f32 * TAU / 24.0;
            let dir = Vec2::new(angle.cos(), angle.sin());
            let reach = lagoon.shore_radius(angle) + Lagoon::BANK + 10.0;
            let mut last = terrain.height(lagoon.center.x, lagoon.center.y);
            let mut r = step;
            while r < reach {
                let p = lagoon.center + dir * r;
                let h = terrain.height(p.x, p.y);
                assert!((h - last).abs() / step < 0.36, "too steep at {p:?}");
                last = h;
                r += step;
            }
        }
    }

    #[test]
    fn deep_water_pushes_a_body_back_to_the_shallows() {
        let (_, lagoon) = arocena();
        assert_eq!(lagoon.push_out(lagoon.center + Vec2::new(500.0, 0.0)), None);
        // Vadear las partes bajas está bien.
        for p in lagoon.ring(64, -Lagoon::WADE + 0.5) {
            assert_eq!(lagoon.push_out(p), None, "{p:?}");
            assert!(lagoon.is_water(p) && !lagoon.is_deep(p));
        }
        for p in lagoon.ring(64, -20.0).into_iter().chain([lagoon.center]) {
            assert!(lagoon.is_deep(p));
            let out = p + lagoon.push_out(p).expect("deep water");
            assert!(!lagoon.is_deep(out), "{out:?}");
            assert!(lagoon.is_water(out));
        }
    }

    #[test]
    fn a_lagoon_on_a_town_off_the_world_or_too_small_is_left_out() {
        use crate::{PlazaPlacement, TownConfig, place_towns};
        let world = WorldConfig::default();
        let town = TownConfig {
            origin: Vec2::new(-1500.0, 0.0),
            plaza: PlazaPlacement::Auto,
            railway: None,
            ..TownConfig::castex()
        };
        let (towns, _) = place_towns(&world, &[town]);
        let (line, _) = RailLine::build(None, &towns, world.bounds());
        let roads = RoadNetwork::new(Vec::new(), &[]);
        let at = |name: &str, x: f32, y: f32, size: f32| LagoonConfig {
            name: name.into(),
            center: Vec2::new(x, y),
            size: Vec2::new(size, size * 0.7),
            seed: 1,
        };
        let configs = [
            at("ok", 1500.0, 0.0, 200.0),
            at("town", -1500.0, 0.0, 200.0),
            at("edge", 3990.0, 0.0, 200.0),
            at("small", 0.0, 1500.0, 10.0),
            at("nan", f32::NAN, 0.0, 200.0),
        ];
        let (laid, errors) =
            place_lagoons(&world, &Terrain::default(), &towns, &line, &roads, &configs);
        assert_eq!(laid.len(), 1);
        assert_eq!(laid[0].name, "ok");
        assert!(matches!(errors[0], LagoonError::Town { .. }), "{errors:?}");
        assert!(matches!(errors[1], LagoonError::OutsideWorld { .. }));
        assert!(matches!(errors[2], LagoonError::BadSize { .. }));
        assert!(matches!(errors[3], LagoonError::BadSize { .. }));
        for e in &errors {
            assert!(e.to_string().starts_with("lagoon "), "{e}");
        }
    }
}
