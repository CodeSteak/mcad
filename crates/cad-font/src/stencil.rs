//! „cad stencil mono“ (Plan 17.4): the default font of `text()`, a very simple monospaced
//! stencil font built from lines and circular arcs.
//!
//! Every glyph is a set of centre lines (lines and tangent arcs) drawn with a pen of width
//! `stroke` (flat ends, mitred corners), plus a few filled shapes (dots), minus the **bridges**
//! (cuts of width `bridge` across a stroke), clipped to the glyph's band where diagonals end on
//! the base or cap line. The bridges connect every counter (inside of O, A, B, 8, @ …) with the
//! outside, so a glyph has no holes: cut out of a sheet, nothing falls out. The outline is
//! computed exactly with [`cad_kernel::region2`] (pieces of lines and arcs), cached per
//! character and parameters.
//!
//! Units: 1000 per em, the em is `size / 0.72` like the other fonts (cap height 720 = `size`,
//! advance 600 like JetBrains Mono). Stroke and bridge widths are given in these units here
//! ([`render`]); [`crate::layout`] converts the `text()` parameters (mm) and supplies the
//! defaults ([`DEFAULT_STROKE`], [`DEFAULT_BRIDGE`] times `size`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::rc::Rc;

use cad_kernel::Segment;
use cad_kernel::region2::{self, Cap, Fill, Join, Loop2, Operand};
use cad_outline::{Contour, Seg};
use glam::{DAffine2, DVec2};

/// Units per em.
pub const UPEM: f64 = 1000.0;
/// Advance of every glyph (monospaced; JetBrains Mono: 600 as well).
pub const ADVANCE: f64 = 600.0;
/// Cap height (outer edge of the strokes; = `size` in mm).
pub const CAP: f64 = 720.0;
/// x-height (outer edge).
pub const XHEIGHT: f64 = 530.0;
/// Ascender of b, d, f, h, k, l and of brackets (outer edge).
pub const ASCENDER: f64 = 760.0;
/// Descender of g, j, p, q, y (outer edge).
pub const DESCENDER: f64 = -210.0;
/// Vertical metrics for top-to-bottom text: the pen moves by `ASCENT - DESCENT` per glyph.
pub const ASCENT: f64 = 800.0;
pub const DESCENT: f64 = -200.0;
/// Side bearing: the regular glyph box is `SIDE .. ADVANCE - SIDE`.
const SIDE: f64 = 60.0;

/// Default stroke width as a fraction of `size` (cap height), regular and bold.
pub const DEFAULT_STROKE: f64 = 0.14;
pub const DEFAULT_STROKE_BOLD: f64 = 0.2;
/// Default bridge width as a fraction of `size`.
pub const DEFAULT_BRIDGE: f64 = 0.1;
/// Largest stroke width (fraction of `size`): wider strokes merge neighbouring strokes (a, e,
/// #, @) and close counters.
pub const MAX_STROKE: f64 = 0.2;
/// Bridge widths (fraction of `size`): 0 (no bridges: counters stay closed, like a normal font)
/// or `MIN_BRIDGE ..= MAX_BRIDGE`. Narrower bridges are closed by heavy strokes (a, e, ø).
pub const MIN_BRIDGE: f64 = 0.03;
pub const MAX_BRIDGE: f64 = 0.2;

/// The characters of the font, in glyph id order (id = position + 1). Covers
/// [`crate::REQUIRED_CHARS`] and a few more (’ ” $).
pub const CHARS: &str = concat!(
    " ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
    "ÄÖÜäöüßẞ",
    ".,;:!?'\"„“‚‘»«()[]{}-–—_/\\&%#+*=<>°²³€@§~^|·•…±×÷µØø",
    "’”$",
);

/// Glyph id of a character (`None`: not in the font).
pub fn glyph_id(c: char) -> Option<u16> {
    CHARS.chars().position(|x| x == c).map(|i| i as u16 + 1)
}

pub fn has(c: char) -> bool {
    glyph_id(c).is_some()
}

// ------------------------------------------------------------------------------------ metrics

/// Positions of a glyph box for a stroke width: outer edges and centre lines.
#[derive(Debug, Clone, Copy)]
struct M {
    /// Stroke width, half of it, bridge width.
    w: f64,
    h: f64,
    bw: f64,
    /// Outer left/right edge of the box.
    x0: f64,
    x1: f64,
    /// Outer base line and cap line.
    y0: f64,
    cap: f64,
    /// Outer x-height, ascender, descender.
    xh: f64,
    asc: f64,
    desc: f64,
    /// Centre lines: left, right, middle (x); base, top (cap), middle of the caps, x-height,
    /// ascender, descender (y).
    l: f64,
    r: f64,
    c: f64,
    b: f64,
    t: f64,
    m: f64,
    x: f64,
    a: f64,
    d: f64,
    /// Half the width between the left and right centre lines (radius of round letters).
    rr: f64,
    /// Width between the left and right centre lines.
    wd: f64,
    /// Size of a dot (period, i, umlauts).
    ds: f64,
}

impl M {
    /// Box `x0..x1` × `y0..cap` (outer edges); x-height, ascender and descender scale with the
    /// box height.
    fn new(w: f64, bw: f64, x0: f64, x1: f64, y0: f64, cap: f64) -> M {
        let h = w / 2.0;
        let s = (cap - y0) / CAP;
        let (xh, asc, desc) = (y0 + XHEIGHT * s, y0 + ASCENDER * s, y0 + DESCENDER * s);
        let (l, r) = (x0 + h, x1 - h);
        M {
            w,
            h,
            bw,
            x0,
            x1,
            y0,
            cap,
            xh,
            asc,
            desc,
            l,
            r,
            c: (x0 + x1) / 2.0,
            b: y0 + h,
            t: cap - h,
            m: (y0 + cap) / 2.0,
            x: xh - h,
            a: asc - h,
            d: desc + h,
            rr: (r - l) / 2.0,
            wd: r - l,
            ds: w,
        }
    }

    fn regular(w: f64, bw: f64) -> M {
        M::new(w, bw, SIDE, ADVANCE - SIDE, 0.0, CAP)
    }
}

// ------------------------------------------------------------------------------------ builder

/// Paths of one stroke width: segments, closed.
type Paths = Vec<(Vec<Segment>, bool)>;

/// Collects the parts of a glyph (in font units).
struct G {
    m: M,
    /// Stroke groups: width and paths.
    strokes: Vec<(f64, Paths)>,
    /// Width of the paths being drawn.
    pen: f64,
    cur: Vec<Segment>,
    pos: DVec2,
    dir: DVec2,
    start: DVec2,
    /// Filled loops (dots, discs).
    fills: Vec<Loop2>,
    /// Bridges (removed).
    cuts: Vec<Loop2>,
    /// Union of rectangles the glyph is clipped to (empty: no clip).
    clip: Vec<Loop2>,
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn perp(d: DVec2) -> DVec2 {
    DVec2::new(-d.y, d.x)
}

fn deg(a: f64) -> f64 {
    a * PI / 180.0
}

fn dir_of(a_deg: f64) -> DVec2 {
    DVec2::new(deg(a_deg).cos(), deg(a_deg).sin())
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop2 {
    poly(&[v(x0, y0), v(x1, y0), v(x1, y1), v(x0, y1)])
}

fn poly(p: &[DVec2]) -> Loop2 {
    (0..p.len()).map(|i| Segment::Line { a: p[i], b: p[(i + 1) % p.len()] }).collect()
}

/// Point on the line through `p` and `q` at height `y`.
fn at_y(p: DVec2, q: DVec2, y: f64) -> DVec2 {
    let t = (y - p.y) / (q.y - p.y);
    p + (q - p) * t
}

/// Point on the line through `p` and `q` at `x`.
fn at_x(p: DVec2, q: DVec2, x: f64) -> DVec2 {
    let t = (x - p.x) / (q.x - p.x);
    p + (q - p) * t
}

impl G {
    fn new(m: M) -> G {
        G {
            m,
            strokes: Vec::new(),
            pen: m.w,
            cur: Vec::new(),
            pos: DVec2::ZERO,
            dir: DVec2::X,
            start: DVec2::ZERO,
            fills: Vec::new(),
            cuts: Vec::new(),
            clip: Vec::new(),
        }
    }

    /// Ends the current path (open).
    fn end(&mut self) {
        self.finish(false);
    }

    fn finish(&mut self, closed: bool) {
        if self.cur.is_empty() {
            return;
        }
        let segs = std::mem::take(&mut self.cur);
        match self.strokes.iter_mut().find(|(w, _)| *w == self.pen) {
            Some((_, paths)) => paths.push((segs, closed)),
            None => self.strokes.push((self.pen, vec![(segs, closed)])),
        }
    }

    /// Pen width for the following paths.
    fn width(&mut self, w: f64) -> &mut Self {
        self.end();
        self.pen = w;
        self
    }

    /// Starts a path at `(x, y)`.
    fn mv(&mut self, x: f64, y: f64) -> &mut Self {
        self.end();
        self.pos = v(x, y);
        self.start = self.pos;
        self.dir = DVec2::ZERO;
        self
    }

    /// Direction (degrees) at the start of a path, for a following [`G::arc_to`].
    fn head(&mut self, a: f64) -> &mut Self {
        self.dir = dir_of(a);
        self
    }

    fn ln(&mut self, x: f64, y: f64) -> &mut Self {
        let q = v(x, y);
        if q.distance(self.pos) > 1e-9 {
            self.cur.push(Segment::Line { a: self.pos, b: q });
            self.dir = (q - self.pos).normalize();
            self.pos = q;
        }
        self
    }

    /// Arc to `(x, y)` tangent to the current direction.
    fn arc_to(&mut self, x: f64, y: f64) -> &mut Self {
        let (p, q, t) = (self.pos, v(x, y), self.dir);
        let d = q - p;
        let cross = t.perp_dot(d);
        if d.length() < 1e-9 {
            return self;
        }
        if cross.abs() <= 1e-9 * d.length() && t.dot(d) > 0.0 {
            return self.ln(x, y);
        }
        let n = perp(t);
        let rs = d.length_squared() / (2.0 * n.dot(d));
        let center = p + n * rs;
        let a0 = (p - center).y.atan2((p - center).x);
        let sweep = 2.0 * cross.atan2(t.dot(d));
        self.cur.push(Segment::Arc { center, r: rs.abs(), a0, a1: a0 + sweep });
        self.dir = DAffine2::from_angle(sweep).transform_vector2(t);
        self.pos = q;
        self
    }

    /// Arc around `(cx, cy)` with radius `r` from angle `a0` to `a1` (degrees; `a1 > a0`
    /// counter-clockwise). Continues the path (with a line to the arc's start if needed) or
    /// starts one.
    fn arc(&mut self, cx: f64, cy: f64, r: f64, a0: f64, a1: f64) -> &mut Self {
        let center = v(cx, cy);
        let s = Segment::Arc { center, r, a0: deg(a0), a1: deg(a1) };
        if self.cur.is_empty() && self.pos.distance(s.start()) > 1e-9 {
            self.mv(s.start().x, s.start().y);
        } else {
            self.ln(s.start().x, s.start().y);
        }
        self.dir = s.end_tangent();
        self.pos = s.end();
        self.cur.push(s);
        self
    }

    /// Closes the path (line back to its start if needed).
    fn close(&mut self) -> &mut Self {
        let s = self.start;
        self.ln(s.x, s.y);
        self.finish(true);
        self
    }

    fn line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) -> &mut Self {
        self.mv(x0, y0).ln(x1, y1);
        self.end();
        self
    }

    /// Line through `p` and `q`, extended by `e` beyond both (for a clip to cut its ends).
    fn line_ext(&mut self, p: DVec2, q: DVec2, e0: f64, e1: f64) -> &mut Self {
        let u = (q - p).normalize();
        let (a, b) = (p - u * e0, q + u * e1);
        self.line(a.x, a.y, b.x, b.y)
    }

    fn vline(&mut self, x: f64, y0: f64, y1: f64) -> &mut Self {
        self.line(x, y0, x, y1)
    }

    fn hline(&mut self, y: f64, x0: f64, x1: f64) -> &mut Self {
        self.line(x0, y, x1, y)
    }

    /// Bridge across a stroke: a band of width `bw` centred at `(x, y)`, running along the
    /// direction `a` (degrees) for 1.3 half strokes to each side.
    fn cut(&mut self, x: f64, y: f64, a: f64) -> &mut Self {
        let e = 1.3 * self.m.h.max(self.pen / 2.0);
        self.cut_ext(x, y, a, e)
    }

    fn cut_ext(&mut self, x: f64, y: f64, a: f64, e: f64) -> &mut Self {
        self.end();
        let bw = self.m.bw;
        if bw <= 0.0 {
            return self;
        }
        let (u, n) = (dir_of(a), perp(dir_of(a)));
        let p = v(x, y);
        let h = bw / 2.0;
        self.cuts.push(poly(&[p - u * e - n * h, p + u * e - n * h, p + u * e + n * h, p - u * e + n * h]));
        self
    }

    /// Clips the glyph to the union of the clip rectangles.
    fn clip(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) -> &mut Self {
        self.clip.push(rect(x0, y0, x1, y1));
        self
    }

    /// Clip to the band `y0..y1` (any x).
    fn clip_y(&mut self, y0: f64, y1: f64) -> &mut Self {
        self.clip(-1e4, y0, 1e4, y1)
    }

    /// Square dot of side `s` centred at `(x, y)`.
    fn sq(&mut self, x: f64, y: f64, s: f64) -> &mut Self {
        let h = s / 2.0;
        self.fills.push(rect(x - h, y - h, x + h, y + h));
        self
    }

    fn disc(&mut self, x: f64, y: f64, r: f64) -> &mut Self {
        let c = v(x, y);
        self.fills.push(vec![
            Segment::Arc { center: c, r, a0: 0.0, a1: PI },
            Segment::Arc { center: c, r, a0: PI, a1: 2.0 * PI },
        ]);
        self
    }

    /// Applies `t` (a similarity) to everything drawn so far.
    fn map(&mut self, t: DAffine2) -> &mut Self {
        self.end();
        let ml = |l: &Loop2| l.iter().map(|s| s.map(&t)).collect::<Loop2>();
        for (_, paths) in &mut self.strokes {
            for (segs, _) in paths.iter_mut() {
                *segs = ml(segs);
            }
        }
        self.fills = self.fills.iter().map(ml).collect();
        self.cuts = self.cuts.iter().map(ml).collect();
        self.clip = self.clip.iter().map(ml).collect();
        self
    }

    /// Mirror at the vertical line `x = c`.
    fn mirror(&mut self) -> &mut Self {
        let c = self.m.c;
        self.map(DAffine2::from_cols(DVec2::new(-1.0, 0.0), DVec2::Y, DVec2::new(2.0 * c, 0.0)))
    }

    /// Rotation by 180° about `(x, y)`.
    fn rot180(&mut self, x: f64, y: f64) -> &mut Self {
        self.map(DAffine2::from_cols(DVec2::new(-1.0, 0.0), DVec2::new(0.0, -1.0), DVec2::new(2.0 * x, 2.0 * y)))
    }

    /// Adds the parts of another builder (its clip applies to everything).
    fn merge(&mut self, mut o: G) {
        self.end();
        o.end();
        for (w, paths) in o.strokes {
            match self.strokes.iter_mut().find(|(x, _)| *x == w) {
                Some((_, p)) => p.extend(paths),
                None => self.strokes.push((w, paths)),
            }
        }
        self.fills.extend(o.fills);
        self.cuts.extend(o.cuts);
        self.clip.extend(o.clip);
    }

    /// The glyph's region: strokes and fills, minus bridges, within the clip.
    fn region(mut self) -> Vec<Loop2> {
        self.end();
        let mut ops: Vec<Operand> = Vec::new();
        for (w, paths) in &self.strokes {
            let loops = region2::stroke(paths, *w, Cap::Flat, Join::Miter);
            ops.push(Operand::new(loops, Fill::NonZero));
        }
        for f in &self.fills {
            ops.push(Operand::new(vec![f.clone()], Fill::NonZero));
        }
        let n_ink = ops.len();
        if n_ink == 0 {
            return Vec::new();
        }
        for c in &self.cuts {
            ops.push(Operand::new(vec![c.clone()], Fill::NonZero));
        }
        let n_cut = ops.len();
        let clipped = !self.clip.is_empty();
        if clipped {
            ops.push(Operand::new(self.clip.clone(), Fill::NonZero));
        }
        region2::combine(&ops, &|ins: &[bool]| {
            ins[..n_ink].iter().any(|&x| x) && !ins[n_ink..n_cut].iter().any(|&x| x) && (!clipped || ins[n_cut])
        })
    }
}

// ------------------------------------------------------------------------------------ shapes

/// Closed stadium (rounded rectangle with semicircular ends) on the centre lines
/// `l..r` × `b..t`.
fn stadium(g: &mut G, l: f64, r: f64, b: f64, t: f64) {
    if t - b >= r - l {
        let rr = (r - l) / 2.0;
        g.mv(l, b + rr).head(90.0).ln(l, t - rr).arc_to(r, t - rr).ln(r, b + rr).arc_to(l, b + rr).close();
    } else {
        let rr = (t - b) / 2.0;
        g.mv(r - rr, t).head(180.0).ln(l + rr, t).arc_to(l + rr, b).ln(r - rr, b).arc_to(r - rr, t).close();
    }
}

/// Bowl from a stem at `xs` towards `xf` (left or right): bar along `yt`, rounded corners of
/// radius `rad` (at most half the height), bar back along `yb` to the stem.
fn bowl(g: &mut G, xs: f64, xf: f64, yt: f64, yb: f64, rad: f64) {
    let s = (xf - xs).signum();
    let rad = rad.min((yt - yb) / 2.0).min((xf - xs).abs());
    g.mv(xs, yt).ln(xf - s * rad, yt).arc_to(xf, yt - rad).ln(xf, yb + rad).arc_to(xf - s * rad, yb).ln(xs, yb);
    g.end();
}

/// C-shaped arc on `l..r` × `b..t`: open on the right, terminals at `±ang` degrees.
fn c_shape(g: &mut G, l: f64, r: f64, b: f64, t: f64, ang: f64) {
    let (rr, c) = ((r - l) / 2.0, (l + r) / 2.0);
    g.arc(c, t - rr, rr, ang, 180.0).ln(l, b + rr).arc(c, b + rr, rr, 180.0, 360.0 - ang);
    g.end();
}

/// Two dots above a letter (umlauts) with their lower edge at `y`.
fn dots(g: &mut G, y: f64) {
    let m = g.m;
    let dx = (m.ds / 2.0 + 50.0).max(100.0);
    g.sq(m.c - dx, y + m.ds / 2.0, m.ds).sq(m.c + dx, y + m.ds / 2.0, m.ds);
}

/// Comma shapes (head square on top, tail down to the left) with their top at `ytop`.
fn commas(g: &mut G, xs: &[f64], ytop: f64) {
    let w = g.m.w;
    for &x in xs {
        g.line_ext(v(x + 0.18 * w, ytop), v(x - 0.32 * w, ytop - 2.0 * w), 0.6 * w, 0.0);
    }
    g.clip_y(ytop - 3.0 * w, ytop);
}

/// Tangent point on the circle (`c`, `r`) of the line from `p` (`side` +1: circle on the left
/// of the way from `p`, −1: on the right).
fn tangent_point(p: DVec2, c: DVec2, r: f64, side: f64) -> DVec2 {
    let d = c - p;
    let dist = d.length();
    let a = (r / dist).asin() * side;
    let u = DAffine2::from_angle(-a).transform_vector2(d / dist);
    let len = (dist * dist - r * r).sqrt();
    p + u * len
}

// ------------------------------------------------------------------------------------ glyphs

fn draw(ch: char, g: &mut G) -> bool {
    let M { w, h, bw, x0, x1, y0, cap, xh, asc, desc, l, r, c, b, t, m, x, a, d, rr, wd, ds } = g.m;
    // free ends of stems run to the outer lines; diagonals are clipped there
    match ch {
        ' ' => {}
        'A' => {
            // two legs, apart by the bridge at the cap line (the counter opens at the apex)
            let foot = v(l + 0.25 * h, y0);
            let mut top = v(c - bw / 2.0 - h, cap);
            for _ in 0..4 {
                let cos = (cap - y0) / (top - foot).length();
                top.x = c - bw / 2.0 - h / cos;
            }
            if bw <= 0.0 {
                top.x = c; // without bridges: a closed apex
            }
            g.line_ext(foot, top, w, w).line_ext(v(2.0 * c - foot.x, y0), v(2.0 * c - top.x, cap), w, w);
            let yb = y0 + 0.3 * (cap - y0);
            let (pl, pr) = (at_y(foot, top, yb), at_y(v(2.0 * c - foot.x, y0), v(2.0 * c - top.x, cap), yb));
            g.hline(yb, pl.x, pr.x);
            g.clip_y(y0, cap);
        }
        'Ä' => {
            // the clipped A as a filled region, the dots above its clip
            let mut s = G::new(g.m);
            draw('A', &mut s);
            g.fills.extend(s.region());
            dots(g, cap + 0.08 * CAP);
        }
        'B' => {
            g.vline(l, y0, cap);
            bowl(g, l, r - 0.06 * wd, t, m, (t - m) / 2.0);
            bowl(g, l, r, m, b, (m - b) / 2.0);
            for y in [t, m, b] {
                g.cut(l + h + bw / 2.0, y, 90.0);
            }
        }
        'C' => c_shape(g, l, r, b, t, 35.0),
        'D' => {
            g.vline(l, y0, cap);
            bowl(g, l, r, t, b, 0.9 * rr);
            g.cut(l + h + bw / 2.0, t, 90.0).cut(l + h + bw / 2.0, b, 90.0);
        }
        'E' => {
            g.mv(x1, t).ln(l, t).ln(l, b).ln(x1, b).end();
            g.hline(m, l, x1 - 0.1 * wd);
        }
        'F' => {
            g.mv(x1, t).ln(l, t).ln(l, y0).end();
            g.hline(m, l, x1 - 0.12 * wd);
        }
        'G' => {
            let gy = m - 0.04 * (cap - y0);
            g.arc(c, t - rr, rr, 35.0, 180.0)
                .ln(l, b + rr)
                .arc(c, b + rr, rr, 180.0, 360.0)
                .ln(r, gy)
                .ln(c + 0.05 * wd, gy);
            g.end();
        }
        'H' => {
            g.vline(l, y0, cap).vline(r, y0, cap).hline(m, l, r);
        }
        'I' => {
            g.vline(c, y0, cap).hline(t, c - 0.42 * wd, c + 0.42 * wd).hline(b, c - 0.42 * wd, c + 0.42 * wd);
        }
        'J' => {
            g.mv(c - 0.2 * wd, t).ln(r, t).ln(r, b + rr).arc_to(l, b + rr).end();
        }
        'K' => {
            g.vline(l, y0, cap);
            let (p, q) = (v(l, y0 + 0.3 * (cap - y0)), v(r, t));
            g.line_ext(p, q, 0.0, w);
            let j = at_x(p, q, l + 0.38 * wd);
            g.line_ext(j, v(r, b), 0.0, w);
            g.clip_y(y0, cap);
        }
        'L' => {
            g.mv(l, cap).ln(l, b).ln(x1, b).end();
        }
        'M' => {
            g.vline(l, y0, cap).vline(r, y0, cap);
            g.mv(l, t).ln(c, y0 + 0.36 * (cap - y0)).ln(r, t).end();
        }
        'N' => {
            g.vline(l, y0, cap).vline(r, y0, cap).line(l, t, r, b);
        }
        'O' | 'Ö' => {
            stadium(g, l, r, b, t);
            g.cut(c, t, 90.0).cut(c, b, 90.0);
            if ch == 'Ö' {
                dots(g, cap + 0.08 * CAP);
            }
        }
        'P' | 'R' => {
            let pm = m - 0.06 * (cap - y0);
            g.vline(l, y0, cap);
            bowl(g, l, r, t, pm, (t - pm) / 2.0);
            g.cut(l + h + bw / 2.0, t, 90.0).cut(l + h + bw / 2.0, pm, 90.0);
            if ch == 'R' {
                g.line_ext(v(l + 0.42 * wd, pm), v(r, y0), 0.0, w);
                g.clip_y(y0, cap);
            }
        }
        'Q' => {
            stadium(g, l, r, b, t);
            g.cut(c, t, 90.0).cut(c, b, 90.0);
            g.line(c + 0.1 * wd, b + 0.3 * (t - b), r + 0.05 * wd, y0 - 0.1 * (cap - y0));
        }
        'S' => {
            let (ru, rl) = ((t - m) / 2.0, (m - b) / 2.0);
            g.mv(r - 0.05 * wd, t)
                .ln(l + ru, t)
                .arc_to(l + ru, m)
                .ln(r - rl, m)
                .arc_to(r - rl, b)
                .ln(l + 0.05 * wd, b)
                .end();
        }
        'T' => {
            g.hline(t, x0, x1).vline(c, y0, t);
        }
        'U' | 'Ü' => {
            g.mv(l, cap).ln(l, b + rr).arc_to(r, b + rr).ln(r, cap).end();
            if ch == 'Ü' {
                dots(g, cap + 0.08 * CAP);
            }
        }
        'V' => {
            g.line_ext(v(l, cap), v(c, y0), w, w).line_ext(v(r, cap), v(c, y0), w, w);
            g.clip_y(y0, cap);
        }
        'W' => {
            let (q1, q3, yp) = (l + 0.27 * wd, r - 0.27 * wd, y0 + 0.62 * (cap - y0));
            g.line_ext(v(l, cap), v(q1, y0), w, w).line_ext(v(r, cap), v(q3, y0), w, w);
            let e1 = v(q1, y0) - (v(c, yp) - v(q1, y0)).normalize() * w;
            g.mv(e1.x, e1.y).ln(c, yp).ln(2.0 * c - e1.x, e1.y).end();
            g.clip_y(y0, cap);
        }
        'X' => {
            g.line_ext(v(l, y0), v(r, cap), w, w).line_ext(v(l, cap), v(r, y0), w, w);
            g.clip_y(y0, cap);
        }
        'Y' => {
            let yy = y0 + 0.45 * (cap - y0);
            let e = v(l, cap) + (v(l, cap) - v(c, yy)).normalize() * w;
            g.mv(e.x, e.y).ln(c, yy).ln(c, y0).end();
            g.line_ext(v(r, cap), v(c, yy), w, 0.0);
            g.clip_y(y0, cap);
        }
        'Z' => {
            g.mv(x0, t).ln(r, t).ln(l, b).ln(x1, b).end();
            g.clip(x0, y0, x1, cap);
        }
        // ---------------------------------------------------------------- lower case
        'a' | 'ä' => {
            let ra = 0.4 * wd;
            g.mv(l + 0.1 * wd, x).ln(r - ra, x).arc_to(r, x - ra).ln(r, y0).end();
            let yb = y0 + 0.56 * (xh - y0);
            let rb = (yb - b) / 2.0;
            g.mv(r, yb).ln(l + rb, yb).arc_to(l + rb, b).ln(r, b).end();
            g.cut(r - h - bw / 2.0, yb, 90.0);
            if ch == 'ä' {
                dots(g, xh + 0.12 * CAP);
            }
        }
        'b' => {
            g.vline(l, y0, asc);
            bowl(g, l, r, x, b, rr);
            g.cut(l + h + bw / 2.0, x, 90.0).cut(l + h + bw / 2.0, b, 90.0);
        }
        'c' => c_shape(g, l, r, b, x, 35.0),
        'd' => {
            draw('b', g);
            g.mirror();
        }
        'e' => {
            let ye = y0 + 0.51 * (xh - y0);
            g.mv(l, ye).ln(r, ye).ln(r, x - rr).arc_to(l, x - rr).ln(l, b + rr).arc(c, b + rr, rr, 180.0, 325.0).end();
            g.cut(r - h - bw / 2.0, ye, 90.0);
        }
        'f' => {
            let (fx, rf) = (c - 0.15 * wd, 0.35 * wd);
            g.mv(fx, y0).ln(fx, a - rf).arc_to(fx + rf, a).ln(r, a).end();
            g.hline(x, l, r - 0.1 * wd);
        }
        'g' => {
            let rg = 0.4 * wd;
            g.mv(r, xh).ln(r, d + rg).arc_to(r - rg, d).ln(l + 0.1 * wd, d).end();
            bowl(g, r, l, x, b, 0.4 * wd);
            g.cut(r - h - bw / 2.0, x, 90.0).cut(r - h - bw / 2.0, b, 90.0);
        }
        'h' => {
            let ra = 0.4 * wd;
            g.vline(l, y0, asc);
            g.mv(l, x).ln(r - ra, x).arc_to(r, x - ra).ln(r, y0).end();
        }
        'i' => {
            g.mv(c - 0.32 * wd, x).ln(c, x).ln(c, b).end();
            g.hline(b, c - 0.42 * wd, c + 0.42 * wd);
            g.sq(c, xh + 0.12 * CAP + ds / 2.0, ds);
        }
        'j' => {
            let (sx, rj) = (c + 0.12 * wd, 0.32 * wd);
            g.mv(c - 0.25 * wd, x).ln(sx, x).ln(sx, d + rj).arc_to(sx - rj, d).ln(l, d).end();
            g.sq(sx, xh + 0.12 * CAP + ds / 2.0, ds);
        }
        'k' => {
            g.vline(l, y0, asc);
            let (p, q) = (v(l, y0 + 0.33 * (xh - y0)), v(r, x));
            g.line_ext(p, q, 0.0, w);
            let j = at_x(p, q, l + 0.38 * wd);
            g.line_ext(j, v(r, b), 0.0, w);
            g.clip(-1e4, y0, l + h, asc).clip(l + h, y0, 1e4, xh);
        }
        'l' => {
            let (sx, rl) = (c - 0.1 * wd, 0.35 * wd);
            g.mv(l + 0.1 * wd, a).ln(sx, a).ln(sx, b + rl).arc_to(sx + rl, b).ln(r, b).end();
        }
        'm' => {
            let rm = (c - l) / 2.0;
            g.vline(l, y0, xh);
            g.mv(l, x).ln(c - rm, x).arc_to(c, x - rm).ln(c, y0).end();
            g.mv(c, x - rm).head(90.0).arc_to(c + rm, x).ln(r - rm, x).arc_to(r, x - rm).ln(r, y0).end();
        }
        'n' => {
            let ra = 0.4 * wd;
            g.vline(l, y0, xh);
            g.mv(l, x).ln(r - ra, x).arc_to(r, x - ra).ln(r, y0).end();
        }
        'o' | 'ö' => {
            stadium(g, l, r, b, x);
            g.cut(c, x, 90.0).cut(c, b, 90.0);
            if ch == 'ö' {
                dots(g, xh + 0.12 * CAP);
            }
        }
        'p' => {
            g.vline(l, desc, xh);
            bowl(g, l, r, x, b, rr);
            g.cut(l + h + bw / 2.0, x, 90.0).cut(l + h + bw / 2.0, b, 90.0);
        }
        'q' => {
            draw('p', g);
            g.mirror();
        }
        'r' => {
            let ra = 0.4 * wd;
            g.vline(l, y0, xh);
            g.mv(l, x - ra).head(90.0).arc_to(l + ra, x).ln(r - 0.05 * wd, x).end();
        }
        's' => {
            let ms = (x + b) / 2.0;
            let (ru, rl) = ((x - ms) / 2.0, (ms - b) / 2.0);
            g.mv(r - 0.05 * wd, x)
                .ln(l + ru, x)
                .arc_to(l + ru, ms)
                .ln(r - rl, ms)
                .arc_to(r - rl, b)
                .ln(l + 0.05 * wd, b)
                .end();
        }
        't' => {
            let (tx, rt) = (c - 0.15 * wd, 0.35 * wd);
            g.mv(tx, y0 + 0.86 * (asc - y0)).ln(tx, b + rt).arc_to(tx + rt, b).ln(r, b).end();
            g.hline(x, l, r - 0.05 * wd);
        }
        'u' | 'ü' => {
            let ru = 0.4 * wd;
            g.vline(r, y0, xh);
            g.mv(l, xh).ln(l, b + ru).arc_to(l + ru, b).ln(r, b).end();
            if ch == 'ü' {
                dots(g, xh + 0.12 * CAP);
            }
        }
        'v' => {
            g.line_ext(v(l, xh), v(c, y0), w, w).line_ext(v(r, xh), v(c, y0), w, w);
            g.clip_y(y0, xh);
        }
        'w' => {
            let (q1, q3, yp) = (l + 0.27 * wd, r - 0.27 * wd, y0 + 0.62 * (xh - y0));
            g.line_ext(v(l, xh), v(q1, y0), w, w).line_ext(v(r, xh), v(q3, y0), w, w);
            let e1 = v(q1, y0) - (v(c, yp) - v(q1, y0)).normalize() * w;
            g.mv(e1.x, e1.y).ln(c, yp).ln(2.0 * c - e1.x, e1.y).end();
            g.clip_y(y0, xh);
        }
        'x' => {
            g.line_ext(v(l, y0), v(r, xh), w, w).line_ext(v(l, xh), v(r, y0), w, w);
            g.clip_y(y0, xh);
        }
        'y' => {
            let (top_r, mid) = (v(r, xh), v(c, y0));
            let bot = at_y(top_r, mid, desc);
            g.line_ext(top_r, bot, w, w);
            g.line_ext(v(l, xh), mid, w, 0.0);
            g.clip_y(desc, xh);
        }
        'z' => {
            g.mv(x0, x).ln(r, x).ln(l, b).ln(x1, b).end();
            g.clip(x0, y0, x1, xh);
        }
        'ß' => {
            let r1 = 0.36 * wd;
            let mb = y0 + 0.55 * (cap - y0);
            let rl = (mb - b) / 2.0;
            let gap = (l + h + bw.max(0.5 * w)).min(r - (mb - b) / 2.0 - 0.1 * wd);
            let xw = (l + r1 + 0.1 * wd).max(gap + 0.3 * w).min(r - rl - 0.05 * wd + 0.3 * w);
            g.mv(l, y0).ln(l, a - r1).arc_to(l + 2.0 * r1, a - r1).arc_to(xw, mb).end();
            g.mv(xw - 0.3 * w, mb).ln(r - rl, mb).arc_to(r - rl, b).ln(gap, b).end();
        }
        'ẞ' => {
            let rs = 0.3 * wd;
            let mb = y0 + 0.55 * (cap - y0);
            let rl = (mb - b) / 2.0;
            let gap = (l + h + bw.max(0.5 * w)).min(r - (mb - b) / 2.0 - 0.1 * wd);
            let xd = c - 0.05 * wd;
            g.mv(l, y0).ln(l, t - rs).arc_to(l + rs, t).ln(r, t).ln(xd, mb).end();
            g.mv((xd - 0.4 * w).max(gap).min(r - rl - 0.05 * wd), mb).ln(r - rl, mb).arc_to(r - rl, b).ln(gap, b).end();
            g.clip(x0 - w, y0, x1, cap);
        }
        // ---------------------------------------------------------------- digits
        '0' => {
            let inset = 0.06 * wd;
            stadium(g, l + inset, r - inset, b, t);
            g.cut(c, t, 90.0).cut(c, b, 90.0);
            let s = ds.min(0.45 * (wd - 2.0 * inset - w));
            g.sq(c, m, s);
        }
        '1' => {
            let sx = c + 0.05 * wd;
            g.mv(l + 0.12 * wd, y0 + 0.68 * (cap - y0)).ln(sx, t).ln(sx, b).end();
            g.hline(b, l + 0.1 * wd, r - 0.1 * wd);
            g.clip(x0, y0, x1, cap);
        }
        '2' => {
            let r2 = 0.95 * rr;
            g.arc(c, t - r2, r2, 180.0, -38.0).ln(l, b).ln(x1, b).end();
            g.clip(x0, y0, x1, cap);
        }
        '3' => {
            let ru = (t - m) / 2.0;
            let rl = (m - b) / 2.0;
            let xm = c - 0.1 * wd;
            g.mv(l + 0.05 * wd, t).ln(r - 0.06 * wd - ru, t).arc_to(r - 0.06 * wd - ru, m).ln(xm, m).end();
            g.mv(xm, m).ln(r - rl, m).arc_to(r - rl, b).ln(l + 0.05 * wd, b).end();
        }
        '4' => {
            let x4 = r - 0.2 * wd;
            let y4 = y0 + 0.28 * (cap - y0);
            let top = v(x4, cap);
            let e = top + (top - v(l, y4)).normalize() * w;
            g.mv(e.x, e.y).ln(l, y4).ln(x1, y4).end();
            g.vline(x4, y0, cap);
            g.cut(x4 - h - bw / 2.0, y4, 90.0);
            g.clip(x0, y0, x1, cap);
        }
        '5' => {
            let y5 = y0 + 0.56 * (cap - y0);
            let r5 = (y5 - b) / 2.0;
            let lx = l + 0.04 * wd;
            g.mv(r - 0.02 * wd, t).ln(lx, t).ln(lx, y5).ln(r - r5, y5).arc_to(r - r5, b).ln(l, b).end();
        }
        '6' | '9' => {
            stadium(g, l, r, b, b + wd);
            g.mv(l, b + rr).head(90.0).ln(l, t - rr).arc(c, t - rr, rr, 180.0, 35.0).end();
            let p = v(c, b + rr) + dir_of(50.0) * rr;
            g.cut(p.x, p.y, 50.0);
            if ch == '9' {
                g.rot180(c, m);
            }
        }
        '7' => {
            let foot = v(c - 0.15 * wd, y0);
            let e = foot + (foot - v(r, t)).normalize() * w;
            g.mv(x0, t).ln(r, t).ln(e.x, e.y).end();
            g.clip(x0, y0, x1, cap);
        }
        '8' => {
            let hh = t - b;
            let (rl, ru) = (0.275 * hh, 0.225 * hh);
            stadium(g, c - rl, c + rl, b, b + 2.0 * rl);
            stadium(g, c - ru, c + ru, t - 2.0 * ru, t);
            g.cut(c, t, 90.0).cut(c, b, 90.0);
        }
        // ---------------------------------------------------------------- punctuation
        '.' => {
            g.sq(c, y0 + ds / 2.0, ds);
        }
        ',' => commas(g, &[c], y0 + w),
        ':' => {
            g.sq(c, y0 + ds / 2.0, ds).sq(c, xh - ds / 2.0, ds);
        }
        ';' => {
            g.sq(c, xh - ds / 2.0, ds);
            commas(g, &[c], y0 + w);
            g.clip.clear();
            g.clip_y(y0 - 3.0 * w, y0 + w).clip(c - w, xh - ds, c + w, xh);
        }
        '!' => {
            g.vline(c, y0 + 0.3 * (cap - y0), cap).sq(c, y0 + ds / 2.0, ds);
        }
        '?' => {
            let rq = 0.4 * wd;
            g.mv(c - rq, t - rq)
                .head(90.0)
                .arc_to(c + rq, t - rq)
                .arc_to(c, y0 + 0.42 * (cap - y0))
                .ln(c, y0 + 0.3 * (cap - y0))
                .end();
            g.sq(c, y0 + ds / 2.0, ds);
        }
        '\'' => {
            g.vline(c, cap - 0.3 * CAP, cap);
        }
        '"' => {
            g.vline(c - 0.26 * wd, cap - 0.3 * CAP, cap).vline(c + 0.26 * wd, cap - 0.3 * CAP, cap);
        }
        '„' | '“' | '”' => {
            let ytop = if ch == '„' { y0 + w } else { cap };
            commas(g, &[c - 0.26 * wd, c + 0.26 * wd], ytop);
            if ch == '“' {
                g.rot180(c, cap - 1.05 * w);
            }
        }
        '‚' | '‘' | '’' => {
            let ytop = if ch == '‚' { y0 + w } else { cap };
            commas(g, &[c], ytop);
            if ch == '‘' {
                g.rot180(c, cap - 1.05 * w);
            }
        }
        '«' | '»' => {
            let (dx, dy) = (0.24 * wd, 0.29 * (xh - y0));
            let th = (dy / dx).atan();
            let off = (w + (0.45 * w).max(0.5 * bw)) / th.sin();
            let yc = y0 + 0.5 * (xh - y0);
            let x0c = c - (off + dx) / 2.0;
            for xt in [x0c, x0c + off] {
                g.mv(xt + dx, yc + dy).ln(xt, yc).ln(xt + dx, yc - dy).end();
            }
            if ch == '»' {
                g.mirror();
            }
        }
        '(' | ')' => {
            let (top, bot) = (asc, y0 - 0.21 * CAP);
            let (xe, xb) = (c + 0.3 * wd, c - 0.25 * wd);
            let (hc, s) = ((top - bot) / 2.0, xe - xb);
            let rad = (hc * hc + s * s) / (2.0 * s);
            let (cx, cy) = (xb + rad, (top + bot) / 2.0);
            let a0 = (hc.atan2(xe - cx)) * 180.0 / PI + 8.0;
            g.arc(cx, cy, rad, a0, 360.0 - a0).end();
            g.clip_y(bot, top);
            if ch == ')' {
                g.mirror();
            }
        }
        '[' | ']' => {
            let (top, bot) = (asc - h, y0 - 0.21 * CAP + h);
            let (xb, xe) = (c - 0.2 * wd, c + 0.32 * wd);
            g.mv(xe, top).ln(xb, top).ln(xb, bot).ln(xe, bot).end();
            if ch == ']' {
                g.mirror();
            }
        }
        '{' | '}' => {
            let (top, bot) = (asc - h, y0 - 0.21 * CAP + h);
            let ym = (top + bot) / 2.0;
            let (xs, rb, xe) = (c - 0.02 * wd, 0.28 * wd, c + 0.35 * wd);
            g.mv(xe, top).ln(xs + rb, top).arc_to(xs, top - rb).ln(xs, ym + rb).arc_to(xs - rb, ym).end();
            g.mv(xe, bot).ln(xs + rb, bot).arc_to(xs, bot + rb).ln(xs, ym - rb).arc_to(xs - rb, ym).end();
            if ch == '}' {
                g.mirror();
            }
        }
        '-' => {
            g.hline(y0 + 0.4 * (cap - y0), c - 0.36 * wd, c + 0.36 * wd);
        }
        '–' => {
            g.hline(y0 + 0.4 * (cap - y0), x0, x1);
        }
        '—' => {
            g.hline(y0 + 0.4 * (cap - y0), 0.0, ADVANCE);
        }
        '_' => {
            g.hline(y0 - 0.12 * CAP, 0.0, ADVANCE);
        }
        '/' | '\\' => {
            let (bot, top) = (y0 - 0.15 * CAP, asc);
            g.line_ext(v(l, bot), v(r, top), w, w).clip_y(bot, top);
            if ch == '\\' {
                g.mirror();
            }
        }
        '&' => {
            let ru = 0.3 * wd;
            let u = v(c - 0.12 * wd, t - ru);
            stadium(g, u.x - ru, u.x + ru, u.y - ru, t);
            let rl = 0.42 * wd;
            let lc = v(c - 0.05 * wd, b + rl);
            let s = lc + dir_of(135.0) * rl;
            // connector from the bottom of the upper loop down to the bowl, tangent to it
            let dirv = dir_of(45.0);
            let pu = {
                let k = s - u;
                let bq = k.dot(dirv);
                let cq = k.length_squared() - ru * ru;
                let tq = -bq - (bq * bq - cq).max(0.0).sqrt();
                s + dirv * tq.max(0.0)
            };
            g.mv(pu.x, pu.y)
                .ln(s.x, s.y)
                .arc(lc.x, lc.y, rl, 135.0, 360.0)
                .ln(r + 0.02 * wd, y0 + 0.44 * (cap - y0))
                .end();
            let foot = v(r, y0);
            let tp = tangent_point(foot, u, ru, 1.0);
            g.line_ext(foot, tp, w, 0.0);
            let pt = u + dir_of(160.0) * ru;
            g.cut(pt.x, pt.y, 160.0);
            let pc = lc + dir_of(225.0) * rl;
            g.cut(pc.x, pc.y, 225.0);
            g.clip_y(y0, cap);
        }
        '%' => {
            let rp = (0.2 * wd).max(0.75 * w);
            stadium(g, l, l + 2.0 * rp, t - 2.0 * rp, t);
            stadium(g, r - 2.0 * rp, r, b, b + 2.0 * rp);
            g.cut(l + rp, t, 90.0).cut(r - rp, b, 90.0);
            g.line_ext(v(l, y0), v(r, cap), w, w).clip_y(y0, cap);
        }
        '#' => {
            let (ya, yb) = (y0 + 0.32 * (cap - y0), y0 + 0.66 * (cap - y0));
            g.vline(c - 0.24 * wd, y0, cap).vline(c + 0.24 * wd, y0, cap);
            g.hline(ya, x0, x1).hline(yb, x0, x1);
            g.cut(c, yb, 90.0);
        }
        '+' => {
            let (yc, s) = (y0 + 0.45 * (cap - y0), 0.42 * wd);
            g.hline(yc, c - s, c + s).vline(c, yc - s, yc + s);
        }
        '*' => {
            let (yc, s) = (y0 + 0.62 * (cap - y0), 0.4 * wd);
            for a in [90.0, 30.0, 150.0] {
                let u = dir_of(a) * s;
                g.line(c - u.x, yc - u.y, c + u.x, yc + u.y);
            }
        }
        '=' => {
            let yc = y0 + 0.45 * (cap - y0);
            let dy = 0.15 * CAP;
            g.hline(yc - dy, c - 0.42 * wd, c + 0.42 * wd).hline(yc + dy, c - 0.42 * wd, c + 0.42 * wd);
        }
        '<' | '>' => {
            let (yc, dy) = (y0 + 0.45 * (cap - y0), 0.32 * CAP);
            g.mv(r - 0.05 * wd, yc + dy).ln(l + 0.1 * wd, yc).ln(r - 0.05 * wd, yc - dy).end();
            if ch == '>' {
                g.mirror();
            }
        }
        '°' => {
            let rd = 0.25 * wd;
            stadium(g, c - rd, c + rd, t - 2.0 * rd, t);
            g.cut(c, t - 2.0 * rd, 90.0);
        }
        '²' | '³' => {
            let hs = 0.53 * CAP;
            let ws = 0.67 * hs;
            let wsup = 0.75 * w;
            let mut s = G::new(M::new(wsup, bw * 0.75, c - ws / 2.0, c + ws / 2.0, cap - hs, cap));
            draw(if ch == '²' { '2' } else { '3' }, &mut s);
            g.merge(s);
        }
        '€' => {
            let lx = l + 0.18 * wd;
            c_shape(g, lx, r, b, t, 40.0);
            let dy = 0.5 * w + (0.35 * w).max(30.0);
            g.hline(m + dy, x0, c + 0.12 * wd).hline(m - dy, x0, c + 0.12 * wd);
        }
        '@' => {
            let wa = (0.72 * w).min(0.11 * CAP);
            g.width(wa);
            let ha = wa / 2.0;
            let (la, ra) = (x0 + ha, x1 - ha);
            let o = v(c, y0 + 0.42 * (cap - y0));
            let ro = (ra - la) / 2.0;
            let ri = 0.24 * wd;
            let ic = v(c - 0.06 * wd, o.y);
            let xs = ic.x + ri;
            stadium(g, ic.x - ri, ic.x + ri, ic.y - ri, ic.y + ri);
            // curl from the stem into the outer ring: circle tangent to the stem and inside the ring
            let rk = (ra - xs) / 2.0 * 0.95;
            let kx = xs + rk;
            let ky = o.y - ((ro - rk).powi(2) - (kx - o.x).powi(2)).max(0.0).sqrt();
            let th = (ky - o.y).atan2(kx - o.x) * 180.0 / PI;
            g.mv(xs, ic.y + ri).ln(xs, ky).arc(kx, ky, rk, 180.0, 360.0 + th).arc(o.x, o.y, ro, th, 280.0).end();
            g.cut_ext(ic.x - ri, ic.y, 0.0, 1.3 * ha);
        }
        '§' => {
            let rs = 0.3 * wd;
            let rh = 0.28 * wd;
            let ys = y0 + 0.42 * (cap - y0);
            stadium(g, c - rs, c + rs, ys - rs, ys + rs);
            g.arc(c, ys + rs + rh, rh, 20.0, 270.0).end();
            g.arc(c, ys - rs - rh, rh, 200.0, 450.0).end();
            g.cut(c - rs, ys, 0.0).cut(c + rs, ys, 0.0);
        }
        '~' => {
            let (rt, yt) = (0.2 * wd, y0 + 0.45 * (cap - y0));
            g.arc(c - rt, yt, rt, 180.0, 0.0).arc(c + rt, yt, rt, 180.0, 360.0).end();
        }
        '^' => {
            let p = v(c - 0.38 * wd, y0 + 0.6 * (cap - y0));
            g.mv(p.x, p.y).ln(c, t).ln(2.0 * c - p.x, p.y).end();
            g.clip_y(y0, cap);
        }
        '|' => {
            g.vline(c, y0 - 0.21 * CAP, asc);
        }
        '·' => {
            g.sq(c, y0 + 0.45 * (cap - y0), ds);
        }
        '•' => {
            g.disc(c, y0 + 0.45 * (cap - y0), (0.28 * wd).max(0.75 * w));
        }
        '…' => {
            for dx in [-0.42 * wd, 0.0, 0.42 * wd] {
                g.sq(c + dx, y0 + ds / 2.0, ds);
            }
        }
        '±' => {
            let (yc, s) = (y0 + 0.52 * (cap - y0), 0.36 * wd);
            g.hline(yc, c - s, c + s).vline(c, yc - s, yc + s);
            g.hline(b, c - s, c + s);
        }
        '×' => {
            let yc = y0 + 0.45 * (cap - y0);
            let s = 0.3 * wd;
            g.line(c - s, yc - s, c + s, yc + s).line(c - s, yc + s, c + s, yc - s);
        }
        '÷' => {
            let yc = y0 + 0.45 * (cap - y0);
            g.hline(yc, c - 0.4 * wd, c + 0.4 * wd);
            let dy = w + 0.5 * w.max(ds);
            g.sq(c, yc + dy, ds).sq(c, yc - dy, ds);
        }
        'µ' => {
            let ru = 0.4 * wd;
            g.vline(l, desc, xh).vline(r, y0, xh);
            g.mv(l, b + ru).head(-90.0).arc_to(l + ru, b).ln(r, b).end();
        }
        'Ø' | 'ø' => {
            let top = if ch == 'Ø' { t } else { x };
            stadium(g, l, r, b, top);
            g.cut(c, top, 90.0).cut(c, b, 90.0);
            let e = 0.06 * (top - b);
            g.line(l - 0.05 * wd, b - h - e, r + 0.05 * wd, top + h + e);
        }
        '$' => {
            draw('S', g);
            g.vline(c, t, cap + 0.1 * CAP).vline(c, y0 - 0.1 * CAP, b);
        }
        _ => return false,
    }
    let _ = (x0, d);
    true
}

// ------------------------------------------------------------------------------------ cache

type Key = (char, u64, u64);

thread_local! {
    static GLYPHS: RefCell<HashMap<Key, Rc<Vec<Contour>>>> = RefCell::new(HashMap::new());
}

/// Outline of `ch` in font units for stroke width `stroke` and bridge width `bridge` (units),
/// cached: closed contours of lines and arcs, outer ones counter-clockwise; `None` if the font
/// lacks the character.
pub fn render(ch: char, stroke: f64, bridge: f64) -> Option<Rc<Vec<Contour>>> {
    if !has(ch) {
        return None;
    }
    let key = (ch, stroke.to_bits(), bridge.to_bits());
    if let Some(c) = GLYPHS.with(|g| g.borrow().get(&key).cloned()) {
        return Some(c);
    }
    let mut g = G::new(M::regular(stroke, bridge.max(0.0)));
    draw(ch, &mut g);
    let loops = g.region();
    let contours: Vec<Contour> = loops
        .iter()
        .map(|lp| Contour {
            segs: lp
                .iter()
                .map(|s| match *s {
                    Segment::Line { a, b } => Seg::Line { a, b },
                    Segment::Arc { center, r, a0, a1 } => Seg::Arc { center, r, a0, a1 },
                    Segment::Nurbs(_) => unreachable!("stencil glyphs are lines and arcs"),
                })
                .collect(),
            closed: true,
        })
        .collect();
    let c = Rc::new(contours);
    GLYPHS.with(|g| {
        let mut g = g.borrow_mut();
        if g.len() > 4096 {
            g.clear();
        }
        g.insert(key, c.clone());
    });
    Some(c)
}

/// The exact region of a glyph (loops of lines and arcs, holes clockwise) in font units.
pub fn region(ch: char, stroke: f64, bridge: f64) -> Option<Vec<Loop2>> {
    if !has(ch) {
        return None;
    }
    let mut g = G::new(M::regular(stroke, bridge.max(0.0)));
    draw(ch, &mut g);
    Some(g.region())
}
