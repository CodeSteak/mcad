//! Export of single nodes (parts, Plan 12.9): a part's result placed in context coordinates,
//! as STL, as a 3MF object (several parts → one 3MF with one object each) or as STEP (exact
//! B-rep through `Kernel::brep` and `cad_step`, the same writer as the whole model:
//! [`Engine::export_step`] is [`Engine::export_step_nodes`] of the root).

use std::fmt::Write as _;

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
        s.push_str("teil");
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
        let r = ev.reports.get(node as usize).ok_or_else(|| format!("Knoten {node} existiert nicht"))?;
        if r.dim == Some(Dim::Two) {
            return Err("2D-Ergebnis kann nicht als Körper exportiert werden".into());
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
        let mut breps = Vec::new();
        let mut o = opts.clone();
        let own = o.tag_colors.is_empty();
        // Colours per face from the colour regions (Bericht 12, F1): a tag can belong to
        // several nodes of equal content in different colours, so faces get synthetic tags
        // (one per colour) in a copy of the B-rep.
        let mut colors: Vec<[u8; 4]> = Vec::new();
        for &n in nodes {
            let r = ev.reports.get(n as usize).ok_or_else(|| format!("Knoten {n} existiert nicht"))?;
            if r.dim == Some(Dim::Two) {
                return Err("2D-Ergebnis kann nicht als Körper exportiert werden".into());
            }
            let world = r.display_world;
            // B-rep parts, then meshes as facetted B-reps
            for i in 0..ev.parts(n).len() + ev.meshes(n).len() {
                let b = self.placed_brep(ev, n, i, &world)?;
                if !own {
                    breps.push(b);
                    continue;
                }
                let fc = self.face_region_colors(ev, n, i)?;
                let mut b = (*b).clone();
                for_each_face_mut(&mut b, &mut |f| {
                    let c = match fc.get(f.id as usize) {
                        Some(crate::colors::RegionColor::Color(k)) => *k,
                        _ => {
                            f.tag = 0;
                            return;
                        }
                    };
                    let idx = match colors.iter().position(|x| *x == c) {
                        Some(i) => i,
                        None => {
                            colors.push(c);
                            colors.len() - 1
                        }
                    };
                    f.tag = STEP_COLOR_TAG + idx as u32;
                });
                breps.push(std::rc::Rc::new(b));
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
        Ok((text.into_bytes(), st))
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
            return Err("keine Bauteile gewählt".into());
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
                let ids: Vec<NodeIdx> = nodes.iter().map(|(_, n)| *n).collect();
                let (bytes, _) = self.export_step_nodes(ev, &ids, &o)?;
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

/// First synthetic face tag of the STEP colours (one per colour; kernel tags are far below).
const STEP_COLOR_TAG: u32 = 0x4000_0000;

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
        assert_eq!(part_file_name("deckel", "stl"), "deckel.stl");
        assert_eq!(part_file_name("k[2]", "3mf"), "k_2.3mf");
        assert_eq!(part_file_name("ä", "stl"), "teil.stl");
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
