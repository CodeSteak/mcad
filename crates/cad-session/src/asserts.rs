//! Geometric assertions (Plan 21 C): `assert_keepout`, `assert_disjoint`, `assert_intersect`,
//! `assert_touches`. Checked after the kernel job of a full evaluation (not while a handle is
//! dragged), one pass per evaluation; a violation is a warning at the statement.
//!
//! * **Bodies.** A child of an assertion is one body set (all solids and meshes of its result,
//!   placed in context coordinates). `assert_keepout` checks its children (the keep-out
//!   volume) against every real body of the model: each solid and each mesh of the parts and
//!   of the material outside them (as the colour list), or of the root.
//! * **Intersection.** Exact kernel boolean of each pair of solids whose boxes meet, volume
//!   from the kernel's mass properties; with a mesh involved (`import("x.stl")` without B-rep)
//!   only yes/no on the meshes ([`cad_measure::dist::solids_overlap`]). A failing boolean (faces
//!   lying on each other are a borderline case for SMLib) is reported as "nicht prüfbar".
//! * **Distance.** Smallest distance of the tessellations ([`cad_measure::dist::min_distance`])
//!   with foot points, accuracy ± the sum of the chordal tolerances (planar faces are exact).
//! * **Contact** (`assert_touches`): face pairs near each other on the exact B-rep
//!   ([`cad_measure::Body`]): opposite coplanar planes → face contact, area of the intersection
//!   of both face outlines with `region2` (exact for lines and arcs); cylinder/sphere on a
//!   plane, cylinder on cylinder → line or point contact; other surfaces only on the mesh.
//! * **Tolerances**, relative to the size `L` (diagonal of the bounds of both bodies):
//!   intersection volume counts above `ε_V = 1e-9 · L³` (at least 1e-9 mm³), contact area above
//!   `1e-9 · L²`; tessellation tolerance `clamp(2e-4 · L, 0.001, 0.02)` mm.
//! * **Cache.** Each pair's result lives in the engine cache under a key of the node keys,
//!   placements, kind and parameters: `watch`, the daemon and the UI recompute only changed
//!   pairs.
//!
//! `assert_intersect` asks for a connected assembly: every child must intersect another one and
//! the children must form one connected group (a chain `a`–`b`–`c` is fine; pairwise would make
//! the middle piece of a bracket a violation). Touching alone is no intersection
//! (`assert_touches`).

use std::rc::Rc;

use cad_engine::{CacheKey, Evaluation, KeyHasher, MeshPart};
use cad_ir::{AssertDecl, AssertKind, Contact, NodeIdx};
use cad_kernel::brep::Surface;
use cad_kernel::curve2::Segment;
use cad_kernel::region2::{self, Fill, Operand};
use cad_kernel::{BoolOp, Kernel, Mesh};
use cad_measure::dist::{self, Tri};
use cad_measure::{Body, EdgeCurve};
use glam::{DMat4, DVec2, DVec3};

use crate::Session;
use crate::measure::{num, vec_text};
use crate::protocol::{self as p, FromEngine};

/// Version of the cached pair data (part of the key).
const PAIR_VERSION: &str = "assert-pair/1";

/// Tolerances of a pair (see the module docs).
#[derive(Debug, Clone, Copy)]
struct Tols {
    eps_volume: f64,
    eps_area: f64,
    eps_len: f64,
    mesh: f64,
}

impl Tols {
    fn of(lo: DVec3, hi: DVec3) -> Tols {
        let l = if lo.x.is_finite() && hi.x.is_finite() { (hi - lo).length().max(1e-3) } else { 1.0 };
        Tols {
            eps_volume: (1e-9 * l * l * l).max(1e-9),
            eps_area: (1e-9 * l * l).max(1e-12),
            eps_len: (1e-6 * l).max(1e-9),
            mesh: (2e-4 * l).clamp(0.001, 0.02),
        }
    }
}

/// A placed solid with its tessellation (context coordinates) and its box.
type Placed<S> = (S, Rc<Mesh>, (DVec3, DVec3));

/// One body set: solids and meshes of a node, placed with its display matrix.
struct BodySet<S> {
    node: NodeIdx,
    id: CacheKey,
    world: DMat4,
    /// Indices into `Evaluation::parts` (solids only).
    parts: Vec<usize>,
    /// Indices into `Evaluation::meshes`.
    meshes: Vec<usize>,
    /// Box in context coordinates (of the node's result).
    lo: DVec3,
    hi: DVec3,
    label: String,
    /// Placed solids, their tessellations (context coordinates) and boxes, built on demand.
    placed: Option<Vec<Placed<S>>>,
    placed_meshes: Option<Vec<MeshPart>>,
}

impl<S: Clone> BodySet<S> {
    fn solids<K: Kernel<Shape = S>>(
        &mut self,
        k: &mut K,
        ev: &Evaluation<S>,
        tol: f64,
    ) -> Result<&[Placed<S>], String> {
        if self.placed.is_none() {
            let mut v = Vec::new();
            for &i in &self.parts {
                let s = &ev.parts(self.node)[i].shape;
                let s = if self.world == DMat4::IDENTITY {
                    s.clone()
                } else {
                    k.transform(s, &self.world).map_err(|e| e.to_string())?
                };
                if k.is_empty(&s) {
                    continue;
                }
                let m = k.tessellate(&s, tol).map_err(|e| e.to_string())?;
                let b = k.bounds(&s).ok().flatten().or_else(|| m.bounds()).unwrap_or((self.lo, self.hi));
                v.push((s, Rc::new(m), b));
            }
            self.placed = Some(v);
        }
        Ok(self.placed.as_deref().unwrap_or(&[]))
    }

    fn meshes(&mut self, ev: &Evaluation<S>) -> &[MeshPart] {
        if self.placed_meshes.is_none() {
            let v = self.meshes.iter().map(|&i| ev.meshes(self.node)[i].placed(&self.world)).collect();
            self.placed_meshes = Some(v);
        }
        self.placed_meshes.as_deref().unwrap_or(&[])
    }

    /// All triangles (solids and meshes) with the engine tag of each.
    fn triangles<K: Kernel<Shape = S>>(
        &mut self,
        k: &mut K,
        ev: &Evaluation<S>,
        tol: f64,
    ) -> Result<(Vec<Tri>, Vec<u32>), String> {
        let mut tris = Vec::new();
        let mut tags = Vec::new();
        for (_, m, _) in self.solids(k, ev, tol)? {
            tris.extend(dist::triangles(m, &DMat4::IDENTITY));
            tags.extend(m.tri_tags.iter().copied());
        }
        for mp in self.meshes(ev) {
            let t = mp.triangles(&DMat4::IDENTITY);
            tags.extend(std::iter::repeat_n(mp.tag, t.len()));
            tris.extend(t);
        }
        Ok((tris, tags))
    }
}

/// Contact of two bodies (`assert_touches`).
#[derive(Debug, Clone, Default)]
struct ContactData {
    /// `face`, `line`, `point` or `mesh`.
    kind: &'static str,
    area: f64,
    exact: bool,
    tris: Vec<[DVec3; 3]>,
    lines: Vec<Vec<DVec3>>,
}

/// What was computed for a pair (tag-independent, cached).
#[derive(Debug, Clone, Default)]
struct PairData {
    /// Exact intersection volume of the solids (sum over solid pairs above ε).
    volume: f64,
    overlap_tris: Vec<[DVec3; 3]>,
    /// Feature edges of the intersection (segments).
    overlap_edges: Vec<[DVec3; 2]>,
    overlap_tags: Vec<u32>,
    overlap_box: Option<(DVec3, DVec3)>,
    /// Decided on meshes (a mesh without B-rep involved).
    mesh_overlap: bool,
    mesh_based: bool,
    bool_failed: Option<String>,
    /// Distance of the tessellations: (d, foot a, foot b, tag a, tag b).
    distance: Option<(f64, DVec3, DVec3, u32, u32)>,
    /// Tags of the faces of a and b nearest to the intersection (its faces may all come from
    /// one side, e.g. a keep-out volume inside a body).
    near_tags: (u32, u32),
    contact: Option<ContactData>,
    tols: Option<Tols>,
}

impl PairData {
    fn overlaps(&self, eps: f64) -> bool {
        self.volume > eps || self.mesh_overlap
    }

    fn bytes(&self) -> usize {
        256 + 72 * self.overlap_tris.len()
            + 48 * self.overlap_edges.len()
            + self
                .contact
                .as_ref()
                .map_or(0, |c| 72 * c.tris.len() + c.lines.iter().map(|l| 24 * l.len()).sum::<usize>())
    }
}

#[derive(Clone, Copy)]
struct Need {
    distance: bool,
    contact: bool,
}

fn box_gap(a: (DVec3, DVec3), b: (DVec3, DVec3)) -> f64 {
    let d = (b.0 - a.1).max(a.0 - b.1).max(DVec3::ZERO);
    d.length()
}

fn merge(a: Option<(DVec3, DVec3)>, b: (DVec3, DVec3)) -> Option<(DVec3, DVec3)> {
    Some(match a {
        None => b,
        Some(a) => (a.0.min(b.0), a.1.max(b.1)),
    })
}

fn mesh_tris(m: &Mesh) -> (Vec<[DVec3; 3]>, Vec<u32>) {
    (dist::triangles(m, &DMat4::IDENTITY), m.tri_tags.clone())
}

/// Computes the data of one pair.
fn compute_pair<K: Kernel>(
    k: &mut K,
    ev: &Evaluation<K::Shape>,
    a: &mut BodySet<K::Shape>,
    b: &mut BodySet<K::Shape>,
    need: Need,
    tol_contact: f64,
) -> PairData {
    let t = Tols::of(a.lo.min(b.lo), a.hi.max(b.hi));
    let mut d = PairData { tols: Some(t), ..Default::default() };
    let tm = t.mesh;
    let run = (|| -> Result<(), String> {
        // exact intersection of the solids
        let sa: Vec<_> = a.solids(k, ev, tm)?.to_vec();
        let sb: Vec<_> = b.solids(k, ev, tm)?.to_vec();
        for (x, _, bx) in &sa {
            for (y, _, by) in &sb {
                if box_gap(*bx, *by) > t.eps_len {
                    continue;
                }
                match k.boolean(BoolOp::Intersection, &[x.clone(), y.clone()], 0) {
                    Ok(i) => {
                        if k.is_empty(&i) {
                            continue;
                        }
                        let v = match k.mass_props(&i) {
                            Ok(m) => m.volume,
                            Err(e) => {
                                d.bool_failed = Some(e.to_string());
                                continue;
                            }
                        };
                        if v <= t.eps_volume {
                            continue;
                        }
                        d.volume += v;
                        if let Ok(m) = k.tessellate(&i, tm) {
                            let (tris, tags) = mesh_tris(&m);
                            if let Some(bb) = m.bounds() {
                                d.overlap_box = merge(d.overlap_box, bb);
                            }
                            d.overlap_tris.extend(tris);
                            d.overlap_tags.extend(tags);
                            let q = |k: usize| {
                                DVec3::new(
                                    m.edge_segments[k] as f64,
                                    m.edge_segments[k + 1] as f64,
                                    m.edge_segments[k + 2] as f64,
                                )
                            };
                            d.overlap_edges.extend((0..m.edge_segments.len() / 6).map(|i| [q(6 * i), q(6 * i + 3)]));
                        }
                    }
                    Err(e) => d.bool_failed = Some(e.to_string()),
                }
            }
        }
        // meshes without B-rep: yes/no on the triangles
        let ma = !a.meshes(ev).is_empty();
        let mb = !b.meshes(ev).is_empty();
        if ma || mb {
            d.mesh_based = true;
            let (ta, _) = a.triangles(k, ev, tm)?;
            let (tb, _) = b.triangles(k, ev, tm)?;
            let only = |set: &mut BodySet<K::Shape>| -> Vec<Tri> {
                set.meshes(ev).iter().flat_map(|mp| mp.triangles(&DMat4::IDENTITY)).collect()
            };
            let (oa, ob) = (only(a), only(b));
            d.mesh_overlap = (!oa.is_empty() && dist::solids_overlap(&oa, &tb))
                || (!ob.is_empty() && dist::solids_overlap(&ta, &ob));
        }
        let tag_at = |tris: &[Tri], tags: &[u32], q: DVec3| {
            let mut best = (f64::INFINITY, 0u32);
            for (t, g) in tris.iter().zip(tags) {
                let dd = dist::closest_on_tri(q, t).distance(q);
                if dd < best.0 {
                    best = (dd, *g);
                }
            }
            best.1
        };
        if let Some((lo, hi)) = d.overlap_box.filter(|_| d.overlaps(t.eps_volume)) {
            let c = (lo + hi) * 0.5;
            let (ta, ga) = a.triangles(k, ev, tm)?;
            let (tb, gb) = b.triangles(k, ev, tm)?;
            d.near_tags = (tag_at(&ta, &ga, c), tag_at(&tb, &gb, c));
        }
        if need.distance && !d.overlaps(t.eps_volume) {
            let (ta, ga) = a.triangles(k, ev, tm)?;
            let (tb, gb) = b.triangles(k, ev, tm)?;
            if let Some(m) = dist::min_distance(&ta, &tb) {
                d.distance = Some((m.dist, m.a, m.b, tag_at(&ta, &ga, m.a), tag_at(&tb, &gb, m.b)));
            }
        }
        if need.contact && !d.overlaps(t.eps_volume) && d.distance.is_some_and(|x| x.0 <= tol_contact + 2.0 * tm) {
            d.contact = Some(contact(k, ev, a, b, tol_contact, t)?);
        }
        Ok(())
    })();
    if let Err(e) = run {
        d.bool_failed.get_or_insert(e);
    }
    d
}

// ---------------------------------------------------------------- contact analysis

/// Outward unit normal of a planar face.
fn plane_normal(s: &Surface, same_sense: bool) -> Option<(DVec3, DVec3)> {
    match s {
        Surface::Plane { frame } => Some((frame.origin, if same_sense { frame.z } else { -frame.z })),
        _ => None,
    }
}

/// 2D segments of a face outline in the plane frame (o, x, y): exact for lines and arcs in the
/// plane, polylines otherwise (`exact` cleared).
fn face_loops(b: &Body, f: usize, o: DVec3, x: DVec3, y: DVec3, n: DVec3, exact: &mut bool) -> Vec<Vec<Segment>> {
    let to2 = |q: DVec3| DVec2::new((q - o).dot(x), (q - o).dot(y));
    let mut out = Vec::new();
    for lp in &b.faces[f].loops {
        let mut segs = Vec::new();
        for &(e, fwd) in lp {
            let c = &b.edges[e as usize];
            let c = if fwd { c.clone() } else { c.reversed() };
            match &c {
                EdgeCurve::Line { a, b } => segs.push(Segment::Line { a: to2(*a), b: to2(*b) }),
                EdgeCurve::Arc { frame, r, a0, sweep } if frame.z.cross(n).length() < 1e-9 => {
                    let c2 = to2(frame.origin);
                    let s = to2(c.start());
                    let start = (s - c2).y.atan2((s - c2).x);
                    let dir = if frame.z.dot(n) > 0.0 { 1.0 } else { -1.0 };
                    let _ = a0;
                    segs.push(Segment::Arc { center: c2, r: *r, a0: start, a1: start + dir * sweep });
                }
                other => {
                    *exact = false;
                    let pts = other.polyline(48);
                    for w in pts.windows(2) {
                        segs.push(Segment::Line { a: to2(w[0]), b: to2(w[1]) });
                    }
                }
            }
        }
        if !segs.is_empty() {
            out.push(segs);
        }
    }
    out
}

fn seg_points(s: &Segment, n: usize) -> Vec<DVec2> {
    match s {
        Segment::Line { a, b } => vec![*a, *b],
        Segment::Arc { center, r, a0, a1 } => {
            let k = ((a1 - a0).abs() / std::f64::consts::TAU * n as f64).ceil().max(2.0) as usize;
            (0..=k).map(|i| *center + DVec2::from_angle(a0 + (a1 - a0) * i as f64 / k as f64) * *r).collect()
        }
        other => other.sample(0.01),
    }
}

/// Contact of two coincident opposite planar faces: area and outline of the intersection of
/// both outlines (`region2`, exact for lines and arcs).
fn plane_contact(
    fa: (&Body, usize),
    fb: (&Body, usize),
    o: DVec3,
    n: DVec3,
    c: &mut ContactData,
    eps_area: f64,
) -> bool {
    let x = n.any_orthonormal_vector();
    let y = n.cross(x);
    let mut exact = true;
    let la = face_loops(fa.0, fa.1, o, x, y, n, &mut exact);
    let lb = face_loops(fb.0, fb.1, o, x, y, n, &mut exact);
    if la.is_empty() || lb.is_empty() {
        return false;
    }
    let ops = [Operand::new(la, Fill::EvenOdd), Operand::new(lb, Fill::EvenOdd)];
    let res = region2::combine(&ops, &|v: &[bool]| v[0] && v[1]);
    let area = region2::region_area(&res).abs();
    if area <= eps_area {
        return false;
    }
    let to3 = |q: DVec2| o + x * q.x + y * q.y;
    let mut polys2: Vec<Vec<DVec2>> = Vec::new();
    for lp in &res {
        let mut pts: Vec<DVec2> = Vec::new();
        for s in lp {
            let sp = seg_points(s, 64);
            let skip = usize::from(!pts.is_empty());
            pts.extend(sp.into_iter().skip(skip));
        }
        if pts.len() >= 3 {
            c.lines.push(pts.iter().map(|q| to3(*q)).chain(std::iter::once(to3(pts[0]))).collect());
            polys2.push(pts);
        }
    }
    let (pts, idx) = cad_engine::fill_loops(&polys2);
    for t in idx.chunks_exact(3) {
        c.tris.push([to3(pts[t[0] as usize]), to3(pts[t[1] as usize]), to3(pts[t[2] as usize])]);
    }
    c.area += area;
    c.exact &= exact;
    true
}

fn tri_area(t: &Tri) -> f64 {
    0.5 * (t[1] - t[0]).cross(t[2] - t[0]).length()
}

fn tris_box(t: &[Tri]) -> Option<(DVec3, DVec3)> {
    let mut b: Option<(DVec3, DVec3)> = None;
    for tr in t {
        for q in tr {
            b = merge(b, (*q, *q));
        }
    }
    b
}

/// Distance of a point to a triangle set.
fn point_dist(q: DVec3, tris: &[Tri]) -> f64 {
    tris.iter().map(|t| dist::closest_on_tri(q, t).distance(q)).fold(f64::INFINITY, f64::min)
}

/// Contact of two close faces on the meshes: area of the triangles of `a` lying on `b`
/// (within `lim`), else the near vertices (their extent decides line or point).
fn mesh_contact(ta: &[Tri], tb: &[Tri], lim: f64, c: &mut ContactData, eps_area: f64, line_len: f64) -> &'static str {
    let mut area = 0.0;
    let mut near: Vec<DVec3> = Vec::new();
    let mut on = Vec::new();
    for t in ta {
        let d: Vec<bool> = t.iter().map(|q| point_dist(*q, tb) <= lim).collect();
        if d.iter().all(|x| *x) {
            area += tri_area(t);
            on.push(*t);
        }
        for (q, dd) in t.iter().zip(&d) {
            if *dd {
                near.push(*q);
            }
        }
    }
    // vertices of b near a (a cylinder's generator on a large planar face of a)
    for t in tb {
        for q in t {
            if point_dist(*q, ta) <= lim {
                near.push(*q);
            }
        }
    }
    if area > eps_area {
        c.area += area;
        c.exact = false;
        c.tris.extend(on);
        return "face";
    }
    let Some(bb) = near.iter().fold(None, |b, q| merge(b, (*q, *q))) else { return "mesh" };
    if (bb.1 - bb.0).length() > line_len {
        // the two farthest near vertices along the box diagonal
        let dir = (bb.1 - bb.0).normalize();
        let lo = near.iter().copied().min_by(|p, q| p.dot(dir).total_cmp(&q.dot(dir))).unwrap_or(bb.0);
        let hi = near.iter().copied().max_by(|p, q| p.dot(dir).total_cmp(&q.dot(dir))).unwrap_or(bb.1);
        c.lines.push(vec![lo, hi]);
        "line"
    } else {
        c.lines.push(vec![bb.0.lerp(bb.1, 0.5)]);
        "point"
    }
}

fn rank(kind: &str) -> u8 {
    match kind {
        "face" => 4,
        "line" => 3,
        "point" => 2,
        "mesh" => 1,
        _ => 0,
    }
}

/// Distance of point `q` to the line through `o` along unit `d`.
fn line_dist(q: DVec3, o: DVec3, d: DVec3) -> f64 {
    let v = q - o;
    (v - d * v.dot(d)).length()
}

/// Contact kind of two close faces from their surfaces; `None`: decide on the mesh.
fn analytic_contact(sa: &Surface, sb: &Surface, tol: f64) -> Option<&'static str> {
    use Surface::*;
    match (sa, sb) {
        (Plane { frame: p }, Cylinder { frame: c, r }) | (Cylinder { frame: c, r }, Plane { frame: p }) => {
            let n = p.z;
            if c.z.dot(n).abs() < 1e-6 && ((c.origin - p.origin).dot(n).abs() - r).abs() <= tol {
                Some("line")
            } else {
                None
            }
        }
        (Plane { frame: p }, Sphere { frame: s, r }) | (Sphere { frame: s, r }, Plane { frame: p }) => {
            (((s.origin - p.origin).dot(p.z)).abs() - r).abs().le(&tol).then_some("point")
        }
        (Cylinder { frame: a, r: ra }, Cylinder { frame: b, r: rb }) => {
            if a.z.cross(b.z).length() < 1e-6 {
                let d = line_dist(b.origin, a.origin, a.z);
                if d <= tol && (ra - rb).abs() <= tol {
                    None // coaxial, equal radius: area contact (mesh estimate)
                } else if (d - (ra + rb)).abs() <= tol || ((d - (ra - rb).abs()).abs() <= tol && (ra - rb).abs() > tol)
                {
                    Some("line")
                } else {
                    None
                }
            } else {
                // skew axes: distance of the axes
                let n = a.z.cross(b.z).normalize();
                let d = (b.origin - a.origin).dot(n).abs();
                ((d - (ra + rb)).abs() <= tol).then_some("point")
            }
        }
        (Sphere { frame: a, r: ra }, Sphere { frame: b, r: rb }) => {
            let d = a.origin.distance(b.origin);
            ((d - (ra + rb)).abs() <= tol || (d - (ra - rb).abs()).abs() <= tol).then_some("point")
        }
        (Sphere { frame: s, r }, Cylinder { frame: c, r: rc })
        | (Cylinder { frame: c, r: rc }, Sphere { frame: s, r }) => {
            let d = line_dist(s.origin, c.origin, c.z);
            ((d - (r + rc)).abs() <= tol).then_some("point")
        }
        _ => None,
    }
}

/// Contact analysis of two body sets that are within `tol` of each other.
fn contact<K: Kernel>(
    k: &mut K,
    ev: &Evaluation<K::Shape>,
    a: &mut BodySet<K::Shape>,
    b: &mut BodySet<K::Shape>,
    tol: f64,
    t: Tols,
) -> Result<ContactData, String> {
    let tm = t.mesh;
    let lim = tol + 2.0 * tm;
    let mut c = ContactData { kind: "", exact: true, ..Default::default() };
    let bodies = |k: &mut K, set: &mut BodySet<K::Shape>| -> Result<Vec<Body>, String> {
        let mut v = Vec::new();
        for (s, m, _) in set.solids(k, ev, tm)?.to_vec() {
            let brep = k.brep(&s).map_err(|e| e.to_string())?;
            v.push(Body::new(Rc::new(brep), &m, &DMat4::IDENTITY, tm, true));
        }
        Ok(v)
    };
    let ba = bodies(k, a)?;
    let bb = bodies(k, b)?;
    let line_len = 10.0 * tm;
    let mut by_kind: Vec<(&'static str, Vec<Vec<DVec3>>, bool)> = Vec::new();
    for x in &ba {
        for y in &bb {
            for (fi, fa) in x.faces.iter().enumerate() {
                let Some(ba_box) = tris_box(&fa.tris) else { continue };
                for (fj, fb) in y.faces.iter().enumerate() {
                    let Some(bb_box) = tris_box(&fb.tris) else { continue };
                    if box_gap(ba_box, bb_box) > lim {
                        continue;
                    }
                    if dist::min_distance(&fa.tris, &fb.tris).is_none_or(|m| m.dist > lim) {
                        continue;
                    }
                    // exactness per face pair (only that of the kind that counts is reported)
                    c.exact = true;
                    let kind =
                        match (plane_normal(&fa.surface, fa.same_sense), plane_normal(&fb.surface, fb.same_sense)) {
                            (Some((oa, na)), Some((ob, nb))) => {
                                if na.dot(nb) < -1.0 + 1e-9 && (ob - oa).dot(na).abs() <= tol + t.eps_len {
                                    if plane_contact((x, fi), (y, fj), oa, na, &mut c, t.eps_area) {
                                        "face"
                                    } else {
                                        ""
                                    }
                                } else if na.dot(nb) > 1.0 - 1e-9 {
                                    "" // same side: no contact of these faces
                                } else {
                                    mesh_contact(&fa.tris, &fb.tris, lim, &mut c, t.eps_area, line_len)
                                }
                            }
                            _ => match analytic_contact(&fa.surface, &fb.surface, tol + t.eps_len) {
                                Some(kd) => {
                                    // where: the near vertices on the mesh (display only)
                                    let mut tmp = ContactData::default();
                                    mesh_contact(&fa.tris, &fb.tris, lim, &mut tmp, f64::INFINITY, line_len);
                                    c.lines.extend(tmp.lines);
                                    kd
                                }
                                None => {
                                    c.exact = false;
                                    mesh_contact(&fa.tris, &fb.tris, lim, &mut c, t.eps_area, line_len)
                                }
                            },
                        };
                    if rank(kind) > rank(c.kind) {
                        c.kind = kind;
                    }
                    if !kind.is_empty() {
                        by_kind.push((kind, std::mem::take(&mut c.lines), c.exact));
                    }
                }
            }
        }
    }
    // only the outlines of the kind that counts (face contact: no extra edge points)
    c.exact = by_kind.iter().filter(|(kd, ..)| *kd == c.kind).all(|(.., ex)| *ex);
    c.lines = by_kind.into_iter().filter(|(kd, ..)| *kd == c.kind).flat_map(|(_, l, _)| l).collect();
    // meshes without B-rep: only on the triangles
    if !a.meshes(ev).is_empty() || !b.meshes(ev).is_empty() {
        let (ta, _) = a.triangles(k, ev, tm)?;
        let (tb, _) = b.triangles(k, ev, tm)?;
        let kind = mesh_contact(&ta, &tb, lim, &mut c, t.eps_area, line_len);
        c.exact = false;
        let kind = if kind == "face" { "face" } else { "mesh" };
        if rank(kind) > rank(c.kind) {
            c.kind = kind;
        }
    }
    if c.kind.is_empty() {
        c.kind = "mesh";
        c.exact = false;
    }
    if c.kind != "face" {
        c.area = 0.0;
    }
    Ok(c)
}

// ---------------------------------------------------------------- session

fn pv(q: DVec3) -> p::Vec3 {
    p::Vec3(q.to_array())
}

fn amesh(tris: &[[DVec3; 3]], edges: &[[DVec3; 2]]) -> Option<p::AssertMesh> {
    if tris.is_empty() {
        return None;
    }
    let f = |q: &DVec3| [q.x as f32, q.y as f32, q.z as f32];
    let positions = tris.iter().flatten().flat_map(f).collect();
    let edges = edges.iter().flatten().flat_map(f).collect();
    Some(p::AssertMesh { positions, indices: (0..tris.len() as u32 * 3).collect(), edges })
}

fn bounds(b: Option<(DVec3, DVec3)>) -> Option<p::Bounds> {
    b.map(|(lo, hi)| p::Bounds { min: pv(lo), max: pv(hi) })
}

fn kind_p(k: AssertKind) -> p::AssertKind {
    match k {
        AssertKind::Keepout => p::AssertKind::Keepout,
        AssertKind::Disjoint => p::AssertKind::Disjoint,
        AssertKind::Intersect => p::AssertKind::Intersect,
        AssertKind::Touches => p::AssertKind::Touches,
    }
}

/// A pair result before formatting (bodies by index into the sets).
struct Checked {
    a: usize,
    b: usize,
    data: Rc<PairData>,
}

impl<K: Kernel> Session<K>
where
    K::Shape: 'static,
{
    /// Unchecked results of every assertion (while dragging, or before the check).
    pub(crate) fn asserts_unchecked(&self, note: &str) -> Vec<p::AssertResult> {
        let (Some(d), Some(cur)) = (&self.done, &self.cur) else { return Vec::new() };
        d.ev.tree
            .asserts
            .iter()
            .map(|a| self.result_base(a, cur.src.range(a.span), Some(note.to_string()), false))
            .collect()
    }

    fn result_base(&self, a: &AssertDecl, range: p::SrcRange, note: Option<String>, checked: bool) -> p::AssertResult {
        let size = self.done.as_ref().and_then(|d| d.ev.root()).and_then(|r| {
            let d = self.done.as_ref()?;
            let b = d.scene.nodes.get(r as usize)?.bounds.clone()?;
            Some((DVec3::from_array(b.min.0), DVec3::from_array(b.max.0)))
        });
        let t = size.map(|(lo, hi)| Tols::of(lo, hi)).unwrap_or(Tols::of(DVec3::ZERO, DVec3::ONE));
        p::AssertResult {
            node: a.node,
            kind: kind_p(a.kind),
            range,
            ok: true,
            checked,
            note,
            clearance: a.clearance,
            tol: a.tol,
            contact: match a.contact {
                Contact::Face => "face".into(),
                Contact::Any => "any".into(),
            },
            eps_volume: t.eps_volume,
            bodies: 0,
            pairs: Vec::new(),
            ms: 0.0,
            cached: false,
        }
    }

    /// Body sets of a node: one set of all its solids and meshes, or (`split`) one per solid
    /// and per mesh.
    fn sets_of(&self, n: NodeIdx, split: bool, label: &str) -> Vec<BodySet<K::Shape>> {
        let Some(d) = &self.done else { return Vec::new() };
        let ev = &d.ev;
        let rep = ev.report(n);
        if rep.status == cad_engine::Status::Skipped || rep.dim != Some(cad_ir::Dim::Three) {
            return Vec::new();
        }
        let world = rep.display_world;
        let (lo, hi) = match rep.bounds {
            Some((lo, hi)) => transform_box(lo, hi, &world),
            None => (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        };
        let solids: Vec<usize> = ev.parts(n).iter().enumerate().filter(|(_, p)| p.solid).map(|(i, _)| i).collect();
        let meshes: Vec<usize> = (0..ev.meshes(n).len()).collect();
        let mk = |parts: Vec<usize>, meshes: Vec<usize>, idx: u64| BodySet {
            node: n,
            id: KeyHasher::new().str("assert-body").key(&rep.key).mat(&world).u64(idx).finish(),
            world,
            parts,
            meshes,
            lo,
            hi,
            label: label.to_string(),
            placed: None,
            placed_meshes: None,
        };
        if !split {
            if solids.is_empty() && meshes.is_empty() {
                return Vec::new();
            }
            return vec![mk(solids, meshes, u64::MAX)];
        }
        let mut v: Vec<BodySet<K::Shape>> = solids.iter().map(|&i| mk(vec![i], Vec::new(), i as u64)).collect();
        v.extend(meshes.iter().map(|&i| mk(Vec::new(), vec![i], (1u64 << 32) + i as u64)));
        v
    }

    /// Label of a node for messages: the instance name or `Zeile n: <code>` (the statement,
    /// shortened).
    fn node_label(&self, n: NodeIdx) -> String {
        let (Some(d), Some(cur)) = (&self.done, &self.cur) else { return String::new() };
        let sc = &d.scene;
        let Some(node) = sc.nodes.get(n as usize) else { return String::new() };
        if let Some(i) = &node.instance {
            return i.clone();
        }
        // a group of named instances (nested assertion): their names
        let mut names = Vec::new();
        let mut stack = vec![n];
        while let Some(c) = stack.pop() {
            let cn = &sc.nodes[c as usize];
            if cn.flags.disabled || (cn.flags.background && c != n) {
                continue;
            }
            match &cn.instance {
                Some(i) if c != n => names.push(i.clone()),
                _ => stack.extend(cn.children.iter().rev()),
            }
        }
        if !names.is_empty() && names.len() <= 4 {
            return names.join(" + ");
        }
        let span = d.ev.tree.node(n).span;
        let code = cur
            .proj
            .file(span.file())
            .and_then(|f| {
                let l = span.local();
                f.text.get(l.start as usize..l.end as usize)
            })
            .map(|t| {
                let t: String = t
                    .lines()
                    .map(|l| l.split("//").next().unwrap_or(""))
                    .flat_map(|l| l.split_whitespace())
                    .collect::<Vec<_>>()
                    .join(" ");
                let t = t.trim_end_matches(';').to_string();
                if t.chars().count() > 48 { format!("{}…", t.chars().take(47).collect::<String>()) } else { t }
            })
            .unwrap_or_else(|| node.label.clone());
        let file = node.range.file.and_then(|f| sc.files.get(f as usize)).filter(|f| f.id != 0);
        match file {
            Some(f) => format!("{}:Zeile {}: {code}", f.path, node.range.line),
            None => format!("Zeile {}: {code}", node.range.line),
        }
    }

    /// Checks one pair (cache by content key).
    fn check_pair(
        &mut self,
        kind: AssertKind,
        decl: &AssertDecl,
        a: &mut BodySet<K::Shape>,
        b: &mut BodySet<K::Shape>,
        cached: &mut bool,
    ) -> Rc<PairData> {
        let need = match kind {
            AssertKind::Keepout => Need { distance: decl.clearance > 0.0, contact: false },
            AssertKind::Disjoint => Need { distance: false, contact: false },
            AssertKind::Intersect => Need { distance: true, contact: false },
            AssertKind::Touches => Need { distance: true, contact: true },
        };
        let mut h = KeyHasher::new();
        h.str(PAIR_VERSION).key(&a.id).key(&b.id).u64(need.distance as u64).u64(need.contact as u64);
        if need.contact {
            h.f64(decl.tol);
        }
        let key = h.finish();
        if let Some(r) = self.engine.aux_get::<PairData>(&key) {
            return r;
        }
        *cached = false;
        let Some(d) = &self.done else { return Rc::new(PairData::default()) };
        let k = self.engine.kernel_mut();
        let data = Rc::new(compute_pair(k, &d.ev, a, b, need, decl.tol));
        let cost = data.bytes();
        self.engine.aux_insert(key, data.clone(), cost);
        data
    }

    /// Runs all assertions of the last finished evaluation: results in `done.scene.asserts`,
    /// answer `Asserts` and the full `Diagnostics` (language, kernel, assertions).
    pub(crate) fn run_asserts(&mut self) -> Vec<FromEngine> {
        let Some(d) = &self.done else { return Vec::new() };
        let version = d.version;
        let decls = d.ev.tree.asserts.clone();
        let mut results = Vec::new();
        let mut diags = Vec::new();
        for decl in &decls {
            let t0 = (self.clock)();
            let (mut r, mut dg) = self.check_assert(decl);
            r.ms = if r.cached { 0.0 } else { (self.clock)() - t0 };
            results.push(r);
            diags.append(&mut dg);
        }
        let Some(cur) = &self.cur else { return Vec::new() };
        let Some(d) = &mut self.done else { return Vec::new() };
        d.scene.asserts = results.clone();
        let mut items = cur.lang_diagnostics.clone();
        items.extend(crate::convert::kernel_diagnostics(&cur.src, &d.ev));
        items.extend(diags);
        vec![FromEngine::Diagnostics { version, items }, FromEngine::Asserts { version, items: results }]
    }

    fn check_assert(&mut self, decl: &AssertDecl) -> (p::AssertResult, Vec<p::Diagnostic>) {
        let Some(cur) = &self.cur else { unreachable!("asserts without source") };
        let (range, path) = (cur.src.range(decl.span), cur.src.path(decl.span));
        let mut r = self.result_base(decl, range, None, true);
        let d = self.done.as_ref().expect("done");
        let tree = d.ev.tree.clone();
        if d.ev.report(decl.node).status == cad_engine::Status::Skipped {
            r.checked = false;
            r.note = Some("nicht ausgewertet (außerhalb des Ergebnisses)".into());
            return (r, Vec::new());
        }
        let children: Vec<NodeIdx> =
            tree.node(decl.node).children.iter().copied().filter(|&c| !tree.node(c).flags.disabled).collect();
        let mut cached = true;
        let mut checked: Vec<Checked> = Vec::new();
        let mut sets_a: Vec<BodySet<K::Shape>>;
        let mut sets_b: Vec<BodySet<K::Shape>>;
        match decl.kind {
            AssertKind::Keepout => {
                sets_a = self.sets_of(decl.node, false, "Sperrvolumen");
                // the real bodies: parts and the material outside them, or the root
                let targets = self.target_nodes();
                sets_b = Vec::new();
                for (part, n) in targets {
                    let label = part.filter(|p| p != crate::NO_PART).unwrap_or_default();
                    sets_b.extend(self.sets_of(n, true, &label));
                }
                r.bodies = sets_b.len() as u32;
                if sets_a.is_empty() {
                    r.note = Some("Sperrvolumen ohne Körper".into());
                    return (r, Vec::new());
                }
                let (klo, khi) = (sets_a[0].lo, sets_a[0].hi);
                let reach = decl.clearance + Tols::of(klo, khi).eps_len;
                for (j, b) in sets_b.iter_mut().enumerate() {
                    if box_gap((klo, khi), (b.lo, b.hi)) > reach {
                        continue;
                    }
                    let data = self.check_pair(decl.kind, decl, &mut sets_a[0], b, &mut cached);
                    checked.push(Checked { a: 0, b: j, data });
                }
            }
            _ => {
                sets_a = Vec::new();
                for &c in &children {
                    let label = self.node_label(c);
                    sets_a.extend(self.sets_of(c, false, &label));
                }
                sets_b = Vec::new();
                r.bodies = sets_a.len() as u32;
                for i in 0..sets_a.len() {
                    for j in i + 1..sets_a.len() {
                        let (lo, hi) = sets_a.split_at_mut(j);
                        let data = self.check_pair(decl.kind, decl, &mut lo[i], &mut hi[0], &mut cached);
                        checked.push(Checked { a: i, b: j, data });
                    }
                }
            }
        }
        r.cached = cached && !checked.is_empty();
        let other: &Vec<BodySet<K::Shape>> = if decl.kind == AssertKind::Keepout { &sets_b } else { &sets_a };
        let (pairs, diags) = self.format_pairs(decl, (range, path), &sets_a, other, &checked);
        r.ok = pairs.iter().all(|p| p.ok);
        r.pairs = pairs;
        if decl.kind == AssertKind::Keepout {
            r.pairs.retain(|p| !p.ok || p.problem.is_some());
        }
        if checked.iter().any(|c| c.data.mesh_based) {
            r.note = Some("Netz ohne B-Rep beteiligt: Überlappung nur ja/nein".into());
        }
        (r, diags)
    }

    /// Parts and the material outside them (file context), else the root.
    fn target_nodes(&self) -> Vec<(Option<String>, NodeIdx)> {
        let v = self.region_nodes();
        if !v.is_empty() {
            return v;
        }
        let Some(d) = &self.done else { return Vec::new() };
        d.ev.root().map(|r| (None, r)).into_iter().collect()
    }

    /// A body for the answer: its label and the code lines of the faces with these tags that
    /// lie in its subtree (outside keep-out volumes unless it is one).
    fn body_of(&self, set: &BodySet<K::Shape>, tags: &[u32], keepout: bool) -> p::AssertBody {
        let Some(d) = &self.done else {
            return p::AssertBody { node: set.node, label: set.label.clone(), origin: Vec::new() };
        };
        let tree = &d.ev.tree;
        let under = |n: u32| {
            let mut cur = Some(n);
            while let Some(c) = cur {
                if c == set.node {
                    return true;
                }
                cur = tree.node(c).parent;
            }
            false
        };
        let keep = |n: u32| under(n) && tree.in_keepout(n) == keepout;
        let mut uniq: Vec<u32> = Vec::new();
        for &t in tags {
            if t != 0 && !uniq.contains(&t) {
                uniq.push(t);
            }
        }
        let origin = self.origin_of(&uniq, None, None, None, &keep);
        let label = if !set.label.is_empty() {
            set.label.clone()
        } else {
            origin.first().cloned().unwrap_or_else(|| "Körper".into())
        };
        p::AssertBody { node: set.node, label, origin }
    }

    #[allow(clippy::too_many_lines)]
    fn format_pairs(
        &self,
        decl: &AssertDecl,
        (range, path): (p::SrcRange, Option<String>),
        sa: &[BodySet<K::Shape>],
        sb: &[BodySet<K::Shape>],
        checked: &[Checked],
    ) -> (Vec<p::AssertPair>, Vec<p::Diagnostic>) {
        let kind = decl.kind;
        let keep = kind == AssertKind::Keepout;
        let mut pairs = Vec::new();
        for c in checked {
            let dd = &c.data;
            let t = dd.tols.unwrap_or(Tols::of(DVec3::ZERO, DVec3::ONE));
            let (ta, tb) = match dd.distance {
                Some((_, _, _, ga, gb)) => (vec![ga], vec![gb]),
                None => (Vec::new(), Vec::new()),
            };
            let mut tags_a = dd.overlap_tags.clone();
            tags_a.extend(ta);
            tags_a.push(dd.near_tags.0);
            let mut tags_b = dd.overlap_tags.clone();
            tags_b.extend(tb);
            tags_b.push(dd.near_tags.1);
            let a = self.body_of(&sa[c.a], &tags_a, keep);
            let b = self.body_of(&sb[c.b], &tags_b, false);
            let acc = 2.0 * t.mesh;
            let mut pr = p::AssertPair {
                a,
                b,
                ok: true,
                problem: None,
                message: String::new(),
                volume: (!dd.mesh_based || dd.volume > 0.0).then_some(dd.volume),
                mesh_based: dd.mesh_based,
                distance: dd.distance.map(|x| x.0),
                accuracy: acc,
                points: dd.distance.map(|x| vec![pv(x.1), pv(x.2)]).unwrap_or_default(),
                contact: None,
                contact_area: None,
                contact_exact: false,
                overlap: amesh(&dd.overlap_tris, &dd.overlap_edges),
                contact_mesh: None,
                contact_lines: Vec::new(),
                focus: bounds(dd.overlap_box),
            };
            let an = pr.a.label.clone();
            let bn = pr.b.label.clone();
            let overlap = dd.overlaps(t.eps_volume);
            let vol_text = |v: f64| format!("{} mm³", num(v));
            let ov_text = if dd.volume > t.eps_volume {
                format!("Schnittvolumen {}", vol_text(dd.volume))
            } else {
                "netzbasiert, Volumen nicht berechnet".to_string()
            };
            let gap_focus =
                |x: &(f64, DVec3, DVec3, u32, u32)| Some(p::Bounds { min: pv(x.1.min(x.2)), max: pv(x.1.max(x.2)) });
            match kind {
                AssertKind::Keepout => {
                    if overlap {
                        pr.ok = false;
                        pr.problem = Some("overlap".into());
                        pr.message = format!("{bn} ragt ins Sperrvolumen ({ov_text})");
                    } else if let Some(x) = dd.distance.filter(|x| decl.clearance > 0.0 && x.0 < decl.clearance) {
                        pr.ok = false;
                        pr.problem = Some("too_close".into());
                        pr.message = format!(
                            "{bn} hält den Abstand {} mm zum Sperrvolumen nicht ein: {} mm (± {}) von {} bis {}",
                            num(decl.clearance),
                            num(x.0),
                            num(acc),
                            vec_text(x.2),
                            vec_text(x.1)
                        );
                        pr.focus = gap_focus(&x);
                    } else if let Some(e) = &dd.bool_failed {
                        pr.problem = Some("unchecked".into());
                        pr.message = format!("{bn}: Überlappung mit dem Sperrvolumen nicht prüfbar ({e})");
                    } else {
                        pr.message = format!("{bn} frei");
                    }
                }
                AssertKind::Disjoint => {
                    if overlap {
                        pr.ok = false;
                        pr.problem = Some("overlap".into());
                        pr.message = format!("{an} und {bn} überschneiden sich ({ov_text})");
                    } else if let Some(e) = &dd.bool_failed {
                        pr.problem = Some("unchecked".into());
                        pr.message = format!("{an} und {bn}: Überlappung nicht prüfbar ({e})");
                    } else {
                        pr.message = format!("{an} und {bn} getrennt");
                    }
                }
                AssertKind::Intersect => {
                    // decided below over the connected groups
                    pr.message = if overlap {
                        format!("{an} und {bn} schneiden sich ({ov_text})")
                    } else {
                        format!(
                            "{an} und {bn} schneiden sich nicht{}",
                            dd.distance.map(|x| format!(" (Abstand {} mm)", num(x.0))).unwrap_or_default()
                        )
                    };
                    if !overlap && let Some(x) = &dd.distance {
                        pr.focus = gap_focus(x);
                    }
                }
                AssertKind::Touches => {
                    let gap = if overlap { 0.0 } else { dd.distance.map(|x| x.0).unwrap_or(f64::INFINITY) };
                    if gap > decl.tol + acc {
                        pr.ok = false;
                        pr.problem = Some("gap".into());
                        pr.message = match dd.distance {
                            Some(x) => {
                                pr.focus = gap_focus(&x);
                                format!(
                                    "{an} berührt {bn} nicht, Spalt {} mm (± {}) von {} bis {}",
                                    num(x.0),
                                    num(acc),
                                    vec_text(x.1),
                                    vec_text(x.2)
                                )
                            }
                            None => format!("{an} berührt {bn} nicht"),
                        };
                    } else if overlap {
                        pr.ok = false;
                        pr.problem = Some("overlap".into());
                        pr.message = format!("{an} und {bn} überschneiden sich ({ov_text})");
                    } else if let Some(c) = &dd.contact {
                        pr.contact = Some(c.kind.to_string());
                        pr.contact_exact = c.exact;
                        pr.contact_mesh = amesh(&c.tris, &[]);
                        pr.contact_lines = c.lines.iter().map(|l| l.iter().map(|q| pv(*q)).collect()).collect();
                        let cb = c
                            .tris
                            .iter()
                            .flatten()
                            .chain(c.lines.iter().flatten())
                            .fold(None, |b, q| merge(b, (*q, *q)));
                        pr.focus = bounds(cb).or(pr.focus);
                        let note = dd
                            .bool_failed
                            .as_ref()
                            .map(|e| format!("; Überlappung nicht prüfbar ({e})"))
                            .unwrap_or_default();
                        match c.kind {
                            "face" => {
                                pr.contact_area = Some(c.area);
                                pr.message = format!(
                                    "{an} berührt {bn}: Flächenkontakt {} mm²{}{note}",
                                    num(c.area),
                                    if c.exact { "" } else { " (netzbasiert)" }
                                );
                            }
                            "line" | "point" => {
                                let what = if c.kind == "line" { "Linienkontakt" } else { "Punktkontakt" };
                                if decl.contact == Contact::Face {
                                    pr.ok = false;
                                    pr.problem = Some(format!("{}_contact", c.kind));
                                    pr.message = format!(
                                        "{an} berührt {bn} nur mit {what} (nicht-mannigfaltig, schlecht druckbar; contact = \"any\" erlaubt das){note}"
                                    );
                                } else {
                                    pr.message = format!("{an} berührt {bn}: {what}{note}");
                                }
                            }
                            _ => {
                                pr.message = format!("{an} berührt {bn} (Kontaktart nur netzbasiert bestimmt){note}");
                            }
                        }
                        if dd.bool_failed.is_some() && pr.ok {
                            pr.problem = Some("unchecked".into());
                        }
                    } else if let Some(e) = &dd.bool_failed {
                        pr.problem = Some("unchecked".into());
                        pr.message = format!("{an} und {bn}: Überlappung nicht prüfbar ({e})");
                    } else {
                        pr.message = format!("{an} berührt {bn}");
                    }
                }
            }
            pairs.push(pr);
        }
        if kind == AssertKind::Intersect {
            intersect_groups(sa.len(), checked, &mut pairs);
        }
        let mut diags = Vec::new();
        for pr in &pairs {
            let severity = if !pr.ok {
                p::Severity::Warning
            } else if pr.problem.as_deref() == Some("unchecked") {
                p::Severity::Info
            } else {
                continue;
            };
            let at = |b: &p::AssertBody| {
                b.origin.first().filter(|o| **o != b.label).map(|o| format!(" [{o}]")).unwrap_or_default()
            };
            let place = if keep { at(&pr.b) } else { String::new() };
            diags.push(p::Diagnostic {
                severity,
                message: format!("{}: {}{place}", kind.name(), pr.message),
                range,
                source: p::DiagSource::Kernel,
                node: Some(decl.node),
                file: path.clone(),
                at: None,
                nodes: None,
            });
        }
        (pairs, diags)
    }
}

/// `assert_intersect`: the children must form one connected group of intersecting bodies.
/// Pairs inside a group are fine; for every other group the closest pair to the first group
/// is the violation.
fn intersect_groups(n: usize, checked: &[Checked], pairs: &mut [p::AssertPair]) {
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while p[r] != r {
            r = p[r];
        }
        let mut c = x;
        while p[c] != r {
            let nx = p[c];
            p[c] = r;
            c = nx;
        }
        r
    }
    for c in checked {
        let t = c.data.tols.unwrap_or(Tols::of(DVec3::ZERO, DVec3::ONE));
        if c.data.overlaps(t.eps_volume) {
            let (x, y) = (find(&mut parent, c.a), find(&mut parent, c.b));
            parent[x] = y;
        }
    }
    let main = find(&mut parent, 0);
    let mut groups: Vec<usize> = Vec::new();
    for i in 0..n {
        let g = find(&mut parent, i);
        if g != main && !groups.contains(&g) {
            groups.push(g);
        }
    }
    for g in groups {
        // closest pair between the main group and group g
        let mut best: Option<(f64, usize)> = None;
        for (pi, c) in checked.iter().enumerate() {
            let (ga, gb) = (find(&mut parent, c.a), find(&mut parent, c.b));
            if (ga == main && gb == g) || (ga == g && gb == main) {
                let d = c.data.distance.map_or(f64::INFINITY, |x| x.0);
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, pi));
                }
            }
        }
        if let Some((_, pi)) = best {
            let pr = &mut pairs[pi];
            pr.ok = false;
            pr.problem = Some("apart".into());
            if n > 2 {
                pr.message.push_str("; keine Verbindung zu den übrigen Kindern");
            }
        }
    }
}

fn transform_box(lo: DVec3, hi: DVec3, m: &DMat4) -> (DVec3, DVec3) {
    let mut rlo = DVec3::splat(f64::INFINITY);
    let mut rhi = DVec3::splat(f64::NEG_INFINITY);
    for i in 0..8 {
        let c = DVec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        let q = m.transform_point3(c);
        rlo = rlo.min(q);
        rhi = rhi.max(q);
    }
    (rlo, rhi)
}
