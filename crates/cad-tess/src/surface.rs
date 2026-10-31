//! Surface evaluation and point inversion (plan 2.2) in the parameterisation of the neutral
//! B-rep (ISO 10303-42, see [`cad_kernel::brep::Surface`]):
//!
//! | Surface | σ(u, v) | periodic | poles |
//! | --- | --- | --- | --- |
//! | plane | `o + u·x + v·y` | – | – |
//! | cylinder | `o + r·(cos u·x + sin u·y) + v·z` | u | – |
//! | cone | `o + (r + v·tan α)·(cos u·x + sin u·y) + v·z` | u | apex `v = −r / tan α` |
//! | sphere | `o + r·cos v·(cos u·x + sin u·y) + r·sin v·z` | u | `v = ±π/2` |
//! | torus | `o + (R + r·cos v)·(cos u·x + sin u·y) + r·sin v·z` | u, v | – |
//! | revolution | `curve(v)` rotated by `u` about the axis | u; v if the curve is closed | curve ends on the axis |
//! | extrusion | `curve(u) + v·dir` | u if the curve is closed | – |
//! | B-spline | tensor product | u/v if the boundary rows coincide | collapsed boundary rows |
//!
//! In every case `σu × σv` points along the STEP normal of the surface. Periodic parameters may
//! leave their period (loops are unwrapped, see `uv.rs`); evaluation maps them back.

use std::f64::consts::{FRAC_PI_2, TAU};

use cad_kernel::brep::Surface;
use glam::{DVec2, DVec3};

use crate::bspline::{self, SurfDers};

#[derive(Debug, Clone)]
enum Kind {
    Plane,
    Cylinder { r: f64 },
    Cone { r: f64, tan: f64 },
    Sphere { r: f64 },
    Torus { major: f64, minor: f64 },
    Revolution { curve: bspline::Curve },
    Extrusion { curve: bspline::Curve, dir: DVec3 },
    BSpline(bspline::Surface),
}

/// A point where one parameter has no meaning: the iso-line of the other one collapses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pole {
    /// `0`: u is undefined (an iso-line `v = value` collapses), `1`: v is undefined.
    pub free: usize,
    /// The value of the other parameter.
    pub value: f64,
}

/// A surface ready for evaluation, with its frame (analytic surfaces; axis for revolutions).
#[derive(Debug, Clone)]
pub(crate) struct Geom {
    kind: Kind,
    o: DVec3,
    x: DVec3,
    y: DVec3,
    z: DVec3,
    /// Period of u and v, if periodic (or closed).
    pub period: [Option<f64>; 2],
    /// Natural domain (wrapping, clamping, seeds); infinite for unbounded parameters.
    pub domain: [(f64, f64); 2],
    poles: Vec<(DVec3, Pole)>,
    /// Distance below which a point counts as a pole.
    eps: f64,
}

fn rot(w: DVec3, z: DVec3, (s, c): (f64, f64)) -> DVec3 {
    w * c + z.cross(w) * s + z * (z.dot(w) * (1.0 - c))
}

impl Geom {
    /// `eps`: distance tolerance of the model (poles, closed rows).
    pub fn new(s: &Surface, eps: f64) -> Geom {
        let inf = (f64::NEG_INFINITY, f64::INFINITY);
        let ang = (0.0, TAU);
        let mut g = Geom {
            kind: Kind::Plane,
            o: DVec3::ZERO,
            x: DVec3::X,
            y: DVec3::Y,
            z: DVec3::Z,
            period: [None, None],
            domain: [inf, inf],
            poles: Vec::new(),
            eps,
        };
        let mut frame = |f: &cad_kernel::brep::Frame| {
            g.o = f.origin;
            g.x = f.x;
            g.y = f.y();
            g.z = f.z;
        };
        match s {
            Surface::Plane { frame: f } => frame(f),
            Surface::Cylinder { frame: f, r } => {
                frame(f);
                g.kind = Kind::Cylinder { r: *r };
                g.period[0] = Some(TAU);
                g.domain[0] = ang;
            }
            Surface::Cone { frame: f, r, semi_angle } => {
                frame(f);
                let tan = semi_angle.tan();
                g.kind = Kind::Cone { r: *r, tan };
                g.period[0] = Some(TAU);
                g.domain[0] = ang;
                let va = -r / tan;
                g.poles.push((g.o + g.z * va, Pole { free: 0, value: va }));
            }
            Surface::Sphere { frame: f, r } => {
                frame(f);
                g.kind = Kind::Sphere { r: *r };
                g.period[0] = Some(TAU);
                g.domain = [ang, (-FRAC_PI_2, FRAC_PI_2)];
                g.poles.push((g.o + g.z * *r, Pole { free: 0, value: FRAC_PI_2 }));
                g.poles.push((g.o - g.z * *r, Pole { free: 0, value: -FRAC_PI_2 }));
            }
            Surface::Torus { frame: f, major, minor } => {
                frame(f);
                g.kind = Kind::Torus { major: *major, minor: *minor };
                g.period = [Some(TAU), Some(TAU)];
                g.domain = [ang, ang];
            }
            Surface::Revolution { curve, axis_origin, axis_dir } => {
                let c = bspline::Curve::new(curve);
                g.o = *axis_origin;
                g.z = axis_dir.normalize();
                g.x = g.z.any_orthonormal_vector();
                g.y = g.z.cross(g.x);
                let (a, b) = c.domain();
                g.period = [Some(TAU), c.is_closed(eps).then_some(b - a)];
                g.domain = [ang, (a, b)];
                for t in [a, b] {
                    let p = c.eval(t);
                    let d = p - g.o;
                    if (d - g.z * d.dot(g.z)).length() <= eps {
                        g.poles.push((p, Pole { free: 0, value: t }));
                    }
                }
                g.kind = Kind::Revolution { curve: c };
            }
            Surface::Extrusion { curve, dir } => {
                let c = bspline::Curve::new(curve);
                let (a, b) = c.domain();
                g.period[0] = c.is_closed(eps).then_some(b - a);
                g.domain[0] = (a, b);
                g.kind = Kind::Extrusion { curve: c, dir: *dir };
            }
            Surface::BSpline(bs) => {
                let s = bspline::Surface::new(bs);
                let ((u0, u1), (v0, v1)) = s.domain();
                g.domain = [(u0, u1), (v0, v1)];
                let row = |i: usize| (0..s.nv).map(move |j| (i, j));
                let col = |j: usize| (0..s.nu).map(move |i| (i, j));
                let same = |a: &mut dyn Iterator<Item = (usize, usize)>,
                            b: &mut dyn Iterator<Item = (usize, usize)>| {
                    a.zip(b).all(|(p, q)| (s.point(p.0, p.1) - s.point(q.0, q.1)).length() <= eps)
                };
                if same(&mut row(0), &mut row(s.nu - 1)) {
                    g.period[0] = Some(u1 - u0);
                }
                if same(&mut col(0), &mut col(s.nv - 1)) {
                    g.period[1] = Some(v1 - v0);
                }
                // collapsed boundary rows: all control points of the row in one point
                let collapsed = |it: &mut dyn Iterator<Item = (usize, usize)>| {
                    let pts: Vec<DVec3> = it.map(|(i, j)| s.point(i, j)).collect();
                    pts.iter().all(|p| (*p - pts[0]).length() <= eps).then_some(pts[0])
                };
                if let Some(p) = collapsed(&mut row(0)) {
                    g.poles.push((p, Pole { free: 1, value: u0 }));
                }
                if let Some(p) = collapsed(&mut row(s.nu - 1)) {
                    g.poles.push((p, Pole { free: 1, value: u1 }));
                }
                if let Some(p) = collapsed(&mut col(0)) {
                    g.poles.push((p, Pole { free: 0, value: v0 }));
                }
                if let Some(p) = collapsed(&mut col(s.nv - 1)) {
                    g.poles.push((p, Pole { free: 0, value: v1 }));
                }
                g.kind = Kind::BSpline(s);
            }
        }
        g
    }

    /// Knot breaks of parameter `i` in its natural domain (curve-based and B-spline surfaces).
    pub fn natural_breaks(&self, i: usize) -> Option<Vec<f64>> {
        match (&self.kind, i) {
            (Kind::Revolution { curve }, 1) | (Kind::Extrusion { curve, .. }, 0) => Some(curve.breaks()),
            (Kind::BSpline(s), _) => Some(s.breaks(i)),
            _ => None,
        }
    }

    pub fn is_plane(&self) -> bool {
        matches!(self.kind, Kind::Plane)
    }

    /// The pole at `p`, if `p` is one.
    pub fn pole(&self, p: DVec3) -> Option<Pole> {
        self.poles.iter().find(|(q, _)| (*q - p).length() <= self.eps).map(|(_, pl)| *pl)
    }

    /// `t` mapped into the natural domain of a periodic parameter `i`.
    fn wrap(&self, i: usize, t: f64) -> f64 {
        match self.period[i] {
            Some(per) if self.domain[i].0.is_finite() => {
                let a = self.domain[i].0;
                a + (t - a).rem_euclid(per)
            }
            _ => t,
        }
    }

    /// Point and first and second derivatives at `(u, v)`.
    pub fn ders(&self, uv: DVec2) -> SurfDers {
        let (u, v) = (uv.x, uv.y);
        let (o, x, y, z) = (self.o, self.x, self.y, self.z);
        match &self.kind {
            Kind::Plane => [o + x * u + y * v, x, y, DVec3::ZERO, DVec3::ZERO, DVec3::ZERO],
            Kind::Cylinder { r } => {
                let (s, c) = u.sin_cos();
                let e = x * c + y * s;
                let de = y * c - x * s;
                [o + e * *r + z * v, de * *r, z, -e * *r, DVec3::ZERO, DVec3::ZERO]
            }
            Kind::Cone { r, tan } => {
                let (s, c) = u.sin_cos();
                let e = x * c + y * s;
                let de = y * c - x * s;
                let rr = r + v * tan;
                [o + e * rr + z * v, de * rr, e * *tan + z, -e * rr, de * *tan, DVec3::ZERO]
            }
            Kind::Sphere { r } => {
                let (s, c) = u.sin_cos();
                let (sv, cv) = v.sin_cos();
                let e = x * c + y * s;
                let de = y * c - x * s;
                [
                    o + (e * cv + z * sv) * *r,
                    de * (cv * r),
                    (z * cv - e * sv) * *r,
                    -e * (cv * r),
                    -de * (sv * r),
                    -(e * cv + z * sv) * *r,
                ]
            }
            Kind::Torus { major, minor } => {
                let (s, c) = u.sin_cos();
                let (sv, cv) = v.sin_cos();
                let e = x * c + y * s;
                let de = y * c - x * s;
                let rr = major + minor * cv;
                [
                    o + e * rr + z * (minor * sv),
                    de * rr,
                    (z * cv - e * sv) * *minor,
                    -e * rr,
                    -de * (minor * sv),
                    -(e * cv + z * sv) * *minor,
                ]
            }
            Kind::Revolution { curve } => {
                let [c0, c1, c2] = curve.eval_d(self.wrap(1, v));
                let sc = u.sin_cos();
                let w = rot(c0 - o, z, sc);
                let su = z.cross(w);
                let sv = rot(c1, z, sc);
                [o + w, su, sv, z.cross(su), z.cross(sv), rot(c2, z, sc)]
            }
            Kind::Extrusion { curve, dir } => {
                let [c0, c1, c2] = curve.eval_d(self.wrap(0, u));
                [c0 + *dir * v, c1, *dir, c2, DVec3::ZERO, DVec3::ZERO]
            }
            Kind::BSpline(s) => s.eval_d(self.wrap(0, u), self.wrap(1, v)),
        }
    }

    pub fn eval(&self, uv: DVec2) -> DVec3 {
        match &self.kind {
            Kind::Revolution { curve } => self.o + rot(curve.eval(self.wrap(1, uv.y)) - self.o, self.z, uv.x.sin_cos()),
            Kind::Extrusion { curve, dir } => curve.eval(self.wrap(0, uv.x)) + *dir * uv.y,
            _ => self.ders(uv)[0],
        }
    }

    /// Unit normal `σu × σv` at `uv`; at a pole the limit from the inside of `bounds`
    /// (`(min, max)` of the face's parameters). `None` if it cannot be found.
    pub fn normal(&self, uv: DVec2, bounds: (DVec2, DVec2)) -> Option<DVec3> {
        match &self.kind {
            Kind::Plane => return Some(self.z),
            Kind::Sphere { .. } => {
                let p = self.eval(uv);
                return (p - self.o).try_normalize();
            }
            _ => {}
        }
        let d = self.ders(uv);
        if let Some(n) = d[1]
            .cross(d[2])
            .try_normalize()
            .filter(|_| d[1].cross(d[2]).length() > 1e-12 * (d[1].length() * d[2].length()).max(1e-300))
        {
            return Some(n);
        }
        // degenerate (pole, apex): step towards the middle of the face
        let mid = (bounds.0 + bounds.1) * 0.5;
        for f in [1e-6, 1e-4, 1e-2] {
            let q = uv + (mid - uv) * f;
            let d = self.ders(q);
            if let Some(n) = d[1].cross(d[2]).try_normalize() {
                return Some(n);
            }
        }
        None
    }

    /// Principal curvature radius estimates for the grid (analytic surfaces): the radius along
    /// u and along v, `None` for a straight direction.
    pub fn radii(&self, bounds: (DVec2, DVec2)) -> Option<[Option<f64>; 2]> {
        Some(match &self.kind {
            Kind::Plane => [None, None],
            Kind::Cylinder { r } => [Some(*r), None],
            Kind::Cone { r, tan } => {
                // the largest circle of the face sags most for a given step in u
                [Some((r + bounds.0.y * tan).abs().max((r + bounds.1.y * tan).abs())), None]
            }
            Kind::Sphere { r } => [Some(*r), Some(*r)],
            Kind::Torus { major, minor } => [Some(major + minor), Some(*minor)],
            _ => return None,
        })
    }

    /// Parameters of `p` (on or near the surface). `hint`: parameters of a point nearby
    /// (continuity along a loop, choice among several solutions).
    pub fn invert(&self, p: DVec3, hint: Option<DVec2>) -> DVec2 {
        let d = p - self.o;
        let (lx, ly, lz) = (d.dot(self.x), d.dot(self.y), d.dot(self.z));
        let ang = ly.atan2(lx);
        match &self.kind {
            Kind::Plane => DVec2::new(lx, ly),
            Kind::Cylinder { .. } => DVec2::new(ang, lz),
            Kind::Cone { r, tan } => {
                // foot on the generator line through the apex in the half-plane of p
                let rho = lx.hypot(ly);
                let va = -r / tan;
                // generator direction (radial, axial) = (tan, 1) / |.|
                let n = (1.0 + tan * tan).sqrt();
                let s = ((rho) * tan + (lz - va)) / n;
                DVec2::new(ang, va + s / n)
            }
            Kind::Sphere { .. } => DVec2::new(ang, lz.atan2(lx.hypot(ly))),
            Kind::Torus { major, .. } => {
                let rho = lx.hypot(ly);
                DVec2::new(ang, lz.atan2(rho - major))
            }
            Kind::Revolution { curve } => {
                let z = self.z;
                let rh = |q: DVec3| {
                    let d = q - self.o;
                    let h = d.dot(z);
                    (d - z * h, h)
                };
                let (rp, hp) = rh(p);
                let v = match hint {
                    Some(h) => self.wrap(1, h.y),
                    None => seed_curve(curve, |q| {
                        let (rc, hc) = rh(q);
                        (rc.length() - rp.length()).hypot(hc - hp)
                    }),
                };
                let (rc, _) = rh(curve.eval(v));
                let u = match (rc.try_normalize(), rp.try_normalize()) {
                    (Some(ec), Some(ep)) => ec.cross(ep).dot(z).atan2(ec.dot(ep)),
                    _ => hint.map_or(0.0, |h| h.x),
                };
                self.newton(p, DVec2::new(u, v))
            }
            Kind::Extrusion { curve, dir } => {
                let dn = dir.normalize();
                let u = match hint {
                    Some(h) => self.wrap(0, h.x),
                    None => seed_curve(curve, |q| (p - q).cross(dn).length()),
                };
                let v = (p - curve.eval(u)).dot(*dir) / dir.length_squared();
                self.newton(p, DVec2::new(u, v))
            }
            Kind::BSpline(s) => {
                let seed = match hint {
                    Some(h) => DVec2::new(self.wrap(0, h.x), self.wrap(1, h.y)),
                    None => seed_surface(s, p),
                };
                let a = self.newton(p, seed);
                // a hint far from the solution can end in another local minimum
                if hint.is_some() && (self.eval(a) - p).length() > 1e3 * self.eps {
                    let b = self.newton(p, seed_surface(s, p));
                    if (self.eval(b) - p).length() < (self.eval(a) - p).length() {
                        return b;
                    }
                }
                a
            }
        }
    }

    /// Foot of `p` near `seed` (local: no global search), for points off the surface.
    pub fn project(&self, p: DVec3, seed: DVec2) -> DVec2 {
        match &self.kind {
            Kind::Revolution { .. } | Kind::Extrusion { .. } | Kind::BSpline(_) => self.newton(p, seed),
            _ => self.invert(p, Some(seed)),
        }
    }

    /// Closest point by Newton iteration (Piegl & Tiller 6.1) from `uv`, kept in the natural
    /// domain (periodic parameters wrap).
    fn newton(&self, p: DVec3, mut uv: DVec2) -> DVec2 {
        let clamp = |g: &Geom, uv: DVec2| {
            let mut q = uv;
            for i in 0..2 {
                let t = if i == 0 { &mut q.x } else { &mut q.y };
                if g.period[i].is_some() {
                    *t = g.wrap(i, *t);
                } else {
                    *t = t.clamp(g.domain[i].0, g.domain[i].1);
                }
            }
            q
        };
        uv = clamp(self, uv);
        let mut best = (uv, (self.eval(uv) - p).length());
        for _ in 0..40 {
            let d = self.ders(uv);
            let r = d[0] - p;
            let f = d[1].dot(r);
            let g = d[2].dot(r);
            let (mut j00, mut j01, mut j11) = (
                d[1].length_squared() + r.dot(d[3]),
                d[1].dot(d[2]) + r.dot(d[4]),
                d[2].length_squared() + r.dot(d[5]),
            );
            let mut det = j00 * j11 - j01 * j01;
            if !(j00 > 0.0 && det > 0.0) {
                // not convex here (curvature against a slow parameter): Gauss-Newton
                (j00, j01, j11) = (d[1].length_squared(), d[1].dot(d[2]), d[2].length_squared());
                det = j00 * j11 - j01 * j01;
            }
            let step = if det.abs() > 1e-300 && det.is_finite() {
                DVec2::new((-f * j11 + g * j01) / det, (f * j01 - g * j00) / det)
            } else {
                // gradient step
                let gl = d[1].length_squared() + d[2].length_squared();
                if gl <= 1e-300 {
                    break;
                }
                DVec2::new(-f, -g) / gl
            };
            if !step.is_finite() {
                break;
            }
            // damping: halve the step while the distance grows
            let mut s = 1.0;
            let mut next = clamp(self, uv + step);
            let mut dist = (self.eval(next) - p).length();
            for _ in 0..8 {
                if dist <= best.1 * (1.0 + 1e-12) {
                    break;
                }
                s *= 0.5;
                next = clamp(self, uv + step * s);
                dist = (self.eval(next) - p).length();
            }
            let moved = (next - uv).length();
            uv = next;
            if dist < best.1 {
                best = (uv, dist);
            }
            if moved <= 1e-15 * (1.0 + uv.length()) || dist <= 1e-14 * (1.0 + p.length()) {
                break;
            }
        }
        best.0
    }
}

/// Parameter of the curve sample minimising `f` (dense samples per knot span).
fn seed_curve(c: &bspline::Curve, f: impl Fn(DVec3) -> f64) -> f64 {
    let br = c.breaks();
    let per = (4 * c.degree).clamp(4, 16);
    let mut best = (br[0], f64::INFINITY);
    for w in br.windows(2) {
        for k in 0..=per {
            let t = w[0] + (w[1] - w[0]) * k as f64 / per as f64;
            let d = f(c.eval(t));
            if d < best.1 {
                best = (t, d);
            }
        }
    }
    // golden-section refinement around the best sample is left to the surface Newton step
    best.0
}

/// Parameters of the surface sample nearest `p` (a grid over the knot spans).
fn seed_surface(s: &bspline::Surface, p: DVec3) -> DVec2 {
    let lines = |dir: usize, deg: usize| -> Vec<f64> {
        let br = s.breaks(dir);
        let per = (2 * deg).clamp(2, 8);
        let mut out = Vec::new();
        for w in br.windows(2) {
            for k in 0..per {
                out.push(w[0] + (w[1] - w[0]) * k as f64 / per as f64);
            }
        }
        out.push(*br.last().unwrap());
        // very dense nets: thin out to at most 64 lines
        if out.len() > 64 {
            let step = out.len() as f64 / 64.0;
            out = (0..64).map(|i| out[(i as f64 * step) as usize]).chain([*br.last().unwrap()]).collect();
        }
        out
    };
    let (us, vs) = (lines(0, s.pu), lines(1, s.pv));
    let mut best = (DVec2::new(us[0], vs[0]), f64::INFINITY);
    for &u in &us {
        for &v in &vs {
            let d = (s.eval_d(u, v)[0] - p).length_squared();
            if d < best.1 {
                best = (DVec2::new(u, v), d);
            }
        }
    }
    best.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_kernel::brep::{BSplineCurve, Frame};

    fn frame() -> Frame {
        Frame::new(DVec3::new(1.0, 2.0, 3.0), DVec3::new(0.2, 0.3, 1.0), DVec3::new(1.0, 0.0, 0.1)).unwrap()
    }

    fn check(s: &Surface, uvs: &[(f64, f64)]) {
        let g = Geom::new(s, 1e-9);
        for &(u, v) in uvs {
            let uv = DVec2::new(u, v);
            let d = g.ders(uv);
            let h = 1e-5;
            let fu = (g.eval(uv + DVec2::X * h) - g.eval(uv - DVec2::X * h)) / (2.0 * h);
            let fv = (g.eval(uv + DVec2::Y * h) - g.eval(uv - DVec2::Y * h)) / (2.0 * h);
            assert!((d[1] - fu).length() < 1e-5 && (d[2] - fv).length() < 1e-5, "{} at {uv}", s.kind());
            let fuu = (g.eval(uv + DVec2::X * h) - 2.0 * d[0] + g.eval(uv - DVec2::X * h)) / (h * h);
            assert!((d[3] - fuu).length() < 1e-2, "{} suu {} {}", s.kind(), d[3], fuu);
            // the normal is the STEP normal
            let n = g.normal(uv, (DVec2::splat(-1.0), DVec2::splat(1.0))).unwrap();
            if let Some(m) = s.normal_at(d[0], Some((u, v))) {
                assert!(n.dot(m) > 0.999, "{}: {n} vs {m}", s.kind());
            }
            // inversion returns the point
            let back = g.invert(d[0], None);
            assert!((g.eval(back) - d[0]).length() < 1e-9, "{} at {uv}: {back}", s.kind());
            let near = g.invert(d[0], Some(uv + DVec2::splat(0.01)));
            assert!((g.eval(near) - d[0]).length() < 1e-9, "{} hint at {uv}: {near}", s.kind());
        }
    }

    const UVS: [(f64, f64); 4] = [(0.3, 0.2), (2.0, -0.7), (4.0, 1.1), (5.9, 0.4)];

    #[test]
    fn analytic_surfaces() {
        let f = frame();
        check(&Surface::Plane { frame: f }, &UVS);
        check(&Surface::Cylinder { frame: f, r: 2.0 }, &UVS);
        check(&Surface::Cone { frame: f, r: 2.0, semi_angle: 0.4 }, &UVS);
        check(&Surface::Sphere { frame: f, r: 2.0 }, &UVS);
        check(&Surface::Torus { frame: f, major: 5.0, minor: 1.5 }, &UVS);
        // poles
        let g = Geom::new(&Surface::Sphere { frame: f, r: 2.0 }, 1e-9);
        assert_eq!(g.pole(f.origin + f.z * 2.0), Some(Pole { free: 0, value: FRAC_PI_2 }));
        let g = Geom::new(&Surface::Cone { frame: f, r: 2.0, semi_angle: 0.4 }, 1e-9);
        let apex = f.origin - f.z * (2.0 / 0.4f64.tan());
        assert!(g.pole(apex).is_some());
        let n = g.normal(DVec2::new(1.0, -2.0 / 0.4f64.tan()), (DVec2::ZERO, DVec2::ONE)).unwrap();
        assert!(n.dot(f.z) < 0.0);
    }

    fn profile() -> BSplineCurve {
        BSplineCurve {
            degree: 3,
            knots: vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0],
            points: vec![
                DVec3::new(2.0, 0.0, 0.0),
                DVec3::new(3.0, 0.0, 1.0),
                DVec3::new(2.5, 0.0, 2.0),
                DVec3::new(3.5, 0.0, 3.0),
                DVec3::new(2.0, 0.0, 4.0),
            ],
            weights: None,
        }
    }

    #[test]
    fn revolution_extrusion_bspline() {
        let c = profile();
        let rev = Surface::Revolution { curve: c.clone(), axis_origin: DVec3::ZERO, axis_dir: DVec3::Z };
        check(&rev, &[(0.3, 0.2), (2.0, 0.7), (4.0, 0.45), (5.9, 0.9)]);
        let ext = Surface::Extrusion { curve: c.clone(), dir: DVec3::new(0.0, 2.0, 0.3) };
        check(&ext, &[(0.2, 0.3), (0.7, -2.0), (0.45, 4.0), (0.9, 5.9)]);
        // a bicubic patch
        let mut pts = Vec::new();
        for i in 0..5 {
            for j in 0..4 {
                pts.push(DVec3::new(i as f64, j as f64, ((i * j) % 3) as f64 * 0.5));
            }
        }
        let bs = Surface::BSpline(cad_kernel::brep::BSplineSurface {
            degree_u: 3,
            degree_v: 2,
            knots_u: c.knots.clone(),
            knots_v: vec![0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0],
            nu: 5,
            nv: 4,
            points: pts,
            weights: None,
        });
        check(&bs, &[(0.2, 0.3), (0.7, 0.1), (0.45, 0.8), (0.9, 0.55)]);
    }
}
