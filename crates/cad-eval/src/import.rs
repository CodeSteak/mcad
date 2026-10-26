//! `import("datei.svg", …)` (SVG subset, Plan 17.5) and `text(…)` (Plan 17.4): both give
//! `Profile::Sketch` nodes of exact contours – one per filled SVG element or glyph, under a
//! group when there are several (their union, as SVG paints and as glyphs may overlap).
//!
//! Data files come from [`crate::Project::assets`] (loaded with the project, so a changed SVG
//! or font changes the evaluation). Parsed SVGs are cached by content and options.

use cad_i18n::{tr, trs};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use cad_ir::{Contour, Op, Profile, Seg};
use cad_syntax::Span;
use cad_syntax::ast::Stmt;
use glam::DMat4;

use crate::env::Dyn;
use crate::eval::{ArgV, Args, Ctx, Ev};
use crate::value::{Value, fmt_num};

/// Exact IR segment of an outline segment.
fn ir_seg(s: &cad_outline::Seg) -> Seg {
    match *s {
        cad_outline::Seg::Line { a, b } => Seg::Line { a, b },
        cad_outline::Seg::Arc { center, r, a0, a1 } => Seg::Arc { center, r, a0, a1, ccw: a1 > a0 },
        cad_outline::Seg::Quad(p) => Seg::Bezier { points: p.to_vec() },
        cad_outline::Seg::Cubic(p) => Seg::Bezier { points: p.to_vec() },
        cad_outline::Seg::Ellipse { center, u, v, t0, t1 } => {
            // the unit circle arc mapped by (u, v): an exact rational quadratic B-spline
            let m = glam::DAffine2::from_cols(u, v, center);
            match (cad_kernel::Segment::Arc { center: glam::DVec2::ZERO, r: 1.0, a0: t0, a1: t1 }).map(&m) {
                cad_kernel::Segment::Arc { center, r, a0, a1 } => Seg::Arc { center, r, a0, a1, ccw: a1 > a0 },
                cad_kernel::Segment::Line { a, b } => Seg::Line { a, b },
                cad_kernel::Segment::Nurbs(c) => Seg::Nurbs(c),
            }
        }
    }
}

/// Bézier control points of an outline segment (`None` for lines and arcs).
fn bezier_points(s: &cad_outline::Seg) -> Option<&[glam::DVec2]> {
    match s {
        cad_outline::Seg::Quad(p) => Some(p),
        cad_outline::Seg::Cubic(p) => Some(p),
        _ => None,
    }
}

/// Tangent-continuous joint of two Bézier curves of the same degree: the ratio of the
/// parameter lengths that makes the joint C1 (`None` at a corner).
fn smooth_ratio(a: &[glam::DVec2], b: &[glam::DVec2]) -> Option<f64> {
    if a.len() != b.len() {
        return None;
    }
    let ta = a[a.len() - 1] - a[a.len() - 2];
    let tb = b[1] - b[0];
    let (la, lb) = (ta.length(), tb.length());
    if la <= 1e-12 || lb <= 1e-12 || ta.dot(tb) <= 0.0 || ta.perp_dot(tb).abs() > 1e-9 * la * lb {
        return None;
    }
    Some(lb / la)
}

/// One clamped B-spline through a chain of C1-joinable Bézier curves of degree `p`: interior
/// knots of multiplicity `p - 1` with parameter lengths chosen for C1 (the joint control
/// points drop out; exact up to the collinearity tolerance of [`smooth_ratio`]).
fn chain_nurbs(chain: &[&[glam::DVec2]]) -> Seg {
    let p = chain[0].len() - 1;
    let mut knots = vec![0.0; p + 1];
    let mut points: Vec<glam::DVec2> = chain[0][..p].to_vec();
    let (mut t, mut d) = (0.0, 1.0);
    for (i, b) in chain.iter().enumerate() {
        if i > 0 {
            d *= smooth_ratio(chain[i - 1], b).unwrap_or(1.0);
            points.extend_from_slice(&b[1..p]);
        }
        t += d;
        if i + 1 < chain.len() {
            knots.extend(std::iter::repeat_n(t, p - 1));
        }
    }
    points.push(chain[chain.len() - 1][p]);
    knots.extend(std::iter::repeat_n(t, p + 1));
    for k in &mut knots {
        *k /= t;
    }
    Seg::Nurbs(cad_kernel::brep::BSplineCurve {
        degree: p as u32,
        knots,
        points: points.iter().map(|q| q.extend(0.0)).collect(),
        weights: None,
    })
}

/// IR segments of a contour: tangent-continuous runs of Bézier curves become one B-spline
/// each (a glyph side is one kernel face instead of dozens), the rest maps one to one.
fn ir_segs(c: &cad_outline::Contour) -> Vec<Seg> {
    let mut segs = c.segs.clone();
    let n = segs.len();
    // closed contour: start at a corner so that no smooth run is cut at the start
    if c.closed && n > 1 {
        let corner = (0..n).find(|&i| {
            let prev = &segs[(i + n - 1) % n];
            match (bezier_points(prev), bezier_points(&segs[i])) {
                (Some(a), Some(b)) => smooth_ratio(a, b).is_none(),
                _ => true,
            }
        });
        if let Some(k) = corner {
            segs.rotate_left(k);
        }
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < segs.len() {
        let Some(first) = bezier_points(&segs[i]) else {
            out.push(ir_seg(&segs[i]));
            i += 1;
            continue;
        };
        let mut chain = vec![first];
        while let Some(next) = segs.get(i + chain.len()).and_then(bezier_points) {
            if smooth_ratio(chain[chain.len() - 1], next).is_none() {
                break;
            }
            chain.push(next);
        }
        i += chain.len();
        if chain.len() == 1 {
            out.push(Seg::Bezier { points: first.to_vec() });
        } else if out.is_empty() && i >= segs.len() && c.closed {
            // a closed contour of one smooth run: two B-splines (a loop needs two edges)
            let h = chain.len() / 2;
            out.push(chain_nurbs(&chain[..h]));
            out.push(chain_nurbs(&chain[h..]));
        } else {
            out.push(chain_nurbs(&chain));
        }
    }
    out
}

/// IR contours of outline contours.
pub(crate) fn ir_contours(cs: &[cad_outline::Contour]) -> Vec<Contour> {
    cs.iter()
        .filter(|c| !c.segs.is_empty())
        .map(|c| Contour { hole: false, closed: c.closed, segs: ir_segs(c) })
        .collect()
}

thread_local! {
    /// Parsed SVGs by (content, options) hash.
    static SVGS: RefCell<HashMap<u64, Rc<Result<cad_svg::Svg, String>>>> = RefCell::new(HashMap::new());
}

fn parse_svg(data: &[u8], o: &cad_svg::Options) -> Rc<Result<cad_svg::Svg, String>> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    o.dpi.to_bits().hash(&mut h);
    o.scale.to_bits().hash(&mut h);
    o.center.hash(&mut h);
    o.id.hash(&mut h);
    let key = h.finish();
    if let Some(r) = SVGS.with(|c| c.borrow().get(&key).cloned()) {
        return r;
    }
    let r = Rc::new(match std::str::from_utf8(data) {
        Ok(t) => cad_svg::parse(t, o),
        Err(_) => Err(trs!("SVG ist kein UTF-8-Text", "SVG is not UTF-8 text").into()),
    });
    SVGS.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 64 {
            c.clear();
        }
        c.insert(key, r.clone());
    });
    r
}

impl<'a> Ev<'a> {
    fn str_arg(&mut self, a: Option<&ArgV<'a>>, what: &str) -> Option<String> {
        let a = a?;
        match &a.val.v {
            Value::Str(s) => Some(s.to_string()),
            Value::Undef => None,
            _ => {
                self.warn(a.expr.span, tr!("`{what}` muss ein Text sein", "`{what}` must be a text"));
                None
            }
        }
    }

    fn pos_num(&mut self, a: Option<&ArgV<'a>>, what: &str, default: f64) -> f64 {
        let Some(x) = a else { return default };
        match x.val.as_num() {
            Some(v) if v.is_finite() && v > 0.0 => v,
            Some(_) | None if matches!(x.val.v, Value::Undef) => default,
            _ => {
                self.warn(
                    x.expr.span,
                    tr!("`{what}` muss eine positive Zahl sein", "`{what}` must be a positive number"),
                );
                default
            }
        }
    }

    /// Profiles as nodes of `s`: one directly, several under a group (their union).
    fn profiles(&mut self, s: &'a Stmt, profiles: Vec<Profile>, ctx: &Ctx<'a>, dy: &Dyn<'a>) {
        if profiles.len() == 1 {
            let p = profiles.into_iter().next().unwrap();
            self.profile(s, p, ctx, dy);
            return;
        }
        let g = self.node(s, Op::Group, ctx);
        let sub = self.sub(ctx, g, &DMat4::IDENTITY);
        for (i, p) in profiles.into_iter().enumerate() {
            let mut c = sub.clone();
            c.key = crate::mix(sub.key, i as u64 + 1) & crate::MASK53;
            self.profile(s, p, &c, dy);
        }
    }

    /// The data file `rel` named at `span`, or an error there.
    pub(crate) fn asset_data(&mut self, span: Span, rel: &str) -> Option<(String, Arc<[u8]>)> {
        match self.proj.asset(span, rel) {
            Some((path, a)) => match &a.0 {
                Ok(b) => Some((path, b.clone())),
                Err(e) => {
                    self.error(span, tr!("Datei `{path}` nicht gefunden ({e})", "file `{path}` not found ({e})"));
                    None
                }
            },
            None => {
                self.error(
                    span,
                    tr!("Datei `{rel}` nicht geladen: Dateinamen werden erkannt, wenn sie als Text im Projekt stehen \
                         (z. B. `import(\"{rel}\")`)", "file `{rel}` not loaded: file names are recognised when they appear as text in the project (e.g. `import(\"{rel}\")`)"
                    ),
                );
                None
            }
        }
    }

    /// `import(file, id, scale, dpi, center)`: SVG files as 2D profiles; STL files as meshes
    /// or B-reps (`import_stl.rs`, also `as`).
    #[inline(never)]
    pub(crate) fn m_import(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>, dy: &Dyn<'a>) {
        let p = self.bind(
            "import",
            a,
            &["file", "id", "scale", "dpi", "center", "convexity", "layer", "origin", "width", "height", "as"],
            1,
        );
        let Some(file) = self.str_arg(p[0], "file") else {
            self.error(s.span, trs!("import: Dateiname fehlt", "import: file name missing"));
            return;
        };
        let fspan = p[0].map_or(s.span, |x| x.expr.span);
        for (i, n) in [(6, "layer"), (7, "origin"), (8, "width"), (9, "height")] {
            if let Some(x) = p[i] {
                self.warn(
                    x.expr.span,
                    tr!(
                        "import: `{n}` gilt nur für DXF und wird ignoriert",
                        "import: `{n}` applies to DXF only and is ignored"
                    ),
                );
            }
        }
        if file.to_ascii_lowercase().ends_with(".stl") {
            self.import_stl(s, &p, fspan, &file, ctx, dy);
            return;
        }
        if !file.to_ascii_lowercase().ends_with(".svg") {
            let ext = file.rsplit('.').next().unwrap_or("");
            self.error(
                fspan,
                tr!(
                    "import: nur SVG und STL werden unterstützt (nicht `.{ext}`)",
                    "import: only SVG and STL are supported (not `.{ext}`)"
                ),
            );
            return;
        }
        if let Some(x) = p[10] {
            self.warn(
                x.expr.span,
                trs!(
                    "import: `as` gilt nur für STL und wird ignoriert",
                    "import: `as` applies to STL only and is ignored"
                ),
            );
        }
        let Some((path, data)) = self.asset_data(fspan, &file) else { return };
        let opts = cad_svg::Options {
            dpi: self.pos_num(p[3], "dpi", 96.0),
            id: self.str_arg(p[1], "id"),
            center: p[4].is_some_and(|x| matches!(x.val.v, Value::Bool(true))),
            scale: self.pos_num(p[2], "scale", 1.0),
        };
        let parsed = parse_svg(&data, &opts);
        let svg = match parsed.as_ref() {
            Ok(svg) => svg,
            Err(e) => {
                self.error(fspan, format!("{path}: {e}"));
                return;
            }
        };
        for w in &svg.warnings {
            self.warn(fspan, format!("{path}:{}: {}", w.line, w.msg));
        }
        let mut profiles = Vec::new();
        for sh in &svg.shapes {
            let contours = ir_contours(&sh.normalized());
            if !contours.is_empty() {
                profiles.push(Profile::Sketch { contours, closed: true });
            }
        }
        if profiles.is_empty() {
            if svg.paths.is_empty() {
                self.warn(fspan, tr!("{path}: keine Flächen im SVG", "{path}: no areas in the SVG"));
                return;
            }
            // only open paths: a path (`sweep`), no region
            let open: Vec<cad_outline::Contour> = svg.paths.iter().map(|(c, _)| c.clone()).collect();
            profiles.push(Profile::Sketch { contours: ir_contours(&open), closed: false });
        } else if !svg.paths.is_empty() {
            self.warn(
                fspan,
                tr!(
                    "{path}: {} offene Pfade neben Flächen weggelassen",
                    "{path}: {} open paths next to areas left out",
                    svg.paths.len()
                ),
            );
        }
        self.profiles(s, profiles, ctx, dy);
    }

    /// `text(t, size, font, halign, valign, spacing, direction, language, script, stroke_width,
    /// bridge_width)`; the last two (mm) only for the stencil font.
    #[inline(never)]
    pub(crate) fn m_text(&mut self, s: &'a Stmt, a: &Args<'a>, ctx: &Ctx<'a>, dy: &Dyn<'a>) {
        let names = [
            "text",
            "size",
            "font",
            "halign",
            "valign",
            "spacing",
            "direction",
            "language",
            "script",
            "stroke_width",
            "bridge_width",
        ];
        let p = self.bind("text", a, &names, 9);
        let text = match p[0].map(|x| &x.val.v) {
            None | Some(Value::Undef) => String::new(),
            Some(Value::Str(t)) => t.to_string(),
            Some(Value::Num(n)) => fmt_num(*n),
            Some(_) => {
                self.warn(
                    p[0].unwrap().expr.span,
                    trs!("text: `text` muss ein Text sein", "text: `text` must be a text"),
                );
                return;
            }
        };
        let mut tp = cad_font::TextParams { text, ..Default::default() };
        tp.size = self.pos_num(p[1], "size", 10.0);
        tp.spacing = match p[5].and_then(|x| x.val.as_num()) {
            Some(v) if v.is_finite() => v,
            _ => 1.0,
        };
        let choice = |ev: &mut Self, i: usize, opts: &[&str]| -> Option<String> {
            let v = ev.str_arg(p[i], names[i])?;
            if opts.contains(&v.as_str()) {
                Some(v)
            } else {
                ev.warn(
                    p[i].unwrap().expr.span,
                    tr!("text: `{}` ist einer von {}", "text: `{}` is one of {}", names[i], opts.join(", ")),
                );
                None
            }
        };
        tp.halign = match choice(self, 3, &["left", "center", "right"]).as_deref() {
            Some("center") => cad_font::HAlign::Center,
            Some("right") => cad_font::HAlign::Right,
            _ => cad_font::HAlign::Left,
        };
        tp.valign = match choice(self, 4, &["baseline", "bottom", "center", "top"]).as_deref() {
            Some("bottom") => cad_font::VAlign::Bottom,
            Some("center") => cad_font::VAlign::Center,
            Some("top") => cad_font::VAlign::Top,
            _ => cad_font::VAlign::Baseline,
        };
        tp.direction = choice(self, 6, &["ltr", "rtl", "ttb", "btt"]).and_then(|d| cad_font::Dir::parse(&d));
        tp.language = self.str_arg(p[7], "language");
        tp.script = self.str_arg(p[8], "script");
        let fspan = p[2].map_or(s.span, |x| x.expr.span);
        let font = match self.str_arg(p[2], "font") {
            Some(f) if cad_font::FontName::is_file(&f) => {
                let file = f.split(':').next().unwrap_or("").trim().to_string();
                let Some((path, data)) = self.asset_data(fspan, &file) else { return };
                match cad_font::Font::from_data(data, 0) {
                    Ok(font) => font,
                    Err(e) => {
                        self.error(fspan, format!("{path}: {e}"));
                        return;
                    }
                }
            }
            name => {
                let n = cad_font::FontName::parse(name.as_deref().unwrap_or(""));
                let (font, pick) = cad_font::select(&n);
                let (fam, sty) = font.names();
                match pick {
                    cad_font::Pick::Exact => {}
                    cad_font::Pick::StyleMissing => self.warn(
                        fspan,
                        tr!("Stil „{}“ von {fam} nicht mitgeliefert; {fam} {sty}", "style \"{}\" of {fam} not bundled; {fam} {sty}", n.style.unwrap_or_default()),
                    ),
                    cad_font::Pick::Fallback => self.warn(
                        fspan,
                        tr!("Schrift „{}“ nicht vorhanden, verwendet wird die Standardschrift {fam} (mitgeliefert: cad stencil mono, JetBrains Mono, Inter, Allerta Stencil; eigene Schrift: font = \"datei.ttf\")", "font \"{}\" not available, using the default font {fam} (bundled: cad stencil mono, JetBrains Mono, Inter, Allerta Stencil; own font: font = \"file.ttf\")",
                            n.family
                        ),
                    ),
                }
                font
            }
        };
        // stroke and bridge width of the stencil font (mm)
        for (i, what) in [(9, "stroke_width"), (10, "bridge_width")] {
            let Some(arg) = p[i] else { continue };
            if matches!(arg.val.v, Value::Undef) {
                continue;
            }
            let span = arg.expr.span;
            if !font.is_stencil() {
                self.warn(
                    span,
                    tr!(
                        "text: `{what}` wirkt nur mit der Schrift „{}“",
                        "text: `{what}` only works with the font \"{}\"",
                        cad_font::STENCIL_FAMILY
                    ),
                );
                continue;
            }
            let (lo, hi, zero) = if i == 9 {
                (0.0, cad_font::stencil::MAX_STROKE, false)
            } else {
                (cad_font::stencil::MIN_BRIDGE, cad_font::stencil::MAX_BRIDGE, true)
            };
            let v = match arg.val.as_num() {
                Some(v) if v.is_finite() && (v > 0.0 || zero && v == 0.0) => v,
                _ => {
                    let need = if zero {
                        trs!("eine Zahl ≥ 0", "a number ≥ 0")
                    } else {
                        trs!("eine positive Zahl", "a positive number")
                    };
                    self.warn(span, tr!("text: `{what}` muss {need} sein (mm)", "text: `{what}` must be {need} (mm)"));
                    continue;
                }
            };
            let (lo, hi) = (lo * tp.size, hi * tp.size);
            if v > hi || v > 0.0 && v < lo {
                let lim = if v > hi {
                    tr!("höchstens {} mm", "at most {} mm", fmt_num(hi))
                } else {
                    tr!("mindestens {} mm (oder 0)", "at least {} mm (or 0)", fmt_num(lo))
                };
                self.warn(
                    span,
                    tr!(
                        "text: `{what}` = {} mm bei size = {}: {lim}",
                        "text: `{what}` = {} mm at size = {}: {lim}",
                        fmt_num(v),
                        fmt_num(tp.size)
                    ),
                );
            }
            if i == 9 {
                tp.stroke_width = Some(v);
            } else {
                tp.bridge_width = Some(v);
            }
        }
        if tp.text.is_empty() {
            return;
        }
        let lay = match cad_font::layout(&font, &tp) {
            Ok(l) => l,
            Err(e) => {
                self.error(s.span, format!("text: {e}"));
                return;
            }
        };
        let tspan = p[0].map_or(s.span, |x| x.expr.span);
        if !lay.fallback.is_empty() {
            let chars: String = lay.fallback.iter().collect();
            let fam = font.names().0;
            let fb = cad_font::fallback_font(&font).names();
            self.warn(
                tspan,
                tr!(
                    "Zeichen „{chars}“ fehlen in {fam}; ersetzt aus {} {}",
                    "characters \"{chars}\" missing in {fam}; replaced from {} {}",
                    fb.0,
                    fb.1
                ),
            );
        }
        if !lay.missing.is_empty() {
            let chars: String = lay.missing.iter().collect();
            self.warn(
                tspan,
                tr!(
                    "Zeichen „{chars}“ fehlen in der Schrift und in der Ersatzschrift (Platzhalter)",
                    "characters \"{chars}\" missing in the font and in the fallback font (placeholder)"
                ),
            );
        }
        let profiles: Vec<Profile> = lay
            .glyphs
            .iter()
            .map(|g| ir_contours(&cad_outline::normalize(&g.contours, cad_outline::FillRule::NonZero)))
            .filter(|c| !c.is_empty())
            .map(|contours| Profile::Sketch { contours, closed: true })
            .collect();
        if profiles.is_empty() {
            return;
        }
        self.profiles(s, profiles, ctx, dy);
    }
}
