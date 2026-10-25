//! 2D operations on exact regions (Plan 17.3): `offset`, `fillet2d`/`chamfer2d`, `stroke`,
//! `hull` and the exact cases of `minkowski`.
//!
//! The children are lifted into the operation's system (`profile::lift`) and turned into an
//! exact region of lines and arcs (`cad_kernel::region2::combine` over the leaf loops with the
//! boolean expression as predicate). The result is a set of contours (outer loops
//! counter-clockwise, holes clockwise) that `profile::lift` hands on as a sketch profile, so
//! extrusions, revolves and STEP see exact lines and arcs. Results are memoised by the content
//! of the subtree (the display, the bounds and every profile operation above lift the node).

use std::cell::RefCell;
use std::rc::Rc;

use cad_ir::{BlendKind, Contour, NodeIdx, Op, Profile, Seg, Select, StrokeCap, StrokeJoin, Tree, ZoneShape};
use cad_kernel::hull2;
use cad_kernel::region2::{self, CornerError, CornerKind, Fill, Loop2, Operand};
use cad_kernel::{BoolOp, Loop, Segment};
use glam::{DMat4, DVec2, DVec3};

use crate::key::{CacheKey, KeyHasher};
use crate::profile::{self, Expr2, Leaf};

/// Result of a 2D operation in its node's own system.
#[derive(Debug, Default)]
pub(crate) struct Op2Out {
    pub contours: Vec<Contour>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub hints: Vec<String>,
    /// New corner pieces (`fillet2d`) as polylines (shown like selected edges).
    pub corners: Vec<Vec<DVec3>>,
    /// Guide lines: (`"source"` input contour, `"result"` offset contour, `"centerline"`
    /// stroke path) as polylines.
    pub guides: Vec<(&'static str, Vec<DVec3>)>,
}

type Memo = Vec<(CacheKey, Rc<Result<Op2Out, String>>)>;

thread_local! {
    static MEMO: RefCell<Memo> = const { RefCell::new(Vec::new()) };
}

const MEMO_SIZE: usize = 48;

fn subtree_key(tree: &Tree, idx: NodeIdx, depth: u32) -> CacheKey {
    let n = tree.node(idx);
    let mut h = KeyHasher::new();
    h.str("op2d/1");
    match &n.op {
        Op::Color { .. } => h.str("color"),
        op => h.value(op),
    };
    if depth < 200 {
        for &c in &n.children {
            let cn = tree.node(c);
            let f = cn.flags;
            if f.disabled || f.background {
                h.tag(b'*');
                continue;
            }
            h.tag(if f.zone { b'z' } else { b'c' });
            h.key(&subtree_key(tree, c, depth + 1)).mat(&cn.local);
        }
    }
    h.finish()
}

/// The 2D operation at `idx` (memoised).
pub(crate) fn compute(tree: &Tree, idx: NodeIdx) -> Rc<Result<Op2Out, String>> {
    let key = subtree_key(tree, idx, 0);
    if let Some(r) = MEMO.with(|m| m.borrow().iter().find(|(k, _)| *k == key).map(|(_, r)| r.clone())) {
        return r;
    }
    let r = Rc::new(run(tree, idx));
    MEMO.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() >= MEMO_SIZE {
            m.remove(0);
        }
        m.push((key, r.clone()));
    });
    r
}

pub(crate) fn is_op2d(op: &Op) -> bool {
    matches!(op, Op::Offset { .. } | Op::Blend2 { .. } | Op::Stroke { .. } | Op::Hull | Op::Minkowski)
}

fn run(tree: &Tree, idx: NodeIdx) -> Result<Op2Out, String> {
    let n = tree.node(idx);
    let mut out = Op2Out::default();
    match &n.op {
        Op::Offset { round, d, chamfer } => {
            let region = children_region(tree, idx, &mut out.warnings)?;
            let size = region_size(&region);
            let res =
                if *round { region2::offset_round(&region, *d) } else { region2::offset_delta(&region, *d, *chamfer) };
            if res.is_empty() && !region.is_empty() {
                out.warnings.push(format!("offset({}) lässt vom Profil nichts übrig", fmt_num(*d)));
            }
            let tol = display_tol(size);
            for lp in &region {
                out.guides.push(("source", polyline3(lp, tol, true)));
            }
            for lp in &res {
                out.guides.push(("result", polyline3(lp, tol, true)));
            }
            out.contours = to_contours(&res);
        }
        Op::Blend2 { kind, r, select } => {
            let loops = blend_input(tree, idx, &mut out.warnings)?;
            let zones = zones_of(tree, idx, &mut out.warnings);
            let size = region_size(&loops);
            let ck = match kind {
                BlendKind::Fillet => CornerKind::Round(*r),
                BlendKind::Chamfer => CornerKind::Chamfer(*r),
            };
            let mut count = 0usize;
            let mut chosen = 0usize;
            let mut res = Vec::new();
            let tol = display_tol(size);
            for lp in &loops {
                let (l2, c, k) = fillet_loop(lp, ck, &|v, pts| zones.selects(v, pts, *select), &mut out, tol);
                count += c;
                chosen += k;
                res.push(l2);
            }
            let noun = if count == 1 { "Ecke" } else { "Ecken" };
            out.hints.push(format!("{count} {noun}"));
            if chosen == 0 && *r > 0.0 {
                out.warnings.push(if zones.is_empty() {
                    "keine Ecke im Profil".to_string()
                } else {
                    "leere Auswahl: keine Ecke in den Zonen".to_string()
                });
            }
            out.contours = to_contours(&res);
        }
        Op::Stroke { w, cap, join } => {
            let paths = stroke_paths(tree, idx, &mut out.warnings)?;
            if paths.is_empty() {
                return Err("stroke braucht einen Pfad: `sketch(closed = false) { … }` oder ein 2D-Profil".into());
            }
            let size = region_size(&paths.iter().map(|p| p.0.clone()).collect::<Vec<_>>());
            let tol = display_tol(size);
            for (segs, closed) in &paths {
                out.guides.push(("centerline", polyline3(segs, tol, *closed)));
            }
            let cap = match cap {
                StrokeCap::Round => region2::Cap::Round,
                StrokeCap::Flat => region2::Cap::Flat,
                StrokeCap::Square => region2::Cap::Square,
            };
            let join = match join {
                StrokeJoin::Round => region2::Join::Round,
                StrokeJoin::Miter => region2::Join::Miter,
                StrokeJoin::Bevel => region2::Join::Bevel,
            };
            if *w <= 0.0 {
                return Err("stroke: Breite muss > 0 sein".into());
            }
            let res = region2::stroke(&paths, *w, cap, join);
            out.contours = to_contours(&res);
        }
        Op::Hull => {
            let e = children_expr(tree, idx, &mut out.warnings)?;
            let loops = hull_loops(&e)?;
            let size = region_size(&loops);
            let h = hull2::hull(&loops);
            if h.is_empty() {
                out.warnings.push("hull: die Kinder haben keine Fläche (Punkte auf einer Linie)".into());
            }
            let tol = display_tol(size);
            for lp in &loops {
                out.guides.push(("source", polyline3(lp, tol, true)));
            }
            out.contours = to_contours(&[h]);
        }
        Op::Minkowski => {
            let res = minkowski(tree, idx, &mut out)?;
            out.contours = to_contours(&res);
        }
        other => return Err(format!("{} ist keine 2D-Operation", other.label())),
    }
    Ok(out)
}

/// Loops whose hull is the hull of the expression: the leaves' loops below unions (holes do
/// not matter), the exact region of differences and intersections.
fn hull_loops(e: &Expr2) -> Result<Vec<Loop2>, String> {
    match e {
        Expr2::Leaf(l) => Ok(leaf_operand(l)?.loops),
        Expr2::Op(BoolOp::Union, args) => {
            let mut out = Vec::new();
            for a in args {
                out.extend(hull_loops(a)?);
            }
            Ok(out)
        }
        _ => expr_loops(e),
    }
}

/// A circle (centre, radius) if the expression is a single `circle()` placed by a similarity
/// (rotation, mirror, uniform scale and translation).
fn as_circle(e: &Expr2) -> Option<(DVec2, f64)> {
    let mut e = e;
    while let Expr2::Op(BoolOp::Union, a) = e {
        if a.len() != 1 {
            return None;
        }
        e = &a[0];
    }
    let Expr2::Leaf(l) = e else { return None };
    let Profile::Circle { r } = l.profile else { return None };
    let a = profile::affine2(&l.m);
    let (x, y) = (a.matrix2.x_axis, a.matrix2.y_axis);
    let s = x.length();
    let similar = (y.length() - s).abs() <= 1e-9 * s && x.dot(y).abs() <= 1e-9 * s * s;
    (similar && s > 0.0 && r > 0.0).then_some((a.translation, r * s))
}

/// `minkowski()` of 2D children: any profile plus circles (rounded offset by the sum of the
/// radii, moved by the sum of the centres), circles only (a circle), or two convex profiles of
/// which one is a polygon (hull of the other moved to the polygon's vertices), plus circles.
/// Anything else: a warning, the children unchanged.
fn minkowski(tree: &Tree, idx: NodeIdx, out: &mut Op2Out) -> Result<Vec<Loop2>, String> {
    let e = children_expr(tree, idx, &mut out.warnings)?;
    let Expr2::Op(_, args) = &e else { unreachable!("children_expr is a union") };
    let mut shift = DVec2::ZERO;
    let mut r = 0.0;
    let mut rest: Vec<&Expr2> = Vec::new();
    for a in args {
        match as_circle(a) {
            Some((c, rr)) => {
                shift += c;
                r += rr;
            }
            None => rest.push(a),
        }
    }
    let size = region_size(&expr_loops(&e)?);
    let tol = display_tol(size);
    let moved = |loops: Vec<Loop2>| -> Vec<Loop2> {
        let m = glam::DAffine2::from_translation(shift);
        loops.into_iter().map(|lp| lp.iter().map(|s| s.map(&m)).collect()).collect()
    };
    let base: Vec<Loop2> = match rest.len() {
        0 => {
            // circles only: a circle of the summed radius
            let c = Loop::Circle { center: DVec2::ZERO, r };
            return Ok(moved(vec![loop_segments(&c)]));
        }
        1 => expr_loops(rest[0])?,
        2 => {
            let a = expr_loops(rest[0])?;
            let b = expr_loops(rest[1])?;
            let poly_of = |l: &[Loop2]| -> Option<Vec<DVec2>> {
                (l.len() == 1 && l[0].iter().all(|s| matches!(s, Segment::Line { .. })))
                    .then(|| l[0].iter().map(|s| s.start()).collect())
            };
            if !hull2::is_convex(&a) || !hull2::is_convex(&b) {
                return unsupported(out, &e, "zwei Profile nur exakt, wenn beide konvex sind");
            }
            match (poly_of(&a), poly_of(&b)) {
                (Some(v), _) => vec![hull2::minkowski_convex(&v, &b)],
                (_, Some(v)) => vec![hull2::minkowski_convex(&v, &a)],
                _ => return unsupported(out, &e, "zwei konvexe Profile nur exakt, wenn eines ein Polygon ist"),
            }
        }
        n => return unsupported(out, &e, &format!("{n} Profile (außer Kreisen); exakt sind höchstens zwei")),
    };
    for lp in &base {
        out.guides.push(("source", polyline3(lp, tol, true)));
    }
    let res = if r > 0.0 { region2::offset_round(&base, r) } else { base };
    Ok(moved(res))
}

/// Unsupported case of `minkowski`: warning with the reason, the children unchanged.
fn unsupported(out: &mut Op2Out, e: &Expr2, why: &str) -> Result<Vec<Loop2>, String> {
    out.warnings.push(format!("minkowski: {why}; Kinder werden unverändert gezeigt"));
    expr_loops(e)
}

fn fmt_num(x: f64) -> String {
    let s = format!("{:.4}", x);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn display_tol(size: f64) -> f64 {
    (size * 1e-3).clamp(1e-4, 0.05)
}

fn region_size(loops: &[Loop2]) -> f64 {
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for s in loops.iter().flatten() {
        for p in [s.start(), s.end()] {
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    if lo.x.is_finite() { (hi - lo).length().max(1e-6) } else { 1.0 }
}

fn polyline3(segs: &[Segment], tol: f64, closed: bool) -> Vec<DVec3> {
    let mut pts = cad_kernel::curve2::polyline(segs, tol);
    if closed && !pts.is_empty() {
        pts.push(pts[0]);
    }
    pts.into_iter().map(|p| p.extend(0.0)).collect()
}

pub(crate) fn to_contours(loops: &[Loop2]) -> Vec<Contour> {
    loops
        .iter()
        .filter(|l| !l.is_empty())
        .map(|l| Contour { hole: false, closed: true, segs: l.iter().map(Seg::from_kernel).collect() })
        .collect()
}

// ---------------------------------------------------------------- exact regions

/// Loops of a leaf in the target system and its fill rule.
fn leaf_operand(l: &Leaf) -> Result<Operand, String> {
    let a = profile::affine2(&l.m);
    let loops = profile::profile_loops(&l.profile)?;
    let fill = if matches!(l.profile, Profile::Polygon { .. }) { Fill::EvenOdd } else { Fill::NonZero };
    let mut out = Vec::with_capacity(loops.len());
    for lp in &loops {
        out.push(loop_segments(lp).iter().map(|s| s.map(&a)).collect());
    }
    Ok(Operand::new(out, fill))
}

/// Segments of a kernel loop (circles as two half arcs, splines as fine polylines).
pub(crate) fn loop_segments(lp: &Loop) -> Vec<Segment> {
    let lines = |p: &[DVec2]| -> Vec<Segment> {
        let n = p.len();
        (0..n)
            .filter(|&i| p[i].distance(p[(i + 1) % n]) > 0.0)
            .map(|i| Segment::Line { a: p[i], b: p[(i + 1) % n] })
            .collect()
    };
    match lp {
        Loop::Polygon(p) => lines(p),
        Loop::Circle { center, r } => vec![
            Segment::Arc { center: *center, r: *r, a0: 0.0, a1: std::f64::consts::PI },
            Segment::Arc { center: *center, r: *r, a0: std::f64::consts::PI, a1: std::f64::consts::TAU },
        ],
        Loop::Spline(p) => {
            let size = p.iter().fold(0.0f64, |m, q| m.max(q.length())).max(1.0);
            lines(&profile::spline_polyline(p, size * 1e-5))
        }
        Loop::Composite(segs) => segs.clone(),
    }
}

enum Prog {
    Leaf(usize),
    Op(BoolOp, Vec<Prog>),
}

fn compile(e: &Expr2, ops: &mut Vec<Operand>) -> Result<Prog, String> {
    Ok(match e {
        Expr2::Leaf(l) => {
            ops.push(leaf_operand(l)?);
            Prog::Leaf(ops.len() - 1)
        }
        Expr2::Op(op, args) => Prog::Op(*op, args.iter().map(|a| compile(a, ops)).collect::<Result<_, _>>()?),
    })
}

fn eval_prog(p: &Prog, ins: &[bool]) -> bool {
    match p {
        Prog::Leaf(i) => ins[*i],
        Prog::Op(BoolOp::Union, a) => a.iter().any(|x| eval_prog(x, ins)),
        Prog::Op(BoolOp::Intersection, a) => !a.is_empty() && a.iter().all(|x| eval_prog(x, ins)),
        Prog::Op(BoolOp::Difference, a) => {
            let mut it = a.iter();
            it.next().is_some_and(|f| eval_prog(f, ins)) && !it.any(|x| eval_prog(x, ins))
        }
    }
}

/// Exact region of a 2D expression.
pub(crate) fn expr_loops(e: &Expr2) -> Result<Vec<Loop2>, String> {
    let mut ops = Vec::new();
    let prog = compile(e, &mut ops)?;
    Ok(region2::combine(&ops, &|ins| eval_prog(&prog, ins)))
}

/// Union of the solid 2D children of `idx` (in its system) as an expression.
fn children_expr(tree: &Tree, idx: NodeIdx, warn: &mut Vec<String>) -> Result<Expr2, String> {
    let what = tree.node(idx).op.label();
    let mut args = Vec::new();
    for c in tree.solid_children(idx) {
        if tree.dim(c) == Some(cad_ir::Dim::Three) {
            return Err(format!("{what} erwartet 2D-Profile als Kinder"));
        }
        if profile::open_sketch_in(tree, c) {
            return Err(format!("{what}: offene Skizze ist kein Profil (für Pfade `stroke(w)` verwenden)"));
        }
        args.push(profile::lift(tree, c, tree.node(c).local, warn)?);
    }
    if args.is_empty() {
        return Err(format!("{what} ohne Profil"));
    }
    Ok(Expr2::Op(BoolOp::Union, args))
}

fn children_region(tree: &Tree, idx: NodeIdx, warn: &mut Vec<String>) -> Result<Vec<Loop2>, String> {
    let e = children_expr(tree, idx, warn)?;
    expr_loops(&e)
}

/// Input loops of `fillet2d`: a single closed contour keeps its exact segments (B-splines
/// stay B-splines); anything else goes through the exact region (B-splines as biarcs).
fn blend_input(tree: &Tree, idx: NodeIdx, warn: &mut Vec<String>) -> Result<Vec<Loop2>, String> {
    let e = children_expr(tree, idx, warn)?;
    let mut single = &e;
    while let Expr2::Op(BoolOp::Union, a) = single {
        if a.len() != 1 {
            break;
        }
        single = &a[0];
    }
    if let Expr2::Leaf(l) = single
        && let Ok(op) = leaf_operand(l)
        && op.loops.len() == 1
        && op.fill == Fill::NonZero
    {
        let mut lp = op.loops.into_iter().next().unwrap_or_default();
        if region2::loop_area(&lp) < 0.0 {
            lp = lp.iter().rev().map(|s| s.reversed()).collect();
        }
        return Ok(vec![lp]);
    }
    expr_loops(&e)
}

// ---------------------------------------------------------------- zones

struct Zones {
    /// Polygon regions (shapes of contours) in the operation's system.
    regions: Vec<Vec<Vec<DVec2>>>,
    /// Grid line / plane zones: shape and the zone node's local matrix (inverse).
    shapes: Vec<(ZoneShape, DMat4)>,
}

impl Zones {
    fn is_empty(&self) -> bool {
        self.regions.is_empty() && self.shapes.is_empty()
    }

    fn contains(&self, p: DVec2, tol: f64) -> bool {
        for shape in &self.regions {
            if point_in_shape(p, shape) || dist_to_shape(p, shape) <= tol {
                return true;
            }
        }
        for (z, inv) in &self.shapes {
            let q = inv.transform_point3(p.extend(0.0));
            let inside = match *z {
                ZoneShape::Line { point, dir, d } => {
                    let dir = dir.normalize_or_zero();
                    let v = q - point;
                    (v - dir * v.dot(dir)).length() <= d / 2.0 + tol
                }
                ZoneShape::Plane { point, normal, d } => {
                    (q - point).dot(normal.normalize_or_zero()).abs() <= d / 2.0 + tol
                }
            };
            if inside {
                return true;
            }
        }
        false
    }

    /// Is the corner at `v` selected? `pts`: points of the profile the rounding replaces.
    fn selects(&self, v: DVec2, pts: &[DVec2], select: Select) -> bool {
        if self.is_empty() {
            return true;
        }
        let tol = 1e-6 * (1.0 + v.length());
        match select {
            Select::Inside => self.contains(v, tol),
            Select::Touch => self.contains(v, tol) || pts.iter().any(|p| self.contains(*p, tol)),
        }
    }
}

fn point_in_shape(p: DVec2, contours: &[Vec<DVec2>]) -> bool {
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

fn dist_to_shape(p: DVec2, contours: &[Vec<DVec2>]) -> f64 {
    let mut d = f64::INFINITY;
    for c in contours {
        let n = c.len();
        for i in 0..n {
            let (a, b) = (c[i], c[(i + 1) % n]);
            let e = b - a;
            let t = ((p - a).dot(e) / e.length_squared().max(1e-300)).clamp(0.0, 1.0);
            d = d.min((a + e * t).distance(p));
        }
    }
    d
}

fn zones_of(tree: &Tree, idx: NodeIdx, warn: &mut Vec<String>) -> Zones {
    let mut z = Zones { regions: Vec::new(), shapes: Vec::new() };
    for &c in &tree.node(idx).children {
        let cn = tree.node(c);
        if !cn.flags.zone || cn.flags.disabled {
            continue;
        }
        collect_zone(tree, c, cn.local, warn, &mut z);
    }
    z
}

fn collect_zone(tree: &Tree, c: NodeIdx, m: DMat4, warn: &mut Vec<String>, z: &mut Zones) {
    let cn = tree.node(c);
    match &cn.op {
        Op::Zone(shape) => z.shapes.push((shape.clone(), m.inverse())),
        Op::Group | Op::ModuleCall { .. } | Op::Transform { .. } | Op::Color { .. }
            if tree.dim(c) != Some(cad_ir::Dim::Two) =>
        {
            for &k in &cn.children {
                let kn = tree.node(k);
                if kn.flags.disabled || kn.flags.background {
                    continue;
                }
                collect_zone(tree, k, m * kn.local, warn, z);
            }
        }
        _ => {
            if tree.dim(c) != Some(cad_ir::Dim::Two) {
                warn.push("fillet2d: Zonen müssen 2D-Formen oder zone(…) sein; 3D-Zone ignoriert".into());
                return;
            }
            let mut w = Vec::new();
            match profile::lift(tree, c, m, &mut w) {
                Ok(e) => {
                    for shape in profile::expr_region(&e, 1e-3) {
                        z.regions
                            .push(shape.iter().map(|c| c.iter().map(|p| DVec2::new(p[0], p[1])).collect()).collect());
                    }
                }
                Err(msg) => warn.push(format!("Zone: {msg}")),
            }
        }
    }
}

// ---------------------------------------------------------------- fillet of a loop

fn sample_piece(s: &Segment, t0: f64, t1: f64) -> Vec<DVec2> {
    (0..=16).map(|i| region2::seg_point(s, t0 + (t1 - t0) * i as f64 / 16.0)).collect()
}

/// Rounds (chamfers) the selected corners of a closed loop; returns the new loop, the number of
/// corners done and the number selected. Errors name the corner.
fn fillet_loop(
    lp: &[Segment],
    kind: CornerKind,
    selected: &dyn Fn(DVec2, &[DVec2]) -> bool,
    out: &mut Op2Out,
    tol: f64,
) -> (Loop2, usize, usize) {
    let n = lp.len();
    if n < 2 {
        return (lp.to_vec(), 0, 0);
    }
    let mut cands: Vec<(usize, CornerKind)> = Vec::new();
    for v in 0..n {
        let (a, b) = (&lp[(v + n - 1) % n], &lp[v]);
        let phi = region2::turn(a.end_tangent(), b.start_tangent());
        if phi.abs() <= 1e-7 {
            continue;
        }
        // points of the profile the rounding replaces (selection with `touch`)
        let removed = match region2::round_corner(a, b, kind) {
            Ok(r) => {
                let mut p = sample_piece(a, r.ua, 1.0);
                p.extend(sample_piece(b, 0.0, r.ub));
                p
            }
            Err(_) => Vec::new(),
        };
        if selected(b.start(), &removed) {
            cands.push((v, kind));
        }
    }
    let (pieces, results) = region2::apply_corners(lp, true, &cands);
    let what = match kind {
        CornerKind::Round(r) => format!("Radius {}", fmt_num(r)),
        CornerKind::Chamfer(d) => format!("Fase {}", fmt_num(d)),
    };
    let mut count = 0;
    for ((v, _), r) in cands.iter().zip(&results) {
        let vertex = lp[*v].start();
        let at = format!("[{}, {}]", fmt_num(vertex.x), fmt_num(vertex.y));
        match r {
            Ok(r) => {
                out.corners.push(r.mid.sample(tol).into_iter().map(|p| p.extend(0.0)).collect());
                count += 1;
            }
            Err(e) => out.errors.push(match e {
                CornerError::TooLarge { max: Some(m) } => {
                    format!("Ecke bei {at}: {what} passt nicht (höchstens ≈ {})", fmt_num((m * 1e4).floor() / 1e4))
                }
                CornerError::Cusp => format!("Ecke bei {at}: Spitze lässt sich nicht verrunden"),
                _ => format!("Ecke bei {at}: {what} passt nicht"),
            }),
        }
    }
    (pieces.into_iter().map(|p| p.1).collect(), count, cands.len())
}

// ---------------------------------------------------------------- stroke paths

/// Paths of `stroke`: open sketches (their contours as they are) and the boundaries of other
/// 2D children (closed paths).
fn stroke_paths(tree: &Tree, idx: NodeIdx, warn: &mut Vec<String>) -> Result<Vec<(Vec<Segment>, bool)>, String> {
    let mut out = Vec::new();
    for c in tree.solid_children(idx) {
        if tree.dim(c) == Some(cad_ir::Dim::Three) {
            return Err("stroke erwartet Pfade (offene Skizzen) oder 2D-Profile als Kinder".into());
        }
        collect_paths(tree, c, tree.node(c).local, warn, &mut out)?;
    }
    Ok(out)
}

fn collect_paths(
    tree: &Tree,
    c: NodeIdx,
    m: DMat4,
    warn: &mut Vec<String>,
    out: &mut Vec<(Vec<Segment>, bool)>,
) -> Result<(), String> {
    let n = tree.node(c);
    match &n.op {
        Op::Profile(Profile::Sketch { contours, closed: false }) => {
            let a = profile::affine2(&m);
            for ct in contours {
                let segs: Vec<Segment> = profile::contour_segments(ct)?.iter().map(|s| s.map(&a)).collect();
                if !segs.is_empty() {
                    let closed = ct.closed
                        || segs[0].start().distance(segs[segs.len() - 1].end())
                            <= 1e-9 * (1.0 + segs[0].start().length());
                    out.push((segs, closed));
                }
            }
            Ok(())
        }
        Op::Group | Op::ModuleCall { .. } | Op::Transform { .. } | Op::Color { .. }
            if profile::open_sketch_in(tree, c) =>
        {
            for k in tree.solid_children(c) {
                collect_paths(tree, k, m * tree.node(k).local, warn, out)?;
            }
            Ok(())
        }
        _ => {
            let e = profile::lift(tree, c, m, warn)?;
            for lp in expr_loops(&e)? {
                out.push((lp, true));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exact booleans against the polygon clipping of the display on random shapes.
    #[test]
    fn exact_booleans_match_polygon_clipping() {
        let mut seed = 12345u64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        for case in 0..60 {
            let mut leaves = Vec::new();
            for k in 0..(2 + case % 4) {
                let m = DMat4::from_translation(DVec3::new(rnd() * 20.0, rnd() * 20.0, 0.0))
                    * DMat4::from_rotation_z(rnd() * 6.0);
                let profile = match (case + k) % 4 {
                    0 => Profile::Circle { r: 2.0 + rnd() * 8.0 },
                    1 => Profile::Rect { min: DVec2::ZERO, max: DVec2::new(3.0 + rnd() * 10.0, 3.0 + rnd() * 10.0) },
                    2 => Profile::Polygon {
                        loops: vec![
                            (0..5)
                                .map(|i| {
                                    let a = i as f64 / 5.0 * std::f64::consts::TAU;
                                    DVec2::new(a.cos(), a.sin()) * (3.0 + rnd() * 6.0)
                                })
                                .collect(),
                        ],
                    },
                    _ => Profile::Sketch {
                        contours: vec![Contour {
                            hole: false,
                            closed: true,
                            segs: vec![
                                Seg::Line { a: DVec2::ZERO, b: DVec2::new(8.0, 0.0) },
                                Seg::Arc {
                                    center: DVec2::new(8.0, 4.0),
                                    r: 4.0,
                                    a0: -std::f64::consts::FRAC_PI_2,
                                    a1: std::f64::consts::FRAC_PI_2,
                                    ccw: true,
                                },
                                Seg::Line { a: DVec2::new(8.0, 8.0), b: DVec2::ZERO },
                            ],
                        }],
                        closed: true,
                    },
                };
                leaves.push(Expr2::Leaf(Leaf { node: k as u32, profile, m }));
            }
            let op = [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection][case % 3];
            let e = Expr2::Op(op, leaves);
            let exact = region2::region_area(&expr_loops(&e).unwrap());
            let poly = profile::region_area(&profile::expr_region(&e, 1e-4));
            assert!((exact - poly).abs() <= 1e-3 * poly.abs().max(1.0), "case {case}: {exact} vs {poly}");
        }
    }

    #[test]
    fn memo_key_depends_on_parameters() {
        let mut t = Tree::default();
        let mk = |op: Op| cad_ir::Node {
            stmt: 0,
            span: Default::default(),
            op,
            children: vec![],
            parent: None,
            local: DMat4::IDENTITY,
            world: DMat4::IDENTITY,
            flags: Default::default(),
            module_inst: 0,
            blend: None,
            tol: None,
            color: None,
            inst_key: 0,
        };
        t.nodes.push(mk(Op::Offset { round: true, d: 1.0, chamfer: false }));
        t.nodes.push(mk(Op::Profile(Profile::Rect { min: DVec2::ZERO, max: DVec2::splat(10.0) })));
        t.nodes[0].children.push(1);
        t.nodes[1].parent = Some(0);
        t.root = Some(0);
        let k1 = subtree_key(&t, 0, 0);
        let r = compute(&t, 0);
        let o = r.as_ref().as_ref().unwrap();
        let loops: Vec<Loop2> = o.contours.iter().map(|c| profile::contour_segments(c).unwrap()).collect();
        let a = region2::region_area(&loops);
        assert!((a - (144.0 - (4.0 - std::f64::consts::PI))).abs() < 1e-9, "{a}");
        t.nodes[0].op = Op::Offset { round: true, d: 2.0, chamfer: false };
        assert_ne!(k1, subtree_key(&t, 0, 0));
    }
}
