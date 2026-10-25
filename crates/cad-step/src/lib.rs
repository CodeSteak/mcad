//! STEP export (ISO 10303-21 exchange structure) of [`cad_kernel::brep::Brep`]s.
//!
//! Schema: **AP242** (ISO 10303-242, `AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF`) by
//! default, AP214 (`AUTOMOTIVE_DESIGN`) on request. AP242 is the current edition that merges
//! and replaces AP203 and AP214; the B-rep entities (ISO 10303-42) are identical in both, so
//! the choice only changes the header and the application context. AP214 remains for older
//! readers that do not know the AP242 schema name.
//!
//! Content:
//! * one `PRODUCT` with `PRODUCT_DEFINITION_SHAPE` and a shape representation in mm and
//!   degrees (`ADVANCED_BREP_SHAPE_REPRESENTATION` for solids, `MANIFOLD_SURFACE_SHAPE_
//!   REPRESENTATION` for open surfaces, `SHAPE_REPRESENTATION` for both);
//! * one `MANIFOLD_SOLID_BREP` (or `BREP_WITH_VOIDS`) per solid body, `CLOSED_SHELL`,
//!   `ADVANCED_FACE`, `FACE_BOUND`, `EDGE_LOOP`/`VERTEX_LOOP`, `ORIENTED_EDGE`, `EDGE_CURVE`,
//!   `VERTEX_POINT`; open surfaces as `SHELL_BASED_SURFACE_MODEL` of `OPEN_SHELL`s;
//! * surfaces `PLANE`, `CYLINDRICAL_SURFACE`, `CONICAL_SURFACE`, `SPHERICAL_SURFACE`,
//!   `TOROIDAL_SURFACE`, `SURFACE_OF_REVOLUTION`, `SURFACE_OF_LINEAR_EXTRUSION`,
//!   `B_SPLINE_SURFACE_WITH_KNOTS` (rational: complex instance with
//!   `RATIONAL_B_SPLINE_SURFACE`); curves `LINE`, `CIRCLE`, `ELLIPSE`,
//!   `B_SPLINE_CURVE_WITH_KNOTS` (rational likewise). No p-curves: readers project the 3D edges.
//! * optional colours per body (`STYLED_ITEM` with `COLOUR_RGB`).

pub mod p21;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use cad_kernel::brep::{BSplineCurve, BSplineSurface, Body, Brep, Curve, Face, Frame, Loop, Shell, Surface};
use glam::DVec3;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub enum Schema {
    #[default]
    Ap242,
    Ap214,
}

impl Schema {
    pub fn parse(s: &str) -> Option<Schema> {
        match s.to_ascii_lowercase().as_str() {
            "ap242" | "242" => Some(Schema::Ap242),
            "ap214" | "214" => Some(Schema::Ap214),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StepOptions {
    pub schema: Schema,
    /// Product and file name.
    pub name: String,
    /// ISO 8601 time stamp of the header (fixed default for reproducible files).
    pub timestamp: String,
    pub author: String,
    pub organization: String,
    pub originating_system: String,
    /// Uncertainty (mm) written into the representation context.
    pub uncertainty: f64,
    /// Optional colour (sRGB 0..1) per body, in the order of [`write`]'s bodies.
    pub colors: Vec<Option<[f64; 3]>>,
    /// Colour per face tag (faces without an entry get none).
    pub tag_colors: BTreeMap<u32, [f64; 3]>,
    /// Name of each B-rep given to [`write`] (in order): the label of its body, or `name 1`,
    /// `name 2`, … if several bodies carry the same name. Empty: `name N` with the product
    /// name and N counted across all bodies.
    pub body_names: Vec<String>,
}

impl Default for StepOptions {
    fn default() -> Self {
        StepOptions {
            schema: Schema::Ap242,
            name: "part".into(),
            timestamp: "1970-01-01T00:00:00".into(),
            author: String::new(),
            organization: String::new(),
            originating_system: "mcad".into(),
            uncertainty: 1e-6,
            colors: Vec::new(),
            tag_colors: BTreeMap::new(),
            body_names: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StepError {
    #[error("STEP: nothing to export (no faces)")]
    Empty,
    #[error("STEP: invalid B-rep: {0}")]
    Invalid(String),
    #[error("STEP: non-finite number")]
    NonFinite,
}

/// Counts of what [`write`] wrote (CLI, tests, report).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct StepStats {
    pub solids: usize,
    pub sheets: usize,
    pub voids: usize,
    pub faces: usize,
    pub edges: usize,
    pub vertices: usize,
    pub surfaces: BTreeMap<String, usize>,
    pub curves: BTreeMap<String, usize>,
    pub approximated_faces: usize,
    pub approximated_edges: usize,
    pub entities: usize,
    pub bytes: usize,
    /// Labels of the bodies (`MANIFOLD_SOLID_BREP`, `BREP_WITH_VOIDS`, surface models) in
    /// file order.
    pub bodies: Vec<String>,
}

/// STEP REAL: always with a decimal point, exponent as `E`.
pub fn real(x: f64) -> Result<String, StepError> {
    if !x.is_finite() {
        return Err(StepError::NonFinite);
    }
    if x == 0.0 {
        return Ok("0.".into());
    }
    let s = format!("{x:?}");
    Ok(match s.split_once('e') {
        Some((m, e)) => {
            let m = if m.contains('.') { m.to_string() } else { format!("{m}.") };
            format!("{m}E{e}")
        }
        None if s.contains('.') => s,
        None => format!("{s}."),
    })
}

/// STEP string literal: quotes doubled, non-ASCII as `\X2\` UCS-2 hex (`\X4\` beyond the BMP).
pub fn string(s: &str) -> String {
    let mut out = String::from("'");
    for ch in s.chars() {
        match ch {
            '\'' => out.push_str("''"),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(ch),
            c if (c as u32) <= 0xFFFF => {
                let _ = write!(out, "\\X2\\{:04X}\\X0\\", c as u32);
            }
            c => {
                let _ = write!(out, "\\X4\\{:08X}\\X0\\", c as u32);
            }
        }
    }
    out.push('\'');
    out
}

struct W {
    data: String,
    next: u32,
    err: Option<StepError>,
    /// (ADVANCED_FACE id, tag) of every face written.
    faces: Vec<(u32, u32)>,
}

impl W {
    fn add(&mut self, body: impl AsRef<str>) -> u32 {
        let id = self.next;
        self.next += 1;
        let _ = writeln!(self.data, "#{id}={};", body.as_ref());
        id
    }

    fn r(&mut self, x: f64) -> String {
        real(x).unwrap_or_else(|e| {
            self.err.get_or_insert(e);
            "0.".into()
        })
    }

    fn reals(&mut self, xs: &[f64]) -> String {
        let v: Vec<String> = xs.iter().map(|x| self.r(*x)).collect();
        v.join(",")
    }

    fn point(&mut self, p: DVec3) -> u32 {
        let s = self.reals(&p.to_array());
        self.add(format!("CARTESIAN_POINT('',({s}))"))
    }

    fn direction(&mut self, d: DVec3) -> u32 {
        let d = d.normalize_or_zero();
        let s = self.reals(&d.to_array());
        self.add(format!("DIRECTION('',({s}))"))
    }

    fn axis2(&mut self, f: &Frame) -> u32 {
        let (p, z, x) = (self.point(f.origin), self.direction(f.z), self.direction(f.x));
        self.add(format!("AXIS2_PLACEMENT_3D('',#{p},#{z},#{x})"))
    }
}

fn refs(ids: &[u32]) -> String {
    ids.iter().map(|i| format!("#{i}")).collect::<Vec<_>>().join(",")
}

fn ints(xs: &[usize]) -> String {
    xs.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
}

/// Distinct knots and multiplicities.
fn knots_mults(k: &[f64]) -> (Vec<f64>, Vec<usize>) {
    let mut u: Vec<f64> = Vec::new();
    let mut m: Vec<usize> = Vec::new();
    for &x in k {
        if u.last() == Some(&x) {
            *m.last_mut().unwrap() += 1;
        } else {
            u.push(x);
            m.push(1);
        }
    }
    (u, m)
}

fn bspline_curve(w: &mut W, c: &BSplineCurve) -> u32 {
    let pts: Vec<u32> = c.points.iter().map(|p| w.point(*p)).collect();
    let (u, m) = knots_mults(&c.knots);
    let (us, ms) = (w.reals(&u), ints(&m));
    let deg = c.degree;
    match &c.weights {
        Some(wt) if c.is_rational() => {
            let ws = w.reals(wt);
            w.add(format!(
                "(BOUNDED_CURVE() B_SPLINE_CURVE({deg},({}),.UNSPECIFIED.,.F.,.F.) \
                 B_SPLINE_CURVE_WITH_KNOTS(({ms}),({us}),.UNSPECIFIED.) CURVE() \
                 GEOMETRIC_REPRESENTATION_ITEM() RATIONAL_B_SPLINE_CURVE(({ws})) REPRESENTATION_ITEM(''))",
                refs(&pts)
            ))
        }
        _ => w.add(format!(
            "B_SPLINE_CURVE_WITH_KNOTS('',{deg},({}),.UNSPECIFIED.,.F.,.F.,({ms}),({us}),.UNSPECIFIED.)",
            refs(&pts)
        )),
    }
}

fn bspline_surface(w: &mut W, s: &BSplineSurface) -> u32 {
    let mut rows = Vec::with_capacity(s.nu);
    for i in 0..s.nu {
        let row: Vec<u32> = (0..s.nv).map(|j| w.point(s.points[i * s.nv + j])).collect();
        rows.push(format!("({})", refs(&row)));
    }
    let (ku, mu) = knots_mults(&s.knots_u);
    let (kv, mv) = knots_mults(&s.knots_v);
    let (kus, kvs) = (w.reals(&ku), w.reals(&kv));
    let (mus, mvs) = (ints(&mu), ints(&mv));
    let (du, dv) = (s.degree_u, s.degree_v);
    let net = rows.join(",");
    match &s.weights {
        Some(wt) if s.is_rational() => {
            let mut wrows = Vec::with_capacity(s.nu);
            for i in 0..s.nu {
                wrows.push(format!("({})", w.reals(&wt[i * s.nv..(i + 1) * s.nv])));
            }
            w.add(format!(
                "(BOUNDED_SURFACE() B_SPLINE_SURFACE({du},{dv},({net}),.UNSPECIFIED.,.F.,.F.,.F.) \
                 B_SPLINE_SURFACE_WITH_KNOTS(({mus}),({mvs}),({kus}),({kvs}),.UNSPECIFIED.) \
                 GEOMETRIC_REPRESENTATION_ITEM() RATIONAL_B_SPLINE_SURFACE(({})) REPRESENTATION_ITEM('') SURFACE())",
                wrows.join(",")
            ))
        }
        _ => w.add(format!(
            "B_SPLINE_SURFACE_WITH_KNOTS('',{du},{dv},({net}),.UNSPECIFIED.,.F.,.F.,.F.,({mus}),({mvs}),({kus}),({kvs}),.UNSPECIFIED.)"
        )),
    }
}

fn curve(w: &mut W, c: &Curve) -> u32 {
    match c {
        Curve::Line { origin, dir } => {
            let p = w.point(*origin);
            let d = w.direction(*dir);
            let v = w.add(format!("VECTOR('',#{d},1.)"));
            w.add(format!("LINE('',#{p},#{v})"))
        }
        Curve::Circle { frame, r } => {
            let a = w.axis2(frame);
            let r = w.r(*r);
            w.add(format!("CIRCLE('',#{a},{r})"))
        }
        Curve::Ellipse { frame, r1, r2 } => {
            let a = w.axis2(frame);
            let (r1, r2) = (w.r(*r1), w.r(*r2));
            w.add(format!("ELLIPSE('',#{a},{r1},{r2})"))
        }
        Curve::BSpline(b) => bspline_curve(w, b),
    }
}

fn surface(w: &mut W, s: &Surface) -> u32 {
    match s {
        Surface::Plane { frame } => {
            let a = w.axis2(frame);
            w.add(format!("PLANE('',#{a})"))
        }
        Surface::Cylinder { frame, r } => {
            let a = w.axis2(frame);
            let r = w.r(*r);
            w.add(format!("CYLINDRICAL_SURFACE('',#{a},{r})"))
        }
        Surface::Cone { frame, r, semi_angle } => {
            let a = w.axis2(frame);
            let (r, ang) = (w.r(*r), w.r(semi_angle.to_degrees()));
            w.add(format!("CONICAL_SURFACE('',#{a},{r},{ang})"))
        }
        Surface::Sphere { frame, r } => {
            let a = w.axis2(frame);
            let r = w.r(*r);
            w.add(format!("SPHERICAL_SURFACE('',#{a},{r})"))
        }
        Surface::Torus { frame, major, minor } => {
            let a = w.axis2(frame);
            let (ma, mi) = (w.r(*major), w.r(*minor));
            w.add(format!("TOROIDAL_SURFACE('',#{a},{ma},{mi})"))
        }
        Surface::Revolution { curve: c, axis_origin, axis_dir } => {
            let c = bspline_curve(w, c);
            let p = w.point(*axis_origin);
            let d = w.direction(*axis_dir);
            let ax = w.add(format!("AXIS1_PLACEMENT('',#{p},#{d})"));
            w.add(format!("SURFACE_OF_REVOLUTION('',#{c},#{ax})"))
        }
        Surface::Extrusion { curve: c, dir } => {
            let c = bspline_curve(w, c);
            let d = w.direction(*dir);
            let m = w.r(dir.length());
            let v = w.add(format!("VECTOR('',#{d},{m})"));
            w.add(format!("SURFACE_OF_LINEAR_EXTRUSION('',#{c},#{v})"))
        }
        Surface::BSpline(b) => bspline_surface(w, b),
    }
}

/// Analytic form of a sweep surface: a linear extrusion of a circular arc along the circle's
/// axis is a cylinder (sketch arcs: SMLib keeps them as extrusions of their exact rational
/// B-spline). Returns the surface and whether its normal is opposite to the original one.
fn analytic_surface(s: &Surface) -> (Option<Surface>, bool) {
    let Surface::Extrusion { curve, dir } = s else { return (None, false) };
    let (t0, t1) = curve.domain();
    let (a, b) = (curve.eval(t0), curve.eval(t1));
    let size = curve.points.iter().fold(1.0f64, |m, p| m.max(p.length()));
    let Curve::Circle { frame, r } = cad_kernel::brep::recognise_curve(curve, a, b, 1e-9 * size) else {
        return (None, false);
    };
    let Some(d) = dir.try_normalize() else { return (None, false) };
    if frame.z.cross(d).length() > 1e-9 {
        return (None, false);
    }
    // extrusion normal: curve' × dir, outwards for a curve turning counter-clockwise about dir;
    // the cylinder's normal always points away from the axis
    let reversed = frame.z.dot(d) < 0.0;
    (Some(Surface::Cylinder { frame, r }), reversed)
}

/// An extrusion of an elliptic arc (rational quadratic B-spline) perpendicular to its plane:
/// the ellipse's frame (z: the arc's turning direction, x: major axis), radii and the extrusion
/// vector. Found by a conic through five samples, checked on 65.
fn elliptic_extrusion(s: &Surface) -> Option<(cad_kernel::brep::Frame, f64, f64, DVec3)> {
    let Surface::Extrusion { curve, dir } = s else { return None };
    if !curve.is_rational() || curve.degree != 2 {
        return None;
    }
    const N: usize = 64;
    let (t0, t1) = curve.domain();
    let ts: Vec<f64> = (0..=N).map(|i| t0 + (t1 - t0) * i as f64 / N as f64).collect();
    let pts = curve.eval_many(&ts);
    let size = pts.iter().fold(1.0f64, |m, p| m.max(p.length()));
    let tol = 1e-9 * size;
    let d = dir.try_normalize()?;
    let c0 = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
    if pts.iter().any(|p| (*p - c0).dot(d).abs() > tol) {
        return None;
    }
    let x = (pts[0] - c0) - d * (pts[0] - c0).dot(d);
    let x = x.try_normalize()?;
    let y = d.cross(x);
    let q: Vec<glam::DVec2> = pts.iter().map(|p| glam::DVec2::new((*p - c0).dot(x), (*p - c0).dot(y))).collect();
    let scale = q.iter().fold(0.0f64, |m, p| m.max(p.length())).max(1e-12);
    let q: Vec<glam::DVec2> = q.iter().map(|p| *p / scale).collect();
    // A x² + B xy + C y² + D x + E y = 1 through five samples (Gauss with pivoting)
    let mut m = [[0.0f64; 6]; 5];
    for (r, &i) in [0, N / 5, 2 * N / 5, 3 * N / 5, 4 * N / 5].iter().enumerate() {
        let p = q[i];
        m[r] = [p.x * p.x, p.x * p.y, p.y * p.y, p.x, p.y, 1.0];
    }
    for col in 0..5 {
        let piv = (col..5).max_by(|&a, &b| m[a][col].abs().total_cmp(&m[b][col].abs()))?;
        m.swap(col, piv);
        if m[col][col].abs() < 1e-12 {
            return None;
        }
        let pivot = m[col];
        for (r, row) in m.iter_mut().enumerate() {
            if r != col {
                let f = row[col] / pivot[col];
                for (x, p) in row.iter_mut().zip(pivot).skip(col) {
                    *x -= f * p;
                }
            }
        }
    }
    let [a, b, c, dd, e] = [0, 1, 2, 3, 4].map(|i| m[i][5] / m[i][i]);
    let conic = |p: glam::DVec2| a * p.x * p.x + b * p.x * p.y + c * p.y * p.y + dd * p.x + e * p.y - 1.0;
    if q.iter().any(|p| conic(*p).abs() > 1e-8) {
        return None;
    }
    let det = 4.0 * a * c - b * b;
    if det <= 1e-12 {
        return None; // not an ellipse
    }
    let ctr = glam::DVec2::new((-2.0 * c * dd + b * e) / det, (b * dd - 2.0 * a * e) / det);
    let f0 = conic(ctr);
    let h = ((a - c) / 2.0).hypot(b / 2.0);
    let (l_small, l_big) = ((a + c) / 2.0 - h, (a + c) / 2.0 + h);
    if l_small <= 0.0 || f0 >= 0.0 {
        return None;
    }
    let (r1, r2) = ((-f0 / l_small).sqrt() * scale, (-f0 / l_big).sqrt() * scale);
    // major axis: eigenvector of the smaller eigenvalue
    let ev = if b.abs() > 1e-15 {
        glam::DVec2::new(b / 2.0, l_small - a).normalize()
    } else if a <= c {
        glam::DVec2::X
    } else {
        glam::DVec2::Y
    };
    let origin = c0 + (x * ctr.x + y * ctr.y) * scale;
    let xaxis = x * ev.x + y * ev.y;
    // turning direction of the arc
    let (u, v) = (q[0] - ctr, q[1] - ctr);
    let z = if u.perp_dot(v) >= 0.0 { d } else { -d };
    let frame = cad_kernel::brep::Frame::new(origin, z, xaxis)?;
    // verify in the ellipse's frame
    let fy = frame.y();
    let ok = pts.iter().all(|p| {
        let w = *p - origin;
        let (s, t) = (w.dot(frame.x) / r1, w.dot(fy) / r2);
        (s * s + t * t - 1.0).abs() <= 1e-8
    });
    ok.then_some((frame, r1, r2, *dir))
}

/// Entity ids of one B-rep's shared vertices and edges.
struct Shared {
    vertices: Vec<Option<u32>>,
    edges: Vec<Option<u32>>,
}

fn vertex(w: &mut W, b: &Brep, sh: &mut Shared, v: u32) -> u32 {
    if let Some(id) = sh.vertices[v as usize] {
        return id;
    }
    let p = w.point(b.vertices[v as usize]);
    let id = w.add(format!("VERTEX_POINT('',#{p})"));
    sh.vertices[v as usize] = Some(id);
    id
}

fn edge(w: &mut W, b: &Brep, sh: &mut Shared, st: &mut StepStats, e: u32) -> u32 {
    if let Some(id) = sh.edges[e as usize] {
        return id;
    }
    let ed = &b.edges[e as usize];
    let (va, vb) = (vertex(w, b, sh, ed.start), vertex(w, b, sh, ed.end));
    let c = curve(w, &ed.curve);
    let id = w.add(format!("EDGE_CURVE('',#{va},#{vb},#{c},.T.)"));
    sh.edges[e as usize] = Some(id);
    st.edges += 1;
    *st.curves.entry(ed.curve.kind().to_string()).or_insert(0) += 1;
    st.approximated_edges += ed.approximated as usize;
    id
}

/// `flip`: write the face reversed (for the closed shell under an inverted
/// `ORIENTED_CLOSED_SHELL`).
fn face(w: &mut W, b: &Brep, sh: &mut Shared, st: &mut StepStats, f: &Face, flip: bool) -> u32 {
    let mut bounds = Vec::with_capacity(f.loops.len());
    let orient = if flip { ".F." } else { ".T." };
    for l in &f.loops {
        let lp = match l {
            Loop::Edges(es) => {
                let mut oes = Vec::with_capacity(es.len());
                for oe in es {
                    let ec = edge(w, b, sh, st, oe.edge);
                    let o = if oe.forward { ".T." } else { ".F." };
                    oes.push(w.add(format!("ORIENTED_EDGE('',*,*,#{ec},{o})")));
                }
                w.add(format!("EDGE_LOOP('',({}))", refs(&oes)))
            }
            Loop::Vertex(v) => {
                let vp = vertex(w, b, sh, *v);
                w.add(format!("VERTEX_LOOP('',#{vp})"))
            }
        };
        bounds.push(w.add(format!("FACE_BOUND('',#{lp},{orient})")));
    }
    let (surf, reversed) = analytic_surface(&f.surface);
    let surf = surf.as_ref().unwrap_or(&f.surface);
    let s = match elliptic_extrusion(surf) {
        Some((frame, r1, r2, dir)) => {
            // `ELLIPSE` as the swept curve (imported SVG ellipses, Bericht 16): OpenCASCADE
            // integrates extrusions of rational B-splines without p-curves inaccurately
            let a = w.axis2(&frame);
            let (r1, r2) = (w.r(r1), w.r(r2));
            let c = w.add(format!("ELLIPSE('',#{a},{r1},{r2})"));
            let d = w.direction(dir);
            let m = w.r(dir.length());
            let v = w.add(format!("VECTOR('',#{d},{m})"));
            w.add(format!("SURFACE_OF_LINEAR_EXTRUSION('',#{c},#{v})"))
        }
        None => surface(w, surf),
    };
    let sense = if (f.same_sense != flip) != reversed { ".T." } else { ".F." };
    st.faces += 1;
    *st.surfaces.entry(surf.kind().to_string()).or_insert(0) += 1;
    st.approximated_faces += f.approximated as usize;
    let id = w.add(format!("ADVANCED_FACE('',({}),#{s},{sense})", refs(&bounds)));
    w.faces.push((id, f.tag));
    id
}

fn shell(w: &mut W, b: &Brep, sh: &mut Shared, st: &mut StepStats, s: &Shell, kind: &str, flip: bool) -> u32 {
    let faces: Vec<u32> = s.faces.iter().map(|f| face(w, b, sh, st, f, flip)).collect();
    w.add(format!("{kind}('',({}))", refs(&faces)))
}

/// Writes `breps` (all their bodies) as one product. Bodies are numbered across all B-reps.
pub fn write(breps: &[&Brep], opts: &StepOptions) -> Result<(String, StepStats), StepError> {
    let mut st = StepStats::default();
    if breps.iter().all(|b| b.faces().next().is_none()) {
        return Err(StepError::Empty);
    }
    for b in breps {
        b.validate().map_err(StepError::Invalid)?;
    }
    let mut w = W { data: String::new(), next: 1, err: None, faces: Vec::new() };
    let name = string(&opts.name);
    let (app, apd, schema) = match opts.schema {
        Schema::Ap242 => (
            "APPLICATION_CONTEXT('managed model based 3d engineering')",
            "'international standard','ap242_managed_model_based_3d_engineering',2014",
            "AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF { 1 0 10303 442 1 1 4 }",
        ),
        Schema::Ap214 => (
            "APPLICATION_CONTEXT('core data for automotive mechanical design processes')",
            "'international standard','automotive_design',2000",
            "AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }",
        ),
    };
    let ac = w.add(app);
    w.add(format!("APPLICATION_PROTOCOL_DEFINITION({apd},#{ac})"));
    let pc = w.add(format!("PRODUCT_CONTEXT('',#{ac},'mechanical')"));
    let pdc = w.add(format!("PRODUCT_DEFINITION_CONTEXT('part definition',#{ac},'design')"));
    let prod = w.add(format!("PRODUCT({name},{name},'',(#{pc}))"));
    w.add(format!("PRODUCT_RELATED_PRODUCT_CATEGORY('part',$,(#{prod}))"));
    let pdf = w.add(format!("PRODUCT_DEFINITION_FORMATION('','',#{prod})"));
    let pd = w.add(format!("PRODUCT_DEFINITION('design','',#{pdf},#{pdc})"));
    let pds = w.add(format!("PRODUCT_DEFINITION_SHAPE('','',#{pd})"));

    // units: mm, degree, steradian
    let mm = w.add("(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.))");
    let rad = w.add("(NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.))");
    let pam = w.add(format!("PLANE_ANGLE_MEASURE_WITH_UNIT(PLANE_ANGLE_MEASURE(0.017453292519943295),#{rad})"));
    let dim = w.add("DIMENSIONAL_EXPONENTS(0.,0.,0.,0.,0.,0.,0.)");
    let deg = w.add(format!("(CONVERSION_BASED_UNIT('DEGREE',#{pam}) NAMED_UNIT(#{dim}) PLANE_ANGLE_UNIT())"));
    let sr = w.add("(NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT())");
    let unc = w.r(opts.uncertainty);
    let um = w.add(format!(
        "UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE({unc}),#{mm},'distance_accuracy_value','confusion accuracy')"
    ));
    let ctx = w.add(format!(
        "(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{um})) \
         GLOBAL_UNIT_ASSIGNED_CONTEXT((#{mm},#{deg},#{sr})) REPRESENTATION_CONTEXT('Context3D','3D Context with UNIT and UNCERTAINTY'))"
    ));

    // body labels: the B-rep's name, numbered where several bodies share it
    let named = !opts.body_names.is_empty();
    let base: Vec<&str> = breps
        .iter()
        .enumerate()
        .flat_map(|(i, b)| {
            let n = if named { opts.body_names.get(i).map_or(opts.name.as_str(), |s| s.as_str()) } else { "" };
            std::iter::repeat_n(n, b.bodies.len())
        })
        .collect();
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let labels: Vec<String> = base
        .iter()
        .enumerate()
        .map(|(k, n)| {
            if !named {
                return format!("{} {}", opts.name, k + 1);
            }
            if base.iter().filter(|x| *x == n).count() == 1 {
                return n.to_string();
            }
            let c = seen.entry(n).or_insert(0);
            *c += 1;
            format!("{n} {c}")
        })
        .collect();
    st.bodies = labels.clone();
    let mut items = Vec::new();
    let (mut n_solid, mut n_sheet) = (0usize, 0usize);
    let mut body_items: Vec<u32> = Vec::new();
    for b in breps {
        let mut sh = Shared { vertices: vec![None; b.vertices.len()], edges: vec![None; b.edges.len()] };
        for body in &b.bodies {
            let label = string(&labels[n_solid + n_sheet]);
            match body {
                Body::Solid { outer, voids } => {
                    n_solid += 1;
                    let cs = shell(&mut w, b, &mut sh, &mut st, outer, "CLOSED_SHELL", false);
                    let id = if voids.is_empty() {
                        w.add(format!("MANIFOLD_SOLID_BREP({label},#{cs})"))
                    } else {
                        let mut vs = Vec::new();
                        for v in voids {
                            let c = shell(&mut w, b, &mut sh, &mut st, v, "CLOSED_SHELL", true);
                            vs.push(w.add(format!("ORIENTED_CLOSED_SHELL('',*,#{c},.F.)")));
                            st.voids += 1;
                        }
                        w.add(format!("BREP_WITH_VOIDS({label},#{cs},({}))", refs(&vs)))
                    };
                    items.push((id, true));
                    body_items.push(id);
                }
                Body::Sheet { shells } => {
                    n_sheet += 1;
                    let ss: Vec<u32> =
                        shells.iter().map(|s| shell(&mut w, b, &mut sh, &mut st, s, "OPEN_SHELL", false)).collect();
                    let id = w.add(format!("SHELL_BASED_SURFACE_MODEL({label},({}))", refs(&ss)));
                    items.push((id, false));
                    body_items.push(id);
                }
            }
        }
        st.vertices += sh.vertices.iter().filter(|v| v.is_some()).count();
    }
    st.solids = n_solid;
    st.sheets = n_sheet;
    let origin = w.axis2(&Frame { origin: DVec3::ZERO, z: DVec3::Z, x: DVec3::X });
    let mut all: Vec<u32> = items.iter().map(|(i, _)| *i).collect();
    all.push(origin);
    let rep_kind = match (n_solid > 0, n_sheet > 0) {
        (true, false) => "ADVANCED_BREP_SHAPE_REPRESENTATION",
        (false, true) => "MANIFOLD_SURFACE_SHAPE_REPRESENTATION",
        _ => "SHAPE_REPRESENTATION",
    };
    let rep = w.add(format!("{rep_kind}({name},({}),#{ctx})", refs(&all)));
    w.add(format!("SHAPE_DEFINITION_REPRESENTATION(#{pds},#{rep})"));

    // colours: one style per distinct colour, styled items for bodies and faces
    let mut styles: Vec<([u64; 3], u32)> = Vec::new();
    let mut style = |w: &mut W, c: &[f64; 3]| -> u32 {
        let key = c.map(f64::to_bits);
        if let Some((_, id)) = styles.iter().find(|(k, _)| *k == key) {
            return *id;
        }
        let (r, g, b) = (w.r(c[0]), w.r(c[1]), w.r(c[2]));
        let col = w.add(format!("COLOUR_RGB('',{r},{g},{b})"));
        let fsc = w.add(format!("FILL_AREA_STYLE_COLOUR('',#{col})"));
        let fas = w.add(format!("FILL_AREA_STYLE('',(#{fsc}))"));
        let ssfa = w.add(format!("SURFACE_STYLE_FILL_AREA(#{fas})"));
        let sss = w.add(format!("SURFACE_SIDE_STYLE('',(#{ssfa}))"));
        let ssu = w.add(format!("SURFACE_STYLE_USAGE(.BOTH.,#{sss})"));
        let psa = w.add(format!("PRESENTATION_STYLE_ASSIGNMENT((#{ssu}))"));
        styles.push((key, psa));
        psa
    };
    let mut styled = Vec::new();
    for (i, id) in body_items.iter().enumerate() {
        let Some(Some(c)) = opts.colors.get(i) else { continue };
        let psa = style(&mut w, c);
        styled.push(w.add(format!("STYLED_ITEM('color',(#{psa}),#{id})")));
    }
    let faces = std::mem::take(&mut w.faces);
    for (id, tag) in faces {
        let Some(c) = opts.tag_colors.get(&tag) else { continue };
        let psa = style(&mut w, c);
        styled.push(w.add(format!("STYLED_ITEM('color',(#{psa}),#{id})")));
    }
    if !styled.is_empty() {
        w.add(format!("MECHANICAL_DESIGN_GEOMETRIC_PRESENTATION_REPRESENTATION('',({}),#{ctx})", refs(&styled)));
    }
    if let Some(e) = w.err {
        return Err(e);
    }
    st.entities = (w.next - 1) as usize;

    let mut out = String::with_capacity(w.data.len() + 1024);
    out.push_str("ISO-10303-21;\nHEADER;\n");
    let _ = writeln!(out, "FILE_DESCRIPTION(('mcad STEP export'),'2;1');");
    let _ = writeln!(
        out,
        "FILE_NAME({},{},({}),({}),'cad-step {}',{},'');",
        string(&format!("{}.step", opts.name)),
        string(&opts.timestamp),
        string(&opts.author),
        string(&opts.organization),
        env!("CARGO_PKG_VERSION"),
        string(&opts.originating_system),
    );
    let _ = writeln!(out, "FILE_SCHEMA(('{schema}'));");
    out.push_str("ENDSEC;\nDATA;\n");
    out.push_str(&w.data);
    out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    st.bytes = out.len();
    Ok((out, st))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reals_are_step_reals() {
        assert_eq!(real(0.0).unwrap(), "0.");
        assert_eq!(real(1.0).unwrap(), "1.0");
        assert_eq!(real(-2.5).unwrap(), "-2.5");
        assert_eq!(real(1e-7).unwrap(), "1.E-7");
        assert_eq!(real(1.5e300).unwrap(), "1.5E300");
        assert_eq!(real(123456789.0).unwrap(), "123456789.0");
        assert!(real(f64::NAN).is_err());
        for x in [0.1, 1.0 / 3.0, -7.25e-12, 6.02e23] {
            let s = real(x).unwrap();
            assert_eq!(s.replace('E', "e").parse::<f64>().unwrap(), x, "{s}");
        }
    }

    #[test]
    fn strings_are_escaped() {
        assert_eq!(string("it's"), "'it''s'");
        assert_eq!(string("Würfel"), "'W\\X2\\00FC\\X0\\rfel'");
        assert_eq!(string("a\\b"), "'a\\\\b'");
    }

    #[test]
    fn knots_compress() {
        let (u, m) = knots_mults(&[0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0]);
        assert_eq!(u, vec![0.0, 0.5, 1.0]);
        assert_eq!(m, vec![3, 1, 3]);
    }
}
