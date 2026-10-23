//! Colours and filaments through the protocol (Plan 16): `Scene.colors`,
//! `RequestColorRegions`, `SetFilament`, multi-colour exports.

use cad_session::protocol::*;
use cad_session::{Session, SessionOptions, std_clock};
use kernel_smlib::SmlibKernel;

const SCHILD: &str = "color(\"white\") cube([40, 20, 2]);\ncolor(\"red\") translate([5, 5, 2]) cube([30, 10, 1]);\ncolor(\"lightgoldenrod\") translate([0, 0, -2]) cube([4, 4, 2]);\n";

fn session() -> Session<SmlibKernel> {
    Session::new(SmlibKernel::new(), SessionOptions::default(), std_clock())
}

fn run(s: &mut Session<SmlibKernel>, text: &str) -> Vec<FromEngine> {
    let mut r = s.post(ToEngine::SetSource { text: text.into(), version: 1 });
    r.extend(s.run_to_idle());
    r
}

fn last_scene(msgs: &[FromEngine]) -> &Scene {
    msgs.iter()
        .rev()
        .find_map(|m| match m {
            FromEngine::Scene(s) => Some(s.as_ref()),
            _ => None,
        })
        .expect("scene")
}

/// Applies UTF-16 edits to an ASCII text.
fn apply(text: &str, edits: &[Edit]) -> String {
    let mut t = text.to_string();
    let mut e: Vec<&Edit> = edits.iter().collect();
    e.sort_by_key(|x| std::cmp::Reverse(x.from));
    for x in e {
        t.replace_range(x.from as usize..x.to as usize, &x.text);
    }
    t
}

#[test]
fn scene_lists_colours_with_slots() {
    let mut s = session();
    let r = run(&mut s, SCHILD);
    let sc = last_scene(&r);
    let c: Vec<(&str, &str, u32, ColorSource)> =
        sc.colors.iter().map(|c| (c.key.as_str(), c.label.as_str(), c.filament, c.source)).collect();
    assert_eq!(
        c,
        [
            ("#f4f3f0", "white", 1, ColorSource::Palette),
            ("#c74240", "red", 2, ColorSource::Palette),
            (sc.colors[2].key.as_str(), "lightgoldenrod", 3, ColorSource::Random),
        ]
    );
    assert_eq!(sc.colors[1].ranges[0].line, 2);
    // the unknown name warns at the statement
    let FromEngine::Diagnostics { items, .. } = &r[0] else { panic!() };
    assert!(items.iter().any(|d| d.severity == Severity::Warning
        && d.message.contains("`lightgoldenrod` unbekannt")
        && d.range.line == 3));
    // the preliminary scene has the list already (no kernel needed)
    let pre = s.post(ToEngine::SetSource { text: SCHILD.replace("red", "blue"), version: 2 });
    let FromEngine::Scene(p) = pre.iter().find(|m| matches!(m, FromEngine::Scene(_))).unwrap() else { panic!() };
    assert!(!p.complete);
    assert_eq!(p.colors[1].label, "blue");
}

#[test]
fn colour_regions_with_meshes() {
    let mut s = session();
    run(&mut s, SCHILD);
    s.post(ToEngine::RequestColorRegions { meshes: true, coarse: None, version: Some(1) });
    let r = s.run_to_idle();
    let FromEngine::ColorRegions { version, items } = &r[0] else { panic!("{r:?}") };
    assert_eq!(*version, 1);
    assert_eq!(items.len(), 1, "no parts: the root");
    let set = &items[0];
    assert_eq!(set.part, None);
    let vols: Vec<(&str, u32, f64)> = set.regions.iter().map(|x| (x.color.as_str(), x.filament, x.volume)).collect();
    assert_eq!(vols.len(), 3);
    assert_eq!((vols[0].0, vols[0].1), ("#f4f3f0", 1));
    assert!((vols[0].2 - 1600.0).abs() < 1e-6 && (vols[1].2 - 300.0).abs() < 1e-6 && (vols[2].2 - 32.0).abs() < 1e-6);
    assert!((set.total - 1932.0).abs() < 1e-6);
    let m = set.regions[1].mesh.as_ref().expect("mesh");
    assert!(!m.indices.is_empty() && m.node == set.node);
    // stale version: nothing
    s.post(ToEngine::RequestColorRegions { meshes: false, coarse: None, version: Some(7) });
    assert!(s.run_to_idle().is_empty());
}

#[test]
fn set_filament_writes_the_code() {
    let mut s = session();
    run(&mut s, SCHILD);
    let r = s.post(ToEngine::SetFilament { color: "#c74240".into(), filament: Some(4) });
    let FromEngine::Edits { edits, .. } = &r[0] else { panic!("{r:?}") };
    let t = apply(SCHILD, edits);
    assert!(t.contains("color(\"red\", filament = 4) translate"), "{t}");
    // the new text gives the slot; automatic again removes it
    let r = run(&mut s, &t);
    assert_eq!(last_scene(&r).colors[1].filament, 4);
    assert!(last_scene(&r).colors[1].explicit);
    let r = s.post(ToEngine::SetFilament { color: "#c74240".into(), filament: None });
    let FromEngine::Edits { edits, .. } = &r[0] else { panic!() };
    assert_eq!(apply(&t, edits), SCHILD);
    // errors: unknown colour, default material, bad slot
    let r = s.post(ToEngine::SetFilament { color: "#000000".into(), filament: Some(2) });
    assert!(matches!(&r[0], FromEngine::Error { message, .. } if message.contains("kommt im Modell nicht vor")));
    let r = s.post(ToEngine::SetFilament { color: "#c74240".into(), filament: Some(17) });
    assert!(matches!(&r[0], FromEngine::Error { .. }));
    run(&mut s, "cube(1); color(\"red\") cube(2);");
    let r = s.post(ToEngine::SetFilament { color: "#c3c8cf".into(), filament: Some(2) });
    assert!(matches!(&r[0], FromEngine::Error { message, .. } if message.contains("immer Filament 1")), "{r:?}");
}

#[test]
fn multicolour_exports() {
    let mut s = session();
    run(&mut s, SCHILD);
    s.post(ToEngine::Export { format: ExportFormat::ThreeMf, parts: None, zip: None, colors: None });
    s.post(ToEngine::Export { format: ExportFormat::ThreeMf, parts: None, zip: None, colors: Some(false) });
    s.post(ToEngine::Export { format: ExportFormat::Stl, parts: None, zip: None, colors: Some(true) });
    let r = s.run_to_idle();
    assert_eq!(r.len(), 3);
    let FromEngine::ExportResult { bytes, colors: Some(true), .. } = &r[0] else { panic!() };
    let t = String::from_utf8_lossy(bytes);
    assert_eq!(t.matches("<m:color ").count(), 3);
    assert!(t.contains("name=\"main – red\""), "root object named after the main file");
    let FromEngine::ExportResult { bytes, colors: None, .. } = &r[1] else { panic!() };
    assert!(!String::from_utf8_lossy(bytes).contains("m:colorgroup"));
    let FromEngine::ExportResult { bytes, zip: Some(true), colors: Some(true), .. } = &r[2] else { panic!() };
    let t = String::from_utf8_lossy(bytes);
    assert!(t.contains("F1_white.stl") && t.contains("F2_red.stl") && t.contains("F3_lightgoldenrod.stl"));
}
