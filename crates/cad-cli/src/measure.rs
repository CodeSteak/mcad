//! `cad measure`: distances between points (anchors, grid points, coordinates), bounds,
//! volume and area of parts or lines, interference of all parts (kernel intersection) and
//! the clearance of two parts (minimum distance of their tessellations).

use cad_kernel::{BoolOp, Kernel};
use cad_session::protocol as p;
use clap::Args;
use glam::{DMat4, DVec3};
use serde_json::json;

use crate::render::{bounds_json, nodes_bounds, parse_vec, target_nodes, vec3d};
use crate::{Common, Fail, Model, Outcome, geom, load, mat, num};

#[derive(Args, Clone, Debug)]
pub(crate) struct MeasureArgs {
    /// Abstand zweier Punkte: Anker (`k.ecke`, `k[2].ecke`, im Modulkontext `ecke`), Rasterpunkt
    /// im Kontext (`g[rechts, hinten, deckel]`, Einträge auch Zahlen oder `_`), Koordinate
    /// `[x,y,z]`.
    #[arg(long, num_args = 2, value_names = ["A", "B"], allow_hyphen_values = true)]
    pub between: Vec<String>,
    /// Hülle (achsparallel, Kontextkoordinaten) von `all`, `part:NAME`, `line:N`, `node:I`.
    #[arg(long, num_args = 0..=1, default_missing_value = "all", value_name = "ZIEL")]
    pub bounds: Option<String>,
    /// Volumen in mm³ (Ziel wie --bounds, Standard all).
    #[arg(long, num_args = 0..=1, default_missing_value = "all", value_name = "ZIEL")]
    pub volume: Option<String>,
    /// Oberfläche in mm² (Ziel wie --bounds, Standard all).
    #[arg(long, num_args = 0..=1, default_missing_value = "all", value_name = "ZIEL")]
    pub area: Option<String>,
    /// Kollisionen: paarweises Schnittvolumen aller Bauteile (exakt über den Kernel).
    #[arg(long)]
    pub interference: bool,
    /// Kollisionen ab diesem Schnittvolumen in mm³.
    #[arg(long, default_value_t = 0.001)]
    pub tolerance: f64,
    /// Kleinster Abstand zweier Bauteile (über die Tessellierung, Genauigkeit wird angegeben).
    #[arg(long, num_args = 2, value_names = ["A", "B"])]
    pub clearance: Vec<String>,
    /// Sehnentoleranz der Tessellierung für --clearance in mm.
    #[arg(long, default_value_t = 0.01)]
    pub mesh_tol: f64,
    /// Farbregionen: Volumen und Masse (PLA) je Farbe und Filament, je Bauteil.
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

fn overlap(a: &p::Bounds, b: &p::Bounds, eps: f64) -> bool {
    (0..3).all(|i| a.min.0[i] <= b.max.0[i] + eps && b.min.0[i] <= a.max.0[i] + eps)
}

fn part_node(sc: &p::Scene, name: &str) -> Result<u32, Fail> {
    target_nodes(sc, &format!("part:{}", name.strip_prefix("part:").unwrap_or(name))).map(|n| n[0])
}

pub(crate) fn command(c: &Common, a: &MeasureArgs) -> Result<Outcome, Fail> {
    let nothing = a.between.is_empty()
        && a.bounds.is_none()
        && a.volume.is_none()
        && a.area.is_none()
        && !a.interference
        && a.clearance.is_empty()
        && !a.colors;
    if nothing {
        return Err(Fail::Usage(
            "nichts zu messen: --between A B, --bounds, --volume, --area, --interference, --clearance A B oder --colors"
                .into(),
        ));
    }
    let mut m = load(c, None, true, true)?;
    let mut j = m.base_json("measure");
    let mut t = m.diagnostics_text();
    let Some(sc) = m.scene.clone() else {
        return Ok(Outcome { json: j, text: t, errors: true });
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
                let v = intersection_volume(&mut m, pa.node, pb.node).map_err(Fail::Kernel)?;
                let hit = v > a.tolerance;
                pairs
                    .push(json!({ "a": pa.name, "b": pb.name, "bounds_overlap": true, "volume": v, "collision": hit }));
                if hit {
                    collisions.push(json!({ "a": pa.name, "b": pb.name, "volume": v }));
                    t.push_str(&format!("Kollision {} ↔ {}: {} mm³\n", pa.name, pb.name, num(v)));
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
        let tris = |m: &mut Model, n: u32| -> Result<Vec<geom::Tri>, Fail> {
            let mesh = m.session.mesh_tol(n, tol).map_err(Fail::Kernel)?;
            Ok(geom::triangles(&mesh, &mat(&sc.nodes[n as usize].display_world)))
        };
        let (ta, tb) = (tris(&mut m, na)?, tris(&mut m, nb)?);
        let d = geom::min_distance(&ta, &tb).ok_or_else(|| Fail::Usage("Bauteil ohne Geometrie".into()))?;
        let inter = intersection_volume(&mut m, na, nb).map_err(Fail::Kernel)?;
        let collide = inter > a.tolerance;
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
            t.push_str(&format!(
                "Abstand {} ↔ {}: 0 mm – die Bauteile überschneiden sich ({} mm³)\n",
                a.clearance[0],
                a.clearance[1],
                num(inter)
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
    let errors = m.has_errors();
    Ok(Outcome { json: j, text: t, errors })
}
