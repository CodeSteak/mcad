//! Measuring in a section plane: contours of the plane ∩ B-rep, exact where the cut surface
//! is analytic.
//!
//! The kernel intersects the solid with the half space that stays (`(q − p) · n ≤ 0`, the
//! convention of `cad render --section` and the UI); the faces of the result that lie in the
//! plane (the caps) bound the section. Their edges are the intersection curves. Each edge
//! borders one cut face of the original solid: if that face is a plane, cylinder, cone,
//! sphere or torus, the contour segment is computed in closed form (line, circle, ellipse)
//! and the kernel's curve only decides its ends and orientation; its deviation from the exact
//! curve is reported (`Seg::dev`). Other surfaces keep the kernel's curve (`exact = false`).
//!
//! Contours run counter-clockwise about `n` around material, clockwise around holes
//! (material to the left), so the inward normal of a contour at a point is `n × tangent`.

use cad_kernel::brep::{Brep, Frame, Loop, Surface};
use glam::DVec3;

use crate::curve::EdgeCurve;
use crate::measure::green;
use crate::num::minimise;

/// Section plane: through `point`, `normal` (unit) points into the removed half.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionPlane {
    pub point: DVec3,
    pub normal: DVec3,
}

impl SectionPlane {
    pub fn new(point: DVec3, normal: DVec3) -> SectionPlane {
        SectionPlane { point, normal: normal.try_normalize().unwrap_or(DVec3::Z) }
    }

    /// Plane frame: origin `point`, z = `normal`, x a stable in-plane direction (world x for
    /// planes perpendicular to z).
    pub fn frame(&self) -> Frame {
        let n = self.normal;
        let x = if n.z.abs() > 0.9 {
            DVec3::X
        } else if n.x.abs() > 0.9 {
            DVec3::Y
        } else {
            DVec3::Z.cross(n)
        };
        Frame::new(self.point, n, x).unwrap_or(Frame { origin: self.point, z: DVec3::Z, x: DVec3::X })
    }

    pub fn dist(&self, q: DVec3) -> f64 {
        (q - self.point).dot(self.normal)
    }

    pub fn project(&self, q: DVec3) -> DVec3 {
        q - self.normal * self.dist(q)
    }
}

/// One contour segment.
#[derive(Debug, Clone)]
pub struct Seg {
    /// The segment in the plane, in the contour's direction.
    pub curve: EdgeCurve,
    /// Closed form (from an analytic cut surface or an analytic edge).
    pub exact: bool,
    /// Largest distance of the kernel's intersection curve from `curve` (mm).
    pub dev: f64,
    /// Kind of the cut surface (`Ebene`, `Zylinder`, …).
    pub source: &'static str,
    /// Provenance tag of the cut face.
    pub tag: u32,
}

#[derive(Debug, Clone)]
pub struct Contour {
    pub segs: Vec<Seg>,
    /// Signed area (counter-clockwise about the normal positive: material outline; negative:
    /// hole).
    pub area: f64,
    pub body: usize,
}

/// A connected piece of the section: outline and holes (indices into `contours`).
#[derive(Debug, Clone)]
pub struct Region {
    pub outer: usize,
    pub holes: Vec<usize>,
    /// Area (outline minus holes), mm².
    pub area: f64,
    pub body: usize,
}

#[derive(Debug, Clone)]
pub struct Section {
    pub plane: SectionPlane,
    pub frame: Frame,
    pub contours: Vec<Contour>,
    pub regions: Vec<Region>,
    /// Largest `Seg::dev` of exact segments.
    pub max_dev: f64,
    /// All segments are exact.
    pub exact: bool,
    /// Diagonal of the section's extent (relative tolerances).
    pub size: f64,
}

/// Wall thickness at a contour point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wall {
    pub thickness: f64,
    /// On the contour (where it was measured) and on the opposite side.
    pub from: DVec3,
    pub to: DVec3,
    pub contour: usize,
    pub seg: usize,
    /// Inward direction (perpendicular to the contour at `from`).
    pub dir: DVec3,
}

fn surface_name(s: &Surface) -> &'static str {
    crate::surface::kind_name(s)
}

/// How well `e` follows `kernel`: interior points of the kernel's curve lie on `e` (the
/// other orientation is the complementary arc); closed curves: same direction at the start.
fn orientation_score(e: &EdgeCurve, kernel: &EdgeCurve, closed: bool) -> f64 {
    if closed {
        return if e.tangent(0.0).dot(kernel.tangent(0.0)) < 0.0 { f64::INFINITY } else { 0.0 };
    }
    [0.25, 0.5, 0.75].iter().map(|&s| e.closest(kernel.point(s)).1.distance(kernel.point(s))).fold(0.0, f64::max)
}

/// Arc through `a` → `b` around `c` in the plane with normal `n`, oriented like `kernel`
/// (its midpoint lies on the arc).
fn oriented_arc(c: DVec3, r: f64, n: DVec3, kernel: &EdgeCurve) -> Option<EdgeCurve> {
    let (a, b) = (kernel.start(), kernel.end());
    let closed = a.distance(b) <= 1e-9 * (1.0 + r);
    let mut best: Option<(f64, EdgeCurve)> = None;
    for z in [n, -n] {
        let frame = Frame::new(c, z, a - c)?;
        let e = if closed {
            EdgeCurve::Arc { frame, r, a0: 0.0, sweep: std::f64::consts::TAU }
        } else {
            let ang = |p: DVec3| {
                let d = p - c;
                d.dot(frame.y()).atan2(d.dot(frame.x))
            };
            let mut sw = (ang(b) - ang(a)).rem_euclid(std::f64::consts::TAU);
            if sw < 1e-12 {
                sw = std::f64::consts::TAU;
            }
            EdgeCurve::Arc { frame, r, a0: 0.0, sweep: sw }
        };
        let d = orientation_score(&e, kernel, closed);
        if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, e));
        }
    }
    best.map(|(_, e)| e)
}

/// Ellipse arc (centre, major axis direction `u` with `r1`, minor `r2`, plane normal `n`)
/// oriented like `kernel`.
fn oriented_ellipse(c: DVec3, u: DVec3, r1: f64, r2: f64, n: DVec3, kernel: &EdgeCurve) -> Option<EdgeCurve> {
    let (a, b) = (kernel.start(), kernel.end());
    let closed = a.distance(b) <= 1e-9 * (1.0 + r1);
    let mut best: Option<(f64, EdgeCurve)> = None;
    for z in [n, -n] {
        let frame = Frame::new(c, z, u)?;
        let ang = |p: DVec3| {
            let d = p - c;
            (d.dot(frame.y()) / r2).atan2(d.dot(frame.x) / r1)
        };
        let a0 = ang(a);
        let mut sw = (ang(b) - a0).rem_euclid(std::f64::consts::TAU);
        if sw < 1e-12 || closed {
            sw = std::f64::consts::TAU;
        }
        let e = EdgeCurve::Ellipse { frame, r1, r2, a0, sweep: sw };
        let d = orientation_score(&e, kernel, closed);
        if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, e));
        }
    }
    best.map(|(_, e)| e)
}

/// Largest distance of `kernel` (sampled) from `exact`.
fn deviation(kernel: &EdgeCurve, exact: &EdgeCurve) -> f64 {
    kernel.sample(32).iter().map(|p| exact.closest(*p).1.distance(*p)).fold(0.0, f64::max)
}

/// Closed form of an intersection curve of `plane` with surface `src` (where it is a line,
/// circle or ellipse), oriented like the kernel's curve.
fn exact_curve(kernel: &EdgeCurve, src: &Surface, plane: &SectionPlane) -> Option<EdgeCurve> {
    let n = plane.normal;
    let axis_hit = |o: DVec3, z: DVec3| -> Option<DVec3> {
        let dn = z.dot(n);
        (dn.abs() > 1e-12).then(|| o + z * ((plane.point - o).dot(n) / dn))
    };
    match src {
        Surface::Plane { .. } => Some(EdgeCurve::Line { a: kernel.start(), b: kernel.end() }),
        Surface::Cylinder { frame, r } => {
            let c = axis_hit(frame.origin, frame.z);
            let cz = frame.z.dot(n).abs();
            if cz > 1.0 - 1e-12 {
                oriented_arc(c?, *r, n, kernel)
            } else if cz < 1e-12 {
                Some(EdgeCurve::Line { a: kernel.start(), b: kernel.end() })
            } else {
                let minor = frame.z.cross(n).normalize();
                let major = n.cross(minor).normalize();
                oriented_ellipse(c?, major, r / cz, *r, n, kernel)
            }
        }
        Surface::Sphere { frame, r } => {
            let d = (frame.origin - plane.point).dot(n);
            let rr = (r * r - d * d).max(0.0).sqrt();
            oriented_arc(frame.origin - n * d, rr, n, kernel)
        }
        Surface::Cone { frame, r, semi_angle } => {
            if frame.z.dot(n).abs() < 1.0 - 1e-12 {
                return None;
            }
            let c = axis_hit(frame.origin, frame.z)?;
            let h = (c - frame.origin).dot(frame.z);
            oriented_arc(c, r + h * semi_angle.tan(), n, kernel)
        }
        Surface::Torus { frame, major, minor } => {
            if frame.z.dot(n).abs() < 1.0 - 1e-12 {
                return None;
            }
            let c = axis_hit(frame.origin, frame.z)?;
            let h = (c - frame.origin).dot(frame.z);
            let s = (minor * minor - h * h).max(0.0).sqrt();
            let rk = kernel.point(0.5).distance(c);
            let rr = if (rk - (major + s)).abs() < (rk - (major - s)).abs() { major + s } else { major - s };
            oriented_arc(c, rr, n, kernel)
        }
        Surface::Revolution { curve, axis_origin, axis_dir } => {
            // perpendicular to the axis: circles, radius where the generator reaches the plane
            if axis_dir.dot(n).abs() < 1.0 - 1e-12 {
                return None;
            }
            let c = axis_hit(*axis_origin, *axis_dir)?;
            let h0 = (c - *axis_origin).dot(*axis_dir);
            let rh = |t: f64| {
                let d = curve.eval(t) - *axis_origin;
                let h = d.dot(*axis_dir);
                ((d - *axis_dir * h).length(), h - h0)
            };
            let (t0, t1) = curve.domain();
            let rk = kernel.point(0.5).distance(c);
            let mut best: Option<f64> = None;
            const N: usize = 128;
            for i in 0..N {
                let (a, b) = (t0 + (t1 - t0) * i as f64 / N as f64, t0 + (t1 - t0) * (i + 1) as f64 / N as f64);
                let (fa, fb) = (rh(a).1, rh(b).1);
                if fa == 0.0 || fa.signum() != fb.signum() {
                    let (mut lo, mut hi, mut flo) = (a, b, fa);
                    for _ in 0..100 {
                        let m = 0.5 * (lo + hi);
                        let fm = rh(m).1;
                        if fm == 0.0 || fm.signum() == flo.signum() {
                            lo = m;
                            flo = fm;
                        } else {
                            hi = m;
                        }
                    }
                    let r = rh(0.5 * (lo + hi)).0;
                    if best.is_none_or(|x| (x - rk).abs() > (r - rk).abs()) {
                        best = Some(r);
                    }
                }
            }
            oriented_arc(c, best?, n, kernel)
        }
        _ => None,
    }
}

/// Circle through three points: centre, radius.
fn circle3(a: DVec3, b: DVec3, c: DVec3) -> Option<(DVec3, f64)> {
    let (u, v) = (b - a, c - a);
    let w = u.cross(v);
    let w2 = w.length_squared();
    if w2 < 1e-24 * u.length_squared() * v.length_squared() || w2 == 0.0 {
        return None;
    }
    let centre = a + (w.cross(u) * v.length_squared() + v.cross(w) * u.length_squared()) / (2.0 * w2);
    Some((centre, (a - centre).length()))
}

/// Contours of the caps of `cut` (the kept half of one body, context coordinates) in `plane`.
pub fn cut_contours(cut: &Brep, plane: &SectionPlane, body: usize) -> Vec<(usize, Vec<Contour>)> {
    let edges: Vec<EdgeCurve> = cut
        .edges
        .iter()
        .map(|e| EdgeCurve::from_edge(&e.curve, cut.vertices[e.start as usize], cut.vertices[e.end as usize]))
        .collect();
    let faces: Vec<&cad_kernel::brep::Face> = cut.faces().collect();
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for v in &cut.vertices {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    let size = if lo.x.is_finite() { (hi - lo).length().max(1.0) } else { 1.0 };
    let eps = 1e-7 * size;
    let mut edge_faces: Vec<Vec<usize>> = vec![Vec::new(); edges.len()];
    for (i, f) in faces.iter().enumerate() {
        for l in &f.loops {
            if let Loop::Edges(es) = l {
                for oe in es {
                    if let Some(v) = edge_faces.get_mut(oe.edge as usize)
                        && !v.contains(&i)
                    {
                        v.push(i);
                    }
                }
            }
        }
    }
    let is_cap = |f: &cad_kernel::brep::Face| match &f.surface {
        Surface::Plane { frame } => {
            let z = if f.same_sense { frame.z } else { -frame.z };
            z.dot(plane.normal) > 1.0 - 1e-9 && plane.dist(frame.origin).abs() <= eps
        }
        _ => false,
    };
    let frame = plane.frame();
    let mut out = Vec::new();
    for (fi, f) in faces.iter().enumerate() {
        if !is_cap(f) {
            continue;
        }
        let mut contours = Vec::new();
        for l in &f.loops {
            let Loop::Edges(es) = l else { continue };
            let mut segs = Vec::new();
            for oe in es {
                let k = &edges[oe.edge as usize];
                let k = if oe.forward { k.clone() } else { k.reversed() };
                // the cut face at this edge (not a cap)
                let src = edge_faces[oe.edge as usize].iter().map(|&j| faces[j]).find(|g| !is_cap(g));
                let (source, tag) = src.map_or(("Ebene", f.tag), |g| (surface_name(&g.surface), g.tag));
                let seg = if k.is_analytic() {
                    Seg { curve: k, exact: true, dev: 0.0, source, tag }
                } else if let Some(e) = src.and_then(|g| exact_curve(&k, &g.surface, plane))
                    && deviation(&k, &e) <= 1e-3 * (size / 100.0).max(1.0)
                {
                    let dev = deviation(&k, &e);
                    Seg { curve: e, exact: true, dev, source, tag }
                } else {
                    // a circle the kernel's curve follows closely (unknown surface)
                    let third = if k.is_closed() { k.point(0.75) } else { k.point(1.0) };
                    let fitted = circle3(k.point(0.0), k.point(0.5), third)
                        .and_then(|(c, r)| oriented_arc(c, r, plane.normal, &k))
                        .filter(|e| deviation(&k, e) <= 1e-7 * size);
                    match fitted {
                        Some(e) => {
                            let dev = deviation(&k, &e);
                            Seg { curve: e, exact: false, dev, source, tag }
                        }
                        None => Seg { curve: k, exact: false, dev: 0.0, source, tag },
                    }
                };
                segs.push(seg);
            }
            let area: f64 = segs.iter().map(|s| green(&s.curve, frame.origin, frame.x, frame.y())).sum();
            contours.push(Contour { segs, area, body });
        }
        out.push((fi, contours));
    }
    out
}

impl Section {
    /// A section from the contours of all bodies (`cut_contours` per body).
    pub fn new(plane: SectionPlane, parts: Vec<Vec<(usize, Vec<Contour>)>>) -> Section {
        let frame = plane.frame();
        let mut contours = Vec::new();
        let mut regions = Vec::new();
        for faces in parts {
            for (_, cs) in faces {
                let base = contours.len();
                let n = cs.len();
                let body = cs.first().map_or(0, |c| c.body);
                contours.extend(cs);
                // the outline is the loop with the largest signed area
                let outer = (base..base + n).max_by(|&a, &b| {
                    contours[a].area.partial_cmp(&contours[b].area).unwrap_or(std::cmp::Ordering::Equal)
                });
                if let Some(o) = outer {
                    let holes: Vec<usize> = (base..base + n).filter(|&i| i != o).collect();
                    let area = (base..base + n).map(|i| contours[i].area).sum();
                    regions.push(Region { outer: o, holes, area, body });
                }
            }
        }
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        let mut max_dev = 0.0f64;
        let mut exact = true;
        for c in &contours {
            for s in &c.segs {
                let (a, b) = s.curve.bounds();
                lo = lo.min(a);
                hi = hi.max(b);
                if s.exact {
                    max_dev = max_dev.max(s.dev);
                } else {
                    exact = false;
                }
            }
        }
        let size = if lo.x.is_finite() { (hi - lo).length().max(1e-6) } else { 1.0 };
        Section { plane, frame, contours, regions, max_dev, exact, size }
    }

    pub fn is_empty(&self) -> bool {
        self.contours.is_empty()
    }

    /// Plane coordinates of a point.
    pub fn local(&self, p: DVec3) -> (f64, f64) {
        let d = p - self.frame.origin;
        (d.dot(self.frame.x), d.dot(self.frame.y()))
    }

    /// Total area of all regions.
    pub fn area(&self) -> f64 {
        self.regions.iter().map(|r| r.area).sum()
    }

    /// Closest contour point: (contour, segment, s, point, distance).
    pub fn closest(&self, p: DVec3) -> Option<(usize, usize, f64, DVec3, f64)> {
        let mut best: Option<(usize, usize, f64, DVec3, f64)> = None;
        for (ci, c) in self.contours.iter().enumerate() {
            for (si, s) in c.segs.iter().enumerate() {
                let (t, q) = s.curve.closest(p);
                let d = q.distance(p);
                if best.is_none_or(|b| d < b.4) {
                    best = Some((ci, si, t, q, d));
                }
            }
        }
        best
    }

    /// Winding number of a contour around `p` (sampled polyline).
    fn winding(&self, c: usize, p: DVec3) -> i32 {
        let (px, py) = self.local(p);
        let mut w = 0;
        let pts: Vec<(f64, f64)> =
            self.contours[c].segs.iter().flat_map(|s| s.curve.polyline(64)).map(|q| self.local(q)).collect();
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            let cross = (b.0 - a.0) * (py - a.1) - (px - a.0) * (b.1 - a.1);
            if a.1 <= py {
                if b.1 > py && cross > 0.0 {
                    w += 1;
                }
            } else if b.1 <= py && cross < 0.0 {
                w -= 1;
            }
        }
        w
    }

    /// Region containing `p` (projected onto the plane).
    pub fn region_at(&self, p: DVec3) -> Option<usize> {
        let p = self.plane.project(p);
        self.regions
            .iter()
            .position(|r| self.winding(r.outer, p) != 0 && r.holes.iter().all(|&h| self.winding(h, p) == 0))
    }

    /// First crossing of the ray `o + t d` (in the plane, `t > tmin`) with the contours:
    /// (t, contour, segment).
    fn ray_hit(&self, o: DVec3, d: DVec3, tmin: f64) -> Option<(f64, usize, usize)> {
        let (ox, oy) = self.local(o);
        let (dx, dy) = (d.dot(self.frame.x), d.dot(self.frame.y()));
        let mut best: Option<(f64, usize, usize)> = None;
        let mut take = |t: f64, c: usize, s: usize| {
            if t > tmin && best.is_none_or(|b| t < b.0) {
                best = Some((t, c, s));
            }
        };
        for (ci, c) in self.contours.iter().enumerate() {
            for (si, s) in c.segs.iter().enumerate() {
                match &s.curve {
                    EdgeCurve::Line { a, b } => {
                        let (ax, ay) = self.local(*a);
                        let (bx, by) = self.local(*b);
                        let (ex, ey) = (bx - ax, by - ay);
                        let den = dx * ey - dy * ex;
                        if den.abs() < 1e-18 {
                            continue;
                        }
                        let (wx, wy) = (ax - ox, ay - oy);
                        let t = (wx * ey - wy * ex) / den;
                        let u = (wx * dy - wy * dx) / den;
                        if (-1e-12..=1.0 + 1e-12).contains(&u) {
                            take(t, ci, si);
                        }
                    }
                    EdgeCurve::Arc { frame, r, .. } => {
                        let (cx, cy) = self.local(frame.origin);
                        let (fx, fy) = (ox - cx, oy - cy);
                        let bq = fx * dx + fy * dy;
                        let cq = fx * fx + fy * fy - r * r;
                        let disc = bq * bq - cq;
                        if disc < 0.0 {
                            continue;
                        }
                        for t in [-bq - disc.sqrt(), -bq + disc.sqrt()] {
                            let q = o + d * t;
                            if s.curve.closest(q).1.distance(q) <= 1e-9 * (1.0 + r) {
                                take(t, ci, si);
                            }
                        }
                    }
                    other => {
                        let pts = other.sample(256);
                        let side = |q: DVec3| {
                            let (qx, qy) = self.local(q);
                            (qx - ox) * dy - (qy - oy) * dx
                        };
                        for k in 0..pts.len() - 1 {
                            let (s0, s1) = (side(pts[k]), side(pts[k + 1]));
                            if s0 == 0.0 || s0.signum() != s1.signum() {
                                // refine the crossing on the curve
                                let (lo, hi) = (k as f64 / 256.0, (k + 1) as f64 / 256.0);
                                let (sm, _) = minimise(lo, hi, 8, |x| side(other.point(x)).abs());
                                let q = other.point(sm);
                                let t = (q - o).dot(d);
                                take(t, ci, si);
                            }
                        }
                    }
                }
            }
        }
        best
    }

    /// Wall thickness at the contour point closest to `p`: distance across the material,
    /// perpendicular to the contour there.
    pub fn wall(&self, p: DVec3) -> Option<Wall> {
        let (ci, si, s, q, _) = self.closest(p)?;
        self.wall_at(ci, si, s, q)
    }

    fn wall_at(&self, ci: usize, si: usize, s: f64, q: DVec3) -> Option<Wall> {
        let seg = &self.contours[ci].segs[si];
        let t = seg.curve.tangent(s);
        let dir = self.plane.normal.cross(t).try_normalize()?;
        let (th, _, _) = self.ray_hit(q, dir, 1e-9 * self.size.max(1.0))?;
        Some(Wall { thickness: th, from: q, to: q + dir * th, contour: ci, seg: si, dir })
    }

    /// Smallest wall thickness over interior points of every segment (an estimate: `samples`
    /// points per segment).
    pub fn min_wall(&self, samples: usize) -> Option<Wall> {
        let mut best: Option<Wall> = None;
        let n = samples.max(1);
        for (ci, c) in self.contours.iter().enumerate() {
            for (si, sg) in c.segs.iter().enumerate() {
                for k in 0..n {
                    let s = (k as f64 + 0.5) / n as f64;
                    let q = sg.curve.point(s);
                    if let Some(w) = self.wall_at(ci, si, s, q)
                        && best.is_none_or(|b| w.thickness < b.thickness)
                    {
                        best = Some(w);
                    }
                }
            }
        }
        best
    }
}
