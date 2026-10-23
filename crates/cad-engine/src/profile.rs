//! 2D profiles: loops, 2D subtrees ("lifting") and the polygon display of 2D nodes.
//!
//! A 2D subtree (profiles combined by booleans, groups and transforms) is turned into an
//! [`Expr2`]: a boolean expression over leaf profiles, each with its accumulated matrix
//! relative to the system the subtree is lifted into. Profile operations turn every leaf into
//! a solid and rebuild the 2D booleans as 3D booleans of those solids; the 2D display
//! evaluates the same expression with polygon clipping.

use cad_ir::{Contour, NodeIdx, Op, Profile, Tree};
use cad_kernel::{BoolOp, Loop, Mesh, Segment, Tag};
use glam::{DAffine2, DMat2, DMat4, DVec2, DVec3, DVec4};
use i_triangle::float::triangulatable::Triangulatable;
use i_triangle::i_overlay::core::fill_rule::FillRule;
use i_triangle::i_overlay::core::overlay_rule::OverlayRule;
use i_triangle::i_overlay::float::single::SingleFloatOverlay;

const EPS: f64 = 1e-9;

#[derive(Debug, Clone)]
pub(crate) struct Leaf {
    pub node: NodeIdx,
    pub profile: Profile,
    /// Profile system → system the expression was lifted into.
    pub m: DMat4,
}

#[derive(Debug, Clone)]
pub(crate) enum Expr2 {
    Leaf(Leaf),
    Op(BoolOp, Vec<Expr2>),
}

impl Expr2 {
    pub fn leaves(&self) -> Vec<&Leaf> {
        let mut out = Vec::new();
        fn rec<'a>(e: &'a Expr2, out: &mut Vec<&'a Leaf>) {
            match e {
                Expr2::Leaf(l) => out.push(l),
                Expr2::Op(_, a) => a.iter().for_each(|x| rec(x, out)),
            }
        }
        rec(self, &mut out);
        out
    }
}

/// Lifts the 2D node `idx` whose own system maps into the target system by `m`.
/// Children contribute with `m * child.local`. Non-fatal remarks go to `warn`.
pub(crate) fn lift(tree: &Tree, idx: NodeIdx, m: DMat4, warn: &mut Vec<String>) -> Result<Expr2, String> {
    let n = tree.node(idx);
    let kids = |warn: &mut Vec<String>| -> Result<Vec<Expr2>, String> {
        tree.solid_children(idx).map(|c| lift(tree, c, m * tree.node(c).local, warn)).collect()
    };
    match &n.op {
        Op::Profile(Profile::Sketch { contours, closed }) => {
            if !closed {
                // a path, no region (profile operations report it, see `open_sketch_in`)
                return Ok(Expr2::Op(BoolOp::Union, Vec::new()));
            }
            sketch_expr(idx, contours, m)
        }
        Op::Profile(p) => Ok(Expr2::Leaf(Leaf { node: idx, profile: p.clone(), m })),
        Op::Offset { .. } | Op::Blend2 { .. } | Op::Stroke { .. } => {
            let r = crate::op2d::compute(tree, idx);
            match r.as_ref() {
                Ok(o) if o.contours.is_empty() => Ok(Expr2::Op(BoolOp::Union, Vec::new())),
                Ok(o) => sketch_expr(idx, &o.contours, m),
                Err(e) => Err(e.clone()),
            }
        }
        Op::Group | Op::ModuleCall { .. } | Op::Transform { .. } | Op::Color { .. } | Op::Empty => {
            Ok(Expr2::Op(BoolOp::Union, kids(warn)?))
        }
        Op::Boolean { op, fillet, chamfer, fillet_law } => {
            if fillet.is_some() || chamfer.is_some() || fillet_law.is_some() {
                warn.push("fillet/chamfer an 2D-Booleans wird ignoriert".into());
            }
            Ok(Expr2::Op(*op, kids(warn)?))
        }
        Op::Resize { size, auto } => {
            // Bounds of the children in this node's own system, then scale.
            let inner = Expr2::Op(
                BoolOp::Union,
                tree.solid_children(idx).map(|c| lift(tree, c, tree.node(c).local, warn)).collect::<Result<_, _>>()?,
            );
            let Some((lo, hi)) = expr_bounds(&inner, 0.05) else {
                return Ok(inner);
            };
            let ext = hi - lo;
            let s =
                resize_scale(DVec3::new(ext.x, ext.y, 0.0), DVec3::new(size.x, size.y, 0.0), [auto[0], auto[1], false]);
            let sm = m * DMat4::from_scale(DVec3::new(s.x, s.y, 1.0));
            Ok(Expr2::Op(
                BoolOp::Union,
                tree.solid_children(idx)
                    .map(|c| lift(tree, c, sm * tree.node(c).local, warn))
                    .collect::<Result<_, _>>()?,
            ))
        }
        other => Err(format!("{} ist kein 2D-Profil", other.label())),
    }
}

/// Scale factors for `resize`: `size[i] > 0` fixes axis i; `auto[i]` scales axis i like the
/// first fixed axis; otherwise 1.
pub(crate) fn resize_scale(extent: DVec3, size: DVec3, auto: [bool; 3]) -> DVec3 {
    let mut s = [1.0; 3];
    let mut first = None;
    for i in 0..3 {
        if size[i] > 0.0 && extent[i] > EPS {
            s[i] = size[i] / extent[i];
            first.get_or_insert(s[i]);
        }
    }
    if let Some(f) = first {
        for i in 0..3 {
            if size[i] <= 0.0 && auto[i] {
                s[i] = f;
            }
        }
    }
    DVec3::from_array(s)
}

/// Loops of a profile in its own system: the first is the outer loop, the rest are holes.
pub(crate) fn profile_loops(p: &Profile) -> Result<Vec<Loop>, String> {
    match p {
        Profile::Rect { min, max } => {
            if (max.x - min.x).abs() < EPS || (max.y - min.y).abs() < EPS {
                return Err("Rechteck ohne Fläche".into());
            }
            Ok(vec![Loop::Polygon(vec![*min, DVec2::new(max.x, min.y), *max, DVec2::new(min.x, max.y)])])
        }
        Profile::Circle { r } => {
            if *r <= 0.0 {
                return Err(format!("Kreis mit Radius {r}"));
            }
            Ok(vec![Loop::Circle { center: DVec2::ZERO, r: *r }])
        }
        Profile::Polygon { loops } => {
            let loops: Vec<_> = loops.iter().filter(|l| !l.is_empty()).collect();
            if loops.is_empty() {
                return Err("leeres Polygon".into());
            }
            loops
                .iter()
                .map(|l| {
                    if l.len() < 3 {
                        Err("Polygon mit weniger als 3 Punkten".to_string())
                    } else {
                        Ok(Loop::Polygon(l.to_vec()))
                    }
                })
                .collect()
        }
        Profile::Spline { points, closed } => {
            if !closed {
                return Err("offener Spline ist kein Profil".into());
            }
            if points.len() < 3 {
                return Err("Spline mit weniger als 3 Punkten".into());
            }
            Ok(vec![Loop::Spline(points.clone())])
        }
        Profile::Sketch { contours, closed } => {
            if !closed {
                return Err("offene Skizze ist kein Profil (nur als `sweep`-Pfad)".into());
            }
            if contours.is_empty() {
                return Err("leere Skizze".into());
            }
            contours.iter().map(|c| contour_segments(c).map(Loop::Composite)).collect()
        }
    }
}

/// An open sketch in the 2D subtree of `idx` (not a profile: a `sweep` path).
pub(crate) fn open_sketch_in(tree: &Tree, idx: NodeIdx) -> bool {
    match &tree.node(idx).op {
        Op::Profile(Profile::Sketch { closed: false, .. }) => true,
        // 2D operations make regions (stroke turns paths into areas)
        Op::Profile(_) | Op::Offset { .. } | Op::Blend2 { .. } | Op::Stroke { .. } => false,
        _ => tree.solid_children(idx).any(|c| open_sketch_in(tree, c)),
    }
}

/// Bounds of an open sketch (a path: no region), in its own system.
pub(crate) fn open_sketch_bounds(p: &Profile, tol: f64) -> Option<Option<(DVec2, DVec2)>> {
    let Profile::Sketch { contours, closed: false } = p else { return None };
    let mut b: Option<(DVec2, DVec2)> = None;
    for c in contours {
        for g in c.segs.iter().filter_map(|g| g.to_kernel()) {
            for q in g.sample(tol) {
                b = Some(match b {
                    None => (q, q),
                    Some((lo, hi)) => (lo.min(q), hi.max(q)),
                });
            }
        }
    }
    Some(b)
}

/// Display of an open sketch: its path as edge lines (no triangles).
pub(crate) fn open_sketch_mesh(p: &Profile, tol: f64) -> Option<Mesh> {
    let Profile::Sketch { contours, closed: false } = p else { return None };
    let mut mesh = Mesh::default();
    for (ci, c) in contours.iter().enumerate() {
        let segs: Vec<Segment> = c.segs.iter().filter_map(|g| g.to_kernel()).collect();
        let mut pts = cad_kernel::curve2::polyline(&segs, tol);
        if c.closed && !pts.is_empty() {
            pts.push(pts[0]);
        }
        for w in pts.windows(2) {
            mesh.edge_segments.extend_from_slice(&[
                w[0].x as f32,
                w[0].y as f32,
                0.0,
                w[1].x as f32,
                w[1].y as f32,
                0.0,
            ]);
            mesh.edge_ids.push(ci as u32);
        }
    }
    Some(mesh)
}

/// Kernel segments of a sketch contour.
pub(crate) fn contour_segments(c: &Contour) -> Result<Vec<Segment>, String> {
    c.segs
        .iter()
        .map(|s| s.to_kernel().ok_or_else(|| format!("{}-Segment lässt sich nicht berechnen", s.kind())))
        .collect()
}

/// Winding number of the closed polyline `poly` around `p`.
fn winding(poly: &[DVec2], p: DVec2) -> i32 {
    let n = poly.len();
    let mut w = 0;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let side = (b - a).perp_dot(p - a);
        if a.y <= p.y {
            if b.y > p.y && side > 0.0 {
                w += 1;
            }
        } else if b.y <= p.y && side < 0.0 {
            w -= 1;
        }
    }
    w
}

fn poly_area(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| poly[i].perp_dot(poly[(i + 1) % n])).sum::<f64>() / 2.0
}

/// Region of a closed sketch as a boolean expression of single-contour leaves. Contours fill by
/// the nonzero winding rule, `hole { … }` contours counting clockwise in any direction: a contour
/// bounds the region where the winding number is nonzero on one side only (its inner side
/// nonzero: material, else a hole); contours with material on both sides (nested in the same
/// direction) drop out. `hole` contours are always holes. Holes and islands are nested by containment: material minus the holes inside it, a
/// hole minus the islands inside it.
fn sketch_expr(idx: NodeIdx, contours: &[Contour], m: DMat4) -> Result<Expr2, String> {
    #[derive(Clone, Copy, PartialEq)]
    enum Role {
        Add,
        Sub,
    }
    let mut polys = Vec::with_capacity(contours.len());
    let mut size: f64 = 1e-9;
    let mut segs_of = Vec::with_capacity(contours.len());
    for c in contours {
        let segs = contour_segments(c)?;
        for sg in &segs {
            size = size.max(sg.start().length()).max(sg.end().length());
        }
        segs_of.push(segs);
    }
    for segs in &segs_of {
        polys.push(cad_kernel::curve2::polyline(segs, size * 1e-4));
    }
    let leaf = |i: usize| {
        Expr2::Leaf(Leaf {
            node: idx,
            profile: Profile::Sketch { contours: vec![Contour { hole: false, ..contours[i].clone() }], closed: true },
            m,
        })
    };
    if contours.len() == 1 && !contours[0].hole {
        return Ok(leaf(0));
    }
    // winding at a point: `hole` contours count clockwise whatever their direction
    let wind = |p: DVec2| -> i32 {
        (0..contours.len())
            .map(|j| {
                let w = winding(&polys[j], p);
                if contours[j].hole { -w.abs() } else { w }
            })
            .sum()
    };
    let mut roles: Vec<Option<Role>> = vec![None; contours.len()];
    for i in 0..contours.len() {
        let poly = &polys[i];
        if poly.len() < 2 {
            continue;
        }
        if contours[i].hole {
            roles[i] = Some(Role::Sub);
            continue;
        }
        // test points left and right of the longest edge
        let n = poly.len();
        let k = (0..n).max_by(|&a, &b| {
            let la = poly[a].distance(poly[(a + 1) % n]);
            let lb = poly[b].distance(poly[(b + 1) % n]);
            la.partial_cmp(&lb).unwrap_or(std::cmp::Ordering::Equal)
        });
        let Some(k) = k else { continue };
        let (a, b) = (poly[k], poly[(k + 1) % n]);
        let d = (b - a).normalize_or_zero();
        let nrm = DVec2::new(-d.y, d.x);
        let delta = (a.distance(b) * 1e-3).min(size * 1e-6).max(size * 1e-12);
        let mid = (a + b) / 2.0;
        let wl = wind(mid + nrm * delta);
        let wr = wind(mid - nrm * delta);
        let (w_in, w_out) = if poly_area(poly) >= 0.0 { (wl, wr) } else { (wr, wl) };
        roles[i] = match (w_in != 0, w_out != 0) {
            (true, false) => Some(Role::Add),
            (false, true) => Some(Role::Sub),
            _ => None,
        };
    }
    // nesting: parent = smallest relevant contour containing a point of the contour
    let relevant: Vec<usize> = (0..contours.len()).filter(|&i| roles[i].is_some()).collect();
    let probe = |i: usize| {
        let p = &polys[i];
        if p.len() >= 2 { (p[0] + p[1]) / 2.0 } else { p[0] }
    };
    let mut parent: Vec<Option<usize>> = vec![None; contours.len()];
    for &i in &relevant {
        let q = probe(i);
        let ai = poly_area(&polys[i]).abs();
        parent[i] = relevant
            .iter()
            .copied()
            .filter(|&j| j != i && poly_area(&polys[j]).abs() > ai && winding(&polys[j], q) != 0)
            .min_by(|&a, &b| poly_area(&polys[a]).abs().partial_cmp(&poly_area(&polys[b]).abs()).unwrap());
    }
    fn build(
        i: usize,
        relevant: &[usize],
        parent: &[Option<usize>],
        roles: &[Option<Role>],
        leaf: &dyn Fn(usize) -> Expr2,
    ) -> Expr2 {
        let me = roles[i];
        let kids: Vec<Expr2> = relevant
            .iter()
            .copied()
            .filter(|&c| parent[c] == Some(i) && roles[c] != me)
            .map(|c| build(c, relevant, parent, roles, leaf))
            .collect();
        if kids.is_empty() {
            return leaf(i);
        }
        let mut args = vec![leaf(i)];
        args.extend(kids);
        Expr2::Op(BoolOp::Difference, args)
    }
    // top level: material contours without a material ancestor of the other role chain
    let tops: Vec<Expr2> = relevant
        .iter()
        .copied()
        .filter(|&i| roles[i] == Some(Role::Add))
        .filter(|&i| {
            // an Add nested in a Sub is built by that Sub; one nested in an Add (same role)
            // stands on its own
            let mut p = parent[i];
            while let Some(j) = p {
                if roles[j] != roles[i] {
                    return false;
                }
                p = parent[j];
            }
            true
        })
        .map(|i| build(i, &relevant, &parent, &roles, &leaf))
        .collect();
    if tops.len() == 1 {
        return Ok(tops.into_iter().next().unwrap());
    }
    Ok(Expr2::Op(BoolOp::Union, tops))
}

/// `true` if `m` maps the xy-plane into the xy-plane.
pub(crate) fn is_planar(m: &DMat4) -> bool {
    let s = m.x_axis.truncate().length().max(m.y_axis.truncate().length()).max(1.0);
    m.x_axis.z.abs() < 1e-9 * s && m.y_axis.z.abs() < 1e-9 * s && m.w_axis.z.abs() < 1e-9 * s
}

/// The xy-part of `m` as a 2D affine map (drops z).
pub(crate) fn affine2(m: &DMat4) -> DAffine2 {
    DAffine2::from_cols(m.x_axis.truncate().truncate(), m.y_axis.truncate().truncate(), m.w_axis.truncate().truncate())
}

/// Splits the restriction of `m` to the xy-plane into a rigid frame and a 2D linear map:
/// `m * (x, y, 0, 1) = frame * (a * (x, y), 0, 1)`. Exact for any affine `m`.
pub(crate) fn split_frame(m: &DMat4) -> (DMat4, DAffine2) {
    let x = m.x_axis.truncate();
    let y = m.y_axis.truncate();
    let o = m.w_axis.truncate();
    let lx = x.length();
    if lx < EPS {
        return (DMat4::IDENTITY, DAffine2::from_mat2(DMat2::ZERO));
    }
    let e1 = x / lx;
    let yd = y.dot(e1);
    let yp = y - e1 * yd;
    let ly = yp.length();
    let e2 = if ly < EPS { e1.any_orthonormal_vector() } else { yp / ly };
    let e3 = e1.cross(e2);
    let frame = DMat4::from_cols(e1.extend(0.0), e2.extend(0.0), e3.extend(0.0), o.extend(1.0));
    let a = DMat2::from_cols(DVec2::new(lx, 0.0), DVec2::new(yd, ly));
    (frame, DAffine2::from_mat2(a))
}

/// Maps a loop by a 2D affine map. Circles stay circles under similarities; otherwise they
/// are approximated by a closed spline (second value `true`).
pub(crate) fn map_loop(lp: &Loop, a: &DAffine2) -> (Loop, bool) {
    match lp {
        Loop::Polygon(p) => (Loop::Polygon(p.iter().map(|q| a.transform_point2(*q)).collect()), false),
        Loop::Spline(p) => (Loop::Spline(p.iter().map(|q| a.transform_point2(*q)).collect()), false),
        Loop::Composite(segs) => (Loop::Composite(segs.iter().map(|g| g.map(a)).collect()), false),
        Loop::Circle { center, r } => {
            let c0 = a.matrix2.x_axis;
            let c1 = a.matrix2.y_axis;
            let (l0, l1) = (c0.length(), c1.length());
            if (l0 - l1).abs() <= 1e-9 * l0.max(1.0) && c0.dot(c1).abs() <= 1e-9 * l0 * l1.max(1.0) {
                (Loop::Circle { center: a.transform_point2(*center), r: r * l0 }, false)
            } else {
                let n = 32;
                let pts = (0..n)
                    .map(|i| {
                        let t = i as f64 / n as f64 * std::f64::consts::TAU;
                        a.transform_point2(*center + DVec2::new(t.cos(), t.sin()) * *r)
                    })
                    .collect();
                (Loop::Spline(pts), true)
            }
        }
    }
}

/// Loop normalised for lofting: counter-clockwise, starting at the vertex closest to the +x
/// direction seen from its centroid (circles start at +x in the kernel), so sections are not
/// twisted against each other.
pub(crate) fn loft_normalize(lp: Loop) -> Loop {
    let fix = |mut p: Vec<DVec2>| {
        let n = p.len();
        if n < 3 {
            return p;
        }
        let area: f64 = (0..n).map(|i| p[i].perp_dot(p[(i + 1) % n])).sum();
        if area < 0.0 {
            p.reverse();
        }
        let c = p.iter().fold(DVec2::ZERO, |a, b| a + *b) / n as f64;
        let start = (0..n)
            .min_by(|&a, &b| {
                let fa = (p[a] - c).to_angle().abs();
                let fb = (p[b] - c).to_angle().abs();
                fa.partial_cmp(&fb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(0);
        p.rotate_left(start);
        p
    };
    match lp {
        Loop::Polygon(p) => Loop::Polygon(fix(p)),
        Loop::Spline(p) => Loop::Spline(fix(p)),
        // sketches keep their start (`start(p)`), only the direction is made counter-clockwise
        Loop::Composite(segs) => {
            if cad_kernel::curve2::signed_area(&segs, 1e-3) < 0.0 {
                Loop::Composite(segs.iter().rev().map(|g| g.reversed()).collect())
            } else {
                Loop::Composite(segs)
            }
        }
        c => c,
    }
}

/// Number of segments for a full circle of radius `r` with chordal deviation `tol`.
pub(crate) fn circle_segments(r: f64, tol: f64) -> usize {
    let tol = tol.clamp(1e-4, r.max(1e-4));
    let c = (1.0 - tol / r.max(1e-9)).clamp(-1.0, 1.0);
    let n = (std::f64::consts::PI / c.acos()).ceil();
    (n as usize).clamp(12, 512)
}

/// Closed centripetal Catmull–Rom spline through `pts` (display approximation of the kernel's
/// interpolating spline).
pub(crate) fn spline_polyline(pts: &[DVec2], tol: f64) -> Vec<DVec2> {
    let n = pts.len();
    if n < 3 {
        return pts.to_vec();
    }
    let mut out = Vec::new();
    for i in 0..n {
        let p0 = pts[(i + n - 1) % n];
        let p1 = pts[i];
        let p2 = pts[(i + 1) % n];
        let p3 = pts[(i + 2) % n];
        let len = p1.distance(p2);
        let k = ((len / (8.0 * tol.max(1e-3)).sqrt()).ceil() as usize).clamp(4, 64);
        let tj = |ti: f64, a: DVec2, b: DVec2| ti + a.distance(b).max(1e-9).sqrt();
        let t0 = 0.0;
        let t1 = tj(t0, p0, p1);
        let t2 = tj(t1, p1, p2);
        let t3 = tj(t2, p2, p3);
        for s in 0..k {
            let t = t1 + (t2 - t1) * s as f64 / k as f64;
            let a1 = p0 * ((t1 - t) / (t1 - t0)) + p1 * ((t - t0) / (t1 - t0));
            let a2 = p1 * ((t2 - t) / (t2 - t1)) + p2 * ((t - t1) / (t2 - t1));
            let a3 = p2 * ((t3 - t) / (t3 - t2)) + p3 * ((t - t2) / (t3 - t2));
            let b1 = a1 * ((t2 - t) / (t2 - t0)) + a2 * ((t - t0) / (t2 - t0));
            let b2 = a2 * ((t3 - t) / (t3 - t1)) + a3 * ((t - t1) / (t3 - t1));
            out.push(b1 * ((t2 - t) / (t2 - t1)) + b2 * ((t - t1) / (t2 - t1)));
        }
    }
    out
}

/// Polyline of a loop (implicitly closed) in its own system.
pub(crate) fn loop_polyline(lp: &Loop, tol: f64) -> Vec<DVec2> {
    match lp {
        Loop::Polygon(p) => p.clone(),
        Loop::Circle { center, r } => {
            let n = circle_segments(*r, tol);
            (0..n)
                .map(|i| {
                    let t = i as f64 / n as f64 * std::f64::consts::TAU;
                    *center + DVec2::new(t.cos(), t.sin()) * *r
                })
                .collect()
        }
        Loop::Spline(p) => spline_polyline(p, tol),
        Loop::Composite(segs) => cad_kernel::curve2::polyline(segs, tol),
    }
}

type Pt = [f64; 2];
type Shape2 = Vec<Vec<Pt>>;

/// Contours of a leaf in the target system (outer first), projected into the xy-plane.
pub(crate) fn leaf_contours(l: &Leaf, tol: f64) -> Vec<Vec<DVec2>> {
    let Ok(loops) = profile_loops(&l.profile) else { return Vec::new() };
    // Tolerance in the profile's own system: divide by the largest scale.
    let s = l.m.x_axis.truncate().length().max(l.m.y_axis.truncate().length()).max(1e-9);
    loops
        .iter()
        .map(|lp| {
            loop_polyline(lp, tol / s)
                .into_iter()
                .map(|p| {
                    let v = l.m * DVec4::new(p.x, p.y, 0.0, 1.0);
                    DVec2::new(v.x, v.y)
                })
                .collect()
        })
        .collect()
}

fn to_pts(c: &[DVec2]) -> Vec<Pt> {
    c.iter().map(|p| [p.x, p.y]).collect()
}

/// Evaluates the expression with polygon clipping.
pub(crate) fn expr_region(e: &Expr2, tol: f64) -> Vec<Shape2> {
    match e {
        Expr2::Leaf(l) => {
            let shape: Shape2 = leaf_contours(l, tol).iter().map(|c| to_pts(c)).collect();
            if shape.is_empty() {
                return Vec::new();
            }
            let empty: Vec<Shape2> = Vec::new();
            vec![shape].overlay(&empty, OverlayRule::Subject, FillRule::EvenOdd)
        }
        Expr2::Op(op, args) => {
            let mut it = args.iter();
            let Some(first) = it.next() else { return Vec::new() };
            let mut acc = expr_region(first, tol);
            for a in it {
                let r = expr_region(a, tol);
                acc = match op {
                    BoolOp::Union => acc.overlay(&r, OverlayRule::Union, FillRule::NonZero),
                    BoolOp::Difference => acc.overlay(&r, OverlayRule::Difference, FillRule::NonZero),
                    BoolOp::Intersection => acc.overlay(&r, OverlayRule::Intersect, FillRule::NonZero),
                };
            }
            acc
        }
    }
}

/// Bounds of the leaves (approximate for differences/intersections: union of the leaves).
pub(crate) fn expr_bounds(e: &Expr2, tol: f64) -> Option<(DVec2, DVec2)> {
    let mut b: Option<(DVec2, DVec2)> = None;
    for l in e.leaves() {
        for c in leaf_contours(l, tol) {
            for p in c {
                b = Some(match b {
                    None => (p, p),
                    Some((lo, hi)) => (lo.min(p), hi.max(p)),
                });
            }
        }
    }
    b
}

fn point_in_contours(p: DVec2, contours: &[Vec<DVec2>]) -> bool {
    let mut inside = false;
    for c in contours {
        let n = c.len();
        for i in 0..n {
            let (a, b) = (c[i], c[(i + 1) % n]);
            if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                inside = !inside;
            }
        }
    }
    inside
}

#[cfg(test)]
/// Area of a region (shapes of contours; holes are subtracted).
pub(crate) fn region_area(shapes: &[Shape2]) -> f64 {
    let area = |c: &Vec<Pt>| {
        let n = c.len();
        (0..n)
            .map(|i| {
                let (a, b) = (c[i], c[(i + 1) % n]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum::<f64>()
            / 2.0
    };
    shapes
        .iter()
        .map(|s| {
            let mut it = s.iter();
            let outer = it.next().map(|c| area(c).abs()).unwrap_or(0.0);
            outer - it.map(|c| area(c).abs()).sum::<f64>()
        })
        .sum()
}

/// Filled, outlined mesh of a 2D expression in the plane z = 0. Each triangle carries the tag
/// of the first leaf containing its centroid.
pub(crate) fn region_mesh(e: &Expr2, tol: f64, tag_of: &dyn Fn(NodeIdx) -> Tag) -> Mesh {
    let shapes = expr_region(e, tol);
    let leaves: Vec<(Tag, Vec<Vec<DVec2>>)> =
        e.leaves().iter().map(|l| (tag_of(l.node), leaf_contours(l, tol))).collect();
    let default_tag = leaves.first().map(|l| l.0).unwrap_or(0);
    let mut mesh = Mesh::default();
    let mut edge_id = 0u32;
    for (si, shape) in shapes.iter().enumerate() {
        let tri = shape.triangulate().to_triangulation::<u32>();
        let base = (mesh.positions.len() / 3) as u32;
        for p in &tri.points {
            mesh.positions.extend_from_slice(&[p[0] as f32, p[1] as f32, 0.0]);
            mesh.normals.extend_from_slice(&[0.0, 0.0, 1.0]);
        }
        for t in tri.indices.chunks_exact(3) {
            let c = (0..3).map(|k| DVec2::from_array(tri.points[t[k] as usize])).fold(DVec2::ZERO, |a, b| a + b) / 3.0;
            let tag = leaves.iter().find(|(_, cs)| point_in_contours(c, cs)).map(|l| l.0).unwrap_or(default_tag);
            mesh.indices.extend_from_slice(&[base + t[0], base + t[1], base + t[2]]);
            mesh.tri_tags.push(tag);
            mesh.tri_faces.push(si as u32);
        }
        for c in shape {
            let n = c.len();
            for i in 0..n {
                let (a, b) = (c[i], c[(i + 1) % n]);
                mesh.edge_segments.extend_from_slice(&[a[0] as f32, a[1] as f32, 0.0, b[0] as f32, b[1] as f32, 0.0]);
                mesh.edge_ids.push(edge_id);
            }
            edge_id += 1;
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_frame_is_exact() {
        let m = DMat4::from_translation(DVec3::new(1.0, 2.0, 3.0))
            * DMat4::from_rotation_x(0.7)
            * DMat4::from_scale(DVec3::new(2.0, 3.0, 1.0))
            * DMat4::from_rotation_z(0.3);
        let (f, a) = split_frame(&m);
        for p in [DVec2::new(1.0, 0.0), DVec2::new(-2.0, 5.0), DVec2::new(0.3, 0.1)] {
            let q = a.transform_point2(p);
            let want = m.transform_point3(p.extend(0.0));
            let got = f.transform_point3(q.extend(0.0));
            assert!((want - got).length() < 1e-9, "{want} {got}");
        }
    }

    #[test]
    fn square_minus_circle_area() {
        let sq = Expr2::Leaf(Leaf {
            node: 0,
            profile: Profile::Rect { min: DVec2::ZERO, max: DVec2::splat(20.0) },
            m: DMat4::IDENTITY,
        });
        let c = Expr2::Leaf(Leaf {
            node: 1,
            profile: Profile::Circle { r: 5.0 },
            m: DMat4::from_translation(DVec3::new(10.0, 10.0, 0.0)),
        });
        let e = Expr2::Op(BoolOp::Difference, vec![sq, c]);
        let r = expr_region(&e, 0.01);
        let a = region_area(&r);
        let want = 400.0 - std::f64::consts::PI * 25.0;
        assert!((a - want).abs() < 0.3, "{a} vs {want}");
        let m = region_mesh(&e, 0.01, &|n| n + 1);
        assert!(m.triangle_count() > 10);
        assert!(m.tri_tags.iter().all(|&t| t == 1));
    }
}
