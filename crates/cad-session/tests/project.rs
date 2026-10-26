//! Session with project files (`SetFiles`, `SetFile`): file-aware ranges and edits, parts
//! and their export.

use std::collections::BTreeMap;

use cad_session::protocol::*;
use cad_session::{Session, SessionOptions, std_clock};
use kernel_smlib::SmlibKernel;

const MAIN: &str = "include <consts.scad>\nuse <lib/parts.mcad>\n\
                    housing: housing(w = width);\ntranslate([0, 50, 0]) lid: lid(w = width);\n";
const CONSTS: &str = "// Dimensions\nwidth = 40;\n";
const PARTS: &str = "module housing(w = 30, h = 12) {\n    difference() {\n        cube([w, 20, h]);\n        \
                     translate([2, 2, 2]) cube([w - 4, 16, h]);\n    }\n}\nmodule lid(w = 30) cube([w, 20, 2]);\n";

fn session() -> Session<SmlibKernel> {
    Session::new(SmlibKernel::new(), SessionOptions::default(), std_clock())
}

fn files() -> BTreeMap<String, String> {
    [("consts.scad", CONSTS), ("lib/parts.mcad", PARTS)].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

fn last_scene(msgs: &[FromEngine]) -> Option<Scene> {
    msgs.iter().rev().find_map(|m| match m {
        FromEngine::Scene(s) => Some((**s).clone()),
        _ => None,
    })
}

fn last_diags(msgs: &[FromEngine]) -> Vec<Diagnostic> {
    msgs.iter()
        .rev()
        .find_map(|m| match m {
            FromEngine::Diagnostics { items, .. } => Some(items.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn start(s: &mut Session<SmlibKernel>) -> Scene {
    s.post(ToEngine::SetFiles { files: files(), main: Some("main.scad".into()) });
    let mut m = s.post(ToEngine::SetSource { text: MAIN.into(), version: 1 });
    m.extend(s.run_to_idle());
    assert!(last_diags(&m).is_empty(), "{:?}", last_diags(&m));
    last_scene(&m).unwrap()
}

#[test]
fn files_ranges_and_parts() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    let sc = start(&mut s);
    assert!(sc.complete);
    let paths: Vec<(&str, FileRole)> = sc.files.iter().map(|f| (f.path.as_str(), f.role)).collect();
    assert_eq!(
        paths,
        [("main.scad", FileRole::Main), ("consts.scad", FileRole::Include), ("lib/parts.mcad", FileRole::Use)]
    );
    // Hint of `width` in consts.scad: local positions, file id 1.
    let h = sc.hints.iter().find(|h| h.text == "40").unwrap();
    assert_eq!(h.range.file, Some(1));
    assert_eq!((h.range.line, h.range.from), (2, 14));
    // Nodes from the library: file 2, local line.
    let cubes: Vec<&SceneNode> = sc.nodes.iter().filter(|n| n.label == "box").collect();
    assert!(cubes.iter().any(|n| n.range.file == Some(2) && n.range.line == 3));
    // Main file ranges have no file.
    let call = sc.nodes.iter().find(|n| n.instance.as_deref() == Some("housing")).unwrap();
    assert_eq!(call.range.file, None);
    assert_eq!(call.range.line, 3);
    // Parts with bounds and volumes.
    let names: Vec<&str> = sc.parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["housing", "lid"]);
    let d = &sc.parts[1];
    assert_eq!(d.bounds.as_ref().unwrap().min.0[1], 50.0);
    assert!((d.volume.unwrap() - 40.0 * 20.0 * 2.0).abs() < 1e-6);
    let g = &sc.parts[0];
    assert!((g.volume.unwrap() - (40.0 * 20.0 * 12.0 - 36.0 * 16.0 * 10.0)).abs() < 1e-6);
    // include statements
    assert!(sc.includes.iter().any(|i| i.target == "consts.scad" && !i.is_use && i.found && i.range.line == 1));
}

#[test]
fn drag_edit_targets_the_included_file() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    let sc = start(&mut s);
    // Context: module housing of the library; its height default lives in lib/parts.mcad.
    let mut m = s.post(ToEngine::SetContext { module: Some("housing".into()), overrides: BTreeMap::new() });
    m.extend(s.run_to_idle());
    let sc2 = last_scene(&m).unwrap();
    assert_eq!(sc2.context.module.as_deref(), Some("housing"));
    assert_eq!(sc2.context.def_range.unwrap().file, Some(2));
    let h = sc2.handles.iter().find(|h| h.kind == HandleKind::Point && h.comps.iter().flatten().count() >= 2).unwrap();
    let r = s.post(ToEngine::Drag { handle: h.id, target: DragTarget::Point { pos: Vec3([40.0, 20.0, 15.0]) } });
    let FromEngine::Edits { edits, .. } = &r[0] else { panic!("{r:?}") };
    let e = edits.iter().find(|e| e.text == "15").expect("height edit");
    assert_eq!(e.file.as_deref(), Some("lib/parts.mcad"));
    assert_eq!(&PARTS[e.from as usize..e.to as usize], "12");
    // Apply in the file, send it, evaluate: the box is taller.
    let new = format!("{}{}{}", &PARTS[..e.from as usize], e.text, &PARTS[e.to as usize..]);
    s.post(ToEngine::SetFile { path: "lib/parts.mcad".into(), text: Some(new) });
    let mut m = s.post(ToEngine::SetSource { text: MAIN.into(), version: 2 });
    m.extend(s.run_to_idle());
    let sc3 = last_scene(&m).unwrap();
    let outer = sc3.nodes.iter().find(|n| n.label == "box" && n.range.file == Some(2)).unwrap();
    assert_eq!(outer.bounds.as_ref().unwrap().max.0[2], 15.0);
    // In the file context the width comes from consts.scad through `width`.
    s.post(ToEngine::SetContext { module: None, overrides: BTreeMap::new() });
    let _ = sc;
}

#[test]
fn set_default_writes_into_the_library() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    start(&mut s);
    let r = s.post(ToEngine::SetDefault { module: "lid".into(), param: "w".into(), value: Value::Num(55.0) });
    let FromEngine::Edits { edits, .. } = &r[0] else { panic!("{r:?}") };
    assert_eq!(edits[0].file.as_deref(), Some("lib/parts.mcad"));
    assert_eq!(&PARTS[edits[0].from as usize..edits[0].to as usize], "30");
}

#[test]
fn missing_files_and_syntax_errors_of_other_files() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    s.post(ToEngine::SetFiles { files: BTreeMap::new(), main: None });
    let mut m = s.post(ToEngine::SetSource { text: "cube(1);\ninclude <gone.scad>\n".into(), version: 1 });
    m.extend(s.run_to_idle());
    let d = last_diags(&m);
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].severity, d[0].source, d[0].range.line), (Severity::Error, DiagSource::Eval, 2));
    assert!(d[0].file.is_none());
    assert!(last_scene(&m).is_some());
    // A syntax error in an included file: only diagnostics (with its path), no scene.
    s.post(ToEngine::SetFile { path: "gone.scad".into(), text: Some("\ncube(;".into()) });
    let m = s.post(ToEngine::SetSource { text: "cube(1);\ninclude <gone.scad>\n".into(), version: 2 });
    let d = last_diags(&m);
    assert_eq!(d[0].source, DiagSource::Syntax);
    assert_eq!(d[0].file.as_deref(), Some("gone.scad"));
    assert_eq!(d[0].range.line, 2);
    assert!(last_scene(&m).is_none());
}

#[test]
fn export_parts() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    start(&mut s);
    s.post(ToEngine::Export { format: ExportFormat::Stl, parts: Some(vec!["lid".into()]), zip: None, colors: None });
    s.post(ToEngine::Export {
        format: ExportFormat::ThreeMf,
        parts: Some(vec!["housing".into(), "lid".into()]),
        zip: None,
        colors: None,
    });
    s.post(ToEngine::Export {
        format: ExportFormat::Stl,
        parts: Some(vec!["housing".into(), "lid".into()]),
        zip: Some(true),
        colors: None,
    });
    s.post(ToEngine::Export {
        format: ExportFormat::Stl,
        parts: Some(vec!["missing".into()]),
        zip: None,
        colors: None,
    });
    s.post(ToEngine::Export { format: ExportFormat::Step, parts: Some(vec!["lid".into()]), zip: None, colors: None });
    let r = s.run_to_idle();
    assert_eq!(r.len(), 5, "{r:?}");
    let FromEngine::ExportResult { bytes, triangles, parts, .. } = &r[0] else { panic!() };
    assert_eq!(bytes.len() as u32, 84 + 50 * triangles);
    assert_eq!(parts.as_deref(), Some(&["lid".to_string()][..]));
    // 3MF is multi-coloured by default: per part a component object with one volume per
    // colour (here one), plus the Bambu/Orca metadata
    let FromEngine::ExportResult { bytes, format: ExportFormat::ThreeMf, colors: Some(true), .. } = &r[1] else {
        panic!("{:?}", r[1])
    };
    let t = String::from_utf8_lossy(bytes);
    // 4 objects in the model (2 meshes, 2 parts), 2 in model_settings.config
    assert_eq!(t.matches("<object ").count(), 6);
    assert_eq!(t.matches("<item ").count(), 2);
    assert!(t.contains("Metadata/model_settings.config"));
    let FromEngine::ExportResult { bytes, zip: Some(true), .. } = &r[2] else { panic!() };
    let t = String::from_utf8_lossy(bytes);
    assert!(t.contains("housing.stl") && t.contains("lid.stl"));
    let FromEngine::Error { message, .. } = &r[3] else { panic!() };
    assert!(message.contains("Bauteil `missing` nicht gefunden (Bauteile: housing, lid)"), "{message}");
    // STEP of a part: the exact B-rep through the same writer as the whole model.
    let FromEngine::ExportResult { format: ExportFormat::Step, bytes, triangles, .. } = &r[4] else {
        panic!("{:?}", r[4])
    };
    assert_eq!(*triangles, 0);
    let t = String::from_utf8_lossy(bytes);
    assert!(t.starts_with("ISO-10303-21;"));
    assert_eq!(t.matches("MANIFOLD_SOLID_BREP(").count(), 1);
    assert!(t.contains("PRODUCT('lid','lid'"), "{}", &t[..t.len().min(2000)]);
    // STEP per part as ZIP; the whole model as STEP.
    s.post(ToEngine::Export {
        format: ExportFormat::Step,
        parts: Some(vec!["housing".into(), "lid".into()]),
        zip: Some(true),
        colors: None,
    });
    s.post(ToEngine::Export { format: ExportFormat::Step, parts: None, zip: None, colors: None });
    let r = s.run_to_idle();
    let FromEngine::ExportResult { bytes, zip: Some(true), .. } = &r[0] else { panic!("{:?}", r[0]) };
    let t = String::from_utf8_lossy(bytes);
    assert!(t.contains("housing.step") && t.contains("lid.step"));
    assert_eq!(t.matches("MANIFOLD_SOLID_BREP(").count(), 2);
    let FromEngine::ExportResult { bytes, parts: None, .. } = &r[1] else { panic!("{:?}", r[1]) };
    assert_eq!(String::from_utf8_lossy(bytes).matches("MANIFOLD_SOLID_BREP(").count(), 2);
}
