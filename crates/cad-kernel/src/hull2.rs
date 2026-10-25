//! Exact convex hull of planar sets bounded by lines and circular arcs (`hull()` in 2D).
//!
//! The hull is traced through its support function `h(θ) = max_x x·u(θ)` (`u(θ)` the unit
//! vector at angle θ, the outward normal of the hull boundary). Every input segment becomes
//! *sites*: the end points of lines and arcs (points, radius 0) and the arcs themselves (a circle
//! `c, r` that counts only for normals within its angular range, where its support is
//! `c·u + r`; outside the range its end points take over). Going round θ, the site with the
//! largest support changes where two support functions cross: at the common tangent of two
//! sites (closed form, see [`overtake`]) or where an arc's range ends. Between two crossings
//! the hull boundary is a vertex (point site) or a piece of an arc; at a crossing it follows the
//! tangent line from one site to the next. So the hull consists of pieces of the input arcs and
//! of lines: exact for lines and arcs, B-splines enter as biarcs ([`Segment::to_arcs`]).
//!
//! Concave arcs need no special case: their support is evaluated like that of any arc and
//! never wins where the region's other points lie beyond them (only their chord counts).

use std::f64::consts::{PI, TAU};

use glam::DVec2;

use crate::curve2::Segment;
use crate::region2::{Loop2, arc_tol, loop_area};

#[derive(Debug, Clone, Copy)]
struct Site {
    c: DVec2,
    r: f64,
    /// Angular range of the outward normals (the arc's points): start and sweep (> 0, `TAU`
    /// for points and full circles).
    a: f64,
    sw: f64,
}

impl Site {
    fn full(&self) -> bool {
        self.sw >= TAU - 1e-12
    }

    fn in_range(&self, th: f64) -> bool {
        self.full() || (th - self.a).rem_euclid(TAU) <= self.sw + 1e-12
    }

    fn support(&self, th: f64) -> f64 {
        if self.in_range(th) { self.c.dot(u(th)) + self.r } else { f64::NEG_INFINITY }
    }

    fn at(&self, th: f64) -> DVec2 {
        self.c + u(th) * self.r
    }
}

fn u(th: f64) -> DVec2 {
    DVec2::new(th.cos(), th.sin())
}

fn ang(v: DVec2) -> f64 {
    v.y.atan2(v.x)
}

/// Size of the input (bounding box diagonal).
fn size_of(loops: &[Loop2]) -> f64 {
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for s in loops.iter().flatten() {
        let ext = match s {
            Segment::Arc { center, r, .. } => vec![*center - DVec2::splat(*r), *center + DVec2::splat(*r)],
            _ => vec![s.start(), s.end()],
        };
        for p in ext {
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    if lo.x.is_finite() { (hi - lo).length().max(1e-9) } else { 1.0 }
}

/// Points and arcs of the input (B-splines as biarcs), duplicates removed, arcs on the same
/// circle merged, points inside the hull of the points dropped.
fn sites(loops: &[Loop2], tol: f64) -> Vec<Site> {
    let atol = arc_tol(size_of(loops));
    let mut pts: Vec<DVec2> = Vec::new();
    // circles (centre, radius) with the angular ranges of their arcs
    type Arcs = Vec<(DVec2, f64, Vec<(f64, f64)>)>;
    let mut arcs: Arcs = Vec::new();
    for s in loops.iter().flatten() {
        for g in s.to_arcs(atol) {
            pts.push(g.start());
            pts.push(g.end());
            if let Segment::Arc { center, r, a0, a1 } = g {
                if r <= tol || (a1 - a0).abs() <= 1e-12 {
                    continue;
                }
                let (lo, sw) = if a1 >= a0 { (a0, a1 - a0) } else { (a1, a0 - a1) };
                let (lo, sw) = (lo.rem_euclid(TAU), sw.min(TAU));
                match arcs.iter_mut().find(|(c, rr, _)| c.distance(center) <= tol && (rr - r).abs() <= tol) {
                    Some((_, _, rs)) => rs.push((lo, sw)),
                    None => arcs.push((center, r, vec![(lo, sw)])),
                }
            }
        }
    }
    let mut out: Vec<Site> = Vec::new();
    for p in hull_points(&pts, tol) {
        out.push(Site { c: p, r: 0.0, a: 0.0, sw: TAU });
    }
    for (c, r, ranges) in arcs {
        for (a, sw) in merge_ranges(ranges) {
            out.push(Site { c, r, a, sw });
        }
    }
    out
}

/// Union of angular ranges `(start in [0, 2π), sweep)`: disjoint ranges, a full circle as one.
fn merge_ranges(mut rs: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    const EPS: f64 = 1e-9;
    if rs.iter().any(|r| r.1 >= TAU - EPS) {
        return vec![(0.0, TAU)];
    }
    rs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (a, sw) in rs {
        match out.last_mut() {
            Some(last) if a <= last.0 + last.1 + EPS => {
                last.1 = last.1.max(a + sw - last.0);
            }
            _ => out.push((a, sw)),
        }
    }
    // the last range may reach round to the first
    if out.len() > 1 {
        let (f, l) = (out[0], out[out.len() - 1]);
        if l.0 + l.1 >= f.0 + TAU - EPS {
            let end = (f.0 + TAU + f.1).max(l.0 + l.1);
            out.pop();
            out[0] = (l.0, end - l.0);
        }
    }
    for r in &mut out {
        if r.1 >= TAU - EPS {
            return vec![(0.0, TAU)];
        }
    }
    out
}

/// Vertices of the convex hull of points (monotone chain, collinear points dropped).
fn hull_points(pts: &[DVec2], tol: f64) -> Vec<DVec2> {
    let mut p: Vec<DVec2> = pts.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup_by(|a, b| a.distance(*b) <= tol);
    if p.len() <= 2 {
        return p;
    }
    let cross = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o);
    let mut h: Vec<DVec2> = Vec::with_capacity(2 * p.len());
    for pass in 0..2 {
        let start = h.len();
        let it: Box<dyn Iterator<Item = &DVec2>> =
            if pass == 0 { Box::new(p.iter()) } else { Box::new(p.iter().rev()) };
        for &q in it {
            while h.len() >= start + 2 && cross(h[h.len() - 2], h[h.len() - 1], q) <= tol * tol {
                h.pop();
            }
            h.push(q);
        }
        h.pop();
    }
    h
}

/// Angle at which site `t`'s support overtakes `s`'s (crossing from below), if any: at the
/// common tangent `φ - acos(-Δr / L)` (`φ` direction from `s` to `t`, `L` distance of the
/// centres, `Δr = r_t - r_s`). A point on an arc's circle gives the point's own angle (exact
/// instead of `acos(±1)`); where `t` contains `s`, the start of `t`'s range.
fn overtake(s: &Site, t: &Site, tol: f64) -> Option<f64> {
    let d = t.c - s.c;
    let l = d.length();
    let dr = t.r - s.r;
    if l <= tol {
        // concentric: the larger one wins where it counts
        return (dr > tol && !t.full()).then_some(t.a);
    }
    if s.r == 0.0 && t.r > 0.0 && (l - t.r).abs() <= tol {
        // a point on the circle of arc t: t takes over at the point
        return Some(ang(-d));
    }
    if t.r == 0.0 && s.r > 0.0 && (l - s.r).abs() <= tol {
        // arc s hands over to a point on its circle
        return Some(ang(d));
    }
    if dr >= l {
        return (!t.full()).then_some(t.a);
    }
    if dr <= -l {
        return None;
    }
    Some(ang(d) - (-dr / l).clamp(-1.0, 1.0).acos())
}

/// Convex hull of the loops (any orientation, holes irrelevant): one counter-clockwise loop of
/// lines and arcs, empty if the hull has no area (a point, collinear points).
pub fn hull(loops: &[Loop2]) -> Loop2 {
    let size = size_of(loops);
    let tol = 1e-9 * size;
    let sites = sites(loops, tol);
    if sites.is_empty() {
        return Vec::new();
    }
    let eta = 1e-7;
    let th0 = 0.1;
    let best_at = |th: f64, skip: Option<usize>| -> usize {
        let mut best = 0usize;
        let mut bv = f64::NEG_INFINITY;
        for (i, s) in sites.iter().enumerate() {
            if Some(i) == skip {
                continue;
            }
            let v = s.support(th);
            if v > bv {
                bv = v;
                best = i;
            }
        }
        best
    };
    let start = best_at(th0 + eta, None);
    // pieces: (site, normal angle where it starts)
    let mut pieces: Vec<(usize, f64)> = vec![(start, th0)];
    let mut cur = start;
    let mut th = th0;
    let limit = 4 * sites.len() + 16;
    for _ in 0..limit {
        let s = sites[cur];
        let mut next: Option<(f64, usize, f64)> = None; // (offset, site, support after)
        let mut consider = |off: f64, t: usize, after: f64| {
            if off <= 1e-12 {
                return;
            }
            match next {
                Some((o, _, a)) if off > o + 1e-10 || (off >= o - 1e-10 && after <= a) => {}
                _ => next = Some((off, t, after)),
            }
        };
        for (i, t) in sites.iter().enumerate() {
            if i == cur {
                continue;
            }
            let Some(a) = overtake(&s, t, tol) else { continue };
            let off = (a - th).rem_euclid(TAU);
            let at = th + off;
            let after = t.support(at + eta);
            if after > s.support(at + eta) && t.in_range(at + 0.5 * eta) {
                consider(off, i, after);
            }
        }
        if !s.full() {
            // the end of the current arc's range: whoever is largest after it
            let end = s.a + s.sw;
            let off = (end - th).rem_euclid(TAU);
            let at = th + off;
            let t = best_at(at + eta, Some(cur));
            consider(off, t, sites[t].support(at + eta));
        }
        let Some((off, t, _)) = next else { break };
        if th + off >= th0 + TAU - 1e-12 {
            break;
        }
        th += off;
        cur = t;
        pieces.push((t, th));
    }
    // the start site's piece is cut at th0: one piece
    if pieces.len() > 1 && pieces[pieces.len() - 1].0 == pieces[0].0 {
        let (_, t_last) = pieces.pop().unwrap_or_default();
        pieces[0].1 = t_last - TAU;
    }
    let end = pieces[0].1 + TAU;
    build(&sites, &pieces, end, tol)
}

fn build(sites: &[Site], pieces: &[(usize, f64)], end: f64, tol: f64) -> Loop2 {
    let mut out: Loop2 = Vec::new();
    let n = pieces.len();
    for (k, &(si, th_in)) in pieces.iter().enumerate() {
        let th_out = if k + 1 < n { pieces[k + 1].1 } else { end };
        let s = sites[si];
        if s.r > 0.0 && (th_out - th_in) * s.r > tol {
            if th_out - th_in >= TAU - 1e-9 {
                out.push(Segment::Arc { center: s.c, r: s.r, a0: th_in, a1: th_in + PI });
                out.push(Segment::Arc { center: s.c, r: s.r, a0: th_in + PI, a1: th_in + TAU });
            } else {
                out.push(Segment::Arc { center: s.c, r: s.r, a0: th_in, a1: th_out });
            }
        }
        // tangent line to the next site
        let (ni, _) = pieces[(k + 1) % n];
        let p = s.at(th_out);
        let q = sites[ni].at(th_out);
        if p.distance(q) > tol {
            out.push(Segment::Line { a: p, b: q });
        }
    }
    // exact joints: lines start and end where their neighbours do
    let m = out.len();
    for i in 0..m {
        let prev_end = out[(i + m - 1) % m].end();
        let next_start = out[(i + 1) % m].start();
        if let Segment::Line { a, b } = &mut out[i] {
            *a = prev_end;
            *b = next_start;
        }
    }
    if out.len() < 2 || loop_area(&out) <= tol * tol * 1e3 {
        return Vec::new();
    }
    out
}

/// Is the region (outer loops counter-clockwise, holes clockwise) convex: one loop whose area is
/// that of its hull?
pub fn is_convex(region: &[Loop2]) -> bool {
    if region.len() != 1 {
        return false;
    }
    let a = loop_area(&region[0]);
    let h = hull(region);
    let ah = loop_area(&h);
    a > 0.0 && (ah - a).abs() <= 1e-9 * ah.max(1e-12) + 1e-12
}

/// Minkowski sum of a convex polygon (its vertices) and a convex region: the hull of the region
/// moved to every vertex. Exact (lines and arcs of the region, moved).
pub fn minkowski_convex(vertices: &[DVec2], region: &[Loop2]) -> Loop2 {
    let mut moved: Vec<Loop2> = Vec::with_capacity(vertices.len() * region.len());
    for v in vertices {
        let m = glam::DAffine2::from_translation(*v);
        for lp in region {
            moved.push(lp.iter().map(|s| s.map(&m)).collect());
        }
    }
    hull(&moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region2::region_area;

    fn circle(c: DVec2, r: f64) -> Loop2 {
        vec![Segment::Arc { center: c, r, a0: 0.0, a1: PI }, Segment::Arc { center: c, r, a0: PI, a1: TAU }]
    }

    fn poly(p: &[DVec2]) -> Loop2 {
        (0..p.len()).map(|i| Segment::Line { a: p[i], b: p[(i + 1) % p.len()] }).collect()
    }

    fn v(x: f64, y: f64) -> DVec2 {
        DVec2::new(x, y)
    }

    #[track_caller]
    fn close(a: f64, b: f64, rel: f64) {
        assert!((a - b).abs() <= rel * b.abs().max(1.0), "{a} vs {b}");
    }

    /// Connected, counter-clockwise, convex.
    #[track_caller]
    fn check_loop(h: &Loop2) {
        assert!(!h.is_empty());
        for i in 0..h.len() {
            let (a, b) = (&h[i], &h[(i + 1) % h.len()]);
            assert!(a.end().distance(b.start()) < 1e-9, "gap at {i}: {h:?}");
            assert!(crate::region2::turn(a.end_tangent(), b.start_tangent()) >= -1e-9, "concave at {i}");
            if let Segment::Arc { a0, a1, .. } = a {
                assert!(a1 > a0, "clockwise arc");
            }
        }
        assert!(loop_area(h) > 0.0);
    }

    fn kinds(h: &Loop2) -> (usize, usize) {
        let arcs = h.iter().filter(|s| matches!(s, Segment::Arc { .. })).count();
        (h.len() - arcs, arcs)
    }

    #[test]
    fn stadium_of_two_equal_circles() {
        let (r, l) = (3.0, 10.0);
        let h = hull(&[circle(v(0.0, 0.0), r), circle(v(l, 0.0), r)]);
        check_loop(&h);
        close(loop_area(&h), PI * r * r + 2.0 * r * l, 1e-12);
        assert_eq!(kinds(&h), (2, 2));
    }

    #[test]
    fn two_circles_of_different_radii() {
        // belt around two pulleys: area = sector parts + trapezoid of the tangents
        let (r1, r2, d) = (5.0, 2.0, 12.0);
        let h = hull(&[circle(v(0.0, 0.0), r1), circle(v(d, 0.0), r2)]);
        check_loop(&h);
        let a = ((r1 - r2) / d).acos(); // angle at the centres between axis and tangent point
        let t = (d * d - (r1 - r2).powi(2)).sqrt(); // tangent length
        let want = r1 * r1 * (PI - a) + r2 * r2 * a + t * (r1 + r2);
        close(loop_area(&h), want, 1e-12);
        assert_eq!(kinds(&h), (2, 2));
        // the same with the circles swapped, and moved
        let h = hull(&[circle(v(d + 3.0, 7.0), r2), circle(v(3.0, 7.0), r1)]);
        close(region_area(&[h]), want, 1e-12);
    }

    #[test]
    fn square_and_circle() {
        let sq = poly(&[v(0.0, 0.0), v(10.0, 0.0), v(10.0, 10.0), v(0.0, 10.0)]);
        // circle centred on the right edge's midpoint, radius 5: half disc outside
        let h = hull(&[sq.clone(), circle(v(10.0, 5.0), 5.0)]);
        check_loop(&h);
        close(loop_area(&h), 100.0 + PI * 25.0 / 2.0, 1e-12);
        // circle away from the square: the tangents from the far corners
        let h = hull(&[sq, circle(v(25.0, 5.0), 3.0)]);
        check_loop(&h);
        let (lines, arcs) = kinds(&h);
        assert_eq!(arcs, 1);
        assert_eq!(lines, 5);
        // the hull of the square's corners and the circle, against the polygon of a fine
        // circle (inscribed: a little smaller)
        let mut pts = vec![v(0.0, 0.0), v(10.0, 0.0), v(10.0, 10.0), v(0.0, 10.0)];
        for i in 0..20000 {
            pts.push(v(25.0, 5.0) + u(TAU * i as f64 / 20000.0) * 3.0);
        }
        let hp = hull_points(&pts, 1e-12);
        let a_poly = loop_area(&poly(&hp));
        let a = region_area(&[h]);
        assert!(a > a_poly && a - a_poly < 1e-5, "{a} {a_poly}");
    }

    #[test]
    fn degenerate_inputs() {
        // one point, collinear points: no area
        assert!(hull(&[poly(&[v(1.0, 1.0), v(1.0, 1.0), v(1.0, 1.0)])]).is_empty());
        assert!(hull(&[poly(&[v(0.0, 0.0), v(5.0, 0.0), v(10.0, 0.0)])]).is_empty());
        // one circle: the circle
        let h = hull(&[circle(v(2.0, 3.0), 4.0)]);
        check_loop(&h);
        close(loop_area(&h), PI * 16.0, 1e-12);
        assert_eq!(kinds(&h), (0, 2));
        // circle in circle, equal circles
        let h = hull(&[circle(v(0.0, 0.0), 5.0), circle(v(1.0, 1.0), 2.0)]);
        close(region_area(&[h]), PI * 25.0, 1e-12);
        let h = hull(&[circle(v(0.0, 0.0), 5.0), circle(v(0.0, 0.0), 5.0)]);
        close(region_area(&[h]), PI * 25.0, 1e-12);
        // internally tangent circles
        let h = hull(&[circle(v(0.0, 0.0), 5.0), circle(v(3.0, 0.0), 2.0)]);
        close(region_area(&[h]), PI * 25.0, 1e-12);
    }

    #[test]
    fn concave_arc_only_its_chord_counts() {
        // square 10 with a semicircular bite (r 3) from the top edge
        let bite = Segment::Arc { center: v(5.0, 10.0), r: 3.0, a0: 0.0, a1: -PI };
        let lp = vec![
            Segment::Line { a: v(0.0, 0.0), b: v(10.0, 0.0) },
            Segment::Line { a: v(10.0, 0.0), b: v(10.0, 10.0) },
            Segment::Line { a: v(10.0, 10.0), b: v(8.0, 10.0) },
            bite,
            Segment::Line { a: v(2.0, 10.0), b: v(0.0, 10.0) },
            Segment::Line { a: v(0.0, 10.0), b: v(0.0, 0.0) },
        ];
        assert!(loop_area(&lp) < 100.0);
        let h = hull(&[lp]);
        check_loop(&h);
        close(loop_area(&h), 100.0, 1e-12);
        assert_eq!(kinds(&h), (4, 0));
    }

    #[test]
    fn convex_arc_pieces_and_rounded_rectangle() {
        // rounded rectangle from four quarter arcs and lines: its own hull
        let (w, hh, r) = (20.0, 10.0, 2.0);
        let mut lp = Vec::new();
        let cs = [v(w - r, r), v(w - r, hh - r), v(r, hh - r), v(r, r)];
        for (i, c) in cs.iter().enumerate() {
            let a0 = -PI / 2.0 + i as f64 * PI / 2.0;
            lp.push(Segment::Arc { center: *c, r, a0, a1: a0 + PI / 2.0 });
            let next = cs[(i + 1) % 4];
            lp.push(Segment::Line { a: *c + u(a0 + PI / 2.0) * r, b: next + u(a0 + PI / 2.0) * r });
        }
        let h = hull(&[lp.clone()]);
        check_loop(&h);
        close(loop_area(&h), w * hh - (4.0 - PI) * r * r, 1e-12);
        assert_eq!(kinds(&h), (4, 4));
        // reversed input (clockwise)
        let rev: Loop2 = lp.iter().rev().map(|s| s.reversed()).collect();
        close(region_area(&[hull(&[rev])]), w * hh - (4.0 - PI) * r * r, 1e-12);
    }

    #[test]
    fn many_points_and_circles() {
        // ring of 12 circles: the hull area against the formula of a "rounded polygon":
        // polygon of the centres + perimeter * r + pi r^2
        let (big, r) = (20.0, 3.0);
        let n = 12;
        let centres: Vec<DVec2> = (0..n).map(|i| u(TAU * i as f64 / n as f64) * big).collect();
        let loops: Vec<Loop2> = centres.iter().map(|c| circle(*c, r)).collect();
        let h = hull(&loops);
        check_loop(&h);
        let a_poly = loop_area(&poly(&centres));
        let per: f64 = (0..n).map(|i| centres[i].distance(centres[(i + 1) % n])).sum();
        close(loop_area(&h), a_poly + per * r + PI * r * r, 1e-12);
        assert_eq!(kinds(&h), (12, 12));
    }

    #[test]
    fn random_sets_against_sampled_hulls() {
        let mut seed = 12345u64;
        let mut rnd = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        for case in 0..200 {
            let mut loops = Vec::new();
            let mut samples = Vec::new();
            let n = 1 + (rnd() * 6.0) as usize;
            for _ in 0..n {
                let c = v(rnd() * 40.0 - 20.0, rnd() * 40.0 - 20.0);
                if rnd() < 0.3 {
                    let q = c + v(rnd() * 5.0, rnd() * 5.0);
                    let w = c + v(-rnd() * 5.0, rnd() * 5.0);
                    loops.push(poly(&[c, q, w]));
                    samples.extend([c, q, w]);
                } else {
                    let r = 0.5 + rnd() * 8.0;
                    if rnd() < 0.5 {
                        loops.push(circle(c, r));
                        samples.extend((0..4000).map(|i| c + u(TAU * i as f64 / 4000.0) * r));
                    } else {
                        // an arc and its chord (a circular segment)
                        let a0 = rnd() * TAU;
                        let sw = 0.3 + rnd() * 5.0;
                        let arc = Segment::Arc { center: c, r, a0, a1: a0 + sw };
                        loops.push(vec![arc.clone(), Segment::Line { a: arc.end(), b: arc.start() }]);
                        samples.extend((0..=4000).map(|i| c + u(a0 + sw * i as f64 / 4000.0) * r));
                    }
                }
            }
            let h = hull(&loops);
            let hp = hull_points(&samples, 1e-12);
            let a_poly = if hp.len() >= 3 { loop_area(&poly(&hp)) } else { 0.0 };
            if a_poly < 1e-6 {
                continue;
            }
            check_loop(&h);
            let a = region_area(&[h]);
            assert!(a >= a_poly - 1e-9 && a - a_poly < 1e-6 * a_poly.max(1.0), "case {case}: {a} vs {a_poly}");
        }
    }

    #[test]
    fn minkowski_of_convex_polygons() {
        let sq = poly(&[v(0.0, 0.0), v(10.0, 0.0), v(10.0, 10.0), v(0.0, 10.0)]);
        let tri = [v(0.0, 0.0), v(4.0, 0.0), v(0.0, 3.0)];
        assert!(is_convex(std::slice::from_ref(&sq)));
        // area of A + B = A + B + mixed area 2 V(A, B); square + triangle: 100 + 6 + (perimeter
        // of the square's support over the triangle) = 100 + 6 + 10 * (4 + 3) = 176
        let m = minkowski_convex(&tri, std::slice::from_ref(&sq));
        check_loop(&m);
        close(loop_area(&m), 176.0, 1e-12);
        // convex polygon + circle = rounded offset
        let m =
            minkowski_convex(&[v(0.0, 0.0), v(10.0, 0.0), v(10.0, 10.0), v(0.0, 10.0)], &[circle(v(0.0, 0.0), 2.0)]);
        close(region_area(&[m]), 100.0 + 40.0 * 2.0 + PI * 4.0, 1e-12);
        let l = poly(&[v(0.0, 0.0), v(10.0, 0.0), v(10.0, 2.0), v(2.0, 2.0), v(2.0, 10.0), v(0.0, 10.0)]);
        assert!(!is_convex(&[l]));
    }
}
