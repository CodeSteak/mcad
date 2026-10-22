//! `cad render`: the model as PNG, optionally with one line highlighted as in the editor
//! (nodes of the line full, the rest dimmed, zones blue, subtracted bodies red transparent).

use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use cad_kernel::Mesh;
use cad_render::{Item, Polyline, RenderOptions, Scene, View, render};
use cad_session::protocol as p;
use glam::{DMat4, DVec3};
use serde_json::json;

use crate::{Common, Fail, Outcome, load, mat, nodes_at_line};

const PART: [f32; 4] = [0.75, 0.78, 0.84, 1.0];
const DIM: [f32; 4] = [0.78, 0.79, 0.82, 0.22];
const RED: [f32; 4] = [0.9, 0.2, 0.15, 0.35];
const BLUE: [f32; 4] = [0.2, 0.45, 0.95, 0.3];
const GREY: [f32; 4] = [0.55, 0.55, 0.58, 0.3];
const EDGE: [f32; 4] = [0.15, 0.15, 0.18, 1.0];
const DIM_EDGE: [f32; 4] = [0.45, 0.45, 0.5, 0.35];
const SELECTED: [f32; 4] = [1.0, 0.55, 0.0, 1.0];
const FAILED: [f32; 4] = [0.95, 0.1, 0.1, 1.0];

fn parse_size(s: &str) -> Result<(u32, u32), Fail> {
    let bad = || Fail::Usage(format!("--size {s}: erwartet BxH, z. B. 1024x768"));
    let (w, h) = s.split_once(['x', 'X']).ok_or_else(bad)?;
    let w: u32 = w.trim().parse().map_err(|_| bad())?;
    let h: u32 = h.trim().parse().map_err(|_| bad())?;
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return Err(bad());
    }
    Ok((w, h))
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
    Polyline {
        points: points.iter().map(|q| m.transform_point3(DVec3::from_array(q.0))).collect(),
        color,
        width,
        on_top: false,
    }
}

pub(crate) fn command(
    c: &Common,
    output: &Path,
    view: &str,
    line: Option<u32>,
    size: &str,
    whole_file: bool,
    axes: bool,
) -> Result<Outcome, Fail> {
    let (width, height) = parse_size(size)?;
    let view_v = View::from_name(view)
        .ok_or_else(|| Fail::Usage(format!("--view {view}: iso, front, back, top, bottom, left oder right")))?;
    let mut m = load(c, line, whole_file, true)?;
    let mut j = m.base_json("render");
    j["line"] = json!(line);
    let Some(sc) = m.scene.clone() else {
        let t = m.diagnostics_text();
        return Ok(Outcome { json: j, text: t, errors: true });
    };
    let highlighted: Vec<u32> = line.map(|l| nodes_at_line(&sc, l)).unwrap_or_default();
    let live = |n: u32| sc.nodes[n as usize].status != p::NodeStatus::Skipped;

    let mut draws: Vec<Draw> = Vec::new();
    let mut lines: Vec<Polyline> = Vec::new();
    if let Some(root) = sc.root.filter(|&r| live(r)) {
        if highlighted.is_empty() || highlighted.contains(&root) {
            draws.push(Draw { node: root, color: PART, edges: Some(EDGE), node_colors: true });
        } else {
            draws.push(Draw { node: root, color: DIM, edges: Some(DIM_EDGE), node_colors: false });
        }
    }
    for &h in &highlighted {
        if !live(h) || Some(h) == sc.root {
            continue;
        }
        let n = &sc.nodes[h as usize];
        let (color, edges) = if up(&sc, h, |x| x.flags.zone) {
            (BLUE, None)
        } else if up(&sc, h, |x| x.subtracted) {
            (RED, None)
        } else if up(&sc, h, |x| x.flags.background) {
            (GREY, None)
        } else {
            (PART, Some(EDGE))
        };
        draws.push(Draw { node: h, color, edges, node_colors: color == PART });
        // Display of the operation: subtracted bodies red, zones blue.
        for &ch in &n.children {
            let cn = &sc.nodes[ch as usize];
            if !live(ch) {
                continue;
            }
            if cn.subtracted {
                draws.push(Draw { node: ch, color: RED, edges: None, node_colors: false });
            } else if cn.flags.zone && matches!(n.label.as_str(), "fillet" | "chamfer" | "shell") {
                draws.push(Draw { node: ch, color: BLUE, edges: None, node_colors: false });
            }
        }
    }
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
    for n in &sc.nodes {
        if !live(n.idx) || n.flags.disabled {
            continue;
        }
        let parent_has = |f: &dyn Fn(&p::SceneNode) -> bool| n.parent.is_some_and(|par| up(&sc, par, |x| f(x)));
        if n.flags.highlight && !parent_has(&|x| x.flags.highlight) {
            draws.push(Draw { node: n.idx, color: RED, edges: None, node_colors: false });
        }
        if n.flags.background && !parent_has(&|x| x.flags.background) {
            draws.push(Draw { node: n.idx, color: GREY, edges: None, node_colors: false });
        }
    }

    // Meshes (cached per key in the engine).
    let mut meshes: Vec<(Rc<Mesh>, &Draw)> = Vec::new();
    for d in &draws {
        let mesh = m.session.mesh(d.node, false).map_err(Fail::Kernel)?;
        meshes.push((mesh, d));
    }
    let tag_color: HashMap<u32, [f32; 4]> = sc
        .tag_nodes
        .iter()
        .filter_map(|tn| tn.nodes.iter().find_map(|&n| sc.nodes[n as usize].color).map(|c| (tn.tag, c.0)))
        .collect();
    let mut owned: Vec<(Mesh, [f32; 4], DMat4, Option<[f32; 4]>)> = Vec::new();
    let mut triangles = 0usize;
    for (mesh, d) in &meshes {
        let w = mat(&sc.nodes[d.node as usize].display_world);
        triangles += mesh.triangle_count();
        if d.node_colors {
            for (sub, col) in split_by_color(mesh, &|t| tag_color.get(&t).copied(), d.color) {
                owned.push((sub, col, w, d.edges));
            }
        } else {
            owned.push(((**mesh).clone(), d.color, w, d.edges));
        }
    }
    let items: Vec<Item> =
        owned.iter().map(|(mesh, col, w, edges)| Item::new(mesh, *col).with_transform(*w).with_edges(*edges)).collect();
    let opts = RenderOptions { width, height, view: view_v, axes, ..Default::default() };
    let img = render(&Scene { items, lines }, &opts);
    img.write_png(output).map_err(|e| Fail::Usage(format!("{}: {e}", output.display())))?;

    j["output"] = json!(output.display().to_string());
    j["width"] = json!(width);
    j["height"] = json!(height);
    j["view"] = json!(view);
    j["highlighted"] = json!(highlighted);
    j["triangles"] = json!(triangles);
    let mut t = m.diagnostics_text();
    t.push_str(&format!("{}: {width}x{height}, Ansicht {view}", output.display()));
    if let Some(l) = line {
        let labels: Vec<String> =
            highlighted.iter().map(|&h| format!("[{}] {}", h, sc.nodes[h as usize].label)).collect();
        if labels.is_empty() {
            t.push_str(&format!(", Zeile {l}: keine Geometrie"));
        } else {
            t.push_str(&format!(", Zeile {l}: {}", labels.join(", ")));
        }
    }
    if let Some(ctx) = &m.context {
        t.push_str(&format!(", Kontext Modul {ctx}"));
    }
    t.push('\n');
    let errors = m.has_errors();
    m.scene = Some(sc);
    Ok(Outcome { json: j, text: t, errors })
}
