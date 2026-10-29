//! Formas de colisión que pertenecen a la lógica, nunca derivadas de los
//! píxeles de los sprites.

use crate::math::Vec2;

/// Un círculo en el plano del suelo (una cápsula vertical vista desde arriba).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle {
    pub radius: f32,
}

impl Circle {
    pub const fn new(radius: f32) -> Self {
        Self { radius }
    }

    /// El desplazamiento más chico que saca de `aabb` a este círculo ubicado en
    /// `center`, o `None` si no se superponen.
    pub fn push_out(self, center: Vec2, aabb: &Aabb) -> Option<Vec2> {
        let closest = aabb.closest_point(center);
        let offset = center - closest;
        let distance = offset.length();
        if distance >= self.radius {
            return None;
        }
        if distance > 0.0 {
            return Some(offset * ((self.radius - distance) / distance));
        }
        // Centro dentro del rectángulo: sale por el borde más cercano.
        let exits = [
            (center.x - aabb.min.x, Vec2::new(-1.0, 0.0)),
            (aabb.max.x - center.x, Vec2::new(1.0, 0.0)),
            (center.y - aabb.min.y, Vec2::new(0.0, -1.0)),
            (aabb.max.y - center.y, Vec2::new(0.0, 1.0)),
        ];
        let (depth, normal) = exits
            .into_iter()
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .expect("four exits");
        Some(normal * (depth + self.radius))
    }

    /// El desplazamiento más chico que aleja a este círculo ubicado en `center`
    /// de `body`, o `None` si no se superponen. Los centros que coinciden se
    /// separan a lo largo de `+x`, para que el resultado siga siendo
    /// determinista.
    pub fn push_out_of_body(self, center: Vec2, body: &Body) -> Option<Vec2> {
        let offset = center - body.position;
        let distance = offset.length();
        let needed = self.radius + body.collider.radius;
        if distance >= needed {
            None
        } else if distance > 0.0 {
            Some(offset * ((needed - distance) / distance))
        } else {
            Some(Vec2::new(needed, 0.0))
        }
    }

    /// Si este círculo en `a` se superpone con `other` en `b`.
    pub fn overlaps(self, a: Vec2, other: Circle, b: Vec2) -> bool {
        a.distance(b) < self.radius + other.radius
    }
}

/// Otro personaje parado en algún lugar: un círculo en una posición.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    pub position: Vec2,
    pub collider: Circle,
}

/// Un rectángulo en movimiento que empuja los círculos de costado, a lo largo
/// de `across`, hacia el lado de su línea central en el que estén: un vagón de
/// tren que saca a los jinetes de su vía en lugar de llevarlos por delante.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SideObstacle {
    pub footprint: Aabb,
    /// Dirección unitaria transversal al obstáculo (transversal a la vía).
    pub across: Vec2,
}

impl SideObstacle {
    /// Desplazamiento que aleja a un `circle` ubicado en `center` de este
    /// rectángulo a lo largo de `across`, o `None` si no se superponen.
    pub fn push_out(&self, circle: Circle, center: Vec2) -> Option<Vec2> {
        circle.push_out(center, &self.footprint)?;
        // Distancia con signo desde la línea central del rectángulo, y su medio
        // ancho.
        let offset = (center - self.footprint.center()).dot(self.across);
        let size = self.footprint.size();
        let half = (self.across.x.abs() * size.x + self.across.y.abs() * size.y) / 2.0;
        let side = if offset >= 0.0 { 1.0 } else { -1.0 };
        Some(self.across * (side * (half + circle.radius) - offset))
    }
}

/// Rectángulo alineado con los ejes en el plano del suelo: la huella de un
/// obstáculo estático.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb {
    pub const fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    /// Rectángulo de tamaño `size` (ancho sobre `x`, profundidad sobre `y`)
    /// centrado en `center`.
    pub fn from_center_size(center: Vec2, size: Vec2) -> Self {
        let half = size * 0.5;
        Self::new(center - half, center + half)
    }

    pub fn center(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    /// Si los dos rectángulos comparten área interior (los bordes que se tocan
    /// no cuentan).
    pub fn intersects(&self, other: &Aabb) -> bool {
        self.min.x < other.max.x
            && other.min.x < self.max.x
            && self.min.y < other.max.y
            && other.min.y < self.max.y
    }

    /// Si `other` está completamente dentro de este rectángulo (bordes
    /// incluidos).
    pub fn contains(&self, other: &Aabb) -> bool {
        self.min.x <= other.min.x
            && self.min.y <= other.min.y
            && other.max.x <= self.max.x
            && other.max.y <= self.max.y
    }

    pub fn contains_point(&self, p: Vec2) -> bool {
        (self.min.x..=self.max.x).contains(&p.x) && (self.min.y..=self.max.y).contains(&p.y)
    }

    /// El punto de este rectángulo más cercano a `p`.
    pub fn closest_point(&self, p: Vec2) -> Vec2 {
        Vec2::new(
            p.x.clamp(self.min.x, self.max.x),
            p.y.clamp(self.min.y, self.max.y),
        )
    }

    /// Dónde cruza este rectángulo el segmento que va de `from` a `to`, como
    /// fracciones `(enter, exit)` del recorrido, recortadas a `0..=1`; `None`
    /// si no lo toca. Un segmento que empieza adentro entra en `0`.
    pub fn segment_overlap(&self, from: Vec2, to: Vec2) -> Option<(f32, f32)> {
        // Método de las franjas (slab): intersecta los rangos del parámetro
        // dentro de la franja de cada eje.
        let d = to - from;
        let mut enter = 0.0_f32;
        let mut exit = 1.0_f32;
        for (start, delta, lo, hi) in [
            (from.x, d.x, self.min.x, self.max.x),
            (from.y, d.y, self.min.y, self.max.y),
        ] {
            if delta == 0.0 {
                if start < lo || start > hi {
                    return None;
                }
                continue;
            }
            let (a, b) = ((lo - start) / delta, (hi - start) / delta);
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
            if enter > exit {
                return None;
            }
        }
        Some((enter, exit))
    }

    /// El rectángulo agrandado `margin` en cada lado.
    pub fn expanded(&self, margin: f32) -> Self {
        Self::new(
            self.min - Vec2::new(margin, margin),
            self.max + Vec2::new(margin, margin),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_depends_on_radii_sum() {
        let c = Circle::new(1.0);
        assert!(c.overlaps(Vec2::ZERO, Circle::new(0.5), Vec2::new(1.4, 0.0)));
        assert!(!c.overlaps(Vec2::ZERO, Circle::new(0.5), Vec2::new(1.6, 0.0)));
    }

    #[test]
    fn aabb_overlap_and_containment() {
        let a = Aabb::new(Vec2::ZERO, Vec2::new(4.0, 2.0));
        assert!(a.intersects(&Aabb::from_center_size(
            Vec2::new(4.0, 1.0),
            Vec2::new(1.0, 1.0)
        )));
        assert!(!a.intersects(&Aabb::new(Vec2::new(4.0, 0.0), Vec2::new(5.0, 2.0))));
        assert!(a.contains(&Aabb::new(Vec2::new(1.0, 0.0), Vec2::new(4.0, 1.0))));
        assert_eq!(a.center(), Vec2::new(2.0, 1.0));
        assert_eq!(a.closest_point(Vec2::new(9.0, -3.0)), Vec2::new(4.0, 0.0));
        assert_eq!(a.expanded(1.0).size(), Vec2::new(6.0, 4.0));
    }

    #[test]
    fn segment_overlap_reports_entry_and_exit_fractions() {
        let b = unit_box();
        // Directo a lo largo de x, desde 2 m al oeste hasta 2 m al este.
        let (enter, exit) = b
            .segment_overlap(Vec2::new(-2.0, 1.0), Vec2::new(6.0, 1.0))
            .unwrap();
        assert!((enter - 0.25).abs() < 1e-6 && (exit - 0.75).abs() < 1e-6);
        // La dirección invertida cambia por qué cara se entra.
        let (enter, exit) = b
            .segment_overlap(Vec2::new(6.0, 1.0), Vec2::new(-2.0, 1.0))
            .unwrap();
        assert!((enter - 0.25).abs() < 1e-6 && (exit - 0.75).abs() < 1e-6);
        // En diagonal a través de la zona de una esquina.
        let (enter, _) = b
            .segment_overlap(Vec2::new(-1.0, -1.0), Vec2::new(1.0, 1.0))
            .unwrap();
        assert!((enter - 0.5).abs() < 1e-6);
    }

    #[test]
    fn segment_overlap_clips_to_the_segment() {
        let b = unit_box();
        // Empieza adentro: entra en 0.
        let (enter, exit) = b
            .segment_overlap(Vec2::new(2.0, 1.0), Vec2::new(8.0, 1.0))
            .unwrap();
        assert_eq!(enter, 0.0);
        assert!((exit - 2.0 / 6.0).abs() < 1e-6);
        // Termina adentro: sale en 1.
        let (_, exit) = b
            .segment_overlap(Vec2::new(-2.0, 1.0), Vec2::new(1.0, 1.0))
            .unwrap();
        assert_eq!(exit, 1.0);
        // Completamente adentro, y un segmento degenerado de un solo punto
        // adentro.
        assert_eq!(
            b.segment_overlap(Vec2::new(1.0, 1.0), Vec2::new(3.0, 1.5)),
            Some((0.0, 1.0))
        );
        assert_eq!(
            b.segment_overlap(Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0)),
            Some((0.0, 1.0))
        );
    }

    #[test]
    fn segment_overlap_misses() {
        let b = unit_box();
        // Se detiene antes, pasa al lado, va paralelo por afuera, punto afuera.
        assert_eq!(
            b.segment_overlap(Vec2::new(-3.0, 1.0), Vec2::new(-1.0, 1.0)),
            None
        );
        assert_eq!(
            b.segment_overlap(Vec2::new(-1.0, 3.0), Vec2::new(5.0, 2.5)),
            None
        );
        assert_eq!(
            b.segment_overlap(Vec2::new(-1.0, 5.0), Vec2::new(9.0, 5.0)),
            None
        );
        assert_eq!(
            b.segment_overlap(Vec2::new(9.0, 9.0), Vec2::new(9.0, 9.0)),
            None
        );
    }

    fn car() -> SideObstacle {
        // Un vagón de 10 m a lo largo de x, de 3 m de ancho, centrado en el
        // origen.
        SideObstacle {
            footprint: Aabb::from_center_size(Vec2::ZERO, Vec2::new(10.0, 3.0)),
            across: Vec2::new(0.0, 1.0),
        }
    }

    #[test]
    fn side_obstacle_pushes_across_to_the_side_the_circle_is_on() {
        let c = Circle::new(1.0);
        assert_eq!(car().push_out(c, Vec2::new(0.0, 3.0)), None);
        // Mitad norte: hasta la cara norte más el radio.
        let push = car().push_out(c, Vec2::new(2.0, 0.5)).unwrap();
        assert!(close(push, Vec2::new(0.0, 2.0)), "{push:?}");
        // Al sur del rectángulo, superpuesto con su cara sur.
        let push = car().push_out(c, Vec2::new(-3.0, -2.0)).unwrap();
        assert!(close(push, Vec2::new(0.0, -0.5)), "{push:?}");
        // Justo delante del extremo del vagón: empujado de costado, nunca hacia
        // adelante.
        let push = car().push_out(c, Vec2::new(5.5, -0.1)).unwrap();
        assert!(push.x == 0.0 && push.y < 0.0, "{push:?}");
    }

    #[test]
    fn circles_push_each_other_apart_along_the_line_between_them() {
        let c = Circle::new(0.9);
        let walker = Body {
            position: Vec2::new(1.0, 0.0),
            collider: Circle::new(0.3),
        };
        assert_eq!(c.push_out_of_body(Vec2::new(-0.3, 0.0), &walker), None);
        assert_eq!(c.push_out_of_body(Vec2::new(1.0, 1.2), &walker), None);
        let push = c.push_out_of_body(Vec2::new(0.0, 0.0), &walker).unwrap();
        assert!(close(push, Vec2::new(-0.2, 0.0)), "{push:?}");
        let push = c.push_out_of_body(Vec2::new(1.0, 0.6), &walker).unwrap();
        assert!(close(push, Vec2::new(0.0, 0.6)), "{push:?}");
        // Exactamente uno encima del otro: se separan a lo largo de +x.
        let push = c.push_out_of_body(walker.position, &walker).unwrap();
        assert!(close(push, Vec2::new(1.2, 0.0)), "{push:?}");
    }

    fn unit_box() -> Aabb {
        Aabb::new(Vec2::ZERO, Vec2::new(4.0, 2.0))
    }

    fn close(a: Vec2, b: Vec2) -> bool {
        a.distance(b) < 1e-5
    }

    #[test]
    fn separate_circle_needs_no_push() {
        let c = Circle::new(1.0);
        assert_eq!(c.push_out(Vec2::new(-1.5, 1.0), &unit_box()), None);
        assert_eq!(c.push_out(Vec2::new(-1.0, 1.0), &unit_box()), None);
        // Cerca de una esquina pero fuera de la zona redondeada de la esquina.
        assert_eq!(c.push_out(Vec2::new(-0.8, -0.8), &unit_box()), None);
    }

    #[test]
    fn pushes_out_through_the_nearest_face() {
        let c = Circle::new(1.0);
        let push = c.push_out(Vec2::new(-0.5, 1.0), &unit_box()).unwrap();
        assert!(close(push, Vec2::new(-0.5, 0.0)), "{push:?}");
        let push = c.push_out(Vec2::new(2.0, 2.25), &unit_box()).unwrap();
        assert!(close(push, Vec2::new(0.0, 0.75)), "{push:?}");
    }

    #[test]
    fn pushes_out_diagonally_at_corners() {
        let c = Circle::new(1.0);
        let center = Vec2::new(4.5, 2.5);
        let push = c.push_out(center, &unit_box()).unwrap();
        let resolved = center + push;
        assert!((resolved.distance(Vec2::new(4.0, 2.0)) - 1.0).abs() < 1e-5);
        assert!((push.x - push.y).abs() < 1e-5 && push.x > 0.0);
    }

    #[test]
    fn center_inside_box_exits_through_the_closest_edge() {
        let c = Circle::new(1.0);
        // Más cerca del borde inferior (y = 0).
        let push = c.push_out(Vec2::new(1.5, 0.4), &unit_box()).unwrap();
        assert!(close(push, Vec2::new(0.0, -1.4)), "{push:?}");
        // Más cerca del borde derecho (x = 4).
        let push = c.push_out(Vec2::new(3.8, 1.0), &unit_box()).unwrap();
        assert!(close(push, Vec2::new(1.2, 0.0)), "{push:?}");
    }
}
