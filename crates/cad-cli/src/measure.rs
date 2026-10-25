//! `cad measure`: distances between points (anchors, grid points, coordinates), bounds,
//! volume and area of parts or lines, interference of all parts (kernel intersection), the
//! clearance of two parts (minimum distance of their tessellations), and measurements on the
//! exact B-rep (Plan 18, logic in `cad_session::measure` and `cad_measure`, shared with the
//! UI): minimum distance with foot points, radius/diameter, angle, length, area of a face,
//! body/part, and in a section plane (`--section`) the contours, their areas, circles and the
//! wall thickness.
//!
//! Elements are named by a point: `vertex@P`, `mid@P` (edge midpoint), `center@P` (circle
//! centre), `edge@P`, `face@P`, `body@P` take the element of the result nearest to P; with
//! `--section` the prefixes refer to the section contour (`region@P`: the region containing
//! P). Points as in `--between` (anchor, grid point, `[x,y,z]`). The UI copies its
//! measurements in this form ("Als cad measure kopieren").

use cad_kernel::{BoolOp, Kernel};
use cad_measure::section::SectionPlane;
use cad_measure::{EdgeCurve, Section};
use cad_session::measure::{num_e, vec_text};
use cad_session::protocol::{self as p, MeasureKind};
use clap::Args;
use glam::{DMat4, DVec3};
use serde_json::{Value as Json, json};

use crate::render::{bounds_json, nodes_bounds, parse_section, parse_vec, target_nodes, vec3d};
use crate::{Common, Fail, Model, Outcome, load, mat, num};

#[derive(Args, Clone, Debug)]
pub(crate) struct MeasureArgs {
    /// Distance of two points: anchor (`k.corner`, `k[2].corner`, in module context `corner`),
    /// grid point in context (`g[right, back, top]`, entries also numbers or `_`), coordinate
    /// `[x,y,z]`.
    #[arg(long, num_args = 2, value_names = ["A", "B"], allow_hyphen_values = true)]
    pub between: Vec<String>,
    /// Smallest distance of two elements with foot points, exact from the B-rep (parallel
    /// planes: normal distance): point as for --between or `vertex@P`, `mid@P`, `center@P`,
    /// `edge@P`, `face@P` (the element nearest P). With --section: elements of the section
    /// contour. Repeatable.
    #[arg(long, num_args = 2, value_names = ["A", "B"], allow_hyphen_values = true)]
    pub distance: Vec<String>,
    /// Radius/diameter of a circular edge (`edge@P`) or round face (`face@P`: cylinder, cone,
    /// sphere, torus, fillet) with centre and axis; with --section an arc of the contour.
    /// Repeatable.
    #[arg(long, value_name = "REF", allow_hyphen_values = true)]
    pub radius: Vec<String>,
    /// Angle between two edges or faces (`edge@P`, `face@P`). Repeatable.
    #[arg(long, num_args = 2, value_names = ["A", "B"], allow_hyphen_values = true)]
    pub angle: Vec<String>,
    /// Length of an edge (`edge@P`; with --section a contour segment). Repeatable.
    #[arg(long, value_name = "REF", allow_hyphen_values = true)]
    pub length: Vec<String>,
    /// Bounds (exact from the B-rep), volume and surface area of the body or part with the
    /// face nearest P (`body@P` or `face@P`). Repeatable.
    #[arg(long, value_name = "REF", allow_hyphen_values = true)]
    pub body: Vec<String>,
    /// What lies at P: nearest vertex, nearest edge, nearest face (with a reference to measure
    /// further); with --section the contour. Repeatable.
    #[arg(long, value_name = "P", allow_hyphen_values = true)]
    pub at: Vec<String>,
    /// Section plane for measuring in a section: `x=VALUE`, `y=VALUE`, `z=VALUE` (as `cad render
    /// --section`); alone: contours, areas, circles and smallest wall thickness of the section.
    #[arg(long, value_name = "x|y|z=VALUE", allow_hyphen_values = true)]
    pub section: Option<String>,
    /// Section at any plane: point and normal (the normal points into the removed half).
    #[arg(long, value_name = "PX,PY,PZ,NX,NY,NZ", allow_hyphen_values = true)]
    pub section_plane: Option<String>,
    /// Section: keep the other half (flip the normal).
    #[arg(long)]
    pub section_flip: bool,
    /// Wall thickness in the section at the contour point nearest P (across the contour). Needs
    /// --section. Repeatable.
    #[arg(long, value_name = "P", allow_hyphen_values = true)]
    pub wall: Vec<String>,
    /// Bounds (axis-aligned, context coordinates) of `all`, `part:NAME`, `line:N`, `node:I`.
    #[arg(long, num_args = 0..=1, default_missing_value = "all", value_name = "TARGET")]
    pub bounds: Option<String>,
    /// Volume in mm³ (target as for --bounds, default all).
    #[arg(long, num_args = 0..=1, default_missing_value = "all", value_name = "TARGET")]
    pub volume: Option<String>,
    /// Surface area in mm² (target as for --bounds, default all); `face@P`: one face of the
    /// B-rep, with --section `region@P`: one region of the section.
    #[arg(long, num_args = 0..=1, default_missing_value = "all", value_name = "TARGET")]
    pub area: Option<String>,
    /// Collisions: pairwise intersection volume of all parts (exact, via the kernel; parts with
    /// an imported mesh: mesh-based overlap test without volume).
    #[arg(long)]
    pub interference: bool,
    /// Report collisions from this intersection volume on (mm³).
    #[arg(long, default_value_t = 0.001)]
    pub tolerance: f64,
    /// Smallest distance of two parts (over the tessellation, the accuracy is reported).
    #[arg(long, num_args = 2, value_names = ["A", "B"])]
    pub clearance: Vec<String>,
    /// Chord tolerance of the tessellation for --clearance (and mesh-based --interference) in mm.
    #[arg(long, default_value_t = 0.01)]
    pub mesh_tol: f64,
    /// Colour regions: volume and mass (PLA) per colour and filament, per part.
    #[arg(long)]
    pub colors: bool,
}

/// A point reference resolved in context coordinates.
pub(crate) fn resolve_point(sc: &p::Scene, s: &str) -> Result<(DVec3, String), Fail> {
    let s = s.trim();
    if s.starts_with('[') {
        let v = parse_vec(s, 3, "Koordinate")?;
        return Ok((DVec3::new(v[0], v[1], v[2]), "Koordinate".into()));
    }
    // grid point g[a, b, c]
    if let Some(open) = s.find('[')
        && s.ends_with(']')
        && !s[..open].contains('.')
        && sc.grids.iter().any(|g| g.name == s[..open].trim())
    {
        let name = s[..open].trim();
        let entries: Vec<&str> = s[open + 1..s.len() - 1].split(',').map(|e| e.trim()).collect();
        if entries.len() != 3 {
            return Err(Fail::Usage(format!("{s}: Rasterpunkt braucht drei Einträge")));
        }
        let candidates: Vec<&p::Grid> = sc.grids.iter().filter(|g| g.name == name).collect();
        let g = candidates.iter().find(|g| g.in_context).copied().ok_or_else(|| {
            Fail::Usage(format!(
                "{s}: Raster `{name}` ist nicht im Kontext (Raster gelten nur in ihrem Modul: --module NAME)"
            ))
        })?;
        let mut local = DVec3::ZERO;
        for (a, e) in entries.iter().enumerate() {
            local[a] = if *e == "_" {
                0.0
            } else if let Ok(v) = e.parse::<f64>() {
                v
            } else {
                let pl = g.planes.iter().find(|pl| pl.axis == a as u8 && pl.name == *e).ok_or_else(|| {
                    let names: Vec<&str> =
                        g.planes.iter().filter(|pl| pl.axis == a as u8).map(|pl| pl.name.as_str()).collect();
                    Fail::Usage(format!(
                        "{s}: Ebene `{e}` gibt es auf Achse {} nicht (Ebenen: {})",
                        ["x", "y", "z"][a],
                        names.join(", ")
                    ))
                })?;
                pl.value
            };
        }
        return Ok((mat(&g.frame).transform_point3(local), "Rasterpunkt".into()));
    }
    // anchor: `k.ecke`, `k[2].ecke` or (module context) `ecke`
    let found = sc
        .anchors
        .iter()
        .find(|a| a.text.as_deref() == Some(s))
        .or_else(|| (!s.contains('.')).then(|| sc.anchors.iter().find(|a| a.inst == 0 && a.name == s)).flatten());
    match found {
        Some(a) => Ok((DVec3::from_array(a.point.0), "Anker".into())),
        None => {
            let known: Vec<String> =
                sc.anchors.iter().map(|a| a.text.clone().unwrap_or_else(|| a.name.clone())).collect();
            Err(Fail::Usage(format!(
                "Punkt `{s}` nicht gefunden: erwartet Anker ({}), Rasterpunkt g[a,b,c] im Kontext oder [x,y,z]",
                if known.is_empty() { "keine".into() } else { known.join(", ") }
            )))
        }
    }
}

fn mass(m: &mut Model, nodes: &[u32]) -> Result<(f64, f64), Fail> {
    let sc = m.scene.as_ref().expect("scene");
    let nodes: Vec<u32> = nodes
        .iter()
        .copied()
        .filter(|&n| {
            let node = &sc.nodes[n as usize];
            node.dim == Some(p::Dim::Three) && node.status != p::NodeStatus::Skipped
        })
        .collect();
    let (mut v, mut a) = (0.0, 0.0);
    for n in nodes {
        let mp = m.session.mass_props(n).map_err(Fail::Kernel)?;
        v += mp.volume;
        a += mp.area;
    }
    Ok((v, a))
}

/// Shapes of a node placed in context coordinates.
fn placed_shapes<K: Kernel>(
    k: &mut K,
    ev: &cad_engine::Evaluation<K::Shape>,
    node: u32,
) -> Result<Vec<K::Shape>, String>
where
    K::Shape: Clone,
{
    let w: DMat4 = ev.report(node).display_world;
    let mut out = Vec::new();
    for part in ev.parts(node) {
        if !part.solid {
            continue;
        }
        out.push(if w == DMat4::IDENTITY {
            part.shape.clone()
        } else {
            k.transform(&part.shape, &w).map_err(|e| e.to_string())?
        });
    }
    Ok(out)
}

/// Intersection volume of two nodes' solids (kernel boolean).
fn intersection_volume(m: &mut Model, a: u32, b: u32) -> Result<f64, String> {
    m.session
        .with_kernel(|k, ev| {
            let sa = placed_shapes(k, ev, a)?;
            let sb = placed_shapes(k, ev, b)?;
            let mut v = 0.0;
            for x in &sa {
                for y in &sb {
                    let i = k.boolean(BoolOp::Intersection, &[x.clone(), y.clone()], 0).map_err(|e| e.to_string())?;
                    if !k.is_empty(&i) {
                        v += k.mass_props(&i).map_err(|e| e.to_string())?.volume;
                    }
                }
            }
            Ok(v)
        })
        .unwrap_or_else(|| Err("keine Auswertung".into()))
}

/// Does the result of a node hold a mesh without B-rep (`import("x.stl")` as a mesh)?
fn has_mesh(m: &mut Model, n: u32) -> bool {
    m.session.with_kernel(|_, ev| !ev.meshes(n).is_empty()).unwrap_or(false)
}

/// Collision of two nodes: the exact intersection volume of their B-reps (kernel), or, if one
/// holds a mesh without B-rep, the mesh test on tessellations with chord tolerance `tol`
/// (overlap yes or no, volume unknown: `Err(overlap)`).
fn collision(m: &mut Model, sc: &p::Scene, a: u32, b: u32, tol: f64) -> Result<Result<f64, bool>, Fail> {
    if !has_mesh(m, a) && !has_mesh(m, b) {
        return intersection_volume(m, a, b).map(Ok).map_err(Fail::Kernel);
    }
    let mut tris = |n: u32| -> Result<Vec<cad_measure::dist::Tri>, Fail> {
        let mesh = m.session.mesh_tol(n, tol).map_err(Fail::Kernel)?;
        Ok(cad_measure::dist::triangles(&mesh, &mat(&sc.nodes[n as usize].display_world)))
    };
    let (ta, tb) = (tris(a)?, tris(b)?);
    Ok(Err(cad_measure::dist::solids_overlap(&ta, &tb)))
}

fn overlap(a: &p::Bounds, b: &p::Bounds, eps: f64) -> bool {
    (0..3).all(|i| a.min.0[i] <= b.max.0[i] + eps && b.min.0[i] <= a.max.0[i] + eps)
}

fn part_node(sc: &p::Scene, name: &str) -> Result<u32, Fail> {
    target_nodes(sc, &format!("part:{}", name.strip_prefix("part:").unwrap_or(name))).map(|n| n[0])
}

/// Text of a section plane (`z = 10, entfernt z > 10`).
fn plane_text(s: &SectionPlane) -> String {
    let n = s.normal;
    for (i, a) in ["x", "y", "z"].iter().enumerate() {
        if (n[i].abs() - 1.0).abs() < 1e-12 {
            let v = num(s.point[i]);
            return format!("{a} = {v} (entfernt {a} {} {v})", if n[i] > 0.0 { ">" } else { "<" });
        }
    }
    format!("durch {} mit Normale {} (entfernt die Seite, in die sie zeigt)", vec_text(s.point), vec_text(n))
}

fn seg_json(c: &EdgeCurve) -> Json {
    let mut j = json!({
        "kind": c.kind(),
        "length": c.length(),
        "start": c.start().to_array(),
        "end": c.end().to_array(),
    });
    match c {
        EdgeCurve::Arc { frame, r, sweep, .. } => {
            j["center"] = json!(frame.origin.to_array());
            j["radius"] = json!(r);
            j["sweep_deg"] = json!(sweep.to_degrees());
        }
        EdgeCurve::Ellipse { frame, r1, r2, .. } => {
            j["center"] = json!(frame.origin.to_array());
            j["radii"] = json!([r1, r2]);
        }
        _ => {}
    }
    j
}

/// Summary of a section: regions with areas, contours with their segments, circles (bores),
/// smallest wall thickness (estimate over sample points).
fn section_summary(s: &Section) -> (Json, String) {
    let mut t = format!(
        "Schnitt {}: {} Bereich(e), Fläche {} mm²{}\n",
        plane_text(&s.plane),
        s.regions.len(),
        num(s.area()),
        if s.exact && s.max_dev == 0.0 {
            " (exakt: Konturen aus Geraden und Kreisen der B-Rep)".to_string()
        } else if s.exact {
            format!(" (exakt in geschlossener Form; die Kernel-Kurven weichen bis {} mm ab)", num_e(s.max_dev))
        } else {
            " (Konturen teils als Kernel-Kurven)".to_string()
        }
    );
    let mut regions = Vec::new();
    for (ri, r) in s.regions.iter().enumerate() {
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        for sg in &s.contours[r.outer].segs {
            let (a, b) = sg.curve.bounds();
            lo = lo.min(a);
            hi = hi.max(b);
        }
        let contours: Vec<Json> = std::iter::once(r.outer)
            .chain(r.holes.iter().copied())
            .map(|ci| {
                let c = &s.contours[ci];
                json!({
                    "hole": ci != r.outer,
                    "area": c.area.abs(),
                    "segments": c.segs.iter().map(|g| {
                        let mut j = seg_json(&g.curve);
                        j["exact"] = json!(g.exact);
                        j["source"] = json!(g.source);
                        j["dev"] = json!(g.dev);
                        j
                    }).collect::<Vec<_>>(),
                })
            })
            .collect();
        t.push_str(&format!(
            "  Bereich {}: {} mm², Hülle {} – {}, {} Loch/Löcher\n",
            ri + 1,
            num(r.area),
            vec_text(lo),
            vec_text(hi),
            r.holes.len()
        ));
        regions.push(json!({
            "area": r.area, "holes": r.holes.len(), "body": r.body,
            "bounds": { "min": lo.to_array(), "max": hi.to_array() },
            "reference": format!("region@[{},{},{}]", num(0.5 * (lo.x + hi.x)), num(0.5 * (lo.y + hi.y)), num(0.5 * (lo.z + hi.z))),
            "contours": contours,
        }));
    }
    // circles: equal centre and radius counted once
    let mut circles: Vec<(DVec3, f64, bool, usize)> = Vec::new();
    for c in &s.contours {
        for g in &c.segs {
            if let Some((o, _, r, sw)) = g.curve.circle() {
                let full = sw >= std::f64::consts::TAU - 1e-9;
                match circles
                    .iter_mut()
                    .find(|x| x.0.distance(o) < 1e-9 * (1.0 + r) && (x.1 - r).abs() < 1e-9 * (1.0 + r))
                {
                    Some(x) => x.2 |= full,
                    None => circles.push((o, r, full, 0)),
                }
            }
        }
    }
    for (o, r, full, _) in &circles {
        t.push_str(&format!(
            "  {} R {} · ⌀ {} bei {} (center@{})\n",
            if *full { "Kreis" } else { "Bogen" },
            num(*r),
            num(2.0 * r),
            vec_text(*o),
            vec_text(*o).replace(' ', "")
        ));
    }
    let min_wall = s.min_wall(8);
    if let Some(w) = &min_wall {
        t.push_str(&format!(
            "  kleinste Wandstärke ≈ {} mm von {} bis {} (8 Stellen je Konturabschnitt; genau: --wall P)\n",
            num(w.thickness),
            vec_text(w.from),
            vec_text(w.to)
        ));
    }
    let j = json!({
        "plane": { "point": s.plane.point.to_array(), "normal": s.plane.normal.to_array() },
        "method": "kernel intersection with the kept half space; contour segments in closed form on analytic cut surfaces",
        "exact": s.exact,
        "max_dev": s.max_dev,
        "area": s.area(),
        "regions": regions,
        "circles": circles.iter().map(|(o, r, full, _)| json!({ "center": o.to_array(), "radius": r, "diameter": 2.0 * r, "full": full })).collect::<Vec<_>>(),
        "min_wall": min_wall.map(|w| json!({ "thickness": w.thickness, "from": w.from.to_array(), "to": w.to.to_array(), "samples_per_segment": 8 })),
    });
    (j, t)
}

/// Rounds every number of a JSON value to 12 significant digits (no `1.9999999999999991`).
fn clean_json(j: &mut Json) {
    match j {
        Json::Number(n) => {
            if let Some(f) = n.as_f64()
                && n.is_f64()
            {
                let c = cad_measure::num::clean(f);
                if let Some(x) = serde_json::Number::from_f64(c) {
                    *n = x;
                }
            }
        }
        Json::Array(a) => a.iter_mut().for_each(clean_json),
        Json::Object(o) => o.values_mut().for_each(clean_json),
        _ => {}
    }
}

/// Measurements on the exact geometry (`--distance`, `--radius`, `--angle`, `--length`,
/// `--area face@…`, `--body`, `--wall`, `--at`, `--section`).
fn exact_measures(m: &mut Model, sc: &p::Scene, a: &MeasureArgs, j: &mut Json, t: &mut String) -> Result<(), Fail> {
    let plane = parse_section(a.section.as_deref(), a.section_plane.as_deref(), a.section_flip)?
        .map(|s| SectionPlane::new(s.point, s.normal));
    let area_ref = a.area.as_deref().filter(|s| s.contains('@'));
    let any = !(a.distance.is_empty()
        && a.radius.is_empty()
        && a.angle.is_empty()
        && a.length.is_empty()
        && a.body.is_empty()
        && a.wall.is_empty()
        && a.at.is_empty()
        && area_ref.is_none());
    if !any && plane.is_none() {
        return Ok(());
    }
    if !a.wall.is_empty() && plane.is_none() {
        return Err(Fail::Usage("--wall braucht --section oder --section-plane".into()));
    }
    let root = sc.root.ok_or_else(|| Fail::Usage("kein Ergebnis zum Messen".into()))?;
    let model = m.session.measure_model(root).map_err(Fail::Usage)?;
    let sec_rc = plane.as_ref().map(|pl| m.session.measure_section(root, pl));
    let sec = match &sec_rc {
        Some(rc) => match rc.as_ref() {
            Ok(s) => Some(s),
            Err(e) if e.contains("trifft") => return Err(Fail::Usage(e.clone())),
            Err(e) => return Err(Fail::Kernel(e.clone())),
        },
        None => None,
    };
    if let Some(s) = sec {
        let (mut sj, st) = section_summary(s);
        clean_json(&mut sj);
        j["section"] = sj;
        t.push_str(&st);
    }
    let resolve = |s: &str| {
        resolve_point(sc, s).map(|x| x.0).map_err(|e| match e {
            Fail::Usage(m) | Fail::Kernel(m) => m,
        })
    };
    let mut list: Vec<p::Measurement> = Vec::new();
    let mut run = |m: &mut Model, kind: MeasureKind, refs: &[String]| -> Result<(), Fail> {
        let picks = refs
            .iter()
            .map(|r| m.session.measure_ref(&model, sec, r, &resolve).map_err(Fail::Usage))
            .collect::<Result<Vec<_>, _>>()?;
        let mut r = m.session.measurement(&model, sec, kind, &picks).map_err(Fail::Usage)?;
        r.section = plane.map(|pl| p::SectionPlaneRef {
            point: p::Vec3(pl.point.to_array()),
            normal: p::Vec3(pl.normal.to_array()),
        });
        // highlight polylines are for the UI
        for pk in &mut r.picks {
            pk.lines.clear();
        }
        t.push_str(&r.text);
        t.push('\n');
        list.push(r);
        Ok(())
    };
    for pair in a.distance.chunks(2) {
        run(m, MeasureKind::Distance, pair)?;
    }
    for r in &a.radius {
        run(m, MeasureKind::Radius, std::slice::from_ref(r))?;
    }
    for pair in a.angle.chunks(2) {
        run(m, MeasureKind::Angle, pair)?;
    }
    for r in &a.length {
        run(m, MeasureKind::Length, std::slice::from_ref(r))?;
    }
    if let Some(r) = area_ref {
        run(m, MeasureKind::Area, &[r.to_string()])?;
    }
    for r in &a.body {
        // the body of the element at the point (`face@P` and a bare point name it as well)
        let r = match r.split_once('@') {
            Some((_, pt)) => format!("body@{pt}"),
            None => format!("body@{r}"),
        };
        run(m, MeasureKind::Body, &[r])?;
    }
    for w in &a.wall {
        run(m, MeasureKind::Wall, std::slice::from_ref(w))?;
    }
    if !list.is_empty() {
        let mut v = serde_json::to_value(&list).unwrap_or(Json::Null);
        clean_json(&mut v);
        j["measurements"] = v;
    }
    // what lies at a point
    let mut ats = Vec::new();
    for pt in &a.at {
        let q = resolve(pt).map_err(Fail::Usage)?;
        let kinds: &[&str] = if sec.is_some() {
            &["vertex", "mid", "center", "edge", "region"]
        } else {
            &["vertex", "mid", "center", "edge", "face"]
        };
        let mut found = Vec::new();
        t.push_str(&format!("Bei {}:\n", vec_text(q)));
        for k in kinds {
            let refs = format!("{k}@[{},{},{}]", q.x, q.y, q.z);
            let Ok(pk) = m.session.measure_ref(&model, sec, &refs, &resolve) else { continue };
            let d = DVec3::from_array(pk.point.0).distance(q);
            t.push_str(&format!("  {:<28} {} ({} mm entfernt)\n", pk.reference, pk.label, num(d)));
            for o in &pk.origin {
                t.push_str(&format!("  {:<28} {o}\n", ""));
            }
            found.push(json!({ "kind": pk.kind, "reference": pk.reference, "label": pk.label, "point": pk.point.0, "distance": d, "origin": pk.origin }));
        }
        ats.push(json!({ "point": q.to_array(), "elements": found }));
    }
    if !ats.is_empty() {
        let mut v = json!(ats);
        clean_json(&mut v);
        j["at"] = v;
    }
    Ok(())
}

pub(crate) fn command(c: &Common, a: &MeasureArgs) -> Result<Outcome, Fail> {
    let exact = !(a.distance.is_empty()
        && a.radius.is_empty()
        && a.angle.is_empty()
        && a.length.is_empty()
        && a.body.is_empty()
        && a.wall.is_empty()
        && a.at.is_empty()
        && a.section.is_none()
        && a.section_plane.is_none());
    let nothing = a.between.is_empty()
        && a.bounds.is_none()
        && a.volume.is_none()
        && a.area.is_none()
        && !a.interference
        && a.clearance.is_empty()
        && !a.colors
        && !exact;
    if nothing {
        return Err(Fail::Usage(
            "nichts zu messen: --distance A B, --radius R, --angle A B, --length R, --body R, --section …, --wall P, --at P, \
             --between A B, --bounds, --volume, --area, --interference, --clearance A B oder --colors"
                .into(),
        ));
    }
    let mut m = load(c, None, true, true)?;
    let mut j = m.base_json("measure");
    let mut t = m.diagnostics_text();
    let Some(sc) = m.scene.clone() else {
        return Ok(Outcome { json: j, text: t, errors: true, exit: None });
    };
    if a.colors {
        let (colors, regions, text) = crate::colors::table(&mut m, &sc, true)?;
        j["colors"] = json!({ "colors": colors, "parts": regions, "density_g_cm3": crate::colors::PLA_DENSITY });
        if text.is_empty() {
            t.push_str("Farben: keine (leeres Modell)\n");
        }
        t.push_str(&text);
    }
    if !a.between.is_empty() {
        let (pa, ka) = resolve_point(&sc, &a.between[0])?;
        let (pb, kb) = resolve_point(&sc, &a.between[1])?;
        let d = pb - pa;
        j["between"] = json!({
            "a": { "ref": a.between[0], "kind": ka, "point": pa.to_array() },
            "b": { "ref": a.between[1], "kind": kb, "point": pb.to_array() },
            "delta": d.to_array(),
            "distance": d.length(),
        });
        t.push_str(&format!(
            "Abstand {} {} → {} {}: {} mm (Δ {})\n",
            a.between[0],
            vec3d(pa),
            a.between[1],
            vec3d(pb),
            num(d.length()),
            vec3d(d)
        ));
    }
    if exact {
        exact_measures(&mut m, &sc, a, &mut j, &mut t)?;
    }
    if let Some(spec) = &a.bounds {
        let nodes = target_nodes(&sc, spec)?;
        let b = nodes_bounds(&sc, &nodes);
        j["bounds"] = json!({ "target": spec, "nodes": nodes, "bounds": bounds_json(&b) });
        match &b {
            Some(b) => {
                let s: Vec<String> = (0..3).map(|i| num(b.max.0[i] - b.min.0[i])).collect();
                t.push_str(&format!(
                    "Hülle {spec}: {} – {}, Größe {} mm\n",
                    crate::vec3(&b.min),
                    crate::vec3(&b.max),
                    s.join(" × ")
                ));
            }
            None => t.push_str(&format!("Hülle {spec}: keine Geometrie\n")),
        }
    }
    for (key, spec) in [("volume", &a.volume), ("area", &a.area)] {
        let Some(spec) = spec else { continue };
        if spec.contains('@') {
            if !exact {
                // `--area face@P` alone: measured on the B-rep
                exact_measures(&mut m, &sc, a, &mut j, &mut t)?;
            }
            continue;
        }
        let nodes = target_nodes(&sc, spec)?;
        let (v, ar) = mass(&mut m, &nodes)?;
        if key == "volume" {
            j["volume"] = json!({ "target": spec, "nodes": nodes, "volume": v });
            t.push_str(&format!("Volumen {spec}: {} mm³\n", num(v)));
        } else {
            j["area"] = json!({ "target": spec, "nodes": nodes, "area": ar });
            t.push_str(&format!("Fläche {spec}: {} mm²\n", num(ar)));
        }
    }
    if a.interference {
        let parts: Vec<&p::Part> = sc.parts.iter().filter(|pt| pt.bounds.is_some()).collect();
        let mut pairs = Vec::new();
        let mut collisions = Vec::new();
        let mut checked = 0;
        for i in 0..parts.len() {
            for k in i + 1..parts.len() {
                let (pa, pb) = (parts[i], parts[k]);
                let (ba, bb) = (pa.bounds.as_ref().unwrap(), pb.bounds.as_ref().unwrap());
                if !overlap(ba, bb, 1e-9) {
                    pairs.push(json!({ "a": pa.name, "b": pb.name, "bounds_overlap": false, "volume": 0.0 }));
                    continue;
                }
                checked += 1;
                match collision(&mut m, &sc, pa.node, pb.node, a.mesh_tol.max(1e-4))? {
                    Ok(v) => {
                        let hit = v > a.tolerance;
                        pairs.push(
                            json!({ "a": pa.name, "b": pb.name, "bounds_overlap": true, "volume": v, "collision": hit }),
                        );
                        if hit {
                            collisions.push(json!({ "a": pa.name, "b": pb.name, "volume": v }));
                            t.push_str(&format!("Kollision {} ↔ {}: {} mm³\n", pa.name, pb.name, num(v)));
                        }
                    }
                    // a mesh without B-rep: overlap yes/no, no volume
                    Err(hit) => {
                        pairs.push(json!({ "a": pa.name, "b": pb.name, "bounds_overlap": true, "volume": null,
                            "collision": hit, "method": "mesh" }));
                        if hit {
                            collisions.push(json!({ "a": pa.name, "b": pb.name, "volume": null, "method": "mesh" }));
                            t.push_str(&format!(
                                "Kollision {} ↔ {}: Netze überschneiden sich (netzbasiert, Volumen unbekannt)\n",
                                pa.name, pb.name
                            ));
                        }
                    }
                }
            }
        }
        if collisions.is_empty() {
            t.push_str(&format!(
                "Keine Kollisionen ({} Bauteile, {} Paare mit überlappender Hülle geprüft, Toleranz {} mm³)\n",
                parts.len(),
                checked,
                num(a.tolerance)
            ));
        }
        j["interference"] = json!({
            "parts": parts.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
            "tolerance": a.tolerance, "method": "kernel intersection (exact B-rep)",
            "pairs": pairs, "collisions": collisions,
        });
    }
    if !a.clearance.is_empty() {
        let na = part_node(&sc, &a.clearance[0])?;
        let nb = part_node(&sc, &a.clearance[1])?;
        let tol = a.mesh_tol.max(1e-4);
        let tris = |m: &mut Model, n: u32| -> Result<Vec<cad_measure::dist::Tri>, Fail> {
            let mesh = m.session.mesh_tol(n, tol).map_err(Fail::Kernel)?;
            Ok(cad_measure::dist::triangles(&mesh, &mat(&sc.nodes[n as usize].display_world)))
        };
        let (ta, tb) = (tris(&mut m, na)?, tris(&mut m, nb)?);
        let d =
            cad_measure::dist::min_distance(&ta, &tb).ok_or_else(|| Fail::Usage("Bauteil ohne Geometrie".into()))?;
        let (inter, collide) = match collision(&mut m, &sc, na, nb, tol)? {
            Ok(v) => (Some(v), v > a.tolerance),
            Err(hit) => (None, hit),
        };
        let dist = if collide { 0.0 } else { d.dist };
        j["clearance"] = json!({
            "a": a.clearance[0], "b": a.clearance[1],
            "distance": dist, "mesh_distance": d.dist,
            "point_a": d.a.to_array(), "point_b": d.b.to_array(),
            "intersection_volume": inter, "collision": collide,
            "accuracy": 2.0 * tol, "mesh_tol": tol, "triangles": [ta.len(), tb.len()],
            "method": "minimum triangle–triangle distance of both tessellations",
        });
        if collide {
            let how = inter.map_or_else(|| "netzbasiert".to_string(), |v| format!("{} mm³", num(v)));
            t.push_str(&format!(
                "Abstand {} ↔ {}: 0 mm – die Bauteile überschneiden sich ({how})\n",
                a.clearance[0], a.clearance[1],
            ));
        } else {
            t.push_str(&format!(
                "Abstand {} ↔ {}: {} mm (± {} mm, Tessellierung {} mm), zwischen {} und {}\n",
                a.clearance[0],
                a.clearance[1],
                num(d.dist),
                num(2.0 * tol),
                num(tol),
                vec3d(d.a),
                vec3d(d.b)
            ));
        }
    }
    // assertions (keep-out volumes are not part of the measured result)
    let (aj, at) = crate::asserts_summary(&sc, None);
    if !at.is_empty() {
        t.push_str("Asserts:\n");
        t.push_str(&at);
        j["asserts"] = aj;
    }
    let errors = m.has_errors();
    Ok(Outcome { json: j, text: t, errors, exit: None })
}
