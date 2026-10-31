//! Edge discretisation (plan 2.1): every edge once, as a polyline from its start vertex to its
//! end vertex, shared by the faces on both sides (so their triangles meet: watertight).
//!
//! | Curve | Points |
//! | --- | --- |
//! | line | the two vertices |
//! | circle | uniform in the angle, step `θ = min(2·acos(1 − tol/r), angle)` |
//! | ellipse, B-spline | adaptive bisection from the knot spans: a piece is split while a point at ¼, ½ or ¾ of it is more than `tol` from its chord, or the tangent turns more than `angle` over it |

use std::f64::consts::TAU;

use cad_kernel::brep::{Brep, Curve};
use glam::DVec3;

use crate::bspline;

/// Points of all edges. Point ids: the B-rep's vertices first (same index), then the edges'
/// interior points.
#[derive(Debug, Clone, Default)]
pub(crate) struct EdgePolys {
    pub points: Vec<DVec3>,
    /// Per edge the point ids from its start vertex to its end vertex.
    pub edges: Vec<Vec<u32>>,
}

/// Largest angle step whose chord stays within `tol` of a circle of radius `r`, at most `angle`.
pub(crate) fn step_angle(r: f64, tol: f64, angle: f64) -> f64 {
    if !crate::gt(r, tol) {
        return angle.min(std::f64::consts::FRAC_PI_2);
    }
    (2.0 * (1.0 - tol / r).acos()).min(angle)
}

/// Number of steps of at most `step` over `span`.
pub(crate) fn steps(span: f64, step: f64) -> usize {
    if !crate::gt(span, 0.0) || !crate::gt(step, 0.0) {
        return 1;
    }
    ((span / step - 1e-9).ceil() as usize).clamp(1, 1 << 16)
}

pub(crate) fn discretise(brep: &Brep, tol: f64, angle: f64) -> EdgePolys {
    let mut out = EdgePolys { points: brep.vertices.clone(), edges: Vec::with_capacity(brep.edges.len()) };
    for e in &brep.edges {
        let (a, b) = (brep.vertices[e.start as usize], brep.vertices[e.end as usize]);
        let inner: Vec<DVec3> = match &e.curve {
            Curve::Line { .. } => Vec::new(),
            Curve::Circle { frame, r } => {
                let (x, y) = (frame.x, frame.y());
                let ang = |p: DVec3| {
                    let d = p - frame.origin;
                    d.dot(y).atan2(d.dot(x))
                };
                let a0 = ang(a);
                let mut a1 = ang(b);
                while a1 <= a0 + 1e-12 {
                    a1 += TAU;
                }
                let n = steps(a1 - a0, step_angle(*r, tol, angle)).max(if e.start == e.end { 3 } else { 1 });
                (1..n)
                    .map(|i| {
                        let t = a0 + (a1 - a0) * i as f64 / n as f64;
                        frame.origin + (x * t.cos() + y * t.sin()) * *r
                    })
                    .collect()
            }
            Curve::Ellipse { frame, r1, r2 } => {
                let (x, y) = (frame.x, frame.y());
                let ang = |p: DVec3| {
                    let d = p - frame.origin;
                    (d.dot(y) / r2).atan2(d.dot(x) / r1)
                };
                let a0 = ang(a);
                let mut a1 = ang(b);
                while a1 <= a0 + 1e-12 {
                    a1 += TAU;
                }
                let f = |t: f64| {
                    let (s, c) = t.sin_cos();
                    (frame.origin + x * (c * r1) + y * (s * r2), y * (c * r2) - x * (s * r1))
                };
                let n = steps(a1 - a0, std::f64::consts::FRAC_PI_4);
                let breaks: Vec<f64> = (0..=n).map(|i| a0 + (a1 - a0) * i as f64 / n as f64).collect();
                adaptive(&f, &breaks, tol, angle).into_iter().map(|t| f(t).0).collect()
            }
            Curve::BSpline(c) => {
                let cv = bspline::Curve::new(c);
                let f = |t: f64| {
                    let d = cv.eval_d(t);
                    (d[0], d[1])
                };
                // each knot span in `degree` pieces at first (an S within a span)
                let br = cv.breaks();
                let mut breaks = vec![br[0]];
                for w in br.windows(2) {
                    let k = cv.degree.max(1);
                    for i in 1..=k {
                        breaks.push(w[0] + (w[1] - w[0]) * i as f64 / k as f64);
                    }
                }
                adaptive(&f, &breaks, tol, angle).into_iter().map(|t| f(t).0).collect()
            }
        };
        let mut ids = Vec::with_capacity(inner.len() + 2);
        ids.push(e.start);
        for p in inner {
            ids.push(out.points.len() as u32);
            out.points.push(p);
        }
        ids.push(e.end);
        out.edges.push(ids);
    }
    out
}

/// Interior parameters (increasing, ends excluded) of an adaptive polyline of `f` (point,
/// tangent) over `breaks` (increasing, at least two values): bisection at half the tolerance
/// from every break, then the points thinned out again as long as the chord stays within `tol`
/// of the dense ones and the tangent turns less than `angle` (dense knots need no points).
fn adaptive(f: &dyn Fn(f64) -> (DVec3, DVec3), breaks: &[f64], tol: f64, angle: f64) -> Vec<f64> {
    let mut ts = vec![breaks[0]];
    let cos_max = angle.cos();
    let mut prev = f(breaks[0]);
    for w in breaks.windows(2) {
        let end = f(w[1]);
        split(f, w[0], w[1], prev, end, 0.5 * tol, cos_max, 0, &mut ts);
        ts.push(w[1]);
        prev = end;
    }
    let pts: Vec<(DVec3, DVec3)> = ts.iter().map(|t| f(*t)).collect();
    let dist = |a: DVec3, b: DVec3, p: DVec3| {
        let ab = b - a;
        let l2 = ab.length_squared();
        let s = if l2 > 0.0 { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
        (a + ab * s - p).length()
    };
    let turned = |a: DVec3, b: DVec3| match (a.try_normalize(), b.try_normalize()) {
        (Some(x), Some(y)) => x.dot(y) < cos_max,
        _ => false,
    };
    let mut out = Vec::new();
    let mut a = 0;
    while a + 1 < ts.len() {
        let mut j = a + 1;
        while j + 1 < ts.len() {
            let c = j + 1;
            let ok = !turned(pts[a].1, pts[c].1) && (a + 1..c).all(|k| dist(pts[a].0, pts[c].0, pts[k].0) <= 0.5 * tol);
            if !ok {
                break;
            }
            j = c;
        }
        if j + 1 < ts.len() {
            out.push(ts[j]);
        }
        a = j;
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn split(
    f: &dyn Fn(f64) -> (DVec3, DVec3),
    t0: f64,
    t1: f64,
    a: (DVec3, DVec3),
    b: (DVec3, DVec3),
    tol: f64,
    cos_max: f64,
    depth: u32,
    out: &mut Vec<f64>,
) {
    if depth >= 24 || (t1 - t0) <= 1e-14 * (t0.abs() + t1.abs()).max(1e-300) {
        return;
    }
    let tm = 0.5 * (t0 + t1);
    let m = f(tm);
    let chord = b.0 - a.0;
    let dist = |p: DVec3| {
        let l2 = chord.length_squared();
        if l2 <= 0.0 {
            return (p - a.0).length();
        }
        let s = ((p - a.0).dot(chord) / l2).clamp(0.0, 1.0);
        (a.0 + chord * s - p).length()
    };
    let mut dev = dist(m.0);
    if dev <= tol {
        for q in [0.25, 0.75] {
            dev = dev.max(dist(f(t0 + (t1 - t0) * q).0));
        }
    }
    let turn = match (a.1.try_normalize(), b.1.try_normalize()) {
        (Some(x), Some(y)) => x.dot(y) < cos_max,
        _ => false,
    };
    if dev > tol || turn {
        split(f, t0, tm, a, m, tol, cos_max, depth + 1, out);
        out.push(tm);
        split(f, tm, t1, m, b, tol, cos_max, depth + 1, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_kernel::brep::{BSplineCurve, Edge, Frame};

    fn brep(curve: Curve, a: DVec3, b: DVec3) -> Brep {
        let closed = (a - b).length() == 0.0;
        Brep {
            vertices: if closed { vec![a] } else { vec![a, b] },
            edges: vec![Edge { start: 0, end: if closed { 0 } else { 1 }, curve, approximated: false }],
            bodies: Vec::new(),
        }
    }

    fn max_dev(pts: &[DVec3], f: impl Fn(DVec3) -> f64) -> f64 {
        let mut worst: f64 = 0.0;
        for w in pts.windows(2) {
            for k in 1..8 {
                worst = worst.max(f(w[0].lerp(w[1], k as f64 / 8.0)));
            }
        }
        worst
    }

    #[test]
    fn circle_steps_follow_the_tolerance() {
        let f = Frame::new(DVec3::ZERO, DVec3::Z, DVec3::X).unwrap();
        let a = DVec3::new(10.0, 0.0, 0.0);
        for tol in [0.1, 0.01, 0.001] {
            let b = brep(Curve::Circle { frame: f, r: 10.0 }, a, a);
            let e = discretise(&b, tol, 0.35);
            let ids = &e.edges[0];
            assert_eq!((ids[0], *ids.last().unwrap()), (0, 0));
            let pts: Vec<DVec3> = ids.iter().map(|i| e.points[*i as usize]).collect();
            let dev = max_dev(&pts, |p| 10.0 - p.length());
            assert!(dev <= tol * 1.0001, "{tol}: {dev}");
            // not much finer than needed
            let n = ids.len() - 1;
            let want = (TAU / (2.0 * (1.0 - tol / 10.0f64).acos())).ceil() as usize;
            assert!(n == want.max(18), "{tol}: {n} segments, {want}");
        }
        // a half circle from a to -a, counter-clockwise about z: through +y
        let b = brep(Curve::Circle { frame: f, r: 10.0 }, a, -a);
        let e = discretise(&b, 0.01, 0.35);
        assert!(e.points[e.edges[0][e.edges[0].len() / 2] as usize].y > 9.9);
    }

    #[test]
    fn bspline_and_ellipse_within_tolerance() {
        let c = BSplineCurve {
            degree: 3,
            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 2.0, 2.0],
            points: vec![
                DVec3::ZERO,
                DVec3::new(10.0, 20.0, 0.0),
                DVec3::new(20.0, -20.0, 0.0),
                DVec3::new(30.0, 20.0, 5.0),
                DVec3::new(40.0, 0.0, 0.0),
            ],
            weights: None,
        };
        let (a, b) = (c.eval(0.0), c.eval(2.0));
        let br = brep(Curve::BSpline(c.clone()), a, b);
        let e = discretise(&br, 0.01, 0.35);
        let pts: Vec<DVec3> = e.edges[0].iter().map(|i| e.points[*i as usize]).collect();
        // deviation: distance of dense curve samples from the polyline
        let dense = c.eval_many(&(0..=2000).map(|i| i as f64 / 1000.0).collect::<Vec<_>>());
        let to_poly = |p: DVec3| {
            pts.windows(2)
                .map(|w| {
                    let ab = w[1] - w[0];
                    let s = ((p - w[0]).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                    (w[0] + ab * s - p).length()
                })
                .fold(f64::INFINITY, f64::min)
        };
        let worst = dense.iter().map(|p| to_poly(*p)).fold(0.0, f64::max);
        assert!(worst <= 0.0101, "{worst}");
        let f = Frame::new(DVec3::ZERO, DVec3::Z, DVec3::X).unwrap();
        let (r1, r2) = (20.0, 5.0);
        let a = DVec3::new(20.0, 0.0, 0.0);
        let br = brep(Curve::Ellipse { frame: f, r1, r2 }, a, a);
        let e = discretise(&br, 0.01, 0.35);
        let pts: Vec<DVec3> = e.edges[0].iter().map(|i| e.points[*i as usize]).collect();
        for p in &pts {
            assert!(((p.x / r1).powi(2) + (p.y / r2).powi(2) - 1.0).abs() < 1e-9);
        }
        let dense: Vec<DVec3> = (0..=4000)
            .map(|i| {
                let t = TAU * i as f64 / 4000.0;
                DVec3::new(r1 * t.cos(), r2 * t.sin(), 0.0)
            })
            .collect();
        let to_poly = |p: DVec3| {
            pts.windows(2)
                .map(|w| {
                    let ab = w[1] - w[0];
                    let s = ((p - w[0]).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                    (w[0] + ab * s - p).length()
                })
                .fold(f64::INFINITY, f64::min)
        };
        let worst = dense.iter().map(|p| to_poly(*p)).fold(0.0, f64::max);
        assert!(worst <= 0.0101, "{worst}");
    }
}
