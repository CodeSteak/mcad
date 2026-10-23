//! Export of single nodes (parts, Plan 12.9): a part's result placed in context coordinates,
//! as STL, as a 3MF object (several parts → one 3MF with one object each) or as STEP through
//! the kernel's export (`Kernel::export`, the same path as the whole model).

use std::fmt::Write as _;

use cad_ir::{Dim, NodeIdx};
use cad_kernel::{ExportFormat, Kernel, Mesh};

use crate::export::{CONTENT_TYPES, RELS, stl_binary, welded, zip_store};
use crate::{Engine, Evaluation};

/// Export format of [`Engine::export_nodes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeFormat {
    /// One binary STL with all nodes.
    Stl,
    /// One 3MF package, one object per node (named).
    ThreeMf,
    /// STEP through the kernel (`Kernel::export(…, ExportFormat::Step)`).
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
    let mut s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || "_.-".contains(c) { c } else { '_' }).collect();
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
        let _ = writeln!(s, "  <object id=\"{}\" type=\"model\" name=\"{}\">\n   <mesh>\n    <vertices>", i + 1, xml_escape(name));
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

    /// Exports nodes `(name, node)` in context coordinates: STL (all in one file), 3MF (one
    /// object per node) or STEP (the kernel's export of all their shapes). Returns the bytes
    /// and the number of triangles (0 for STEP).
    pub fn export_nodes(
        &mut self,
        ev: &Evaluation<K::Shape>,
        nodes: &[(String, NodeIdx)],
        format: NodeFormat,
        tol: Option<f64>,
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
                let mut shapes = Vec::new();
                for (_, n) in nodes {
                    let world = ev.reports.get(*n as usize).ok_or("Knoten existiert nicht")?.display_world;
                    for p in ev.parts(*n) {
                        let s = if world == glam::DMat4::IDENTITY {
                            p.shape.clone()
                        } else {
                            self.kernel_mut().transform(&p.shape, &world).map_err(|e| e.to_string())?
                        };
                        shapes.push(s);
                    }
                }
                let bytes =
                    self.kernel_mut().export(&shapes, ExportFormat::Step).map_err(|e| format!("STEP-Export: {e}"))?;
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
        let mut files = Vec::new();
        let mut tris = 0;
        for (name, n) in nodes {
            let (b, t) = self.export_nodes(ev, &[(name.clone(), *n)], format, tol)?;
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
        let mut m = Mesh::default();
        m.positions = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        m.normals = vec![0.0; 9];
        m.indices = vec![0, 1, 2];
        let x = threemf_objects_xml(&[("a<b", &m), ("c", &m)]);
        assert_eq!(x.matches("<object ").count(), 2);
        assert!(x.contains("name=\"a&lt;b\""));
        assert_eq!(x.matches("<item ").count(), 2);
    }
}
