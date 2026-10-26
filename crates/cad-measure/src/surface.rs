//! Exact surface geometry: closest point on the (unbounded) surface, outward normal, kind.
//!
//! Plane, cylinder, cone, sphere and torus in closed form; surfaces of revolution and
//! extrusion by a 1D search along their generator; B-spline surfaces by a grid search plus
//! Gauss–Newton steps.

use cad_i18n::trs;
use cad_kernel::brep::{BSplineSurface, Frame, Surface};
use glam::DVec3;

use crate::num::minimise;

/// Plane, cylinder, cone, sphere or torus.
pub fn is_analytic(s: &Surface) -> bool {
    matches!(
        s,
        Surface::Plane { .. }
            | Surface::Cylinder { .. }
            | Surface::Cone { .. }
            | Surface::Sphere { .. }
            | Surface::Torus { .. }
    )
}

/// Name of a surface kind (labels), in the current language.
pub fn kind_name(s: &Surface) -> &'static str {
    match s {
        Surface::Plane { .. } => trs!("Ebene", "plane"),
        Surface::Cylinder { .. } => trs!("Zylinder", "cylinder"),
        Surface::Cone { .. } => trs!("Kegel", "cone"),
        Surface::Sphere { .. } => trs!("Kugel", "sphere"),
        Surface::Torus { .. } => trs!("Torus", "torus"),
        Surface::Revolution { .. } => trs!("Rotationsfläche", "surface of revolution"),
        Surface::Extrusion { .. } => trs!("Extrusionsfläche", "extrusion surface"),
        Surface::BSpline(_) => trs!("Freiformfläche", "free-form surface"),
    }
}

/// Fixed identifier of a surface kind (JSON, the same in every language): `plane`,
/// `cylinder`, `cone`, `sphere`, `torus`, `revolution`, `extrusion`, `bspline`.
pub fn kind_id(s: &Surface) -> &'static str {
    match s {
        Surface::Plane { .. } => "plane",
        Surface::Cylinder { .. } => "cylinder",
        Surface::Cone { .. } => "cone",
        Surface::Sphere { .. } => "sphere",
        Surface::Torus { .. } => "torus",
        Surface::Revolution { .. } => "revolution",
        Surface::Extrusion { .. } => "extrusion",
        Surface::BSpline(_) => "bspline",
    }
}

/// Display name of a [`kind_id`], in the current language (unknown ids unchanged).
pub fn kind_id_name(id: &'static str) -> &'static str {
    match id {
        "plane" => trs!("Ebene", "plane"),
        "cylinder" => trs!("Zylinder", "cylinder"),
        "cone" => trs!("Kegel", "cone"),
        "sphere" => trs!("Kugel", "sphere"),
        "torus" => trs!("Torus", "torus"),
        "revolution" => trs!("Rotationsfläche", "surface of revolution"),
        "extrusion" => trs!("Extrusionsfläche", "extrusion surface"),
        "bspline" => trs!("Freiformfläche", "free-form surface"),
        other => other,
    }
}

fn radial(f: &Frame, p: DVec3) -> (DVec3, f64) {
    let d = p - f.origin;
    let h = d.dot(f.z);
    (d - f.z * h, h)
}

/// A unit vector perpendicular to the frame axis, towards `p` (any if `p` is on the axis).
fn radial_dir(f: &Frame, p: DVec3) -> DVec3 {
    radial(f, p).0.try_normalize().unwrap_or(f.x)
}

/// Closest point to `p` on the unbounded surface (`None` if it cannot be found).
pub fn project(s: &Surface, p: DVec3) -> Option<DVec3> {
    Some(match s {
        Surface::Plane { frame } => p - frame.z * (p - frame.origin).dot(frame.z),
        Surface::Cylinder { frame, r } => {
            let (_, h) = radial(frame, p);
            frame.origin + frame.z * h + radial_dir(frame, p) * *r
        }
        Surface::Cone { frame, r, semi_angle } => {
            // generatrix in the (ρ, h) half-plane: ρ = r + h tan α
            let (rv, h) = radial(frame, p);
            let rho = rv.length();
            let ta = semi_angle.tan();
            // line through (r, 0) with direction (sin α, cos α)
            let (sa, ca) = semi_angle.sin_cos();
            let t = (rho - r) * sa + h * ca;
            let (mut rr, mut hh) = (r + t * sa, t * ca);
            if rr < 0.0 {
                // beyond the apex
                rr = 0.0;
                hh = -r / ta;
            }
            frame.origin + frame.z * hh + radial_dir(frame, p) * rr
        }
        Surface::Sphere { frame, r } => frame.origin + (p - frame.origin).try_normalize().unwrap_or(frame.z) * *r,
        Surface::Torus { frame, major, minor } => {
            let c = frame.origin + radial_dir(frame, p) * *major;
            c + (p - c).try_normalize().unwrap_or(frame.z) * *minor
        }
        Surface::Revolution { curve, axis_origin, axis_dir } => {
            let z = *axis_dir;
            let rh = |q: DVec3| {
                let d = q - *axis_origin;
                let h = d.dot(z);
                ((d - z * h).length(), h)
            };
            let (rp, hp) = rh(p);
            let (a, b) = curve.domain();
            let (t, _) = minimise(a, b, 128, |t| {
                let (rc, hc) = rh(curve.eval(t));
                (rp - rc).hypot(hp - hc)
            });
            let (rc, hc) = rh(curve.eval(t));
            let d = p - *axis_origin;
            let e = (d - z * d.dot(z)).try_normalize().unwrap_or_else(|| z.any_orthonormal_vector());
            *axis_origin + z * hc + e * rc
        }
        Surface::Extrusion { curve, dir } => {
            let d = dir.try_normalize()?;
            let (a, b) = curve.domain();
            let (t, _) = minimise(a, b, 128, |t| (p - curve.eval(t)).cross(d).length());
            let c = curve.eval(t);
            c + d * (p - c).dot(d)
        }
        Surface::BSpline(b) => bspline_project(b, p).2,
    })
}

/// Closest point on a B-spline surface: `(u, v, point)`.
pub fn bspline_project(s: &BSplineSurface, p: DVec3) -> (f64, f64, DVec3) {
    let ((u0, u1), (v0, v1)) = s.domain();
    let mut best = (u0, v0, s.eval(u0, v0), f64::INFINITY);
    for (u, v, q) in s.samples(24) {
        let d = q.distance_squared(p);
        if d < best.3 {
            best = (u, v, q, d);
        }
    }
    let (mut u, mut v) = (best.0, best.1);
    let (hu, hv) = (1e-6 * (u1 - u0), 1e-6 * (v1 - v0));
    for _ in 0..40 {
        let q = s.eval(u, v);
        let su = (s.eval((u + hu).min(u1), v) - s.eval((u - hu).max(u0), v)) / ((u + hu).min(u1) - (u - hu).max(u0));
        let sv = (s.eval(u, (v + hv).min(v1)) - s.eval(u, (v - hv).max(v0))) / ((v + hv).min(v1) - (v - hv).max(v0));
        let r = p - q;
        // normal equations of the linearised problem
        let (a, b, c) = (su.dot(su), su.dot(sv), sv.dot(sv));
        let (e, f) = (su.dot(r), sv.dot(r));
        let det = a * c - b * b;
        if det.abs() < 1e-30 {
            break;
        }
        let du = (c * e - b * f) / det;
        let dv = (a * f - b * e) / det;
        let (nu, nv) = ((u + du).clamp(u0, u1), (v + dv).clamp(v0, v1));
        let moved = (nu - u).abs() + (nv - v).abs();
        u = nu;
        v = nv;
        if moved < 1e-14 * (1.0 + (u1 - u0) + (v1 - v0)) {
            break;
        }
    }
    (u, v, s.eval(u, v))
}

/// Outward unit normal of a face with surface `s` at `p` (on the surface).
pub fn normal(s: &Surface, same_sense: bool, p: DVec3) -> Option<DVec3> {
    let uv = match s {
        Surface::BSpline(b) => {
            let (u, v, _) = bspline_project(b, p);
            Some((u, v))
        }
        _ => None,
    };
    let n = s.normal_at(p, uv)?;
    Some(if same_sense { n } else { -n })
}

/// Axis of a surface of rotation: point, unit direction.
pub fn axis(s: &Surface) -> Option<(DVec3, DVec3)> {
    match s {
        Surface::Cylinder { frame, .. }
        | Surface::Cone { frame, .. }
        | Surface::Torus { frame, .. }
        | Surface::Sphere { frame, .. } => Some((frame.origin, frame.z)),
        Surface::Revolution { axis_origin, axis_dir, .. } => Some((*axis_origin, *axis_dir)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f() -> Frame {
        Frame { origin: DVec3::new(1.0, 1.0, 0.0), z: DVec3::Z, x: DVec3::X }
    }

    #[test]
    fn projections() {
        let p = DVec3::new(5.0, 1.0, 3.0);
        let c = project(&Surface::Cylinder { frame: f(), r: 2.0 }, p).unwrap();
        assert!(c.distance(DVec3::new(3.0, 1.0, 3.0)) < 1e-12);
        let s = project(&Surface::Sphere { frame: f(), r: 5.0 }, p).unwrap();
        assert!(((s - f().origin).length() - 5.0).abs() < 1e-12);
        let t = project(&Surface::Torus { frame: f(), major: 3.0, minor: 1.0 }, DVec3::new(4.0, 1.0, 2.0)).unwrap();
        assert!(t.distance(DVec3::new(4.0, 1.0, 1.0)) < 1e-12, "{t}");
        // cone r = 2 at h = 0, 45°: at h = 1 radius 3
        let cone = Surface::Cone { frame: f(), r: 2.0, semi_angle: std::f64::consts::FRAC_PI_4 };
        let q = project(&cone, DVec3::new(1.0 + 3.0, 1.0, 1.0)).unwrap();
        assert!(q.distance(DVec3::new(4.0, 1.0, 1.0)) < 1e-12, "{q}");
        assert!((cone.distance(q).unwrap()).abs() < 1e-12);
        let q = project(&cone, DVec3::new(1.0 + 5.0, 1.0, 0.0)).unwrap();
        assert!(cone.distance(q).unwrap() < 1e-12);
        assert!((q.distance(DVec3::new(6.0, 1.0, 0.0)) - 3.0 * std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-12);
    }
}
