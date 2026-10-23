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
use crate::output::{HandleKind, SketchContourInfo, SketchInfo, SketchSegInfo};
use crate::value::fmt_num;

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

fn bbox(pts: &[DVec2]) -> (DVec2, DVec2) {
    pts.iter().fold((DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)), |(lo, hi), p| (lo.min(*p), hi.max(*p)))
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
        self.sketch = Some(SketchBuild { node, closed, contours: Vec::new(), hole: 0 });
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
            "corner" => {
                self.warn(s.span, "`corner()` wird noch nicht unterstützt (Plan 17.7, Schritt 2) und ignoriert");
            }
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

    fn sk_frame(&self) -> DMat4 {
        let node = self.sketch.as_ref().map_or(0, |b| b.node);
        self.nodes[node as usize].world
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
            base: None,
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
            c.segs.push(SegB { seg, stmt: s, implicit: false, error: false });
        }
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
        self.sk().contours.push(ContourB { hole, closed: false, start: pt, start_stmt: Some(s), segs: Vec::new() });
        if let Some(x) = p[0] {
            self.sk_handle(s, ctx, 0, x, pt, None);
        }
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
                    self.mark_error(s, format!("unmöglicher Bogen: Winkel {}° (erlaubt 0 < |Winkel| ≤ 360)", fmt_num(ang)));
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
            if !(r > 0.0) {
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
            self.mark_error(s, "unmöglicher tangentialer Bogen: der Endpunkt liegt auf der Tangente (`line` verwenden)");
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
            if c.closed {
                let area = cad_kernel::curve2::signed_area(&ks, tol);
                if area.abs() <= size * size * 1e-9 {
                    let st = c.start_stmt.unwrap_or(c.segs[0].stmt);
                    errors.push((st.id, "Kontur ohne Fläche".into()));
                    continue;
                }
            }
            if let Some((i, j)) = self_intersection(&polys, c.closed, eps) {
                let other = c.segs[i].stmt;
                let st = c.segs[j].stmt;
                let msg = if other.id == st.id {
                    "Selbstüberschneidung der Kontur".to_string()
                } else {
                    format!("Selbstüberschneidung der Kontur mit Segment in {}", self.where_(other.span, st.span))
                };
                errors.push((st.id, msg));
                let _ = j;
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
                .map(|c| Contour { hole: c.hole, closed: c.closed, segs: c.segs.iter().map(|g| g.seg.clone()).collect() })
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
                        .map(|g| SketchSegInfo { stmt: g.stmt.id, span: g.stmt.span, implicit: g.implicit, error: g.error })
                        .collect(),
                })
                .collect(),
        };
        self.sketches.push(info);
    }
}
