//! Export of single nodes (parts, Plan 12.9): a part's result placed in context coordinates,
//! as STL, as a 3MF object (several parts → one 3MF with one object each) or as STEP (exact
//! B-rep through `Kernel::brep` and `cad_step`, the same writer as the whole model:
//! [`Engine::export_step`] is [`Engine::export_step_nodes`] of the root).

use std::fmt::Write as _;

use cad_i18n::{tr, trs};
use cad_ir::{Dim, NodeIdx};
use cad_kernel::{Kernel, Mesh};

use crate::export::{CONTENT_TYPES, RELS, stl_binary, welded, zip_store};
use crate::{Engine, Evaluation};

/// Export format of [`Engine::export_nodes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeFormat {
    /// One binary STL with all nodes.
    Stl,
    /// One 3MF package, one object per node (named).
    ThreeMf,
    /// STEP with the exact B-rep ([`Engine::export_step_nodes`]).
    Step,
}

impl NodeFormat {
    pub fn extension(self) -> &'static str {
        match self {
            NodeFormat::Stl => "stl",
            NodeFormat::ThreeMf => "3mf",
            NodeFormat::Step => "step",
        }
    }
}

/// A file name for a part name: `k[2]` → `k_2`, characters outside `[A-Za-z0-9_.-]` → `_`.
pub fn part_file_name(name: &str, ext: &str) -> String {
    let mut s: String =
        name.chars().map(|c| if c.is_ascii_alphanumeric() || "_.-".contains(c) { c } else { '_' }).collect();
    while s.ends_with('_') {
        s.pop();
    }
    if s.is_empty() {
        s.push_str("part");
    }
    format!("{s}.{ext}")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('"', "&quot;").replace('>', "&gt;")
}

/// 3D model XML with one object per `(name, mesh)` and a build item for each.
pub fn threemf_objects_xml(objects: &[(&str, &Mesh)]) -> String {
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str("<model unit=\"millimeter\" xml:lang=\"en-US\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">\n");
    s.push_str(" <resources>\n");
    for (i, (name, m)) in objects.iter().enumerate() {
        let (verts, tris) = welded(m);
        let _ = writeln!(
            s,
            "  <object id=\"{}\" type=\"model\" name=\"{}\">\n   <mesh>\n    <vertices>",
            i + 1,
            xml_escape(name)
        );
        for v in &verts {
            let _ = writeln!(s, "     <vertex x=\"{}\" y=\"{}\" z=\"{}\"/>", v[0], v[1], v[2]);
        }
        s.push_str("    </vertices>\n    <triangles>\n");
        for t in &tris {
            let _ = writeln!(s, "     <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>", t[0], t[1], t[2]);
        }
        s.push_str("    </triangles>\n   </mesh>\n  </object>\n");
    }
    s.push_str(" </resources>\n <build>\n");
    for i in 0..objects.len() {
        let _ = writeln!(s, "  <item objectid=\"{}\"/>", i + 1);
    }
    s.push_str(" </build>\n</model>\n");
    s
}

/// 3MF package with one object per `(name, mesh)`.
pub fn threemf_objects(objects: &[(&str, &Mesh)]) -> Vec<u8> {
    let model = threemf_objects_xml(objects);
    zip_store(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", RELS.as_bytes()),
        ("3D/3dmodel.model", model.as_bytes()),
    ])
}

impl<K: Kernel> Engine<K>
where
    K::Shape: 'static,
{
    /// Mesh of a node placed in context coordinates (`NodeReport::display_world`), for export
    /// (fine tolerance by default: `tol` or 0.01 mm).
    pub fn placed_mesh(&mut self, ev: &Evaluation<K::Shape>, node: NodeIdx, tol: Option<f64>) -> Result<Mesh, String> {
        let r = ev
            .reports
            .get(node as usize)
            .ok_or_else(|| tr!("Knoten {node} existiert nicht", "node {node} does not exist"))?;
        if r.dim == Some(Dim::Two) {
            return Err(trs!(
                "2D-Ergebnis kann nicht als Körper exportiert werden",
                "a 2D result cannot be exported as a body"
            )
            .into());
        }
        let world = r.display_world;
        let m = self.tessellate(ev, node, Some(tol.unwrap_or(0.01)))?;
        let mut out = Mesh::default();
        out.append_transformed(&m, &world);
        Ok(out)
    }

    /// STEP of nodes in context coordinates (`NodeReport::display_world`): the exact B-reps of
    /// their results ([`Kernel::brep`], placed with the kernel's `transform`) written as one
    /// product by `cad_step`. Faces get the colour of their colour region
    /// ([`Engine::face_region_colors`]; material without `color()` gets none), unless
    /// `opts.tag_colors` is set.
    pub fn export_step_nodes(
        &mut self,
        ev: &Evaluation<K::Shape>,
        nodes: &[NodeIdx],
        opts: &cad_step::StepOptions,
    ) -> Result<(Vec<u8>, cad_step::StepStats), String> {
        let nodes: Vec<(Option<String>, NodeIdx)> = nodes.iter().map(|&n| (None, n)).collect();
        self.export_step_named(ev, &nodes, opts)
    }

    /// [`Engine::export_step_nodes`] with a body name per node (`Some`): every body of the node
    /// gets that label (numbered if it has several; `cad_step::StepOptions::body_names`).
    /// Nodes without a name: `product N` as before.
    pub fn export_step_named(
        &mut self,
        ev: &Evaluation<K::Shape>,
        nodes: &[(Option<String>, NodeIdx)],
        opts: &cad_step::StepOptions,
    ) -> Result<(Vec<u8>, cad_step::StepStats), String> {
        let groups: Vec<(Option<String>, Vec<NodeIdx>)> = nodes.iter().map(|(l, n)| (l.clone(), vec![*n])).collect();
        self.export_step_groups(ev, &groups, opts).map(|(b, st, _)| (b, st))
    }

    /// [`Engine::export_step_named`] with groups of nodes: a group of one node writes its
    /// bodies as there; the solids of a group of several nodes are fused into one body by a
    /// kernel union (overlapping objects of the top level, `Session::export_step`). Faces of a
    /// fused body keep the colour of the face they come from (by face tag). If the union fails,
    /// the group's bodies are written apart; the third value lists those groups (their label)
    /// with the kernel message.
    pub fn export_step_groups(
        &mut self,
        ev: &Evaluation<K::Shape>,
        groups: &[(Option<String>, Vec<NodeIdx>)],
        opts: &cad_step::StepOptions,
    ) -> Result<StepGroupsOut, String> {
        let named = groups.iter().any(|(n, _)| n.is_some());
        let mut breps = Vec::new();
        let mut o = opts.clone();
        o.body_names.clear();
        let own = o.tag_colors.is_empty();
        let mut failed = Vec::new();
        // Colours per face from the colour regions (Bericht 12, F1): a tag can belong to
        // several nodes of equal content in different colours, so faces get synthetic tags
        // (one per colour) in a copy of the B-rep.
        let mut colors: Vec<[u8; 4]> = Vec::new();
        for (label, nodes) in groups {
            let name = label.clone().unwrap_or_else(|| opts.name.clone());
            let mut written = Vec::new();
            if nodes.len() > 1 {
                match self.fused_brep(ev, nodes) {
                    Ok(Some((b, rest))) => {
                        // the source faces' colours by tag (the union keeps the face tags)
                        let mut by_tag = std::collections::HashMap::new();
                        if own {
                            for &n in nodes {
                                let world = ev.reports[n as usize].display_world;
                                for i in 0..ev.parts(n).len() {
                                    let src = self.placed_brep(ev, n, i, &world)?;
                                    let fc = self.face_region_colors(ev, n, i)?;
                                    for_each_face(&src, &mut |f| {
                                        if let Some(crate::colors::RegionColor::Color(c)) = fc.get(f.id as usize) {
                                            by_tag.entry(f.tag).or_insert(*c);
                                        }
                                    });
                                }
                            }
                        }
                        let mut b = b;
                        if own {
                            for_each_face_mut(&mut b, &mut |f| match by_tag.get(&f.tag) {
                                Some(c) => f.tag = color_tag(&mut colors, *c),
                                None => f.tag = 0,
                            });
                        }
                        if named {
                            o.body_names.push(name.clone());
                        }
                        breps.push(std::rc::Rc::new(b));
                        written = rest;
                    }
                    Ok(None) => written = nodes.iter().map(|&n| (n, None)).collect(),
                    Err(e) => {
                        failed.push((name.clone(), e));
                        written = nodes.iter().map(|&n| (n, None)).collect();
                    }
                }
            } else {
                written.extend(nodes.iter().map(|&n| (n, None)));
            }
            // single nodes, and what a fused group left (open surfaces, meshes)
            for (n, only) in written {
                let r = ev
                    .reports
                    .get(n as usize)
                    .ok_or_else(|| tr!("Knoten {n} existiert nicht", "node {n} does not exist"))?;
                if r.dim == Some(Dim::Two) {
                    return Err(trs!(
                        "2D-Ergebnis kann nicht als Körper exportiert werden",
                        "a 2D result cannot be exported as a body"
                    )
                    .into());
                }
                let world = r.display_world;
                // B-rep parts, then meshes as facetted B-reps
                for i in 0..ev.parts(n).len() + ev.meshes(n).len() {
                    if only.as_ref().is_some_and(|v: &Vec<usize>| !v.contains(&i)) {
                        continue;
                    }
                    let b = self.placed_brep(ev, n, i, &world)?;
                    if named {
                        o.body_names.push(name.clone());
                    }
                    if !own {
                        breps.push(b);
                        continue;
                    }
                    let fc = self.face_region_colors(ev, n, i)?;
                    let mut b = (*b).clone();
                    for_each_face_mut(&mut b, &mut |f| match fc.get(f.id as usize) {
                        Some(crate::colors::RegionColor::Color(c)) => f.tag = color_tag(&mut colors, *c),
                        _ => f.tag = 0,
                    });
                    breps.push(std::rc::Rc::new(b));
                }
            }
        }
        if own {
            for (i, c) in colors.iter().enumerate() {
                let f = |x: u8| x as f64 / 255.0;
                o.tag_colors.insert(STEP_COLOR_TAG + i as u32, [f(c[0]), f(c[1]), f(c[2])]);
            }
        }
        let refs: Vec<&cad_kernel::brep::Brep> = breps.iter().map(|b| &**b).collect();
        let (text, st) = cad_step::write(&refs, &o).map_err(|e| e.to_string())?;
        Ok((text.into_bytes(), st, failed))
    }

    /// The union of the solids of `nodes`, placed in context coordinates, as a B-rep (face tags
    /// of the sources), and per node the parts it leaves to be written alone (open surfaces,
    /// meshes; `Some(indices)` as in [`Engine::export_step_groups`]). `None` with fewer than
    /// two solids.
    #[allow(clippy::type_complexity)]
    fn fused_brep(
        &mut self,
        ev: &Evaluation<K::Shape>,
        nodes: &[NodeIdx],
    ) -> Result<Option<(cad_kernel::brep::Brep, Vec<(NodeIdx, Option<Vec<usize>>)>)>, String> {
        let mut shapes = Vec::new();
        let mut rest = Vec::new();
        for &n in nodes {
            let r = ev
                .reports
                .get(n as usize)
                .ok_or_else(|| tr!("Knoten {n} existiert nicht", "node {n} does not exist"))?;
            let world = r.display_world;
            let mut alone = Vec::new();
            for (i, p) in ev.parts(n).iter().enumerate() {
                if !p.solid {
                    alone.push(i);
                    continue;
                }
                let s = if world == glam::DMat4::IDENTITY {
                    p.shape.clone()
                } else {
                    self.kernel.transform(&p.shape, &world).map_err(|e| e.to_string())?
                };
                shapes.push(s);
            }
            alone.extend(ev.parts(n).len()..ev.parts(n).len() + ev.meshes(n).len());
            if !alone.is_empty() {
                rest.push((n, Some(alone)));
            }
        }
        if shapes.len() < 2 {
            return Ok(None);
        }
        let u = self.kernel.boolean(cad_kernel::BoolOp::Union, &shapes, 0).map_err(|e| e.to_string())?;
        let b = self.kernel.brep(&u).map_err(|e| e.to_string())?;
        Ok(Some((b, rest)))
    }

    /// Exports nodes `(name, node)` in context coordinates: STL (all in one file), 3MF (one
    /// object per node) or STEP (one product with all their bodies, default options). Returns
    /// the bytes and the number of triangles (0 for STEP).
    pub fn export_nodes(
        &mut self,
        ev: &Evaluation<K::Shape>,
        nodes: &[(String, NodeIdx)],
        format: NodeFormat,
        tol: Option<f64>,
    ) -> Result<(Vec<u8>, u32), String> {
        self.export_nodes_with(ev, nodes, format, tol, &cad_step::StepOptions::default())
    }

    /// [`Engine::export_nodes`] with STEP options (schema, header); the product name is the
    /// node names joined unless `step.name` is set to something else than the default.
    pub fn export_nodes_with(
        &mut self,
        ev: &Evaluation<K::Shape>,
        nodes: &[(String, NodeIdx)],
        format: NodeFormat,
        tol: Option<f64>,
        step: &cad_step::StepOptions,
    ) -> Result<(Vec<u8>, u32), String> {
        if nodes.is_empty() {
            return Err(trs!("keine Bauteile gewählt", "no parts selected").into());
        }
        match format {
            NodeFormat::Stl => {
                let mut all = Mesh::default();
                for (_, n) in nodes {
                    let m = self.placed_mesh(ev, *n, tol)?;
                    all.append_transformed(&m, &glam::DMat4::IDENTITY);
                }
                let tris = all.triangle_count() as u32;
                Ok((stl_binary(&all, "cad-engine"), tris))
            }
            NodeFormat::ThreeMf => {
                let mut meshes = Vec::new();
                for (name, n) in nodes {
                    meshes.push((name.clone(), self.placed_mesh(ev, *n, tol)?));
                }
                let tris = meshes.iter().map(|(_, m)| m.triangle_count() as u32).sum();
                let objs: Vec<(&str, &Mesh)> = meshes.iter().map(|(n, m)| (n.as_str(), m)).collect();
                Ok((threemf_objects(&objs), tris))
            }
            NodeFormat::Step => {
                let mut o = step.clone();
                if o.name == cad_step::StepOptions::default().name {
                    o.name = nodes.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(" + ");
                }
                // one body per part, labelled with its name
                let named: Vec<(Option<String>, NodeIdx)> = nodes.iter().map(|(s, n)| (Some(s.clone()), *n)).collect();
                let (bytes, _) = self.export_step_named(ev, &named, &o)?;
                Ok((bytes, 0))
            }
        }
    }

    /// One file per node (`part_file_name`) in a ZIP package.
    pub fn export_nodes_zip(
        &mut self,
        ev: &Evaluation<K::Shape>,
        nodes: &[(String, NodeIdx)],
        format: NodeFormat,
        tol: Option<f64>,
    ) -> Result<(Vec<u8>, u32), String> {
        self.export_nodes_zip_with(ev, nodes, format, tol, &cad_step::StepOptions::default())
    }

    /// [`Engine::export_nodes_zip`] with STEP options.
    pub fn export_nodes_zip_with(
        &mut self,
        ev: &Evaluation<K::Shape>,
        nodes: &[(String, NodeIdx)],
        format: NodeFormat,
        tol: Option<f64>,
        step: &cad_step::StepOptions,
    ) -> Result<(Vec<u8>, u32), String> {
        let mut files = Vec::new();
        let mut tris = 0;
        for (name, n) in nodes {
            let (b, t) = self.export_nodes_with(ev, &[(name.clone(), *n)], format, tol, step)?;
            tris += t;
            let mut fname = part_file_name(name, format.extension());
            while files.iter().any(|(f, _): &(String, Vec<u8>)| *f == fname) {
                fname = format!("_{fname}");
            }
            files.push((fname, b));
        }
        let entries: Vec<(&str, &[u8])> = files.iter().map(|(n, b)| (n.as_str(), b.as_slice())).collect();
        Ok((zip_store(&entries), tris))
    }
}

/// [`Engine::export_step_groups`]: bytes, statistics, groups written apart (label, message).
pub type StepGroupsOut = (Vec<u8>, cad_step::StepStats, Vec<(String, String)>);

/// First synthetic face tag of the STEP colours (one per colour; kernel tags are far below).
const STEP_COLOR_TAG: u32 = 0x4000_0000;

/// The synthetic face tag of colour `c` (added to `colors` when new).
fn color_tag(colors: &mut Vec<[u8; 4]>, c: [u8; 4]) -> u32 {
    let idx = match colors.iter().position(|x| *x == c) {
        Some(i) => i,
        None => {
            colors.push(c);
            colors.len() - 1
        }
    };
    STEP_COLOR_TAG + idx as u32
}

fn for_each_face(b: &cad_kernel::brep::Brep, f: &mut dyn FnMut(&cad_kernel::brep::Face)) {
    use cad_kernel::brep::Body;
    for body in &b.bodies {
        let shells: Vec<&cad_kernel::brep::Shell> = match body {
            Body::Solid { outer, voids } => std::iter::once(outer).chain(voids.iter()).collect(),
            Body::Sheet { shells } => shells.iter().collect(),
        };
        for sh in shells {
            sh.faces.iter().for_each(&mut *f);
        }
    }
}

fn for_each_face_mut(b: &mut cad_kernel::brep::Brep, f: &mut dyn FnMut(&mut cad_kernel::brep::Face)) {
    use cad_kernel::brep::Body;
    for body in &mut b.bodies {
        match body {
            Body::Solid { outer, voids } => {
                for sh in std::iter::once(outer).chain(voids.iter_mut()) {
                    sh.faces.iter_mut().for_each(&mut *f);
                }
            }
            Body::Sheet { shells } => {
                for sh in shells {
                    sh.faces.iter_mut().for_each(&mut *f);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names() {
        assert_eq!(part_file_name("lid", "stl"), "lid.stl");
        assert_eq!(part_file_name("k[2]", "3mf"), "k_2.3mf");
        assert_eq!(part_file_name("ä", "stl"), "part.stl");
    }

    #[test]
    fn objects_xml_has_one_object_per_mesh() {
        let m = Mesh {
            positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            normals: vec![0.0; 9],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        let x = threemf_objects_xml(&[("a<b", &m), ("c", &m)]);
        assert_eq!(x.matches("<object ").count(), 2);
        assert!(x.contains("name=\"a&lt;b\""));
        assert_eq!(x.matches("<item ").count(), 2);
    }
}
