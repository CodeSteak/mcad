//! [`Exact`]: a kernel wrapper that carries the exact volume of a shape where it follows from the
//! parameters of the operation (`diff-kernels`, plan 0.4): box, sphere, cylinder/cone frustum,
//! torus, an extrusion of a polygon or circle (also with twist and scale: A·|h|·∫₀¹ sx·sy dt), a
//! revolution of a polygon or circle (Pappus), and every affine image of these (volume × |det|). Other operations give `None`.

use std::f64::consts::PI;

use cad_kernel::{
    BoolOp, Class, EdgeId, EdgeInfo, ExportFormat, FaceId, FaceInfo, KResult, Kernel, Loop, MassProps, Mesh,
    PathInterp, PlacedLoop, PlacedPath, Tag, brep,
};
use glam::{DMat4, DVec2, DVec3};

/// `K` with the exact volume next to each shape.
pub struct Exact<K>(pub K);

/// A shape and its exact volume, if known.
pub type Shape<S> = (S, Option<f64>);

/// Area of a closed loop in its own plane (polygon, circle).
fn loop_area(lp: &Loop) -> Option<f64> {
    match lp {
        Loop::Polygon(ps) if ps.len() >= 3 => Some(shoelace(ps).abs()),
        Loop::Circle { r, .. } => Some(PI * r * r),
        _ => None,
    }
}

/// Signed area of a polygon.
fn shoelace(ps: &[DVec2]) -> f64 {
    (0..ps.len()).map(|i| ps[i].perp_dot(ps[(i + 1) % ps.len()])).sum::<f64>() / 2.0
}

/// Volume of extruding `p` by `h` with twist and `scale`: the section at height t·h is the
/// profile turned by twist·t (area unchanged) and scaled by 1 + (scale − 1)·t per axis, so
/// A·|h|·∫₀¹ sx(t)·sy(t) dt, times the frame's |det|.
fn extrude_volume(p: &PlacedLoop, h: f64, scale: DVec2) -> Option<f64> {
    let (a, b) = (scale.x - 1.0, scale.y - 1.0);
    let factor = 1.0 + (a + b) / 2.0 + a * b / 3.0;
    loop_area(&p.lp).map(|area| area * h.abs() * factor * det3(&p.frame))
}

/// Volume of revolving `p` around the z-axis by `angle` radians (Pappus: angle × first moment
/// of the area about the axis). The loop must lie in the xz-plane after its frame (as the
/// kernels expect, x ≥ 0); a circle needs a frame without distortion in its plane.
fn revolve_volume(p: &PlacedLoop, angle: f64) -> Option<f64> {
    let angle = angle.abs().min(2.0 * PI);
    let to_xz = |q: DVec2| {
        let w = p.frame.transform_point3(DVec3::new(q.x, q.y, 0.0));
        (w.y.abs() <= 1e-9 * (1.0 + w.length())).then_some(DVec2::new(w.x, w.z))
    };
    match &p.lp {
        Loop::Polygon(ps) if ps.len() >= 3 => {
            let q: Vec<DVec2> = ps.iter().map(|&v| to_xz(v)).collect::<Option<_>>()?;
            // ∫∫ x dA over the polygon in the (x, z) plane
            let moment: f64 = (0..q.len())
                .map(|i| {
                    let (a, b) = (q[i], q[(i + 1) % q.len()]);
                    (a.x + b.x) * a.perp_dot(b)
                })
                .sum::<f64>()
                / 6.0;
            Some(angle * moment.abs())
        }
        Loop::Circle { center, r } => {
            let c = to_xz(*center)?;
            // the whole circle in the xz-plane
            for d in [DVec2::X, DVec2::Y] {
                to_xz(*center + d * *r)?;
            }
            let (ex, ey) = (p.frame.x_axis.truncate(), p.frame.y_axis.truncate());
            let (lx, ly) = (ex.length(), ey.length());
            if (lx - ly).abs() > 1e-12 * lx.max(ly) || ex.dot(ey).abs() > 1e-12 * lx * ly {
                return None;
            }
            let r = r * lx;
            (c.x >= r).then_some(angle * PI * r * r * c.x)
        }
        _ => None,
    }
}

fn det3(m: &DMat4) -> f64 {
    glam::DMat3::from_mat4(*m).determinant().abs()
}

fn strip<S: Clone>(shapes: &[Shape<S>]) -> Vec<S> {
    shapes.iter().map(|s| s.0.clone()).collect()
}

impl<K: Kernel> Kernel for Exact<K> {
    type Shape = Shape<K::Shape>;

    fn id(&self) -> &'static str {
        self.0.id()
    }

    fn make_box(&mut self, min: DVec3, max: DVec3, tag: Tag) -> KResult<Self::Shape> {
        let d = max - min;
        self.0.make_box(min, max, tag).map(|s| (s, Some((d.x * d.y * d.z).abs())))
    }

    fn make_sphere(&mut self, r: f64, tag: Tag) -> KResult<Self::Shape> {
        self.0.make_sphere(r, tag).map(|s| (s, Some(4.0 / 3.0 * PI * r.powi(3))))
    }

    fn make_cylinder(&mut self, h: f64, r1: f64, r2: f64, tag: Tag) -> KResult<Self::Shape> {
        let v = PI * h.abs() * (r1 * r1 + r1 * r2 + r2 * r2) / 3.0;
        self.0.make_cylinder(h, r1, r2, tag).map(|s| (s, Some(v)))
    }

    fn make_torus(&mut self, big_r: f64, r: f64, tag: Tag) -> KResult<Self::Shape> {
        let v = (r < big_r).then_some(2.0 * PI * PI * big_r * r * r);
        self.0.make_torus(big_r, r, tag).map(|s| (s, v))
    }

    fn make_polyhedron(&mut self, points: &[DVec3], faces: &[Vec<u32>], tag: Tag) -> KResult<Self::Shape> {
        self.0.make_polyhedron(points, faces, tag).map(|s| (s, None))
    }

    fn transform(&mut self, s: &Self::Shape, m: &DMat4) -> KResult<Self::Shape> {
        self.0.transform(&s.0, m).map(|t| (t, s.1.map(|v| v * det3(m))))
    }

    fn boolean(&mut self, op: BoolOp, shapes: &[Self::Shape], tag: Tag) -> KResult<Self::Shape> {
        self.0.boolean(op, &strip(shapes), tag).map(|s| (s, None))
    }

    fn fillet(&mut self, s: &Self::Shape, edges: &[EdgeId], r: f64, tag: Tag) -> KResult<Self::Shape> {
        self.0.fillet(&s.0, edges, r, tag).map(|s| (s, None))
    }

    fn chamfer(&mut self, s: &Self::Shape, edges: &[EdgeId], d: f64, tag: Tag) -> KResult<Self::Shape> {
        self.0.chamfer(&s.0, edges, d, tag).map(|s| (s, None))
    }

    fn law_stations(&mut self, s: &Self::Shape, edges: &[EdgeId], counts: &[u32]) -> KResult<Vec<Vec<DVec3>>> {
        self.0.law_stations(&s.0, edges, counts)
    }

    fn fillet_variable(
        &mut self,
        s: &Self::Shape,
        edges: &[EdgeId],
        radii: &[Vec<f64>],
        tag: Tag,
    ) -> KResult<Self::Shape> {
        self.0.fillet_variable(&s.0, edges, radii, tag).map(|s| (s, None))
    }

    fn extrude(&mut self, p: &PlacedLoop, h: f64, twist: f64, scale: DVec2, tag: Tag) -> KResult<Self::Shape> {
        let v = extrude_volume(p, h, scale);
        self.0.extrude(p, h, twist, scale, tag).map(|s| (s, v))
    }

    fn revolve(&mut self, p: &PlacedLoop, angle: f64, tag: Tag) -> KResult<Self::Shape> {
        let v = revolve_volume(p, angle);
        self.0.revolve(p, angle, tag).map(|s| (s, v))
    }

    fn loft(&mut self, sections: &[PlacedLoop], ruled: bool, tag: Tag) -> KResult<Self::Shape> {
        self.0.loft(sections, ruled, tag).map(|s| (s, None))
    }

    fn sweep(&mut self, p: &Loop, path: &[DVec3], interp: PathInterp, tag: Tag) -> KResult<Self::Shape> {
        self.0.sweep(p, path, interp, tag).map(|s| (s, None))
    }

    fn sweep_path(&mut self, p: &Loop, path: &PlacedPath, tag: Tag) -> KResult<Self::Shape> {
        self.0.sweep_path(p, path, tag).map(|s| (s, None))
    }

    fn nurbs_surface(&mut self, rows: &[Vec<DVec3>], tag: Tag) -> KResult<Self::Shape> {
        self.0.nurbs_surface(rows, tag).map(|s| (s, None))
    }

    fn thicken(&mut self, s: &Self::Shape, t: f64, tag: Tag) -> KResult<Self::Shape> {
        self.0.thicken(&s.0, t, tag).map(|s| (s, None))
    }

    fn shell(&mut self, s: &Self::Shape, t: f64, open: &[FaceId], tag: Tag) -> KResult<Self::Shape> {
        self.0.shell(&s.0, t, open, tag).map(|s| (s, None))
    }

    fn offset(&mut self, s: &Self::Shape, d: f64, tag: Tag) -> KResult<Self::Shape> {
        self.0.offset(&s.0, d, tag).map(|s| (s, None))
    }

    fn edges(&mut self, s: &Self::Shape, samples: u32) -> KResult<Vec<EdgeInfo>> {
        self.0.edges(&s.0, samples)
    }

    fn faces(&mut self, s: &Self::Shape, samples: u32) -> KResult<Vec<FaceInfo>> {
        self.0.faces(&s.0, samples)
    }

    fn classify(&mut self, s: &Self::Shape, pts: &[DVec3]) -> KResult<Vec<Class>> {
        self.0.classify(&s.0, pts)
    }

    fn tessellate(&mut self, s: &Self::Shape, tol: f64) -> KResult<Mesh> {
        self.0.tessellate(&s.0, tol)
    }

    fn mass_props(&mut self, s: &Self::Shape) -> KResult<MassProps> {
        self.0.mass_props(&s.0)
    }

    fn bounds(&mut self, s: &Self::Shape) -> KResult<Option<(DVec3, DVec3)>> {
        self.0.bounds(&s.0)
    }

    fn is_empty(&mut self, s: &Self::Shape) -> bool {
        self.0.is_empty(&s.0)
    }

    fn is_solid(&mut self, s: &Self::Shape) -> bool {
        self.0.is_solid(&s.0)
    }

    fn brep(&mut self, s: &Self::Shape) -> KResult<brep::Brep> {
        self.0.brep(&s.0)
    }

    fn export(&mut self, shapes: &[Self::Shape], fmt: ExportFormat) -> KResult<Vec<u8>> {
        self.0.export(&strip(shapes), fmt)
    }

    fn shape_bytes(&mut self, s: &Self::Shape) -> usize {
        self.0.shape_bytes(&s.0)
    }

    fn set_draft(&mut self, draft: bool) {
        self.0.set_draft(draft)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extrusion_with_twist_and_scale() {
        let sq = PlacedLoop {
            lp: Loop::Polygon(vec![DVec2::ZERO, DVec2::new(2.0, 0.0), DVec2::new(2.0, 2.0), DVec2::new(0.0, 2.0)]),
            frame: DMat4::IDENTITY,
        };
        // twist alone: A h; scale to 0: the pyramid A h / 3; scale (2, 0.5): ∫ (1 + t)(1 − t/2)
        assert!((extrude_volume(&sq, -3.0, DVec2::ONE).unwrap() - 12.0).abs() < 1e-12);
        assert!((extrude_volume(&sq, 3.0, DVec2::ZERO).unwrap() - 4.0).abs() < 1e-12);
        let v = extrude_volume(&sq, 3.0, DVec2::new(2.0, 0.5)).unwrap();
        assert!((v - 12.0 * (1.0 + 0.25 - 1.0 / 6.0)).abs() < 1e-12, "{v}");
    }

    #[test]
    fn pappus_polygon_and_circle() {
        // rectangle x 2…4, z 0…3 revolved fully: π (4² − 2²) 3
        let rect = PlacedLoop {
            lp: Loop::Polygon(vec![
                DVec2::new(2.0, 0.0),
                DVec2::new(4.0, 0.0),
                DVec2::new(4.0, 3.0),
                DVec2::new(2.0, 3.0),
            ]),
            frame: DMat4::from_rotation_x(std::f64::consts::FRAC_PI_2),
        };
        let v = revolve_volume(&rect, 2.0 * PI).unwrap();
        assert!((v - PI * 12.0 * 3.0).abs() < 1e-9, "{v}");
        // circle r 1 at x 5: the torus 2π² R r²
        let c = PlacedLoop {
            lp: Loop::Circle { center: DVec2::new(5.0, 0.0), r: 1.0 },
            frame: DMat4::from_rotation_x(std::f64::consts::FRAC_PI_2),
        };
        let v = revolve_volume(&c, 2.0 * PI).unwrap();
        assert!((v - 2.0 * PI * PI * 5.0).abs() < 1e-9, "{v}");
        // not in the xz-plane: unknown
        let off = PlacedLoop { lp: c.lp.clone(), frame: DMat4::IDENTITY };
        assert!(revolve_volume(&off, PI).is_none());
    }
}
