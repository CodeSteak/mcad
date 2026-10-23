//! SVG import (subset, Plan 17.5): the filled shapes of an SVG document as exact 2D contours
//! in millimetres, y up, OpenSCAD's placement (the document's lower left corner is the
//! origin).
//!
//! Supported: `path` (M, L, H, V, C, S, Q, T, A, Z, absolute and relative), `rect` (with
//! `rx`/`ry`), `circle`, `ellipse`, `polygon`, `polyline`, `line`, `g`, `a` and nested `svg`
//! with `transform` (matrix, translate, scale, rotate, skewX, skewY), `viewBox` /
//! `preserveAspectRatio`, `width`/`height` with units (px, mm, cm, in, pt, pc), `fill`,
//! `fill-rule` (nonzero, evenodd), `display:none`, `visibility`, as attributes and in `style`.
//! Open subpaths, `line` and `polyline` are paths, not areas (warning). Everything else is
//! reported as a [`Warning`] with its line: text, images, `use`, CSS classes and style sheets,
//! masks, clip paths, filters, gradients and patterns as fills, stroke widths.
//!
//! Units: unitless numbers (user units without `viewBox`, `width="100"`) are pixels of
//! `dpi` (default 96), `px` are CSS pixels (1/96 in); with a `viewBox` user units follow from
//! `width`/`height`. OpenSCAD 2021.01 differs in one point: without a `viewBox` it takes user
//! units as millimetres (but the document height for the y flip in pixels).

mod path;

pub use cad_outline::{Contour, FillRule, Seg};
pub use path::{arc, parse_path};

use glam::{DAffine2, DMat2, DVec2};
use roxmltree::{Document, Node};

const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// Import options (the `import()` parameters besides the file).
#[derive(Debug, Clone)]
pub struct Options {
    /// Pixels per inch of unitless numbers.
    pub dpi: f64,
    /// Import only the element with this `id` (and what it contains).
    pub id: Option<String>,
    /// Move the result so that the centre of its bounds is the origin.
    pub center: bool,
    /// Extra scale factor (after the unit conversion).
    pub scale: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options { dpi: 96.0, id: None, center: false, scale: 1.0 }
    }
}

/// Something of the document that was not imported, or imported differently.
#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    /// 1-based line of the element in the SVG file.
    pub line: u32,
    pub msg: String,
}

/// A filled element: its closed contours (mm) and fill rule.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub contours: Vec<Contour>,
    pub rule: FillRule,
    /// Element name and id, e.g. `path#logo`.
    pub element: String,
    pub line: u32,
}

impl Shape {
    /// Contours oriented so that the region is filled by the nonzero rule (see
    /// [`cad_outline::normalize`]).
    pub fn normalized(&self) -> Vec<Contour> {
        cad_outline::normalize(&self.contours, self.rule)
    }
}

/// An imported document.
#[derive(Debug, Clone, Default)]
pub struct Svg {
    /// Filled elements in document order.
    pub shapes: Vec<Shape>,
    /// Open paths (`line`, `polyline`, subpaths without `Z`), mm.
    pub paths: Vec<(Contour, u32)>,
    pub warnings: Vec<Warning>,
    /// Document size in mm (0 if not given).
    pub size: DVec2,
}

impl Svg {
    /// Bounds of all shapes and paths.
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let tol = 1e-3;
        let all = self.shapes.iter().flat_map(|s| s.contours.iter()).chain(self.paths.iter().map(|p| &p.0));
        cad_outline::bounds_of(all.filter_map(|c| c.bounds(tol)).flat_map(|(a, b)| [a, b]))
    }

    /// Applies `m` to all geometry.
    pub fn transform(&mut self, m: &DAffine2) {
        for s in &mut self.shapes {
            s.contours = s.contours.iter().map(|c| c.transform(m)).collect();
        }
        for p in &mut self.paths {
            p.0 = p.0.transform(m);
        }
    }
}

/// Parses an SVG document. `Err` only if the XML is broken or the root is not `svg`, or the
/// requested `id` does not exist; everything else is a warning.
pub fn parse(text: &str, opts: &Options) -> Result<Svg, String> {
    let doc = Document::parse_with_options(text, roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() })
        .map_err(|e| format!("SVG nicht lesbar: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err(format!("kein SVG-Dokument (Wurzelelement `{}`)", root.tag_name().name()));
    }
    let mut st = State { doc: &doc, out: Svg::default(), select: None, warned: Vec::new(), has_css: false };
    if let Some(id) = &opts.id {
        let Some(n) = doc.descendants().find(|n| n.attribute("id") == Some(id.as_str())) else {
            return Err(format!("kein Element mit id \"{id}\" im SVG"));
        };
        st.select = Some(n.id());
    }
    st.has_css = doc.descendants().any(|n| n.tag_name().name() == "style" && !n.text().unwrap_or("").trim().is_empty());
    let unit = 25.4 / opts.dpi.max(1e-9);
    let w = root.attribute("width").and_then(|v| length_mm(v, unit));
    let h = root.attribute("height").and_then(|v| length_mm(v, unit));
    let vb = root.attribute("viewBox").and_then(parse_viewbox);
    let (user, size) = match vb {
        Some([x, y, vw, vh]) => {
            let wmm = w.unwrap_or(vw * unit);
            let hmm = h.unwrap_or(vh * unit);
            (viewbox_map([x, y, vw, vh], wmm, hmm, root.attribute("preserveAspectRatio")), DVec2::new(wmm, hmm))
        }
        None => (DAffine2::from_scale(DVec2::splat(unit)), DVec2::new(w.unwrap_or(0.0), h.unwrap_or(0.0))),
    };
    st.out.size = size;
    // y flip about the document height: the lower left corner is the origin
    let flip = DAffine2::from_cols(DVec2::X, DVec2::NEG_Y, DVec2::new(0.0, size.y));
    let top = DAffine2::from_scale(DVec2::splat(opts.scale)) * flip * user;
    let style = Style::default();
    let sel = st.select.is_none();
    st.children(root, top, &style, sel);
    let mut out = st.out;
    if opts.center
        && let Some((lo, hi)) = out.bounds()
    {
        out.transform(&DAffine2::from_translation(-(lo + hi) / 2.0));
    }
    Ok(out)
}

struct State<'d, 'i> {
    doc: &'d Document<'i>,
    out: Svg,
    select: Option<roxmltree::NodeId>,
    /// Messages already reported (each once, with the first line).
    warned: Vec<String>,
    has_css: bool,
}

/// Inherited presentation properties.
#[derive(Debug, Clone)]
struct Style {
    fill: bool,
    rule: FillRule,
    visible: bool,
    stroke: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style { fill: true, rule: FillRule::NonZero, visible: true, stroke: false }
    }
}

impl<'d, 'i> State<'d, 'i> {
    fn line(&self, n: Node) -> u32 {
        self.doc.text_pos_at(n.range().start).row
    }

    fn warn(&mut self, n: Node, msg: String) {
        if self.warned.contains(&msg) {
            return;
        }
        self.warned.push(msg.clone());
        let line = self.line(n);
        self.out.warnings.push(Warning { line, msg });
    }

    fn children(&mut self, n: Node, m: DAffine2, style: &Style, sel: bool) {
        for c in n.children().filter(|c| c.is_element()) {
            self.element(c, m, style, sel);
        }
    }

    fn element(&mut self, n: Node, parent: DAffine2, pstyle: &Style, sel: bool) {
        let ns = n.tag_name().namespace();
        if ns.is_some_and(|ns| ns != SVG_NS) {
            return; // editor metadata (Inkscape, Sodipodi, …)
        }
        let name = n.tag_name().name();
        let sel = sel || self.select == Some(n.id());
        let props = props(n);
        if props.get("display") == Some("none") {
            return;
        }
        let mut style = pstyle.clone();
        if let Some(f) = props.get("fill") {
            style.fill = f != "none" && f != "transparent";
            if f.starts_with("url(") && sel {
                self.warn(n, format!("`{name}`: Verlauf/Muster als Füllung ({f}) wird als einfache Fläche übernommen"));
            }
        }
        if let Some(r) = props.get("fill-rule") {
            style.rule = if r == "evenodd" { FillRule::EvenOdd } else { FillRule::NonZero };
        }
        if let Some(v) = props.get("visibility") {
            style.visible = v == "visible";
        }
        if let Some(s) = props.get("stroke") {
            style.stroke = s != "none";
        }
        let mut m = parent;
        if let Some(t) = n.attribute("transform") {
            match parse_transform(t) {
                Ok(t) => m = parent * t,
                Err(e) => self.warn(n, format!("`{name}`: transform \"{t}\" nicht lesbar ({e}), ignoriert")),
            }
        }
        if sel {
            for a in ["clip-path", "mask", "filter"] {
                if props.get(a).is_some_and(|v| v != "none") {
                    self.warn(n, format!("`{name}`: `{a}` nicht unterstützt (ignoriert)"));
                }
            }
            if n.attribute("class").is_some() && self.has_css {
                self.warn(
                    n,
                    format!("`{name}`: CSS-Klassen (`class`) nicht unterstützt; Stil nur aus Attributen und `style`"),
                );
            }
        }
        match name {
            "svg" => {
                // nested viewport: x/y offset, viewBox mapping into width/height
                let num = |a: &str| n.attribute(a).and_then(|v| length_user(v));
                let off = DAffine2::from_translation(DVec2::new(num("x").unwrap_or(0.0), num("y").unwrap_or(0.0)));
                let inner = match (n.attribute("viewBox").and_then(parse_viewbox), num("width"), num("height")) {
                    (Some(vb), Some(w), Some(h)) => viewbox_map(vb, w, h, n.attribute("preserveAspectRatio")),
                    (Some([x, y, _, _]), _, _) => DAffine2::from_translation(DVec2::new(-x, -y)),
                    _ => DAffine2::IDENTITY,
                };
                self.children(n, m * off * inner, &style, sel);
            }
            "g" | "a" => self.children(n, m, &style, sel),
            "switch" => {
                self.warn(n, "`switch`: alle Zweige werden übernommen".into());
                self.children(n, m, &style, sel);
            }
            "path" | "rect" | "circle" | "ellipse" | "polygon" | "polyline" | "line" => {
                if sel && style.visible {
                    self.shape(n, name, m, &style);
                }
            }
            "defs" | "symbol" | "clipPath" | "mask" | "marker" | "pattern" | "linearGradient" | "radialGradient"
            | "filter" | "metadata" | "title" | "desc" | "script" | "font" | "font-face" => {}
            "style" => {
                if sel && !n.text().unwrap_or("").trim().is_empty() {
                    self.warn(n, "CSS-Stylesheet (`<style>`) nicht unterstützt".into());
                }
            }
            "text" | "tspan" | "textPath" => {
                if sel {
                    self.warn(n, format!("`{name}`: Text nicht unterstützt (in Pfade umwandeln)"));
                }
            }
            "image" => {
                if sel {
                    self.warn(n, "`image`: Bilder nicht unterstützt".into());
                }
            }
            "use" => {
                if sel {
                    self.warn(n, "`use`: Verweise nicht unterstützt (Klone in Pfade umwandeln)".into());
                }
            }
            other => {
                if sel {
                    self.warn(n, format!("`{other}` nicht unterstützt"));
                }
            }
        }
    }

    fn shape(&mut self, n: Node, name: &str, m: DAffine2, style: &Style) {
        let num = |a: &str| n.attribute(a).and_then(length_user);
        let label = match n.attribute("id") {
            Some(id) => format!("{name}#{id}"),
            None => name.to_string(),
        };
        let mut contours: Vec<Contour> = Vec::new();
        match name {
            "path" => {
                let (cs, err) = parse_path(n.attribute("d").unwrap_or(""));
                if let Some(e) = err {
                    self.warn(n, format!("`{label}`: Pfaddaten fehlerhaft ({e}); Rest ignoriert"));
                }
                contours = cs;
            }
            "rect" => {
                let (x, y) = (num("x").unwrap_or(0.0), num("y").unwrap_or(0.0));
                let (w, h) = (num("width").unwrap_or(0.0), num("height").unwrap_or(0.0));
                if w <= 0.0 || h <= 0.0 {
                    return;
                }
                let (rx, ry) = match (num("rx"), num("ry")) {
                    (Some(a), Some(b)) => (a, b),
                    (Some(a), None) | (None, Some(a)) => (a, a),
                    (None, None) => (0.0, 0.0),
                };
                contours.push(rounded_rect(x, y, w, h, rx.clamp(0.0, w / 2.0), ry.clamp(0.0, h / 2.0)));
            }
            "circle" => {
                let r = num("r").unwrap_or(0.0);
                if r <= 0.0 {
                    return;
                }
                contours.push(cad_outline::circle(DVec2::new(num("cx").unwrap_or(0.0), num("cy").unwrap_or(0.0)), r));
            }
            "ellipse" => {
                let (rx, ry) = match (num("rx"), num("ry")) {
                    (Some(a), Some(b)) => (a, b),
                    (Some(a), None) | (None, Some(a)) => (a, a),
                    (None, None) => (0.0, 0.0),
                };
                if rx <= 0.0 || ry <= 0.0 {
                    return;
                }
                let c = DVec2::new(num("cx").unwrap_or(0.0), num("cy").unwrap_or(0.0));
                let (u, v) = (DVec2::new(rx, 0.0), DVec2::new(0.0, ry));
                let half = |t0: f64, t1: f64| Seg::Ellipse { center: c, u, v, t0, t1 };
                let segs = vec![half(0.0, std::f64::consts::PI), half(std::f64::consts::PI, std::f64::consts::TAU)];
                // a circle after all: exact circular arcs
                let segs = segs.iter().map(|s| s.transform(&DAffine2::IDENTITY)).collect();
                contours.push(Contour { segs, closed: true });
            }
            "polygon" | "polyline" => {
                let pts = parse_points(n.attribute("points").unwrap_or(""));
                if pts.len() < 2 {
                    return;
                }
                contours.push(cad_outline::polygon(&pts, name == "polygon"));
            }
            "line" => {
                let a = DVec2::new(num("x1").unwrap_or(0.0), num("y1").unwrap_or(0.0));
                let b = DVec2::new(num("x2").unwrap_or(0.0), num("y2").unwrap_or(0.0));
                if a != b {
                    contours.push(cad_outline::polygon(&[a, b], false));
                }
            }
            _ => return,
        }
        let line = self.line(n);
        let (closed, open): (Vec<Contour>, Vec<Contour>) = contours.into_iter().partition(|c| c.closed);
        if !open.is_empty() {
            self.warn(
                n,
                format!("`{label}`: offener Pfad wird nicht gefüllt (nur als Pfad übernommen; Z schließt ihn)"),
            );
            for c in open {
                self.out.paths.push((c.transform(&m), line));
            }
        }
        if style.stroke {
            self.warn(n, "Strichstärke (`stroke`) wird nicht als Fläche übernommen".into());
        }
        if closed.is_empty() {
            return;
        }
        if !style.fill {
            self.warn(n, format!("`{label}`: ohne Füllung (fill=\"none\"), nicht übernommen"));
            return;
        }
        self.out.shapes.push(Shape {
            contours: closed.iter().map(|c| c.transform(&m)).collect(),
            rule: style.rule,
            element: label,
            line,
        });
    }
}

/// Presentation attributes and `style` declarations (`style` wins).
struct Props<'a> {
    pairs: Vec<(String, &'a str)>,
}

impl Props<'_> {
    fn get(&self, k: &str) -> Option<&str> {
        self.pairs.iter().rev().find(|(n, _)| n == k).map(|(_, v)| *v)
    }
}

fn props<'a>(n: Node<'a, '_>) -> Props<'a> {
    let mut pairs = Vec::new();
    for k in ["fill", "fill-rule", "display", "visibility", "stroke", "clip-path", "mask", "filter"] {
        if let Some(v) = n.attribute(k) {
            pairs.push((k.to_string(), v.trim()));
        }
    }
    if let Some(s) = n.attribute("style") {
        for decl in s.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                let v = v.trim().trim_end_matches("!important").trim();
                pairs.push((k.trim().to_ascii_lowercase(), v));
            }
        }
    }
    Props { pairs }
}

/// Rectangle with elliptic (circular if `rx == ry`) corners, as in SVG.
fn rounded_rect(x: f64, y: f64, w: f64, h: f64, rx: f64, ry: f64) -> Contour {
    let p = DVec2::new;
    if rx <= 0.0 || ry <= 0.0 {
        return cad_outline::polygon(&[p(x, y), p(x + w, y), p(x + w, y + h), p(x, y + h)], true);
    }
    use std::f64::consts::{FRAC_PI_2, PI};
    let corner = |cx: f64, cy: f64, t0: f64| {
        Seg::Ellipse { center: p(cx, cy), u: p(rx, 0.0), v: p(0.0, ry), t0, t1: t0 + FRAC_PI_2 }
            .transform(&DAffine2::IDENTITY)
    };
    let mut segs = Vec::new();
    let line = |a: DVec2, b: DVec2, segs: &mut Vec<Seg>| {
        if a.distance(b) > 1e-12 {
            segs.push(Seg::Line { a, b });
        }
    };
    // counter-clockwise in user space (y down): top edge first
    line(p(x + rx, y), p(x + w - rx, y), &mut segs);
    segs.push(corner(x + w - rx, y + ry, -FRAC_PI_2));
    line(p(x + w, y + ry), p(x + w, y + h - ry), &mut segs);
    segs.push(corner(x + w - rx, y + h - ry, 0.0));
    line(p(x + w - rx, y + h), p(x + rx, y + h), &mut segs);
    segs.push(corner(x + rx, y + h - ry, FRAC_PI_2));
    line(p(x, y + h - ry), p(x, y + ry), &mut segs);
    segs.push(corner(x + rx, y + ry, PI));
    Contour { segs, closed: true }
}

fn parse_numbers(s: &str) -> Vec<f64> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b',') {
            i += 1;
        }
        let st = i;
        if i < b.len() && matches!(b[i], b'-' | b'+') {
            i += 1;
        }
        let mut dot = false;
        while i < b.len() && (b[i].is_ascii_digit() || (b[i] == b'.' && !dot)) {
            dot |= b[i] == b'.';
            i += 1;
        }
        if i < b.len() && matches!(b[i], b'e' | b'E') {
            i += 1;
            if i < b.len() && matches!(b[i], b'-' | b'+') {
                i += 1;
            }
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i == st {
            break;
        }
        match s[st..i].parse() {
            Ok(v) => out.push(v),
            Err(_) => break,
        }
    }
    out
}

fn parse_points(s: &str) -> Vec<DVec2> {
    parse_numbers(s).chunks_exact(2).map(|c| DVec2::new(c[0], c[1])).collect()
}

fn parse_viewbox(s: &str) -> Option<[f64; 4]> {
    let v = parse_numbers(s);
    (v.len() == 4 && v[2] > 0.0 && v[3] > 0.0).then(|| [v[0], v[1], v[2], v[3]])
}

/// Map of a viewBox into a viewport of `w` × `h` (SVG `preserveAspectRatio`, default
/// `xMidYMid meet`).
fn viewbox_map(vb: [f64; 4], w: f64, h: f64, par: Option<&str>) -> DAffine2 {
    let [x, y, vw, vh] = vb;
    let (sx, sy) = (w / vw, h / vh);
    let par = par.unwrap_or("xMidYMid meet").trim();
    let mut it = par.split_whitespace();
    let align = it.next().unwrap_or("xMidYMid");
    let slice = it.next() == Some("slice");
    if align == "none" {
        return DAffine2::from_cols(DVec2::new(sx, 0.0), DVec2::new(0.0, sy), DVec2::new(-x * sx, -y * sy));
    }
    let s = if slice { sx.max(sy) } else { sx.min(sy) };
    let fx = if align.contains("xMin") {
        0.0
    } else if align.contains("xMax") {
        1.0
    } else {
        0.5
    };
    let fy = if align.contains("YMin") {
        0.0
    } else if align.contains("YMax") {
        1.0
    } else {
        0.5
    };
    let tx = (w - vw * s) * fx - x * s;
    let ty = (h - vh * s) * fy - y * s;
    DAffine2::from_cols(DVec2::new(s, 0.0), DVec2::new(0.0, s), DVec2::new(tx, ty))
}

/// A length in mm; unitless numbers are pixels of `unit` mm, `px` CSS pixels.
pub fn length_mm(v: &str, unit: f64) -> Option<f64> {
    let v = v.trim();
    let split = v.find(|c: char| c.is_ascii_alphabetic() || c == '%').unwrap_or(v.len());
    let (num, u) = v.split_at(split);
    let x: f64 = num.trim().parse().ok()?;
    let f = match u.trim() {
        "" => unit,
        "px" => 25.4 / 96.0,
        "mm" => 1.0,
        "cm" => 10.0,
        "in" => 25.4,
        "pt" => 25.4 / 72.0,
        "pc" => 25.4 / 6.0,
        _ => return None, // %, em, ex: unknown here
    };
    Some(x * f)
}

/// A coordinate in user units (units other than none/px are ignored with their number).
fn length_user(v: &str) -> Option<f64> {
    let v = v.trim();
    let split = v.find(|c: char| c.is_ascii_alphabetic() || c == '%').unwrap_or(v.len());
    v[..split].trim().parse().ok()
}

/// Parses an SVG `transform` list into one affine map (applied right to left as in SVG).
pub fn parse_transform(s: &str) -> Result<DAffine2, String> {
    let mut m = DAffine2::IDENTITY;
    let mut rest = s.trim();
    while !rest.is_empty() {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        if rest.is_empty() {
            break;
        }
        let open = rest.find('(').ok_or("`(` fehlt")?;
        let close = rest.find(')').ok_or("`)` fehlt")?;
        let name = rest[..open].trim();
        let a = parse_numbers(&rest[open + 1..close]);
        let t = match (name, a.len()) {
            ("matrix", 6) => {
                DAffine2::from_cols(DVec2::new(a[0], a[1]), DVec2::new(a[2], a[3]), DVec2::new(a[4], a[5]))
            }
            ("translate", 1) => DAffine2::from_translation(DVec2::new(a[0], 0.0)),
            ("translate", 2) => DAffine2::from_translation(DVec2::new(a[0], a[1])),
            ("scale", 1) => DAffine2::from_scale(DVec2::splat(a[0])),
            ("scale", 2) => DAffine2::from_scale(DVec2::new(a[0], a[1])),
            ("rotate", 1) => DAffine2::from_angle(a[0].to_radians()),
            ("rotate", 3) => {
                let c = DVec2::new(a[1], a[2]);
                DAffine2::from_translation(c) * DAffine2::from_angle(a[0].to_radians()) * DAffine2::from_translation(-c)
            }
            ("skewX", 1) => DAffine2::from_mat2(DMat2::from_cols(DVec2::X, DVec2::new(a[0].to_radians().tan(), 1.0))),
            ("skewY", 1) => DAffine2::from_mat2(DMat2::from_cols(DVec2::new(1.0, a[0].to_radians().tan()), DVec2::Y)),
            _ => return Err(format!("`{name}` mit {} Werten", a.len())),
        };
        m = m * t;
        rest = &rest[close + 1..];
    }
    Ok(m)
}

#[cfg(test)]
mod tests;
