//! `sketch { … }` (Plan 17.1): a 2D block of segments. Every segment statement (`start`,
//! `line`, `arc`, `bezier`, `spline`, `close`) appends to the sketch being evaluated, also
//! through module calls and loops inside the block. The result is one `Profile::Sketch` node;
//! the statements of its segments go to `EvalOutput::sketches` (display, handles, errors).

use cad_ir::{Contour, NodeIdx, Op, Profile, Seg};
use cad_syntax::ast::{Ident, Stmt, StmtId};
use glam::{DMat4, DVec2};

use crate::env::Dyn;
use crate::eval::{ArgV, Args, Ctx, Ev, R};
use crate::handles::{HSpec, comp_scalar, comps_of};
use crate::output::{HandleKind, SketchContourInfo, SketchInfo, SketchSegInfo, SketchStep, SketchStepKind};
use crate::value::fmt_num;
use cad_kernel::region2::{CornerError, CornerKind, Piece, apply_corners};

/// Statements that only exist inside a sketch.
pub(crate) const SEGMENTS: &[&str] = &["start", "line", "arc", "bezier", "spline", "close", "corner", "hole"];

/// Built-in statements allowed in a sketch besides the segments.
const ALLOWED: &[&str] = &["echo", "assert", "children", "group"];

pub(crate) fn is_segment(name: &str) -> bool {
    SEGMENTS.contains(&name)
}

pub(crate) fn allowed_in_sketch(name: &str) -> bool {
    ALLOWED.contains(&name)
}

struct SegB<'a> {
    seg: Seg,
    stmt: &'a Stmt,
    implicit: bool,
    error: bool,
}

struct ContourB<'a> {
    hole: bool,
    closed: bool,
    start: DVec2,
    start_stmt: Option<&'a Stmt>,
    segs: Vec<SegB<'a>>,
    corners: Vec<CornerB<'a>>,
}

/// `corner(r = …)` / `corner(chamfer = …)`: rounds the vertex after segment `at - 1` (the
/// start vertex for `at = 0`, and for `at` = segment count of a contour that ends at its start).
struct CornerB<'a> {
    stmt: &'a Stmt,
    at: usize,
    kind: CornerKind,
    /// Index of its handle in `Ev::handles` (position fixed once the corner is computed).
    handle: Option<usize>,
}

impl ContourB<'_> {
    fn end(&self) -> DVec2 {
        self.segs.last().map_or(self.start, |s| s.seg.end())
    }

    fn end_tangent(&self) -> Option<DVec2> {
        let s = self.segs.last()?;
        cad_ir::Seg::to_kernel(&s.seg).map(|k| k.end_tangent()).filter(|t| t.length() > 0.5)
    }
}

pub(crate) struct SketchBuild<'a> {
    node: NodeIdx,
    closed: bool,
    contours: Vec<ContourB<'a>>,
    /// Depth of `hole { … }` blocks.
    hole: u32,
    /// Evaluated segment statements (sketch editor).
    steps: Vec<SketchStep>,
}

fn fmt2(p: DVec2) -> String {
    format!("[{}, {}]", fmt_num(p.x), fmt_num(p.y))
}

fn angle_of(v: DVec2) -> f64 {
    v.y.atan2(v.x)
}

/// Signed sweep from `a0` to `a1` in the given direction, in (0, 2π].
fn sweep(a0: f64, a1: f64, ccw: bool) -> f64 {
    let tau = std::f64::consts::TAU;
    let mut d = (a1 - a0).rem_euclid(tau);
    if d < 1e-12 {
        d = tau;
    }
    if ccw { d } else { d - tau }
}

fn arc_seg(center: DVec2, from: DVec2, to: DVec2, ccw: bool) -> Seg {
    let a0 = angle_of(from - center);
    let mut sw = sweep(a0, angle_of(to - center), ccw);
    // a closed full circle is not a valid segment of a loop; an arc ending at its start is
    if (from - to).length() > 1e-9 * (1.0 + from.length()) && (sw.abs() - std::f64::consts::TAU).abs() < 1e-12 {
        sw = 0.0;
    }
    Seg::Arc { center, r: (from - center).length(), a0, a1: a0 + sw, ccw }
}

/// Do segments `p0-p1` and `q0-q1` intersect (touching counts)? Collinear overlaps count.
fn seg_hit(p0: DVec2, p1: DVec2, q0: DVec2, q1: DVec2, eps: f64) -> Option<DVec2> {
    let r = p1 - p0;
    let s = q1 - q0;
    let den = r.perp_dot(s);
    let qp = q0 - p0;
    if den.abs() <= eps * (r.length() + s.length()).max(1e-300) {
        // parallel: collinear overlap?
        if qp.perp_dot(r).abs() > eps * r.length().max(1e-300) {
            return None;
        }
        let rr = r.length_squared().max(1e-300);
        let t0 = qp.dot(r) / rr;
        let t1 = (q1 - p0).dot(r) / rr;
        let (lo, hi) = (t0.min(t1).max(0.0), t0.max(t1).min(1.0));
        return (hi - lo > eps / r.length().max(1e-300)).then(|| p0 + r * ((lo + hi) / 2.0));
    }
    let t = qp.perp_dot(s) / den;
    let u = qp.perp_dot(r) / den;
    let m = 1e-12;
    ((-m..=1.0 + m).contains(&t) && (-m..=1.0 + m).contains(&u)).then(|| p0 + r * t)
}

/// All points on one straight line (no area at all).
fn flat(polys: &[Vec<DVec2>], size: f64) -> bool {
    let pts: Vec<DVec2> = polys.iter().flatten().copied().collect();
    let n = pts.len().max(1) as f64;
    let m = pts.iter().fold(DVec2::ZERO, |a, p| a + *p) / n;
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for p in &pts {
        let d = *p - m;
        sxx += d.x * d.x;
        syy += d.y * d.y;
        sxy += d.x * d.y;
    }
    // smallest eigenvalue of the covariance matrix
    let tr = (sxx + syy) / n;
    let det = (sxx * syy - sxy * sxy) / (n * n);
    let lmin = tr / 2.0 - ((tr * tr / 4.0 - det).max(0.0)).sqrt();
    lmin.max(0.0).sqrt() <= size * 1e-9
}

fn bbox(pts: &[DVec2]) -> (DVec2, DVec2) {
    pts.iter()
        .fold((DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)), |(lo, hi), p| (lo.min(*p), hi.max(*p)))
}

/// First self-intersection of a contour: `(i, j)` segment indices (`i <= j`).
fn self_intersection(polys: &[Vec<DVec2>], closed: bool, eps: f64) -> Option<(usize, usize)> {
    let n = polys.len();
    let boxes: Vec<(DVec2, DVec2)> = polys.iter().map(|p| bbox(p)).collect();
    for j in 0..n {
        for i in 0..=j {
            let (a, b) = (boxes[i], boxes[j]);
            if a.0.x > b.1.x + eps || b.0.x > a.1.x + eps || a.0.y > b.1.y + eps || b.0.y > a.1.y + eps {
                continue;
            }
            let pi = &polys[i];
            let pj = &polys[j];
            // joints where the two polylines meet legitimately
            let mut joints: Vec<DVec2> = Vec::new();
            if j == i + 1 {
                joints.push(pi[pi.len() - 1]);
            }
            if closed && i == 0 && j + 1 == n {
                joints.push(pi[0]);
            }
            for ei in 0..pi.len() - 1 {
                let ej0 = if i == j { ei + 2 } else { 0 };
                for ej in ej0..pj.len().saturating_sub(1) {
                    let Some(x) = seg_hit(pi[ei], pi[ei + 1], pj[ej], pj[ej + 1], eps) else { continue };
                    if joints.iter().any(|jp| x.distance(*jp) <= eps * 10.0) {
                        continue;
                    }
                    // a single closed segment: its first and last edge meet at the start
                    if i == j && closed && n == 1 && ei == 0 && ej + 2 == pj.len() && x.distance(pi[0]) <= eps * 10.0 {
                        continue;
                    }
                    return Some((i, j));
                }
            }
        }
    }
    None
}

impl<'a> Ev<'a> {
    /// `sketch(closed = true) { … }`.
    #[inline(never)]
    pub(crate) fn m_sketch(
        &mut self,
        s: &'a Stmt,
        a: &Args<'a>,
        children: &'a [Stmt],
        ctx: &Ctx<'a>,
        dy: &Dyn<'a>,
    ) -> R<()> {
        let p = self.bind("sketch", a, &["closed"], 1);
        let closed = p[0].is_none_or(|x| x.val.truthy());
        if self.sketch.is_some() {
            self.error(s.span, "`sketch` in einer Skizze nicht erlaubt");
            return Ok(());
        }
        let node = self.profile(s, Profile::Sketch { contours: Vec::new(), closed }, ctx, dy);
        self.sketch = Some(SketchBuild { node, closed, contours: Vec::new(), hole: 0, steps: Vec::new() });
        let r = self.eval_children(node, &DMat4::IDENTITY, children, ctx, dy);
        let Some(b) = self.sketch.take() else { return r };
        r?;
        self.finish_sketch(s, b);
        Ok(())
    }

    /// A segment statement (only called while a sketch is evaluated).
    #[inline(never)]
    pub(crate) fn segment_stmt(
        &mut self,
        s: &'a Stmt,
        name: &'a Ident,
        a: Args<'a>,
        children: &'a [Stmt],
        ctx: &Ctx<'a>,
    ) -> R<()> {
        let n = name.name.as_str();
        if n != "hole" && children.iter().any(crate::eval::is_geometric) {
            self.warn(name.span, format!("`{n}()` nimmt keine Kinder"));
        }
        match n {
            "start" => self.sk_start(s, &a, ctx),
            "line" => self.sk_line(s, &a, ctx),
            "arc" => self.sk_arc(s, &a, ctx),
            "bezier" => self.sk_bezier(s, &a, ctx),
            "spline" => self.sk_spline(s, &a, ctx),
            "close" => self.sk_close(s, &a),
            "corner" => self.sk_corner(s, &a, ctx),
            _ => {
                // hole { … }
                let _ = self.bind("hole", &a, &[], 0);
                self.close_open_contour();
                let node = self.node(s, Op::Group, ctx);
                if let Some(b) = self.sketch.as_mut() {
                    b.hole += 1;
                }
                let dy = ctx.dyn_.clone();
                let r = self.eval_children(node, &DMat4::IDENTITY, children, ctx, &dy);
                self.close_open_contour();
                if let Some(b) = self.sketch.as_mut() {
                    b.hole -= 1;
                }
                return r;
            }
        }
        Ok(())
    }

    fn sk(&mut self) -> &mut SketchBuild<'a> {
        self.sketch.as_mut().expect("segment outside a sketch")
    }

    /// Records the drawing state after a segment statement (sketch editor).
    fn sk_step(&mut self, s: &'a Stmt, kind: SketchStepKind, seg: Option<Seg>) {
        let Some(b) = self.sketch.as_mut() else { return };
        let Some(c) = b.contours.last() else { return };
        let contour = b.contours.len() as u32 - 1;
        let (point, tangent) = (c.end(), c.end_tangent());
        b.steps.push(SketchStep { stmt: s.id, span: s.span, kind, contour, hole: c.hole, seg, point, tangent });
    }

    /// Current point and end tangent of the open contour, if any.
    fn sk_cur(&self) -> Option<(DVec2, Option<DVec2>)> {
        let b = self.sketch.as_ref()?;
        let c = b.contours.last().filter(|c| !c.closed)?;
        Some((c.end(), c.end_tangent()))
    }

    fn sk_need_cur(&mut self, s: &'a Stmt) -> Option<(DVec2, Option<DVec2>)> {
        match self.sk_cur() {
            Some(c) => Some(c),
            None => {
                let after_close = self.sketch.as_ref().is_some_and(|b| b.contours.last().is_some_and(|c| c.closed));
                let msg = if after_close {
                    "nach `close()` beginnt eine neue Kontur mit `start(p)`"
                } else {
                    "Kontur ohne Anfang: zuerst `start(p)`"
                };
                self.error(s.span, msg);
                None
            }
        }
    }

    /// Point argument in sketch coordinates (grid points and anchors are projected).
    fn sk_point(&mut self, x: Option<&ArgV<'a>>, what: &str) -> Option<DVec2> {
        let x = x?;
        if x.val.is_special() {
            self.special_misuse(x, what);
            return None;
        }
        match self.pt2(&x.val, x.expr.span) {
            Some(p) if p.is_finite() => Some(p),
            _ => {
                let t = x.val.type_de();
                self.error(x.expr.span, format!("`{what}` muss ein Punkt [x, y] sein, ist {t}"));
                None
            }
        }
    }

    /// Handle frame: the sketch's plane (for the profile of a `rotate_extrude` the xz-plane,
    /// where the sketch is drawn, Plan 17.12).
    fn sk_frame(&self) -> DMat4 {
        let node = self.sketch.as_ref().map_or(0, |b| b.node);
        crate::sketch_edit::frame_in(&self.nodes, node)
    }

    /// Point handle on a point argument (`origin`: the point the value is relative to).
    fn sk_handle(&mut self, s: &'a Stmt, ctx: &Ctx<'a>, index: u32, x: &ArgV<'a>, v: DVec2, origin: Option<DVec2>) {
        let dims = x.val.as_list().map_or(2, |l| l.len().clamp(2, 3));
        let (comps, has_ref) = comps_of(&x.val, dims, 1.0);
        let mut frame = self.sk_frame();
        if let Some(o) = origin {
            frame *= DMat4::from_translation(o.extend(0.0));
        }
        let node = self.sketch.as_ref().map(|b| b.node);
        let pos = v.extend(0.0);
        self.add_handle(HSpec {
            stmt: s,
            ctx,
            node,
            kind: HandleKind::Point,
            index,
            frame,
            pos,
            axis: None,
            // relative value (`by`): no snapping to grid points (see `snap_targets`)
            base: origin.map(|_| glam::DVec3::ZERO),
            value: (0..dims).map(|i| pos[i]).collect(),
            comps,
            has_ref,
            expr_span: x.expr.span,
            def_span: x.val.def,
            via_var: false,
        });
    }

    /// 1D handle (radius, length) along `axis` from `base`.
    #[allow(clippy::too_many_arguments)]
    fn sk_scalar_handle(
        &mut self,
        s: &'a Stmt,
        ctx: &Ctx<'a>,
        kind: HandleKind,
        index: u32,
        x: &ArgV<'a>,
        value: f64,
        base: DVec2,
        axis: DVec2,
    ) {
        let (comps, has_ref) = comp_scalar(&x.val, 1.0);
        let frame = self.sk_frame();
        let node = self.sketch.as_ref().map(|b| b.node);
        let (base, axis) = (base.extend(0.0), axis.extend(0.0));
        self.add_handle(HSpec {
            stmt: s,
            ctx,
            node,
            kind,
            index,
            frame,
            pos: base + axis * value,
            axis: Some(axis),
            base: Some(base),
            value: vec![value],
            comps,
            has_ref,
            expr_span: x.expr.span,
            def_span: x.val.def,
            via_var: false,
        });
    }

    fn push_seg(&mut self, s: &'a Stmt, seg: Seg) {
        let (a, b) = (seg.start(), seg.end());
        let len = match &seg {
            Seg::Arc { r, a0, a1, .. } => r * (a1 - a0).abs(),
            _ => a.distance(b),
        };
        if len <= 1e-9 * (1.0 + a.length()) && !matches!(seg, Seg::Bezier { .. } | Seg::Spline { .. }) {
            self.warn(s.span, "Segment ohne Länge wird ignoriert");
            return;
        }
        if let Some(c) = self.sk().contours.last_mut() {
            c.segs.push(SegB { seg: seg.clone(), stmt: s, implicit: false, error: false });
        }
        let kind = if matches!(&s.kind, cad_syntax::ast::StmtKind::Call { name, .. } if name.name == "close") {
            SketchStepKind::Close
        } else {
            SketchStepKind::Seg
        };
        self.sk_step(s, kind, Some(seg));
    }

    fn mark_error(&mut self, s: &Stmt, msg: impl Into<String>) {
        self.error(s.span, msg);
        if let Some(b) = self.sketch.as_mut() {
            for c in &mut b.contours {
                for g in &mut c.segs {
                    if g.stmt.id == s.id {
                        g.error = true;
                    }
                }
            }
        }
    }

    /// Ends an unclosed contour before a new one starts (error in closed sketches).
    fn close_open_contour(&mut self) {
        let Some(b) = self.sketch.as_ref() else { return };
        let closed_mode = b.closed;
        let Some(c) = b.contours.last() else { return };
        if c.closed {
            return;
        }
        let tol = 1e-9 * (1.0 + c.start.length());
        let (start, end) = (c.start, c.end());
        let last = c.segs.last().map(|g| g.stmt).or(c.start_stmt);
        let nsegs = c.segs.len();
        if !closed_mode {
            return;
        }
        if nsegs > 0 && start.distance(end) <= tol {
            if let Some(c) = self.sk().contours.last_mut() {
                c.closed = true;
            }
            return;
        }
        if let Some(st) = last {
            if nsegs == 0 {
                self.mark_error(st, "Kontur ohne Segmente");
            } else {
                self.mark_error(
                    st,
                    format!("Kontur nicht geschlossen: endet bei {} statt {}; `close()` fehlt", fmt2(end), fmt2(start)),
                );
            }
        }
        if let Some(c) = self.sk().contours.last_mut() {
            if nsegs > 0
                && let Some(st) = last
            {
                c.segs.push(SegB { seg: Seg::Line { a: end, b: start }, stmt: st, implicit: true, error: true });
            }
            c.closed = true;
        }
    }

    fn sk_start(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>) {
        let p = self.bind("start", a, &["p"], 1);
        let Some(pt) = self.sk_point(p[0], "p") else {
            if p[0].is_none() {
                self.error(s.span, "`start(p)` braucht einen Punkt");
            }
            return;
        };
        self.close_open_contour();
        let hole = self.sk().hole > 0;
        self.sk().contours.push(ContourB {
            hole,
            closed: false,
            start: pt,
            start_stmt: Some(s),
            segs: Vec::new(),
            corners: Vec::new(),
        });
        if let Some(x) = p[0] {
            self.sk_handle(s, ctx, 0, x, pt, None);
        }
        self.sk_step(s, SketchStepKind::Start, None);
    }

    fn sk_line(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>) {
        let p = self.bind("line", a, &["to", "by", "length", "angle"], 1);
        let Some((cur, tan)) = self.sk_need_cur(s) else { return };
        let given = p.iter().filter(|x| x.is_some()).count();
        let end = if let Some(x) = p[0] {
            if p[1].is_some() || p[2].is_some() || p[3].is_some() {
                self.warn(s.span, "line: `to` gegeben, `by`/`length`/`angle` werden ignoriert");
            }
            let Some(to) = self.sk_point(Some(x), "to") else { return };
            self.sk_handle(s, ctx, 0, x, to, None);
            to
        } else if let Some(x) = p[1] {
            if p[2].is_some() || p[3].is_some() {
                self.warn(s.span, "line: `by` gegeben, `length`/`angle` werden ignoriert");
            }
            let Some(by) = self.sk_point(Some(x), "by") else { return };
            self.sk_handle(s, ctx, 0, x, by, Some(cur));
            cur + by
        } else if let Some(lx) = p[2] {
            let Some(len) = self.num_of(Some(lx), "length") else { return };
            let dir = match p[3] {
                Some(ax) => {
                    let Some(ang) = self.num_of(Some(ax), "angle") else { return };
                    DVec2::new(crate::math::cos_deg(ang), crate::math::sin_deg(ang))
                }
                None => match tan {
                    Some(t) => t,
                    None => {
                        self.error(s.span, "line(length = …) ohne `angle` braucht ein vorheriges Segment (Richtung)");
                        return;
                    }
                },
            };
            self.sk_scalar_handle(s, ctx, HandleKind::Height, 0, lx, len, cur, dir);
            cur + dir * len
        } else {
            if given == 0 {
                self.error(s.span, "line() braucht `to`, `by` oder `length` (+ `angle`)");
            } else {
                self.error(s.span, "line(angle = …) braucht auch `length`");
            }
            return;
        };
        self.push_seg(s, Seg::Line { a: cur, b: end });
    }

    fn sk_arc(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>) {
        let p = self.bind("arc", a, &["to", "r", "large", "cw", "center", "angle", "through", "tangent"], 1);
        let Some((cur, tan)) = self.sk_need_cur(s) else { return };
        let flag = |x: Option<&ArgV<'a>>| x.is_some_and(|v| v.val.truthy());
        let (to_a, r_a, large_a, cw_a, center_a, angle_a, through_a, tangent_a) =
            (p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7]);
        // centre + angle
        if let Some(cx) = center_a {
            let Some(c) = self.sk_point(Some(cx), "center") else { return };
            self.sk_handle(s, ctx, 1, cx, c, None);
            let r = cur.distance(c);
            if r <= 1e-9 * (1.0 + c.length()) {
                self.mark_error(s, "unmöglicher Bogen: Mittelpunkt liegt auf dem Anfangspunkt");
                return;
            }
            let seg = if let Some(ax) = angle_a {
                if to_a.is_some() {
                    self.warn(s.span, "arc: `center` und `angle` gegeben, `to` wird ignoriert");
                }
                let Some(ang) = self.num_of(Some(ax), "angle") else { return };
                if ang == 0.0 || ang.abs() > 360.0 {
                    self.mark_error(
                        s,
                        format!("unmöglicher Bogen: Winkel {}° (erlaubt 0 < |Winkel| ≤ 360)", fmt_num(ang)),
                    );
                    return;
                }
                let a0 = angle_of(cur - c);
                Seg::Arc { center: c, r, a0, a1: a0 + ang.to_radians(), ccw: ang > 0.0 }
            } else if let Some(tx) = to_a {
                // centre + end point: the end lies on the circle (its angle counts)
                let Some(to) = self.sk_point(Some(tx), "to") else { return };
                self.sk_handle(s, ctx, 0, tx, to, None);
                let rt = to.distance(c);
                if (rt - r).abs() > 1e-6 * (1.0 + r) {
                    self.warn(
                        s.span,
                        format!("arc: Endpunkt hat Abstand {} vom Mittelpunkt statt {}", fmt_num(rt), fmt_num(r)),
                    );
                }
                let to = c + (to - c).normalize_or_zero() * r;
                arc_seg(c, cur, to, !flag(cw_a))
            } else {
                self.error(s.span, "arc(center = …) braucht `angle` oder `to`");
                return;
            };
            self.push_seg(s, seg);
            return;
        }
        let Some(tx) = to_a else {
            self.error(s.span, "arc() braucht `to` (mit `r`, `through` oder `tangent`) oder `center` + `angle`");
            return;
        };
        let Some(to) = self.sk_point(Some(tx), "to") else { return };
        self.sk_handle(s, ctx, 0, tx, to, None);
        let chord = to - cur;
        let d = chord.length();
        if d <= 1e-9 * (1.0 + cur.length()) {
            self.mark_error(s, "unmöglicher Bogen: Endpunkt gleich Anfangspunkt");
            return;
        }
        if let Some(px) = through_a {
            let Some(q) = self.sk_point(Some(px), "through") else { return };
            self.sk_handle(s, ctx, 2, px, q, None);
            // circumcentre of cur, q, to
            let (b, c) = (q - cur, to - cur);
            let den = 2.0 * b.perp_dot(c);
            if den.abs() <= 1e-12 * (b.length() * c.length()).max(1e-300) {
                self.mark_error(s, "unmöglicher Bogen: die drei Punkte liegen auf einer Geraden");
                return;
            }
            let ux = (c.y * b.length_squared() - b.y * c.length_squared()) / den;
            let uy = (b.x * c.length_squared() - c.x * b.length_squared()) / den;
            let center = cur + DVec2::new(ux, uy);
            let ccw = b.perp_dot(to - q) > 0.0;
            self.push_seg(s, arc_seg(center, cur, to, ccw));
            return;
        }
        if let Some(rx) = r_a {
            let Some(r) = self.num_of(Some(rx), "r") else { return };
            if tangent_a.is_some() {
                self.warn(s.span, "arc: `r` gegeben, `tangent` wird ignoriert");
            }
            if r.is_nan() || r <= 0.0 {
                self.mark_error(s, format!("unmöglicher Bogen: Radius {} ≤ 0", fmt_num(r)));
                return;
            }
            if r < d / 2.0 * (1.0 - 1e-9) {
                self.mark_error(
                    s,
                    format!(
                        "unmöglicher Bogen: Radius {} kleiner als die halbe Sehne {}",
                        fmt_num(r),
                        fmt_num(d / 2.0)
                    ),
                );
                return;
            }
            let ccw = !flag(cw_a);
            let large = flag(large_a);
            let h = (r * r - d * d / 4.0).max(0.0).sqrt();
            let n = DVec2::new(-chord.y, chord.x) / d;
            let sign = if ccw != large { 1.0 } else { -1.0 };
            let center = (cur + to) / 2.0 + n * h * sign;
            let seg = arc_seg(center, cur, to, ccw);
            if let Seg::Arc { center, a0, a1, .. } = &seg {
                let mid = (a0 + a1) / 2.0;
                let axis = DVec2::new(mid.cos(), mid.sin());
                self.sk_scalar_handle(s, ctx, HandleKind::Radius, 0, rx, r, *center, axis);
            }
            self.push_seg(s, seg);
            return;
        }
        // tangent arc (default without r / through)
        if tangent_a.is_some_and(|t| !t.val.truthy()) {
            self.error(s.span, "arc(to = …) braucht `r`, `through` oder `tangent = true`");
            return;
        }
        let Some(t) = tan else {
            self.mark_error(s, "tangentialer Bogen braucht ein vorheriges Segment (Richtung)");
            return;
        };
        let n = DVec2::new(-t.y, t.x);
        let nd = n.dot(chord);
        if nd.abs() <= 1e-9 * d {
            self.mark_error(
                s,
                "unmöglicher tangentialer Bogen: der Endpunkt liegt auf der Tangente (`line` verwenden)",
            );
            return;
        }
        let k = d * d / (2.0 * nd);
        let center = cur + n * k;
        self.push_seg(s, arc_seg(center, cur, to, k > 0.0));
    }

    fn sk_bezier(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>) {
        // positional: (c1, to) or (c1, c2, to)
        let npos = a.positional().count();
        let names: &[&str] = if npos == 2 { &["c1", "to", "c2"] } else { &["c1", "c2", "to"] };
        let p = self.bind("bezier", a, names, npos.min(3));
        let (c1a, c2a, toa) = if npos == 2 { (p[0], p[2], p[1]) } else { (p[0], p[1], p[2]) };
        let Some((cur, _)) = self.sk_need_cur(s) else { return };
        let (Some(c1x), Some(tox)) = (c1a, toa) else {
            self.error(s.span, "bezier() braucht `c1` und `to` (kubisch zusätzlich `c2`)");
            return;
        };
        let Some(c1) = self.sk_point(Some(c1x), "c1") else { return };
        let c2 = match c2a {
            Some(x) => match self.sk_point(Some(x), "c2") {
                Some(v) => Some(v),
                None => return,
            },
            None => None,
        };
        let Some(to) = self.sk_point(Some(tox), "to") else { return };
        self.sk_handle(s, ctx, 0, tox, to, None);
        self.sk_handle(s, ctx, 1, c1x, c1, None);
        if let (Some(x), Some(v)) = (c2a, c2) {
            self.sk_handle(s, ctx, 2, x, v, None);
        }
        let mut points = vec![cur, c1];
        points.extend(c2);
        points.push(to);
        if cur.distance(to) <= 1e-12 && points.iter().all(|q| q.distance(cur) <= 1e-12) {
            self.warn(s.span, "Segment ohne Länge wird ignoriert");
            return;
        }
        self.push_seg(s, Seg::Bezier { points });
    }

    fn sk_spline(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>) {
        let p = self.bind("spline", a, &["points", "tangent", "end_tangent"], 1);
        let Some((cur, tan)) = self.sk_need_cur(s) else { return };
        let Some(px) = p[0] else {
            self.error(s.span, "spline() braucht `points` (Punkte nach dem aktuellen Punkt)");
            return;
        };
        let Some(list) = px.val.as_list() else {
            self.error(px.expr.span, "spline: `points` muss eine Liste von [x, y] sein");
            return;
        };
        if list.is_empty() {
            self.error(px.expr.span, "spline: `points` braucht mindestens einen Punkt");
            return;
        }
        let mut pts = vec![cur];
        let node = self.sketch.as_ref().map_or(0, |b| b.node);
        let frame = self.sk_frame();
        for (i, e) in list.iter().enumerate() {
            let Some(q) = self.pt2(e, e.def.unwrap_or(px.expr.span)) else {
                self.error(px.expr.span, "spline: `points` muss eine Liste von [x, y] sein");
                return;
            };
            let dims = e.as_list().map_or(2, |l| l.len().clamp(2, 3));
            self.elem_handle(s, ctx, node, i as u32, px, e, q.extend(0.0), dims, frame);
            pts.push(q);
        }
        if pts.windows(2).any(|w| w[0].distance(w[1]) <= 1e-9 * (1.0 + w[0].length())) {
            self.mark_error(s, "spline: zwei aufeinanderfolgende Punkte sind gleich");
            return;
        }
        let t0 = match p[1] {
            None => None,
            Some(x) => match (&x.val.v, x.val.as_vec2()) {
                (crate::value::Value::Bool(true), _) => match tan {
                    Some(t) => Some(t),
                    None => {
                        self.error(x.expr.span, "spline(tangent = true) braucht ein vorheriges Segment");
                        None
                    }
                },
                (crate::value::Value::Bool(false), _) => None,
                (_, Some(v)) if v.length() > 0.0 => Some(v),
                _ => {
                    self.error(x.expr.span, "spline: `tangent` muss true/false oder eine Richtung [x, y] sein");
                    None
                }
            },
        };
        let t1 = match p[2] {
            None => None,
            Some(x) => match x.val.as_vec2() {
                Some(v) if v.length() > 0.0 => Some(v),
                _ => {
                    self.error(x.expr.span, "spline: `end_tangent` muss eine Richtung [x, y] ≠ 0 sein");
                    None
                }
            },
        };
        self.push_seg(s, Seg::Spline { points: pts, t0, t1 });
    }

    /// `corner(r = …)` / `corner(chamfer = d)` between the previous and the next segment.
    fn sk_corner(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>) {
        let p = self.bind("corner", a, &["r", "chamfer"], 1);
        let (x, chamfer) = match (p[0], p[1]) {
            (Some(x), None) => (x, false),
            (None, Some(x)) => (x, true),
            (Some(x), Some(_)) => {
                self.warn(s.span, "corner: `r` und `chamfer` gegeben, `chamfer` wird ignoriert");
                (x, false)
            }
            (None, None) => {
                self.error(s.span, "corner() braucht `r` (Radius) oder `chamfer` (Abstand)");
                return;
            }
        };
        let what = if chamfer { "chamfer" } else { "r" };
        let Some(v) = self.num_of(Some(x), what) else { return };
        if v.is_nan() || v <= 0.0 {
            self.error(x.expr.span, format!("corner: `{what}` = {} muss > 0 sein", fmt_num(v)));
            return;
        }
        let Some(b) = self.sketch.as_ref() else { return };
        let Some(c) = b.contours.last() else {
            self.error(s.span, "corner() vor `start(p)`: eine Ecke liegt zwischen zwei Segmenten");
            return;
        };
        if c.closed {
            self.error(
                s.span,
                "corner() nach `close()`: für die Startecke `corner()` direkt nach `start(p)` schreiben",
            );
            return;
        }
        let at = c.segs.len();
        if c.corners.iter().any(|k| k.at == at) {
            self.error(s.span, "an dieser Ecke steht schon ein `corner()`");
            return;
        }
        let cur = c.end();
        let dir = c.end_tangent().unwrap_or(DVec2::X);
        let before = self.handles.len();
        let kind = if chamfer { HandleKind::Height } else { HandleKind::Radius };
        self.sk_scalar_handle(s, ctx, kind, 0, x, v, cur, DVec2::new(-dir.y, dir.x));
        let handle = (self.handles.len() > before).then_some(before);
        let kind = if chamfer { CornerKind::Chamfer(v) } else { CornerKind::Round(v) };
        if let Some(c) = self.sk().contours.last_mut() {
            c.corners.push(CornerB { stmt: s, at, kind, handle });
        }
        self.sk_step(s, SketchStepKind::Corner, None);
    }

    /// Applies the `corner()`s of a contour: the segments at each vertex are shortened and the
    /// rounding (arc) or chamfer (line) is inserted, attributed to the `corner` statement.
    fn apply_corners(&mut self, c: &mut ContourB<'a>) {
        if c.corners.is_empty() {
            return;
        }
        let corners = std::mem::take(&mut c.corners);
        let n = c.segs.len();
        let has_error = c.segs.iter().any(|g| g.implicit);
        let ks: Vec<Option<cad_kernel::Segment>> = c.segs.iter().map(|g| g.seg.to_kernel()).collect();
        if ks.iter().any(|k| k.is_none()) {
            return;
        }
        let ks: Vec<cad_kernel::Segment> = ks.into_iter().flatten().collect();
        let mut list: Vec<(usize, CornerKind)> = Vec::new();
        let mut which: Vec<&CornerB<'a>> = Vec::new();
        for k in &corners {
            if n == 0 {
                self.error(k.stmt.span, "corner(): Kontur ohne Segmente");
                continue;
            }
            let v = if c.closed { k.at % n } else { k.at };
            if !c.closed && (v == 0 || v >= n) {
                let msg = if v == 0 {
                    "corner(): eine offene Kontur hat keine Startecke"
                } else {
                    "corner(): nach dem letzten Segment einer offenen Kontur folgt keines mehr"
                };
                self.error(k.stmt.span, msg);
                continue;
            }
            if has_error {
                continue;
            }
            if list.iter().any(|(w, _)| *w == v) {
                self.error(k.stmt.span, "an dieser Ecke steht schon ein `corner()`");
                continue;
            }
            list.push((v, k.kind));
            which.push(k);
        }
        if list.is_empty() {
            return;
        }
        let (pieces, results) = apply_corners(&ks, c.closed, &list);
        for (i, (k, r)) in which.iter().zip(&results).enumerate() {
            let v = list[i].0;
            let (a, b) = (&ks[(v + n - 1) % n], &ks[v]);
            let vertex = b.start();
            let what = match k.kind {
                CornerKind::Round(r) => format!("Radius {}", fmt_num(r)),
                CornerKind::Chamfer(d) => format!("Fase {}", fmt_num(d)),
            };
            match r {
                Ok(res) => {
                    if res.residual > 1e-6 {
                        self.warn(
                            k.stmt.span,
                            format!("corner(): numerische Lösung ungenau (Rest {:.1e})", res.residual),
                        );
                    }
                    let mid_pt = match &res.mid {
                        cad_kernel::Segment::Arc { center, r, a0, a1 } => {
                            *center + DVec2::new(((a0 + a1) / 2.0).cos(), ((a0 + a1) / 2.0).sin()) * *r
                        }
                        m => (m.start() + m.end()) / 2.0,
                    };
                    self.fix_corner_handle(k, vertex, mid_pt);
                }
                Err(e) => {
                    let (ta, tb) = (a.end_tangent(), b.start_tangent());
                    let inside = (tb - ta).normalize_or_zero();
                    let phi = cad_kernel::region2::turn(ta, tb).abs();
                    let val = match k.kind {
                        CornerKind::Round(r) | CornerKind::Chamfer(r) => r,
                    };
                    let reach = match k.kind {
                        CornerKind::Round(r) => r * (1.0 / (phi / 2.0).cos().max(1e-6) - 1.0),
                        CornerKind::Chamfer(d) => d * (phi / 2.0).sin(),
                    };
                    self.fix_corner_handle(k, vertex, vertex + inside * reach.max(val * 1e-3));
                    let msg = match e {
                        CornerError::Tangent => {
                            "corner(): keine Ecke, die Segmente gehen tangential ineinander über".to_string()
                        }
                        CornerError::Cusp => "corner(): Spitze (Umkehr um 180°) lässt sich nicht verrunden".to_string(),
                        CornerError::TooLarge { max: Some(m) } => format!(
                            "corner(): {what} passt nicht zwischen die Segmente (höchstens ≈ {})",
                            fmt_num((m * 1e4).floor() / 1e4)
                        ),
                        CornerError::TooLarge { max: None } | CornerError::NoSolution => {
                            format!("corner(): {what} passt nicht an diese Ecke")
                        }
                    };
                    self.error(k.stmt.span, msg);
                }
            }
        }
        let old = std::mem::take(&mut c.segs);
        let mut segs: Vec<SegB<'a>> = Vec::with_capacity(pieces.len());
        for (piece, seg) in pieces {
            match piece {
                Piece::Seg(i) => {
                    let g = &old[i];
                    if seg == ks[i] {
                        segs.push(SegB { seg: g.seg.clone(), ..*g });
                    } else {
                        segs.push(SegB { seg: Seg::from_kernel(&seg), ..*g });
                    }
                }
                Piece::Corner(k) => {
                    segs.push(SegB { seg: Seg::from_kernel(&seg), stmt: which[k].stmt, implicit: false, error: false });
                }
            }
        }
        if let Some(first) = segs.first() {
            c.start = first.seg.start();
        }
        c.segs = segs;
    }

    /// Places a corner's radius handle on the rounding (linear in the value from the vertex).
    fn fix_corner_handle(&mut self, k: &CornerB<'a>, vertex: DVec2, mid: DVec2) {
        let Some(i) = k.handle else { return };
        let val = match k.kind {
            CornerKind::Round(r) | CornerKind::Chamfer(r) => r,
        };
        let Some(h) = self.handles.get_mut(i) else { return };
        let axis = (mid - vertex) / val;
        h.base = Some(vertex.extend(0.0));
        h.axis = Some(axis.extend(0.0));
        h.pos = mid.extend(0.0);
        h.value = vec![val];
    }

    fn sk_close(&mut self, s: &'a Stmt, a: &Args<'a>) {
        let _ = self.bind("close", a, &[], 0);
        let Some((cur, _)) = self.sk_need_cur(s) else { return };
        let Some(c) = self.sketch.as_ref().and_then(|b| b.contours.last()) else { return };
        let start = c.start;
        let nsegs = c.segs.len();
        if nsegs == 0 {
            self.mark_error(s, "close(): Kontur ohne Segmente");
            if let Some(c) = self.sk().contours.last_mut() {
                c.closed = true;
            }
            return;
        }
        if cur.distance(start) > 1e-9 * (1.0 + start.length()) {
            self.push_seg(s, Seg::Line { a: cur, b: start });
        } else {
            self.sk_step(s, SketchStepKind::Close, None);
        }
        if let Some(c) = self.sk().contours.last_mut() {
            c.closed = true;
        }
    }

    /// Validates the contours, writes the profile into the sketch node and records the
    /// segment statements.
    fn finish_sketch(&mut self, s: &'a Stmt, b: SketchBuild<'a>) {
        let node = b.node;
        self.sketch = Some(b);
        self.close_open_contour();
        let b = self.sketch.take().expect("sketch");
        let steps = b.steps;
        let mut contours: Vec<ContourB<'a>> = b.contours;
        if contours.is_empty() {
            self.warn(s.span, "leere Skizze");
        }
        // size for tolerances
        let mut lo = DVec2::splat(f64::INFINITY);
        let mut hi = DVec2::splat(f64::NEG_INFINITY);
        for c in &contours {
            lo = lo.min(c.start);
            hi = hi.max(c.start);
            for g in &c.segs {
                for q in [g.seg.start(), g.seg.end()] {
                    lo = lo.min(q);
                    hi = hi.max(q);
                }
            }
        }
        let size = if lo.x.is_finite() { (hi - lo).length().max(1e-6) } else { 1.0 };
        for c in &mut contours {
            self.apply_corners(c);
        }
        let tol = size * 2e-4;
        let eps = size * 1e-9;
        let mut errors: Vec<(StmtId, String)> = Vec::new();
        for c in &mut contours {
            if c.segs.is_empty() {
                if !b.closed
                    && let Some(st) = c.start_stmt
                {
                    errors.push((st.id, "Kontur ohne Segmente".into()));
                }
                continue;
            }
            let mut ks = Vec::with_capacity(c.segs.len());
            for g in &c.segs {
                match g.seg.to_kernel() {
                    Some(k) => ks.push(k),
                    None => errors.push((g.stmt.id, "Segment lässt sich nicht berechnen".into())),
                }
            }
            if ks.len() != c.segs.len() {
                continue;
            }
            let polys: Vec<Vec<DVec2>> = ks.iter().map(|k| k.sample(tol)).collect();
            if c.closed && flat(&polys, size) {
                let st = c.start_stmt.unwrap_or(c.segs[0].stmt);
                errors.push((st.id, "Kontur ohne Fläche".into()));
            } else if let Some((i, j)) = self_intersection(&polys, c.closed, eps) {
                let other = c.segs[i].stmt;
                let st = c.segs[j].stmt;
                let msg = if other.id == st.id {
                    "Selbstüberschneidung der Kontur".to_string()
                } else {
                    format!("Selbstüberschneidung der Kontur mit Segment in {}", self.where_(other.span, st.span))
                };
                errors.push((st.id, msg));
            } else if c.closed {
                let area = cad_kernel::curve2::signed_area(&ks, tol);
                if area.abs() <= size * size * 1e-9 {
                    let st = c.start_stmt.unwrap_or(c.segs[0].stmt);
                    errors.push((st.id, "Kontur ohne Fläche".into()));
                }
            }
        }
        for (id, msg) in &errors {
            if let Some(g) = contours.iter_mut().flat_map(|c| c.segs.iter_mut()).find(|g| g.stmt.id == *id) {
                g.error = true;
                let sp = g.stmt.span;
                self.error(sp, msg.clone());
            } else if let Some(st) = contours.iter().filter_map(|c| c.start_stmt).find(|st| st.id == *id) {
                self.error(st.span, msg.clone());
            }
        }
        let prof = Profile::Sketch {
            contours: contours
                .iter()
                .filter(|c| !c.segs.is_empty())
                .map(|c| Contour {
                    hole: c.hole,
                    closed: c.closed,
                    segs: c.segs.iter().map(|g| g.seg.clone()).collect(),
                })
                .collect(),
            closed: b.closed,
        };
        self.nodes[node as usize].op = Op::Profile(prof);
        let info = SketchInfo {
            node,
            stmt: s.id,
            span: s.span,
            closed: b.closed,
            contours: contours
                .iter()
                .filter(|c| !c.segs.is_empty())
                .map(|c| SketchContourInfo {
                    hole: c.hole,
                    closed: c.closed,
                    start_stmt: c.start_stmt.map(|x| x.id),
                    start_span: c.start_stmt.map(|x| x.span),
                    segs: c
                        .segs
                        .iter()
                        .map(|g| SketchSegInfo {
                            stmt: g.stmt.id,
                            span: g.stmt.span,
                            implicit: g.implicit,
                            error: g.error,
                        })
                        .collect(),
                })
                .collect(),
            steps,
        };
        self.sketches.push(info);
    }
}
