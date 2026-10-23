//! Exact planar regions bounded by lines and circular arcs (Plan 17.3).
//!
//! * [`combine`]: boolean of operands (closed loops with a fill rule) under any predicate of
//!   the operands' insideness. Union, difference, intersection and the cleanup of
//!   self-overlapping loops are one routine: all segments are split at their exact
//!   intersections (line–line, line–circle, circle–circle, overlaps), and every piece is kept
//!   where the predicate differs on its two sides, oriented with the region on its left. The
//!   result consists of pieces of the input segments, so lines stay lines and arcs stay arcs.
//! * [`offset_round`], [`offset_delta`] (OpenSCAD `offset(r)` / `offset(delta, chamfer)`) and
//!   [`stroke`] build exact pieces (bands along the segments, discs and corner pieces at the
//!   vertices) and combine them with the region.
//!
//! B-spline segments enter as G1 biarc approximations ([`Segment::to_arcs`]): the offset of a
//! B-spline is no B-spline, the offset of an arc is an arc.

use std::f64::consts::{PI, TAU};

use glam::DVec2;

use crate::curve2::Segment;

/// A closed loop of connected segments.
pub type Loop2 = Vec<Segment>;

/// Fill rule of an operand: which winding numbers count as inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    NonZero,
    EvenOdd,
    Positive,
}

#[derive(Debug, Clone)]
pub struct Operand {
    pub loops: Vec<Loop2>,
    pub fill: Fill,
}

impl Operand {
    pub fn new(loops: Vec<Loop2>, fill: Fill) -> Operand {
        Operand { loops, fill }
    }
}

/// Default tolerance of the biarc approximation of B-splines for a model of size `size`:
/// 1 µm, at most 1e-5 of the size.
pub fn arc_tol(size: f64) -> f64 {
    (size * 1e-5).clamp(1e-7, 1e-3)
}

// ---------------------------------------------------------------- geometry pieces

/// A line or an arc with signed sweep (internal form).
#[derive(Debug, Clone, Copy)]
enum G {
    L { a: DVec2, b: DVec2 },
    A { c: DVec2, r: f64, a0: f64, sw: f64 },
}

fn polar(c: DVec2, r: f64, a: f64) -> DVec2 {
    c + DVec2::new(a.cos(), a.sin()) * r
}

fn perp(v: DVec2) -> DVec2 {
    DVec2::new(-v.y, v.x)
}

impl G {
    fn at(&self, t: f64) -> DVec2 {
        match *self {
            G::L { a, b } => a + (b - a) * t,
            G::A { c, r, a0, sw } => polar(c, r, a0 + sw * t),
        }
    }

    fn start(&self) -> DVec2 {
        self.at(0.0)
    }

    fn end(&self) -> DVec2 {
        self.at(1.0)
    }

    /// Unit tangent in the direction of travel.
    fn tangent(&self, t: f64) -> DVec2 {
        match *self {
            G::L { a, b } => (b - a).normalize_or_zero(),
            G::A { a0, sw, .. } => {
                let a = a0 + sw * t;
                DVec2::new(-a.sin(), a.cos()) * sw.signum()
            }
        }
    }

    fn len(&self) -> f64 {
        match *self {
            G::L { a, b } => a.distance(b),
            G::A { r, sw, .. } => r * sw.abs(),
        }
    }

    fn sub(&self, t0: f64, t1: f64) -> G {
        match *self {
            G::L { .. } => G::L { a: self.at(t0), b: self.at(t1) },
            G::A { c, r, a0, sw } => G::A { c, r, a0: a0 + sw * t0, sw: sw * (t1 - t0) },
        }
    }

    fn reversed(&self) -> G {
        match *self {
            G::L { a, b } => G::L { a: b, b: a },
            G::A { c, r, a0, sw } => G::A { c, r, a0: a0 + sw, sw: -sw },
        }
    }

    fn bbox(&self) -> (DVec2, DVec2) {
        match *self {
            G::L { a, b } => (a.min(b), a.max(b)),
            G::A { c, r, a0, sw } => {
                let (p, q) = (self.start(), self.end());
                let (mut lo, mut hi) = (p.min(q), p.max(q));
                for k in 0..4 {
                    let ang = k as f64 * PI / 2.0;
                    if in_sweep(ang, a0, sw) {
                        let x = polar(c, r, ang);
                        lo = lo.min(x);
                        hi = hi.max(x);
                    }
                }
                (lo, hi)
            }
        }
    }

    /// Parameter of a point on (or near) the segment, clamped to [0, 1].
    fn param_of(&self, p: DVec2) -> f64 {
        match *self {
            G::L { a, b } => {
                let d = b - a;
                ((p - a).dot(d) / d.length_squared().max(1e-300)).clamp(0.0, 1.0)
            }
            G::A { c, a0, sw, .. } => sweep_param(angle(p - c), a0, sw).unwrap_or_else(|| {
                // outside the sweep: the nearer end
                let d0 = angdist(angle(p - c), a0);
                let d1 = angdist(angle(p - c), a0 + sw);
                if d0 <= d1 { 0.0 } else { 1.0 }
            }),
        }
    }

    fn to_seg(self) -> Segment {
        match self {
            G::L { a, b } => Segment::Line { a, b },
            G::A { c, r, a0, sw } => Segment::Arc { center: c, r, a0, a1: a0 + sw },
        }
    }

    fn is_full_circle(&self) -> bool {
        matches!(*self, G::A { sw, .. } if sw.abs() >= TAU - 1e-12)
    }
}

fn angle(v: DVec2) -> f64 {
    v.y.atan2(v.x)
}

fn angdist(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(TAU);
    d.min(TAU - d)
}

/// Fraction of the sweep at which angle `ang` is reached (`None` outside), with a small
/// angular tolerance at both ends.
fn sweep_param(ang: f64, a0: f64, sw: f64) -> Option<f64> {
    let tol = 1e-12;
    let d = if sw >= 0.0 { (ang - a0).rem_euclid(TAU) } else { (a0 - ang).rem_euclid(TAU) };
    let s = sw.abs();
    if s >= TAU - 1e-12 {
        return Some(d / TAU);
    }
    if d <= s + tol {
        return Some((d / s).clamp(0.0, 1.0));
    }
    if TAU - d <= tol {
        return Some(0.0);
    }
    None
}

fn in_sweep(ang: f64, a0: f64, sw: f64) -> bool {
    let d = if sw >= 0.0 { (ang - a0).rem_euclid(TAU) } else { (a0 - ang).rem_euclid(TAU) };
    d <= sw.abs()
}

fn to_gs(seg: &Segment, tol: f64) -> Vec<G> {
    match seg {
        Segment::Line { a, b } => vec![G::L { a: *a, b: *b }],
        Segment::Arc { center, r, a0, a1 } => vec![G::A { c: *center, r: *r, a0: *a0, sw: a1 - a0 }],
        Segment::Nurbs(_) => seg.to_arcs(tol).iter().flat_map(|s| to_gs(s, tol)).collect(),
    }
}

// ---------------------------------------------------------------- winding

struct OpData {
    loops: Vec<Vec<G>>,
    lo: DVec2,
    hi: DVec2,
    fill: Fill,
}

impl OpData {
    fn winding(&self, p: DVec2) -> i32 {
        if p.x < self.lo.x || p.y < self.lo.y || p.x > self.hi.x || p.y > self.hi.y {
            return 0;
        }
        let mut w = 0;
        for lp in &self.loops {
            w += loop_winding(lp, p);
        }
        w
    }

    fn inside(&self, p: DVec2) -> bool {
        let w = self.winding(p);
        match self.fill {
            Fill::NonZero => w != 0,
            Fill::EvenOdd => w % 2 != 0,
            Fill::Positive => w > 0,
        }
    }
}

/// Winding number of a closed loop of lines and arcs around `p` (not on the loop): crossings
/// of the ray from `p` towards +x, with half-open intervals in y (arcs split into pieces
/// monotone in y).
fn loop_winding(lp: &[G], p: DVec2) -> i32 {
    let mut w = 0;
    // the shared vertices: every segment starts where the previous one ends, exactly (arc end
    // points computed from angles differ by rounding, and a ray through a vertex must see the
    // same point on both sides)
    let n = lp.len();
    for (i, g) in lp.iter().enumerate() {
        let (va, vb) = (lp[i].start(), lp[(i + 1) % n].start());
        match *g {
            G::L { .. } => w += line_crossing(va, vb, p),
            G::A { c, r, a0, sw } => {
                if p.x > c.x + r || p.y > c.y + r || p.y < c.y - r {
                    continue;
                }
                // split at the angles of horizontal tangents (π/2 + kπ) inside the sweep
                let mut cuts = vec![0.0];
                let k0 = ((a0.min(a0 + sw) - PI / 2.0) / PI).floor() as i64;
                let k1 = ((a0.max(a0 + sw) - PI / 2.0) / PI).ceil() as i64;
                for k in k0..=k1 {
                    let ang = PI / 2.0 + k as f64 * PI;
                    let t = (ang - a0) / sw;
                    if t > 0.0 && t < 1.0 {
                        cuts.push(t);
                    }
                }
                cuts.push(1.0);
                cuts.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
                for win in cuts.windows(2) {
                    let (t0, t1) = (win[0], win[1]);
                    if t1 <= t0 {
                        continue;
                    }
                    let s = if t0 == 0.0 { va } else { g.at(t0) };
                    let e = if t1 == 1.0 { vb } else { g.at(t1) };
                    let up = s.y <= p.y && e.y > p.y;
                    let down = e.y <= p.y && s.y > p.y;
                    if !(up || down) {
                        continue;
                    }
                    let mid = a0 + sw * (t0 + t1) / 2.0;
                    let dy = p.y - c.y;
                    let x = c.x + mid.cos().signum() * (r * r - dy * dy).max(0.0).sqrt();
                    if x > p.x {
                        w += if up { 1 } else { -1 };
                    }
                }
            }
        }
    }
    w
}

fn line_crossing(a: DVec2, b: DVec2, p: DVec2) -> i32 {
    let side = (b - a).perp_dot(p - a);
    if a.y <= p.y {
        if b.y > p.y && side > 0.0 {
            return 1;
        }
    } else if b.y <= p.y && side < 0.0 {
        return -1;
    }
    0
}

// ---------------------------------------------------------------- intersections

/// Intersection parameters `(t on g, u on h, point)`; overlaps give their end points.
fn intersect(g: &G, h: &G, eps: f64, out: &mut Vec<(f64, f64, DVec2)>) {
    match (*g, *h) {
        (G::L { a: a1, b: b1 }, G::L { a: a2, b: b2 }) => {
            let d1 = b1 - a1;
            let d2 = b2 - a2;
            let (l1, l2) = (d1.length(), d2.length());
            if l1 <= 0.0 || l2 <= 0.0 {
                return;
            }
            let den = d1.perp_dot(d2);
            let w = a2 - a1;
            if den.abs() <= 1e-12 * l1 * l2 {
                // parallel: collinear overlap?
                if (w.perp_dot(d1) / l1).abs() > eps {
                    return;
                }
                for (p, on_g) in [(a2, true), (b2, true), (a1, false), (b1, false)] {
                    if on_g {
                        let t = (p - a1).dot(d1) / (l1 * l1);
                        if (-eps / l1..=1.0 + eps / l1).contains(&t) {
                            out.push((t.clamp(0.0, 1.0), h.param_of(p), p));
                        }
                    } else {
                        let u = (p - a2).dot(d2) / (l2 * l2);
                        if (-eps / l2..=1.0 + eps / l2).contains(&u) {
                            out.push((g.param_of(p), u.clamp(0.0, 1.0), p));
                        }
                    }
                }
                return;
            }
            let t = w.perp_dot(d2) / den;
            let u = w.perp_dot(d1) / den;
            let (et, eu) = (eps / l1, eps / l2);
            if (-et..=1.0 + et).contains(&t) && (-eu..=1.0 + eu).contains(&u) {
                let t = t.clamp(0.0, 1.0);
                out.push((t, u.clamp(0.0, 1.0), a1 + d1 * t));
            }
        }
        (G::L { .. }, G::A { .. }) => {
            let mut tmp = Vec::new();
            intersect(h, g, eps, &mut tmp);
            out.extend(tmp.into_iter().map(|(u, t, p)| (t, u, p)));
        }
        (G::A { c, r, a0, sw }, G::L { a, b }) => {
            let d = b - a;
            let l = d.length();
            if l <= 0.0 {
                return;
            }
            let dir = d / l;
            let foot_s = (c - a).dot(dir);
            let foot = a + dir * foot_s;
            let hgt = foot.distance(c);
            let mut pts = Vec::new();
            if (hgt - r).abs() <= eps {
                pts.push(foot);
            } else if hgt < r {
                let k = (r * r - hgt * hgt).sqrt();
                pts.push(foot - dir * k);
                pts.push(foot + dir * k);
            }
            for p in pts {
                let u = (p - a).dot(dir) / l;
                if !(-eps / l..=1.0 + eps / l).contains(&u) {
                    continue;
                }
                let ang = angle(p - c);
                let Some(t) = sweep_param_tol(ang, a0, sw, eps / r) else { continue };
                // the point exactly on the circle
                let p = c + (p - c).normalize_or_zero() * r;
                out.push((t, u.clamp(0.0, 1.0), p));
            }
        }
        (G::A { c: c1, r: r1, a0: s1, sw: w1 }, G::A { c: c2, r: r2, a0: s2, sw: w2 }) => {
            let dv = c2 - c1;
            let dd = dv.length();
            if dd <= eps && (r1 - r2).abs() <= eps {
                // same circle: end points of one arc on the other
                for (p, on_g) in [(h.start(), true), (h.end(), true), (g.start(), false), (g.end(), false)] {
                    if on_g {
                        if let Some(t) = sweep_param_tol(angle(p - c1), s1, w1, eps / r1) {
                            out.push((t, h.param_of(p), p));
                        }
                    } else if let Some(u) = sweep_param_tol(angle(p - c2), s2, w2, eps / r2) {
                        out.push((g.param_of(p), u, p));
                    }
                }
                return;
            }
            if dd <= eps {
                return;
            }
            let mut pts = Vec::new();
            if (dd - (r1 + r2)).abs() <= eps || (dd - (r1 - r2).abs()).abs() <= eps {
                // tangent circles
                let s = if (dd - (r1 + r2)).abs() <= eps || r1 >= r2 { 1.0 } else { -1.0 };
                pts.push(c1 + dv / dd * r1 * s);
            } else if dd < r1 + r2 && dd > (r1 - r2).abs() {
                let a = (r1 * r1 - r2 * r2 + dd * dd) / (2.0 * dd);
                let hh = (r1 * r1 - a * a).max(0.0).sqrt();
                let m = c1 + dv / dd * a;
                let n = perp(dv / dd);
                pts.push(m + n * hh);
                pts.push(m - n * hh);
            }
            for p in pts {
                let Some(t) = sweep_param_tol(angle(p - c1), s1, w1, eps / r1) else { continue };
                let Some(u) = sweep_param_tol(angle(p - c2), s2, w2, eps / r2) else { continue };
                out.push((t, u, p));
            }
        }
    }
}

fn sweep_param_tol(ang: f64, a0: f64, sw: f64, atol: f64) -> Option<f64> {
    let d = if sw >= 0.0 { (ang - a0).rem_euclid(TAU) } else { (a0 - ang).rem_euclid(TAU) };
    let s = sw.abs();
    if s >= TAU - 1e-12 {
        return Some(d / TAU);
    }
    if d <= s + atol {
        return Some((d / s).clamp(0.0, 1.0));
    }
    if TAU - d <= atol {
        return Some(0.0);
    }
    None
}

// ---------------------------------------------------------------- vertices

struct Verts {
    pos: Vec<DVec2>,
    cell: f64,
    map: std::collections::HashMap<(i64, i64), Vec<usize>>,
    eps: f64,
}

impl Verts {
    fn new(eps: f64) -> Verts {
        Verts { pos: Vec::new(), cell: eps * 4.0, map: Default::default(), eps }
    }

    fn key(&self, p: DVec2) -> (i64, i64) {
        ((p.x / self.cell).floor() as i64, (p.y / self.cell).floor() as i64)
    }

    fn get(&mut self, p: DVec2) -> usize {
        let (kx, ky) = self.key(p);
        let mut best: Option<(f64, usize)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(v) = self.map.get(&(kx + dx, ky + dy)) {
                    for &i in v {
                        let d = self.pos[i].distance(p);
                        if d <= self.eps && best.is_none_or(|b| d < b.0) {
                            best = Some((d, i));
                        }
                    }
                }
            }
        }
        if let Some((_, i)) = best {
            return i;
        }
        let i = self.pos.len();
        self.pos.push(p);
        self.map.entry((kx, ky)).or_default().push(i);
        i
    }
}

// ---------------------------------------------------------------- combine

#[derive(Debug, Clone, Copy)]
struct Edge {
    g: G,
    v0: usize,
    v1: usize,
}

/// Boolean of the operands: the region where `pred(inside_0, inside_1, …)` holds. Loops of
/// the result have the region on their left (outer loops counter-clockwise, holes clockwise),
/// consist of lines and arcs, and do not cross each other.
pub fn combine(ops: &[Operand], pred: &dyn Fn(&[bool]) -> bool) -> Vec<Loop2> {
    // size and tolerances
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for o in ops {
        for lp in &o.loops {
            for s in lp {
                for p in [s.start(), s.end()] {
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
                if let Segment::Arc { center, r, .. } = s {
                    lo = lo.min(*center - DVec2::splat(*r));
                    hi = hi.max(*center + DVec2::splat(*r));
                }
            }
        }
    }
    if !lo.x.is_finite() {
        return Vec::new();
    }
    let size = (hi - lo).length().max(1e-9) + lo.abs().max(hi.abs()).max_element() * 1e-3;
    let eps = size * 1e-10;
    let atol = arc_tol(size);
    let data: Vec<OpData> = ops
        .iter()
        .map(|o| {
            let loops: Vec<Vec<G>> = o
                .loops
                .iter()
                .map(|lp| lp.iter().flat_map(|s| to_gs(s, atol)).filter(|g| g.len() > eps).collect::<Vec<G>>())
                .filter(|l: &Vec<G>| !l.is_empty())
                .collect();
            let (mut a, mut b) = (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY));
            for g in loops.iter().flatten() {
                let (p, q) = g.bbox();
                a = a.min(p);
                b = b.max(q);
            }
            let pad = DVec2::splat(eps * 10.0);
            OpData { loops, lo: a - pad, hi: b + pad, fill: o.fill }
        })
        .collect();
    let segs: Vec<G> = data.iter().flat_map(|d| d.loops.iter().flatten().copied()).collect();
    let n = segs.len();
    let boxes: Vec<(DVec2, DVec2)> = segs.iter().map(|g| g.bbox()).collect();
    // candidate pairs: sweep over x
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| boxes[i].0.x.partial_cmp(&boxes[j].0.x).unwrap_or(std::cmp::Ordering::Equal));
    let mut verts = Verts::new(eps * 8.0);
    let mut cuts: Vec<Vec<(f64, usize)>> = vec![Vec::new(); n];
    for (i, g) in segs.iter().enumerate() {
        let v0 = verts.get(g.start());
        let v1 = verts.get(g.end());
        cuts[i].push((0.0, v0));
        cuts[i].push((1.0, v1));
    }
    let mut hits = Vec::new();
    for (k, &i) in order.iter().enumerate() {
        let (ilo, ihi) = boxes[i];
        for &j in &order[k + 1..] {
            let (jlo, jhi) = boxes[j];
            if jlo.x > ihi.x + eps * 4.0 {
                break;
            }
            if jlo.y > ihi.y + eps * 4.0 || ilo.y > jhi.y + eps * 4.0 || ilo.x > jhi.x + eps * 4.0 {
                continue;
            }
            hits.clear();
            intersect(&segs[i], &segs[j], eps, &mut hits);
            for &(t, u, p) in &hits {
                let v = verts.get(p);
                cuts[i].push((t, v));
                cuts[j].push((u, v));
            }
        }
    }
    // split into edges
    let mut edges: Vec<Edge> = Vec::new();
    for (i, g) in segs.iter().enumerate() {
        let c = &mut cuts[i];
        c.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        // a vertex met again later on the same segment: keep its parameter order
        let mut pts: Vec<(f64, usize)> = Vec::with_capacity(c.len());
        for &(t, v) in c.iter() {
            if pts.last().is_some_and(|l| l.1 == v) {
                continue;
            }
            pts.push((t, v));
        }
        if g.is_full_circle() && pts.len() == 1 {
            // unsplit full circle: one self-loop edge
            edges.push(Edge { g: *g, v0: pts[0].1, v1: pts[0].1 });
            continue;
        }
        if g.is_full_circle() && pts.len() >= 2 && pts[0].1 == pts[pts.len() - 1].1 {
            // closed again at the start vertex: fine
        }
        for w in pts.windows(2) {
            let (t0, a) = w[0];
            let (t1, b) = w[1];
            if a == b || t1 - t0 <= 0.0 {
                continue;
            }
            let mut e = g.sub(t0, t1);
            if let G::L { .. } = e {
                e = G::L { a: verts.pos[a], b: verts.pos[b] };
            }
            if e.len() <= eps {
                continue;
            }
            edges.push(Edge { g: e, v0: a, v1: b });
        }
    }
    // coincident edges: keep one
    let mut seen: std::collections::HashMap<(usize, usize), Vec<usize>> = Default::default();
    let mut uniq: Vec<Edge> = Vec::new();
    for e in edges {
        let key = (e.v0.min(e.v1), e.v0.max(e.v1));
        let mid = e.g.at(0.5);
        let dup = seen.get(&key).is_some_and(|l| {
            l.iter().any(|&k| {
                let f = &uniq[k];
                std::mem::discriminant(&f.g) == std::mem::discriminant(&e.g)
                    && f.g.at(0.5).distance(mid) <= eps * 100.0
                    && (!e.g.is_full_circle()
                        || f.g.at(0.25).distance(e.g.at(0.25)).min(f.g.at(0.75).distance(e.g.at(0.25))) <= eps * 100.0)
            })
        });
        if dup {
            continue;
        }
        seen.entry(key).or_default().push(uniq.len());
        uniq.push(e);
    }
    // classification
    let mut kept: Vec<Edge> = Vec::new();
    let mut ins = vec![false; data.len()];
    for e in &uniq {
        let m = e.g.at(0.5);
        let t = e.g.tangent(0.5);
        let nrm = perp(t);
        let delta = (e.g.len() * 0.01).min(size * 1e-6).max(size * 1e-9);
        for (k, d) in data.iter().enumerate() {
            ins[k] = d.inside(m + nrm * delta);
        }
        let left = pred(&ins);
        for (k, d) in data.iter().enumerate() {
            ins[k] = d.inside(m - nrm * delta);
        }
        let right = pred(&ins);
        if left == right {
            continue;
        }
        kept.push(if left { *e } else { Edge { g: e.g.reversed(), v0: e.v1, v1: e.v0 } });
    }
    link(&kept, &verts.pos, eps)
}

/// Joins oriented edges into loops (the tightest turn at shared vertices) and merges pieces
/// of the same line or circle.
fn link(kept: &[Edge], vpos: &[DVec2], eps: f64) -> Vec<Loop2> {
    let mut out_of: std::collections::HashMap<usize, Vec<usize>> = Default::default();
    for (i, e) in kept.iter().enumerate() {
        out_of.entry(e.v0).or_default().push(i);
    }
    let mut used = vec![false; kept.len()];
    let mut loops = Vec::new();
    for s in 0..kept.len() {
        if used[s] {
            continue;
        }
        used[s] = true;
        let mut lp = vec![kept[s].g];
        let start = kept[s].v0;
        let mut cur = s;
        let mut ok = true;
        while kept[cur].v1 != start {
            let v = kept[cur].v1;
            let cands: Vec<usize> =
                out_of.get(&v).map(|l| l.iter().copied().filter(|&k| !used[k]).collect()).unwrap_or_default();
            if cands.is_empty() {
                ok = false;
                break;
            }
            let next = if cands.len() == 1 {
                cands[0]
            } else {
                // probe directions at a common small distance
                let inc = kept[cur].g;
                let mut rho = inc.len();
                for &k in &cands {
                    rho = rho.min(kept[k].g.len());
                }
                rho *= 0.25;
                let vp = vpos[v];
                let rdir = inc.at(1.0 - (rho / inc.len()).min(1.0)) - vp;
                let ar = angle(rdir);
                *cands
                    .iter()
                    .min_by(|&&a, &&b| {
                        let cw = |k: usize| {
                            let g = kept[k].g;
                            let d = g.at((rho / g.len()).min(1.0)) - vp;
                            let x = (ar - angle(d)).rem_euclid(TAU);
                            if x <= 1e-12 { TAU } else { x }
                        };
                        cw(a).partial_cmp(&cw(b)).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .unwrap()
            };
            used[next] = true;
            lp.push(kept[next].g);
            cur = next;
            if lp.len() > kept.len() + 1 {
                ok = false;
                break;
            }
        }
        if ok {
            let merged = merge(lp, eps);
            if !merged.is_empty() {
                loops.push(merged);
            }
        }
    }
    loops
}

fn mergeable(a: &G, b: &G, eps: f64) -> Option<G> {
    match (*a, *b) {
        (G::L { a: p, b: q }, G::L { a: q2, b: r }) => {
            let d1 = q - p;
            let d2 = r - q2;
            let cr = d1.perp_dot(d2).abs() / (d1.length() * d2.length()).max(1e-300);
            // also the distance of the joint from the merged line
            let dist = (r - p).perp_dot(q - p).abs() / (r - p).length().max(1e-300);
            (cr < 1e-9 && d1.dot(d2) > 0.0 && dist <= eps * 10.0).then_some(G::L { a: p, b: r })
        }
        (G::A { c: c1, r: r1, a0, sw: s1 }, G::A { c: c2, r: r2, sw: s2, .. }) => {
            let same = c1.distance(c2) <= eps * 10.0 && (r1 - r2).abs() <= eps * 10.0 && s1 * s2 > 0.0;
            (same && (s1 + s2).abs() <= TAU + 1e-9).then_some(G::A { c: c1, r: r1, a0, sw: s1 + s2 })
        }
        _ => None,
    }
}

fn merge(lp: Vec<G>, eps: f64) -> Loop2 {
    let mut v: Vec<G> = Vec::with_capacity(lp.len());
    for g in lp {
        if let Some(last) = v.last()
            && let Some(m) = mergeable(last, &g, eps)
        {
            *v.last_mut().unwrap() = m;
            continue;
        }
        v.push(g);
    }
    while v.len() > 1 {
        let (first, last) = (v[0], v[v.len() - 1]);
        match mergeable(&last, &first, eps) {
            Some(m) => {
                v.pop();
                v[0] = m;
            }
            None => break,
        }
    }
    // connect exactly: every segment starts where the previous one ends (lines take the arc's
    // end points)
    let n = v.len();
    let mut out: Vec<Segment> = Vec::with_capacity(n + 1);
    for g in &v {
        if g.is_full_circle() {
            // a full circle as two halves (no zero-length chord)
            let G::A { c, r, a0, sw } = *g else { unreachable!() };
            out.push(G::A { c, r, a0, sw: sw / 2.0 }.to_seg());
            out.push(G::A { c, r, a0: a0 + sw / 2.0, sw: sw / 2.0 }.to_seg());
        } else {
            out.push(g.to_seg());
        }
    }
    let n = out.len();
    for i in 0..n {
        let prev_end = out[(i + n - 1) % n].end();
        if let Segment::Line { a, .. } = &mut out[i] {
            *a = prev_end;
        }
    }
    for i in 0..n {
        let next_start = out[(i + 1) % n].start();
        if let Segment::Line { b, .. } = &mut out[i] {
            *b = next_start;
        }
    }
    out
}

// ---------------------------------------------------------------- helpers

/// Signed area of a loop of lines and arcs (exact for arcs).
pub fn loop_area(lp: &[Segment]) -> f64 {
    let mut a = 0.0;
    for s in lp {
        match s {
            Segment::Arc { r, a0, a1, .. } => {
                // area under the arc: chord part + circular segment
                let (p, q) = (s.start(), s.end());
                a += p.perp_dot(q) / 2.0;
                let sw = a1 - a0;
                a += r * r / 2.0 * (sw - sw.sin());
            }
            Segment::Line { a: p, b: q } => a += p.perp_dot(*q) / 2.0,
            Segment::Nurbs(_) => {
                let pts = s.sample(1e-5);
                for w in pts.windows(2) {
                    a += w[0].perp_dot(w[1]) / 2.0;
                }
            }
        }
    }
    a
}

/// Area of a region (sum of signed loop areas).
pub fn region_area(loops: &[Loop2]) -> f64 {
    loops.iter().map(|l| loop_area(l)).sum()
}

fn circle_loop(c: DVec2, r: f64) -> Loop2 {
    vec![Segment::Arc { center: c, r, a0: 0.0, a1: PI }, Segment::Arc { center: c, r, a0: PI, a1: TAU }]
}

fn poly_loop(pts: &[DVec2]) -> Loop2 {
    let n = pts.len();
    (0..n).map(|i| Segment::Line { a: pts[i], b: pts[(i + 1) % n] }).collect()
}

/// Band of half-width `h` around a segment between signed normal distances `lo..hi`
/// (left positive). Arcs give annular sectors; an inner radius below zero becomes a sector
/// (plus the mirrored sector for a two-sided band).
fn band(g: &Segment, lo: f64, hi: f64) -> Vec<Loop2> {
    match g {
        Segment::Line { a, b } => {
            let n = perp((*b - *a).normalize_or_zero());
            vec![poly_loop(&[*a + n * lo, *b + n * lo, *b + n * hi, *a + n * hi])]
        }
        Segment::Arc { center, r, a0, a1 } => {
            let sw = a1 - a0;
            // left of travel is towards the centre for counter-clockwise arcs
            let s = if sw >= 0.0 { -1.0 } else { 1.0 };
            let (ra, rb) = (r + s * lo, r + s * hi);
            let (rmin, rmax) = (ra.min(rb), ra.max(rb));
            let mut out = Vec::new();
            if rmax <= 0.0 {
                // entirely on the far side of the centre
                out.extend(sector(*center, -rmin, *a0 + PI, sw, (-rmax).max(0.0)));
                return out;
            }
            out.extend(sector(*center, rmax, *a0, sw, rmin.max(0.0)));
            if rmin < 0.0 {
                out.extend(sector(*center, -rmin, *a0 + PI, sw, 0.0));
            }
            out
        }
        Segment::Nurbs(_) => Vec::new(),
    }
}

/// Annular sector (`ri = 0`: sector; full sweep: ring or disc) as loops.
fn sector(c: DVec2, ro: f64, a0: f64, sw: f64, ri: f64) -> Vec<Loop2> {
    if ro <= 0.0 {
        return Vec::new();
    }
    let ri = if ri < ro * 1e-9 { 0.0 } else { ri };
    if sw.abs() >= TAU - 1e-9 {
        let mut v = vec![circle_loop(c, ro)];
        if ri > 0.0 {
            let mut inner = circle_loop(c, ri);
            inner = inner.iter().rev().map(|s| s.reversed()).collect();
            v.push(inner);
        }
        return v;
    }
    let a1 = a0 + sw;
    let outer = Segment::Arc { center: c, r: ro, a0, a1 };
    if ri == 0.0 {
        vec![vec![Segment::Line { a: c, b: outer.start() }, outer.clone(), Segment::Line { a: outer.end(), b: c }]]
    } else {
        let inner = Segment::Arc { center: c, r: ri, a0: a1, a1: a0 };
        vec![vec![
            outer.clone(),
            Segment::Line { a: outer.end(), b: inner.start() },
            inner.clone(),
            Segment::Line { a: inner.end(), b: outer.start() },
        ]]
    }
}

/// Lines and arcs of a region (B-splines as biarcs).
fn flatten(loops: &[Loop2], tol: f64) -> Vec<Loop2> {
    loops
        .iter()
        .map(|lp| {
            lp.iter()
                .flat_map(|s| match s {
                    Segment::Nurbs(_) => s.to_arcs(tol),
                    _ => vec![s.clone()],
                })
                .collect()
        })
        .collect()
}

fn size_of(loops: &[Loop2]) -> f64 {
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for s in loops.iter().flatten() {
        for p in [s.start(), s.end()] {
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    if lo.x.is_finite() { (hi - lo).length().max(1e-9) } else { 1.0 }
}

fn union_pred(ins: &[bool]) -> bool {
    ins.iter().any(|&x| x)
}

fn minus_pred(ins: &[bool]) -> bool {
    ins[0] && !ins[1..].iter().any(|&x| x)
}

/// Turn angle at the joint from tangent `t0` to `t1` (positive: left turn).
pub fn turn(t0: DVec2, t1: DVec2) -> f64 {
    t0.perp_dot(t1).atan2(t0.dot(t1))
}

// ---------------------------------------------------------------- offset

/// `offset(r)`: all points within `r` of the region (`r > 0`), or the points farther than
/// `-r` from its outside (`r < 0`). Exact: bands along lines and arcs, discs at the vertices.
pub fn offset_round(region: &[Loop2], r: f64) -> Vec<Loop2> {
    if r == 0.0 || region.is_empty() {
        return region.to_vec();
    }
    let region = flatten(region, arc_tol(size_of(region)));
    let rho = r.abs();
    let mut ops = vec![Operand::new(region.clone(), Fill::Positive)];
    for lp in &region {
        for s in lp {
            ops.push(Operand::new(band(s, -rho, rho), Fill::NonZero));
            ops.push(Operand::new(vec![circle_loop(s.start(), rho)], Fill::NonZero));
        }
    }
    combine(&ops, if r > 0.0 { &union_pred } else { &minus_pred })
}

/// `offset(delta)` (`chamfer`: corners cut at distance `delta` from the vertex, as Clipper's
/// square joins): edges move by `delta` along their normals, corners extend to the
/// intersection of the moved tangent lines.
pub fn offset_delta(region: &[Loop2], delta: f64, chamfer: bool) -> Vec<Loop2> {
    if delta == 0.0 || region.is_empty() {
        return region.to_vec();
    }
    let region = flatten(region, arc_tol(size_of(region)));
    let d = delta.abs();
    // outward = right of travel; inward offsets work on the left
    let side = if delta > 0.0 { -1.0 } else { 1.0 };
    let mut ops = vec![Operand::new(region.clone(), Fill::Positive)];
    for lp in &region {
        let n = lp.len();
        for (i, s) in lp.iter().enumerate() {
            let (lo, hi) = if side > 0.0 { (0.0, d) } else { (-d, 0.0) };
            ops.push(Operand::new(band(s, lo, hi), Fill::NonZero));
            // corner piece at the start of s, where the moved tangent lines separate
            let prev = &lp[(i + n - 1) % n];
            let (t0, t1) = (prev.end_tangent(), s.start_tangent());
            let phi = turn(t0, t1);
            // separating: a left turn for outward offsets, a right turn for inward ones
            if phi * -side <= 1e-9 {
                continue;
            }
            let v = s.start();
            let n0 = perp(t0) * side;
            let n1 = perp(t1) * side;
            let p0 = v + n0 * d;
            let p1 = v + n1 * d;
            let cosn = n0.dot(n1).clamp(-1.0, 1.0);
            let piece = if !chamfer && cosn > -1.0 + 1e-9 {
                let m = v + (n0 + n1) * (d / (1.0 + cosn));
                vec![v, p0, m, p1]
            } else {
                // cut perpendicular to the bisector at distance d from the vertex
                let b = (n0 + n1).try_normalize().unwrap_or(t0);
                let q0 = p0 + t0 * ((d - (p0 - v).dot(b)) / t0.dot(b).max(1e-12));
                let q1 = p1 - t1 * ((d - (p1 - v).dot(b)) / (-t1).dot(b).max(1e-12));
                vec![v, p0, q0, q1, p1]
            };
            ops.push(Operand::new(vec![poly_loop(&piece)], Fill::NonZero));
        }
    }
    combine(&ops, if delta > 0.0 { &union_pred } else { &minus_pred })
}

// ---------------------------------------------------------------- stroke

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap {
    Round,
    Flat,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    Round,
    Miter,
    Bevel,
}

/// Miter length limit of `stroke` (in half-widths, as SVG's default `stroke-miterlimit`):
/// longer miters are bevelled.
pub const MITER_LIMIT: f64 = 4.0;

/// Area covered by a pen of width `w` moved along the paths (`closed`: joins at every
/// vertex, no caps). Exact for lines and arcs.
pub fn stroke(paths: &[(Vec<Segment>, bool)], w: f64, cap: Cap, join: Join) -> Vec<Loop2> {
    let h = w.abs() / 2.0;
    if h <= 0.0 {
        return Vec::new();
    }
    let all: Vec<Loop2> = paths.iter().map(|p| p.0.clone()).collect();
    let tol = arc_tol(size_of(&all));
    let mut ops: Vec<Operand> = Vec::new();
    for (segs, closed) in paths {
        let segs: Vec<Segment> = segs
            .iter()
            .flat_map(|s| match s {
                Segment::Nurbs(_) => s.to_arcs(tol),
                _ => vec![s.clone()],
            })
            .collect();
        let n = segs.len();
        if n == 0 {
            continue;
        }
        for s in &segs {
            ops.push(Operand::new(band(s, -h, h), Fill::NonZero));
        }
        // joins
        let joints: Vec<usize> = if *closed { (0..n).collect() } else { (1..n).collect() };
        for i in joints {
            let prev = &segs[(i + n - 1) % n];
            let s = &segs[i];
            let v = s.start();
            let (t0, t1) = (prev.end_tangent(), s.start_tangent());
            let phi = turn(t0, t1);
            if phi.abs() <= 1e-9 {
                continue;
            }
            match join {
                Join::Round => ops.push(Operand::new(vec![circle_loop(v, h)], Fill::NonZero)),
                Join::Miter | Join::Bevel => {
                    // outer side of the turn: right for a left turn
                    let side = if phi > 0.0 { -1.0 } else { 1.0 };
                    let n0 = perp(t0) * side;
                    let n1 = perp(t1) * side;
                    let (p0, p1) = (v + n0 * h, v + n1 * h);
                    let cosn = n0.dot(n1);
                    let miter_len = if cosn > -1.0 + 1e-12 { (2.0 / (1.0 + cosn)).sqrt() } else { f64::INFINITY };
                    if join == Join::Miter && miter_len <= MITER_LIMIT {
                        let m = v + (n0 + n1) * (h / (1.0 + cosn));
                        ops.push(Operand::new(vec![poly_loop(&[v, p0, m, p1])], Fill::NonZero));
                    } else {
                        ops.push(Operand::new(vec![poly_loop(&[v, p0, p1])], Fill::NonZero));
                    }
                }
            }
        }
        if !*closed {
            let (a, ta) = (segs[0].start(), segs[0].start_tangent());
            let (b, tb) = (segs[n - 1].end(), segs[n - 1].end_tangent());
            match cap {
                Cap::Round => {
                    ops.push(Operand::new(vec![circle_loop(a, h)], Fill::NonZero));
                    ops.push(Operand::new(vec![circle_loop(b, h)], Fill::NonZero));
                }
                Cap::Square => {
                    let (na, nb) = (perp(ta), perp(tb));
                    ops.push(Operand::new(
                        vec![poly_loop(&[a - ta * h - na * h, a - na * h, a + na * h, a - ta * h + na * h])],
                        Fill::NonZero,
                    ));
                    ops.push(Operand::new(
                        vec![poly_loop(&[b - nb * h, b + tb * h - nb * h, b + tb * h + nb * h, b + nb * h])],
                        Fill::NonZero,
                    ));
                }
                Cap::Flat => {}
            }
        }
    }
    combine(&ops, &union_pred)
}

// ---------------------------------------------------------------- corners

/// Rounding (`Round(r)`) or chamfer (`Chamfer(d)`: cut points at distance `d` from the vertex)
/// of a corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CornerKind {
    Round(f64),
    Chamfer(f64),
}

#[derive(Debug, Clone)]
pub struct CornerResult {
    /// The incoming segment, shortened (`None`: used up completely).
    pub a: Option<Segment>,
    /// Arc (rounding) or line (chamfer).
    pub mid: Segment,
    /// The outgoing segment, shortened (`None`: used up completely).
    pub b: Option<Segment>,
    /// Centre of the rounding.
    pub center: Option<DVec2>,
    /// Residual of the numerical solution (B-splines; 0 for the closed forms).
    pub residual: f64,
    /// Where the rounding starts on the incoming segment and ends on the outgoing one
    /// (fractions of their parameter ranges).
    pub ua: f64,
    pub ub: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CornerError {
    /// The segments meet tangentially: no corner.
    Tangent,
    /// The segments turn back on themselves (180°).
    Cusp,
    /// The rounding does not fit on the segments; `max`: the largest value that fits.
    TooLarge { max: Option<f64> },
    /// No solution found.
    NoSolution,
}

/// Point and unit tangent of a segment at parameter `t ∈ [0, 1]`.
fn seg_eval(s: &Segment, t: f64) -> (DVec2, DVec2) {
    match s {
        Segment::Line { a, b } => (*a + (*b - *a) * t, (*b - *a).normalize_or_zero()),
        Segment::Arc { center, r, a0, a1 } => {
            let a = a0 + (a1 - a0) * t;
            (polar(*center, *r, a), DVec2::new(-a.sin(), a.cos()) * (a1 - a0).signum())
        }
        Segment::Nurbs(c) => {
            let (lo, hi) = c.domain();
            let u = lo + (hi - lo) * t;
            let p = c.eval(u).truncate();
            let mut d = c.tangent(u).truncate();
            if d.length_squared() < 1e-300 {
                d = if t < 0.5 { s.start_tangent() } else { s.end_tangent() };
            }
            (p, d.normalize_or_zero())
        }
    }
}

/// The piece of a segment over `[t0, t1]` (fractions of its parameter range).
pub fn seg_sub(s: &Segment, t0: f64, t1: f64) -> Segment {
    match s {
        Segment::Line { .. } => Segment::Line { a: seg_eval(s, t0).0, b: seg_eval(s, t1).0 },
        Segment::Arc { center, r, a0, a1 } => {
            let sw = a1 - a0;
            Segment::Arc { center: *center, r: *r, a0: a0 + sw * t0, a1: a0 + sw * t1 }
        }
        Segment::Nurbs(c) => {
            let (lo, hi) = c.domain();
            Segment::Nurbs(c.trimmed(lo + (hi - lo) * t0, lo + (hi - lo) * t1))
        }
    }
}

/// Parameter (fraction) of the point of `s` nearest to `p`.
fn seg_param(s: &Segment, p: DVec2) -> f64 {
    match s {
        Segment::Line { a, b } => {
            let d = *b - *a;
            ((p - *a).dot(d) / d.length_squared().max(1e-300)).clamp(0.0, 1.0)
        }
        Segment::Arc { center, r, a0, a1 } => G::A { c: *center, r: *r, a0: *a0, sw: a1 - a0 }.param_of(p),
        Segment::Nurbs(_) => {
            let n = 200;
            let mut best = (f64::INFINITY, 0.0);
            for i in 0..=n {
                let t = i as f64 / n as f64;
                let d = seg_eval(s, t).0.distance(p);
                if d < best.0 {
                    best = (d, t);
                }
            }
            let (mut x0, mut x1) = ((best.1 - 1.0 / n as f64).max(0.0), (best.1 + 1.0 / n as f64).min(1.0));
            for _ in 0..60 {
                let m0 = x0 + (x1 - x0) / 3.0;
                let m1 = x1 - (x1 - x0) / 3.0;
                if seg_eval(s, m0).0.distance(p) < seg_eval(s, m1).0.distance(p) {
                    x1 = m1;
                } else {
                    x0 = m0;
                }
            }
            (x0 + x1) / 2.0
        }
    }
}

/// Unit left normal at parameter t.
fn seg_normal(s: &Segment, t: f64) -> DVec2 {
    perp(seg_eval(s, t).1)
}

/// Offset curve of a line or arc on the left by `k` (negative: right), as `G`-like data:
/// `Ok(line)` or `Err((centre, radius))`; `None` if the arc's offset radius would be ≤ 0.
enum Off {
    Line(DVec2, DVec2),
    Circle(DVec2, f64),
}

fn offset_curve(s: &Segment, k: f64) -> Option<Off> {
    match s {
        Segment::Line { a, b } => {
            let n = perp((*b - *a).normalize_or_zero());
            Some(Off::Line(*a + n * k, *b - *a))
        }
        Segment::Arc { center, r, a0, a1 } => {
            // left is towards the centre for counter-clockwise arcs
            let rr = if a1 >= a0 { r - k } else { r + k };
            (rr > 0.0).then_some(Off::Circle(*center, rr))
        }
        Segment::Nurbs(_) => None,
    }
}

/// Intersections of two offset curves (infinite lines, full circles).
fn off_hits(a: &Off, b: &Off) -> Vec<DVec2> {
    match (a, b) {
        (Off::Line(p, d), Off::Line(q, e)) => {
            let den = d.perp_dot(*e);
            if den.abs() <= 1e-15 * d.length() * e.length() {
                return Vec::new();
            }
            let t = (*q - *p).perp_dot(*e) / den;
            vec![*p + *d * t]
        }
        (Off::Line(p, d), Off::Circle(c, r)) | (Off::Circle(c, r), Off::Line(p, d)) => {
            let dir = d.normalize_or_zero();
            let foot = *p + dir * (*c - *p).dot(dir);
            let h = foot.distance(*c);
            if h > *r * (1.0 + 1e-12) {
                return Vec::new();
            }
            let k = (r * r - h * h).max(0.0).sqrt();
            vec![foot - dir * k, foot + dir * k]
        }
        (Off::Circle(c1, r1), Off::Circle(c2, r2)) => {
            let dv = *c2 - *c1;
            let dd = dv.length();
            if dd <= 1e-15 || dd > r1 + r2 + 1e-12 * (r1 + r2) || dd < (r1 - r2).abs() - 1e-12 * (r1 + r2) {
                return Vec::new();
            }
            let a = (r1 * r1 - r2 * r2 + dd * dd) / (2.0 * dd);
            let hh = (r1 * r1 - a * a).max(0.0).sqrt();
            let m = *c1 + dv / dd * a;
            let n = perp(dv / dd);
            vec![m + n * hh, m - n * hh]
        }
    }
}

/// Foot of the centre `c` on a line/arc (parameter fraction, unclamped for lines; `None` if
/// outside an arc's sweep).
fn foot_param(s: &Segment, c: DVec2) -> Option<f64> {
    match s {
        Segment::Line { a, b } => {
            let d = *b - *a;
            Some((c - *a).dot(d) / d.length_squared().max(1e-300))
        }
        Segment::Arc { center, a0, a1, .. } => {
            let ang = angle(c - *center);
            sweep_param_tol(ang, *a0, a1 - a0, 1e-12).or_else(|| {
                // the foot may lie just beyond an end: report a value outside [0, 1]
                let sw = a1 - a0;
                let d0 = angdist(ang, *a0);
                let d1 = angdist(ang, *a1);
                Some(if d0 < d1 { -d0 / sw.abs() } else { 1.0 + d1 / sw.abs() })
            })
        }
        Segment::Nurbs(_) => None,
    }
}

/// Rounds or chamfers the corner where `a` ends and `b` starts.
pub fn round_corner(a: &Segment, b: &Segment, kind: CornerKind) -> Result<CornerResult, CornerError> {
    let v = b.start();
    let (ta, tb) = (a.end_tangent(), b.start_tangent());
    let phi = turn(ta, tb);
    if phi.abs() <= 1e-9 {
        return Err(CornerError::Tangent);
    }
    if phi.abs() >= PI - 1e-9 {
        return Err(CornerError::Cusp);
    }
    let r = match kind {
        CornerKind::Round(r) | CornerKind::Chamfer(r) => r,
    };
    if r.is_nan() || r <= 0.0 {
        return Err(CornerError::NoSolution);
    }
    let res = match kind {
        CornerKind::Round(r) => fillet(a, b, r, phi),
        CornerKind::Chamfer(d) => chamfer(a, b, d, v),
    };
    match res {
        Some(x) => Ok(x),
        None => {
            // largest value that still fits (bisection)
            let fits = |x: f64| match kind {
                CornerKind::Round(_) => fillet(a, b, x, phi).is_some(),
                CornerKind::Chamfer(_) => chamfer(a, b, x, v).is_some(),
            };
            let mut lo = 0.0;
            let mut hi = r;
            if !fits(r * 1e-6) {
                return Err(CornerError::TooLarge { max: None });
            }
            for _ in 0..50 {
                let m = (lo + hi) / 2.0;
                if fits(m) {
                    lo = m;
                } else {
                    hi = m;
                }
            }
            Err(CornerError::TooLarge { max: Some(lo) })
        }
    }
}

const PTOL: f64 = 1e-9;

fn finish_corner(
    a: &Segment,
    b: &Segment,
    ua: f64,
    ub: f64,
    mid: Segment,
    center: Option<DVec2>,
    residual: f64,
) -> CornerResult {
    let a2 = (ua > PTOL).then(|| seg_sub(a, 0.0, ua));
    let b2 = (ub < 1.0 - PTOL).then(|| seg_sub(b, ub, 1.0));
    CornerResult { a: a2, mid, b: b2, center, residual, ua, ub }
}

/// Point of a segment at parameter fraction `t`.
pub fn seg_point(s: &Segment, t: f64) -> DVec2 {
    seg_eval(s, t).0
}

/// Result of one corner of [`apply_corners`].
pub type CornerOutcome = Result<CornerResult, CornerError>;

/// A piece of the result of [`apply_corners`]: (part of) input segment `i`, or the rounding
/// of corner `k`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece {
    Seg(usize),
    Corner(usize),
}

/// Rounds the corners `(vertex, kind)` of a chain of segments (`closed`: a loop, vertex 0 is
/// between the last and the first segment; open chains have the vertices 1..n-1). Every
/// corner is computed on the original segments; two corners that need overlapping parts of
/// one segment both fail (`TooLarge` with the largest common scale of their values). Returns
/// the new chain (a closed loop keeps its first segment first, the rounding of vertex 0
/// comes last) and the result of every corner.
pub fn apply_corners(
    segs: &[Segment],
    closed: bool,
    corners: &[(usize, CornerKind)],
) -> (Vec<(Piece, Segment)>, Vec<CornerOutcome>) {
    let n = segs.len();
    let at = |k: usize| -> (usize, usize) {
        let v = corners[k].0;
        ((v + n - 1) % n, v % n)
    };
    let run = |k: usize, kind: CornerKind| {
        let (ia, ib) = at(k);
        round_corner(&segs[ia], &segs[ib], kind)
    };
    let mut res: Vec<Result<CornerResult, CornerError>> = (0..corners.len()).map(|k| run(k, corners[k].1)).collect();
    // overlaps on a segment between the corner at its start and the one at its end
    let scaled = |kind: CornerKind, f: f64| match kind {
        CornerKind::Round(r) => CornerKind::Round(r * f),
        CornerKind::Chamfer(d) => CornerKind::Chamfer(d * f),
    };
    loop {
        let mut start_of = vec![None; n];
        let mut end_of = vec![None; n];
        for (k, r) in res.iter().enumerate() {
            if r.is_ok() {
                let (ia, ib) = at(k);
                end_of[ia] = Some(k);
                start_of[ib] = Some(k);
            }
        }
        let mut conflict = None;
        for i in 0..n {
            if let (Some(ks), Some(ke)) = (start_of[i], end_of[i])
                && ks != ke
                && let (Ok(rs), Ok(re)) = (&res[ks], &res[ke])
                && rs.ub > re.ua + 1e-9
            {
                conflict = Some((ks, ke));
                break;
            }
        }
        let Some((ks, ke)) = conflict else { break };
        // largest common factor with which both fit on the shared segment
        let fits = |f: f64| {
            let (Ok(a), Ok(b)) = (run(ks, scaled(corners[ks].1, f)), run(ke, scaled(corners[ke].1, f))) else {
                return false;
            };
            a.ub <= b.ua + 1e-9
        };
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..50 {
            let m = (lo + hi) / 2.0;
            if fits(m) {
                lo = m;
            } else {
                hi = m;
            }
        }
        for k in [ks, ke] {
            let v = match corners[k].1 {
                CornerKind::Round(r) | CornerKind::Chamfer(r) => r,
            };
            res[k] = Err(CornerError::TooLarge { max: (lo > 0.0).then_some(lo * v) });
        }
    }
    let mut t0 = vec![0.0; n];
    let mut t1 = vec![1.0; n];
    let mut at_vertex: Vec<Option<usize>> = vec![None; n];
    for (k, r) in res.iter().enumerate() {
        if let Ok(r) = r {
            let (ia, ib) = at(k);
            t1[ia] = r.ua;
            t0[ib] = r.ub;
            at_vertex[ib] = Some(k);
        }
    }
    let mut out = Vec::with_capacity(n * 2);
    let mut first = None;
    for i in 0..n {
        if let Some(k) = at_vertex[i]
            && let Ok(r) = &res[k]
        {
            if i == 0 && closed {
                first = Some((Piece::Corner(k), r.mid.clone()));
            } else {
                out.push((Piece::Corner(k), r.mid.clone()));
            }
        }
        if t1[i] - t0[i] > PTOL {
            let s = if t0[i] == 0.0 && t1[i] == 1.0 { segs[i].clone() } else { seg_sub(&segs[i], t0[i], t1[i]) };
            out.push((Piece::Seg(i), s));
        }
    }
    out.extend(first);
    (out, res)
}

fn arc_between(c: DVec2, r: f64, p: DVec2, q: DVec2, ccw: bool) -> Segment {
    let a0 = angle(p - c);
    let a1r = angle(q - c);
    let mut sw = if ccw { (a1r - a0).rem_euclid(TAU) } else { -((a0 - a1r).rem_euclid(TAU)) };
    if sw.abs() < 1e-15 {
        sw = 0.0;
    }
    Segment::Arc { center: c, r, a0, a1: a0 + sw }
}

fn fillet(a: &Segment, b: &Segment, r: f64, phi: f64) -> Option<CornerResult> {
    let s = phi.signum();
    let v = b.start();
    let exact = !matches!(a, Segment::Nurbs(_)) && !matches!(b, Segment::Nurbs(_));
    if exact {
        let oa = offset_curve(a, s * r)?;
        let ob = offset_curve(b, s * r)?;
        let mut best: Option<(f64, CornerResult)> = None;
        for c in off_hits(&oa, &ob) {
            let (Some(ua), Some(ub)) = (foot_param(a, c), foot_param(b, c)) else { continue };
            let tol = 1e-9;
            if !(-tol..=1.0 + tol).contains(&ua) || !(-tol..=1.0 + tol).contains(&ub) {
                continue;
            }
            let (ua, ub) = (ua.clamp(0.0, 1.0), ub.clamp(0.0, 1.0));
            // the centre must be on the turn side of both
            let pa = seg_eval(a, ua).0;
            let pb = seg_eval(b, ub).0;
            if (c - pa).dot(seg_normal(a, ua)) * s <= 0.0 || (c - pb).dot(seg_normal(b, ub)) * s <= 0.0 {
                continue;
            }
            let mid = arc_between(c, r, pa, pb, s > 0.0);
            // the rounding replaces the corner: it turns by less than a full circle
            let Segment::Arc { a0, a1, .. } = mid else { continue };
            if (a1 - a0).abs() >= PI + 1e-9 + phi.abs() {
                continue;
            }
            let d = c.distance(v);
            if best.as_ref().is_none_or(|x| d < x.0) {
                best = Some((d, finish_corner(a, b, ua, ub, mid, Some(c), 0.0)));
            }
        }
        return best.map(|x| x.1);
    }
    // numerical: a(u) + s r n_a(u) = b(w) + s r n_b(w), Newton in (u, w)
    let la = a.length().max(1e-12);
    let lb = b.length().max(1e-12);
    let setback = r * (phi.abs() / 2.0).tan();
    let mut u = (1.0 - setback / la).clamp(0.0, 1.0);
    let mut w = (setback / lb).clamp(0.0, 1.0);
    let f = |u: f64, w: f64| -> DVec2 {
        let (pa, tna) = seg_eval(a, u);
        let (pb, tnb) = seg_eval(b, w);
        (pa + perp(tna) * (s * r)) - (pb + perp(tnb) * (s * r))
    };
    let scale = la.max(lb).max(r);
    let mut res = f(u, w).length();
    for _ in 0..80 {
        if res <= 1e-13 * scale {
            break;
        }
        let h = 1e-7;
        let fu = (f((u + h).min(1.0), w) - f((u - h).max(0.0), w)) / ((u + h).min(1.0) - (u - h).max(0.0));
        let fw = (f(u, (w + h).min(1.0)) - f(u, (w - h).max(0.0))) / ((w + h).min(1.0) - (w - h).max(0.0));
        let fv = f(u, w);
        let det = fu.perp_dot(fw);
        if det.abs() < 1e-300 {
            break;
        }
        // solve [fu fw] [du dw]^T = -fv
        let du = -(fv.perp_dot(fw)) / det;
        let dw = -(fu.perp_dot(fv)) / det;
        let mut step = 1.0;
        let mut improved = false;
        for _ in 0..30 {
            let (nu, nw) = (u + du * step, w + dw * step);
            if !(-0.5..=1.5).contains(&nu) || !(-0.5..=1.5).contains(&nw) {
                step *= 0.5;
                continue;
            }
            let nr = f(nu.clamp(0.0, 1.0), nw.clamp(0.0, 1.0)).length();
            if nr < res {
                u = nu;
                w = nw;
                res = nr;
                improved = true;
                break;
            }
            step *= 0.5;
        }
        if !improved {
            break;
        }
    }
    if !(-PTOL..=1.0 + PTOL).contains(&u) || !(-PTOL..=1.0 + PTOL).contains(&w) || res > 1e-7 * scale {
        return None;
    }
    let (u, w) = (u.clamp(0.0, 1.0), w.clamp(0.0, 1.0));
    let (pa, tna) = seg_eval(a, u);
    let pb = seg_eval(b, w).0;
    let c = pa + perp(tna) * (s * r);
    let mid = arc_between(c, r, pa, pb, s > 0.0);
    Some(finish_corner(a, b, u, w, mid, Some(c), res))
}

/// Parameter on `s` of the point at distance `d` from `v` nearest to the end `at_end`.
fn at_distance(s: &Segment, v: DVec2, d: f64, at_end: bool) -> Option<f64> {
    match s {
        Segment::Line { a, b } => {
            let l = a.distance(*b);
            if d > l * (1.0 + 1e-12) {
                return None;
            }
            let f = (d / l).min(1.0);
            Some(if at_end { 1.0 - f } else { f })
        }
        _ => {
            // walk from the vertex end until the distance reaches d, then bisect
            let n = 400;
            let par = |i: usize| if at_end { 1.0 - i as f64 / n as f64 } else { i as f64 / n as f64 };
            let mut prev = par(0);
            for i in 1..=n {
                let t = par(i);
                if seg_eval(s, t).0.distance(v) >= d {
                    let (mut x0, mut x1) = (prev, t);
                    for _ in 0..80 {
                        let m = (x0 + x1) / 2.0;
                        if seg_eval(s, m).0.distance(v) >= d {
                            x1 = m;
                        } else {
                            x0 = m;
                        }
                    }
                    return Some((x0 + x1) / 2.0);
                }
                prev = t;
            }
            None
        }
    }
}

fn chamfer(a: &Segment, b: &Segment, d: f64, v: DVec2) -> Option<CornerResult> {
    let ua = at_distance(a, v, d, true)?;
    let ub = at_distance(b, v, d, false)?;
    let pa = seg_eval(a, ua).0;
    let pb = seg_eval(b, ub).0;
    Some(finish_corner(a, b, ua, ub, Segment::Line { a: pa, b: pb }, None, 0.0))
}

/// Exposed for the fillet of a whole loop: nearest parameter of a point.
pub fn param_on(s: &Segment, p: DVec2) -> f64 {
    seg_param(s, p)
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    fn square(a: f64) -> Loop2 {
        poly_loop(&[DVec2::ZERO, DVec2::new(a, 0.0), DVec2::new(a, a), DVec2::new(0.0, a)])
    }

    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "{a} vs {b}");
    }

    #[test]
    fn union_of_overlapping_squares() {
        let a = square(10.0);
        let b: Loop2 =
            square(10.0).iter().map(|s| s.map(&glam::DAffine2::from_translation(DVec2::new(5.0, 5.0)))).collect();
        let u = combine(
            &[Operand::new(vec![a.clone()], Fill::NonZero), Operand::new(vec![b.clone()], Fill::NonZero)],
            &union_pred,
        );
        assert_eq!(u.len(), 1);
        close(region_area(&u), 175.0, 1e-9);
        assert_eq!(u[0].len(), 8);
        let d = combine(
            &[Operand::new(vec![a.clone()], Fill::NonZero), Operand::new(vec![b.clone()], Fill::NonZero)],
            &minus_pred,
        );
        close(region_area(&d), 75.0, 1e-9);
        let i =
            combine(&[Operand::new(vec![a], Fill::NonZero), Operand::new(vec![b], Fill::NonZero)], &|x| x[0] && x[1]);
        close(region_area(&i), 25.0, 1e-9);
    }

    #[test]
    fn square_minus_circle_is_exact() {
        let a = square(20.0);
        let c = circle_loop(DVec2::new(10.0, 10.0), 5.0);
        let d = combine(&[Operand::new(vec![a], Fill::NonZero), Operand::new(vec![c], Fill::NonZero)], &minus_pred);
        assert_eq!(d.len(), 2);
        close(region_area(&d), 400.0 - PI * 25.0, 1e-9);
        // circle crossing the edge
        let c = circle_loop(DVec2::new(20.0, 10.0), 5.0);
        let d = combine(
            &[Operand::new(vec![square(20.0)], Fill::NonZero), Operand::new(vec![c], Fill::NonZero)],
            &union_pred,
        );
        assert_eq!(d.len(), 1);
        close(region_area(&d), 400.0 + PI * 25.0 / 2.0, 1e-9);
    }

    #[test]
    fn offset_square_round() {
        for (a, r) in [(10.0, 2.0), (10.0, 0.5), (3.0, 4.0)] {
            let o = offset_round(&[square(a)], r);
            assert_eq!(o.len(), 1);
            close(region_area(&o), (a + 2.0 * r) * (a + 2.0 * r) - (4.0 - PI) * r * r, 1e-9);
            // 4 lines + 4 arcs
            assert_eq!(o[0].len(), 8, "{:?}", o[0]);
        }
        // inward: sharp square
        let o = offset_round(&[square(10.0)], -2.0);
        close(region_area(&o), 36.0, 1e-9);
        // vanishes
        assert!(offset_round(&[square(10.0)], -6.0).is_empty());
    }

    #[test]
    fn offset_square_delta() {
        let o = offset_delta(&[square(10.0)], 2.0, false);
        close(region_area(&o), 196.0, 1e-9);
        assert_eq!(o[0].len(), 4);
        let o = offset_delta(&[square(10.0)], 2.0, true);
        // corner cut at distance 2 from the vertex: removed triangle (2√2 − 2)²
        let t = (2.0 * 2f64.sqrt() - 2.0).powi(2);
        close(region_area(&o), 196.0 - 4.0 * t, 1e-9);
        let o = offset_delta(&[square(10.0)], -2.0, false);
        close(region_area(&o), 36.0, 1e-9);
    }

    #[test]
    fn offset_circle_and_hole() {
        let ring = vec![
            circle_loop(DVec2::ZERO, 10.0),
            circle_loop(DVec2::ZERO, 4.0).iter().rev().map(|s| s.reversed()).collect(),
        ];
        close(region_area(&ring), PI * (100.0 - 16.0), 1e-9);
        let o = offset_round(&ring, 1.0);
        close(region_area(&o), PI * (121.0 - 9.0), 1e-9);
        let o = offset_round(&ring, -1.0);
        close(region_area(&o), PI * (81.0 - 25.0), 1e-9);
        // hole closes
        let o = offset_round(&ring, 4.5);
        assert_eq!(o.len(), 1);
        close(region_area(&o), PI * 14.5 * 14.5, 1e-9);
    }

    #[test]
    fn stroke_of_an_arc() {
        let (r, w) = (10.0, 2.0);
        let arc = Segment::Arc { center: DVec2::ZERO, r, a0: 0.0, a1: PI / 2.0 };
        let ring_sector = PI / 2.0 / 2.0 * ((r + w / 2.0).powi(2) - (r - w / 2.0).powi(2));
        let f = stroke(&[(vec![arc.clone()], false)], w, Cap::Flat, Join::Round);
        close(region_area(&f), ring_sector, 1e-9);
        let f = stroke(&[(vec![arc.clone()], false)], w, Cap::Round, Join::Round);
        close(region_area(&f), ring_sector + PI * (w / 2.0).powi(2), 1e-9);
        let f = stroke(&[(vec![arc], false)], w, Cap::Square, Join::Round);
        close(region_area(&f), ring_sector + w * w, 1e-9);
    }

    #[test]
    fn stroke_of_a_polyline_joins() {
        let p = vec![
            Segment::Line { a: DVec2::ZERO, b: DVec2::new(10.0, 0.0) },
            Segment::Line { a: DVec2::new(10.0, 0.0), b: DVec2::new(10.0, 10.0) },
        ];
        let w = 2.0;
        // flat caps: two 10×2 bands overlapping in a 1×1 square at the inner side; the outer
        // corner square 1×1 (miter), quarter disc (round) or triangle (bevel)
        let bands = 20.0 + 20.0 - 1.0;
        close(region_area(&stroke(&[(p.clone(), false)], w, Cap::Flat, Join::Miter)), bands + 1.0, 1e-9);
        close(region_area(&stroke(&[(p.clone(), false)], w, Cap::Flat, Join::Bevel)), bands + 0.5, 1e-9);
        close(region_area(&stroke(&[(p, false)], w, Cap::Flat, Join::Round)), bands + PI / 4.0, 1e-9);
    }

    #[test]
    fn fillet_line_line() {
        let a = Segment::Line { a: DVec2::new(0.0, 0.0), b: DVec2::new(10.0, 0.0) };
        let b = Segment::Line { a: DVec2::new(10.0, 0.0), b: DVec2::new(10.0, 10.0) };
        let c = round_corner(&a, &b, CornerKind::Round(2.0)).unwrap();
        let Segment::Arc { center, r, .. } = c.mid else { panic!() };
        assert!((center - DVec2::new(8.0, 2.0)).length() < 1e-12);
        close(r, 2.0, 1e-12);
        assert!((c.a.unwrap().end() - DVec2::new(8.0, 0.0)).length() < 1e-12);
        assert!((c.b.unwrap().start() - DVec2::new(10.0, 2.0)).length() < 1e-12);
        // too large
        let e = round_corner(&a, &b, CornerKind::Round(11.0)).unwrap_err();
        let CornerError::TooLarge { max: Some(m) } = e else { panic!("{e:?}") };
        close(m, 10.0, 1e-6);
        // chamfer
        let c = round_corner(&a, &b, CornerKind::Chamfer(3.0)).unwrap();
        assert!((c.mid.start() - DVec2::new(7.0, 0.0)).length() < 1e-12);
        // tangent
        let t = Segment::Line { a: DVec2::new(10.0, 0.0), b: DVec2::new(20.0, 0.0) };
        assert_eq!(round_corner(&a, &t, CornerKind::Round(1.0)).unwrap_err(), CornerError::Tangent);
    }

    #[test]
    fn fillet_line_arc_and_arc_arc() {
        // line into a quarter circle (convex corner at (10, 0) turning left)
        let a = Segment::Line { a: DVec2::new(0.0, 0.0), b: DVec2::new(10.0, 0.0) };
        let b = Segment::Arc { center: DVec2::new(0.0, 0.0), r: 10.0, a0: 0.0, a1: PI / 2.0 };
        let c = round_corner(&a, &b, CornerKind::Round(1.0)).unwrap();
        let Segment::Arc { center, .. } = c.mid else { panic!() };
        // tangent to the line (y = 1) and internally to the circle (|c| = 9)
        close(center.y, 1.0, 1e-12);
        close(center.length(), 9.0, 1e-12);
        // two arcs meeting at a corner
        let a = Segment::Arc { center: DVec2::new(0.0, 0.0), r: 10.0, a0: -PI / 2.0, a1: 0.0 };
        let b = Segment::Arc { center: DVec2::new(10.0, 10.0), r: 10.0, a0: -PI / 2.0, a1: 0.0 };
        let c = round_corner(&a, &b, CornerKind::Round(2.0)).unwrap();
        let center = c.center.unwrap();
        // right turn, the rounding outside both circles: distance 12 from both centres
        close(center.distance(DVec2::ZERO), 12.0, 1e-9);
        close(center.distance(DVec2::new(10.0, 10.0)), 12.0, 1e-9);
    }

    #[test]
    fn fillet_bezier_numerically() {
        let a = Segment::Line { a: DVec2::new(0.0, 0.0), b: DVec2::new(10.0, 0.0) };
        let b = Segment::bezier(&[DVec2::new(10.0, 0.0), DVec2::new(10.0, 5.0), DVec2::new(15.0, 10.0)]);
        let c = round_corner(&a, &b, CornerKind::Round(1.0)).unwrap();
        assert!(c.residual < 1e-9, "{}", c.residual);
        let center = c.center.unwrap();
        close(center.y, 1.0, 1e-9);
        // distance to the curve is r
        let bb = c.b.unwrap();
        let d = bb.sample(1e-6).iter().map(|p| p.distance(center)).fold(f64::INFINITY, f64::min);
        close(d, 1.0, 1e-6);
        // a line against a straight Bézier gives the line–line result
        let b2 = Segment::bezier(&[DVec2::new(10.0, 0.0), DVec2::new(10.0, 5.0), DVec2::new(10.0, 10.0)]);
        let c = round_corner(&a, &b2, CornerKind::Round(2.0)).unwrap();
        assert!((c.center.unwrap() - DVec2::new(8.0, 2.0)).length() < 1e-7);
    }

    #[test]
    fn rings_of_arcs_of_any_sweep() {
        for (sweep, n) in [(180.0f64, 2), (120.0, 3), (90.0, 4), (360.0, 1), (72.0, 5)] {
            let ring = |r: f64, s: f64| -> Loop2 {
                (0..n)
                    .map(|i| {
                        let a0 = (s * sweep * i as f64).to_radians();
                        Segment::Arc { center: DVec2::ZERO, r, a0, a1: a0 + (s * sweep).to_radians() }
                    })
                    .collect()
            };
            let outer = Operand::new(vec![ring(18.0, 1.0)], Fill::NonZero);
            let inner = Operand::new(vec![ring(14.8, -1.0)], Fill::NonZero);
            let d = combine(&[outer, inner], &minus_pred);
            close(region_area(&d), PI * (18.0 * 18.0 - 14.8 * 14.8), 1e-9);
        }
    }

    #[test]
    fn biarcs_follow_the_curve() {
        let b = Segment::bezier(&[
            DVec2::new(0.0, 0.0),
            DVec2::new(5.0, 10.0),
            DVec2::new(10.0, -10.0),
            DVec2::new(15.0, 0.0),
        ]);
        let arcs = b.to_arcs(1e-4);
        assert!(arcs.len() >= 2);
        for w in arcs.windows(2) {
            assert!(w[0].end().distance(w[1].start()) < 1e-9);
            // G1
            assert!(w[0].end_tangent().distance(w[1].start_tangent()) < 1e-6);
        }
        for p in b.sample(1e-5) {
            let d = arcs.iter().map(|s| crate::curve2::dist_to_seg(s, p)).fold(f64::INFINITY, f64::min);
            assert!(d < 2e-4, "{d}");
        }
    }
}
