//! One face in its parameter plane (plan 2.3, 2.4): a constrained Delaunay triangulation (crate
//! `spade`) of the boundary loops, with grid points inside for curved faces, refined where the
//! triangles leave the surface.
//!
//! 1. **Grid.** Lines in u and v whose step keeps the chord within `tol` and the turn within
//!    the angle: analytic surfaces from their radii (straight directions get no lines),
//!    revolutions and extrusions from their curve, B-splines per knot span from the second
//!    derivatives. Grid points inside the loops (even-odd per row) and not too near them are
//!    inserted.
//! 2. **CDT** in (u, v) scaled by the mean lengths of `σu` and `σv` (so that Delaunay in the
//!    plane is roughly Delaunay on the surface), the loops as constraint edges. Triangles inside
//!    are those an odd number of constraint edges away from the outside (nested loops).
//! 3. **Refinement.** A triangle whose centroid lies farther than `tol` from its plane gets the
//!    centroid, an inner edge whose midpoint lies farther than `tol` from the chord its
//!    midpoint, and a triangle facing against the surface its centroid; repeated a few passes.
//!    Boundary edges are never split (their points are the edges', shared with the neighbour).
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
    /// Largest deviation measured in the last refinement pass.
    pub deviation: f64,
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
    scale: DVec2,
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
        Point2::new(snap(uv.x * self.scale.x), snap(uv.y * self.scale.y))
    }

    /// `uv` lies in an inner triangle or on an inner edge between two (not on the boundary).
    /// `uv` (surface point `q`) lies in an inner triangle or on an inner edge between two (not
    /// on the boundary), farther than `min` from the corners there.
    fn inside_point(&self, uv: DVec2, q: DVec3, min: f64, inside: &[bool]) -> bool {
        use spade::PositionInTriangulation as P;
        if !uv.is_finite() {
            return false;
        }
        let ins = |f: spade::handles::FaceHandle<_, _, _, _, _>| f.as_inner().is_some_and(|f| inside[f.fix().index()]);
        let far = |vs: &[spade::handles::VertexHandle<Point2<f64>, (), spade::CdtEdge<()>, ()>]| {
            vs.iter().all(|v| (self.info[v.fix().index()].p - q).length() > min)
        };
        match self.cdt.locate(self.point(uv)) {
            P::OnFace(f) => inside[f.index()] && far(&self.cdt.face(f).vertices()),
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
        let mut out = [Vec::new(), Vec::new()];
        for i in 0..2 {
            if let Some(r) = radii[i] {
                let (a, b) = if i == 0 { (lo.x, hi.x) } else { (lo.y, hi.y) };
                out[i] = uniform(a, b, step_angle(r, p.tol, p.angle));
            }
        }
        return out;
    }
    // general: lines from the derivatives sampled over the bounds
    let mut out = [Vec::new(), Vec::new()];
    for (i, lines) in out.iter_mut().enumerate() {
        let (a, b) = if i == 0 { (lo.x, hi.x) } else { (lo.y, hi.y) };
        if !crate::gt(b, a) {
            continue;
        }
        let (c, d) = if i == 0 { (lo.y, hi.y) } else { (lo.x, hi.x) };
        let pieces = g.breaks(i, a, b);
        // lines needed per unit of the parameter on each piece
        let mut density = Vec::with_capacity(pieces.len());
        for w in pieces.windows(2) {
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
            density.push((d2 / (8.0 * p.tol)).sqrt().max(turn / p.angle));
        }
        // lines evenly over the cumulated need (knots without need get none)
        let need: Vec<f64> = pieces.windows(2).zip(&density).map(|(w, r)| (w[1] - w[0]) * r).collect();
        let total: f64 = need.iter().sum();
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
        *lines = all.into_iter().filter(|t| *t > a && *t < b).collect();
    }
    out
}

/// A straight direction (cylinder, cone and extrusion along v) needs no lines for the chord, but
/// where the boundary runs across the grid (a mitre, a hole), Delaunay fans long triangles from
/// it over the face. Lines then at twice the other direction's spacing over the span of such a
/// run (a run of boundary segments that change both parameters and span more than two grid
/// steps along the straight direction) and a little beyond, at sixteen times elsewhere.
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
        let fine = 2.0 * step3 / pick(sc, i);
        let mut out = Vec::new();
        let mut t = a;
        while t < b {
            let near = spans.iter().any(|(x, y)| t >= x - 4.0 * fine && t <= y + 4.0 * fine);
            t += if near { fine } else { 8.0 * fine };
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
        DVec2::new(v.uv.x * b.scale.x, v.uv.y * b.scale.y)
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
    let uv = DVec2::new(q.x / b.scale.x, q.y / b.scale.y);
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
    let sc = scale(g, bounds);
    straight_lines(&loops, &mut lines, bounds, sc);
    split_poles(&mut loops, &lines);
    let mut b = Builder { g, cdt: Cdt::new(), info: Vec::new(), scale: sc, bounds };
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
    // grid
    if !g.is_plane() {
        let step = |ls: &[f64], a: f64, c: f64| {
            if ls.is_empty() { (c - a).max(0.0) } else { (c - a) / (ls.len() + 1) as f64 }
        };
        let margin = DVec2::new(step(&lines[0], lo.x, hi.x), step(&lines[1], lo.y, hi.y)) * 0.3;
        for uv in grid_points(&loops, &lines, margin) {
            b.interior(uv);
        }
    }
    // refinement
    let mut inside = b.inside();
    if !g.is_plane() {
        for _ in 0..p.max_passes {
            if b.cdt.num_inner_faces() > p.max_triangles {
                break;
            }
            let (add, worst) = refine_pass(&b, &inside, p.tol);
            fm.deviation = worst;
            if add.is_empty() {
                break;
            }
            // only points inside the face and off its boundary (a point on a boundary edge
            // would not be on the neighbour's side of it)
            // and none within `tol / 20` of a vertex or of another new point
            let mut seen: std::collections::HashMap<glam::I64Vec3, Vec<DVec3>> = Default::default();
            let cell = 0.05 * p.tol;
            let ok: Vec<DVec2> = add
                .into_iter()
                .filter(|uv| {
                    let q = b.g.eval(*uv);
                    let key = (q / cell).floor().as_i64vec3();
                    let mut near = false;
                    for dx in -1..=1 {
                        for dy in -1..=1 {
                            for dz in -1..=1 {
                                let k = key + glam::I64Vec3::new(dx, dy, dz);
                                near |= seen.get(&k).is_some_and(|v| v.iter().any(|o| (*o - q).length() <= cell));
                            }
                        }
                    }
                    if near || !b.inside_point(*uv, q, cell, &inside) {
                        return false;
                    }
                    seen.entry(key).or_default().push(q);
                    true
                })
                .collect();
            if ok.is_empty() {
                break;
            }
            for uv in &ok {
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
    for f in b.cdt.inner_faces() {
        let mut vs = f.vertices();
        if crossed {
            let c = vs.iter().map(|v| b.info[v.fix().index()].uv).sum::<DVec2>() / 3.0;
            let w = winding(&loops, c);
            if w == 0 {
                continue;
            }
            if (w > 0) != (orient > 0.0) {
                vs.swap(1, 2);
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
    }
    fm
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

/// Points to insert (see the module docs) and the largest deviation found.
fn refine_pass(b: &Builder, inside: &[bool], tol: f64) -> (Vec<DVec2>, f64) {
    let mut add = Vec::new();
    let mut worst: f64 = 0.0;
    let info = |h: spade::handles::VertexHandle<Point2<f64>, (), spade::CdtEdge<()>, ()>| b.info[h.fix().index()];
    for f in b.cdt.inner_faces() {
        if !inside[f.fix().index()] {
            continue;
        }
        let [a, c, d] = f.vertices().map(info);
        let n = (c.p - a.p).cross(d.p - a.p);
        let area2 = n.length();
        let scale2 = (c.p - a.p).length_squared().max((d.p - a.p).length_squared());
        if !crate::gt(area2, 1e-14 * scale2) || scale2.max((d.p - c.p).length_squared()) < (0.1 * tol).powi(2) {
            continue; // a pole triangle, a sliver or a tiny one: nothing to measure
        }
        let n = n / area2;
        let uvc = (a.uv + c.uv + d.uv) / 3.0;
        let (lo, hi) = (a.uv.min(c.uv).min(d.uv), a.uv.max(c.uv).max(d.uv));
        let (foot, dev) = b.foot((a.p + c.p + d.p) / 3.0, uvc, (lo, hi), n);
        worst = worst.max(dev);
        let flipped = b.g.normal(uvc, b.bounds).is_some_and(|m| m.dot(n) < 0.0);
        if dev > tol || flipped {
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
                Some((l2, mid)) if l2 >= 0.5 * longest => add.push(mid),
                _ => add.push(foot),
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
        let [v0, v1] = e.vertices().map(info);
        let mid = (v0.uv + v1.uv) * 0.5;
        let ab = v1.p - v0.p;
        if ab.length_squared() < (0.1 * tol).powi(2) {
            continue;
        }
        let (lo, hi) = (v0.uv.min(v1.uv), v0.uv.max(v1.uv));
        let (foot, dev) = b.foot((v0.p + v1.p) * 0.5, mid, (lo, hi), DVec3::ZERO);
        worst = worst.max(dev);
        if dev > tol {
            add.push(foot);
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
