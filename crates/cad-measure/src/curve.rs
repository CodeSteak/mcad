//! Exact edge geometry: a B-rep edge as a bounded curve with a normalised parameter
//! `s ∈ [0, 1]` from its start to its end vertex.
//!
//! Lines, circular arcs and ellipse arcs are evaluated in closed form (closest point on lines
//! and arcs too); B-splines and polylines numerically (dense samples plus golden-section
//! search, lengths with Gauss–Legendre panels).

use std::f64::consts::TAU;

use cad_kernel::brep::{BSplineCurve, Curve, Frame};
use glam::DVec3;

use crate::num::{integrate, minimise};

#[derive(Debug, Clone, PartialEq)]
pub enum EdgeCurve {
    Line {
        a: DVec3,
        b: DVec3,
    },
    /// Circular arc around `frame.origin`, counter-clockwise about `frame.z` from angle `a0`
    /// (measured from `frame.x`) by `sweep` (0 < sweep ≤ 2π).
    Arc {
        frame: Frame,
        r: f64,
        a0: f64,
        sweep: f64,
    },
    /// Ellipse arc (`r1` along `frame.x`, `r2` along `frame.y`), parametric angle from `a0` by
    /// `sweep`.
    Ellipse {
        frame: Frame,
        r1: f64,
        r2: f64,
        a0: f64,
        sweep: f64,
    },
    /// B-spline from parameter `t0` to `t1` (`t1 < t0`: traversed backwards).
    Spline {
        c: BSplineCurve,
        t0: f64,
        t1: f64,
    },
    /// Polyline (sampled curves).
    Poly {
        pts: Vec<DVec3>,
    },
}

/// Angle of `p` about a frame (from `x` towards `y`).
fn frame_angle(f: &Frame, p: DVec3) -> f64 {
    let d = p - f.origin;
    d.dot(f.y()).atan2(d.dot(f.x))
}

/// `a1 − a0` reduced to (0, 2π]; equal angles give a full turn.
fn sweep_of(a0: f64, a1: f64) -> f64 {
    let mut s = (a1 - a0).rem_euclid(TAU);
    if s < 1e-12 {
        s = TAU;
    }
    s
}

impl EdgeCurve {
    /// The edge with curve `c` from vertex `a` to vertex `b` (B-rep conventions, see
    /// `cad_kernel::brep`).
    pub fn from_edge(c: &Curve, a: DVec3, b: DVec3) -> EdgeCurve {
        match c {
            Curve::Line { .. } => EdgeCurve::Line { a, b },
            Curve::Circle { frame, r } => {
                let a0 = frame_angle(frame, a);
                let sweep = if a.distance(b) <= 1e-9 * r.max(1.0) { TAU } else { sweep_of(a0, frame_angle(frame, b)) };
                EdgeCurve::Arc { frame: *frame, r: *r, a0, sweep }
            }
            Curve::Ellipse { frame, r1, r2 } => {
                let ang = |p: DVec3| {
                    let d = p - frame.origin;
                    (d.dot(frame.y()) / r2).atan2(d.dot(frame.x) / r1)
                };
                let a0 = ang(a);
                let sweep = if a.distance(b) <= 1e-9 * r1.max(1.0) { TAU } else { sweep_of(a0, ang(b)) };
                EdgeCurve::Ellipse { frame: *frame, r1: *r1, r2: *r2, a0, sweep }
            }
            Curve::BSpline(s) => {
                let (t0, t1) = s.domain();
                EdgeCurve::Spline { c: s.clone(), t0, t1 }
            }
        }
    }

    /// An arc in the plane of `frame` from angle `a0` by the signed `sweep` (negative:
    /// clockwise about `frame.z`).
    pub fn arc(frame: Frame, r: f64, a0: f64, sweep: f64) -> EdgeCurve {
        if sweep >= 0.0 {
            EdgeCurve::Arc { frame, r, a0, sweep }
        } else {
            let f = Frame { origin: frame.origin, z: -frame.z, x: frame.x };
            EdgeCurve::Arc { frame: f, r, a0: -a0, sweep: -sweep }
        }
    }

    /// The same point set traversed from the end to the start.
    pub fn reversed(&self) -> EdgeCurve {
        match self {
            EdgeCurve::Line { a, b } => EdgeCurve::Line { a: *b, b: *a },
            EdgeCurve::Arc { frame, r, a0, sweep } => {
                let f = Frame { origin: frame.origin, z: -frame.z, x: frame.x };
                EdgeCurve::Arc { frame: f, r: *r, a0: -(a0 + sweep), sweep: *sweep }
            }
            EdgeCurve::Ellipse { frame, r1, r2, a0, sweep } => {
                let f = Frame { origin: frame.origin, z: -frame.z, x: frame.x };
                EdgeCurve::Ellipse { frame: f, r1: *r1, r2: *r2, a0: -(a0 + sweep), sweep: *sweep }
            }
            EdgeCurve::Spline { c, t0, t1 } => EdgeCurve::Spline { c: c.clone(), t0: *t1, t1: *t0 },
            EdgeCurve::Poly { pts } => EdgeCurve::Poly { pts: pts.iter().rev().copied().collect() },
        }
    }

    /// `line`, `arc`, `circle` (full turn), `ellipse`, `spline`, `polyline`.
    pub fn kind(&self) -> &'static str {
        match self {
            EdgeCurve::Line { .. } => "line",
            EdgeCurve::Arc { sweep, .. } if *sweep >= TAU - 1e-9 => "circle",
            EdgeCurve::Arc { .. } => "arc",
            EdgeCurve::Ellipse { .. } => "ellipse",
            EdgeCurve::Spline { .. } => "spline",
            EdgeCurve::Poly { .. } => "polyline",
        }
    }

    /// Closed-form geometry (line, circle, ellipse).
    pub fn is_analytic(&self) -> bool {
        matches!(self, EdgeCurve::Line { .. } | EdgeCurve::Arc { .. } | EdgeCurve::Ellipse { .. })
    }

    pub fn is_closed(&self) -> bool {
        self.start().distance(self.end()) <= 1e-9 * (1.0 + self.start().length())
    }

    pub fn point(&self, s: f64) -> DVec3 {
        match self {
            EdgeCurve::Line { a, b } => a.lerp(*b, s),
            EdgeCurve::Arc { frame, r, a0, sweep } => {
                let t = a0 + sweep * s;
                frame.origin + (frame.x * t.cos() + frame.y() * t.sin()) * *r
            }
            EdgeCurve::Ellipse { frame, r1, r2, a0, sweep } => {
                let t = a0 + sweep * s;
                frame.origin + frame.x * (r1 * t.cos()) + frame.y() * (r2 * t.sin())
            }
            EdgeCurve::Spline { c, t0, t1 } => c.eval(t0 + (t1 - t0) * s.clamp(0.0, 1.0)),
            EdgeCurve::Poly { pts } => {
                if pts.len() < 2 {
                    return pts.first().copied().unwrap_or(DVec3::ZERO);
                }
                let f = s.clamp(0.0, 1.0) * (pts.len() - 1) as f64;
                let i = (f.floor() as usize).min(pts.len() - 2);
                pts[i].lerp(pts[i + 1], f - i as f64)
            }
        }
    }

    /// d point / ds.
    pub fn deriv(&self, s: f64) -> DVec3 {
        match self {
            EdgeCurve::Line { a, b } => *b - *a,
            EdgeCurve::Arc { frame, r, a0, sweep } => {
                let t = a0 + sweep * s;
                (frame.y() * t.cos() - frame.x * t.sin()) * (r * sweep)
            }
            EdgeCurve::Ellipse { frame, r1, r2, a0, sweep } => {
                let t = a0 + sweep * s;
                (frame.y() * (r2 * t.cos()) - frame.x * (r1 * t.sin())) * *sweep
            }
            EdgeCurve::Spline { c, t0, t1 } => c.tangent(t0 + (t1 - t0) * s.clamp(0.0, 1.0)) * (t1 - t0),
            EdgeCurve::Poly { pts } => {
                if pts.len() < 2 {
                    return DVec3::ZERO;
                }
                let n = (pts.len() - 1) as f64;
                let i = ((s.clamp(0.0, 1.0) * n).floor() as usize).min(pts.len() - 2);
                (pts[i + 1] - pts[i]) * n
            }
        }
    }

    pub fn start(&self) -> DVec3 {
        self.point(0.0)
    }

    pub fn end(&self) -> DVec3 {
        self.point(1.0)
    }

    /// Unit tangent at `s` (direction of traversal).
    pub fn tangent(&self, s: f64) -> DVec3 {
        let d = self.deriv(s);
        d.try_normalize()
            .unwrap_or_else(|| (self.point((s + 1e-6).min(1.0)) - self.point((s - 1e-6).max(0.0))).normalize_or_zero())
    }

    /// Arc length (closed form for lines and arcs, else Gauss–Legendre).
    pub fn length(&self) -> f64 {
        match self {
            EdgeCurve::Line { a, b } => a.distance(*b),
            EdgeCurve::Arc { r, sweep, .. } => r * sweep,
            EdgeCurve::Poly { pts } => pts.windows(2).map(|w| w[0].distance(w[1])).sum(),
            EdgeCurve::Spline { c, .. } => {
                let pieces = (c.knots.len().saturating_sub(2 * c.degree as usize + 1)).max(1) * 6;
                integrate(0.0, 1.0, pieces.min(512), |s| self.deriv(s).length())
            }
            EdgeCurve::Ellipse { .. } => integrate(0.0, 1.0, 64, |s| self.deriv(s).length()),
        }
    }

    /// Length from the start to `s`.
    pub fn length_to(&self, s: f64) -> f64 {
        match self {
            EdgeCurve::Line { a, b } => a.distance(*b) * s,
            EdgeCurve::Arc { r, sweep, .. } => r * sweep * s,
            _ => integrate(0.0, s, 32, |x| self.deriv(x).length()),
        }
    }

    /// Parameter at half the arc length.
    pub fn mid_param(&self) -> f64 {
        match self {
            EdgeCurve::Line { .. } | EdgeCurve::Arc { .. } => 0.5,
            _ => {
                let half = 0.5 * self.length();
                let (mut lo, mut hi) = (0.0, 1.0);
                for _ in 0..60 {
                    let m = 0.5 * (lo + hi);
                    if self.length_to(m) < half {
                        lo = m;
                    } else {
                        hi = m;
                    }
                }
                0.5 * (lo + hi)
            }
        }
    }

    /// Point at half the arc length.
    pub fn mid(&self) -> DVec3 {
        self.point(self.mid_param())
    }

    /// `n + 1` points from the start to the end.
    pub fn sample(&self, n: usize) -> Vec<DVec3> {
        if let EdgeCurve::Poly { pts } = self {
            return pts.clone();
        }
        let n = n.max(1);
        (0..=n).map(|i| self.point(i as f64 / n as f64)).collect()
    }

    /// Samples for display and coarse searches: lines 1 segment, arcs about every 6°, other
    /// curves `n` segments.
    pub fn polyline(&self, n: usize) -> Vec<DVec3> {
        match self {
            EdgeCurve::Line { a, b } => vec![*a, *b],
            EdgeCurve::Arc { sweep, .. } | EdgeCurve::Ellipse { sweep, .. } => {
                self.sample(((sweep / TAU) * 64.0).ceil().max(4.0) as usize)
            }
            EdgeCurve::Poly { pts } => pts.clone(),
            EdgeCurve::Spline { .. } => self.sample(n),
        }
    }

    /// Closest point to `p`: `(s, point)`.
    pub fn closest(&self, p: DVec3) -> (f64, DVec3) {
        match self {
            EdgeCurve::Line { a, b } => {
                let d = *b - *a;
                let l2 = d.length_squared();
                let s = if l2 > 0.0 { ((p - *a).dot(d) / l2).clamp(0.0, 1.0) } else { 0.0 };
                (s, a.lerp(*b, s))
            }
            EdgeCurve::Arc { frame, a0, sweep, .. } => {
                let d = p - frame.origin;
                let inplane = d - frame.z * d.dot(frame.z);
                if inplane.length() < 1e-12 {
                    return (0.0, self.start());
                }
                let rel = (frame_angle(frame, p) - a0).rem_euclid(TAU);
                if rel <= *sweep + 1e-14 {
                    let s = (rel / sweep).min(1.0);
                    return (s, self.point(s));
                }
                let (pa, pb) = (self.start(), self.end());
                if pa.distance_squared(p) <= pb.distance_squared(p) { (0.0, pa) } else { (1.0, pb) }
            }
            EdgeCurve::Poly { pts } => {
                let mut best = (0.0, pts.first().copied().unwrap_or(DVec3::ZERO), f64::INFINITY);
                let n = pts.len().saturating_sub(1).max(1) as f64;
                for (i, w) in pts.windows(2).enumerate() {
                    let d = w[1] - w[0];
                    let l2 = d.length_squared();
                    let t = if l2 > 0.0 { ((p - w[0]).dot(d) / l2).clamp(0.0, 1.0) } else { 0.0 };
                    let q = w[0] + d * t;
                    let dd = q.distance_squared(p);
                    if dd < best.2 {
                        best = ((i as f64 + t) / n, q, dd);
                    }
                }
                (best.0, best.1)
            }
            _ => {
                let (s, _) = minimise(0.0, 1.0, 128, |s| self.point(s).distance_squared(p));
                (s, self.point(s))
            }
        }
    }

    /// Point of the curve closest to the line `o + t d` (`d` unit): `(s, point, distance from
    /// the line, depth t)`.
    pub fn closest_to_ray(&self, o: DVec3, d: DVec3) -> (f64, DVec3, f64, f64) {
        let dist = |q: DVec3| {
            let v = q - o;
            (v - d * v.dot(d)).length()
        };
        let s = match self {
            EdgeCurve::Line { a, b } => {
                // closest points of the segment and the line
                let u = *b - *a;
                let w = *a - o;
                let (aa, bb, cc, dd, ee) = (u.dot(u), u.dot(d), d.dot(d), u.dot(w), d.dot(w));
                let den = aa * cc - bb * bb;
                if den.abs() < 1e-18 { 0.0 } else { ((bb * ee - cc * dd) / den).clamp(0.0, 1.0) }
            }
            _ => minimise(0.0, 1.0, 96, |s| dist(self.point(s))).0,
        };
        let q = self.point(s);
        (s, q, dist(q), (q - o).dot(d))
    }

    /// Circle data of an arc: centre, unit axis, radius, sweep.
    pub fn circle(&self) -> Option<(DVec3, DVec3, f64, f64)> {
        match self {
            EdgeCurve::Arc { frame, r, sweep, .. } => Some((frame.origin, frame.z, *r, *sweep)),
            _ => None,
        }
    }

    /// Direction of a straight edge (start → end).
    pub fn line_dir(&self) -> Option<DVec3> {
        match self {
            EdgeCurve::Line { a, b } => (*b - *a).try_normalize(),
            _ => None,
        }
    }

    /// Normal of the curve's plane if it is planar (lines: none).
    pub fn plane_normal(&self) -> Option<DVec3> {
        match self {
            EdgeCurve::Arc { frame, .. } | EdgeCurve::Ellipse { frame, .. } => Some(frame.z),
            EdgeCurve::Line { .. } => None,
            _ => {
                let pts = self.sample(16);
                let c = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
                let mut best = DVec3::ZERO;
                for i in 0..pts.len() {
                    for j in i + 1..pts.len() {
                        let n = (pts[i] - c).cross(pts[j] - c);
                        if n.length_squared() > best.length_squared() {
                            best = n;
                        }
                    }
                }
                let n = best.try_normalize()?;
                let flat = pts.iter().all(|p| (*p - c).dot(n).abs() <= 1e-7 * (1.0 + self.length()));
                flat.then_some(n)
            }
        }
    }

    /// Axis-aligned bounds (arcs: exact extreme points).
    pub fn bounds(&self) -> (DVec3, DVec3) {
        let mut lo = self.start().min(self.end());
        let mut hi = self.start().max(self.end());
        match self {
            EdgeCurve::Line { .. } => {}
            EdgeCurve::Arc { frame, r, a0, sweep } => {
                // extremes along each world axis: angle where the tangent is perpendicular to it
                for ax in [DVec3::X, DVec3::Y, DVec3::Z] {
                    let (cx, cy) = (frame.x.dot(ax), frame.y().dot(ax));
                    if cx.hypot(cy) < 1e-15 {
                        continue;
                    }
                    let t = cy.atan2(cx);
                    for tt in [t, t + std::f64::consts::PI] {
                        let rel = (tt - a0).rem_euclid(TAU);
                        if rel <= *sweep {
                            let q = frame.origin + (frame.x * tt.cos() + frame.y() * tt.sin()) * *r;
                            lo = lo.min(q);
                            hi = hi.max(q);
                        }
                    }
                }
            }
            _ => {
                for q in self.sample(256) {
                    lo = lo.min(q);
                    hi = hi.max(q);
                }
            }
        }
        (lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xy() -> Frame {
        Frame { origin: DVec3::new(1.0, 2.0, 3.0), z: DVec3::Z, x: DVec3::X }
    }

    #[test]
    fn arc_basics() {
        let c = Curve::Circle { frame: xy(), r: 5.0 };
        let a = DVec3::new(6.0, 2.0, 3.0);
        let b = DVec3::new(1.0, 7.0, 3.0);
        let e = EdgeCurve::from_edge(&c, a, b);
        assert_eq!(e.kind(), "arc");
        assert!((e.length() - 5.0 * std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!(e.start().distance(a) < 1e-12 && e.end().distance(b) < 1e-12);
        let m = e.mid();
        let h = std::f64::consts::FRAC_1_SQRT_2 * 5.0;
        assert!(m.distance(DVec3::new(1.0 + h, 2.0 + h, 3.0)) < 1e-12);
        // closest point inside and outside the arc
        let (s, q) = e.closest(DVec3::new(11.0, 12.0, 10.0));
        assert!((s - 0.5).abs() < 1e-12 && q.distance(m) < 1e-12);
        let (s, _) = e.closest(DVec3::new(-10.0, -1.0, 3.0));
        assert!(s == 0.0 || s == 1.0);
        let r = e.reversed();
        assert!(r.start().distance(b) < 1e-12 && r.end().distance(a) < 1e-12);
        assert!((r.length() - e.length()).abs() < 1e-12);
        assert!(r.mid().distance(m) < 1e-12);
        // full circle
        let f = EdgeCurve::from_edge(&c, a, a);
        assert_eq!(f.kind(), "circle");
        assert!((f.length() - 10.0 * std::f64::consts::PI).abs() < 1e-12);
        let (lo, hi) = f.bounds();
        assert!(lo.distance(DVec3::new(-4.0, -3.0, 3.0)) < 1e-12 && hi.distance(DVec3::new(6.0, 7.0, 3.0)) < 1e-12);
        // signed construction
        let cw = EdgeCurve::arc(xy(), 2.0, 0.0, -std::f64::consts::FRAC_PI_2);
        assert!(cw.end().distance(DVec3::new(1.0, 0.0, 3.0)) < 1e-12, "{:?}", cw.end());
    }

    #[test]
    fn ellipse_length() {
        let c = Curve::Ellipse { frame: xy(), r1: 3.0, r2: 3.0 };
        let a = DVec3::new(4.0, 2.0, 3.0);
        let e = EdgeCurve::from_edge(&c, a, a);
        assert!((e.length() - 6.0 * std::f64::consts::PI).abs() < 1e-10, "{}", e.length());
        // ellipse 2 x 1: perimeter (Ramanujan II, ~1e-10 here)
        let c = Curve::Ellipse { frame: xy(), r1: 2.0, r2: 1.0 };
        let e = EdgeCurve::from_edge(&c, DVec3::new(3.0, 2.0, 3.0), DVec3::new(3.0, 2.0, 3.0));
        let (a, b) = (2.0f64, 1.0f64);
        let h = ((a - b) / (a + b)).powi(2);
        let p = std::f64::consts::PI * (a + b) * (1.0 + 3.0 * h / (10.0 + (4.0 - 3.0 * h).sqrt()));
        assert!((e.length() - p).abs() < 1e-5, "{} {}", e.length(), p);
    }

    #[test]
    fn line_ray() {
        let e = EdgeCurve::Line { a: DVec3::ZERO, b: DVec3::new(10.0, 0.0, 0.0) };
        let (s, q, d, t) = e.closest_to_ray(DVec3::new(4.0, -5.0, 1.0), DVec3::Y);
        assert!((s - 0.4).abs() < 1e-12 && q.distance(DVec3::new(4.0, 0.0, 0.0)) < 1e-12);
        assert!((d - 1.0).abs() < 1e-12 && (t - 5.0).abs() < 1e-12);
    }
}
