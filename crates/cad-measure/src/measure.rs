//! Measurements between picked elements: minimum distance with foot points (parallel planes:
//! normal distance), angle, radius/diameter, length, area.
//!
//! Distances: a coarse search over the tessellation (triangles, edge polylines) finds the
//! neighbourhood of the minimum; alternating projections onto the exact geometry (closed form
//! for lines, arcs, planes, cylinders, cones, spheres and tori; numeric otherwise) refine it.
//! The result is exact up to rounding when both elements are analytic, otherwise within the
//! chordal tolerance of the tessellation.

use std::f64::consts::PI;

use cad_kernel::brep::Surface;
use glam::DVec3;

use crate::body::Body;
use crate::curve::EdgeCurve;
use crate::dist::{Tri, min_distance};
use crate::num::angle_between;
use crate::surface;

/// A picked element.
#[derive(Debug, Clone, Copy)]
pub enum Geo<'a> {
    Point(DVec3),
    Curve(&'a EdgeCurve),
    Face { body: &'a Body, face: usize },
}

impl Geo<'_> {
    /// Triangles for the coarse search (points and segments degenerate).
    fn prims(&self) -> Vec<Tri> {
        match self {
            Geo::Point(p) => vec![[*p, *p, *p]],
            Geo::Curve(c) => c.polyline(96).windows(2).map(|w| [w[0], w[1], w[1]]).collect(),
            Geo::Face { body, face } => body.faces[*face].tris.clone(),
        }
    }

    /// Closest point to `p` and whether it is exact.
    fn project(&self, p: DVec3) -> (DVec3, bool) {
        match self {
            Geo::Point(q) => (*q, true),
            Geo::Curve(c) => (c.closest(p).1, c.is_analytic()),
            Geo::Face { body, face } => body.face_closest(*face, p),
        }
    }

    fn analytic(&self) -> bool {
        match self {
            Geo::Point(_) => true,
            Geo::Curve(c) => c.is_analytic(),
            Geo::Face { body, face } => surface::is_analytic(&body.faces[*face].surface),
        }
    }

    fn plane(&self) -> Option<(DVec3, DVec3)> {
        match self {
            Geo::Face { body, face } => match &body.faces[*face].surface {
                Surface::Plane { frame } => {
                    Some((frame.origin, if body.faces[*face].same_sense { frame.z } else { -frame.z }))
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn axis(&self) -> Option<(DVec3, DVec3)> {
        match self {
            Geo::Face { body, face } => match &body.faces[*face].surface {
                s @ (Surface::Cylinder { .. } | Surface::Cone { .. }) => surface::axis(s),
                _ => None,
            },
            Geo::Curve(c) => c.circle().map(|(o, z, _, _)| (o, z)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DistanceResult {
    /// The distance reported: the minimum distance, or for parallel planes their normal
    /// distance.
    pub distance: f64,
    /// Foot points on the first and second element.
    pub a: DVec3,
    pub b: DVec3,
    /// Two parallel planes: `distance` is the normal distance.
    pub parallel: bool,
    /// Minimum distance of the bounded elements (equals `distance` unless `parallel`) and
    /// its foot points.
    pub min_distance: f64,
    pub min_a: DVec3,
    pub min_b: DVec3,
    /// Closed-form geometry on both sides and the refinement converged.
    pub exact: bool,
    /// Accuracy when not exact (mm).
    pub tol: f64,
    /// Distance of the axes of two cylinders/cones/circles with parallel axes.
    pub axis_distance: Option<f64>,
}

/// Minimum distance between two elements. `tol`: chordal tolerance of the triangles.
pub fn distance(a: &Geo, b: &Geo, tol: f64) -> Option<DistanceResult> {
    let coarse = min_distance(&a.prims(), &b.prims())?;
    let (mut pa, mut pb) = (coarse.a, coarse.b);
    let mut exact = a.analytic() && b.analytic();
    let scale = 1.0 + pa.length().max(pb.length());
    let mut converged = false;
    if coarse.dist > 0.0 {
        for _ in 0..200 {
            let (na, ea) = a.project(pb);
            let (nb, eb) = b.project(na);
            let moved = na.distance(pa) + nb.distance(pb);
            pa = na;
            pb = nb;
            if !(ea && eb) {
                exact = false;
            }
            if moved <= 1e-13 * scale {
                converged = true;
                break;
            }
        }
    } else {
        // touching or intersecting: the coarse contact point, on both elements
        let (na, _) = a.project(coarse.a);
        let (nb, _) = b.project(na);
        if na.distance(nb) <= 2.0 * tol {
            pa = na;
            pb = nb;
        }
        converged = true;
    }
    let mut d = pa.distance(pb);
    if !converged || d > coarse.dist + 4.0 * tol + 1e-9 * scale {
        // refinement left the neighbourhood: keep the coarse answer
        pa = coarse.a;
        pb = coarse.b;
        d = coarse.dist;
        exact = false;
    }
    if coarse.dist == 0.0 && d <= 2.0 * tol {
        d = 0.0;
    }
    let mut out = DistanceResult {
        distance: d,
        a: pa,
        b: pb,
        parallel: false,
        min_distance: d,
        min_a: pa,
        min_b: pb,
        exact,
        tol: if exact { 0.0 } else { 2.0 * tol },
        axis_distance: None,
    };
    if let (Some((oa, na)), Some((ob, nb))) = (a.plane(), b.plane())
        && na.cross(nb).length() < 1e-9
    {
        let n = na;
        out.parallel = true;
        out.distance = (ob - oa).dot(n).abs();
        out.b = pa - n * (pa - ob).dot(n);
        out.exact = true;
        out.tol = 0.0;
    }
    if let (Some((oa, da)), Some((ob, db))) = (a.axis(), b.axis())
        && da.cross(db).length() < 1e-9
    {
        let w = ob - oa;
        out.axis_distance = Some((w - da * w.dot(da)).length());
    }
    Some(out)
}

// ------------------------------------------------------------------------------------------
// angles
// ------------------------------------------------------------------------------------------

/// Direction of an element for angles.
#[derive(Debug, Clone, Copy)]
enum Dir {
    /// Line through `p` with direction `d`.
    Line { p: DVec3, d: DVec3 },
    /// Plane through `p` with normal `n`.
    Plane { p: DVec3, n: DVec3 },
}

fn dir_of(g: &Geo, at: DVec3) -> Result<(Dir, &'static str), String> {
    match g {
        Geo::Point(_) => Err("Ein Punkt hat keine Richtung: Kante oder Fläche wählen".into()),
        Geo::Curve(c) => {
            if let Some(d) = c.line_dir() {
                return Ok((Dir::Line { p: c.start(), d }, "Gerade"));
            }
            if let Some((o, z, _, _)) = c.circle() {
                return Ok((Dir::Plane { p: o, n: z }, "Kreisebene"));
            }
            if let Some(n) = c.plane_normal() {
                return Ok((Dir::Plane { p: c.start(), n }, "Kurvenebene"));
            }
            let (s, q) = c.closest(at);
            Ok((Dir::Line { p: q, d: c.tangent(s) }, "Tangente"))
        }
        Geo::Face { body, face } => {
            let f = &body.faces[*face];
            match &f.surface {
                Surface::Plane { frame } => {
                    Ok((Dir::Plane { p: frame.origin, n: if f.same_sense { frame.z } else { -frame.z } }, "Ebene"))
                }
                s @ (Surface::Cylinder { .. }
                | Surface::Cone { .. }
                | Surface::Torus { .. }
                | Surface::Revolution { .. }) => {
                    let (o, z) = surface::axis(s).expect("axis");
                    Ok((Dir::Line { p: o, d: z }, "Achse"))
                }
                Surface::Sphere { .. } => Err("Eine Kugel hat keine Richtung".into()),
                _ => {
                    let (q, _) = body.face_closest(*face, at);
                    let n = body.face_normal(*face, q).ok_or("keine Normale an dieser Stelle")?;
                    Ok((Dir::Plane { p: q, n }, "Tangentialebene"))
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AngleResult {
    /// Degrees: between two lines 0..90 (0..180 for two edges with a common vertex,
    /// measured away from it), between planes 0..90, between a line and a plane 0..90.
    pub deg: f64,
    /// `lines`, `vertex` (edges with a common vertex), `planes`, `line_plane`.
    pub kind: &'static str,
    /// Angle of the normals of two planes (0..180).
    pub normals: Option<f64>,
    /// Opening angle at the intersection line between the half planes towards both picks
    /// (two planes; for adjacent faces of a solid the angle through the material or outside).
    pub opening: Option<f64>,
    pub parallel: bool,
    /// Where the angle is drawn and the two directions of its arc.
    pub vertex: DVec3,
    pub da: DVec3,
    pub db: DVec3,
    /// What each side was taken as (`Gerade`, `Ebene`, `Achse`, …).
    pub what: [&'static str; 2],
}

fn closest_lines(p1: DVec3, d1: DVec3, p2: DVec3, d2: DVec3) -> (DVec3, DVec3) {
    let r = p1 - p2;
    let (a, b, c, d, e) = (d1.dot(d1), d1.dot(d2), d2.dot(d2), d1.dot(r), d2.dot(r));
    let den = a * c - b * b;
    if den.abs() < 1e-18 {
        return (p1, p2 + d2 * (e / c));
    }
    let s = (b * e - c * d) / den;
    let t = (a * e - b * d) / den;
    (p1 + d1 * s, p2 + d2 * t)
}

/// Common vertex of two curves (end points within `eps`).
fn common_vertex(a: &EdgeCurve, b: &EdgeCurve, eps: f64) -> Option<DVec3> {
    for p in [a.start(), a.end()] {
        for q in [b.start(), b.end()] {
            if p.distance(q) <= eps {
                return Some(p);
            }
        }
    }
    None
}

/// Away from `v` along the line (towards the far end of the curve).
fn away(c: &EdgeCurve, v: DVec3, d: DVec3) -> DVec3 {
    let far = if c.start().distance(v) > c.end().distance(v) { c.start() } else { c.end() };
    if (far - v).dot(d) < 0.0 { -d } else { d }
}

/// Angle between two elements; `at_a`, `at_b`: where they were picked.
pub fn angle(a: &Geo, at_a: DVec3, b: &Geo, at_b: DVec3) -> Result<AngleResult, String> {
    let (da, wa) = dir_of(a, at_a)?;
    let (db, wb) = dir_of(b, at_b)?;
    let deg = |r: f64| r * 180.0 / PI;
    let mut res = AngleResult {
        deg: 0.0,
        kind: "lines",
        normals: None,
        opening: None,
        parallel: false,
        vertex: 0.5 * (at_a + at_b),
        da: DVec3::X,
        db: DVec3::X,
        what: [wa, wb],
    };
    match (da, db) {
        (Dir::Line { p: p1, d: d1 }, Dir::Line { p: p2, d: d2 }) => {
            if let (Geo::Curve(ca), Geo::Curve(cb)) = (a, b)
                && ca.line_dir().is_some()
                && cb.line_dir().is_some()
                && let Some(v) = common_vertex(ca, cb, 1e-9 * (1.0 + at_a.length()))
            {
                let (u, w) = (away(ca, v, d1), away(cb, v, d2));
                res.kind = "vertex";
                res.deg = deg(angle_between(u, w));
                res.vertex = v;
                res.da = u;
                res.db = w;
                res.parallel = res.deg < 1e-9 || res.deg > 180.0 - 1e-9;
                return Ok(res);
            }
            let mut t = angle_between(d1, d2);
            let mut w = d2;
            if t > PI / 2.0 {
                t = PI - t;
                w = -d2;
            }
            res.deg = deg(t);
            res.parallel = t < 1e-9;
            let (q1, q2) = closest_lines(p1, d1, p2, d2);
            res.vertex = if res.parallel { 0.5 * (at_a + at_b) } else { 0.5 * (q1 + q2) };
            // arc directions towards the picks
            let s = if (at_a - res.vertex).dot(d1) < 0.0 { -1.0 } else { 1.0 };
            res.da = d1 * s;
            res.db = w * s;
        }
        (Dir::Plane { p: p1, n: n1 }, Dir::Plane { p: p2, n: n2 }) => {
            res.kind = "planes";
            let tn = angle_between(n1, n2);
            res.normals = Some(deg(tn));
            res.deg = deg(tn.min(PI - tn));
            let l = n1.cross(n2);
            if l.length() < 1e-9 {
                res.parallel = true;
                res.deg = 0.0;
                res.vertex = 0.5 * (at_a + at_b);
                res.da = n1;
                res.db = n1;
                return Ok(res);
            }
            let l = l.normalize();
            // a point on the intersection line, closest to the picks' middle
            let m = 0.5 * (at_a + at_b);
            let (h1, h2) = (n1.dot(p1), n2.dot(p2));
            let (a11, a12, a22) = (n1.dot(n1), n1.dot(n2), n2.dot(n2));
            let det = a11 * a22 - a12 * a12;
            let c1 = (h1 * a22 - h2 * a12) / det;
            let c2 = (h2 * a11 - h1 * a12) / det;
            let base = n1 * c1 + n2 * c2;
            let v = base + l * (m - base).dot(l);
            let toward = |at: DVec3, n: DVec3| {
                let w = at - v;
                let w = w - l * w.dot(l);
                let w = w - n * w.dot(n);
                w.try_normalize().unwrap_or_else(|| n.cross(l).normalize())
            };
            let (u, w) = (toward(at_a, n1), toward(at_b, n2));
            let open = angle_between(u, w);
            res.opening = Some(deg(open));
            res.vertex = v;
            res.da = u;
            res.db = if open > PI / 2.0 + 1e-12 { -w } else { w };
        }
        (Dir::Line { p, d }, Dir::Plane { p: q, n }) | (Dir::Plane { p: q, n }, Dir::Line { p, d }) => {
            res.kind = "line_plane";
            let t = (PI / 2.0 - angle_between(d, n)).abs();
            res.deg = deg(t);
            let dn = d.dot(n);
            res.parallel = t < 1e-9;
            let line_at = if matches!(da, Dir::Line { .. }) { at_a } else { at_b };
            res.vertex = if dn.abs() > 1e-12 { p + d * ((q - p).dot(n) / dn) } else { line_at };
            let dd = if (line_at - res.vertex).dot(d) < 0.0 { -d } else { d };
            let proj = (dd - n * dd.dot(n)).try_normalize().unwrap_or_else(|| n.any_orthonormal_vector());
            res.da = dd;
            res.db = proj;
        }
    }
    Ok(res)
}

/// Points of an arc of radius `r` around `v` from direction `a` to `b` (both unit).
pub fn arc_points(v: DVec3, a: DVec3, b: DVec3, r: f64, n: usize) -> Vec<DVec3> {
    let t = angle_between(a, b);
    let axis = a.cross(b);
    let Some(axis) = axis.try_normalize() else { return vec![v + a * r, v + b * r] };
    let y = axis.cross(a);
    (0..=n)
        .map(|i| {
            let s = t * i as f64 / n as f64;
            v + (a * s.cos() + y * s.sin()) * r
        })
        .collect()
}

// ------------------------------------------------------------------------------------------
// radius
// ------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Radius {
    /// Circular edge (arc or full circle).
    Circle {
        centre: DVec3,
        axis: DVec3,
        r: f64,
        sweep_deg: f64,
        at: DVec3,
    },
    Ellipse {
        centre: DVec3,
        axis: DVec3,
        r1: f64,
        r2: f64,
        at: DVec3,
    },
    /// Cylinder face: axis through `axis_a`..`axis_b` (extent of the face along the axis).
    Cylinder {
        axis_a: DVec3,
        axis_b: DVec3,
        dir: DVec3,
        r: f64,
        at: DVec3,
        foot: DVec3,
    },
    /// Cone face: radius at the pick, half angle, apex.
    Cone {
        axis_a: DVec3,
        axis_b: DVec3,
        dir: DVec3,
        r: f64,
        semi_deg: f64,
        apex: DVec3,
        at: DVec3,
        foot: DVec3,
    },
    Sphere {
        centre: DVec3,
        r: f64,
        at: DVec3,
    },
    /// Torus (or a surface of revolution of a circular arc, e.g. a fillet along a circle):
    /// major radius (axis to tube centre), minor (tube) radius, the tube centre at the pick.
    Torus {
        centre: DVec3,
        axis: DVec3,
        major: f64,
        minor: f64,
        tube: DVec3,
        at: DVec3,
    },
    /// Free-form face: principal radii of curvature at the pick (smallest first), centre of
    /// the smaller one.
    Curvature {
        r_min: f64,
        r_max: f64,
        centre: DVec3,
        at: DVec3,
    },
}

impl Radius {
    /// The radius shown first: circles, cylinders, cones, spheres their radius, ellipses the
    /// larger, tori the tube radius, free-form faces the smaller radius of curvature.
    pub fn r(&self) -> f64 {
        match self {
            Radius::Circle { r, .. }
            | Radius::Cylinder { r, .. }
            | Radius::Cone { r, .. }
            | Radius::Sphere { r, .. } => *r,
            Radius::Ellipse { r1, r2, .. } => r1.max(*r2),
            Radius::Torus { minor, .. } => *minor,
            Radius::Curvature { r_min, .. } => *r_min,
        }
    }

    /// Where the radius was picked (on the element).
    pub fn at(&self) -> DVec3 {
        match self {
            Radius::Circle { at, .. }
            | Radius::Ellipse { at, .. }
            | Radius::Cylinder { at, .. }
            | Radius::Cone { at, .. }
            | Radius::Sphere { at, .. }
            | Radius::Torus { at, .. }
            | Radius::Curvature { at, .. } => *at,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RadiusResult {
    pub radius: Radius,
    /// Taken from the B-rep in closed form (analytic curve or surface, or a generator that is
    /// a circle within 1e-9); otherwise fitted (`dev`) or estimated from the curvature.
    pub exact: bool,
    /// Largest deviation of the B-rep geometry from the fitted circle (mm; 0 when analytic).
    pub dev: f64,
    /// What the radius belongs to (`Kreis`, `Zylinder`, `Rotationsfläche (Kreisbogen)`, …).
    pub source: &'static str,
}

/// Extent of points along an axis: (min, max) parameter.
fn axis_extent(pts: impl Iterator<Item = DVec3>, o: DVec3, d: DVec3) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for p in pts {
        let t = (p - o).dot(d);
        lo = lo.min(t);
        hi = hi.max(t);
    }
    if lo.is_finite() { (lo, hi) } else { (0.0, 0.0) }
}

/// Circle through three points: centre, unit normal, radius.
fn circle3(a: DVec3, b: DVec3, c: DVec3) -> Option<(DVec3, DVec3, f64)> {
    let (u, v) = (b - a, c - a);
    let w = u.cross(v);
    let w2 = w.length_squared();
    if w2 <= 1e-24 * u.length_squared() * v.length_squared() || w2 == 0.0 {
        return None;
    }
    let centre = a + (w.cross(u) * v.length_squared() + v.cross(w) * u.length_squared()) / (2.0 * w2);
    Some((centre, w.normalize(), (a - centre).length()))
}

/// A circle the points follow (centre, unit normal, radius, largest deviation) if they
/// deviate by at most `rel` × max(radius, 1).
pub fn fit_circle(pts: &[DVec3], rel: f64) -> Option<(DVec3, DVec3, f64, f64)> {
    if pts.len() < 3 {
        return None;
    }
    let n = pts.len();
    let (c, nrm, r) = circle3(pts[0], pts[n / 3], pts[2 * n / 3])?;
    let dev = pts.iter().map(|p| ((*p - c).length() - r).abs().max((*p - c).dot(nrm).abs())).fold(0.0, f64::max);
    (dev <= rel * r.max(1.0)).then_some((c, nrm, r, dev))
}

/// Principal radii of curvature of a B-spline surface at `(u, v)` (finite differences):
/// (smaller, larger, unit normal towards the centre of the smaller).
fn principal_radii(s: &cad_kernel::brep::BSplineSurface, u: f64, v: f64) -> Option<(f64, f64, DVec3)> {
    let ((u0, u1), (v0, v1)) = s.domain();
    let (hu, hv) = (1e-4 * (u1 - u0), 1e-4 * (v1 - v0));
    let u = u.clamp(u0 + hu, u1 - hu);
    let v = v.clamp(v0 + hv, v1 - hv);
    let e = |a: f64, b: f64| s.eval(a, b);
    let p = e(u, v);
    let su = (e(u + hu, v) - e(u - hu, v)) / (2.0 * hu);
    let sv = (e(u, v + hv) - e(u, v - hv)) / (2.0 * hv);
    let suu = (e(u + hu, v) - p * 2.0 + e(u - hu, v)) / (hu * hu);
    let svv = (e(u, v + hv) - p * 2.0 + e(u, v - hv)) / (hv * hv);
    let suv = (e(u + hu, v + hv) - e(u + hu, v - hv) - e(u - hu, v + hv) + e(u - hu, v - hv)) / (4.0 * hu * hv);
    let n = su.cross(sv).try_normalize()?;
    let (ee, ff, gg) = (su.dot(su), su.dot(sv), sv.dot(sv));
    let (ll, mm, nn) = (suu.dot(n), suv.dot(n), svv.dot(n));
    let den = ee * gg - ff * ff;
    if den.abs() < 1e-30 {
        return None;
    }
    let k_mean = (ee * nn - 2.0 * ff * mm + gg * ll) / (2.0 * den);
    let k_gauss = (ll * nn - mm * mm) / den;
    let disc = (k_mean * k_mean - k_gauss).max(0.0).sqrt();
    let (k1, k2) = (k_mean + disc, k_mean - disc);
    let (a, b) = (k1.abs().max(k2.abs()), k1.abs().min(k2.abs()));
    let r_min = if a > 1e-12 { 1.0 / a } else { f64::INFINITY };
    let r_max = if b > 1e-12 { 1.0 / b } else { f64::INFINITY };
    let kk = if k1.abs() >= k2.abs() { k1 } else { k2 };
    Some((r_min, r_max, if kk >= 0.0 { n } else { -n }))
}

/// Radius of a circular edge or a round face, from the B-rep: closed form for circles,
/// ellipses, cylinders, cones, spheres and tori; surfaces of revolution and extrusion whose
/// generator is a circle are recognised (fillets along circles); free-form faces give their
/// principal radii of curvature. Other elements: an error naming their kind.
pub fn radius(g: &Geo, at: DVec3) -> Result<RadiusResult, String> {
    let exact = |radius: Radius, source: &'static str| Ok(RadiusResult { radius, exact: true, dev: 0.0, source });
    match g {
        Geo::Point(_) => Err("Radius: Kreiskante oder runde Fläche wählen".into()),
        Geo::Curve(c) => match c {
            EdgeCurve::Arc { frame, r, sweep, .. } => {
                let (_, q) = c.closest(at);
                let src = if c.kind() == "circle" { "Kreis" } else { "Kreisbogen" };
                exact(
                    Radius::Circle { centre: frame.origin, axis: frame.z, r: *r, sweep_deg: sweep * 180.0 / PI, at: q },
                    src,
                )
            }
            EdgeCurve::Ellipse { frame, r1, r2, .. } => {
                let (_, q) = c.closest(at);
                exact(Radius::Ellipse { centre: frame.origin, axis: frame.z, r1: *r1, r2: *r2, at: q }, "Ellipse")
            }
            EdgeCurve::Line { .. } => Err("Radius: die Kante ist eine Gerade – keine Kreiskante".into()),
            _ => {
                let pts = c.sample(64);
                let (centre, axis, r, dev) = fit_circle(&pts, 1e-6)
                    .ok_or_else(|| format!("Radius: die Kante ist eine {} – keine Kreiskante", curve_name(c)))?;
                let (_, q) = c.closest(at);
                let sweep: f64 = pts.windows(2).map(|w| angle_between(w[0] - centre, w[1] - centre)).sum();
                Ok(RadiusResult {
                    radius: Radius::Circle { centre, axis, r, sweep_deg: sweep * 180.0 / PI, at: q },
                    exact: false,
                    dev,
                    source: "Kurve (Kreis erkannt)",
                })
            }
        },
        Geo::Face { body, face } => {
            let f = &body.faces[*face];
            let (q, _) = body.face_closest(*face, at);
            let pts =
                || f.tris.iter().flatten().copied().chain(f.edges.iter().flat_map(|&e| body.polys[e as usize].clone()));
            let cylinder = |o: DVec3, z: DVec3, r: f64| {
                let (lo, hi) = axis_extent(pts(), o, z);
                let foot = o + z * (q - o).dot(z);
                Radius::Cylinder { axis_a: o + z * lo, axis_b: o + z * hi, dir: z, r, at: q, foot }
            };
            match &f.surface {
                Surface::Cylinder { frame, r } => exact(cylinder(frame.origin, frame.z, *r), "Zylinder"),
                Surface::Cone { frame, r, semi_angle } => {
                    let (lo, hi) = axis_extent(pts(), frame.origin, frame.z);
                    let h = (q - frame.origin).dot(frame.z);
                    let foot = frame.origin + frame.z * h;
                    exact(
                        Radius::Cone {
                            axis_a: frame.origin + frame.z * lo,
                            axis_b: frame.origin + frame.z * hi,
                            dir: frame.z,
                            r: r + h * semi_angle.tan(),
                            semi_deg: semi_angle * 180.0 / PI,
                            apex: frame.origin - frame.z * (r / semi_angle.tan()),
                            at: q,
                            foot,
                        },
                        "Kegel",
                    )
                }
                Surface::Sphere { frame, r } => exact(Radius::Sphere { centre: frame.origin, r: *r, at: q }, "Kugel"),
                Surface::Torus { frame, major, minor } => {
                    let d = q - frame.origin;
                    let e = (d - frame.z * d.dot(frame.z)).try_normalize().unwrap_or(frame.x);
                    exact(
                        Radius::Torus {
                            centre: frame.origin,
                            axis: frame.z,
                            major: *major,
                            minor: *minor,
                            tube: frame.origin + e * *major,
                            at: q,
                        },
                        "Torus",
                    )
                }
                Surface::Revolution { curve, axis_origin, axis_dir } => {
                    let z = *axis_dir;
                    let (t0, t1) = curve.domain();
                    let gpts: Vec<DVec3> = (0..=64).map(|i| curve.eval(t0 + (t1 - t0) * i as f64 / 64.0)).collect();
                    let radial = |p: DVec3| {
                        let d = p - *axis_origin;
                        let h = d.dot(z);
                        (d - z * h, h)
                    };
                    if let Some((c, _, r, dev)) = fit_circle(&gpts, 1e-6) {
                        let (rv, h) = radial(c);
                        let (rq, _) = radial(q);
                        let e = rq
                            .try_normalize()
                            .or_else(|| rv.try_normalize())
                            .unwrap_or_else(|| z.any_orthonormal_vector());
                        let centre = *axis_origin + z * h;
                        let major = rv.length();
                        return Ok(RadiusResult {
                            radius: Radius::Torus { centre, axis: z, major, minor: r, tube: centre + e * major, at: q },
                            exact: dev <= 1e-9 * r.max(1.0),
                            dev,
                            source: "Rotationsfläche (Kreisbogen)",
                        });
                    }
                    // straight generator parallel to the axis: a cylinder
                    let rs: Vec<f64> = gpts.iter().map(|p| radial(*p).0.length()).collect();
                    let (rmin, rmax) = rs.iter().fold((f64::INFINITY, 0.0f64), |(a, b), &x| (a.min(x), b.max(x)));
                    if rmax - rmin <= 1e-9 * rmax.max(1.0) {
                        return exact(cylinder(*axis_origin, z, 0.5 * (rmin + rmax)), "Rotationsfläche (Zylinder)");
                    }
                    Err("Radius: Rotationsfläche ohne Kreisbogen als Erzeugende".into())
                }
                Surface::Extrusion { curve, dir } => {
                    let d = dir.try_normalize().ok_or("Radius: Extrusion ohne Richtung")?;
                    let (t0, t1) = curve.domain();
                    let gpts: Vec<DVec3> = (0..=64).map(|i| curve.eval(t0 + (t1 - t0) * i as f64 / 64.0)).collect();
                    let (c, nrm, r, dev) = fit_circle(&gpts, 1e-6).ok_or("Radius: Extrusionsfläche ohne Kreisbogen")?;
                    if nrm.cross(d).length() > 1e-6 {
                        return Err("Radius: schräg extrudierter Kreisbogen".into());
                    }
                    Ok(RadiusResult {
                        radius: cylinder(c, d, r),
                        exact: dev <= 1e-9 * r.max(1.0),
                        dev,
                        source: "Extrusionsfläche (Kreisbogen)",
                    })
                }
                Surface::BSpline(bs) => {
                    let (u, v, p) = surface::bspline_project(bs, q);
                    let (r_min, r_max, n) = principal_radii(bs, u, v).ok_or("Radius: Krümmung nicht bestimmbar")?;
                    if !r_min.is_finite() {
                        return Err("Radius: die Fläche ist an dieser Stelle eben".into());
                    }
                    Ok(RadiusResult {
                        radius: Radius::Curvature { r_min, r_max, centre: p + n * r_min, at: p },
                        exact: false,
                        dev: 0.0,
                        source: "Freiformfläche (Krümmung)",
                    })
                }
                s => Err(format!("Radius: die Fläche ist eine {} – kein Radius", surface::kind_name(s))),
            }
        }
    }
}

/// German name of a curve kind.
pub fn curve_name(c: &EdgeCurve) -> &'static str {
    match c.kind() {
        "line" => "Gerade",
        "arc" => "Kreisbogen",
        "circle" => "Kreis",
        "ellipse" => "Ellipse",
        "spline" => "Freiformkurve",
        _ => "Polylinie",
    }
}

// ------------------------------------------------------------------------------------------
// area
// ------------------------------------------------------------------------------------------

/// Area of face `f`: `(area, exact)`. Planar faces exactly from their boundary (Green's
/// theorem over the exact edge curves); cylinder, cone, sphere and torus faces from the
/// triangles subdivided and projected onto the surface (boundary points onto the boundary
/// curves), extrapolated (Richardson) – relative error far below 1e-6; other surfaces from
/// the triangles (accuracy of the tessellation).
pub fn face_area(body: &Body, f: usize) -> (f64, bool) {
    let fg = &body.faces[f];
    match &fg.surface {
        Surface::Plane { frame } => {
            let (x, y) = (frame.x, frame.y());
            let mut a = 0.0;
            for lp in &fg.loops {
                for &(e, fwd) in lp {
                    let c = &body.edges[e as usize];
                    let c = if fwd { c.clone() } else { c.reversed() };
                    a += green(&c, frame.origin, x, y);
                }
            }
            (a.abs(), true)
        }
        s if surface::is_analytic(s) => {
            let a4 = refined_area(body, f, 4);
            let a8 = refined_area(body, f, 8);
            ((4.0 * a8 - a4) / 3.0, true)
        }
        _ => (fg.tris.iter().map(tri_area).sum(), false),
    }
}

fn tri_area(t: &Tri) -> f64 {
    0.5 * (t[1] - t[0]).cross(t[2] - t[0]).length()
}

/// ½ ∮ (x dy − y dx) of a curve in the plane (origin `o`, axes `x`, `y`).
pub fn green(c: &EdgeCurve, o: DVec3, x: DVec3, y: DVec3) -> f64 {
    let uv = |p: DVec3| ((p - o).dot(x), (p - o).dot(y));
    match c {
        EdgeCurve::Line { a, b } => {
            let (p, q) = (uv(*a), uv(*b));
            0.5 * (p.0 * q.1 - p.1 * q.0)
        }
        EdgeCurve::Poly { pts } => pts
            .windows(2)
            .map(|w| {
                let (p, q) = (uv(w[0]), uv(w[1]));
                0.5 * (p.0 * q.1 - p.1 * q.0)
            })
            .sum(),
        _ => {
            let pieces = match c {
                EdgeCurve::Spline { c: s, .. } => (s.knots.len().saturating_sub(2 * s.degree as usize + 1)).max(1) * 4,
                _ => 16,
            };
            crate::num::integrate(0.0, 1.0, pieces.min(512), |s| {
                let (px, py) = uv(c.point(s));
                let d = c.deriv(s);
                let (dx, dy) = (d.dot(x), d.dot(y));
                0.5 * (px * dy - py * dx)
            })
        }
    }
}

/// Area of the face's triangles subdivided `k` times per edge and projected onto the surface.
fn refined_area(body: &Body, f: usize, k: usize) -> f64 {
    use std::collections::HashMap;
    let fg = &body.faces[f];
    // boundary edges of the triangulation (used by one triangle of the face)
    let key = |p: DVec3| {
        let q = (p / (1e-7 * body.size.max(1e-9))).round();
        (q.x as i64, q.y as i64, q.z as i64)
    };
    type Key = (i64, i64, i64);
    let mut uses: HashMap<(Key, Key), u32> = HashMap::new();
    let ek = |a: DVec3, b: DVec3| {
        let (x, y) = (key(a), key(b));
        if x < y { (x, y) } else { (y, x) }
    };
    for t in &fg.tris {
        for i in 0..3 {
            *uses.entry(ek(t[i], t[(i + 1) % 3])).or_default() += 1;
        }
    }
    let on_boundary = |p: DVec3| {
        // closest boundary curve point
        let mut best = (f64::INFINITY, p);
        for &e in &fg.edges {
            let (_, q) = body.edges[e as usize].closest(p);
            let d = q.distance(p);
            if d < best.0 {
                best = (d, q);
            }
        }
        best.1
    };
    let proj = |p: DVec3| surface::project(&fg.surface, p).unwrap_or(p);
    let mut area = 0.0;
    for t in &fg.tris {
        let bnd = [
            uses.get(&ek(t[0], t[1])) == Some(&1),
            uses.get(&ek(t[1], t[2])) == Some(&1),
            uses.get(&ek(t[2], t[0])) == Some(&1),
        ];
        // grid points (i along t0→t1, j along t0→t2)
        let n = k;
        let mut pos: Vec<DVec3> = Vec::with_capacity((n + 1) * (n + 2) / 2);
        for j in 0..=n {
            for i in 0..=(n - j) {
                let p = t[0] + (t[1] - t[0]) * (i as f64 / n as f64) + (t[2] - t[0]) * (j as f64 / n as f64);
                let edge01 = j == 0;
                let edge12 = i + j == n;
                let edge20 = i == 0;
                let corner = (i == 0 && j == 0) || (i == n && j == 0) || (i == 0 && j == n);
                let q = if corner {
                    p
                } else if (edge01 && bnd[0]) || (edge12 && bnd[1]) || (edge20 && bnd[2]) {
                    on_boundary(p)
                } else {
                    proj(p)
                };
                pos.push(q);
            }
        }
        let at = |i: usize, j: usize| -> DVec3 {
            // row j starts after rows 0..j with (n + 1 − r) points each
            let start: usize = (0..j).map(|r| n + 1 - r).sum();
            pos[start + i]
        };
        for j in 0..n {
            for i in 0..(n - j) {
                area += tri_area(&[at(i, j), at(i + 1, j), at(i, j + 1)]);
                if i + j + 1 < n {
                    area += tri_area(&[at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)]);
                }
            }
        }
    }
    area
}
