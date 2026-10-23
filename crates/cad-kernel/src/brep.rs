//! Neutral boundary representation: topology and exact geometry of a shape, independent of the
//! kernel. Kernels produce it ([`crate::Kernel::brep`]); exporters (STEP, `cad-step`) consume it.
//!
//! Conventions (they match ISO 10303-42, so a STEP writer maps them one to one):
//! * Every [`Face`]'s normal is the surface normal ([`Surface::normal_at`]) if `same_sense`,
//!   else its opposite. It points out of the material, for void shells into the void.
//! * Every loop runs counter-clockwise about the face normal (face to the left).
//! * An [`Edge`]'s curve runs from `start` to `end` with increasing parameter: B-splines over
//!   their whole domain, lines along `dir`, circles and ellipses counter-clockwise about their
//!   frame's z axis.
//!
//! Units: millimetres; angles in radians.

use glam::DVec3;
use serde::{Deserialize, Serialize};

use crate::{FaceId, Tag};

/// Right-handed orthonormal placement: `z` is the axis, `x` the reference direction.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub origin: DVec3,
    pub z: DVec3,
    pub x: DVec3,
}

impl Frame {
    /// Orthonormalised frame (`x` made perpendicular to `z`), `None` if degenerate.
    pub fn new(origin: DVec3, z: DVec3, x: DVec3) -> Option<Frame> {
        let z = z.try_normalize()?;
        let x = (x - z * x.dot(z)).try_normalize().unwrap_or_else(|| z.any_orthonormal_vector());
        if !(origin.is_finite() && z.is_finite() && x.is_finite()) {
            return None;
        }
        Some(Frame { origin, z, x })
    }

    pub fn y(&self) -> DVec3 {
        self.z.cross(self.x)
    }
}

/// Non-uniform rational B-spline curve (clamped or not). `weights` is `None` when polynomial.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BSplineCurve {
    pub degree: u32,
    /// `points.len() + degree + 1` values, non-decreasing.
    pub knots: Vec<f64>,
    pub points: Vec<DVec3>,
    pub weights: Option<Vec<f64>>,
}

/// NURBS surface. Control points are u-major: `points[i * nv + j]` for u index `i`, v index `j`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BSplineSurface {
    pub degree_u: u32,
    pub degree_v: u32,
    pub knots_u: Vec<f64>,
    pub knots_v: Vec<f64>,
    pub nu: usize,
    pub nv: usize,
    pub points: Vec<DVec3>,
    pub weights: Option<Vec<f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Curve {
    /// Unbounded line `origin + s * dir`, `dir` unit length.
    Line {
        origin: DVec3,
        dir: DVec3,
    },
    Circle {
        frame: Frame,
        r: f64,
    },
    /// `r1` along the frame's x axis, `r2` along y.
    Ellipse {
        frame: Frame,
        r1: f64,
        r2: f64,
    },
    BSpline(BSplineCurve),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Surface {
    /// Normal: frame z.
    Plane { frame: Frame },
    /// Normal points away from the axis.
    Cylinder { frame: Frame, r: f64 },
    /// Radius `r` in the frame's xy-plane, growing along +z: `r + h * tan(semi_angle)`,
    /// `0 < semi_angle < π/2`. Normal points away from the axis.
    Cone { frame: Frame, r: f64, semi_angle: f64 },
    /// Normal points away from the centre.
    Sphere { frame: Frame, r: f64 },
    /// Major radius `major` (axis to tube centre), tube radius `minor`. Normal points away from
    /// the tube's centre circle.
    Torus { frame: Frame, major: f64, minor: f64 },
    /// `curve` swept around the axis (`σ(u, v)`: `curve(v)` rotated by `u`); normal
    /// `∂σ/∂u × ∂σ/∂v`.
    Revolution { curve: BSplineCurve, axis_origin: DVec3, axis_dir: DVec3 },
    /// `curve(u) + v * dir`; normal `curve'(u) × dir`.
    Extrusion { curve: BSplineCurve, dir: DVec3 },
    /// Normal `∂σ/∂u × ∂σ/∂v`.
    BSpline(BSplineSurface),
}

impl Surface {
    pub fn kind(&self) -> &'static str {
        match self {
            Surface::Plane { .. } => "plane",
            Surface::Cylinder { .. } => "cylinder",
            Surface::Cone { .. } => "cone",
            Surface::Sphere { .. } => "sphere",
            Surface::Torus { .. } => "torus",
            Surface::Revolution { .. } => "revolution",
            Surface::Extrusion { .. } => "extrusion",
            Surface::BSpline(s) if s.weights.is_some() => "nurbs",
            Surface::BSpline(_) => "bspline",
        }
    }
}

impl Curve {
    pub fn kind(&self) -> &'static str {
        match self {
            Curve::Line { .. } => "line",
            Curve::Circle { .. } => "circle",
            Curve::Ellipse { .. } => "ellipse",
            Curve::BSpline(c) if c.weights.is_some() => "nurbs",
            Curve::BSpline(_) => "bspline",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub start: u32,
    pub end: u32,
    pub curve: Curve,
    /// The kernel's curve was not a B-spline and was approximated.
    pub approximated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrientedEdge {
    pub edge: u32,
    /// `true`: traversed from the edge's start to its end.
    pub forward: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Loop {
    Edges(Vec<OrientedEdge>),
    /// A loop degenerated to one vertex (e.g. a cone apex).
    Vertex(u32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Face {
    /// Index of the face in [`crate::Kernel::faces`] of the same shape.
    pub id: FaceId,
    pub tag: Tag,
    pub surface: Surface,
    pub same_sense: bool,
    pub loops: Vec<Loop>,
    /// The kernel's surface was not a B-spline and was approximated.
    pub approximated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shell {
    pub faces: Vec<Face>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Body {
    /// Closed solid: outer shell and voids.
    Solid { outer: Shell, voids: Vec<Shell> },
    /// Open surface(s).
    Sheet { shells: Vec<Shell> },
}

/// Topology and geometry of one shape. Vertices and edges are shared by its bodies.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Brep {
    pub vertices: Vec<DVec3>,
    pub edges: Vec<Edge>,
    pub bodies: Vec<Body>,
}

impl Brep {
    /// All faces of all bodies.
    pub fn faces(&self) -> impl Iterator<Item = &Face> {
        self.bodies.iter().flat_map(|b| {
            let shells: Vec<&Shell> = match b {
                Body::Solid { outer, voids } => std::iter::once(outer).chain(voids.iter()).collect(),
                Body::Sheet { shells } => shells.iter().collect(),
            };
            shells.into_iter().flat_map(|s| s.faces.iter())
        })
    }

    /// Applies `m` (rigid motion or uniform scale with positive determinant) to all geometry.
    pub fn transform(&mut self, m: &glam::DMat4) {
        for v in &mut self.vertices {
            *v = m.transform_point3(*v);
        }
        let s = m.transform_vector3(DVec3::X).length();
        let frame = |f: &mut Frame| {
            f.origin = m.transform_point3(f.origin);
            f.z = m.transform_vector3(f.z).normalize();
            f.x = m.transform_vector3(f.x).normalize();
        };
        let bc = |c: &mut BSplineCurve| c.points.iter_mut().for_each(|p| *p = m.transform_point3(*p));
        for e in &mut self.edges {
            match &mut e.curve {
                Curve::Line { origin, dir } => {
                    *origin = m.transform_point3(*origin);
                    *dir = m.transform_vector3(*dir).normalize();
                }
                Curve::Circle { frame: f, r } => {
                    frame(f);
                    *r *= s;
                }
                Curve::Ellipse { frame: f, r1, r2 } => {
                    frame(f);
                    *r1 *= s;
                    *r2 *= s;
                }
                Curve::BSpline(c) => bc(c),
            }
        }
        let shells: Vec<&mut Shell> = self
            .bodies
            .iter_mut()
            .flat_map(|b| -> Vec<&mut Shell> {
                match b {
                    Body::Solid { outer, voids } => std::iter::once(outer).chain(voids.iter_mut()).collect(),
                    Body::Sheet { shells } => shells.iter_mut().collect(),
                }
            })
            .collect();
        for sh in shells {
            for f in &mut sh.faces {
                match &mut f.surface {
                    Surface::Plane { frame: fr } => frame(fr),
                    Surface::Cylinder { frame: fr, r } | Surface::Sphere { frame: fr, r } => {
                        frame(fr);
                        *r *= s;
                    }
                    Surface::Cone { frame: fr, r, .. } => {
                        frame(fr);
                        *r *= s;
                    }
                    Surface::Torus { frame: fr, major, minor } => {
                        frame(fr);
                        *major *= s;
                        *minor *= s;
                    }
                    Surface::Revolution { curve, axis_origin, axis_dir } => {
                        bc(curve);
                        *axis_origin = m.transform_point3(*axis_origin);
                        *axis_dir = m.transform_vector3(*axis_dir).normalize();
                    }
                    Surface::Extrusion { curve, dir } => {
                        bc(curve);
                        *dir = m.transform_vector3(*dir);
                    }
                    Surface::BSpline(b) => b.points.iter_mut().for_each(|p| *p = m.transform_point3(*p)),
                }
            }
        }
    }

    /// Structural check: indices in range, loops closed (each edge-use ends where the next one
    /// starts), B-spline sizes consistent. Returns the first problem.
    pub fn validate(&self) -> Result<(), String> {
        let nv = self.vertices.len() as u32;
        for (i, e) in self.edges.iter().enumerate() {
            if e.start >= nv || e.end >= nv {
                return Err(format!("edge {i}: vertex index out of range"));
            }
            if let Curve::BSpline(c) = &e.curve {
                c.check().map_err(|m| format!("edge {i}: {m}"))?;
            }
        }
        for f in self.faces() {
            if let Surface::BSpline(s) = &f.surface {
                s.check().map_err(|m| format!("face {}: {m}", f.id))?;
            }
            if f.loops.is_empty() {
                return Err(format!("face {}: no loop", f.id));
            }
            for l in &f.loops {
                match l {
                    Loop::Vertex(v) if *v >= nv => return Err(format!("face {}: vertex out of range", f.id)),
                    Loop::Vertex(_) => {}
                    Loop::Edges(es) => {
                        if es.is_empty() {
                            return Err(format!("face {}: empty loop", f.id));
                        }
                        for (k, oe) in es.iter().enumerate() {
                            let Some(e) = self.edges.get(oe.edge as usize) else {
                                return Err(format!("face {}: edge index out of range", f.id));
                            };
                            let next = &es[(k + 1) % es.len()];
                            let ne = &self.edges[next.edge as usize];
                            let end = if oe.forward { e.end } else { e.start };
                            let start = if next.forward { ne.start } else { ne.end };
                            if end != start {
                                return Err(format!("face {}: loop not closed after edge {}", f.id, oe.edge));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

// ------------------------------------------------------------------------------------------
// B-spline evaluation and editing
// ------------------------------------------------------------------------------------------

/// Index `k` of the knot span with `knots[k] <= t < knots[k + 1]` among the spans of the domain
/// (`degree <= k < n`), clamping `t` into the domain.
fn find_span(knots: &[f64], degree: usize, n: usize, t: f64) -> usize {
    if t >= knots[n] {
        let mut k = n - 1;
        while k > degree && knots[k] >= knots[n] {
            k -= 1;
        }
        return k;
    }
    if t <= knots[degree] {
        let mut k = degree;
        while k + 1 < n && knots[k + 1] <= knots[degree] {
            k += 1;
        }
        return k;
    }
    let (mut lo, mut hi) = (degree, n);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if t < knots[mid] { hi = mid } else { lo = mid }
    }
    lo
}

/// `a >= b`, false if either is NaN.
fn ge(a: f64, b: f64) -> bool {
    matches!(a.partial_cmp(&b), Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal))
}

/// `a > b`, false if either is NaN.
fn gt(a: f64, b: f64) -> bool {
    matches!(a.partial_cmp(&b), Some(std::cmp::Ordering::Greater))
}

/// De Boor on homogeneous 4D points.
fn de_boor(knots: &[f64], degree: usize, pw: &[[f64; 4]], t: f64) -> [f64; 4] {
    let n = pw.len();
    let k = find_span(knots, degree, n, t);
    let mut d: Vec<[f64; 4]> = (0..=degree).map(|j| pw[j + k - degree]).collect();
    for r in 1..=degree {
        for j in (r..=degree).rev() {
            let i = j + k - degree;
            let den = knots[i + degree + 1 - r] - knots[i];
            let a = if den.abs() > 0.0 { (t - knots[i]) / den } else { 0.0 };
            let prev = d[j - 1];
            for (c, x) in d[j].iter_mut().enumerate() {
                *x = (1.0 - a) * prev[c] + a * *x;
            }
        }
    }
    d[degree]
}

fn homog(p: DVec3, w: f64) -> [f64; 4] {
    [p.x * w, p.y * w, p.z * w, w]
}

fn dehomog(h: [f64; 4]) -> DVec3 {
    DVec3::new(h[0] / h[3], h[1] / h[3], h[2] / h[3])
}

impl BSplineCurve {
    pub fn weight(&self, i: usize) -> f64 {
        self.weights.as_ref().map_or(1.0, |w| w[i])
    }

    /// Parameter domain `[knots[p], knots[n]]`.
    pub fn domain(&self) -> (f64, f64) {
        (self.knots[self.degree as usize], self.knots[self.points.len()])
    }

    pub fn check(&self) -> Result<(), String> {
        let p = self.degree as usize;
        if p == 0 || self.points.len() < p + 1 {
            return Err(format!("B-spline curve: degree {p} with {} control points", self.points.len()));
        }
        if self.knots.len() != self.points.len() + p + 1 {
            return Err("B-spline curve: knot count".into());
        }
        if self.knots.windows(2).any(|w| !ge(w[1], w[0])) || !gt(self.domain().1, self.domain().0) {
            return Err("B-spline curve: knots not increasing".into());
        }
        if let Some(w) = &self.weights
            && (w.len() != self.points.len() || w.iter().any(|w| !gt(*w, 0.0)))
        {
            return Err("B-spline curve: weights".into());
        }
        if self.points.iter().any(|p| !p.is_finite()) || self.knots.iter().any(|k| !k.is_finite()) {
            return Err("B-spline curve: non-finite value".into());
        }
        Ok(())
    }

    fn pw(&self) -> Vec<[f64; 4]> {
        self.points.iter().enumerate().map(|(i, p)| homog(*p, self.weight(i))).collect()
    }

    pub fn eval(&self, t: f64) -> DVec3 {
        dehomog(de_boor(&self.knots, self.degree as usize, &self.pw(), t))
    }

    /// Evaluates at many parameters (shares the homogeneous control points).
    pub fn eval_many(&self, ts: &[f64]) -> Vec<DVec3> {
        let pw = self.pw();
        ts.iter().map(|&t| dehomog(de_boor(&self.knots, self.degree as usize, &pw, t))).collect()
    }

    /// Tangent by central differences (not normalised).
    pub fn tangent(&self, t: f64) -> DVec3 {
        let (a, b) = self.domain();
        let h = 1e-6 * (b - a);
        let (t0, t1) = ((t - h).max(a), (t + h).min(b));
        (self.eval(t1) - self.eval(t0)) / (t1 - t0)
    }

    pub fn is_rational(&self) -> bool {
        self.weights.as_ref().is_some_and(|w| w.iter().any(|x| (x - w[0]).abs() > 1e-14 * w[0].abs()))
    }

    /// Inserts knot `t` once (Boehm).
    fn insert(&mut self, t: f64) {
        let p = self.degree as usize;
        let n = self.points.len();
        let k = find_span(&self.knots, p, n, t);
        let s = self.knots.iter().filter(|&&u| u == t).count();
        let pw = self.pw();
        let mut q = Vec::with_capacity(n + 1);
        for i in 0..=n {
            if i + p <= k {
                q.push(pw[i]);
            } else if i + s > k {
                q.push(pw[i - 1]);
            } else {
                let a = (t - self.knots[i]) / (self.knots[i + p] - self.knots[i]);
                let mut h = [0.0; 4];
                for c in 0..4 {
                    h[c] = a * pw[i][c] + (1.0 - a) * pw[i - 1][c];
                }
                q.push(h);
            }
        }
        self.knots.insert(k + 1, t);
        self.points = q.iter().map(|h| dehomog(*h)).collect();
        if self.weights.is_some() {
            self.weights = Some(q.iter().map(|h| h[3]).collect());
        }
    }

    /// The piece over `[t0, t1]` (inside the domain), as a clamped curve with domain
    /// `[t0, t1]`. Parameters within `1e-12` of the domain size snap to existing knots.
    pub fn trimmed(&self, t0: f64, t1: f64) -> BSplineCurve {
        let (a, b) = self.domain();
        let eps = 1e-12 * (b - a).max(1.0);
        let snap = |t: f64| -> f64 {
            let t = t.clamp(a, b);
            self.knots.iter().copied().find(|k| (k - t).abs() <= eps).unwrap_or(t)
        };
        let (t0, t1) = (snap(t0), snap(t1));
        let p = self.degree as usize;
        let mut c = self.clone();
        for t in [t0, t1] {
            let s = c.knots.iter().filter(|&&u| u == t).count();
            for _ in s..p {
                c.insert(t);
            }
        }
        let r = c.knots.iter().rposition(|&u| u == t0).unwrap_or(p);
        let q = c.knots.iter().position(|&u| u == t1).unwrap_or(c.points.len());
        let (s0, e0) = (r - p, q - 1);
        let mut knots = vec![t0; p + 1];
        knots.extend(c.knots[r + 1..q].iter().copied());
        knots.extend(std::iter::repeat_n(t1, p + 1));
        let points = c.points[s0..=e0].to_vec();
        let weights = c.weights.as_ref().map(|w| w[s0..=e0].to_vec());
        let mut out = BSplineCurve { degree: c.degree, knots, points, weights };
        if !out.is_rational() {
            out.weights = None;
        }
        out
    }

    /// The same point set traversed backwards.
    pub fn reversed(&self) -> BSplineCurve {
        let (a, b) = self.domain();
        let knots: Vec<f64> = self.knots.iter().rev().map(|k| a + b - k).collect();
        let mut points = self.points.clone();
        points.reverse();
        let weights = self.weights.as_ref().map(|w| w.iter().rev().copied().collect());
        BSplineCurve { degree: self.degree, knots, points, weights }
    }
}

impl BSplineSurface {
    pub fn weight(&self, i: usize, j: usize) -> f64 {
        self.weights.as_ref().map_or(1.0, |w| w[i * self.nv + j])
    }

    pub fn domain(&self) -> ((f64, f64), (f64, f64)) {
        let (p, q) = (self.degree_u as usize, self.degree_v as usize);
        ((self.knots_u[p], self.knots_u[self.nu]), (self.knots_v[q], self.knots_v[self.nv]))
    }

    pub fn check(&self) -> Result<(), String> {
        let (p, q) = (self.degree_u as usize, self.degree_v as usize);
        if p == 0 || q == 0 || self.nu < p + 1 || self.nv < q + 1 {
            return Err("B-spline surface: degree/control net".into());
        }
        if self.points.len() != self.nu * self.nv
            || self.knots_u.len() != self.nu + p + 1
            || self.knots_v.len() != self.nv + q + 1
        {
            return Err("B-spline surface: sizes".into());
        }
        for k in [&self.knots_u, &self.knots_v] {
            if k.windows(2).any(|w| !ge(w[1], w[0])) || k.iter().any(|x| !x.is_finite()) {
                return Err("B-spline surface: knots".into());
            }
        }
        let ((u0, u1), (v0, v1)) = self.domain();
        if !(gt(u1, u0) && gt(v1, v0)) {
            return Err("B-spline surface: empty domain".into());
        }
        if let Some(w) = &self.weights
            && (w.len() != self.points.len() || w.iter().any(|w| !gt(*w, 0.0)))
        {
            return Err("B-spline surface: weights".into());
        }
        if self.points.iter().any(|p| !p.is_finite()) {
            return Err("B-spline surface: non-finite value".into());
        }
        Ok(())
    }

    pub fn is_rational(&self) -> bool {
        self.weights.as_ref().is_some_and(|w| w.iter().any(|x| (x - w[0]).abs() > 1e-14 * w[0].abs()))
    }

    pub fn eval(&self, u: f64, v: f64) -> DVec3 {
        let q = self.degree_v as usize;
        let rows: Vec<[f64; 4]> = (0..self.nu)
            .map(|i| {
                let row: Vec<[f64; 4]> =
                    (0..self.nv).map(|j| homog(self.points[i * self.nv + j], self.weight(i, j))).collect();
                de_boor(&self.knots_v, q, &row, v)
            })
            .collect();
        dehomog(de_boor(&self.knots_u, self.degree_u as usize, &rows, u))
    }

    /// `∂σ/∂u × ∂σ/∂v` by central differences (not normalised).
    pub fn normal(&self, u: f64, v: f64) -> DVec3 {
        let ((u0, u1), (v0, v1)) = self.domain();
        let (hu, hv) = (1e-6 * (u1 - u0), 1e-6 * (v1 - v0));
        let (ua, ub) = ((u - hu).max(u0), (u + hu).min(u1));
        let (va, vb) = ((v - hv).max(v0), (v + hv).min(v1));
        let su = (self.eval(ub, v) - self.eval(ua, v)) / (ub - ua);
        let sv = (self.eval(u, vb) - self.eval(u, va)) / (vb - va);
        su.cross(sv)
    }

    /// `n x n` sample points over the domain (corners included).
    pub fn samples(&self, n: usize) -> Vec<(f64, f64, DVec3)> {
        let ((u0, u1), (v0, v1)) = self.domain();
        let mut out = Vec::with_capacity(n * n);
        for i in 0..n {
            for j in 0..n {
                let u = u0 + (u1 - u0) * i as f64 / (n - 1) as f64;
                let v = v0 + (v1 - v0) * j as f64 / (n - 1) as f64;
                out.push((u, v, self.eval(u, v)));
            }
        }
        out
    }
}

// ------------------------------------------------------------------------------------------
// analytic geometry
// ------------------------------------------------------------------------------------------

/// Minimises `f` over `[a, b]`: dense samples, then golden-section search around the best.
fn minimise(a: f64, b: f64, f: impl Fn(f64) -> f64) -> (f64, f64) {
    const N: usize = 96;
    let mut best = (a, f(a));
    for i in 1..=N {
        let t = a + (b - a) * i as f64 / N as f64;
        let v = f(t);
        if v < best.1 {
            best = (t, v);
        }
    }
    let h = (b - a) / N as f64;
    let (mut lo, mut hi) = ((best.0 - h).max(a), (best.0 + h).min(b));
    let g = 0.5 * (5f64.sqrt() - 1.0);
    let (mut x1, mut x2) = (hi - g * (hi - lo), lo + g * (hi - lo));
    let (mut f1, mut f2) = (f(x1), f(x2));
    for _ in 0..60 {
        if f1 < f2 {
            hi = x2;
            x2 = x1;
            f2 = f1;
            x1 = hi - g * (hi - lo);
            f1 = f(x1);
        } else {
            lo = x1;
            x1 = x2;
            f1 = f2;
            x2 = lo + g * (hi - lo);
            f2 = f(x2);
        }
    }
    let t = 0.5 * (lo + hi);
    let v = f(t);
    if v < best.1 { (t, v) } else { best }
}

fn rotate_about(v: DVec3, axis: DVec3, angle: f64) -> DVec3 {
    let (s, c) = angle.sin_cos();
    v * c + axis.cross(v) * s + axis * axis.dot(v) * (1.0 - c)
}

impl Surface {
    /// Distance of `p` from the (unbounded) surface; for B-spline surfaces `None`.
    pub fn distance(&self, p: DVec3) -> Option<f64> {
        let radial = |f: &Frame| {
            let d = p - f.origin;
            let h = d.dot(f.z);
            (d - f.z * h, h)
        };
        Some(match self {
            Surface::Plane { frame } => (p - frame.origin).dot(frame.z).abs(),
            Surface::Cylinder { frame, r } => (radial(frame).0.length() - r).abs(),
            Surface::Cone { frame, r, semi_angle } => {
                let (rv, h) = radial(frame);
                (rv.length() - (r + h * semi_angle.tan())).abs() * semi_angle.cos()
            }
            Surface::Sphere { frame, r } => ((p - frame.origin).length() - r).abs(),
            Surface::Torus { frame, major, minor } => {
                let (rv, h) = radial(frame);
                ((rv.length() - major).hypot(h) - minor).abs()
            }
            Surface::Revolution { curve, axis_origin, axis_dir } => {
                let rh = |q: DVec3| {
                    let d = q - *axis_origin;
                    let h = d.dot(*axis_dir);
                    ((d - *axis_dir * h).length(), h)
                };
                let (rp, hp) = rh(p);
                let (a, b) = curve.domain();
                minimise(a, b, |t| {
                    let (rc, hc) = rh(curve.eval(t));
                    (rp - rc).hypot(hp - hc)
                })
                .1
            }
            Surface::Extrusion { curve, dir } => {
                let d = dir.normalize();
                let (a, b) = curve.domain();
                minimise(a, b, |t| (p - curve.eval(t)).cross(d).length()).1
            }
            Surface::BSpline(_) => return None,
        })
    }

    /// Surface normal (STEP parameterisation, see the variants) at `p` on the surface; `uv` are
    /// the parameters of `p` for B-spline surfaces. `None` where it is undefined (on an axis).
    pub fn normal_at(&self, p: DVec3, uv: Option<(f64, f64)>) -> Option<DVec3> {
        let radial = |f: &Frame| {
            let d = p - f.origin;
            d - f.z * d.dot(f.z)
        };
        let n = match self {
            Surface::Plane { frame } => frame.z,
            Surface::Cylinder { frame, .. } => radial(frame),
            Surface::Cone { frame, semi_angle, .. } => radial(frame).try_normalize()? - frame.z * semi_angle.tan(),
            Surface::Sphere { frame, .. } => p - frame.origin,
            Surface::Torus { frame, major, .. } => {
                let rv = radial(frame).try_normalize()?;
                p - (frame.origin + rv * *major)
            }
            Surface::Revolution { curve, axis_origin, axis_dir } => {
                let z = *axis_dir;
                let rh = |q: DVec3| {
                    let d = q - *axis_origin;
                    let h = d.dot(z);
                    (d - z * h, h)
                };
                let (rp, hp) = rh(p);
                let (a, b) = curve.domain();
                let (t, _) = minimise(a, b, |t| {
                    let (rc, hc) = rh(curve.eval(t));
                    (rp.length() - rc.length()).hypot(hp - hc)
                });
                let (rc, _) = rh(curve.eval(t));
                let (ep, ec) = (rp.try_normalize()?, rc.try_normalize()?);
                let angle = ec.cross(ep).dot(z).atan2(ec.dot(ep));
                let sv = rotate_about(curve.tangent(t), z, angle);
                let su = z.cross(p - *axis_origin);
                su.cross(sv)
            }
            Surface::Extrusion { curve, dir } => {
                let d = dir.normalize();
                let (a, b) = curve.domain();
                let (t, _) = minimise(a, b, |t| (p - curve.eval(t)).cross(d).length());
                curve.tangent(t).cross(*dir)
            }
            Surface::BSpline(s) => {
                let (u, v) = uv?;
                s.normal(u, v)
            }
        };
        n.try_normalize()
    }
}

// ------------------------------------------------------------------------------------------
// recognition of analytic curves
// ------------------------------------------------------------------------------------------

/// The analytic form of a B-spline edge curve from `a` to `b` if it is (within `tol`) a straight
/// segment or a circular arc, else the curve itself. `c` must run over its domain from `a` to
/// `b` (see [`BSplineCurve::trimmed`]).
pub fn recognise_curve(c: &BSplineCurve, a: DVec3, b: DVec3, tol: f64) -> Curve {
    let (t0, t1) = c.domain();
    // straight: every control point on the segment (convex hull: the curve stays on it)
    let ab = b - a;
    let len = ab.length();
    if len > tol {
        let d = ab / len;
        let on_segment = c.points.iter().all(|p| {
            let s = (*p - a).dot(d);
            (*p - a - d * s).length() <= tol && s >= -tol && s <= len + tol
        });
        if on_segment {
            return Curve::Line { origin: a, dir: d };
        }
    }
    // circle: planar, constant distance from a centre through three curve points
    const N: usize = 64;
    let ts: Vec<f64> = (0..=N).map(|i| t0 + (t1 - t0) * i as f64 / N as f64).collect();
    let pts = c.eval_many(&ts);
    let (p0, p1, p2) = (pts[0], pts[N / 3], pts[2 * N / 3]);
    if let Some((centre, normal, r)) = circle_through(p0, p1, p2)
        && r > tol
        && pts.iter().all(|p| ((*p - centre).length() - r).abs() <= tol && (*p - centre).dot(normal).abs() <= tol)
    {
        // turning direction from the first samples
        let z = (pts[0] - centre).cross(pts[1] - centre);
        let z = if z.dot(normal) < 0.0 { -normal } else { normal };
        if let Some(frame) = Frame::new(centre, z, a - centre) {
            // the arc must not exceed one turn
            let mut total = 0.0;
            for w in pts.windows(2) {
                let (u, v) = (w[0] - centre, w[1] - centre);
                total += u.cross(v).dot(z).atan2(u.dot(v));
            }
            if total > 0.0 && total <= std::f64::consts::TAU + 1e-6 {
                return Curve::Circle { frame, r };
            }
        }
    }
    Curve::BSpline(c.clone())
}

/// Centre, unit normal and radius of the circle through three points.
fn circle_through(a: DVec3, b: DVec3, c: DVec3) -> Option<(DVec3, DVec3, f64)> {
    let (u, v) = (b - a, c - a);
    let w = u.cross(v);
    let w2 = w.length_squared();
    if !gt(w2, 1e-24 * u.length_squared() * v.length_squared()) {
        return None;
    }
    let centre = a + (w.cross(u) * v.length_squared() + v.cross(w) * u.length_squared()) / (2.0 * w2);
    Some((centre, w.normalize(), (a - centre).length()))
}

/// Evaluates an analytic edge curve between its vertices for tests: `n + 1` points from `a`
/// to `b` (lines straight, circles counter-clockwise about the frame).
pub fn sample_edge(curve: &Curve, a: DVec3, b: DVec3, n: usize) -> Vec<DVec3> {
    match curve {
        Curve::Line { .. } => (0..=n).map(|i| a.lerp(b, i as f64 / n as f64)).collect(),
        Curve::Circle { frame, r } => {
            let ang = |p: DVec3| {
                let d = p - frame.origin;
                d.dot(frame.y()).atan2(d.dot(frame.x))
            };
            let a0 = ang(a);
            let mut a1 = ang(b);
            while a1 <= a0 + 1e-12 {
                a1 += std::f64::consts::TAU;
            }
            (0..=n)
                .map(|i| {
                    let t = a0 + (a1 - a0) * i as f64 / n as f64;
                    frame.origin + (frame.x * t.cos() + frame.y() * t.sin()) * *r
                })
                .collect()
        }
        Curve::Ellipse { frame, r1, r2 } => {
            let ang = |p: DVec3| {
                let d = p - frame.origin;
                (d.dot(frame.y()) / r2).atan2(d.dot(frame.x) / r1)
            };
            let a0 = ang(a);
            let mut a1 = ang(b);
            while a1 <= a0 + 1e-12 {
                a1 += std::f64::consts::TAU;
            }
            (0..=n)
                .map(|i| {
                    let t = a0 + (a1 - a0) * i as f64 / n as f64;
                    frame.origin + frame.x * (t.cos() * r1) + frame.y() * (t.sin() * r2)
                })
                .collect()
        }
        Curve::BSpline(c) => {
            let (t0, t1) = c.domain();
            c.eval_many(&(0..=n).map(|i| t0 + (t1 - t0) * i as f64 / n as f64).collect::<Vec<_>>())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quarter_circles() -> BSplineCurve {
        // full unit circle as 4 rational quadratic arcs
        let s = std::f64::consts::FRAC_1_SQRT_2;
        let pts = [
            (1.0, 0.0),
            (1.0, 1.0),
            (0.0, 1.0),
            (-1.0, 1.0),
            (-1.0, 0.0),
            (-1.0, -1.0),
            (0.0, -1.0),
            (1.0, -1.0),
            (1.0, 0.0),
        ];
        BSplineCurve {
            degree: 2,
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0, 4.0],
            points: pts.iter().map(|(x, y)| DVec3::new(*x, *y, 0.0)).collect(),
            weights: Some((0..9).map(|i| if i % 2 == 1 { s } else { 1.0 }).collect()),
        }
    }

    #[test]
    fn rational_circle_evaluates_on_circle() {
        let c = quarter_circles();
        c.check().unwrap();
        for i in 0..=40 {
            let p = c.eval(4.0 * i as f64 / 40.0);
            assert!((p.length() - 1.0).abs() < 1e-12, "{p}");
        }
    }

    #[test]
    fn trimming_keeps_points_and_clamps() {
        let c = quarter_circles();
        let t = c.trimmed(0.5, 2.7);
        t.check().unwrap();
        assert_eq!(t.domain(), (0.5, 2.7));
        for i in 0..=20 {
            let s = 0.5 + 2.2 * i as f64 / 20.0;
            assert!((t.eval(s) - c.eval(s)).length() < 1e-12);
        }
        let whole = c.trimmed(0.0, 4.0);
        assert_eq!(whole.points.len(), c.points.len());
    }

    #[test]
    fn recognises_lines_and_circles() {
        let line = BSplineCurve {
            degree: 1,
            knots: vec![0.0, 0.0, 1.0, 1.0],
            points: vec![DVec3::ZERO, DVec3::new(3.0, 4.0, 0.0)],
            weights: None,
        };
        assert!(matches!(recognise_curve(&line, line.points[0], line.points[1], 1e-9), Curve::Line { .. }));
        let c = quarter_circles();
        let arc = c.trimmed(0.25, 1.5);
        let (a, b) = (arc.eval(0.25), arc.eval(1.5));
        match recognise_curve(&arc, a, b, 1e-9) {
            Curve::Circle { frame, r } => {
                assert!((r - 1.0).abs() < 1e-9);
                assert!(frame.z.z > 0.99);
                let s = sample_edge(&Curve::Circle { frame, r }, a, b, 8);
                assert!((s[4] - arc.eval(arc.domain().0 + 0.5 * (arc.domain().1 - arc.domain().0))).length() < 0.2);
            }
            other => panic!("{other:?}"),
        }
        // reversed arc: clockwise about +z, so the frame flips
        let rev = arc.reversed();
        match recognise_curve(&rev, b, a, 1e-9) {
            Curve::Circle { frame, .. } => assert!(frame.z.z < -0.99),
            other => panic!("{other:?}"),
        }
        // full circle
        assert!(matches!(recognise_curve(&c, c.eval(0.0), c.eval(4.0), 1e-9), Curve::Circle { .. }));
    }

    #[test]
    fn analytic_normals_and_distances() {
        let f = Frame::new(DVec3::ZERO, DVec3::Z, DVec3::X).unwrap();
        let cyl = Surface::Cylinder { frame: f, r: 2.0 };
        let p = DVec3::new(0.0, 2.0, 5.0);
        assert!(cyl.distance(p).unwrap() < 1e-12);
        assert!((cyl.normal_at(p, None).unwrap() - DVec3::Y).length() < 1e-12);
        let cone = Surface::Cone { frame: f, r: 1.0, semi_angle: std::f64::consts::FRAC_PI_4 };
        let q = DVec3::new(2.0, 0.0, 1.0);
        assert!(cone.distance(q).unwrap() < 1e-12);
        let n = cone.normal_at(q, None).unwrap();
        assert!(n.x > 0.0 && n.z < 0.0);
        let tor = Surface::Torus { frame: f, major: 5.0, minor: 1.0 };
        let t = DVec3::new(6.0, 0.0, 0.0);
        assert!(tor.distance(t).unwrap() < 1e-12);
        assert!((tor.normal_at(t, None).unwrap() - DVec3::X).length() < 1e-12);
    }

    #[test]
    fn revolution_and_extrusion() {
        let line = BSplineCurve {
            degree: 1,
            knots: vec![0.0, 0.0, 1.0, 1.0],
            points: vec![DVec3::new(2.0, 0.0, 0.0), DVec3::new(2.0, 0.0, 3.0)],
            weights: None,
        };
        // revolving a line parallel to the axis gives a cylinder of radius 2
        let rev = Surface::Revolution { curve: line.clone(), axis_origin: DVec3::ZERO, axis_dir: DVec3::Z };
        let p = DVec3::new(0.0, -2.0, 1.5);
        assert!(rev.distance(p).unwrap() < 1e-9);
        // σ(u, v) = curve(v) rotated by u: ∂u = z × r, ∂v = +z, normal = (z × r) × z = r: outward
        let n = rev.normal_at(p, None).unwrap();
        assert!((n - DVec3::new(0.0, -1.0, 0.0)).length() < 1e-6, "{n}");
        let ext = Surface::Extrusion { curve: line, dir: DVec3::new(0.0, 1.0, 0.0) };
        let q = DVec3::new(2.0, 7.0, 2.0);
        assert!(ext.distance(q).unwrap() < 1e-9);
        let n = ext.normal_at(q, None).unwrap();
        assert!((n - DVec3::new(-1.0, 0.0, 0.0)).length() < 1e-6, "{n}");
    }
}
