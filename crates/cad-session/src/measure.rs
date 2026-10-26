//! Measure mode (Plan 18): snapping and measurements on the exact B-rep, shared by the UI
//! (`ToEngine::Measure`) and `mcad measure` (references such as `face@[x,y,z]`).
//!
//! * The measure model of a node ([`MeasureModel`]) holds one [`cad_measure::Body`] per part:
//!   the neutral B-rep and a tessellation (display tolerance) in context coordinates. It is
//!   built on the first request for a node and kept until the evaluation changes.
//! * Sections: the kept half of each solid part (kernel boolean), its contours in the plane
//!   ([`cad_measure::Section`]); kept per plane.
//! * The engine keeps no pick state: the picks come with each request and go back with the
//!   answer (like the sketch editor). Picks name their element by index (body, edge, face,
//!   contour, segment, region) and carry the `mcad measure` reference that finds it again.
//! * Provenance of a pick: the code lines (face tags → nodes), named instances, and grid
//!   points, lines and planes or anchors the element lies on.

use std::rc::Rc;

use cad_kernel::brep::Surface;
use cad_kernel::{Kernel, MassProps};
use cad_measure::measure::{arc_points, curve_name};
use cad_measure::section::{SectionPlane, cut_contours};
use cad_measure::{Body, EdgeCurve, Geo, Radius, Section};
use glam::{DMat4, DVec3};

use cad_i18n::{tr, trs};

use crate::Session;
use crate::protocol::{self as p, FromEngine, MeasureKind};

/// The measurable bodies of one node (context coordinates).
pub struct MeasureModel {
    pub node: u32,
    pub version: u64,
    pub bodies: Vec<Body>,
    /// Per body: index of the engine part, mass properties (solids).
    pub parts: Vec<(usize, Option<MassProps>)>,
    /// Chordal tolerance of the tessellation.
    pub tol: f64,
    /// Offset of each body's face ids in the node's display mesh (`MeshData.tri_faces`).
    face_offsets: Vec<u32>,
}

impl MeasureModel {
    /// Body and face index of a face id of the node's display mesh.
    pub fn mesh_face(&self, id: u32) -> Option<(usize, usize)> {
        let b = self.face_offsets.iter().rposition(|&o| o <= id)?;
        let f = self.bodies[b].face_by_id(id - self.face_offsets[b])?;
        Some((b, f))
    }

    /// Diagonal of all bodies.
    pub fn size(&self) -> f64 {
        self.bodies.iter().map(|b| b.size).fold(0.0, f64::max).max(1e-6)
    }
}

/// Node and plane (bit patterns of point and normal).
type SectionKey = (u32, [u64; 6]);

#[derive(Default)]
pub(crate) struct MeasureCache {
    version: u64,
    models: Vec<(u32, Rc<MeasureModel>)>,
    sections: Vec<(SectionKey, Rc<Result<Section, String>>)>,
}

fn plane_key(p: &SectionPlane) -> [u64; 6] {
    let a = p.point.to_array();
    let b = p.normal.to_array();
    [a[0].to_bits(), a[1].to_bits(), a[2].to_bits(), b[0].to_bits(), b[1].to_bits(), b[2].to_bits()]
}

fn v(p: p::Vec3) -> DVec3 {
    DVec3::from_array(p.0)
}

fn pv(p: DVec3) -> p::Vec3 {
    p::Vec3(p.to_array())
}

/// Number text: at most 4 decimals, no trailing zeros, never `-0` (as the CLI).
pub fn num(x: f64) -> String {
    let r = (x * 1e4).round() / 1e4;
    let s = format!("{r:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// `[x, y, z]` with [`num`].
pub fn vec_text(p: DVec3) -> String {
    format!("[{}, {}, {}]", num(p.x), num(p.y), num(p.z))
}

/// Coordinates of a reference (`face@[x,y,z]`): 6 decimals, no spaces.
fn ref_point(p: DVec3) -> String {
    let f = |x: f64| {
        let r = (x * 1e6).round() / 1e6;
        let s = format!("{r:.6}");
        let s = s.trim_end_matches('0').trim_end_matches('.');
        if s == "-0" { "0".to_string() } else { s.to_string() }
    };
    format!("[{},{},{}]", f(p.x), f(p.y), f(p.z))
}

/// An element of a pick, resolved against the model (or section).
#[derive(Clone, Copy)]
enum Elem<'a> {
    Point(DVec3),
    Edge(&'a Body, usize),
    Face(&'a Body, usize),
    Body(usize),
    Seg(&'a Section, usize, usize),
    Region(&'a Section, usize),
}

impl<'a> Elem<'a> {
    fn geo(&self) -> Option<Geo<'a>> {
        Some(match *self {
            Elem::Point(p) => Geo::Point(p),
            Elem::Edge(b, e) => Geo::Curve(&b.edges[e]),
            Elem::Face(b, f) => Geo::Face { body: b, face: f },
            Elem::Seg(s, c, g) => Geo::Curve(&s.contours[c].segs[g].curve),
            Elem::Body(_) | Elem::Region(..) => return None,
        })
    }
}

/// What a pick kind is, for labels.
fn kind_label(kind: &str) -> &'static str {
    match kind {
        "vertex" | "section_vertex" => trs!("Eckpunkt", "vertex"),
        "mid" | "section_mid" => trs!("Kantenmitte", "edge midpoint"),
        "center" | "section_center" => trs!("Kreismitte", "circle centre"),
        "edge" => trs!("Kante", "edge"),
        "section_edge" => trs!("Kontur", "contour"),
        "face" => trs!("Fläche", "face"),
        "body" => trs!("Körper", "body"),
        "region" => trs!("Schnittfläche", "section region"),
        _ => trs!("Punkt", "point"),
    }
}

/// Short description of a curve (`Kreis R 3`, `Gerade 12 mm`).
fn curve_text(c: &EdgeCurve) -> String {
    match c {
        EdgeCurve::Arc { r, .. } => format!("{} R {}", curve_name(c), num(*r)),
        EdgeCurve::Ellipse { r1, r2, .. } => tr!("Ellipse {} × {}", "ellipse {} × {}", num(*r1), num(*r2)),
        _ => format!("{} {} mm", curve_name(c), num(c.length())),
    }
}

fn surface_text(s: &Surface) -> String {
    match s {
        Surface::Cylinder { r, .. } => tr!("Zylinder R {}", "cylinder R {}", num(*r)),
        Surface::Sphere { r, .. } => tr!("Kugel R {}", "sphere R {}", num(*r)),
        Surface::Cone { semi_angle, .. } => tr!("Kegel {}°", "cone {}°", num(semi_angle.to_degrees())),
        Surface::Torus { major, minor, .. } => tr!("Torus R {} / {}", "torus R {} / {}", num(*major), num(*minor)),
        s => cad_measure::surface::kind_name(s).to_string(),
    }
}

/// Which pick kinds a measurement accepts.
fn accepts(kind: MeasureKind, section: bool, pick: &str) -> bool {
    use MeasureKind::*;
    match (kind, section) {
        (Distance, false) => matches!(pick, "vertex" | "mid" | "center" | "edge" | "face" | "point"),
        (Distance, true) => {
            matches!(pick, "section_vertex" | "section_mid" | "section_center" | "section_edge" | "point")
        }
        (Radius | Angle | Length, false) => matches!(pick, "edge" | "face") && !(kind == Length && pick == "face"),
        (Radius | Angle | Length, true) => pick == "section_edge",
        (Area, false) => pick == "face",
        (Area, true) => pick == "region",
        (Body, false) => matches!(pick, "face" | "body"),
        (Body, true) => false,
        (Wall, true) => matches!(pick, "section_edge" | "section_vertex" | "section_mid" | "point"),
        (Wall, false) => false,
    }
}

fn needed(kind: MeasureKind) -> usize {
    match kind {
        MeasureKind::Distance | MeasureKind::Angle => 2,
        _ => 1,
    }
}

fn title(kind: MeasureKind) -> &'static str {
    match kind {
        MeasureKind::Distance => trs!("Abstand", "Distance"),
        MeasureKind::Radius => trs!("Radius", "Radius"),
        MeasureKind::Angle => trs!("Winkel", "Angle"),
        MeasureKind::Length => trs!("Länge", "Length"),
        MeasureKind::Area => trs!("Fläche", "Area"),
        MeasureKind::Body => trs!("Körper", "Body"),
        MeasureKind::Wall => trs!("Wandstärke", "Wall thickness"),
    }
}

/// What the measurement asks for next.
pub fn prompt(kind: MeasureKind, section: bool, picks: usize) -> String {
    use MeasureKind::*;
    let s = match (kind, section, picks) {
        (Distance, false, 0) => trs!(
            "Abstand: erstes Element wählen (Eckpunkt, Kantenmitte, Kreismitte, Kante oder Fläche)",
            "Distance: pick the first element (vertex, edge midpoint, circle centre, edge or face)"
        ),
        (Distance, false, _) => trs!("Abstand: zweites Element wählen", "Distance: pick the second element"),
        (Distance, true, 0) => trs!(
            "Abstand im Schnitt: ersten Punkt oder Konturabschnitt wählen",
            "Distance in the section: pick the first point or contour segment"
        ),
        (Distance, true, _) => trs!(
            "Abstand im Schnitt: zweiten Punkt oder Konturabschnitt wählen",
            "Distance in the section: pick the second point or contour segment"
        ),
        (Radius, false, _) => {
            trs!("Radius: Kreiskante oder runde Fläche wählen", "Radius: pick a circular edge or round face")
        }
        (Radius, true, _) => {
            trs!("Radius im Schnitt: Kreisbogen der Kontur wählen", "Radius in the section: pick an arc of the contour")
        }
        (Angle, false, 0) => trs!("Winkel: erste Kante oder Fläche wählen", "Angle: pick the first edge or face"),
        (Angle, false, _) => trs!("Winkel: zweite Kante oder Fläche wählen", "Angle: pick the second edge or face"),
        (Angle, true, 0) => {
            trs!(
                "Winkel im Schnitt: ersten Konturabschnitt wählen",
                "Angle in the section: pick the first contour segment"
            )
        }
        (Angle, true, _) => trs!(
            "Winkel im Schnitt: zweiten Konturabschnitt wählen",
            "Angle in the section: pick the second contour segment"
        ),
        (Length, false, _) => trs!("Länge: Kante wählen", "Length: pick an edge"),
        (Length, true, _) => {
            trs!("Länge im Schnitt: Konturabschnitt wählen", "Length in the section: pick a contour segment")
        }
        (Area, false, _) => trs!("Fläche: Fläche wählen", "Area: pick a face"),
        (Area, true, _) => trs!(
            "Fläche im Schnitt: in einen Bereich der Schnittfläche klicken",
            "Area in the section: click into a region of the section"
        ),
        (Body, false, _) => trs!(
            "Körper: eine Fläche des Körpers wählen (Hülle, Volumen)",
            "Body: pick a face of the body (bounds, volume)"
        ),
        (Body, true, _) => trs!(
            "Körper: nicht im Schnitt – „Im Schnitt“ ausschalten",
            "Body: not in the section – turn off “In section”"
        ),
        (Wall, true, _) => trs!(
            "Wandstärke: Punkt auf der Schnittkontur wählen",
            "Wall thickness: pick a point on the section contour"
        ),
        (Wall, false, _) => trs!(
            "Wandstärke: nur im Schnitt – „Im Schnitt“ einschalten",
            "Wall thickness: only in the section – turn on “In section”"
        ),
    };
    s.to_string()
}

/// A section plane from the protocol.
pub fn plane_of(s: &p::SectionPlaneRef) -> SectionPlane {
    SectionPlane::new(v(s.point), v(s.normal))
}

/// `--section …` arguments of a plane (like `mcad render`).
pub fn section_args(s: &SectionPlane) -> Vec<String> {
    let n = s.normal;
    for (i, a) in ["x", "y", "z"].iter().enumerate() {
        let d = n[i];
        if (d.abs() - 1.0).abs() < 1e-12 {
            let val = num(s.point[i]);
            let mut v = vec!["--section".to_string(), format!("{a}={val}")];
            if d < 0.0 {
                v.push("--section-flip".into());
            }
            return v;
        }
    }
    let q = s.point;
    vec!["--section-plane".into(), [q.x, q.y, q.z, n.x, n.y, n.z].iter().map(|x| num(*x)).collect::<Vec<_>>().join(",")]
}

/// Polylines of a body's face boundary.
fn face_lines(b: &Body, f: usize) -> Vec<Vec<p::Vec3>> {
    b.faces[f].edges.iter().map(|&e| b.polys[e as usize].iter().map(|q| pv(*q)).collect()).collect()
}

fn poly(c: &EdgeCurve) -> Vec<p::Vec3> {
    c.polyline(64).into_iter().map(pv).collect()
}

/// Box wireframe (12 edges).
fn box_lines(lo: DVec3, hi: DVec3) -> Vec<Vec<p::Vec3>> {
    let c = |i: u32| {
        DVec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    };
    let mut out = Vec::new();
    for i in 0..8u32 {
        for bit in [1u32, 2, 4] {
            if i & bit == 0 {
                out.push(vec![pv(c(i)), pv(c(i | bit))]);
            }
        }
    }
    out
}

impl<K: Kernel> Session<K>
where
    K::Shape: 'static,
{
    fn measure_cache(&mut self) -> &mut MeasureCache {
        let v = self.done.as_ref().map_or(0, |d| d.version);
        if self.measure_cache.version != v {
            self.measure_cache = MeasureCache { version: v, ..Default::default() };
        }
        &mut self.measure_cache
    }

    /// The measure model of a node of the last finished evaluation (built once per
    /// evaluation and node).
    pub fn measure_model(&mut self, node: u32) -> Result<Rc<MeasureModel>, String> {
        if let Some((_, m)) = self.measure_cache().models.iter().find(|(n, _)| *n == node) {
            return Ok(m.clone());
        }
        let d = self.done.as_ref().ok_or_else(crate::no_evaluation)?;
        let sn = d
            .scene
            .nodes
            .get(node as usize)
            .ok_or_else(|| tr!("Knoten {node} existiert nicht", "node {node} does not exist"))?;
        if sn.dim == Some(p::Dim::Two) {
            return Err(tr!(
                "2D-Ergebnis: Messen nur an Körpern (im Schnitt oder nach dem Extrudieren)",
                "2D result: measuring works on bodies only (in a section or after extruding)"
            ));
        }
        let world = DMat4::from_cols_array(&sn.display_world.0);
        let tol = self.tolerance(node, false);
        let d = self.done.as_ref().expect("done");
        let parts = self.engine.measure_parts(&d.ev, node, &world, tol)?;
        // display mesh: face ids of part i follow those of the parts before (max id + 1)
        let mut bodies = Vec::new();
        let mut infos = Vec::new();
        let mut offs = Vec::new();
        let mut off = 0u32;
        for mp in parts {
            let max = mp.mesh.tri_faces.iter().max().copied();
            bodies.push(Body::new(mp.brep.clone(), &mp.mesh, &DMat4::IDENTITY, tol, mp.solid));
            infos.push((mp.part, mp.mass));
            offs.push(off);
            if let Some(m) = max {
                off += m + 1;
            }
        }
        let version = self.done.as_ref().map_or(0, |d| d.version);
        let m = Rc::new(MeasureModel { node, version, bodies, parts: infos, tol, face_offsets: offs });
        let c = self.measure_cache();
        if c.models.len() > 8 {
            c.models.remove(0);
        }
        c.models.push((node, m.clone()));
        Ok(m)
    }

    /// The section of a node in `plane` (kernel boolean per solid part; kept per plane).
    pub fn measure_section(&mut self, node: u32, plane: &SectionPlane) -> Rc<Result<Section, String>> {
        let key = (node, plane_key(plane));
        if let Some((_, s)) = self.measure_cache().sections.iter().find(|(k, _)| *k == key) {
            return s.clone();
        }
        let res = (|| {
            let model = self.measure_model(node)?;
            let d = self.done.as_ref().ok_or_else(crate::no_evaluation)?;
            let sn = &d.scene.nodes[node as usize];
            let world = DMat4::from_cols_array(&sn.display_world.0);
            let cuts = self.engine.section_parts(&d.ev, node, &world, plane.point, plane.normal)?;
            let mut parts = Vec::new();
            for (part, brep) in cuts {
                let Some(brep) = brep else { continue };
                let body = model.parts.iter().position(|(pi, _)| *pi == part).unwrap_or(0);
                parts.push(cut_contours(&brep, plane, body));
            }
            let s = Section::new(*plane, parts);
            if s.is_empty() {
                return Err(tr!("Die Schnittebene trifft das Modell nicht", "the section plane misses the model"));
            }
            Ok(s)
        })();
        let res = Rc::new(res);
        let c = self.measure_cache();
        if c.sections.len() > 6 {
            c.sections.remove(0);
        }
        c.sections.push((key, res.clone()));
        res
    }

    /// Session tag of an engine tag (the scene's numbering).
    fn session_tag(&self, t: u32) -> u32 {
        if self.tags_identity { t } else { self.tags.get(&t).copied().unwrap_or(t) }
    }

    /// Provenance of faces with these (engine) tags and of a point/plane/axis.
    fn origin(
        &self,
        tags: &[u32],
        point: Option<DVec3>,
        plane: Option<(DVec3, DVec3)>,
        axis: Option<(DVec3, DVec3)>,
    ) -> Vec<String> {
        self.origin_of(tags, point, plane, axis, &|_| true)
    }

    /// [`Self::origin`] with only the nodes `keep` accepts (nodes of equal content share a
    /// tag; the assertion checks know which subtree a body comes from).
    pub(crate) fn origin_of(
        &self,
        tags: &[u32],
        point: Option<DVec3>,
        plane: Option<(DVec3, DVec3)>,
        axis: Option<(DVec3, DVec3)>,
        keep: &dyn Fn(u32) -> bool,
    ) -> Vec<String> {
        let Some(d) = &self.done else { return Vec::new() };
        let sc = &d.scene;
        let mut out: Vec<String> = Vec::new();
        let mut push = |s: String| {
            if !out.contains(&s) {
                out.push(s);
            }
        };
        let mut lines = Vec::new();
        for &t in tags {
            let st = self.session_tag(t);
            let Some(tn) = sc.tag_nodes.iter().find(|x| x.tag == st) else { continue };
            for &n in tn.nodes.iter().filter(|&&n| keep(n)).take(4) {
                let node = &sc.nodes[n as usize];
                let file = node
                    .range
                    .file
                    .and_then(|f| sc.files.get(f as usize))
                    .filter(|f| f.id != 0)
                    .map(|f| format!("{}:", f.path));
                let key = (node.range.file, node.range.line);
                if lines.contains(&key) {
                    continue;
                }
                lines.push(key);
                let mut s =
                    tr!("{}Zeile {}: {}", "{}line {}: {}", file.unwrap_or_default(), node.range.line, node.label);
                // the named instance the node is in (or that the node is)
                let mut inst = Some(node.module_inst);
                let mut names = Vec::new();
                while let Some(i) = inst {
                    let Some(x) = sc.instances.get(i as usize) else { break };
                    if let Some(l) = &x.label {
                        names.push(format!("{l}: {}", x.module));
                    }
                    inst = x.parent;
                }
                if let Some(l) = &node.instance {
                    names.insert(0, l.clone());
                }
                if !names.is_empty() {
                    s.push_str(&tr!(" (Instanz {})", " (instance {})", names.join(" › ")));
                }
                push(s);
            }
        }
        // grid points / lines / planes and anchors
        for g in &sc.grids {
            let f = DMat4::from_cols_array(&g.frame.0);
            let inv = f.inverse();
            let names_at = |q: DVec3| -> [Option<String>; 3] {
                let l = inv.transform_point3(q);
                let mut r: [Option<String>; 3] = [None, None, None];
                for (a, slot) in r.iter_mut().enumerate() {
                    let on_axis: Vec<&p::Plane> = g.planes.iter().filter(|pl| pl.axis == a as u8).collect();
                    if on_axis.is_empty() {
                        if l[a].abs() <= 1e-6 * (1.0 + l[a].abs()) {
                            *slot = Some("_".into());
                        }
                        continue;
                    }
                    if let Some(pl) = on_axis.iter().find(|pl| (pl.value - l[a]).abs() <= 1e-6 * (1.0 + pl.value.abs()))
                    {
                        *slot = Some(pl.name.clone());
                    }
                }
                r
            };
            if let Some(q) = point {
                let r = names_at(q);
                if r.iter().all(|x| x.is_some()) && !r.iter().all(|x| x.as_deref() == Some("_")) {
                    push(tr!(
                        "Rasterpunkt {}[{}]",
                        "grid point {}[{}]",
                        g.name,
                        r.iter().map(|x| x.clone().unwrap()).collect::<Vec<_>>().join(", ")
                    ));
                }
            }
            if let Some((o, n)) = plane {
                let ln = inv.transform_vector3(n).normalize_or_zero();
                for a in 0..3 {
                    if (ln[a].abs() - 1.0).abs() < 1e-9 {
                        let r = names_at(o);
                        if let Some(name) = &r[a]
                            && name != "_"
                        {
                            push(tr!("Rasterebene {}.{}", "grid plane {}.{}", g.name, name));
                        }
                    }
                }
            }
            if let Some((o, dir)) = axis {
                let ld = inv.transform_vector3(dir).normalize_or_zero();
                for a in 0..3 {
                    if (ld[a].abs() - 1.0).abs() < 1e-9 {
                        let r = names_at(o);
                        let others: Vec<usize> = (0..3).filter(|&b| b != a).collect();
                        if others.iter().all(|&b| r[b].is_some()) {
                            let parts: Vec<String> = r
                                .iter()
                                .enumerate()
                                .map(|(b, x)| if b == a { "*".to_string() } else { x.clone().unwrap_or_default() })
                                .collect();
                            if parts.iter().any(|x| x != "*" && x != "_") {
                                push(tr!("Rasterlinie {}[{}]", "grid line {}[{}]", g.name, parts.join(", ")));
                            }
                        }
                    }
                }
            }
        }
        if let Some(q) = point {
            for a in &sc.anchors {
                if DVec3::from_array(a.point.0).distance(q) <= 1e-6 * (1.0 + q.length()) {
                    push(tr!("Anker {}", "anchor {}", a.text.clone().unwrap_or_else(|| a.name.clone())));
                }
            }
        }
        out
    }

    fn pick_base(&self, kind: &str, node: u32, body: usize, point: DVec3) -> p::MeasurePick {
        p::MeasurePick {
            kind: kind.into(),
            node,
            body: body as u32,
            edge: None,
            face: None,
            contour: None,
            segment: None,
            region: None,
            point: pv(point),
            label: kind_label(kind).into(),
            reference: String::new(),
            origin: Vec::new(),
            lines: Vec::new(),
        }
    }

    fn pick_vertex(&self, m: &MeasureModel, b: usize, vi: usize) -> p::MeasurePick {
        let body = &m.bodies[b];
        let q = body.brep.vertices[vi];
        let mut pk = self.pick_base("vertex", m.node, b, q);
        let tags: Vec<u32> = body.vertex_edges(vi as u32).iter().flat_map(|&e| body.edge_tags(e)).collect::<Vec<_>>();
        pk.reference = format!("vertex@{}", ref_point(q));
        pk.origin = self.origin(&tags, Some(q), None, None);
        pk.label = tr!("Eckpunkt {}", "vertex {}", vec_text(q));
        pk
    }

    fn pick_edge(&self, m: &MeasureModel, b: usize, e: usize, kind: &str, at: DVec3) -> p::MeasurePick {
        let body = &m.bodies[b];
        let c = &body.edges[e];
        let (point, label, reference) = match kind {
            "mid" => {
                let q = c.mid();
                (q, tr!("Kantenmitte · {}", "edge midpoint · {}", curve_text(c)), format!("mid@{}", ref_point(q)))
            }
            "center" => {
                let (o, _, r, _) = c.circle().expect("circle");
                (
                    o,
                    tr!("Kreismitte · R {} · ⌀ {}", "circle centre · R {} · ⌀ {}", num(r), num(2.0 * r)),
                    format!("center@{}", ref_point(o)),
                )
            }
            _ => {
                let q = c.closest(at).1;
                (q, tr!("Kante · {}", "edge · {}", curve_text(c)), format!("edge@{}", ref_point(q)))
            }
        };
        let mut pk = self.pick_base(kind, m.node, b, point);
        pk.edge = Some(e as u32);
        pk.label = label;
        pk.reference = reference;
        pk.lines = vec![body.polys[e].iter().map(|q| pv(*q)).collect()];
        let tags = body.edge_tags(e as u32);
        let axis = c.circle().map(|(o, z, _, _)| (o, z)).or_else(|| c.line_dir().map(|d| (c.start(), d)));
        pk.origin = self.origin(&tags, matches!(kind, "mid" | "center").then_some(point), None, axis);
        pk
    }

    fn pick_face(&self, m: &MeasureModel, b: usize, f: usize, at: DVec3, kind: &str) -> p::MeasurePick {
        let body = &m.bodies[b];
        let fg = &body.faces[f];
        let (q, _) = body.face_closest(f, at);
        let mut pk = self.pick_base(kind, m.node, b, q);
        pk.face = Some(f as u32);
        if kind == "body" {
            pk.label = match self.part_of_face(body, Some(f)) {
                Some((name, _, _)) => tr!("Bauteil {name}", "part {name}"),
                None => tr!("Körper {}", "body {}", b + 1),
            };
            pk.reference = format!("body@{}", ref_point(q));
        } else {
            pk.label = tr!("Fläche · {}", "face · {}", surface_text(&fg.surface));
            pk.reference = format!("face@{}", ref_point(q));
        }
        pk.lines = face_lines(body, f);
        let plane = match &fg.surface {
            Surface::Plane { frame } => Some((frame.origin, frame.z)),
            _ => None,
        };
        pk.origin = self.origin(&[fg.tag], None, plane, cad_measure::surface::axis(&fg.surface));
        pk
    }

    fn pick_seg(&self, m: &MeasureModel, sec: &Section, c: usize, s: usize, kind: &str, at: DVec3) -> p::MeasurePick {
        let seg = &sec.contours[c].segs[s];
        let (point, label, reference) = match kind {
            "section_vertex" => {
                let q = seg.curve.start();
                (q, tr!("Eckpunkt der Kontur {}", "contour vertex {}", vec_text(q)), format!("vertex@{}", ref_point(q)))
            }
            "section_mid" => {
                let q = seg.curve.mid();
                (q, tr!("Mitte · {}", "midpoint · {}", curve_text(&seg.curve)), format!("mid@{}", ref_point(q)))
            }
            "section_center" => {
                let (o, r) = match &seg.curve {
                    EdgeCurve::Arc { frame, r, .. } => (frame.origin, *r),
                    EdgeCurve::Ellipse { frame, r1, .. } => (frame.origin, *r1),
                    _ => (seg.curve.mid(), 0.0),
                };
                (
                    o,
                    tr!("Kreismitte · R {} · ⌀ {}", "circle centre · R {} · ⌀ {}", num(r), num(2.0 * r)),
                    format!("center@{}", ref_point(o)),
                )
            }
            _ => {
                let q = seg.curve.closest(at).1;
                (
                    q,
                    tr!(
                        "Kontur · {} ({})",
                        "contour · {} ({})",
                        curve_text(&seg.curve),
                        cad_measure::surface::kind_id_name(seg.source)
                    ),
                    format!("edge@{}", ref_point(q)),
                )
            }
        };
        let body = sec.contours[c].body;
        let mut pk = self.pick_base(kind, m.node, body, point);
        pk.contour = Some(c as u32);
        pk.segment = Some(s as u32);
        pk.label = label;
        pk.reference = reference;
        pk.lines = vec![poly(&seg.curve)];
        let axis = seg.curve.circle().map(|(o, z, _, _)| (o, z));
        pk.origin = self.origin(
            &[seg.tag],
            matches!(kind, "section_vertex" | "section_mid" | "section_center").then_some(point),
            None,
            axis,
        );
        pk
    }

    fn pick_region(&self, m: &MeasureModel, sec: &Section, r: usize, at: DVec3) -> p::MeasurePick {
        let reg = &sec.regions[r];
        let q = sec.plane.project(at);
        let mut pk = self.pick_base("region", m.node, reg.body, q);
        pk.region = Some(r as u32);
        pk.label = tr!("Schnittfläche {} ({} Löcher)", "section region {} ({} holes)", r + 1, reg.holes.len());
        pk.reference = format!("region@{}", ref_point(q));
        pk.lines = std::iter::once(reg.outer)
            .chain(reg.holes.iter().copied())
            .flat_map(|c| sec.contours[c].segs.iter().map(|s| poly(&s.curve)))
            .collect();
        let tags: Vec<u32> = sec.contours[reg.outer].segs.iter().map(|s| s.tag).collect();
        pk.origin = self.origin(&tags, None, None, None);
        pk
    }

    /// The element under the mouse ray.
    #[allow(clippy::too_many_arguments)]
    fn snap(
        &self,
        m: &MeasureModel,
        sec: Option<&Section>,
        kind: MeasureKind,
        ray: &p::Ray,
        hit: Option<&p::MeasureHit>,
        pixel: f64,
        snap_px: f64,
        free: bool,
        clip: Option<&SectionPlane>,
    ) -> Option<p::MeasurePick> {
        let o = v(ray.origin);
        let d = v(ray.dir).try_normalize()?;
        let tol = |t: f64| snap_px.max(0.0) * pixel.max(0.0) * t.max(1e-6);
        let size = m.size();
        if let Some(sec) = sec {
            // in the plane
            let n = sec.plane.normal;
            let dn = d.dot(n);
            if dn.abs() < 1e-12 {
                return None;
            }
            let t = (sec.plane.point - o).dot(n) / dn;
            if t <= 0.0 {
                return None;
            }
            let q = o + d * t;
            let r = tol(t);
            let ok = |k: &str| accepts(kind, true, k);
            if kind == MeasureKind::Area {
                return sec.region_at(q).map(|ri| self.pick_region(m, sec, ri, q));
            }
            if !free {
                let mut best: Option<(f64, usize, usize, &str)> = None;
                for (ci, c) in sec.contours.iter().enumerate() {
                    for (si, s) in c.segs.iter().enumerate() {
                        let mut cand = vec![(s.curve.start(), "section_vertex")];
                        if s.curve.kind() != "circle" {
                            cand.push((s.curve.mid(), "section_mid"));
                        }
                        if let Some((ctr, _, _, _)) = s.curve.circle() {
                            cand.push((ctr, "section_center"));
                        } else if let EdgeCurve::Ellipse { frame, .. } = &s.curve {
                            cand.push((frame.origin, "section_center"));
                        }
                        for (pt, k) in cand {
                            if !ok(k) {
                                continue;
                            }
                            let dd = pt.distance(q);
                            if dd <= r && best.is_none_or(|b| dd < b.0) {
                                best = Some((dd, ci, si, k));
                            }
                        }
                    }
                }
                if let Some((_, ci, si, k)) = best {
                    return Some(self.pick_seg(m, sec, ci, si, k, q));
                }
            }
            if ok("section_edge")
                && let Some((ci, si, _, _, dd)) = sec.closest(q)
            {
                let seg = &sec.contours[ci].segs[si];
                let fits = match kind {
                    MeasureKind::Radius => matches!(seg.curve, EdgeCurve::Arc { .. } | EdgeCurve::Ellipse { .. }),
                    MeasureKind::Angle => seg.curve.line_dir().is_some() || seg.curve.circle().is_some(),
                    _ => true,
                };
                if dd <= r && fits {
                    return Some(self.pick_seg(m, sec, ci, si, "section_edge", q));
                }
            }
            return None;
        }
        let clip_ok = |q: DVec3| clip.is_none_or(|c| c.dist(q) <= 1e-6 * size);
        // depth of the hit: candidates behind it are hidden
        let hit_face = hit.filter(|h| h.node == m.node).and_then(|h| {
            let hp = v(h.point);
            let bf =
                m.mesh_face(h.face).filter(|&(b, f)| m.bodies[b].mesh_closest(f, hp).0 <= 4.0 * m.tol + 1e-6 * size);
            let bf = bf.or_else(|| {
                // the hint did not fit: nearest face of any body
                let mut best: Option<(f64, usize, usize)> = None;
                for (bi, b) in m.bodies.iter().enumerate() {
                    for fi in 0..b.faces.len() {
                        let dd = b.mesh_closest(fi, hp).0;
                        if best.is_none_or(|x| dd < x.0) {
                            best = Some((dd, bi, fi));
                        }
                    }
                }
                best.filter(|x| x.0 <= 4.0 * m.tol + 1e-6 * size).map(|x| (x.1, x.2))
            });
            bf.map(|(b, f)| (b, f, hp))
        });
        let t_hit = hit_face.map(|(_, _, hp)| (hp - o).dot(d));
        let visible = |q: DVec3| {
            let t = (q - o).dot(d);
            t > 0.0 && t_hit.is_none_or(|th| t <= th + tol(th).max(2.0 * m.tol)) && clip_ok(q)
        };
        let ray_dist = |q: DVec3| {
            let w = q - o;
            let t = w.dot(d);
            ((w - d * t).length(), t)
        };
        if !free {
            // points: vertices, edge midpoints, circle centres
            let mut best: Option<(f64, f64, usize, &str, usize)> = None;
            // nearer to the ray wins; (nearly) equal ones (coaxial circles): nearer to the eye
            let better = |score: f64, t: f64, best: &Option<(f64, f64, usize, &str, usize)>| {
                best.is_none_or(|x| score < x.0 - 0.02 || (score <= x.0 + 0.02 && t < x.1))
            };
            for (bi, b) in m.bodies.iter().enumerate() {
                let mut cands: Vec<(DVec3, &str, usize)> = Vec::new();
                if accepts(kind, false, "vertex") {
                    cands.extend(b.brep.vertices.iter().enumerate().map(|(vi, q)| (*q, "vertex", vi)));
                }
                for (ei, c) in b.edges.iter().enumerate() {
                    if accepts(kind, false, "mid") && c.kind() != "circle" {
                        cands.push((c.mid(), "mid", ei));
                    }
                    if accepts(kind, false, "center")
                        && let Some((ctr, _, _, _)) = c.circle()
                    {
                        cands.push((ctr, "center", ei));
                    }
                }
                for (q, k, i) in cands {
                    let (rd, t) = ray_dist(q);
                    let score = rd / tol(t);
                    if rd <= tol(t) && visible(q) && better(score, t, &best) {
                        best = Some((score, t, bi, k, i));
                    }
                }
            }
            if let Some((_, _, bi, k, i)) = best {
                return Some(if k == "vertex" {
                    self.pick_vertex(m, bi, i)
                } else {
                    self.pick_edge(m, bi, i, k, DVec3::ZERO)
                });
            }
        }
        // edges near the ray
        if accepts(kind, false, "edge") {
            let mut best: Option<(f64, usize, usize, DVec3)> = None;
            for (bi, b) in m.bodies.iter().enumerate() {
                for (ei, c) in b.edges.iter().enumerate() {
                    // quick reject with the polyline
                    let near = b.polys[ei].windows(2).any(|w| {
                        let seg = EdgeCurve::Line { a: w[0], b: w[1] };
                        let (_, _, rd, t) = seg.closest_to_ray(o, d);
                        rd <= 3.0 * tol(t) + 2.0 * m.tol
                    });
                    if !near {
                        continue;
                    }
                    let (_, q, rd, t) = c.closest_to_ray(o, d);
                    if rd <= tol(t) && visible(q) && best.is_none_or(|x| rd / tol(t) < x.0) {
                        let fits = match kind {
                            MeasureKind::Radius => {
                                matches!(c, EdgeCurve::Arc { .. } | EdgeCurve::Ellipse { .. })
                                    || (!c.is_analytic()
                                        && cad_measure::measure::fit_circle(&c.sample(32), 1e-6).is_some())
                            }
                            _ => true,
                        };
                        if fits {
                            best = Some((rd / tol(t), bi, ei, q));
                        }
                    }
                }
            }
            if let Some((_, bi, ei, q)) = best {
                return Some(self.pick_edge(m, bi, ei, "edge", q));
            }
        }
        // the face under the mouse
        let (b, f, hp) = hit_face?;
        let fg = &m.bodies[b].faces[f];
        let fits = match kind {
            MeasureKind::Radius => !matches!(fg.surface, Surface::Plane { .. }),
            MeasureKind::Angle => !matches!(fg.surface, Surface::Sphere { .. }),
            _ => true,
        };
        if !fits {
            return None;
        }
        if accepts(kind, false, "face") {
            let k = if kind == MeasureKind::Body { "body" } else { "face" };
            return Some(self.pick_face(m, b, f, hp, k));
        }
        None
    }

    /// Element of a pick (indices checked).
    fn elem<'a>(&self, m: &'a MeasureModel, sec: Option<&'a Section>, pk: &p::MeasurePick) -> Result<Elem<'a>, String> {
        let stale = || tr!("Auswahl veraltet (Modell geändert): neu wählen", "stale pick (model changed): pick again");
        let b = pk.body as usize;
        match pk.kind.as_str() {
            "point" | "vertex" | "mid" | "center" | "section_vertex" | "section_mid" | "section_center" => {
                Ok(Elem::Point(v(pk.point)))
            }
            "edge" => {
                let body = m.bodies.get(b).ok_or_else(stale)?;
                let e = pk.edge.ok_or_else(stale)? as usize;
                if e >= body.edges.len() {
                    return Err(stale());
                }
                Ok(Elem::Edge(body, e))
            }
            "face" => {
                let body = m.bodies.get(b).ok_or_else(stale)?;
                let f = pk.face.ok_or_else(stale)? as usize;
                if f >= body.faces.len() {
                    return Err(stale());
                }
                Ok(Elem::Face(body, f))
            }
            "body" => {
                m.bodies.get(b).ok_or_else(stale)?;
                Ok(Elem::Body(b))
            }
            "section_edge" => {
                let s = sec.ok_or_else(|| tr!("Kontur ohne Schnitt", "contour without a section"))?;
                let (c, g) = (pk.contour.ok_or_else(stale)? as usize, pk.segment.ok_or_else(stale)? as usize);
                if s.contours.get(c).is_none_or(|x| g >= x.segs.len()) {
                    return Err(stale());
                }
                Ok(Elem::Seg(s, c, g))
            }
            "region" => {
                let s = sec.ok_or_else(|| tr!("Bereich ohne Schnitt", "region without a section"))?;
                let r = pk.region.ok_or_else(stale)? as usize;
                if r >= s.regions.len() {
                    return Err(stale());
                }
                Ok(Elem::Region(s, r))
            }
            k => Err(tr!("unbekannte Auswahl `{k}`", "unknown pick `{k}`")),
        }
    }

    /// Handles `ToEngine::Measure`.
    pub(crate) fn measure(&mut self, req: &p::MeasureRequest) -> Vec<FromEngine> {
        let version = self.done.as_ref().map_or(self.version, |d| d.version);
        let mut ans = p::MeasureAnswer {
            id: req.id,
            version,
            hover: None,
            picks: req.picks.clone(),
            result: None,
            prompt: prompt(req.kind, req.section.is_some(), req.picks.len()),
            message: None,
            section: None,
        };
        let done = |a: p::MeasureAnswer| vec![FromEngine::Measure(Box::new(a))];
        let Some(d) = &self.done else {
            ans.message = Some(crate::no_evaluation());
            return done(ans);
        };
        let node = req.hit.map(|h| h.node).or(req.node).or(d.scene.root);
        let Some(node) = node else {
            ans.message = Some(tr!("kein Ergebnis zum Messen", "no result to measure"));
            return done(ans);
        };
        // picks of another node: measure on the node of the first pick
        let node = req.picks.first().map_or(node, |p| p.node);
        let model = match self.measure_model(node) {
            Ok(m) => m,
            Err(e) => {
                ans.message = Some(e);
                return done(ans);
            }
        };
        let plane = req.section.as_ref().map(plane_of);
        let sec_rc = plane.as_ref().map(|pl| self.measure_section(node, pl));
        let sec = match &sec_rc {
            Some(rc) => match rc.as_ref() {
                Ok(s) => {
                    ans.section = Some(p::MeasureSectionInfo {
                        plane: req.section.unwrap(),
                        regions: s.regions.len() as u32,
                        contours: s.contours.len() as u32,
                        area: s.area(),
                        exact: s.exact,
                        max_dev: s.max_dev,
                        message: None,
                    });
                    Some(s)
                }
                Err(e) => {
                    ans.section = Some(p::MeasureSectionInfo {
                        plane: req.section.unwrap(),
                        regions: 0,
                        contours: 0,
                        area: 0.0,
                        exact: false,
                        max_dev: 0.0,
                        message: Some(e.clone()),
                    });
                    ans.message = Some(e.clone());
                    return done(ans);
                }
            },
            None => None,
        };
        if (req.kind == MeasureKind::Wall && sec.is_none()) || (req.kind == MeasureKind::Body && sec.is_some()) {
            ans.message = Some(prompt(req.kind, sec.is_some(), 0));
            return done(ans);
        }
        let Some(ray) = &req.ray else { return done(ans) };
        let clip = req.clip.as_ref().map(plane_of);
        let hover =
            self.snap(&model, sec, req.kind, ray, req.hit.as_ref(), req.pixel, req.snap_px, req.free, clip.as_ref());
        if req.click {
            let Some(pk) = hover else {
                ans.message = Some(tr!("Hier ist nichts zu wählen", "nothing to pick here"));
                return done(ans);
            };
            let mut picks = req.picks.clone();
            picks.push(pk);
            if picks.len() >= needed(req.kind) {
                match self.measurement(&model, sec, req.kind, &picks) {
                    Ok(mut r) => {
                        r.section = req.section;
                        ans.result = Some(r);
                        ans.picks = Vec::new();
                    }
                    Err(e) => {
                        ans.message = Some(e);
                        ans.picks = picks[..picks.len() - 1].to_vec();
                    }
                }
            } else {
                ans.picks = picks;
            }
            ans.prompt = prompt(req.kind, sec.is_some(), ans.picks.len());
        } else {
            ans.hover = hover;
        }
        done(ans)
    }

    /// `--module`/`-D` arguments of the context.
    fn context_args(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(m) = self.done.as_ref().and_then(|d| d.scene.context.module.clone()) {
            out.push("--module".into());
            out.push(m);
        }
        let overrides = match &self.context {
            cad_eval::Context::Module { .. } if out.is_empty() => &[][..],
            cad_eval::Context::Module { overrides, .. } | cad_eval::Context::File { overrides } => &overrides[..],
        };
        for (k, val) in overrides {
            out.push("-D".into());
            out.push(format!("{k}={}", crate::convert::value_text(&crate::convert::value(val))));
        }
        out
    }

    /// The measurement of complete picks.
    pub fn measurement(
        &mut self,
        m: &MeasureModel,
        sec: Option<&Section>,
        kind: MeasureKind,
        picks: &[p::MeasurePick],
    ) -> Result<p::Measurement, String> {
        for pk in picks {
            if !accepts(kind, sec.is_some(), &pk.kind) {
                return Err(tr!("{}: {} passt nicht", "{}: {} does not fit", title(kind), kind_label(&pk.kind)));
            }
        }
        if picks.len() < needed(kind) {
            return Err(prompt(kind, sec.is_some(), picks.len()));
        }
        let elems: Vec<Elem> = picks.iter().map(|pk| self.elem(m, sec, pk)).collect::<Result<_, _>>()?;
        let at: Vec<DVec3> = picks.iter().map(|pk| v(pk.point)).collect();
        let mut r = p::Measurement {
            kind,
            title: title(kind).into(),
            value: 0.0,
            unit: "mm".into(),
            label: String::new(),
            values: Vec::new(),
            lines: Vec::new(),
            guides: Vec::new(),
            points: Vec::new(),
            at: pv(at[0]),
            details: Vec::new(),
            picks: picks.to_vec(),
            exact: true,
            tol: None,
            section: None,
            text: String::new(),
            cli: Vec::new(),
        };
        let val = |key: &str, label: &str, value: f64, unit: &str| p::MeasureValue {
            key: key.into(),
            label: label.into(),
            value: cad_measure::num::clean(value),
            unit: unit.into(),
        };
        let flag = match kind {
            MeasureKind::Distance => "--distance",
            MeasureKind::Radius => "--radius",
            MeasureKind::Angle => "--angle",
            MeasureKind::Length => "--length",
            MeasureKind::Area => "--area",
            MeasureKind::Body => "--body",
            MeasureKind::Wall => "--wall",
        };
        match kind {
            MeasureKind::Distance => {
                let (ga, gb) = (
                    elems[0]
                        .geo()
                        .ok_or_else(|| tr!("Abstand: Element ohne Geometrie", "distance: element without geometry"))?,
                    elems[1]
                        .geo()
                        .ok_or_else(|| tr!("Abstand: Element ohne Geometrie", "distance: element without geometry"))?,
                );
                let dr = cad_measure::distance(&ga, &gb, m.tol)
                    .ok_or_else(|| tr!("Abstand: keine Geometrie", "distance: no geometry"))?;
                let delta = dr.b - dr.a;
                r.value = dr.distance;
                r.label = format!("{} mm", num(dr.distance));
                r.values.push(val(
                    "distance",
                    if dr.parallel { trs!("Normalabstand", "normal distance") } else { trs!("Abstand", "distance") },
                    dr.distance,
                    "mm",
                ));
                r.values.push(val("dx", "Δx", delta.x, "mm"));
                r.values.push(val("dy", "Δy", delta.y, "mm"));
                r.values.push(val("dz", "Δz", delta.z, "mm"));
                if dr.parallel {
                    r.details.push(tr!("parallele Ebenen: Normalabstand", "parallel planes: normal distance"));
                    if (dr.min_distance - dr.distance).abs() > 1e-9 {
                        r.values.push(val(
                            "min_distance",
                            trs!("kleinster Abstand der Flächen", "smallest distance of the faces"),
                            dr.min_distance,
                            "mm",
                        ));
                        r.details.push(tr!(
                            "kleinster Abstand der Flächen {} mm",
                            "smallest distance of the faces {} mm",
                            num(dr.min_distance)
                        ));
                    }
                }
                if let Some(ad) = dr.axis_distance {
                    r.values.push(val("axis_distance", trs!("Achsabstand", "axis distance"), ad, "mm"));
                    r.details.push(tr!("Achsabstand {} mm", "axis distance {} mm", num(ad)));
                }
                r.details.push(tr!(
                    "von {} bis {} · Δ {}",
                    "from {} to {} · Δ {}",
                    vec_text(dr.a),
                    vec_text(dr.b),
                    vec_text(delta)
                ));
                if dr.distance > 1e-12 {
                    r.lines.push(vec![pv(dr.a), pv(dr.b)]);
                }
                // parallel planes: the normal ends beside the second face; to its nearest point
                if dr.parallel && dr.b.distance(dr.min_b) > 1e-9 * (1.0 + dr.b.length()) {
                    r.guides.push(vec![pv(dr.b), pv(dr.min_b)]);
                }
                r.points = vec![pv(dr.a), pv(dr.b)];
                r.at = pv(0.5 * (dr.a + dr.b));
                r.exact = dr.exact;
                if !dr.exact {
                    r.tol = Some(dr.tol);
                }
            }
            MeasureKind::Radius => {
                let g = elems[0]
                    .geo()
                    .ok_or_else(|| tr!("Radius: Element ohne Geometrie", "radius: element without geometry"))?;
                let rr = cad_measure::radius(&g, at[0])?;
                let rad = rr.radius.clone();
                r.value = rad.r();
                let size = m.size();
                let axis_seg = |c: DVec3, z: DVec3, h: f64| vec![pv(c - z * h), pv(c + z * h)];
                match &rad {
                    Radius::Circle { centre, axis, r: rv, sweep_deg, at } => {
                        r.label = format!("R {} · ⌀ {}", num(*rv), num(2.0 * rv));
                        r.values.push(val("radius", trs!("Radius", "radius"), *rv, "mm"));
                        r.values.push(val("diameter", trs!("Durchmesser", "diameter"), 2.0 * rv, "mm"));
                        r.values.push(val("sweep", trs!("Bogenwinkel", "arc angle"), *sweep_deg, "°"));
                        r.lines.push(vec![pv(*centre), pv(*at)]);
                        r.guides.push(axis_seg(*centre, *axis, (0.4 * rv).max(0.02 * size)));
                        r.points.push(pv(*centre));
                        r.details.push(tr!(
                            "Mittelpunkt {} · Achse {}",
                            "centre {} · axis {}",
                            vec_text(*centre),
                            vec_text(*axis)
                        ));
                        if *sweep_deg < 359.999 {
                            r.details.push(tr!("Kreisbogen {}°", "arc {}°", num(*sweep_deg)));
                        }
                        r.at = pv(0.5 * (*centre + *at));
                    }
                    Radius::Ellipse { centre, axis, r1, r2, at } => {
                        r.label = tr!("Ellipse {} × {}", "ellipse {} × {}", num(*r1), num(*r2));
                        r.values.push(val("radius", trs!("große Halbachse", "semi-major axis"), r1.max(*r2), "mm"));
                        r.values.push(val("minor", trs!("kleine Halbachse", "semi-minor axis"), r1.min(*r2), "mm"));
                        r.lines.push(vec![pv(*centre), pv(*at)]);
                        r.guides.push(axis_seg(*centre, *axis, 0.4 * r1.max(*r2)));
                        r.points.push(pv(*centre));
                        r.details.push(tr!(
                            "Mittelpunkt {} · Normale {}",
                            "centre {} · normal {}",
                            vec_text(*centre),
                            vec_text(*axis)
                        ));
                        r.at = pv(0.5 * (*centre + *at));
                    }
                    Radius::Cylinder { axis_a, axis_b, dir, r: rv, at, foot } => {
                        r.label = format!("R {} · ⌀ {}", num(*rv), num(2.0 * rv));
                        r.values.push(val("radius", trs!("Radius", "radius"), *rv, "mm"));
                        r.values.push(val("diameter", trs!("Durchmesser", "diameter"), 2.0 * rv, "mm"));
                        let ext = 0.15 * axis_a.distance(*axis_b) + 0.02 * size;
                        r.guides.push(vec![pv(*axis_a - *dir * ext), pv(*axis_b + *dir * ext)]);
                        r.lines.push(vec![pv(*foot), pv(*at)]);
                        r.points.push(pv(*foot));
                        r.details.push(tr!(
                            "Achse {} durch {} (Länge {} mm)",
                            "axis {} through {} (length {} mm)",
                            vec_text(*dir),
                            vec_text(*axis_a),
                            num(axis_a.distance(*axis_b))
                        ));
                        r.at = pv(0.5 * (*foot + *at));
                    }
                    Radius::Cone { axis_a, axis_b, dir, r: rv, semi_deg, apex, at, foot } => {
                        r.label = format!("R {} · ⌀ {} · {}°", num(*rv), num(2.0 * rv), num(*semi_deg));
                        r.values.push(val("radius", trs!("Radius an der Stelle", "radius at the point"), *rv, "mm"));
                        r.values.push(val(
                            "diameter",
                            trs!("Durchmesser an der Stelle", "diameter at the point"),
                            2.0 * rv,
                            "mm",
                        ));
                        r.values.push(val("semi_angle", trs!("Halbwinkel", "half angle"), *semi_deg, "°"));
                        let ext = 0.15 * axis_a.distance(*axis_b) + 0.02 * size;
                        r.guides.push(vec![pv(*axis_a - *dir * ext), pv(*axis_b + *dir * ext)]);
                        r.lines.push(vec![pv(*foot), pv(*at)]);
                        r.points.push(pv(*foot));
                        r.details.push(tr!(
                            "Achse {} · Spitze {} · Halbwinkel {}°",
                            "axis {} · apex {} · half angle {}°",
                            vec_text(*dir),
                            vec_text(*apex),
                            num(*semi_deg)
                        ));
                        r.at = pv(0.5 * (*foot + *at));
                    }
                    Radius::Sphere { centre, r: rv, at } => {
                        r.label = format!("R {} · ⌀ {}", num(*rv), num(2.0 * rv));
                        r.values.push(val("radius", trs!("Radius", "radius"), *rv, "mm"));
                        r.values.push(val("diameter", trs!("Durchmesser", "diameter"), 2.0 * rv, "mm"));
                        r.lines.push(vec![pv(*centre), pv(*at)]);
                        r.points.push(pv(*centre));
                        r.details.push(tr!("Kugel, Mittelpunkt {}", "sphere, centre {}", vec_text(*centre)));
                        r.at = pv(0.5 * (*centre + *at));
                    }
                    Radius::Torus { centre, axis, major, minor, tube, at } => {
                        r.label = tr!(
                            "R {} · ⌀ {} (Ring R {})",
                            "R {} · ⌀ {} (ring R {})",
                            num(*minor),
                            num(2.0 * minor),
                            num(*major)
                        );
                        r.values.push(val("radius", trs!("Radius (Rohr)", "radius (tube)"), *minor, "mm"));
                        r.values.push(val(
                            "diameter",
                            trs!("Durchmesser (Rohr)", "diameter (tube)"),
                            2.0 * minor,
                            "mm",
                        ));
                        r.values.push(val("major", trs!("Ringradius", "ring radius"), *major, "mm"));
                        r.lines.push(vec![pv(*tube), pv(*at)]);
                        r.guides.push(vec![pv(*centre), pv(*tube)]);
                        r.guides.push(axis_seg(*centre, *axis, (0.5 * major).max(0.02 * size)));
                        r.points.push(pv(*tube));
                        r.details.push(tr!(
                            "Mittelpunkt {} · Achse {} · Ringradius {} mm",
                            "centre {} · axis {} · ring radius {} mm",
                            vec_text(*centre),
                            vec_text(*axis),
                            num(*major)
                        ));
                        r.at = pv(0.5 * (*tube + *at));
                    }
                    Radius::Curvature { r_min, r_max, centre, at } => {
                        r.label = format!("R ≈ {}", num(*r_min));
                        r.values.push(val(
                            "radius",
                            trs!("Krümmungsradius (kleinster)", "radius of curvature (smallest)"),
                            *r_min,
                            "mm",
                        ));
                        if r_max.is_finite() {
                            r.values.push(val(
                                "radius_max",
                                trs!("Krümmungsradius (größter)", "radius of curvature (largest)"),
                                *r_max,
                                "mm",
                            ));
                        }
                        r.lines.push(vec![pv(*centre), pv(*at)]);
                        r.points.push(pv(*centre));
                        r.details.push(tr!(
                            "Freiformfläche: Hauptkrümmungsradien an der Stelle",
                            "freeform face: principal radii of curvature at the point"
                        ));
                        r.at = pv(0.5 * (*centre + *at));
                    }
                }
                r.details.insert(0, rr.source.to_string());
                r.exact = rr.exact;
                if !rr.exact {
                    r.tol = Some(if rr.dev > 0.0 { rr.dev } else { m.tol });
                    if rr.dev > 0.0 {
                        r.details.push(tr!(
                            "Kreis angepasst, Abweichung {} mm",
                            "circle fitted, deviation {} mm",
                            num_e(rr.dev)
                        ));
                    }
                }
            }
            MeasureKind::Angle => {
                let (ga, gb) = (
                    elems[0]
                        .geo()
                        .ok_or_else(|| tr!("Winkel: Element ohne Richtung", "angle: element without direction"))?,
                    elems[1]
                        .geo()
                        .ok_or_else(|| tr!("Winkel: Element ohne Richtung", "angle: element without direction"))?,
                );
                let a = cad_measure::angle(&ga, at[0], &gb, at[1])?;
                r.value = a.deg;
                r.unit = "°".into();
                r.label = format!("{}°", num(a.deg));
                r.values.push(val("angle", trs!("Winkel", "angle"), a.deg, "°"));
                if let Some(n) = a.normals {
                    r.values.push(val("normals", trs!("Winkel der Normalen", "angle of the normals"), n, "°"));
                }
                if let Some(o) = a.opening {
                    r.values.push(val("opening", trs!("Öffnungswinkel", "opening angle"), o, "°"));
                    r.details.push(tr!(
                        "Öffnungswinkel an der Schnittgeraden {}°",
                        "opening angle at the line of intersection {}°",
                        num(o)
                    ));
                }
                r.details.insert(0, format!("{} – {}", a.what[0], a.what[1]));
                if a.parallel {
                    r.details.push("parallel".into());
                }
                let ra = (at[0] - a.vertex).length();
                let rb = (at[1] - a.vertex).length();
                let size = m.size();
                let rad = (0.5 * ra.min(rb)).clamp(0.06 * size, 0.25 * size).max(1e-3);
                if !a.parallel {
                    let arc = arc_points(a.vertex, a.da, a.db, rad, 32);
                    r.at = pv(arc[arc.len() / 2] + (arc[arc.len() / 2] - a.vertex).normalize_or_zero() * 0.15 * rad);
                    r.lines.push(arc.into_iter().map(pv).collect());
                    r.guides.push(vec![pv(a.vertex), pv(a.vertex + a.da * rad * 1.25)]);
                    r.guides.push(vec![pv(a.vertex), pv(a.vertex + a.db * rad * 1.25)]);
                    r.points.push(pv(a.vertex));
                } else {
                    r.at = pv(0.5 * (at[0] + at[1]));
                }
            }
            MeasureKind::Length => {
                let c = match elems[0] {
                    Elem::Edge(b, e) => &b.edges[e],
                    Elem::Seg(s, c, g) => &s.contours[c].segs[g].curve,
                    _ => return Err(tr!("Länge: Kante wählen", "length: pick an edge")),
                };
                let l = c.length();
                r.value = l;
                r.label = format!("{} mm", num(l));
                r.values.push(val("length", trs!("Länge", "length"), l, "mm"));
                r.lines.push(poly(c));
                r.points = vec![pv(c.start()), pv(c.end())];
                r.at = pv(c.mid());
                r.details.push(tr!(
                    "{} von {} bis {}",
                    "{} from {} to {}",
                    curve_text(c),
                    vec_text(c.start()),
                    vec_text(c.end())
                ));
                if let Some((o, _, rv, sw)) = c.circle() {
                    r.details.push(tr!(
                        "Kreisbogen R {} · {}° · Mittelpunkt {}",
                        "arc R {} · {}° · centre {}",
                        num(rv),
                        num(sw.to_degrees()),
                        vec_text(o)
                    ));
                }
                r.exact = c.is_analytic();
                if !r.exact {
                    r.tol = Some(1e-9 * l.max(1.0));
                    r.details
                        .push(tr!("numerisch integriert (Gauß-Legendre)", "integrated numerically (Gauss-Legendre)"));
                }
            }
            MeasureKind::Area => match elems[0] {
                Elem::Face(b, f) => {
                    let (a, exact) = cad_measure::face_area(b, f);
                    r.value = a;
                    r.unit = "mm²".into();
                    r.label = format!("{} mm²", num(a));
                    r.values.push(val("area", trs!("Fläche", "area"), a, "mm²"));
                    r.lines = face_lines(b, f);
                    r.details.push(surface_text(&b.faces[f].surface));
                    r.exact = exact;
                    if !exact {
                        r.tol =
                            Some(m.tol * b.faces[f].edges.iter().map(|&e| b.edges[e as usize].length()).sum::<f64>());
                        r.details.push(tr!(
                            "aus der Tessellierung (Sehnentoleranz {} mm)",
                            "from the tessellation (chord tolerance {} mm)",
                            num(m.tol)
                        ));
                    }
                }
                Elem::Region(s, ri) => {
                    let reg = &s.regions[ri];
                    r.value = reg.area;
                    r.unit = "mm²".into();
                    r.label = format!("{} mm²", num(reg.area));
                    r.values.push(val(
                        "area",
                        trs!("Fläche des Schnittbereichs", "area of the section region"),
                        reg.area,
                        "mm²",
                    ));
                    r.values.push(val(
                        "section_area",
                        trs!("Schnittfläche gesamt", "total section area"),
                        s.area(),
                        "mm²",
                    ));
                    r.lines = r.picks[0].lines.clone();
                    r.details.push(tr!(
                        "Bereich {} von {} · {} Löcher · Schnitt gesamt {} mm²",
                        "region {} of {} · {} holes · section total {} mm²",
                        ri + 1,
                        s.regions.len(),
                        reg.holes.len(),
                        num(s.area())
                    ));
                    r.exact = s.exact;
                    if !s.exact {
                        r.tol = Some(s.max_dev.max(m.tol));
                    }
                }
                _ => return Err(tr!("Fläche: Fläche wählen", "area: pick a face")),
            },
            MeasureKind::Body => {
                let bi = match elems[0] {
                    Elem::Body(b) => b,
                    Elem::Face(..) => picks[0].body as usize,
                    _ => return Err(tr!("Körper: Fläche wählen", "body: pick a face")),
                };
                let face = picks[0].face.map(|f| f as usize);
                let part = self.part_of_face(&m.bodies[bi], face);
                // a part (named instance): all its bodies; else the picked body
                let (bodies, masses, what): (Vec<Body>, Vec<Option<MassProps>>, String) = match &part {
                    Some((name, pnode, line)) if *pnode != m.node => {
                        let pm = self.measure_model(*pnode)?;
                        (
                            pm.bodies.clone(),
                            pm.parts.iter().map(|x| x.1).collect(),
                            tr!("Bauteil {name} (Zeile {line})", "part {name} (line {line})"),
                        )
                    }
                    Some((name, _, line)) => (
                        vec![m.bodies[bi].clone()],
                        vec![m.parts[bi].1],
                        tr!("Bauteil {name} (Zeile {line})", "part {name} (line {line})"),
                    ),
                    None => (
                        vec![m.bodies[bi].clone()],
                        vec![m.parts[bi].1],
                        tr!("Körper {} des Ergebnisses", "body {} of the result", bi + 1),
                    ),
                };
                let mut lo = DVec3::splat(f64::INFINITY);
                let mut hi = DVec3::splat(f64::NEG_INFINITY);
                for b in &bodies {
                    if let Some((a, c)) = b.bounds() {
                        lo = lo.min(a);
                        hi = hi.max(c);
                    }
                }
                if !lo.x.is_finite() {
                    return Err(tr!("Körper ohne Geometrie", "body without geometry"));
                }
                let size = hi - lo;
                let solids: Vec<MassProps> = masses.iter().flatten().copied().collect();
                let mass = (!solids.is_empty()).then(|| {
                    let v: f64 = solids.iter().map(|x| x.volume).sum();
                    let a: f64 = solids.iter().map(|x| x.area).sum();
                    let c = if v.abs() > 0.0 {
                        solids.iter().map(|x| x.centroid * x.volume).sum::<DVec3>() / v
                    } else {
                        DVec3::ZERO
                    };
                    MassProps { volume: v, area: a, centroid: c }
                });
                r.value = mass.map_or(0.0, |x| x.volume);
                r.unit = "mm³".into();
                r.label = match mass {
                    Some(x) => format!("{} mm³ · {} × {} × {}", num(x.volume), num(size.x), num(size.y), num(size.z)),
                    None => format!("{} × {} × {} mm", num(size.x), num(size.y), num(size.z)),
                };
                if let Some(x) = mass {
                    r.values.push(val("volume", trs!("Volumen", "volume"), x.volume, "mm³"));
                    r.values.push(val("area", trs!("Oberfläche", "surface area"), x.area, "mm²"));
                }
                r.values.push(val("size_x", trs!("Größe x", "size x"), size.x, "mm"));
                r.values.push(val("size_y", trs!("Größe y", "size y"), size.y, "mm"));
                r.values.push(val("size_z", trs!("Größe z", "size z"), size.z, "mm"));
                r.details.push(what);
                r.details.push(tr!(
                    "Hülle {} – {} · Größe {} × {} × {} mm",
                    "bounds {} – {} · size {} × {} × {} mm",
                    vec_text(lo),
                    vec_text(hi),
                    num(size.x),
                    num(size.y),
                    num(size.z)
                ));
                if let Some(x) = mass {
                    r.details.push(tr!(
                        "Oberfläche {} mm² · Schwerpunkt {}",
                        "surface area {} mm² · centroid {}",
                        num(x.area),
                        vec_text(x.centroid)
                    ));
                }
                r.guides = box_lines(lo, hi);
                r.at = pv(hi);
                r.exact = bodies.iter().all(|b| b.faces.iter().all(|f| cad_measure::surface::is_analytic(&f.surface)));
                if !r.exact {
                    r.tol = Some(m.tol);
                }
                r.details.push(tr!(
                    "Volumen und Oberfläche aus dem Kernel (Integration ~1e-4 genau), Hülle aus der B-Rep",
                    "volume and surface area from the kernel (integration accurate to ~1e-4), bounds from the B-rep"
                ));
            }
            MeasureKind::Wall => {
                let s = sec.ok_or_else(|| tr!("Wandstärke nur im Schnitt", "wall thickness only in a section"))?;
                let w = s.wall(at[0]).ok_or_else(|| {
                    tr!(
                        "Wandstärke: kein Gegenüber in dieser Richtung",
                        "wall thickness: no opposite side in this direction"
                    )
                })?;
                r.value = w.thickness;
                r.label = format!("{} mm", num(w.thickness));
                r.values.push(val("wall", trs!("Wandstärke", "wall thickness"), w.thickness, "mm"));
                r.lines.push(vec![pv(w.from), pv(w.to)]);
                r.points = vec![pv(w.from), pv(w.to)];
                r.at = pv(0.5 * (w.from + w.to));
                r.details.push(tr!(
                    "senkrecht zur Kontur von {} bis {}",
                    "normal to the contour from {} to {}",
                    vec_text(w.from),
                    vec_text(w.to)
                ));
                r.exact = s.contours[w.contour].segs.iter().all(|g| g.exact);
                if !r.exact {
                    r.tol = Some(s.max_dev.max(m.tol));
                }
            }
        }
        // text and cli
        let names: Vec<String> = picks.iter().map(|pk| pk.label.clone()).collect();
        let mut text = format!("{} {}: {}", r.title, names.join(" – "), value_text(&r));
        if let Some(t) = r.tol {
            text.push_str(&format!(" (± {} mm)", num_e(t)));
        }
        for dl in &r.details {
            text.push_str(&format!("\n  {dl}"));
        }
        for pk in picks {
            for o in &pk.origin {
                text.push_str(&format!("\n  {}: {o}", kind_label(&pk.kind)));
            }
        }
        if let Some(s) = sec {
            text.push_str(&tr!("\n  im Schnitt {}", "\n  in section {}", section_args(&s.plane).join(" ")));
        }
        r.text = text;
        let mut cli = self.context_args();
        if let Some(s) = sec {
            cli.extend(section_args(&s.plane));
        }
        cli.push(flag.into());
        for pk in picks {
            cli.push(pk.reference.clone());
        }
        r.cli = cli;
        Ok(r)
    }

    /// The part (name, node, line) a face belongs to.
    fn part_of_face(&self, body: &Body, face: Option<usize>) -> Option<(String, u32, u32)> {
        let d = self.done.as_ref()?;
        let sc = &d.scene;
        let f = face?;
        let st = self.session_tag(body.faces[f].tag);
        let nodes = &sc.tag_nodes.iter().find(|x| x.tag == st)?.nodes;
        for pt in &sc.parts {
            for &n in nodes {
                let mut cur = Some(n);
                while let Some(c) = cur {
                    if c == pt.node {
                        return Some((pt.name.clone(), pt.node, pt.range.line));
                    }
                    cur = sc.nodes[c as usize].parent;
                }
            }
        }
        None
    }

    /// Resolves a `mcad measure` reference against the model (or the section): `vertex@P`,
    /// `mid@P`, `center@P`, `edge@P`, `face@P`, `body@P`, `region@P` (section), a coordinate
    /// `[x,y,z]`, an anchor or a grid point (`resolve_point`).
    pub fn measure_ref(
        &self,
        m: &MeasureModel,
        sec: Option<&Section>,
        text: &str,
        resolve_point: &dyn Fn(&str) -> Result<DVec3, String>,
    ) -> Result<p::MeasurePick, String> {
        let t = text.trim();
        let Some((pre, rest)) = t.split_once('@') else {
            let q = resolve_point(t)?;
            let mut pk = self.pick_base("point", m.node, 0, q);
            pk.label = tr!("Punkt {t}", "point {t}");
            pk.reference = t.to_string();
            pk.origin = self.origin(&[], Some(q), None, None);
            return Ok(pk);
        };
        let q = resolve_point(rest)?;
        let bad = |what: &str| format!("{t}: {what}");
        if let Some(s) = sec {
            let near = |pts: &mut dyn Iterator<Item = (usize, usize, DVec3)>| {
                pts.min_by(|a, b| a.2.distance(q).partial_cmp(&b.2.distance(q)).unwrap_or(std::cmp::Ordering::Equal))
            };
            let all = || {
                s.contours
                    .iter()
                    .enumerate()
                    .flat_map(|(ci, c)| c.segs.iter().enumerate().map(move |(si, g)| (ci, si, g)))
            };
            return match pre {
                "vertex" => {
                    let (ci, si, _) = near(&mut all().map(|(c, i, g)| (c, i, g.curve.start())))
                        .ok_or_else(|| bad(trs!("keine Kontur", "no contour")))?;
                    Ok(self.pick_seg(m, s, ci, si, "section_vertex", q))
                }
                "mid" => {
                    let (ci, si, _) = near(&mut all().map(|(c, i, g)| (c, i, g.curve.mid())))
                        .ok_or_else(|| bad(trs!("keine Kontur", "no contour")))?;
                    Ok(self.pick_seg(m, s, ci, si, "section_mid", q))
                }
                "center" => {
                    let (ci, si, _) = near(&mut all().filter_map(|(c, i, g)| match &g.curve {
                        EdgeCurve::Arc { frame, .. } | EdgeCurve::Ellipse { frame, .. } => Some((c, i, frame.origin)),
                        _ => None,
                    }))
                    .ok_or_else(|| bad(trs!("kein Kreisbogen in der Kontur", "no arc in the contour")))?;
                    Ok(self.pick_seg(m, s, ci, si, "section_center", q))
                }
                "edge" => {
                    let (ci, si, _, _, _) = s.closest(q).ok_or_else(|| bad(trs!("keine Kontur", "no contour")))?;
                    Ok(self.pick_seg(m, s, ci, si, "section_edge", q))
                }
                "region" => {
                    let ri = s.region_at(q).ok_or_else(|| {
                        bad(trs!(
                            "Punkt liegt in keinem Bereich des Schnitts",
                            "point lies in no region of the section"
                        ))
                    })?;
                    Ok(self.pick_region(m, s, ri, q))
                }
                _ => Err(bad(trs!(
                    "im Schnitt: vertex@, mid@, center@, edge@ oder region@",
                    "in a section: vertex@, mid@, center@, edge@ or region@"
                ))),
            };
        }
        match pre {
            "vertex" => {
                let mut best: Option<(f64, usize, usize)> = None;
                for (bi, b) in m.bodies.iter().enumerate() {
                    for (vi, p) in b.brep.vertices.iter().enumerate() {
                        let d = p.distance(q);
                        if best.is_none_or(|x| d < x.0) {
                            best = Some((d, bi, vi));
                        }
                    }
                }
                let (_, bi, vi) = best.ok_or_else(|| bad(trs!("keine Eckpunkte", "no vertices")))?;
                Ok(self.pick_vertex(m, bi, vi))
            }
            "mid" | "center" | "edge" => {
                let mut best: Option<(f64, usize, usize)> = None;
                for (bi, b) in m.bodies.iter().enumerate() {
                    for (ei, c) in b.edges.iter().enumerate() {
                        let p = match pre {
                            "mid" if c.kind() != "circle" => c.mid(),
                            "center" => match c.circle() {
                                Some((o, _, _, _)) => o,
                                None => continue,
                            },
                            "edge" => c.closest(q).1,
                            _ => continue,
                        };
                        let d = p.distance(q);
                        if best.is_none_or(|x| d < x.0) {
                            best = Some((d, bi, ei));
                        }
                    }
                }
                let (_, bi, ei) = best.ok_or_else(|| bad(trs!("keine passende Kante", "no matching edge")))?;
                Ok(self.pick_edge(m, bi, ei, pre, q))
            }
            "face" | "body" => {
                // nearest by the triangles; among (nearly) equal ones the exact surface decides
                let mut cands: Vec<(f64, usize, usize)> = Vec::new();
                for (bi, b) in m.bodies.iter().enumerate() {
                    for fi in 0..b.faces.len() {
                        cands.push((b.mesh_closest(fi, q).0, bi, fi));
                    }
                }
                let dmin = cands.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
                if !dmin.is_finite() {
                    return Err(bad(trs!("keine Flächen", "no faces")));
                }
                let (_, bi, fi) = cands
                    .into_iter()
                    .filter(|c| c.0 <= dmin + 2.0 * m.tol)
                    .map(|(_, bi, fi)| (m.bodies[bi].face_closest(fi, q).0.distance(q), bi, fi))
                    .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
                    .expect("candidate");
                Ok(self.pick_face(m, bi, fi, q, pre))
            }
            "region" => Err(bad(trs!("region@ braucht --section", "region@ needs --section"))),
            _ => Err(bad(trs!(
                "erwartet vertex@, mid@, center@, edge@, face@, body@ (oder region@ mit --section)",
                "expected vertex@, mid@, center@, edge@, face@, body@ (or region@ with --section)"
            ))),
        }
    }
}

/// Value of a measurement as text (`12.5 mm`, `45°`).
fn value_text(r: &p::Measurement) -> String {
    if r.unit == "°" { format!("{}°", num(r.value)) } else { format!("{} {}", num(r.value), r.unit) }
}

/// Small numbers in scientific notation (`2e-7`), else [`num`].
pub fn num_e(x: f64) -> String {
    if x != 0.0 && x.abs() < 1e-3 { format!("{x:.1e}") } else { num(x) }
}
