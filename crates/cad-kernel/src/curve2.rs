//! Planar curve segments of composite profile loops (sketches, Plan 17.1).
//!
//! A [`Segment`] is a line, a circular arc or a (rational) B-spline in the xy-plane of its
//! profile system. Lines and arcs stay exact through the kernel (STEP writes lines and circles);
//! Bézier curves, interpolating splines and arcs under non-uniform maps are B-splines.
//! B-spline segments use [`BSplineCurve`] with `z = 0`.
//!
//! The helpers here are shared by the engine (display polylines, validation) and the kernels:
//! conversion to NURBS, affine maps, sampling, global interpolation and joining several
//! segments into one closed B-spline (lofts between sections with different segment counts).

use glam::{DAffine2, DVec2, DVec3};
use serde::{Deserialize, Serialize};

use crate::brep::BSplineCurve;

const EPS: f64 = 1e-12;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Segment {
    Line {
        a: DVec2,
        b: DVec2,
    },
    /// Circular arc around `center` from angle `a0` to `a1` (radians): `a1 > a0` runs
    /// counter-clockwise, `a1 < a0` clockwise; `|a1 - a0| <= 2π`.
    Arc {
        center: DVec2,
        r: f64,
        a0: f64,
        a1: f64,
    },
    /// (Rational) B-spline in the xy-plane (`z = 0`).
    Nurbs(BSplineCurve),
}

fn d3(p: DVec2) -> DVec3 {
    p.extend(0.0)
}

fn d2(p: DVec3) -> DVec2 {
    p.truncate()
}

fn polar(c: DVec2, r: f64, a: f64) -> DVec2 {
    c + DVec2::new(a.cos(), a.sin()) * r
}

impl Segment {
    pub fn start(&self) -> DVec2 {
        match self {
            Segment::Line { a, .. } => *a,
            Segment::Arc { center, r, a0, .. } => polar(*center, *r, *a0),
            Segment::Nurbs(c) => d2(c.eval(c.domain().0)),
        }
    }

    pub fn end(&self) -> DVec2 {
        match self {
            Segment::Line { b, .. } => *b,
            Segment::Arc { center, r, a1, .. } => polar(*center, *r, *a1),
            Segment::Nurbs(c) => d2(c.eval(c.domain().1)),
        }
    }

    /// Unit tangent at the start, in the direction of travel.
    pub fn start_tangent(&self) -> DVec2 {
        match self {
            Segment::Line { a, b } => (*b - *a).normalize_or_zero(),
            Segment::Arc { a0, a1, .. } => {
                let s = if a1 >= a0 { 1.0 } else { -1.0 };
                DVec2::new(-a0.sin(), a0.cos()) * s
            }
            Segment::Nurbs(c) => nurbs_end_tangent(c, false),
        }
    }

    /// Unit tangent at the end, in the direction of travel.
    pub fn end_tangent(&self) -> DVec2 {
        match self {
            Segment::Line { a, b } => (*b - *a).normalize_or_zero(),
            Segment::Arc { a0, a1, .. } => {
                let s = if a1 >= a0 { 1.0 } else { -1.0 };
                DVec2::new(-a1.sin(), a1.cos()) * s
            }
            Segment::Nurbs(c) => nurbs_end_tangent(c, true),
        }
    }

    /// The same points traversed backwards.
    pub fn reversed(&self) -> Segment {
        match self {
            Segment::Line { a, b } => Segment::Line { a: *b, b: *a },
            Segment::Arc { center, r, a0, a1 } => Segment::Arc { center: *center, r: *r, a0: *a1, a1: *a0 },
            Segment::Nurbs(c) => Segment::Nurbs(c.reversed()),
        }
    }

    /// Image under an affine map. Arcs stay arcs under similarities (mirrored ones reverse
    /// their direction), otherwise they become rational B-splines (exact ellipse arcs).
    pub fn map(&self, m: &DAffine2) -> Segment {
        match self {
            Segment::Line { a, b } => Segment::Line { a: m.transform_point2(*a), b: m.transform_point2(*b) },
            Segment::Arc { center, r, a0, a1 } => {
                let c0 = m.matrix2.x_axis;
                let c1 = m.matrix2.y_axis;
                let (l0, l1) = (c0.length(), c1.length());
                let similar = (l0 - l1).abs() <= 1e-9 * l0.max(1.0) && c0.dot(c1).abs() <= 1e-9 * (l0 * l1).max(1.0);
                if similar && l0 > EPS {
                    let rot = c0.y.atan2(c0.x);
                    let det = m.matrix2.determinant();
                    let (b0, b1) = if det >= 0.0 { (a0 + rot, a1 + rot) } else { (rot - a0, rot - a1) };
                    Segment::Arc { center: m.transform_point2(*center), r: r * l0, a0: b0, a1: b1 }
                } else {
                    Segment::Nurbs(map_nurbs(&arc_nurbs(*center, *r, *a0, *a1), m))
                }
            }
            Segment::Nurbs(c) => Segment::Nurbs(map_nurbs(c, m)),
        }
    }

    /// Exact NURBS form (lines of degree 1, arcs rational quadratic).
    pub fn to_nurbs(&self) -> BSplineCurve {
        match self {
            Segment::Line { a, b } => {
                BSplineCurve { degree: 1, knots: vec![0.0, 0.0, 1.0, 1.0], points: vec![d3(*a), d3(*b)], weights: None }
            }
            Segment::Arc { center, r, a0, a1 } => arc_nurbs(*center, *r, *a0, *a1),
            Segment::Nurbs(c) => c.clone(),
        }
    }

    /// Points along the segment from start to end (both included) with chordal deviation
    /// about `tol`.
    pub fn sample(&self, tol: f64) -> Vec<DVec2> {
        let tol = tol.max(1e-6);
        match self {
            Segment::Line { a, b } => vec![*a, *b],
            Segment::Arc { center, r, a0, a1 } => {
                let sweep = (a1 - a0).abs();
                let c = (1.0 - tol / r.max(1e-9)).clamp(-1.0, 1.0);
                let step = (2.0 * c.acos()).max(1e-3);
                let n = ((sweep / step).ceil() as usize).clamp(2, 512);
                (0..=n).map(|i| polar(*center, *r, a0 + (a1 - a0) * i as f64 / n as f64)).collect()
            }
            Segment::Nurbs(c) => {
                let p = c.degree as usize;
                let mut ts = Vec::new();
                let knots: Vec<f64> = {
                    let (lo, hi) = c.domain();
                    let mut k: Vec<f64> = c.knots.iter().copied().filter(|k| *k >= lo && *k <= hi).collect();
                    k.dedup_by(|a, b| (*a - *b).abs() <= EPS * (hi - lo).max(1.0));
                    k
                };
                // control polygon length per span as the size measure
                let poly: f64 = c.points.windows(2).map(|w| w[0].distance(w[1])).sum();
                let len = (poly / (knots.len().max(2) - 1) as f64).max(1e-9);
                for w in knots.windows(2) {
                    let k = if p == 1 { 1 } else { ((len / (8.0 * tol)).sqrt().ceil() as usize * p).clamp(4, 96) };
                    for s in 0..k {
                        ts.push(w[0] + (w[1] - w[0]) * s as f64 / k as f64);
                    }
                }
                ts.push(c.domain().1);
                c.eval_many(&ts).into_iter().map(d2).collect()
            }
        }
    }

    /// Bézier curve through `pts` (3 points: quadratic, 4: cubic; start and end included).
    pub fn bezier(pts: &[DVec2]) -> Segment {
        let n = pts.len();
        let mut knots = vec![0.0; n];
        knots.extend(std::iter::repeat_n(1.0, n));
        Segment::Nurbs(BSplineCurve {
            degree: (n - 1) as u32,
            knots,
            points: pts.iter().map(|p| d3(*p)).collect(),
            weights: None,
        })
    }

    /// Approximate length (sampled).
    pub fn length(&self) -> f64 {
        match self {
            Segment::Line { a, b } => a.distance(*b),
            Segment::Arc { r, a0, a1, .. } => r * (a1 - a0).abs(),
            Segment::Nurbs(_) => self.sample(1e-3).windows(2).map(|w| w[0].distance(w[1])).sum(),
        }
    }
}

fn nurbs_end_tangent(c: &BSplineCurve, at_end: bool) -> DVec2 {
    // first/last distinct control point (clamped curves)
    let pts = &c.points;
    let n = pts.len();
    if at_end {
        let e = pts[n - 1];
        for i in (0..n - 1).rev() {
            if pts[i].distance(e) > EPS {
                return d2(e - pts[i]).normalize_or_zero();
            }
        }
    } else {
        let s = pts[0];
        for p in pts.iter().skip(1) {
            if p.distance(s) > EPS {
                return d2(*p - s).normalize_or_zero();
            }
        }
    }
    DVec2::ZERO
}

fn map_nurbs(c: &BSplineCurve, m: &DAffine2) -> BSplineCurve {
    BSplineCurve { points: c.points.iter().map(|p| d3(m.transform_point2(d2(*p)))).collect(), ..c.clone() }
}

/// Rational quadratic B-spline of a circular arc (Piegl & Tiller A7.1): pieces of at most 90°.
pub fn arc_nurbs(center: DVec2, r: f64, a0: f64, a1: f64) -> BSplineCurve {
    let sweep = a1 - a0;
    let n = ((sweep.abs() / (std::f64::consts::FRAC_PI_2 + 1e-9)).ceil() as usize).max(1);
    let d = sweep / n as f64;
    let w1 = (d / 2.0).cos();
    let mut points = vec![d3(polar(center, r, a0))];
    let mut weights = vec![1.0];
    let mut knots = vec![0.0; 3];
    for i in 0..n {
        let a = a0 + d * i as f64;
        points.push(d3(polar(center, r / w1, a + d / 2.0)));
        weights.push(w1);
        points.push(d3(polar(center, r, a + d)));
        weights.push(1.0);
        let k = (i + 1) as f64;
        if i + 1 < n {
            knots.extend([k, k]);
        }
    }
    knots.extend([n as f64; 3]);
    BSplineCurve { degree: 2, knots, points, weights: Some(weights) }
}

// ---------------------------------------------------------------- interpolation

/// Non-zero basis functions and their first derivative at `u` in span `k` (Piegl & Tiller
/// A2.3, first derivative only): `(values, derivatives)`, each `p + 1` entries for the
/// functions `k - p ..= k`.
fn basis_ders(knots: &[f64], p: usize, k: usize, u: f64) -> (Vec<f64>, Vec<f64>) {
    let mut ndu = vec![vec![0.0; p + 1]; p + 1];
    let mut left = vec![0.0; p + 1];
    let mut right = vec![0.0; p + 1];
    ndu[0][0] = 1.0;
    for j in 1..=p {
        left[j] = u - knots[k + 1 - j];
        right[j] = knots[k + j] - u;
        let mut saved = 0.0;
        for r in 0..j {
            ndu[j][r] = right[r + 1] + left[j - r];
            let temp = if ndu[j][r].abs() > 0.0 { ndu[r][j - 1] / ndu[j][r] } else { 0.0 };
            ndu[r][j] = saved + right[r + 1] * temp;
            saved = left[j - r] * temp;
        }
        ndu[j][j] = saved;
    }
    let vals: Vec<f64> = (0..=p).map(|j| ndu[j][p]).collect();
    let mut ders = vec![0.0; p + 1];
    if p >= 1 {
        for (r, d) in ders.iter_mut().enumerate() {
            let mut v = 0.0;
            if r >= 1 {
                let den = ndu[p][r - 1];
                if den.abs() > 0.0 {
                    v += ndu[r - 1][p - 1] / den;
                }
            }
            if r < p {
                let den = ndu[p][r];
                if den.abs() > 0.0 {
                    v -= ndu[r][p - 1] / den;
                }
            }
            *d = v * p as f64;
        }
    }
    (vals, ders)
}

fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<DVec2>) -> Option<Vec<DVec2>> {
    let n = b.len();
    for c in 0..n {
        let piv = (c..n).max_by(|&i, &j| a[i][c].abs().partial_cmp(&a[j][c].abs()).unwrap())?;
        if a[piv][c].abs() < 1e-14 {
            return None;
        }
        a.swap(c, piv);
        b.swap(c, piv);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            if f == 0.0 {
                continue;
            }
            let (top, bottom) = a.split_at_mut(r);
            for (x, y) in bottom[0][c..n].iter_mut().zip(&top[c][c..n]) {
                *x -= f * y;
            }
            let bc = b[c];
            b[r] -= bc * f;
        }
    }
    let mut x = vec![DVec2::ZERO; n];
    for r in (0..n).rev() {
        let mut s = b[r];
        for (xk, ak) in x[r + 1..n].iter().zip(&a[r][r + 1..n]) {
            s -= *xk * *ak;
        }
        x[r] = s / a[r][r];
    }
    Some(x)
}

/// Spline through `pts` (at least 2), cubic where enough conditions exist: global
/// interpolation with chord-length parameters and averaged knots (Piegl & Tiller 9.2.1/9.2.2),
/// optional end tangent directions `t0`/`t1` (scaled to the total chord length). `None` for
/// coincident consecutive points.
pub fn interpolate(pts: &[DVec2], t0: Option<DVec2>, t1: Option<DVec2>) -> Option<BSplineCurve> {
    let k = pts.len();
    if k < 2 || pts.windows(2).any(|w| w[0].distance(w[1]) <= EPS) {
        return None;
    }
    let t0 = t0.and_then(|t| t.try_normalize());
    let t1 = t1.and_then(|t| t.try_normalize());
    let m = k + t0.is_some() as usize + t1.is_some() as usize;
    let p = (m - 1).min(3);
    let total: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut u = vec![0.0; k];
    for i in 1..k {
        u[i] = u[i - 1] + pts[i].distance(pts[i - 1]) / total;
    }
    u[k - 1] = 1.0;
    // parameter list with the ends repeated once per derivative condition
    let mut ext = Vec::with_capacity(m);
    if t0.is_some() {
        ext.push(0.0);
    }
    ext.extend_from_slice(&u);
    if t1.is_some() {
        ext.push(1.0);
    }
    let mut knots = vec![0.0; p + 1];
    for j in 1..m - p {
        knots.push(ext[j..j + p].iter().sum::<f64>() / p as f64);
    }
    knots.extend(std::iter::repeat_n(1.0, p + 1));
    let mut a = vec![vec![0.0; m]; m];
    let mut b = vec![DVec2::ZERO; m];
    let mut row = 0;
    let put = |a: &mut Vec<Vec<f64>>, row: usize, uu: f64, der: bool| {
        let span = crate::brep::find_span(&knots, p, m, uu);
        let (v, d) = basis_ders(&knots, p, span, uu);
        for j in 0..=p {
            a[row][span - p + j] = if der { d[j] } else { v[j] };
        }
    };
    put(&mut a, row, 0.0, false);
    b[row] = pts[0];
    row += 1;
    if let Some(t) = t0 {
        put(&mut a, row, 0.0, true);
        b[row] = t * total;
        row += 1;
    }
    for i in 1..k - 1 {
        put(&mut a, row, u[i], false);
        b[row] = pts[i];
        row += 1;
    }
    if let Some(t) = t1 {
        put(&mut a, row, 1.0, true);
        b[row] = t * total;
        row += 1;
    }
    put(&mut a, row, 1.0, false);
    b[row] = pts[k - 1];
    let x = solve(a, b)?;
    Some(BSplineCurve { degree: p as u32, knots, points: x.into_iter().map(d3).collect(), weights: None })
}

// ---------------------------------------------------------------- joining

/// Homogeneous control points of a curve.
fn hpoints(c: &BSplineCurve) -> Vec<[f64; 3]> {
    c.points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let w = c.weight(i);
            [p.x * w, p.y * w, w]
        })
        .collect()
}

/// Raises a (homogeneous) Bézier control polygon by one degree.
fn elevate(b: &[[f64; 3]]) -> Vec<[f64; 3]> {
    let p = b.len() - 1;
    let mut out = Vec::with_capacity(p + 2);
    out.push(b[0]);
    for i in 1..=p {
        let a = i as f64 / (p + 1) as f64;
        out.push([0, 1, 2].map(|c| a * b[i - 1][c] + (1.0 - a) * b[i][c]));
    }
    out.push(b[p]);
    out
}

/// Bézier pieces of a clamped B-spline (homogeneous control polygons).
fn bezier_pieces(c: &BSplineCurve) -> Vec<Vec<[f64; 3]>> {
    let (lo, hi) = c.domain();
    let mut ks: Vec<f64> = c.knots.iter().copied().filter(|k| *k >= lo && *k <= hi).collect();
    ks.dedup_by(|a, b| (*a - *b).abs() <= EPS * (hi - lo).max(1.0));
    ks.windows(2).map(|w| hpoints(&c.trimmed(w[0], w[1]))).collect()
}

/// One B-spline through all segments in order (C0 at the joints, degree = the highest degree
/// of the pieces, rational if any piece is). Each segment gets a parameter interval as long as
/// the segment.
pub fn join(segs: &[Segment]) -> BSplineCurve {
    let curves: Vec<BSplineCurve> = segs.iter().map(|s| s.to_nurbs()).collect();
    let q = curves.iter().map(|c| c.degree).max().unwrap_or(1).max(1) as usize;
    let rational = curves.iter().any(|c| c.is_rational());
    let mut knots = vec![0.0; q + 1];
    let mut pts: Vec<[f64; 3]> = Vec::new();
    let mut t = 0.0;
    for (si, (c, s)) in curves.iter().zip(segs).enumerate() {
        let pieces = bezier_pieces(c);
        let seg_len = s.length().max(1e-9);
        let np = pieces.len();
        for (pi, mut b) in pieces.into_iter().enumerate() {
            while b.len() - 1 < q {
                b = elevate(&b);
            }
            if pts.is_empty() {
                pts.extend_from_slice(&b);
            } else {
                // shared end point: keep the previous one (equal up to rounding)
                pts.extend_from_slice(&b[1..]);
            }
            t += seg_len / np as f64;
            let last = si + 1 == segs.len() && pi + 1 == np;
            if !last {
                knots.extend(std::iter::repeat_n(t, q));
            }
        }
    }
    knots.extend(std::iter::repeat_n(t, q + 1));
    let points = pts.iter().map(|h| DVec3::new(h[0] / h[2], h[1] / h[2], 0.0)).collect();
    let weights = rational.then(|| pts.iter().map(|h| h[2]).collect());
    BSplineCurve { degree: q as u32, knots, points, weights }
}

impl Segment {
    /// Splits the segment at its point closest to `p`; `None` if that is (nearly) an end.
    pub fn split_at(&self, p: DVec2) -> Option<(Segment, Segment)> {
        const E: f64 = 1e-9;
        match self {
            Segment::Line { a, b } => {
                let d = *b - *a;
                let t = ((p - *a).dot(d) / d.length_squared().max(1e-300)).clamp(0.0, 1.0);
                if !(E..=1.0 - E).contains(&t) {
                    return None;
                }
                let m = *a + d * t;
                Some((Segment::Line { a: *a, b: m }, Segment::Line { a: m, b: *b }))
            }
            Segment::Arc { center, r, a0, a1 } => {
                let sw = a1 - a0;
                let ang = (p - *center).y.atan2((p - *center).x);
                let tau = std::f64::consts::TAU;
                let d = if sw >= 0.0 { (ang - a0).rem_euclid(tau) } else { -((a0 - ang).rem_euclid(tau)) };
                let f = d / sw;
                if !(E..=1.0 - E).contains(&f) {
                    return None;
                }
                let am = a0 + sw * f;
                Some((
                    Segment::Arc { center: *center, r: *r, a0: *a0, a1: am },
                    Segment::Arc { center: *center, r: *r, a0: am, a1: *a1 },
                ))
            }
            Segment::Nurbs(c) => {
                let (lo, hi) = c.domain();
                let n = 400;
                let ts: Vec<f64> = (0..=n).map(|i| lo + (hi - lo) * i as f64 / n as f64).collect();
                let pts = c.eval_many(&ts);
                let k = (0..=n)
                    .min_by(|&i, &j| d2(pts[i]).distance(p).partial_cmp(&d2(pts[j]).distance(p)).unwrap())
                    .unwrap_or(0);
                // refine by ternary search around the best sample
                let (mut x0, mut x1) = (ts[k.saturating_sub(1)], ts[(k + 1).min(n)]);
                for _ in 0..60 {
                    let m0 = x0 + (x1 - x0) / 3.0;
                    let m1 = x1 - (x1 - x0) / 3.0;
                    if d2(c.eval(m0)).distance(p) < d2(c.eval(m1)).distance(p) {
                        x1 = m1;
                    } else {
                        x0 = m0;
                    }
                }
                let t = (x0 + x1) / 2.0;
                if (t - lo) <= E * (hi - lo) || (hi - t) <= E * (hi - lo) {
                    return None;
                }
                Some((Segment::Nurbs(c.trimmed(lo, t)), Segment::Nurbs(c.trimmed(t, hi))))
            }
        }
    }
}

/// Area centroid of a closed loop of segments (from samples).
pub fn centroid(segs: &[Segment], tol: f64) -> DVec2 {
    let pts = polyline(segs, tol);
    let n = pts.len();
    let (mut a, mut c) = (0.0, DVec2::ZERO);
    for i in 0..n {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        let w = p.perp_dot(q);
        a += w;
        c += (p + q) * w;
    }
    if a.abs() < 1e-300 {
        return pts.iter().fold(DVec2::ZERO, |s, p| s + *p) / n.max(1) as f64;
    }
    c / (3.0 * a)
}

/// The closed loop restarted where the ray from `origin` along `dir` crosses it (the farthest
/// crossing); unchanged without a crossing.
pub fn restart_at_ray(segs: &[Segment], origin: DVec2, dir: DVec2, tol: f64) -> Vec<Segment> {
    let dir = dir.normalize_or_zero();
    let mut best: Option<(f64, usize, DVec2)> = None;
    for (k, s) in segs.iter().enumerate() {
        let pts = s.sample(tol);
        for w in pts.windows(2) {
            let e = w[1] - w[0];
            let den = dir.perp_dot(e);
            if den.abs() < 1e-300 {
                continue;
            }
            let q = w[0] - origin;
            let t = q.perp_dot(e) / den; // along the ray
            let u = q.perp_dot(dir) / den; // along the edge
            if t > 0.0 && (0.0..=1.0).contains(&u) && best.is_none_or(|b| t > b.0) {
                best = Some((t, k, origin + dir * t));
            }
        }
    }
    let Some((_, k, x)) = best else { return segs.to_vec() };
    let n = segs.len();
    let (first, last) = match segs[k].split_at(x) {
        Some((a, b)) => (Some(b), Some(a)),
        None => {
            // at an end of segment k: start at k or at k + 1
            if segs[k].start().distance(x) <= segs[k].end().distance(x) {
                (Some(segs[k].clone()), None)
            } else {
                return (1..=n).map(|i| segs[(k + i) % n].clone()).collect();
            }
        }
    };
    let mut out = Vec::with_capacity(n + 1);
    out.extend(first);
    for i in 1..n {
        out.push(segs[(k + i) % n].clone());
    }
    out.extend(last);
    out
}

/// Signed area of a closed loop of segments (positive: counter-clockwise), from samples.
pub fn signed_area(segs: &[Segment], tol: f64) -> f64 {
    let pts = polyline(segs, tol);
    let n = pts.len();
    (0..n).map(|i| pts[i].perp_dot(pts[(i + 1) % n])).sum::<f64>() / 2.0
}

/// Polyline of consecutive segments (closed loops: the last point is not repeated).
pub fn polyline(segs: &[Segment], tol: f64) -> Vec<DVec2> {
    let mut out: Vec<DVec2> = Vec::new();
    for s in segs {
        let p = s.sample(tol);
        let skip = usize::from(!out.is_empty());
        out.extend_from_slice(&p[skip..]);
    }
    if out.len() > 2 && out[0].distance(out[out.len() - 1]) <= 1e-9 * (1.0 + out[0].length()) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn arc_nurbs_lies_on_circle() {
        for (a0, a1) in [(0.0, PI / 2.0), (0.3, 2.9), (1.0, -2.5), (0.0, 2.0 * PI)] {
            let c = arc_nurbs(DVec2::new(1.0, 2.0), 3.0, a0, a1);
            c.check().unwrap();
            let (lo, hi) = c.domain();
            for i in 0..=40 {
                let p = c.eval(lo + (hi - lo) * i as f64 / 40.0);
                assert!(((p.truncate() - DVec2::new(1.0, 2.0)).length() - 3.0).abs() < 1e-12);
            }
            let s = Segment::Arc { center: DVec2::new(1.0, 2.0), r: 3.0, a0, a1 };
            assert!((c.eval(lo).truncate() - s.start()).length() < 1e-12);
            assert!((c.eval(hi).truncate() - s.end()).length() < 1e-12);
        }
    }

    #[test]
    fn interpolation_hits_points_and_tangents() {
        let pts = [DVec2::new(0.0, 0.0), DVec2::new(2.0, 1.0), DVec2::new(4.0, 0.0), DVec2::new(6.0, 2.0)];
        let c = interpolate(&pts, Some(DVec2::Y), Some(DVec2::X)).unwrap();
        c.check().unwrap();
        let seg = Segment::Nurbs(c.clone());
        assert!((seg.start_tangent() - DVec2::Y).length() < 1e-9);
        assert!((seg.end_tangent() - DVec2::X).length() < 1e-9);
        // chord-length parameters, as in `interpolate`
        let total: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        let mut u = 0.0;
        for (i, p) in pts.iter().enumerate() {
            if i > 0 {
                u += pts[i].distance(pts[i - 1]) / total;
            }
            let d = c.eval(u.min(1.0)).truncate().distance(*p);
            assert!(d < 1e-9, "{p} {d}");
        }
        // two points: a straight degree-1 curve
        let l = interpolate(&pts[..2], None, None).unwrap();
        assert_eq!(l.degree, 1);
        assert!(interpolate(&[DVec2::ZERO, DVec2::ZERO], None, None).is_none());
    }

    #[test]
    fn join_keeps_the_geometry() {
        let segs = vec![
            Segment::Line { a: DVec2::new(0.0, 0.0), b: DVec2::new(4.0, 0.0) },
            Segment::Arc { center: DVec2::new(4.0, 2.0), r: 2.0, a0: -PI / 2.0, a1: PI / 2.0 },
            Segment::bezier(&[DVec2::new(4.0, 4.0), DVec2::new(2.0, 6.0), DVec2::new(0.0, 4.0)]),
            Segment::Line { a: DVec2::new(0.0, 4.0), b: DVec2::new(0.0, 0.0) },
        ];
        let j = join(&segs);
        j.check().unwrap();
        assert_eq!(j.degree, 2);
        let a1 = signed_area(&segs, 1e-4);
        let a2 = signed_area(&[Segment::Nurbs(j)], 1e-4);
        assert!((a1 - a2).abs() < 1e-3, "{a1} {a2}");
        // 16 + half disk 2π + parabola cap 2/3 · 4 · 1
        let want = 16.0 + 2.0 * PI + 8.0 / 3.0;
        assert!((a1 - want).abs() < 1e-3, "{a1} {want}");
    }

    #[test]
    fn mapped_arcs() {
        let s = Segment::Arc { center: DVec2::ZERO, r: 1.0, a0: 0.0, a1: PI / 2.0 };
        let m = DAffine2::from_scale_angle_translation(DVec2::splat(2.0), 0.5, DVec2::new(1.0, 0.0));
        let t = s.map(&m);
        assert!(matches!(t, Segment::Arc { .. }));
        assert!((t.start() - m.transform_point2(s.start())).length() < 1e-12);
        assert!((t.end() - m.transform_point2(s.end())).length() < 1e-12);
        let mirror = DAffine2::from_scale(DVec2::new(-1.0, 1.0));
        let t = s.map(&mirror);
        assert!((t.start() - DVec2::new(-1.0, 0.0)).length() < 1e-12);
        assert!((t.end() - DVec2::new(0.0, 1.0)).length() < 1e-12);
        let squash = DAffine2::from_scale(DVec2::new(2.0, 1.0));
        let t = s.map(&squash);
        assert!(matches!(t, Segment::Nurbs(_)));
        assert!((t.end() - DVec2::new(0.0, 1.0)).length() < 1e-12);
    }
}
