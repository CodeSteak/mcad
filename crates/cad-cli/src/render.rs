//! `cad render`: the model as PNG.
//!
//! * Selection: the root (default), `--part NAME` (only these parts), `--line N` / `--node I`
//!   (highlighted as in the editor: nodes of the line full, the rest dimmed, zones blue,
//!   subtracted bodies red transparent; `--only` hides the rest).
//! * Camera: `--view`, `--azimuth/--elevation`, `--zoom`, `--target x,y,z`,
//!   `--fit all|part:NAME|line:N`, `--ortho`/`--perspective [--fov]`; `--views a,b,…|all`
//!   renders a contact sheet of labelled tiles with the same scale.
//! * Overlays `--show grids,anchors,handles,edges,labels,bbox,dims`.
//! * `--section x|y|z=WERT`, `--section-plane px,py,pz,nx,ny,nz`, `--section-flip`: clipping
//!   in the renderer with a filled cap (see `cad_render` and [`parse_section`] for the
//!   convention); `--xray`: everything transparent with edges.

use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use cad_kernel::Mesh;
use cad_render::{
    CameraInfo, Item, Label, Marker, MarkerShape, Polyline, Projection, RenderOptions, Scene, Section, View,
    contact_sheet, render_info, tile_origin,
};
use cad_session::protocol as p;
use clap::Args;
use glam::{DMat4, DVec3};
use serde_json::{Value as Json, json};

use crate::{Common, Fail, Model, Outcome, load, mat, nodes_at_line, num, world_bounds};

const PART: [f32; 4] = [0.75, 0.78, 0.84, 1.0];
const DIM: [f32; 4] = [0.78, 0.79, 0.82, 0.22];
const RED: [f32; 4] = [0.9, 0.2, 0.15, 0.35];
const BLUE: [f32; 4] = [0.2, 0.45, 0.95, 0.3];
const GREY: [f32; 4] = [0.55, 0.55, 0.58, 0.3];
const EDGE: [f32; 4] = [0.15, 0.15, 0.18, 1.0];
const DIM_EDGE: [f32; 4] = [0.45, 0.45, 0.5, 0.35];
const SELECTED: [f32; 4] = [1.0, 0.55, 0.0, 1.0];
const FAILED: [f32; 4] = [0.95, 0.1, 0.1, 1.0];
const XRAY: [f32; 4] = [0.55, 0.66, 0.82, 0.22];
const XRAY_HI: [f32; 4] = [0.95, 0.62, 0.2, 0.42];
const GRID: [f32; 4] = [0.22, 0.4, 0.78, 0.6];
const GRID_POINT: [f32; 4] = [0.15, 0.3, 0.7, 0.95];
const ANCHOR: [f32; 4] = [0.98, 0.76, 0.1, 1.0];
const HANDLE: [f32; 4] = [1.0, 0.45, 0.05, 1.0];
const BBOX: [f32; 4] = [0.1, 0.55, 0.3, 0.9];
const HIDDEN: [f32; 4] = [0.15, 0.15, 0.3, 0.7];
const TEXT: [f32; 4] = [0.08, 0.08, 0.12, 1.0];

/// Names of `--views all`.
pub(crate) const ALL_VIEWS: [&str; 4] = ["iso", "front", "top", "right"];
/// Grids with more points get no point labels (only plane names).
const MAX_POINT_LABELS: usize = 48;
const SHOW: [&str; 7] = ["grids", "anchors", "handles", "edges", "labels", "bbox", "dims"];

/// Selection, camera, overlays, section (shared by `render` and `watch --render`).
#[derive(Args, Clone, Debug, Default)]
pub(crate) struct ViewArgs {
    /// iso, front, back, top, bottom, left, right
    #[arg(long, default_value = "iso")]
    pub view: String,
    /// Kontaktbogen: mehrere Ansichten mit gleicher Skalierung, z. B. `iso,front,top,right`;
    /// `all` = iso,front,top,right.
    #[arg(long, value_name = "A,B,…|all")]
    pub views: Option<String>,
    /// Freie Kamera: Azimut in Grad (0 = von vorn/−y, 90 = von rechts/+x).
    #[arg(long, allow_hyphen_values = true)]
    pub azimuth: Option<f64>,
    /// Freie Kamera: Höhe in Grad (90 = von oben).
    #[arg(long, allow_hyphen_values = true)]
    pub elevation: Option<f64>,
    /// Vergrößerung nach dem Einpassen (2 = doppelt so groß).
    #[arg(long)]
    pub zoom: Option<f64>,
    /// Blickpunkt `x,y,z` (Kontextkoordinaten, mm).
    #[arg(long, value_name = "X,Y,Z", allow_hyphen_values = true)]
    pub target: Option<String>,
    /// Einpassen auf `all` (ganzes Modell), `part:NAME` oder `line:N` (Standard: alles Gezeichnete).
    #[arg(long, value_name = "all|part:NAME|line:N")]
    pub fit: Option<String>,
    /// Parallelprojektion (Standard).
    #[arg(long, conflicts_with = "perspective")]
    pub ortho: bool,
    /// Zentralprojektion.
    #[arg(long)]
    pub perspective: bool,
    /// Bildwinkel der Zentralprojektion in Grad.
    #[arg(long, default_value_t = 35.0)]
    pub fov: f64,
    /// Nur dieses Bauteil (mehrfach möglich).
    #[arg(long = "part", value_name = "NAME")]
    pub part: Vec<String>,
    /// Knoten dieser Zeile voll, Rest gedimmt (mehrfach möglich).
    #[arg(long)]
    pub line: Vec<u32>,
    /// Knoten mit diesem Index (aus `cad eval`) voll, Rest gedimmt (mehrfach möglich).
    #[arg(long)]
    pub node: Vec<u32>,
    /// Mit --line/--node: Rest nicht gedimmt zeigen, sondern weglassen.
    #[arg(long)]
    pub only: bool,
    /// Bildgröße BxH (Standard 1024x768, Kontaktbogen 1600x1200 gesamt).
    #[arg(long)]
    pub size: Option<String>,
    /// Dateikontext behalten, auch wenn die Zeile in einer Moduldefinition liegt.
    #[arg(long)]
    pub whole_file: bool,
    /// Kein Achsenkreuz.
    #[arg(long)]
    pub no_axes: bool,
    /// Überlagerungen: grids, anchors, handles, edges, labels, bbox, dims (Komma-Liste, `all`).
    #[arg(long, value_delimiter = ',')]
    pub show: Vec<String>,
    /// Schnitt an einer Achsebene: `x=WERT`, `y=WERT`, `z=WERT` (entfernt die Seite +Achse).
    #[arg(long, value_name = "x|y|z=WERT", allow_hyphen_values = true)]
    pub section: Option<String>,
    /// Schnitt an beliebiger Ebene: Punkt und Normale; entfernt wird die Seite, in die die
    /// Normale zeigt.
    #[arg(long, value_name = "PX,PY,PZ,NX,NY,NZ", allow_hyphen_values = true)]
    pub section_plane: Option<String>,
    /// Schnitt: die andere Hälfte behalten (Normale umkehren).
    #[arg(long)]
    pub section_flip: bool,
    /// Alles halbtransparent mit Kanten (Röntgen).
    #[arg(long)]
    pub xray: bool,
}

pub(crate) fn parse_size(s: &str) -> Result<(u32, u32), Fail> {
    let bad = || Fail::Usage(format!("--size {s}: erwartet BxH, z. B. 1024x768"));
    let (w, h) = s.split_once(['x', 'X']).ok_or_else(bad)?;
    let w: u32 = w.trim().parse().map_err(|_| bad())?;
    let h: u32 = h.trim().parse().map_err(|_| bad())?;
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return Err(bad());
    }
    Ok((w, h))
}

pub(crate) fn parse_vec(s: &str, n: usize, what: &str) -> Result<Vec<f64>, Fail> {
    let t = s.trim().trim_start_matches('[').trim_end_matches(']');
    let v: Result<Vec<f64>, _> = t.split(',').map(|x| x.trim().parse::<f64>()).collect();
    match v {
        Ok(v) if v.len() == n && v.iter().all(|x| x.is_finite()) => Ok(v),
        _ => Err(Fail::Usage(format!("{what} {s}: erwartet {n} Zahlen mit Komma"))),
    }
}

/// Section plane from `--section x|y|z=WERT` / `--section-plane p,n` and `--section-flip`.
///
/// Convention (also for the UI): the plane through `point` with `normal` removes the half
/// space the normal points into; what stays is `(q − point) · normal ≤ 0`. `x=WERT` has the
/// normal +x (removes x > WERT), likewise y and z; `--section-flip` reverses the normal.
pub(crate) fn parse_section(sec: Option<&str>, plane: Option<&str>, flip: bool) -> Result<Option<Section>, Fail> {
    let s = match (sec, plane) {
        (Some(_), Some(_)) => return Err(Fail::Usage("--section und --section-plane schließen sich aus".into())),
        (Some(s), None) => {
            let bad = || Fail::Usage(format!("--section {s}: erwartet x=WERT, y=WERT oder z=WERT"));
            let (a, v) = s.split_once('=').ok_or_else(bad)?;
            let v: f64 = v.trim().parse().map_err(|_| bad())?;
            let n = match a.trim() {
                "x" => DVec3::X,
                "y" => DVec3::Y,
                "z" => DVec3::Z,
                _ => return Err(bad()),
            };
            Section { point: n * v, normal: n }
        }
        (None, Some(pl)) => {
            let v = parse_vec(pl, 6, "--section-plane")?;
            let n = DVec3::new(v[3], v[4], v[5]);
            if n.length() < 1e-12 {
                return Err(Fail::Usage("--section-plane: Normale ist null".into()));
            }
            Section { point: DVec3::new(v[0], v[1], v[2]), normal: n.normalize() }
        }
        (None, None) => {
            if flip {
                return Err(Fail::Usage("--section-flip braucht --section oder --section-plane".into()));
            }
            return Ok(None);
        }
    };
    Ok(Some(if flip { Section { point: s.point, normal: -s.normal } } else { s }))
}

/// A parsed render request.
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub views: Vec<(String, View)>,
    pub sheet: bool,
    pub size: (u32, u32),
    pub projection: Projection,
    pub zoom: f64,
    pub target: Option<DVec3>,
    pub fit: Option<String>,
    pub parts: Vec<String>,
    pub lines: Vec<u32>,
    pub nodes: Vec<u32>,
    pub only: bool,
    pub axes: bool,
    pub show: Vec<String>,
    pub section: Option<Section>,
    pub xray: bool,
}

impl Plan {
    pub fn from_args(a: &ViewArgs) -> Result<Plan, Fail> {
        let one = |name: &str| {
            View::from_name(name)
                .ok_or_else(|| Fail::Usage(format!("--view {name}: iso, front, back, top, bottom, left oder right")))
        };
        let (views, sheet) = match &a.views {
            Some(v) => {
                let names: Vec<String> = if v.trim() == "all" {
                    ALL_VIEWS.iter().map(|s| s.to_string()).collect()
                } else {
                    v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
                };
                if names.is_empty() {
                    return Err(Fail::Usage("--views: keine Ansicht".into()));
                }
                let mut out = Vec::new();
                for n in names {
                    let view = one(&n).map_err(|_| Fail::Usage(format!("--views: unbekannte Ansicht {n}")))?;
                    out.push((n, view));
                }
                (out, true)
            }
            None => {
                let base = one(&a.view)?;
                if a.azimuth.is_some() || a.elevation.is_some() {
                    let (az, el) = base.angles();
                    let (az, el) = (a.azimuth.unwrap_or(az), a.elevation.unwrap_or(el));
                    (vec![(format!("az {} el {}", num(az), num(el)), View::from_angles(az, el))], false)
                } else {
                    (vec![(a.view.clone(), base)], false)
                }
            }
        };
        let size = match &a.size {
            Some(s) => parse_size(s)?,
            None if sheet => (1600, 1200),
            None => (1024, 768),
        };
        let mut show: Vec<String> = Vec::new();
        for s in &a.show {
            let s = s.trim();
            if s == "all" {
                show.extend(SHOW.iter().map(|x| x.to_string()));
            } else if SHOW.contains(&s) {
                show.push(s.to_string());
            } else if !s.is_empty() {
                return Err(Fail::Usage(format!("--show {s}: erwartet {}", SHOW.join(", "))));
            }
        }
        show.dedup();
        if let Some(z) = a.zoom
            && !(z.is_finite() && z > 0.0)
        {
            return Err(Fail::Usage(format!("--zoom {z}: muss positiv sein")));
        }
        if !(a.fov > 1.0 && a.fov < 170.0) {
            return Err(Fail::Usage(format!("--fov {}: 1…170 Grad", a.fov)));
        }
        let target = match &a.target {
            Some(t) => {
                let v = parse_vec(t, 3, "--target")?;
                Some(DVec3::new(v[0], v[1], v[2]))
            }
            None => None,
        };
        Ok(Plan {
            views,
            sheet,
            size,
            projection: if a.perspective { Projection::Perspective { fov_deg: a.fov } } else { Projection::Ortho },
            zoom: a.zoom.unwrap_or(1.0),
            target,
            fit: a.fit.clone(),
            parts: a.part.clone(),
            lines: a.line.clone(),
            nodes: a.node.clone(),
            only: a.only,
            axes: !a.no_axes,
            show,
            section: parse_section(a.section.as_deref(), a.section_plane.as_deref(), a.section_flip)?,
            xray: a.xray,
        })
    }

    fn shows(&self, s: &str) -> bool {
        self.show.iter().any(|x| x == s)
    }
}

/// `true` if `n` or an ancestor satisfies `f`.
fn up(sc: &p::Scene, mut n: u32, f: impl Fn(&p::SceneNode) -> bool) -> bool {
    loop {
        let node = &sc.nodes[n as usize];
        if f(node) {
            return true;
        }
        match node.parent {
            Some(par) if Some(n) != sc.root => n = par,
            _ => return false,
        }
    }
}

struct Draw {
    node: u32,
    color: [f32; 4],
    edges: Option<[f32; 4]>,
    /// Split the mesh by `color()` of the creating nodes.
    node_colors: bool,
    role: &'static str,
}

/// Splits a mesh by triangle colour. Edges stay with the first part.
fn split_by_color(m: &Mesh, color_of: &dyn Fn(u32) -> Option<[f32; 4]>, base: [f32; 4]) -> Vec<(Mesh, [f32; 4])> {
    let colors: Vec<[f32; 4]> = m.tri_tags.iter().map(|&t| color_of(t).unwrap_or(base)).collect();
    if colors.iter().all(|c| *c == base) {
        return vec![(m.clone(), base)];
    }
    let mut groups: Vec<([f32; 4], Vec<usize>)> = Vec::new();
    for (i, c) in colors.iter().enumerate() {
        match groups.iter_mut().find(|(gc, _)| gc == c) {
            Some(g) => g.1.push(i),
            None => groups.push((*c, vec![i])),
        }
    }
    let mut out = Vec::new();
    for (k, (c, tris)) in groups.into_iter().enumerate() {
        let mut sub = Mesh::default();
        let mut remap: HashMap<u32, u32> = HashMap::new();
        for t in tris {
            for &vi in &m.indices[3 * t..3 * t + 3] {
                let ni = *remap.entry(vi).or_insert_with(|| {
                    let v = vi as usize * 3;
                    sub.positions.extend_from_slice(&m.positions[v..v + 3]);
                    sub.normals.extend_from_slice(&m.normals[v..v + 3]);
                    (sub.positions.len() / 3 - 1) as u32
                });
                sub.indices.push(ni);
            }
            sub.tri_tags.push(m.tri_tags[t]);
        }
        if k == 0 {
            sub.edge_segments = m.edge_segments.clone();
            sub.edge_ids = m.edge_ids.clone();
        }
        let mut c = c;
        c[3] = base[3];
        out.push((sub, c));
    }
    out
}

fn polyline(points: &[p::Vec3], m: &DMat4, color: [f32; 4], width: f32) -> Polyline {
    Polyline::new(points.iter().map(|q| m.transform_point3(DVec3::from_array(q.0))).collect(), color, width)
}

fn top_line(points: Vec<DVec3>, color: [f32; 4], width: f32, dashed: bool) -> Polyline {
    Polyline { points, color, width, on_top: true, dashed }
}

/// Bounds union.
pub(crate) fn union(a: Option<p::Bounds>, b: &p::Bounds) -> Option<p::Bounds> {
    Some(match a {
        None => b.clone(),
        Some(a) => p::Bounds {
            min: p::Vec3(DVec3::from_array(a.min.0).min(DVec3::from_array(b.min.0)).to_array()),
            max: p::Vec3(DVec3::from_array(a.max.0).max(DVec3::from_array(b.max.0)).to_array()),
        },
    })
}

/// World bounds of nodes (context coordinates).
pub(crate) fn nodes_bounds(sc: &p::Scene, nodes: &[u32]) -> Option<p::Bounds> {
    let mut acc = None;
    for &n in nodes {
        let node = &sc.nodes[n as usize];
        if node.status == p::NodeStatus::Skipped {
            continue;
        }
        if let Some(b) = &node.bounds {
            acc = union(acc, &world_bounds(b, &mat(&node.display_world)));
        }
    }
    acc
}

pub(crate) fn bounds_json(b: &Option<p::Bounds>) -> Json {
    match b {
        None => Json::Null,
        Some(b) => {
            let r = |v: f64| {
                let x = (v * 1e9).round() / 1e9;
                if x == 0.0 { 0.0 } else { x }
            };
            let (lo, hi): (Vec<f64>, Vec<f64>) = (0..3).map(|i| (r(b.min.0[i]), r(b.max.0[i]))).unzip();
            let s: Vec<f64> = (0..3).map(|i| r(hi[i] - lo[i])).collect();
            json!({ "min": lo, "max": hi, "size": s })
        }
    }
}

/// Nodes of a target spec `all`, `part:NAME` (or a bare part name), `line:N`, `node:I`.
pub(crate) fn target_nodes(sc: &p::Scene, spec: &str) -> Result<Vec<u32>, Fail> {
    let spec = spec.trim();
    if spec == "all" {
        return Ok(sc.root.into_iter().collect());
    }
    if let Some(l) = spec.strip_prefix("line:") {
        let l: u32 = l.trim().parse().map_err(|_| Fail::Usage(format!("{spec}: erwartet line:ZEILE")))?;
        let n = nodes_at_line(sc, l);
        if n.is_empty() {
            return Err(Fail::Usage(format!("{spec}: Zeile {l} erzeugt keine Geometrie")));
        }
        return Ok(n);
    }
    if let Some(i) = spec.strip_prefix("node:") {
        let i: u32 = i.trim().parse().map_err(|_| Fail::Usage(format!("{spec}: erwartet node:INDEX")))?;
        if i as usize >= sc.nodes.len() {
            return Err(Fail::Usage(format!("{spec}: Knoten {i} existiert nicht ({} Knoten)", sc.nodes.len())));
        }
        return Ok(vec![i]);
    }
    let name = spec.strip_prefix("part:").unwrap_or(spec);
    match sc.parts.iter().find(|pt| pt.name == name) {
        Some(pt) => Ok(vec![pt.node]),
        None => {
            let known: Vec<&str> = sc.parts.iter().map(|p| p.name.as_str()).collect();
            Err(Fail::Usage(format!(
                "Bauteil `{name}` nicht gefunden (Bauteile: {}; Ziele: all, part:NAME, line:N, node:I)",
                if known.is_empty() { "keine".to_string() } else { known.join(", ") }
            )))
        }
    }
}

/// (mesh, colour, placement, edge colour, section cap)
type Owned = (Mesh, [f32; 4], DMat4, Option<[f32; 4]>, bool);

/// Everything to draw, built once per evaluation (meshes owned).
struct Built {
    meshes: Vec<Owned>,
    lines: Vec<Polyline>,
    markers: Vec<Marker>,
    labels: Vec<Label>,
    triangles: usize,
    visible: Vec<Json>,
    highlighted: Vec<u32>,
    subject: Option<p::Bounds>,
    counts: Json,
    notes: Vec<String>,
}

fn build(m: &mut Model, sc: &p::Scene, plan: &Plan) -> Result<Built, Fail> {
    let live = |n: u32| sc.nodes[n as usize].status != p::NodeStatus::Skipped;
    let mut highlighted: Vec<u32> = Vec::new();
    for &l in &plan.lines {
        for n in nodes_at_line(sc, l) {
            if !highlighted.contains(&n) {
                highlighted.push(n);
            }
        }
    }
    for &n in &plan.nodes {
        if n as usize >= sc.nodes.len() {
            return Err(Fail::Usage(format!("--node {n}: Knoten existiert nicht ({} Knoten)", sc.nodes.len())));
        }
        if !highlighted.contains(&n) {
            highlighted.push(n);
        }
    }
    let mut base: Vec<u32> = Vec::new();
    for name in &plan.parts {
        base.extend(target_nodes(sc, &format!("part:{name}"))?);
    }
    if plan.parts.is_empty() {
        base.extend(sc.root);
    }
    base.retain(|&n| live(n));
    let selecting = !plan.lines.is_empty() || !plan.nodes.is_empty();

    let mut draws: Vec<Draw> = Vec::new();
    let (part_c, dim_c) = if plan.xray { (XRAY, XRAY) } else { (PART, DIM) };
    let edge_c = if plan.xray { Some([0.2, 0.25, 0.35, 0.9]) } else { Some(EDGE) };
    for &b in &base {
        if !selecting || highlighted.contains(&b) {
            draws.push(Draw { node: b, color: part_c, edges: edge_c, node_colors: !plan.xray, role: "base" });
        } else if !plan.only {
            let e = if plan.xray { edge_c } else { Some(DIM_EDGE) };
            draws.push(Draw { node: b, color: dim_c, edges: e, node_colors: false, role: "dimmed" });
        }
    }
    for &h in &highlighted {
        if !live(h) || base.contains(&h) {
            continue;
        }
        let n = &sc.nodes[h as usize];
        let (color, edges, role) = if up(sc, h, |x| x.flags.zone) {
            (BLUE, None, "zone")
        } else if up(sc, h, |x| x.subtracted) {
            (RED, None, "subtracted")
        } else if up(sc, h, |x| x.flags.background) {
            (GREY, None, "background")
        } else if plan.xray {
            (XRAY_HI, edge_c, "highlighted")
        } else {
            (PART, Some(EDGE), "highlighted")
        };
        draws.push(Draw { node: h, color, edges, node_colors: color == PART, role });
        // Display of the operation: subtracted bodies red, zones blue.
        for &ch in &n.children {
            let cn = &sc.nodes[ch as usize];
            if !live(ch) {
                continue;
            }
            if cn.subtracted {
                draws.push(Draw { node: ch, color: RED, edges: None, node_colors: false, role: "subtracted" });
            } else if cn.flags.zone && matches!(n.label.as_str(), "fillet" | "chamfer" | "shell") {
                draws.push(Draw { node: ch, color: BLUE, edges: None, node_colors: false, role: "zone" });
            }
        }
    }
    let mut lines: Vec<Polyline> = Vec::new();
    for &h in &highlighted {
        let n = &sc.nodes[h as usize];
        let w = mat(&n.display_world);
        for e in &n.selected_edges {
            lines.push(polyline(e, &w, SELECTED, 2.5));
        }
        for msg in &n.messages {
            for e in &msg.edges {
                lines.push(polyline(e, &w, FAILED, 3.0));
            }
        }
    }
    // Modifiers are always visible: `#` red transparent, `%` grey transparent.
    if plan.parts.is_empty() {
        for n in &sc.nodes {
            if !live(n.idx) || n.flags.disabled {
                continue;
            }
            let parent_has = |f: &dyn Fn(&p::SceneNode) -> bool| n.parent.is_some_and(|par| up(sc, par, |x| f(x)));
            if n.flags.highlight && !parent_has(&|x| x.flags.highlight) {
                draws.push(Draw { node: n.idx, color: RED, edges: None, node_colors: false, role: "modifier #" });
            }
            if n.flags.background && !parent_has(&|x| x.flags.background) {
                draws.push(Draw { node: n.idx, color: GREY, edges: None, node_colors: false, role: "modifier %" });
            }
        }
    }

    // Meshes (cached per key in the engine).
    let tag_color: HashMap<u32, [f32; 4]> = sc
        .tag_nodes
        .iter()
        .filter_map(|tn| tn.nodes.iter().find_map(|&n| sc.nodes[n as usize].color).map(|c| (tn.tag, c.0)))
        .collect();
    let mut meshes = Vec::new();
    let mut triangles = 0usize;
    let mut visible = Vec::new();
    for d in &draws {
        let mesh: Rc<Mesh> = m.session.mesh(d.node, false).map_err(Fail::Kernel)?;
        let node = &sc.nodes[d.node as usize];
        let w = mat(&node.display_world);
        triangles += mesh.triangle_count();
        let cap = d.color[3] >= 0.999;
        if d.node_colors {
            for (sub, col) in split_by_color(&mesh, &|t| tag_color.get(&t).copied(), d.color) {
                meshes.push((sub, col, w, d.edges, cap));
            }
        } else {
            meshes.push(((*mesh).clone(), d.color, w, d.edges, cap));
        }
        visible.push(json!({
            "idx": node.idx, "label": node.label, "instance": node.instance, "role": d.role,
            "line": node.range.line, "file": node.range.file, "triangles": mesh.triangle_count(),
        }));
    }

    let subject_nodes: Vec<u32> = if highlighted.is_empty() { base.clone() } else { highlighted.clone() };
    let subject = nodes_bounds(sc, &subject_nodes).or_else(|| nodes_bounds(sc, &base));
    let mut markers = Vec::new();
    let mut labels = Vec::new();
    let mut notes = Vec::new();
    let size = subject
        .as_ref()
        .map(|b| (DVec3::from_array(b.max.0) - DVec3::from_array(b.min.0)).length())
        .filter(|s| *s > 1e-9)
        .unwrap_or(10.0);
    let labels_on = plan.shows("labels");
    let (mut n_grids, mut n_points, mut n_anchors, mut n_handles) = (0, 0, 0, 0);

    if plan.shows("grids") {
        let grids: Vec<&p::Grid> = sc.grids.iter().filter(|g| g.in_context).collect();
        if grids.is_empty() {
            notes.push("keine Raster im Kontext (Raster gelten nur in ihrem Modul: --module NAME)".into());
        }
        for g in grids {
            let n: usize = (0..3u8).map(|a| g.planes.iter().filter(|pl| pl.axis == a).count().max(1)).product();
            if labels_on && n > MAX_POINT_LABELS {
                notes.push(format!(
                    "Raster {}: {n} Punkte, nur Ebenen beschriftet (Punkte bis {MAX_POINT_LABELS}; --view und --zoom für Details)",
                    g.name
                ));
            }
            n_grids += 1;
            n_points += grid_overlay(g, subject.as_ref(), size, labels_on, &mut lines, &mut markers, &mut labels);
        }
    }
    if plan.shows("anchors") {
        for a in &sc.anchors {
            n_anchors += 1;
            let pnt = DVec3::from_array(a.point.0);
            markers.push(Marker { pos: pnt, shape: MarkerShape::Diamond, size: 5.5, color: ANCHOR, on_top: true });
            let name = a.text.clone().unwrap_or_else(|| a.name.clone());
            labels.push(Label { pos: pnt, text: name, color: TEXT, offset: (7.0, -18.0), priority: 4 });
            if let Some(d) = &a.dir {
                let d = DVec3::from_array(d.0).normalize_or_zero();
                if d != DVec3::ZERO {
                    let l = size * 0.12;
                    let tip = pnt + d * l;
                    let side = d.any_orthonormal_vector();
                    lines.push(top_line(vec![pnt, tip], ANCHOR, 2.0, false));
                    lines.push(top_line(
                        vec![tip - d * l * 0.3 + side * l * 0.15, tip, tip - d * l * 0.3 - side * l * 0.15],
                        ANCHOR,
                        2.0,
                        false,
                    ));
                }
            }
        }
    }
    if plan.shows("handles") {
        for h in &sc.handles {
            n_handles += 1;
            let pnt = DVec3::from_array(h.world_pos.0);
            markers.push(Marker { pos: pnt, shape: MarkerShape::Square, size: 3.5, color: HANDLE, on_top: true });
            if labels_on {
                let kind = match h.kind {
                    p::HandleKind::Point => "Punkt",
                    p::HandleKind::Radius => "Radius",
                    p::HandleKind::Height => "Höhe",
                    p::HandleKind::Angle { .. } => "Winkel",
                    p::HandleKind::Plane { .. } => "Ebene",
                    p::HandleKind::Scale { .. } => "Skala",
                };
                labels.push(Label {
                    pos: pnt,
                    text: format!("{kind} Z.{}", h.range.line),
                    color: [0.55, 0.22, 0.0, 1.0],
                    offset: (6.0, 4.0),
                    priority: 0,
                });
            }
        }
    }
    if plan.shows("edges") {
        // hidden edges dashed on top of everything
        for (mesh, _, w, _, _) in &meshes {
            for seg in mesh.edge_segments.chunks_exact(6) {
                let a = w.transform_point3(DVec3::new(seg[0] as f64, seg[1] as f64, seg[2] as f64));
                let b = w.transform_point3(DVec3::new(seg[3] as f64, seg[4] as f64, seg[5] as f64));
                lines.push(top_line(vec![a, b], HIDDEN, 1.0, true));
            }
        }
    }
    if labels_on {
        for pt in sc.parts.iter().filter(|pt| plan.parts.is_empty() || plan.parts.contains(&pt.name)) {
            if let Some(b) = &pt.bounds {
                let (lo, hi) = (DVec3::from_array(b.min.0), DVec3::from_array(b.max.0));
                let pos = DVec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, hi.z);
                labels.push(Label {
                    pos,
                    text: pt.name.clone(),
                    color: [0.1, 0.1, 0.45, 1.0],
                    offset: (-8.0, -20.0),
                    priority: 5,
                });
            }
        }
    }
    if (plan.shows("bbox") || plan.shows("dims"))
        && let Some(b) = &subject
    {
        let (lo, hi) = (DVec3::from_array(b.min.0), DVec3::from_array(b.max.0));
        let c = |i: u32| {
            DVec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            )
        };
        for (a, b2) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)]
        {
            lines.push(top_line(vec![c(a), c(b2)], BBOX, 1.3, false));
        }
        if plan.shows("dims") {
            let s = hi - lo;
            let dims = [
                ((lo + DVec3::new(s.x / 2.0, 0.0, 0.0)), format!("B {} mm", num(s.x)), (-30.0, 6.0)),
                ((DVec3::new(hi.x, lo.y + s.y / 2.0, lo.z)), format!("T {} mm", num(s.y)), (8.0, 4.0)),
                ((DVec3::new(hi.x, lo.y, lo.z + s.z / 2.0)), format!("H {} mm", num(s.z)), (8.0, -6.0)),
            ];
            for (pos, text, off) in dims {
                labels.push(Label { pos, text, color: [0.0, 0.35, 0.12, 1.0], offset: off, priority: 6 });
            }
        }
    }
    let counts = json!({
        "grids": n_grids, "grid_points": n_points, "anchors": n_anchors, "handles": n_handles,
        "labels": labels.len(), "lines": lines.len(), "markers": markers.len(),
    });
    Ok(Built { meshes, lines, markers, labels, triangles, visible, highlighted, subject, counts, notes })
}

/// Lines (plane rectangles at the grid's border), points and labels of one grid.
fn grid_overlay(
    g: &p::Grid,
    subject: Option<&p::Bounds>,
    size: f64,
    labels_on: bool,
    lines: &mut Vec<Polyline>,
    markers: &mut Vec<Marker>,
    labels: &mut Vec<Label>,
) -> usize {
    let frame = mat(&g.frame);
    let inv = frame.inverse();
    // subject bounds in the grid system (for axes with a single plane)
    let local_box = subject.map(|b| {
        let (lo, hi) = (DVec3::from_array(b.min.0), DVec3::from_array(b.max.0));
        let (mut l, mut h) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for i in 0..8 {
            let c = DVec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            );
            let q = inv.transform_point3(c);
            l = l.min(q);
            h = h.max(q);
        }
        (l, h)
    });
    let planes: [Vec<&p::Plane>; 3] =
        std::array::from_fn(|a| g.planes.iter().filter(|pl| pl.axis == a as u8).collect());
    let mut ext = [(0.0f64, 0.0f64); 3];
    for a in 0..3 {
        let vals: Vec<f64> = planes[a].iter().map(|pl| pl.value).collect();
        let (mut lo, mut hi) = vals.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| (l.min(*v), h.max(*v)));
        if hi - lo < 1e-9 {
            match local_box {
                Some((l, h)) => {
                    lo = lo.min(l[a]);
                    hi = hi.max(h[a]);
                }
                None => {
                    lo -= size * 0.25;
                    hi += size * 0.25;
                }
            }
        }
        ext[a] = (lo, hi);
    }
    let margin = (size * 0.04).max(0.5);
    let ext: [(f64, f64); 3] = ext.map(|(l, h)| (l - margin, h + margin));
    let w = |v: DVec3| frame.transform_point3(v);
    for a in 0..3 {
        let (b, c) = ((a + 1) % 3, (a + 2) % 3);
        for pl in &planes[a] {
            if pl.implicit {
                continue;
            }
            let corner = |u: f64, v: f64| {
                let mut q = DVec3::ZERO;
                q[a] = pl.value;
                q[b] = u;
                q[c] = v;
                w(q)
            };
            let (b0, b1, c0, c1) = (ext[b].0, ext[b].1, ext[c].0, ext[c].1);
            let pts = vec![corner(b0, c0), corner(b1, c0), corner(b1, c1), corner(b0, c1), corner(b0, c0)];
            lines.push(top_line(pts, GRID, 1.0, pl.literal.is_none()));
            labels.push(Label {
                pos: corner(b0, c0),
                text: format!("{}.{}", g.name, pl.name),
                color: [0.1, 0.22, 0.6, 1.0],
                offset: (3.0, 2.0),
                priority: 3,
            });
        }
    }
    // intersection points
    let mut count = 0;
    let point_labels = labels_on && planes.iter().map(|p| p.len().max(1)).product::<usize>() <= MAX_POINT_LABELS;
    let axis_planes: [Vec<(String, f64)>; 3] = std::array::from_fn(|a| {
        if planes[a].is_empty() {
            vec![("_".to_string(), 0.0)]
        } else {
            planes[a].iter().map(|pl| (pl.name.clone(), pl.value)).collect()
        }
    });
    'outer: for (nx, x) in &axis_planes[0] {
        for (ny, y) in &axis_planes[1] {
            for (nz, z) in &axis_planes[2] {
                if count >= 1000 {
                    break 'outer;
                }
                count += 1;
                let pnt = w(DVec3::new(*x, *y, *z));
                markers.push(Marker { pos: pnt, shape: MarkerShape::Dot, size: 2.5, color: GRID_POINT, on_top: true });
                if point_labels {
                    labels.push(Label {
                        pos: pnt,
                        text: format!("{}[{nx},{ny},{nz}]", g.name),
                        color: [0.1, 0.18, 0.45, 1.0],
                        offset: (4.0, 3.0),
                        priority: 1,
                    });
                }
            }
        }
    }
    count
}

fn camera_json(c: &CameraInfo) -> Json {
    let (proj, fov) = match c.projection {
        Projection::Ortho => ("ortho", None),
        Projection::Perspective { fov_deg } => ("perspective", Some(fov_deg)),
    };
    let v = |d: DVec3| {
        json!([
            num(d.x).parse::<f64>().unwrap_or(d.x),
            num(d.y).parse::<f64>().unwrap_or(d.y),
            num(d.z).parse::<f64>().unwrap_or(d.z)
        ])
    };
    json!({
        "projection": proj, "fov_deg": fov,
        "azimuth": num(c.azimuth).parse::<f64>().unwrap_or(c.azimuth),
        "elevation": num(c.elevation).parse::<f64>().unwrap_or(c.elevation),
        "dir": v(c.dir), "up": v(c.up), "right": v(c.right), "target": v(c.target), "eye": v(c.eye),
        "scale_px_per_mm": c.scale, "width_mm": c.width_mm, "height_mm": c.height_mm,
    })
}

/// Renders the evaluated model `m` with `plan` into `output`; (JSON fields, text line).
pub(crate) fn render_model(m: &mut Model, plan: &Plan, output: &Path) -> Result<(Json, String), Fail> {
    let Some(sc) = m.scene.clone() else { return Err(Fail::Usage("keine Szene (Syntaxfehler)".into())) };
    let built = build(m, &sc, plan)?;
    let items: Vec<Item> = built
        .meshes
        .iter()
        .map(|(mesh, col, w, edges, cap)| Item::new(mesh, *col).with_transform(*w).with_edges(*edges).with_cap(*cap))
        .collect();
    let scene =
        Scene { items, lines: built.lines.clone(), markers: built.markers.clone(), labels: built.labels.clone() };
    let fit = match &plan.fit {
        None => None,
        Some(spec) => {
            let nodes = target_nodes(&sc, spec)?;
            nodes_bounds(&sc, &nodes).map(|b| (DVec3::from_array(b.min.0), DVec3::from_array(b.max.0)))
        }
    };
    let n = plan.views.len();
    let cols = match n {
        1 => 1,
        2 | 4 => 2,
        _ => 3,
    };
    let rows = n.div_ceil(cols);
    let gap = if plan.sheet { 8 } else { 0 };
    let (tw, th) = if plan.sheet {
        let tw = (plan.size.0.saturating_sub(gap * (cols as u32 + 1)) / cols as u32).max(16);
        let th = (plan.size.1.saturating_sub(gap * (rows as u32 + 1)) / rows as u32).max(16);
        (tw, th)
    } else {
        plan.size
    };
    let base = RenderOptions {
        width: tw,
        height: th,
        axes: plan.axes,
        projection: plan.projection,
        target: plan.target,
        fit,
        zoom: plan.zoom,
        section: plan.section,
        ..Default::default()
    };
    // Contact sheet: one target (centre of the fitted geometry) and the smallest fitting scale.
    let mut common = base.clone();
    if plan.sheet {
        let pts_box = fit.or_else(|| {
            let b = built.subject.as_ref()?;
            let all = nodes_bounds(&sc, &sc.root.into_iter().collect::<Vec<_>>()).unwrap_or(b.clone());
            let u = union(Some(all), b)?;
            Some((DVec3::from_array(u.min.0), DVec3::from_array(u.max.0)))
        });
        if common.target.is_none() {
            common.target = pts_box.map(|(lo, hi)| (lo + hi) / 2.0);
        }
        let s = plan
            .views
            .iter()
            .map(|(_, v)| cad_render::fit_scale(&scene, &RenderOptions { view: *v, ..common.clone() }))
            .fold(f64::INFINITY, f64::min);
        if s.is_finite() {
            common.scale = Some(s);
        }
    }
    let mut tiles = Vec::new();
    let mut views_json = Vec::new();
    for (k, (name, view)) in plan.views.iter().enumerate() {
        let title = if plan.sheet {
            Some(match name.as_str() {
                "iso" => "iso".to_string(),
                "front" => "front (von -y)".into(),
                "back" => "back (von +y)".into(),
                "top" => "top (von +z)".into(),
                "bottom" => "bottom (von -z)".into(),
                "left" => "left (von -x)".into(),
                "right" => "right (von +x)".into(),
                other => other.to_string(),
            })
        } else {
            None
        };
        let opts = RenderOptions { view: *view, title, ..common.clone() };
        let (img, cam) = render_info(&scene, &opts);
        let (x, y) = if plan.sheet { tile_origin(k, cols, (tw, th), gap) } else { (0, 0) };
        views_json
            .push(json!({ "name": name, "x": x, "y": y, "width": tw, "height": th, "camera": camera_json(&cam) }));
        tiles.push(img);
    }
    let img = if plan.sheet { contact_sheet(&tiles, cols, gap, [0.96, 0.96, 0.97, 1.0]) } else { tiles.remove(0) };
    crate::host::write_file(output, &img.to_png()).map_err(|e| Fail::Usage(format!("{}: {e}", output.display())))?;

    let section = plan.section.map(|s| {
        json!({ "point": s.point.to_array(), "normal": s.normal.to_array(), "keeps": "(q - point) · normal <= 0",
                "method": "renderer clipping + cap" })
    });
    let j = json!({
        "output": output.display().to_string(),
        "width": img.width,
        "height": img.height,
        "view": plan.views[0].0,
        "views": views_json,
        "camera": views_json[0]["camera"].clone(),
        "line": plan.lines.first(),
        "lines": plan.lines,
        "nodes": plan.nodes,
        "parts": plan.parts,
        "highlighted": built.highlighted,
        "visible_nodes": built.visible,
        "triangles": built.triangles,
        "show": plan.show,
        "overlays": built.counts,
        "section": section,
        "xray": plan.xray,
        "bounds": bounds_json(&built.subject),
        "notes": built.notes,
    });
    let mut t = format!(
        "{}: {}x{}, {} {}",
        output.display(),
        img.width,
        img.height,
        if plan.sheet { "Ansichten" } else { "Ansicht" },
        plan.views.iter().map(|v| v.0.as_str()).collect::<Vec<_>>().join(", ")
    );
    if !plan.lines.is_empty() || !plan.nodes.is_empty() {
        let labels: Vec<String> =
            built.highlighted.iter().map(|&h| format!("[{}] {}", h, sc.nodes[h as usize].label)).collect();
        let what = plan.lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(", ");
        if labels.is_empty() {
            t.push_str(&format!(", Zeile {what}: keine Geometrie"));
        } else if what.is_empty() {
            t.push_str(&format!(", Knoten: {}", labels.join(", ")));
        } else {
            t.push_str(&format!(", Zeile {what}: {}", labels.join(", ")));
        }
    }
    if !plan.parts.is_empty() {
        t.push_str(&format!(", Bauteil {}", plan.parts.join(", ")));
    }
    if let Some(ctx) = &m.context {
        t.push_str(&format!(", Kontext Modul {ctx}"));
    }
    if let Some(s) = plan.section {
        t.push_str(&format!(", Schnitt durch {} Normale {}", vec3d(s.point), vec3d(s.normal)));
    }
    if plan.xray {
        t.push_str(", Röntgen");
    }
    if let Some(b) = &built.subject {
        let s: Vec<String> = (0..3).map(|i| num(b.max.0[i] - b.min.0[i])).collect();
        t.push_str(&format!(", Hülle {} mm", s.join(" × ")));
    }
    t.push('\n');
    for n in &built.notes {
        t.push_str(&format!("Hinweis: {n}\n"));
    }
    Ok((j, t))
}

pub(crate) fn vec3d(v: DVec3) -> String {
    format!("[{}, {}, {}]", num(v.x), num(v.y), num(v.z))
}

pub(crate) fn command(c: &Common, output: &Path, a: &ViewArgs) -> Result<Outcome, Fail> {
    let plan = Plan::from_args(a)?;
    let t0 = std::time::Instant::now();
    let mut m = load(c, plan.lines.first().copied(), a.whole_file, true)?;
    let load_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let mut j = m.base_json("render");
    j["line"] = json!(plan.lines.first());
    if m.scene.is_none() {
        let t = m.diagnostics_text();
        return Ok(Outcome { json: j, text: t, errors: true });
    }
    let t1 = std::time::Instant::now();
    let (fields, line) = render_model(&mut m, &plan, output)?;
    if let (Json::Object(dst), Json::Object(src)) = (&mut j, fields) {
        dst.extend(src);
    }
    let stats = m.scene.as_ref().map(|s| s.stats).unwrap_or_default();
    j["timings"] = json!({
        "evaluate_ms": load_ms, "eval_ms": stats.eval_ms, "kernel_ms": stats.kernel_ms,
        "render_ms": t1.elapsed().as_secs_f64() * 1000.0,
    });
    let mut t = m.diagnostics_text();
    t.push_str(&line);
    Ok(Outcome { json: j, text: t, errors: m.has_errors() })
}
