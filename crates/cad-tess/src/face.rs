//! One face in its parameter plane (plan 2.3, 2.4): a constrained Delaunay triangulation (crate
//! `spade`) of the boundary loops, with grid points inside for curved faces, refined where the
//! triangles leave the surface.
//!
//! 1. **Grid.** Lines in u and v whose step keeps the chord within `tol` (half of it in each
//!    direction where both are curved) and the turn within the angle: analytic surfaces from
//!    their radii (straight directions get no lines), revolutions and extrusions from their
//!    curve, B-splines per knot span from the second derivatives; where one direction is
//!    straight over part of the face only, lines there at most four times the other
//!    direction's spacing; a straight direction gets rows only where the boundary runs across
//!    the grid; no ring about a pole nearer to it than the boundary's first point on a run
//!    along the rings (`clear_poles`). Grid points inside the loops (even-odd per row) and not
//!    too near them are inserted.
//! 2. **CDT** in (u, v) mapped to grid steps (`Warp`: a step of either direction is the same
//!    length, so Delaunay triangles are round for the chord error, long along a gently curved
//!    direction and short across a tightly curved one), a straight direction squashed by four
//!    (Delaunay then joins points along it, not across the curved one), the loops as
//!    constraint edges. Triangles inside are those an odd number of constraint edges away from
//!    the outside (nested loops).
//! 3. **Refinement.** A triangle whose centroid lies farther than `tol` from the surface gets
//!    the midpoint of its longest inner edge (or the centroid's foot), an inner edge whose
//!    midpoint lies farther than `tol` the foot of its midpoint, and a triangle facing against
//!    the surface its centroid; a point that would land on the boundary is replaced by the
//!    second choice. Passes repeat while they add points (at most `max_passes`); each measures
//!    only the triangles and edges that are new since the last. Boundary edges are never split
//!    (their points are the edges', shared with the neighbour).
//! 4. **Folds.** Every triangle is checked against the surface normal at its centroid, and one
//!    facing against it (a fold) or more than 60° off the normal at a corner is repaired by
//!    flipping an inner edge (`repair_folds`); folds left make `tessellate` fail.
//!
//! Planar faces take only the first two steps without grid (`σ` is affine, nothing to refine).

use glam::{DVec2, DVec3};
use spade::handles::FixedVertexHandle;
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};

use crate::edges::{step_angle, steps};
use crate::surface::Geom;
use crate::uv::UvPt;

/// A vertex of a face's triangulation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FVert {
    pub uv: DVec2,
    pub p: DVec3,
    /// Boundary point id; `None` for points inside the face.
    pub id: Option<u32>,
    /// On a pole (its `uv` matters for the normal only).
    pub pole: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FaceMesh {
    pub verts: Vec<FVert>,
    /// Counter-clockwise in (u, v).
    pub tris: Vec<[u32; 3]>,
    /// Parameter bounds of the loops.
    pub bounds: (DVec2, DVec2),
    /// Boundary segments the CDT could not insert (loops crossing in (u, v)).
    pub lost_constraints: usize,
    /// Boundary segments split where the loops cross in (u, v).
    pub splits: Vec<Split>,
    /// Points inserted by the refinement.
    pub refined: usize,
    /// Largest deviation measured on the final mesh.
    pub deviation: f64,
    /// The tolerance needs more triangles than allowed (`Params::max_triangles`): about this
    /// many when it stopped.
    pub too_fine: Option<usize>,
    /// Triangles still facing against the surface after `repair_folds`.
    pub folds: usize,
    /// Of those, the ones on the boundary where its chord sags more than the triangle is high
    /// (a feature narrower than a coarse tolerance: no point inside changes that chord).
    pub forced_folds: usize,
}

pub(crate) struct Params {
    pub tol: f64,
    pub angle: f64,
    pub max_passes: usize,
    pub max_triangles: usize,
}

type Cdt = ConstrainedDelaunayTriangulation<Point2<f64>>;

struct Builder<'a> {
    g: &'a Geom,
    cdt: Cdt,
    info: Vec<FVert>,
    warp: Warp,
    bounds: (DVec2, DVec2),
}

impl Builder<'_> {
    fn insert(&mut self, v: FVert) -> Option<FixedVertexHandle> {
        let q = self.point(v.uv);
        let h = self.cdt.insert(q).ok()?;
        if h.index() >= self.info.len() {
            self.info.resize(h.index() + 1, v);
            self.info[h.index()] = v;
        }
        Some(h)
    }

    fn point(&self, uv: DVec2) -> Point2<f64> {
        // spade rejects magnitudes below 2^-142 (other than zero)
        let snap = |x: f64| if x.abs() < 1e-30 { 0.0 } else { x };
        let q = self.warp.map(uv);
        Point2::new(snap(q.x), snap(q.y))
    }

    /// `uv` (surface point `q`) lies in an inner triangle or on an inner edge between two (not
    /// on the boundary), farther than `min` from the corners there, and not on a boundary edge
    /// of the triangle (within 1e-6 of its length in the CDT's coordinates: the CDT would split
    /// that edge, which the neighbour face does not).
    fn inside_point(&self, uv: DVec2, q: DVec3, min: f64, inside: &[bool]) -> bool {
        use spade::PositionInTriangulation as P;
        if !uv.is_finite() {
            return false;
        }
        let ins = |f: spade::handles::FaceHandle<_, _, _, _, _>| f.as_inner().is_some_and(|f| inside[f.fix().index()]);
        let far = |vs: &[spade::handles::VertexHandle<Point2<f64>, (), spade::CdtEdge<()>, ()>]| {
            vs.iter().all(|v| (self.info[v.fix().index()].p - q).length() > min)
        };
        let x = self.point(uv);
        let x = DVec2::new(x.x, x.y);
        let off_boundary = |f: spade::handles::FixedFaceHandle<spade::handles::InnerTag>| {
            self.cdt.face(f).adjacent_edges().iter().filter(|e| e.is_constraint_edge()).all(|e| {
                let [a, b] = e.positions().map(|p| DVec2::new(p.x, p.y));
                let ab = b - a;
                let t = ((x - a).dot(ab) / ab.length_squared().max(1e-300)).clamp(0.0, 1.0);
                (a + ab * t - x).length() > 1e-6 * ab.length()
            })
        };
        match self.cdt.locate(self.point(uv)) {
            P::OnFace(f) => inside[f.index()] && far(&self.cdt.face(f).vertices()) && off_boundary(f),
            P::OnEdge(e) => {
                let e = self.cdt.directed_edge(e);
                !e.is_constraint_edge() && ins(e.face()) && ins(e.rev().face()) && far(&e.vertices())
            }
            _ => false,
        }
    }

    /// Foot of `q` (a point of the mesh near `seed`) on the surface and its distance. A foot
    /// outside the box `near` (grown by its size) is another part of the surface: then the
    /// seed, and the distance of `σ(seed)` from the plane through `q` with normal `n` (from
    /// `q` if `n` is zero).
    fn foot(&self, q: DVec3, seed: DVec2, near: (DVec2, DVec2), n: DVec3) -> (DVec2, f64) {
        let mut uv = self.g.project(q, seed);
        for i in 0..2 {
            if let Some(per) = self.g.period[i] {
                let (t, s) = if i == 0 { (&mut uv.x, seed.x) } else { (&mut uv.y, seed.y) };
                *t += per * ((s - *t) / per).round();
            }
        }
        let grow = (near.1 - near.0).max(DVec2::splat(1e-12));
        if uv.cmpge(near.0 - grow).all() && uv.cmple(near.1 + grow).all() {
            return (uv, (self.g.eval(uv) - q).length());
        }
        let s = self.g.eval(seed);
        let d = if n == DVec3::ZERO { (s - q).length() } else { (s - q).dot(n).abs() };
        (seed, d)
    }

    fn interior(&mut self, uv: DVec2) -> Option<FixedVertexHandle> {
        let p = self.g.eval(uv);
        self.insert(FVert { uv, p, id: None, pole: false })
    }

    /// Inner faces (odd number of constraint edges from the outside), by fixed face index.
    fn inside(&self) -> Vec<bool> {
        use spade::handles::{FixedFaceHandle, InnerTag};
        use std::collections::VecDeque;
        let n = self.cdt.num_all_faces();
        let mut depth = vec![u32::MAX; n];
        let mut queue: VecDeque<(FixedFaceHandle<InnerTag>, u32)> = VecDeque::new();
        let visit = |depth: &mut Vec<u32>, q: &mut VecDeque<_>, f: FixedFaceHandle<InnerTag>, d: u32, front: bool| {
            if d < depth[f.index()] {
                depth[f.index()] = d;
                if front { q.push_front((f, d)) } else { q.push_back((f, d)) }
            }
        };
        for e in self.cdt.directed_edges() {
            if e.face().is_outer()
                && let Some(f) = e.rev().face().as_inner()
            {
                let c = e.is_constraint_edge();
                visit(&mut depth, &mut queue, f.fix(), u32::from(c), !c);
            }
        }
        while let Some((fh, d)) = queue.pop_front() {
            if d > depth[fh.index()] {
                continue;
            }
            for e in self.cdt.face(fh).adjacent_edges() {
                let Some(nf) = e.rev().face().as_inner() else { continue };
                let c = e.is_constraint_edge();
                visit(&mut depth, &mut queue, nf.fix(), d + u32::from(c), !c);
            }
        }
        depth.iter().map(|d| *d != u32::MAX && d % 2 == 1).collect()
    }
}

/// cos 60°: a triangle farther than this off the surface normal at one of its corners is
/// refined (and straightened by flips after the refinement, see `repair_folds`).
const SKEW_COS: f64 = 0.5;

/// Factor by which a straight direction is squashed in the CDT's coordinates (see `Warp`).
const SQUASH: f64 = 4.0;

/// Rows of a straight direction near a boundary that runs across the grid: this many steps of
/// the other direction apart (see `straight_lines`).
const NEAR: f64 = 2.0;

/// A surface curved in both directions gets grid steps for `tol / DOUBLE` in each: the chord
/// of a cell's diagonal sags by the sum of the two directions' sags, so a grid for the full
/// tolerance in each direction would leave every diagonal at about twice the tolerance, and
/// the refinement would have to split every cell.
const DOUBLE: f64 = 2.0;

/// Lines of parameter `dir` strictly inside `(lo, hi)`.
fn uniform(lo: f64, hi: f64, step: f64) -> Vec<f64> {
    let n = steps(hi - lo, step);
    (1..n).map(|i| lo + (hi - lo) * i as f64 / n as f64).collect()
}

/// Grid lines in u and v over `bounds` (see the module docs).
fn grid_lines(g: &Geom, bounds: (DVec2, DVec2), p: &Params) -> [Vec<f64>; 2] {
    let (lo, hi) = bounds;
    if g.is_plane() {
        return [Vec::new(), Vec::new()];
    }
    if let Some(radii) = g.radii(bounds) {
        // doubly curved: half the tolerance each (see `DOUBLE`)
        let tol = if radii.iter().all(Option::is_some) { p.tol / DOUBLE } else { p.tol };
        let mut out = [Vec::new(), Vec::new()];
        for i in 0..2 {
            if let Some(r) = radii[i] {
                let (a, b) = if i == 0 { (lo.x, hi.x) } else { (lo.y, hi.y) };
                out[i] = uniform(a, b, step_angle(r, tol, p.angle));
            }
        }
        return out;
    }
    // general: lines from the derivatives sampled over the bounds
    let mut pieces: [Vec<f64>; 2] = [Vec::new(), Vec::new()];
    // lines per piece for the chord and for the turn
    let mut chord: [Vec<f64>; 2] = [Vec::new(), Vec::new()];
    let mut turns: [Vec<f64>; 2] = [Vec::new(), Vec::new()];
    for i in 0..2 {
        let (a, b) = if i == 0 { (lo.x, hi.x) } else { (lo.y, hi.y) };
        if !crate::gt(b, a) {
            continue;
        }
        let (c, d) = if i == 0 { (lo.y, hi.y) } else { (lo.x, hi.x) };
        pieces[i] = g.breaks(i, a, b);
        // lines needed on each piece
        for w in pieces[i].windows(2) {
            // samples of the normal part of σ'' (the sag of a chord off the surface; the
            // tangential part only changes the speed) and of the normal's turn rate
            let mut d2: f64 = 0.0;
            let mut turn: f64 = 0.0;
            for si in 0..=4 {
                for sj in 0..=6 {
                    let t = w[0] + (w[1] - w[0]) * si as f64 / 4.0;
                    let s = c + (d - c) * sj as f64 / 6.0;
                    let uv = if i == 0 { DVec2::new(t, s) } else { DVec2::new(s, t) };
                    let ds = g.ders(uv);
                    let Some(n) = ds[1].cross(ds[2]).try_normalize() else { continue };
                    let (d1, dd) = if i == 0 { (ds[1], ds[3]) } else { (ds[2], ds[5]) };
                    let k = dd.dot(n).abs();
                    d2 = d2.max(k);
                    if d1.length() > 1e-12 {
                        turn = turn.max(k / d1.length());
                    }
                }
            }
            chord[i].push((w[1] - w[0]) * (d2 / (8.0 * p.tol)).sqrt());
            turns[i].push((w[1] - w[0]) * turn / p.angle);
        }
    }
    // curved in both directions (at least a line each): half the tolerance each
    let f = if chord.iter().all(|c| c.iter().sum::<f64>() >= 1.0) { DOUBLE.sqrt() } else { 1.0 };
    let mut need: [Vec<f64>; 2] =
        [0, 1].map(|i| chord[i].iter().zip(&turns[i]).map(|(c, t)| (c * f).max(*t)).collect());
    // a direction that is straight over part of the face (a revolution's profile with a
    // straight piece) gets lines there as well, at most ASPECT times the other direction's
    // spacing (by arc length) and no denser than on its curved pieces: else the grid leaves a
    // band of long thin triangles, and every point the refinement puts into it is joined to
    // its neighbours along the band
    const ASPECT: f64 = 4.0;
    let sc = scale(g, bounds);
    let total = |n: &[f64]| n.iter().sum::<f64>();
    let spacing: [f64; 2] = [0, 1].map(|i| {
        let span = if i == 0 { hi.x - lo.x } else { hi.y - lo.y };
        span * sc[i] / (total(&need[i]).ceil() + 1.0)
    });
    let mut out = [Vec::new(), Vec::new()];
    for i in 0..2 {
        let o = 1 - i;
        let (a, b) = if i == 0 { (lo.x, hi.x) } else { (lo.y, hi.y) };
        if pieces[i].is_empty() {
            continue;
        }
        if total(&need[i]) > 0.0 && total(&need[o]) > 0.0 {
            // never denser than the direction is elsewhere (a narrow strip keeps its few lines)
            let densest = need[i]
                .iter()
                .zip(pieces[i].windows(2))
                .map(|(n, w)| n / (w[1] - w[0]).max(1e-300))
                .fold(0.0, f64::max);
            let floor = (sc[i] / (ASPECT * spacing[o])).min(densest);
            for (n, w) in need[i].iter_mut().zip(pieces[i].windows(2)) {
                *n = n.max((w[1] - w[0]) * floor);
            }
        }
        // lines evenly over the cumulated need (knots without need get none)
        let need = &need[i];
        let pieces = &pieces[i];
        let total = total(need);
        let n = (total.ceil() as usize).min(4096);
        let mut all = Vec::new();
        let (mut k, mut acc) = (0usize, 0.0);
        for j in 1..n {
            let target = total * j as f64 / n as f64;
            while k + 1 < need.len() && acc + need[k] < target {
                acc += need[k];
                k += 1;
            }
            let f = if need[k] > 0.0 { ((target - acc) / need[k]).clamp(0.0, 1.0) } else { 0.0 };
            all.push(pieces[k] + (pieces[k + 1] - pieces[k]) * f);
        }
        out[i] = all.into_iter().filter(|t| *t > a && *t < b).collect();
    }
    out
}

/// No ring about a pole (a line of the parameter that is not the pole's free one) nearer the
/// pole than the nearest point of a boundary run along the rings' direction (a corner blend's
/// meridian edge): a triangle from the pole to that point and to a ring point at another
/// height nearly contains the axis where the blend tapers to a needle, and faces up to 90° off
/// the surface. The first ring goes through that point instead, so the fan from the pole
/// reaches points of one height.
fn clear_poles(loops: &[Vec<UvPt>], lines: &mut [Vec<f64>; 2], (lo, hi): (DVec2, DVec2)) {
    for o in 0..2 {
        let i = 1 - o;
        let pick = |v: DVec2, k: usize| if k == 0 { v.x } else { v.y };
        let span = (pick(hi, o) - pick(lo, o)).abs().max(1e-300);
        // the poles the face's loops pass through (the surface's others are not the face's)
        let mut poles: Vec<f64> =
            loops.iter().flatten().filter_map(|q| q.pole.filter(|p| p.free == o)).map(|p| p.value).collect();
        poles.sort_by(f64::total_cmp);
        poles.dedup();
        for pole in poles {
            // the nearest point of a run along direction i (the other parameter constant)
            let mut nearest = f64::INFINITY;
            for l in loops {
                for k in 0..l.len() {
                    let (p, q) = (l[k], l[(k + 1) % l.len()]);
                    if p.pole.is_some() || q.pole.is_some() || (pick(p.uv, o) - pick(q.uv, o)).abs() > 1e-9 * span {
                        continue;
                    }
                    for t in [pick(p.uv, i), pick(q.uv, i)] {
                        if (t - pole).abs() < (nearest - pole).abs() {
                            nearest = t;
                        }
                    }
                }
            }
            let (a, b) = (pick(lo, i), pick(hi, i));
            if nearest.is_finite() && nearest > a && nearest < b {
                let (x, y) = (pole.min(nearest), pole.max(nearest));
                let eps = 1e-6 * (b - a);
                lines[i].retain(|t| !(*t > x - eps && *t < y + eps));
                // the first ring through that point
                lines[i].push(nearest);
                lines[i].sort_by(f64::total_cmp);
            }
        }
    }
}

/// A straight direction (cylinder, cone and extrusion along v) needs no lines for the chord, but
/// where the boundary runs across the grid (a mitre, a hole), Delaunay fans long triangles from
/// it over the face. Lines then at twice the other direction's spacing over the span of such a
/// run (a run of boundary segments that change both parameters and span more than two grid
/// steps along the straight direction) and one row beyond, at 32 times elsewhere (in the CDT's
/// coordinates all rows are half a step apart, see `Warp`: a band between rows far apart costs
/// no more points than one between rows near each other; the rows away from the runs only stop
/// Delaunay fans along the whole face).
fn straight_lines(loops: &[Vec<UvPt>], lines: &mut [Vec<f64>; 2], (lo, hi): (DVec2, DVec2), sc: DVec2) {
    for i in 0..2 {
        let o = 1 - i;
        if !lines[i].is_empty() || lines[o].is_empty() {
            continue;
        }
        let pick = |v: DVec2, k: usize| if k == 0 { v.x } else { v.y };
        let step3 = (pick(hi, o) - pick(lo, o)) * pick(sc, o) / (lines[o].len() + 1) as f64;
        // spans (in parameter i) of the runs across
        let mut spans: Vec<(f64, f64)> = Vec::new();
        for l in loops {
            let mut run: Option<(f64, f64, f64)> = None;
            for k in 0..=l.len() {
                let (a, b) = (l[k % l.len()].uv, l[(k + 1) % l.len()].uv);
                let d = (b - a) * sc;
                let len = d.length();
                if k < l.len() && d.x.abs() > 1e-6 * len && d.y.abs() > 1e-6 * len {
                    let r = run.get_or_insert((pick(a, i), pick(a, i), 0.0));
                    r.0 = r.0.min(pick(b, i));
                    r.1 = r.1.max(pick(b, i));
                    r.2 += pick(d, i).abs();
                } else if let Some((x, y, total)) = run.take()
                    && total > 2.0 * step3
                {
                    spans.push((x, y));
                }
            }
        }
        let (a, b) = (pick(lo, i), pick(hi, i));
        if spans.is_empty() || !crate::gt(b, a) {
            continue;
        }
        let fine = NEAR * step3 / pick(sc, i);
        let mut out = Vec::new();
        let mut t = a;
        while t < b {
            let near = spans.iter().any(|(x, y)| t >= x - fine && t <= y + fine);
            t += if near {
                fine
            } else {
                // far rows, but never past the start of the next run's rows (a far step longer
                // than a run and its margins would skip it)
                let next = spans.iter().map(|(x, _)| x - fine).filter(|x| *x > t).fold(f64::INFINITY, f64::min);
                (16.0 * fine).min(next - t)
            };
            if t < b - 0.5 * fine {
                out.push(t);
            }
        }
        lines[i] = out;
    }
}

/// Crossings of the loops with the line `v = const` (u values, sorted).
fn crossings(loops: &[Vec<UvPt>], v: f64) -> Vec<f64> {
    let mut xs = Vec::new();
    for l in loops {
        for k in 0..l.len() {
            let (a, b) = (l[k].uv, l[(k + 1) % l.len()].uv);
            if (a.y > v) != (b.y > v) {
                xs.push(a.x + (b.x - a.x) * (v - a.y) / (b.y - a.y));
            }
        }
    }
    xs.sort_by(f64::total_cmp);
    xs
}

/// Grid points inside the loops, at least `margin` (in each parameter) away from them.
fn grid_points(loops: &[Vec<UvPt>], lines: &[Vec<f64>; 2], margin: DVec2) -> Vec<DVec2> {
    let mut out = Vec::new();
    for &v in &lines[1] {
        let rows = [crossings(loops, v - margin.y), crossings(loops, v), crossings(loops, v + margin.y)];
        for &u in &lines[0] {
            let ok = rows.iter().all(|xs| {
                let k = xs.partition_point(|x| *x < u);
                let near = (k > 0 && u - xs[k - 1] < margin.x) || (k < xs.len() && xs[k] - u < margin.x);
                k % 2 == 1 && !near
            });
            if ok {
                out.push(DVec2::new(u, v));
            }
        }
    }
    out
}

/// Mean lengths of `σu` and `σv` over the bounds (the CDT's scale), 1 where degenerate.
fn scale(g: &Geom, (lo, hi): (DVec2, DVec2)) -> DVec2 {
    if g.is_plane() {
        return DVec2::ONE;
    }
    let mut su = Vec::new();
    let mut sv = Vec::new();
    for i in 0..5 {
        for j in 0..5 {
            let uv = lo + (hi - lo) * DVec2::new((i as f64 + 0.5) / 5.0, (j as f64 + 0.5) / 5.0);
            let d = g.ders(uv);
            su.push(d[1].length());
            sv.push(d[2].length());
        }
    }
    let med = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    let (a, b) = (med(&mut su), med(&mut sv));
    let ok = |x: f64| if x.is_finite() && x > 1e-12 { x } else { 1.0 };
    DVec2::new(ok(a), ok(b))
}

/// The CDT's coordinates, in which Delaunay triangles are well shaped for the chord error:
///
/// - a direction with grid lines for the chord in *grid steps*: the bounds and the lines at
///   0, 1, 2, …, and between two of them by the arc length along the parameter (its speed
///   `|σu|`, `|σv|`, the median across the other parameter, integrated over 64 steps of the
///   bounds: dense knots at the ends of a sweep). A step keeps the chord within the tolerance
///   in each direction, so a triangle that is round in these coordinates is within it in every
///   direction: a torus with a large major radius gets long triangles along its circle and
///   short ones across, not the isotropic ones of the arc length (many times the triangles);
/// - a straight direction (no lines for the chord where the other one has some) with rows
///   (`straight_lines`) in rows: each band between two rows `NEAR / SQUASH` (half a step)
///   high, as the rows near a run across are by arc length squashed by `SQUASH`. The rows
///   away from such runs, 32 steps of the other direction apart, would otherwise be eight
///   steps apart in these coordinates and leave bands in which a point the refinement inserts
///   is joined to row points several steps away across the curved direction, whose chords are
///   then off by several times the tolerance, and so on along the band (a cone between a
///   fillet and the holes of a handle: the refinement did not converge);
/// - a straight direction without rows by its arc length in units of `SQUASH` times the other
///   direction's mean step: Delaunay then joins points along it rather than across the curved
///   one, whose extent alone makes the chord error;
/// - otherwise (no lines either way) the arc length.
///
/// Linear beyond the bounds.
#[derive(Debug, Clone)]
struct Warp {
    t: [Vec<f64>; 2],
    s: [Vec<f64>; 2],
}

impl Warp {
    /// `chord`: the grid lines for the chord (`grid_lines`), `lines`: with the rows of a
    /// straight direction (`straight_lines`).
    fn new(g: &Geom, (lo, hi): (DVec2, DVec2), sc: DVec2, chord: &[Vec<f64>; 2], lines: &[Vec<f64>; 2]) -> Warp {
        const N: usize = 64;
        let mut w = Warp { t: [Vec::new(), Vec::new()], s: [Vec::new(), Vec::new()] };
        for i in 0..2 {
            let (a, b) = if i == 0 { (lo.x, hi.x) } else { (lo.y, hi.y) };
            let (c, d) = if i == 0 { (lo.y, hi.y) } else { (lo.x, hi.x) };
            let mean = if i == 0 { sc.x } else { sc.y };
            if g.is_plane() || !crate::gt(b, a) {
                w.t[i] = vec![a, a + 1.0];
                w.s[i] = vec![a * mean, (a + 1.0) * mean];
                continue;
            }
            let speed = |t: f64| {
                let mut v: Vec<f64> = (0..5)
                    .map(|k| {
                        let o = c + (d - c) * (k as f64 + 0.5) / 5.0;
                        let uv = if i == 0 { DVec2::new(t, o) } else { DVec2::new(o, t) };
                        let ds = g.ders(uv);
                        if i == 0 { ds[1].length() } else { ds[2].length() }
                    })
                    .collect();
                v.sort_by(f64::total_cmp);
                // never below a tenth of the mean (a pole row stays apart from its neighbours)
                v[2].max(0.1 * mean)
            };
            let ts: Vec<f64> = (0..=N).map(|k| a + (b - a) * k as f64 / N as f64).collect();
            let sp: Vec<f64> = ts.iter().map(|t| speed(*t)).collect();
            let mut acc = a * mean;
            let mut ss = vec![acc];
            for k in 1..=N {
                acc += 0.5 * (sp[k - 1] + sp[k]) * (ts[k] - ts[k - 1]);
                ss.push(acc);
            }
            w.t[i] = ts;
            w.s[i] = ss;
        }
        let has = [0, 1].map(|i| !chord[i].is_empty());
        if g.is_plane() || !(has[0] || has[1]) {
            return w;
        }
        // mean step by arc length
        let step = [0, 1].map(|i| {
            let s = &w.s[i];
            (s[s.len() - 1] - s[0]) / (chord[i].len() + 1) as f64
        });
        for i in 0..2 {
            if has[i] {
                w.in_steps(i, chord[i].as_slice());
            } else if !lines[i].is_empty() {
                w.in_steps(i, lines[i].as_slice());
                w.s[i].iter_mut().for_each(|x| *x *= NEAR / SQUASH);
            } else {
                let unit = SQUASH * step[1 - i];
                if crate::gt(unit, 0.0) {
                    w.s[i].iter_mut().for_each(|x| *x /= unit);
                }
            }
        }
        w
    }

    /// Direction `i` in grid steps: the bounds and `lines` (increasing, inside the bounds) at
    /// 0, 1, 2, …, the arc length in between.
    fn in_steps(&mut self, i: usize, lines: &[f64]) {
        let (a, b) = (self.t[i][0], self.t[i][self.t[i].len() - 1]);
        let mut knots = Vec::with_capacity(lines.len() + 2);
        knots.push(a);
        knots.extend(lines.iter().copied().filter(|t| *t > a && *t < b));
        knots.push(b);
        let arc: Vec<f64> = knots.iter().map(|t| self.map1(i, *t)).collect();
        let mut ts: Vec<f64> = self.t[i].iter().chain(&knots).copied().collect();
        ts.sort_by(f64::total_cmp);
        ts.dedup();
        let ss = ts
            .iter()
            .map(|&t| {
                let j = knots.partition_point(|k| *k <= t).clamp(1, knots.len() - 1) - 1;
                let (s0, s1) = (arc[j], arc[j + 1]);
                let f = if s1 > s0 {
                    (self.map1(i, t) - s0) / (s1 - s0)
                } else {
                    (t - knots[j]) / (knots[j + 1] - knots[j])
                };
                j as f64 + f.clamp(0.0, 1.0)
            })
            .collect();
        self.t[i] = ts;
        self.s[i] = ss;
    }

    fn map1(&self, i: usize, x: f64) -> f64 {
        let (t, s) = (&self.t[i], &self.s[i]);
        let n = t.len();
        let k = t.partition_point(|v| *v <= x).clamp(1, n - 1);
        let f = (x - t[k - 1]) / (t[k] - t[k - 1]);
        s[k - 1] + (s[k] - s[k - 1]) * f
    }

    fn inv1(&self, i: usize, y: f64) -> f64 {
        let (t, s) = (&self.t[i], &self.s[i]);
        let n = s.len();
        let k = s.partition_point(|v| *v <= y).clamp(1, n - 1);
        let ds = s[k] - s[k - 1];
        let f = if ds > 0.0 { (y - s[k - 1]) / ds } else { 0.0 };
        t[k - 1] + (t[k] - t[k - 1]) * f
    }

    fn map(&self, uv: DVec2) -> DVec2 {
        DVec2::new(self.map1(0, uv.x), self.map1(1, uv.y))
    }

    fn inv(&self, q: DVec2) -> DVec2 {
        DVec2::new(self.inv1(0, q.x), self.inv1(1, q.y))
    }
}

/// Inserts pole copies at the grid lines of the free parameter between two copies of a pole.
fn split_poles(loops: &mut [Vec<UvPt>], lines: &[Vec<f64>; 2]) {
    for l in loops.iter_mut() {
        let mut out = Vec::with_capacity(l.len());
        for k in 0..l.len() {
            let a = l[k];
            out.push(a);
            let b = l[(k + 1) % l.len()];
            let (Some(pa), Some(_)) = (a.pole, b.pole) else { continue };
            if a.id != b.id {
                continue;
            }
            let free = pa.free;
            let (ta, tb) = if free == 0 { (a.uv.x, b.uv.x) } else { (a.uv.y, b.uv.y) };
            let mut ts: Vec<f64> = lines[free].iter().copied().filter(|t| (*t - ta) * (*t - tb) < 0.0).collect();
            if tb < ta {
                ts.reverse();
            }
            for t in ts {
                let uv = if free == 0 { DVec2::new(t, a.uv.y) } else { DVec2::new(a.uv.x, t) };
                out.push(UvPt { uv, ..a });
            }
        }
        *l = out;
    }
}

/// Fills in the vertex `i` that `add_constraint_and_split` created where the segment `xy`
/// crosses an earlier one of `segs`: a new boundary point (id from `next_id`) on both, at the
/// mean of the two segments' points there, and a split of both for the neighbour faces.
fn split_vertex(
    b: &mut Builder,
    fm: &mut FaceMesh,
    i: usize,
    xy: (FixedVertexHandle, FixedVertexHandle),
    segs: &[(FixedVertexHandle, FixedVertexHandle)],
    next_id: &mut u32,
) {
    let pos = b.cdt.vertex(FixedVertexHandle::from_index(i)).position();
    let q = DVec2::new(pos.x, pos.y);
    let uv_of = |h: FixedVertexHandle| {
        let v = b.info[h.index()];
        b.warp.map(v.uv)
    };
    let on = |s: (FixedVertexHandle, FixedVertexHandle)| {
        let (a, c) = (uv_of(s.0), uv_of(s.1));
        let ab = c - a;
        let t = ((q - a).dot(ab) / ab.length_squared().max(1e-300)).clamp(0.0, 1.0);
        ((a + ab * t - q).length(), t)
    };
    let other = segs.iter().map(|s| (*s, on(*s))).min_by(|x, y| x.1.0.total_cmp(&y.1.0));
    let mut hits = vec![(xy, on(xy).1)];
    hits.extend(other.map(|(s, (_, t))| (s, t)));
    let mut p = DVec3::ZERO;
    for ((a, c), t) in &hits {
        p += b.info[a.index()].p.lerp(b.info[c.index()].p, *t);
    }
    p /= hits.len() as f64;
    let id = *next_id;
    *next_id += 1;
    let uv = b.warp.inv(q);
    let v = FVert { uv, p, id: Some(id), pole: false };
    if i >= b.info.len() {
        b.info.resize(i + 1, v);
    }
    b.info[i] = v;
    for ((a, c), t) in hits {
        match (b.info[a.index()].id, b.info[c.index()].id) {
            (Some(ia), Some(ic)) => fm.splits.push(Split { a: ia, b: ic, t, id, p }),
            _ => fm.lost_constraints += 1,
        }
    }
}

/// A boundary segment `a`–`b` (point ids) that a face split at the new point `id` (at `t`
/// along it): the faces on the other side must split it as well.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Split {
    pub a: u32,
    pub b: u32,
    pub t: f64,
    pub id: u32,
    pub p: DVec3,
}

/// Triangulates one face. `points`: the edge points (ids of the loops).
pub(crate) fn mesh_face(
    g: &Geom,
    mut loops: Vec<Vec<UvPt>>,
    points: &[DVec3],
    p: &Params,
    orient: f64,
    next_id: &mut u32,
) -> FaceMesh {
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for q in loops.iter().flatten() {
        lo = lo.min(q.uv);
        hi = hi.max(q.uv);
    }
    let bounds = (lo, hi);
    let mut lines = grid_lines(g, bounds, p);
    clear_poles(&loops, &mut lines, bounds);
    let sc = scale(g, bounds);
    let chord = lines.clone();
    straight_lines(&loops, &mut lines, bounds, sc);
    split_poles(&mut loops, &lines);
    let mut b =
        Builder { g, cdt: Cdt::new(), info: Vec::new(), warp: Warp::new(g, bounds, sc, &chord, &lines), bounds };
    let mut fm = FaceMesh { bounds, ..Default::default() };
    // boundary
    let mut handles: Vec<Vec<Option<FixedVertexHandle>>> = Vec::new();
    for l in &loops {
        let hs = l
            .iter()
            .map(|q| b.insert(FVert { uv: q.uv, p: points[q.id as usize], id: Some(q.id), pole: q.pole.is_some() }))
            .collect();
        handles.push(hs);
    }
    // segments inserted so far (boundary point ids at both ends)
    let mut segs: Vec<(FixedVertexHandle, FixedVertexHandle)> = Vec::new();
    for hs in &handles {
        for k in 0..hs.len() {
            let (Some(x), Some(y)) = (hs[k], hs[(k + 1) % hs.len()]) else {
                fm.lost_constraints += 1;
                continue;
            };
            if x == y || b.cdt.exists_constraint(x, y) {
                continue;
            }
            if b.cdt.try_add_constraint(x, y).is_empty() && !b.cdt.exists_constraint(x, y) {
                // the loops cross in (u, v) (polylines of tangent edges near a common vertex,
                // a face's own loops near a cusp): split both segments at the crossing
                let n0 = b.cdt.num_vertices();
                b.cdt.add_constraint_and_split(x, y, |v| v);
                for i in n0..b.cdt.num_vertices() {
                    split_vertex(&mut b, &mut fm, i, (x, y), &segs, next_id);
                }
            }
            segs.push((x, y));
        }
    }
    // grid (none if it alone is beyond the budget)
    let estimate = 2 * (lines[0].len() + 1) * (lines[1].len() + 1);
    if estimate > p.max_triangles {
        fm.too_fine = Some(estimate);
        return fm;
    }
    if !g.is_plane() {
        let step = |ls: &[f64], a: f64, c: f64| {
            if ls.is_empty() { (c - a).max(0.0) } else { (c - a) / (ls.len() + 1) as f64 }
        };
        let margin = DVec2::new(step(&lines[0], lo.x, hi.x), step(&lines[1], lo.y, hi.y)) * 0.3;
        for uv in grid_points(&loops, &lines, margin) {
            b.interior(uv);
        }
    }
    if b.cdt.num_inner_faces() > p.max_triangles {
        fm.too_fine = Some(b.cdt.num_inner_faces());
        return fm;
    }
    // refinement
    let mut inside = b.inside();
    if !g.is_plane() {
        let mut checked = Checked::default();
        // each pass measures the mesh as it is; the last one only measures
        let mut passes = 0;
        loop {
            let (add, worst) = refine_pass(&b, &inside, p.tol, &mut checked);
            fm.deviation = worst;
            if add.is_empty() || passes == p.max_passes {
                break;
            }
            passes += 1;
            // only points inside the face and off its boundary (a point on a boundary edge
            // would not be on the neighbour's side of it)
            // and none within `tol / 20` of a vertex or of another new point
            let mut seen: std::collections::HashMap<glam::I64Vec3, Vec<DVec3>> = Default::default();
            let cell = 0.05 * p.tol;
            // the second candidate where the first is not inside (a foot on the boundary)
            let mut ok: Vec<DVec2> = Vec::new();
            'cands: for cands in add {
                for uv in cands {
                    let q = b.g.eval(uv);
                    let key = (q / cell).floor().as_i64vec3();
                    let mut near = false;
                    for dx in -1..=1 {
                        for dy in -1..=1 {
                            for dz in -1..=1 {
                                let k = key.saturating_add(glam::I64Vec3::new(dx, dy, dz));
                                near |= seen.get(&k).is_some_and(|v| v.iter().any(|o| (*o - q).length() <= cell));
                            }
                        }
                    }
                    if near {
                        continue 'cands;
                    }
                    if b.inside_point(uv, q, cell, &inside) {
                        seen.entry(key).or_default().push(q);
                        ok.push(uv);
                        continue 'cands;
                    }
                }
            }
            if ok.is_empty() {
                break;
            }
            for uv in &ok {
                if b.cdt.num_inner_faces() > p.max_triangles {
                    fm.too_fine = Some(b.cdt.num_inner_faces());
                    return fm;
                }
                if b.interior(*uv).is_some() {
                    fm.refined += 1;
                }
            }
            inside = b.inside();
        }
    }
    // output
    let mut map = vec![u32::MAX; b.info.len()];
    // loops that cross: classified by winding number instead of parity, a region running the
    // other way (a fold of the polylines near a cusp) kept with its triangles reversed, so its
    // edges still meet the neighbours' in opposite directions
    let crossed = !fm.splits.is_empty();
    // triangles kept reversed by design (see above): not checked for folds
    let mut reversed = Vec::new();
    for f in b.cdt.inner_faces() {
        let mut vs = f.vertices();
        let mut rev = false;
        if crossed {
            let c = vs.iter().map(|v| b.info[v.fix().index()].uv).sum::<DVec2>() / 3.0;
            let w = winding(&loops, c);
            if w == 0 {
                continue;
            }
            if (w > 0) != (orient > 0.0) {
                vs.swap(1, 2);
                rev = true;
            }
        } else if !inside[f.fix().index()] {
            continue;
        }
        let mut t = [0u32; 3];
        for (k, v) in vs.iter().enumerate() {
            let i = v.fix().index();
            if map[i] == u32::MAX {
                map[i] = fm.verts.len() as u32;
                fm.verts.push(b.info[i]);
            }
            t[k] = map[i];
        }
        fm.tris.push(t);
        reversed.push(rev);
    }
    if !g.is_plane() {
        let fixed: std::collections::HashSet<[u32; 2]> = b
            .cdt
            .undirected_edges()
            .filter(|e| e.is_constraint_edge())
            .filter_map(|e| {
                let [x, y] = e.vertices().map(|v| map[v.fix().index()]);
                (x != u32::MAX && y != u32::MAX).then(|| [x.min(y), x.max(y)])
            })
            .collect();
        (fm.folds, fm.forced_folds) = repair_folds(g, &mut fm, &fixed, &reversed);
    }
    fm
}

/// How well triangle `t` (counter-clockwise in (u, v)) of `fm` faces along the surface: the
/// cosine between its normal and the surface normal at its centroid, negative for a fold;
/// `None` for a triangle without area (a pole, a sliver at the `f64` resolution), where the
/// surface normal is undefined, or where the surface turns over under the triangle.
fn facing(g: &Geom, fm: &FaceMesh, t: [u32; 3]) -> Option<f64> {
    let [a, b, c] = t.map(|i| fm.verts[i as usize]);
    let n = (b.p - a.p).cross(c.p - a.p);
    let scale2 = (b.p - a.p).length_squared().max((c.p - a.p).length_squared()).max((c.p - b.p).length_squared());
    if !crate::gt(n.length(), 1e-12 * scale2) {
        return None;
    }
    let m = g.normal((a.uv + b.uv + c.uv) / 3.0, fm.bounds)?;
    // the surface itself turns over under the triangle (its normal at a corner against the
    // centroid's: a sweep whose profile folds near its end): no direction to face
    let corners = [a, b, c].map(|v| g.normal(v.uv, fm.bounds));
    if corners.iter().any(|k| k.is_some_and(|k| k.dot(m) < 0.0)) {
        return None;
    }
    Some(m.dot(n.normalize()))
}

/// Folded triangles (facing against the surface, see `facing`) after the refinement: a
/// triangle of the CDT is counter-clockwise in its warped coordinates, which are not an affine
/// image of (u, v) (rows of a straight direction far apart), and a thin one along a curved
/// boundary can turn over in 3D. The same pass straightens triangles more than 60° off the
/// surface normal at a corner (`SKEW_COS`; Delaunay in grid steps near a pole, where a step of the
/// pole's free parameter is far shorter on the surface than one of the other). Each is
/// repaired by flipping an inner edge with a neighbour where neither new triangle folds and
/// the worse of the two lies better along the surface than before. Returns the folds left and
/// how many of them are forced (on the boundary, whose chord sags more than half the
/// triangle's height above it: a feature narrower than a coarse tolerance, which no point
/// inside changes); `tessellate` fails on the others, never exporting them silently. `fixed`: boundary edges
/// (never flipped), `skip`: triangles reversed by design (crossing loops).
fn repair_folds(
    g: &Geom,
    fm: &mut FaceMesh,
    fixed: &std::collections::HashSet<[u32; 2]>,
    skip: &[bool],
) -> (usize, usize) {
    use std::collections::HashMap;
    // vertices by their output position (seam and pole copies are one point there): edges and
    // degenerate triangles as the welded mesh will see them
    let mut by_pos: HashMap<[u32; 3], u32> = HashMap::new();
    let canon: Vec<u32> = fm
        .verts
        .iter()
        .enumerate()
        .map(|(i, v)| *by_pos.entry(v.p.as_vec3().to_array().map(f32::to_bits)).or_insert(i as u32))
        .collect();
    let key = |x: u32, y: u32| {
        let (x, y) = (canon[x as usize], canon[y as usize]);
        [x.min(y), x.max(y)]
    };
    let fixed: std::collections::HashSet<[u32; 2]> = fixed.iter().map(|e| key(e[0], e[1])).collect();
    let normals: Vec<Option<DVec3>> = fm.verts.iter().map(|v| g.normal(v.uv, fm.bounds)).collect();
    let folded = |fm: &FaceMesh, t: [u32; 3]| facing(g, fm, t).is_some_and(|d| d < 0.0);
    // the smallest cosine between the triangle's normal and its corners' surface normals
    let worst = |fm: &FaceMesh, t: [u32; 3]| {
        let [a, b, c] = t.map(|i| fm.verts[i as usize].p);
        let Some(n) = (b - a).cross(c - a).try_normalize() else { return 1.0 };
        t.iter().filter_map(|i| normals[*i as usize]).map(|m| m.dot(n)).fold(1.0, f64::min)
    };
    let bad = |fm: &FaceMesh, t: usize| !skip[t] && (folded(fm, fm.tris[t]) || worst(fm, fm.tris[t]) < SKEW_COS);
    let mut todo: Vec<usize> = (0..fm.tris.len()).filter(|t| bad(fm, *t)).collect();
    if todo.is_empty() {
        return (0, 0);
    }
    let mut edges: HashMap<[u32; 2], Vec<usize>> = HashMap::new();
    for (i, t) in fm.tris.iter().enumerate() {
        for k in 0..3 {
            edges.entry(key(t[k], t[(k + 1) % 3])).or_default().push(i);
        }
    }
    for _ in 0..8 {
        let mut left = Vec::new();
        for &t in &todo {
            if !bad(fm, t) {
                continue;
            }
            let tri = fm.tris[t];
            let mut done = false;
            for k in 0..3 {
                let (a, b, c) = (tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]);
                let e = key(a, b);
                if fixed.contains(&e) {
                    continue;
                }
                let Some(&o) = edges.get(&e).and_then(|v| v.iter().find(|x| **x != t)) else { continue };
                if skip[o] {
                    continue;
                }
                let ot = fm.tris[o];
                let Some(d) = ot.iter().copied().find(|v| key(*v, *v) != key(a, a) && key(*v, *v) != key(b, b)) else {
                    continue;
                };
                if edges.get(&key(c, d)).is_some_and(|v| !v.is_empty()) {
                    continue; // the new edge exists already
                }
                // (a, b, c) and (b, a, d) become (a, d, c) and (d, b, c)
                let (n1, n2) = ([a, d, c], [d, b, c]);
                if canon[c as usize] == canon[d as usize] {
                    continue;
                }
                if [n1, n2].iter().any(|n| facing(g, fm, *n).is_none_or(|x| x <= 0.0)) {
                    continue;
                }
                let before = if folded(fm, tri) || folded(fm, ot) { -2.0 } else { worst(fm, tri).min(worst(fm, ot)) };
                if worst(fm, n1).min(worst(fm, n2)) <= before + 1e-9 {
                    continue;
                }
                for (x, y) in [(a, b), (b, c), (c, a)] {
                    if let Some(v) = edges.get_mut(&key(x, y)) {
                        v.retain(|i| *i != t);
                    }
                }
                for (x, y) in [(b, a), (a, d), (d, b)] {
                    if let Some(v) = edges.get_mut(&key(x, y)) {
                        v.retain(|i| *i != o);
                    }
                }
                fm.tris[t] = n1;
                fm.tris[o] = n2;
                for (i, n) in [(t, n1), (o, n2)] {
                    for k in 0..3 {
                        edges.entry(key(n[k], n[(k + 1) % 3])).or_default().push(i);
                    }
                }
                done = true;
                // the neighbour may need another flip
                left.push(o);
                break;
            }
            if !done || bad(fm, t) {
                left.push(t);
            }
        }
        left.sort_unstable();
        left.dedup();
        let changed = left != todo;
        todo = left;
        if !changed {
            break;
        }
    }
    let left: Vec<usize> = (0..fm.tris.len()).filter(|t| !skip[*t] && folded(fm, fm.tris[*t])).collect();
    // on the boundary where its chord sags more than half the triangle's height above it
    let forced = |t: usize| {
        let tri = fm.tris[t];
        (0..3).any(|k| {
            let (a, b, c) = (tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]);
            if !fixed.contains(&key(a, b)) {
                return false;
            }
            let [pa, pb, pc] = [a, b, c].map(|i| fm.verts[i as usize]);
            let ab = pb.p - pa.p;
            let s = ((pc.p - pa.p).dot(ab) / ab.length_squared().max(1e-300)).clamp(0.0, 1.0);
            let height = (pa.p + ab * s - pc.p).length();
            let mid = (pa.p + pb.p) * 0.5;
            let sag = (g.eval(g.project(mid, (pa.uv + pb.uv) * 0.5)) - mid).length();
            height < 2.0 * sag
        })
    };
    let n = left.iter().filter(|t| forced(**t)).count();
    (left.len(), n)
}

/// Winding number of the loops about `q`.
fn winding(loops: &[Vec<UvPt>], q: DVec2) -> i32 {
    let mut w = 0;
    for l in loops {
        for k in 0..l.len() {
            let (a, b) = (l[k].uv, l[(k + 1) % l.len()].uv);
            let side = (b - a).perp_dot(q - a);
            if a.y <= q.y {
                if b.y > q.y && side > 0.0 {
                    w += 1;
                }
            } else if b.y <= q.y && side < 0.0 {
                w -= 1;
            }
        }
    }
    w
}

/// Triangles and inner edges (by their vertices) within the tolerance and their deviation:
/// a refinement pass measures only the new ones.
#[derive(Default)]
struct Checked {
    tris: std::collections::HashMap<[usize; 3], f64>,
    edges: std::collections::HashMap<[usize; 2], f64>,
}

/// Points to insert, each with a second choice (see the module docs), and the largest
/// deviation found.
fn refine_pass(b: &Builder, inside: &[bool], tol: f64, ok: &mut Checked) -> (Vec<[DVec2; 2]>, f64) {
    let mut add = Vec::new();
    let mut worst: f64 = 0.0;
    let info = |h: spade::handles::VertexHandle<Point2<f64>, (), spade::CdtEdge<()>, ()>| b.info[h.fix().index()];
    for f in b.cdt.inner_faces() {
        if !inside[f.fix().index()] {
            continue;
        }
        let mut key = f.vertices().map(|v| v.fix().index());
        key.sort_unstable();
        if let Some(d) = ok.tris.get(&key) {
            worst = worst.max(*d);
            continue;
        }
        let [a, c, d] = f.vertices().map(info);
        let n = (c.p - a.p).cross(d.p - a.p);
        let area2 = n.length();
        let scale2 = (c.p - a.p).length_squared().max((d.p - a.p).length_squared());
        if !crate::gt(area2, 1e-14 * scale2) || scale2.max((d.p - c.p).length_squared()) < (0.1 * tol).powi(2) {
            ok.tris.insert(key, 0.0);
            continue; // a pole triangle, a sliver or a tiny one: nothing to measure
        }
        let n = n / area2;
        let uvc = (a.uv + c.uv + d.uv) / 3.0;
        let (lo, hi) = (a.uv.min(c.uv).min(d.uv), a.uv.max(c.uv).max(d.uv));
        let (mut foot, mut dev) = b.foot((a.p + c.p + d.p) / 3.0, uvc, (lo, hi), n);
        // near the tolerance also three inner points towards the corners (on a twisted
        // surface the largest deviation need not be at the centroid)
        if dev > tol / 3.0 && dev <= tol {
            for w in [[4.0, 1.0, 1.0], [1.0, 4.0, 1.0], [1.0, 1.0, 4.0]] {
                let q = (a.p * w[0] + c.p * w[1] + d.p * w[2]) / 6.0;
                let quv = (a.uv * w[0] + c.uv * w[1] + d.uv * w[2]) / 6.0;
                let (fq, dq) = b.foot(q, quv, (lo, hi), n);
                if dq > dev {
                    (foot, dev) = (fq, dq);
                }
            }
        }
        worst = worst.max(dev);
        // a triangle on the boundary may face against the surface where the boundary's chord
        // sags more than the face is wide (a narrow fillet at a coarse tolerance): no point
        // inside changes that chord, so only inner triangles count as flipped
        let on_boundary = f.adjacent_edges().iter().any(|e| e.is_constraint_edge());
        let flipped = !on_boundary && b.g.normal(uvc, b.bounds).is_some_and(|m| m.dot(n) < 0.0);
        let split = dev > tol || flipped;
        if !split {
            ok.tris.insert(key, dev);
        }
        if split {
            // the midpoint of the longest edge that may be split, else the centroid
            let es = f.adjacent_edges();
            let best = es
                .iter()
                .filter(|e| !e.is_constraint_edge())
                .map(|e| {
                    let [x, y] = [e.from(), e.to()].map(info);
                    ((x.p - y.p).length_squared(), (x.uv + y.uv) * 0.5)
                })
                .max_by(|x, y| x.0.total_cmp(&y.0));
            let longest = es.iter().map(|e| {
                let [x, y] = [e.from(), e.to()].map(info);
                (x.p - y.p).length_squared()
            });
            let longest = longest.fold(0.0, f64::max);
            match best {
                Some((l2, mid)) if l2 >= 0.5 * longest => add.push([mid, foot]),
                _ => add.push([foot, uvc]),
            }
        }
    }
    for e in b.cdt.undirected_edges() {
        if e.is_constraint_edge() {
            continue;
        }
        let d = e.as_directed();
        let ins = |f: spade::handles::FaceHandle<_, _, _, _, _>| f.as_inner().is_some_and(|f| inside[f.fix().index()]);
        if !(ins(d.face()) && ins(d.rev().face())) {
            continue;
        }
        let mut key = e.vertices().map(|v| v.fix().index());
        key.sort_unstable();
        if let Some(d) = ok.edges.get(&key) {
            worst = worst.max(*d);
            continue;
        }
        let [v0, v1] = e.vertices().map(info);
        let mid = (v0.uv + v1.uv) * 0.5;
        let ab = v1.p - v0.p;
        if ab.length_squared() < (0.1 * tol).powi(2) {
            ok.edges.insert(key, 0.0);
            continue;
        }
        let (lo, hi) = (v0.uv.min(v1.uv), v0.uv.max(v1.uv));
        let (foot, dev) = b.foot((v0.p + v1.p) * 0.5, mid, (lo, hi), DVec3::ZERO);
        worst = worst.max(dev);
        if dev > tol {
            add.push([foot, mid]);
        } else {
            ok.edges.insert(key, dev);
        }
    }
    (add, worst)
}

impl Geom {
    /// Knot breaks of the parameter `i` over `[a, b]` (shifted by periods; curve-based and
    /// B-spline surfaces), the ends included; for the other surfaces `[a, b]` split in quarters.
    pub(crate) fn breaks(&self, i: usize, a: f64, b: f64) -> Vec<f64> {
        let natural = self.natural_breaks(i);
        let mut out = vec![a];
        if let Some(nb) = natural {
            let (d0, d1) = self.domain[i];
            let shifts: Vec<f64> = match self.period[i] {
                Some(per) => {
                    let k0 = ((a - d1) / per).floor() as i64;
                    let k1 = ((b - d0) / per).ceil() as i64;
                    (k0..=k1).map(|k| k as f64 * per).collect()
                }
                None => vec![0.0],
            };
            let mut ts: Vec<f64> = shifts.iter().flat_map(|s| nb.iter().map(move |t| t + s)).collect();
            ts.sort_by(f64::total_cmp);
            for t in ts {
                if t > *out.last().unwrap() + 1e-12 * (b - a) && t < b - 1e-12 * (b - a) {
                    out.push(t);
                }
            }
        } else {
            for k in 1..4 {
                out.push(a + (b - a) * k as f64 / 4.0);
            }
        }
        out.push(b);
        out
    }
}
