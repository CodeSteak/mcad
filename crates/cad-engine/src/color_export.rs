//! Multi-colour exports (Plan 16.4): a 3MF for Bambu Studio / OrcaSlicer with one object per
//! part and one part (volume) per colour, and a ZIP with one STL per colour.
//!
//! The 3MF (all in `3D/3dmodel.model`, core spec 2015/02, materials extension optional):
//! * one mesh object per (part, colour);
//! * one object per part with a `components` list of its colour meshes (identity
//!   transforms: all meshes are in the same coordinates, nothing moves), referenced by a
//!   build item;
//! * `Metadata/model_settings.config` (Bambu/Orca project metadata): per part object its name
//!   and `extruder` (slot of the largest colour), per colour mesh a `part` with name and
//!   `extruder` = filament slot;
//! * only with [`PackageOptions::color_group`] (and more than one colour): `m:colorgroup` id 1
//!   with one `m:color` per colour (sRGB `#RRGGBBAA`) and `pid="1" pindex="i"` at every colour
//!   mesh, for viewers that show 3MF colours.
//!
//! Sources (BambuStudio `src/libslic3r/Format/bbs_3mf.cpp` and `src/slic3r/GUI/Plater.cpp`,
//! OrcaSlicer the same files, read on 23.10.2026, Bericht 11 and 12): both importers read
//! `model_settings.config` also from 3MF files of other applications (not gated by the version
//! check), match `<part id>` to the component's object id (`_generate_volumes_new`) and apply
//! `extruder` as volume setting; objects with a single volume use the object's `extruder`.
//! Bambu Studio opens its colour dialog (`ObjColorDialog`) for a foreign 3MF exactly when the
//! file has a `m:colorgroup` and triangles with colours (`Plater.cpp`: `!color_group_map.empty()
//! && !volume_color_data.empty() && en_3mf_file_type != From_BBS`; object-level `pid` gives
//! every triangle a colour); OK there overwrites `extruder` of every part
//! (`ObjColorPanel::apply_volume_filament_ids`), "Abbrechen" keeps it. Without the colour group
//! the dialog does not open and the slots of the code stay. OrcaSlicer has no such dialog; it
//! uses colour groups only for objects without `model_settings.config` volumes. Marking the
//! file as a Bambu project (`Application` = `BambuStudio-…`) would also skip the dialog but
//! claims a producer it is not and switches on the project code paths: not done. No
//! `project_settings.config`: Bambu Studio ignores it for files not written by itself, and
//! OrcaSlicer would apply it over its default printer and process settings.

use std::fmt::Write as _;

use cad_i18n::{tr, trs};
use cad_ir::{Dim, NodeIdx};
use cad_kernel::Mesh;

use crate::colors::ColorTable;
use crate::export::{RELS, stl_binary, welded, zip_store};
use crate::{Engine, Evaluation};

/// Content types including the `.config` metadata files.
pub const CONTENT_TYPES_COLOR: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"model\" ContentType=\"application/vnd.ms-package.3dmanufacturing-3dmodel+xml\"/><Default Extension=\"config\" ContentType=\"text/xml\"/></Types>\n";

/// One colour of the package (`m:color` of the colour group).
#[derive(Debug, Clone, PartialEq)]
pub struct PackageColor {
    /// sRGB bytes with alpha.
    pub rgba: [u8; 4],
    pub label: String,
    pub filament: u32,
}

/// A colour mesh of a part.
#[derive(Debug, Clone)]
pub struct PackagePart {
    pub name: String,
    /// Index into the package colours.
    pub color: usize,
    pub mesh: Mesh,
    pub volume: f64,
}

#[derive(Debug, Clone)]
pub struct PackageObject {
    pub name: String,
    pub parts: Vec<PackagePart>,
}

/// What a multi-colour export wrote.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColorExportStats {
    pub objects: usize,
    pub parts: usize,
    pub triangles: u32,
    /// (label, filament, volume mm³) per colour, in package order.
    pub colors: Vec<(String, u32, f64)>,
    /// Object names in package order.
    pub object_names: Vec<String>,
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('"', "&quot;").replace('>', "&gt;")
}

/// Options of the multi-colour 3MF.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PackageOptions {
    /// Write `m:colorgroup` and `pid`/`pindex` (more than one colour only). Bambu Studio then
    /// opens its colour dialog, whose OK replaces the filament slots (see the module docs).
    pub color_group: bool,
}

/// `3D/3dmodel.model` of a multi-colour package; also returns the object id of every part
/// object and of every colour mesh (for `model_settings.config`).
pub fn color_model_xml(
    objects: &[PackageObject],
    colors: &[PackageColor],
    app: &str,
    opts: PackageOptions,
) -> (String, Vec<(u32, Vec<u32>)>) {
    let group = opts.color_group && colors.len() > 1;
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str("<model unit=\"millimeter\" xml:lang=\"en-US\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\" xmlns:m=\"http://schemas.microsoft.com/3dmanufacturing/material/2015/02\">\n");
    let _ = writeln!(s, " <metadata name=\"Application\">{}</metadata>", xml_escape(app));
    s.push_str(" <resources>\n");
    if group {
        s.push_str("  <m:colorgroup id=\"1\">\n");
        for c in colors {
            let b = c.rgba;
            let _ = writeln!(s, "   <m:color color=\"#{:02X}{:02X}{:02X}{:02X}\"/>", b[0], b[1], b[2], b[3]);
        }
        s.push_str("  </m:colorgroup>\n");
    }
    let mut next = 2u32;
    let mut ids = Vec::new();
    for o in objects {
        let mut part_ids = Vec::new();
        for p in &o.parts {
            let id = next;
            next += 1;
            part_ids.push(id);
            let (verts, tris) = welded(&p.mesh);
            let color = if group { format!(" pid=\"1\" pindex=\"{}\"", p.color) } else { String::new() };
            let _ = writeln!(
                s,
                "  <object id=\"{id}\" type=\"model\" name=\"{}\"{color}>\n   <mesh>\n    <vertices>",
                xml_escape(&p.name),
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
        let id = next;
        next += 1;
        let _ = writeln!(s, "  <object id=\"{id}\" type=\"model\" name=\"{}\">\n   <components>", xml_escape(&o.name));
        for pid in &part_ids {
            let _ = writeln!(s, "    <component objectid=\"{pid}\"/>");
        }
        s.push_str("   </components>\n  </object>\n");
        ids.push((id, part_ids));
    }
    s.push_str(" </resources>\n <build>\n");
    for (id, _) in &ids {
        let _ = writeln!(s, "  <item objectid=\"{id}\"/>");
    }
    s.push_str(" </build>\n</model>\n");
    (s, ids)
}

/// `Metadata/model_settings.config`: names and extruder (filament slot) per object and part.
pub fn model_settings_xml(objects: &[PackageObject], colors: &[PackageColor], ids: &[(u32, Vec<u32>)]) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<config>\n");
    for (o, (oid, pids)) in objects.iter().zip(ids) {
        // object default: slot of the largest colour (used when the object has one part)
        let main = o.parts.iter().max_by(|a, b| a.volume.total_cmp(&b.volume)).map(|p| colors[p.color].filament);
        let _ = writeln!(s, "  <object id=\"{oid}\">");
        let _ = writeln!(s, "    <metadata key=\"name\" value=\"{}\"/>", xml_escape(&o.name));
        let _ = writeln!(s, "    <metadata key=\"extruder\" value=\"{}\"/>", main.unwrap_or(1));
        for (p, pid) in o.parts.iter().zip(pids) {
            let _ = writeln!(s, "    <part id=\"{pid}\" subtype=\"normal_part\">");
            let _ = writeln!(s, "      <metadata key=\"name\" value=\"{}\"/>", xml_escape(&p.name));
            let _ = writeln!(s, "      <metadata key=\"extruder\" value=\"{}\"/>", colors[p.color].filament);
            s.push_str("    </part>\n");
        }
        s.push_str("  </object>\n");
    }
    s.push_str("</config>\n");
    s
}

/// The complete multi-colour 3MF package.
pub fn color_package(objects: &[PackageObject], colors: &[PackageColor], app: &str, opts: PackageOptions) -> Vec<u8> {
    let (model, ids) = color_model_xml(objects, colors, app, opts);
    let settings = model_settings_xml(objects, colors, &ids);
    zip_store(&[
        ("[Content_Types].xml", CONTENT_TYPES_COLOR.as_bytes()),
        ("_rels/.rels", RELS.as_bytes()),
        ("3D/3dmodel.model", model.as_bytes()),
        ("Metadata/model_settings.config", settings.as_bytes()),
    ])
}

/// File-name friendly label: `#12ab34` → `12ab34`, other characters outside
/// `[A-Za-z0-9_-]` → `_`.
fn label_file(label: &str) -> String {
    let s: String = label
        .trim_start_matches('#')
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if s.is_empty() { "color".into() } else { s }
}

impl<K: cad_kernel::Kernel> Engine<K>
where
    K::Shape: 'static,
{
    /// Colour objects of nodes `(name, node)` in context coordinates: per node its colour
    /// regions tessellated with `tol` (default 0.01 mm) and placed with `display_world`;
    /// nodes of the same name form one object.
    pub fn color_objects(
        &mut self,
        ev: &Evaluation<K::Shape>,
        nodes: &[(String, NodeIdx)],
        tol: Option<f64>,
        table: &ColorTable,
    ) -> Result<(Vec<PackageObject>, Vec<PackageColor>), String> {
        let tol = tol.unwrap_or(0.01);
        let mut colors: Vec<PackageColor> = Vec::new();
        let mut entry_color: Vec<Option<usize>> = vec![None; table.entries.len()];
        let mut objects = Vec::new();
        for (name, n) in nodes {
            let r = ev
                .reports
                .get(*n as usize)
                .ok_or_else(|| tr!("Knoten {n} existiert nicht", "node {n} does not exist"))?;
            if r.dim == Some(Dim::Two) {
                return Err(trs!(
                    "2D-Ergebnis kann nicht als Körper exportiert werden",
                    "a 2D result cannot be exported as a body"
                )
                .into());
            }
            let world = r.display_world;
            let regions = self.color_regions(ev, *n)?;
            let mut parts = Vec::new();
            for (i, reg) in regions.regions.iter().enumerate() {
                let e = table.entry_of(reg.color).ok_or(trs!(
                    "Farbe der Region fehlt in der Farbtabelle",
                    "colour of the region missing from the colour table"
                ))?;
                let ci = match entry_color[e] {
                    Some(ci) => ci,
                    None => {
                        let en = &table.entries[e];
                        colors.push(PackageColor { rgba: en.key, label: en.label.clone(), filament: en.filament });
                        entry_color[e] = Some(colors.len() - 1);
                        colors.len() - 1
                    }
                };
                let m = self.region_mesh(&regions, i, tol)?;
                let mut mesh = Mesh::default();
                mesh.append_transformed(&m, &world);
                parts.push(PackagePart {
                    name: format!("{name} – {}", table.entries[e].label),
                    color: ci,
                    mesh,
                    volume: reg.volume,
                });
            }
            // several nodes of one name (the material outside the parts): one object
            match objects.iter_mut().find(|o: &&mut PackageObject| o.name == *name) {
                Some(o) => o.parts.extend(parts),
                None => objects.push(PackageObject { name: name.clone(), parts }),
            }
        }
        Ok((objects, colors))
    }

    /// Multi-colour 3MF of nodes `(name, node)` (see the module docs).
    pub fn export_3mf_colors(
        &mut self,
        ev: &Evaluation<K::Shape>,
        nodes: &[(String, NodeIdx)],
        tol: Option<f64>,
        table: &ColorTable,
        app: &str,
        opts: PackageOptions,
    ) -> Result<(Vec<u8>, ColorExportStats), String> {
        if nodes.is_empty() {
            return Err(trs!("keine Bauteile gewählt", "no parts selected").into());
        }
        let (objects, colors) = self.color_objects(ev, nodes, tol, table)?;
        let st = stats(&objects, &colors);
        Ok((color_package(&objects, &colors, app, opts), st))
    }

    /// ZIP with one binary STL per colour (several nodes: per node and colour), named
    /// `F<slot>_<colour>.stl` (`<name>_F<slot>_<colour>.stl`), all in the same coordinates.
    pub fn export_color_stl_zip(
        &mut self,
        ev: &Evaluation<K::Shape>,
        nodes: &[(String, NodeIdx)],
        tol: Option<f64>,
        table: &ColorTable,
    ) -> Result<(Vec<u8>, ColorExportStats), String> {
        if nodes.is_empty() {
            return Err(trs!("keine Bauteile gewählt", "no parts selected").into());
        }
        let (objects, colors) = self.color_objects(ev, nodes, tol, table)?;
        let st = stats(&objects, &colors);
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        for o in &objects {
            for p in &o.parts {
                let c = &colors[p.color];
                let base = format!("F{}_{}", c.filament, label_file(&c.label));
                let mut fname = if objects.len() > 1 {
                    crate::part_file_name(&format!("{}_{base}", o.name), "stl")
                } else {
                    format!("{base}.stl")
                };
                while files.iter().any(|(f, _)| *f == fname) {
                    fname = format!("_{fname}");
                }
                files.push((fname, stl_binary(&p.mesh, &p.name)));
            }
        }
        let entries: Vec<(&str, &[u8])> = files.iter().map(|(n, b)| (n.as_str(), b.as_slice())).collect();
        Ok((zip_store(&entries), st))
    }
}

fn stats(objects: &[PackageObject], colors: &[PackageColor]) -> ColorExportStats {
    let mut st = ColorExportStats {
        objects: objects.len(),
        parts: objects.iter().map(|o| o.parts.len()).sum(),
        triangles: objects.iter().flat_map(|o| &o.parts).map(|p| p.mesh.triangle_count() as u32).sum(),
        colors: colors.iter().map(|c| (c.label.clone(), c.filament, 0.0)).collect(),
        object_names: objects.iter().map(|o| o.name.clone()).collect(),
    };
    for p in objects.iter().flat_map(|o| &o.parts) {
        st.colors[p.color].2 += p.volume;
    }
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri() -> Mesh {
        Mesh {
            positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            normals: vec![0.0; 9],
            indices: vec![0, 1, 2],
            ..Default::default()
        }
    }

    #[test]
    fn structure() {
        let colors = vec![
            PackageColor { rgba: [0xc7, 0x42, 0x40, 255], label: "red".into(), filament: 2 },
            PackageColor { rgba: [0xf4, 0xf3, 0xf0, 255], label: "white".into(), filament: 1 },
        ];
        let objects = vec![PackageObject {
            name: "sign<1>".into(),
            parts: vec![
                PackagePart { name: "a".into(), color: 0, mesh: tri(), volume: 1.0 },
                PackagePart { name: "b".into(), color: 1, mesh: tri(), volume: 5.0 },
            ],
        }];
        let (x, ids) = color_model_xml(&objects, &colors, "controller-cad", PackageOptions { color_group: true });
        assert_eq!(ids, vec![(4, vec![2, 3])]);
        assert!(x.contains("<m:color color=\"#C74240FF\"/>"));
        assert!(x.contains("<object id=\"2\" type=\"model\" name=\"a\" pid=\"1\" pindex=\"0\">"));
        assert!(x.contains("<object id=\"3\" type=\"model\" name=\"b\" pid=\"1\" pindex=\"1\">"));
        assert!(x.contains("<component objectid=\"2\"/>") && x.contains("<component objectid=\"3\"/>"));
        assert!(!x.contains("transform="), "components keep the common origin");
        assert!(x.contains("name=\"sign&lt;1&gt;\""));
        assert!(x.contains("<item objectid=\"4\"/>"));
        let c = model_settings_xml(&objects, &colors, &ids);
        assert!(c.contains("<object id=\"4\">\n    <metadata key=\"name\" value=\"sign&lt;1&gt;\"/>\n    <metadata key=\"extruder\" value=\"1\"/>"), "{c}");
        assert!(c.contains("<part id=\"2\" subtype=\"normal_part\">\n      <metadata key=\"name\" value=\"a\"/>\n      <metadata key=\"extruder\" value=\"2\"/>"), "{c}");
        assert!(c.contains("<part id=\"3\" subtype=\"normal_part\">\n      <metadata key=\"name\" value=\"b\"/>\n      <metadata key=\"extruder\" value=\"1\"/>"), "{c}");
    }

    #[test]
    fn no_color_group_by_default() {
        // Bericht 12: without `m:colorgroup` Bambu Studio opens no colour dialog and keeps the
        // `extruder` of every part; a single colour never gets a group.
        let colors = vec![
            PackageColor { rgba: [0xc7, 0x42, 0x40, 255], label: "red".into(), filament: 2 },
            PackageColor { rgba: [0xf4, 0xf3, 0xf0, 255], label: "white".into(), filament: 1 },
        ];
        let objects = vec![PackageObject {
            name: "a".into(),
            parts: vec![
                PackagePart { name: "a".into(), color: 0, mesh: tri(), volume: 1.0 },
                PackagePart { name: "b".into(), color: 1, mesh: tri(), volume: 5.0 },
            ],
        }];
        let (x, _) = color_model_xml(&objects, &colors, "controller-cad", PackageOptions::default());
        assert!(!x.contains("colorgroup") && !x.contains("pid=") && !x.contains("pindex="), "{x}");
        let one = &colors[..1];
        let objects1 = vec![PackageObject {
            name: "a".into(),
            parts: vec![PackagePart { name: "a".into(), color: 0, mesh: tri(), volume: 1.0 }],
        }];
        let (x, _) = color_model_xml(&objects1, one, "controller-cad", PackageOptions { color_group: true });
        assert!(!x.contains("colorgroup") && !x.contains("pid="), "{x}");
    }

    #[test]
    fn label_files() {
        assert_eq!(label_file("#12ab34"), "12ab34");
        assert_eq!(label_file("dark red"), "dark_red");
        assert_eq!(label_file(""), "color");
    }
}
