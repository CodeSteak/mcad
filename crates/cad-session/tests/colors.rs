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
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
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
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
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
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
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
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    run(&mut s, SCHILD);
    s.post(ToEngine::Export { format: ExportFormat::ThreeMf, parts: None, zip: None, colors: None });
    s.post(ToEngine::Export { format: ExportFormat::ThreeMf, parts: None, zip: None, colors: Some(false) });
    s.post(ToEngine::Export { format: ExportFormat::Stl, parts: None, zip: None, colors: Some(true) });
    let r = s.run_to_idle();
    assert_eq!(r.len(), 3);
    let FromEngine::ExportResult { bytes, colors: Some(true), .. } = &r[0] else { panic!() };
    let t = String::from_utf8_lossy(bytes);
    // Bericht 12: no colour group (Bambu Studio would open its colour dialog), slots only in
    // model_settings.config
    assert_eq!(t.matches("<m:color ").count(), 0);
    assert!(t.contains("<metadata key=\"extruder\" value=\"2\"/>"));
    assert!(t.contains("name=\"main – red\""), "root object named after the main file");
    let FromEngine::ExportResult { bytes, colors: None, .. } = &r[1] else { panic!() };
    assert!(!String::from_utf8_lossy(bytes).contains("m:colorgroup"));
    let FromEngine::ExportResult { bytes, zip: Some(true), colors: Some(true), .. } = &r[2] else { panic!() };
    let t = String::from_utf8_lossy(bytes);
    assert!(t.contains("F1_white.stl") && t.contains("F2_red.stl") && t.contains("F3_lightgoldenrod.stl"));
}

fn edits_of(r: &[FromEngine]) -> &[Edit] {
    match &r[0] {
        FromEngine::Edits { edits, .. } => edits,
        other => panic!("{other:?}"),
    }
}

fn slots(sc: &Scene) -> Vec<(String, u32, bool)> {
    sc.colors.iter().map(|c| (c.label.clone(), c.filament, c.explicit)).collect()
}

/// Bericht 12, F2: the colour is a parameter (`module bar(c) color(c) …`): `filament = n` at
/// that statement would give every colour the slot. SetFilament writes a `filament(c, n);`
/// table at the top of the file instead; only the chosen colour moves.
#[test]
fn set_filament_of_a_parameter_colour_writes_a_table() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let src =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/color-review/cases/slot_mod.scad"))
            .unwrap();
    let mut s = session();
    let r = run(&mut s, &src);
    assert_eq!(
        slots(last_scene(&r)),
        [("red".into(), 1, false), ("blue".into(), 2, false), ("green".into(), 3, false)]
    );
    let t = apply(&src, edits_of(&s.post(ToEngine::SetFilament { color: "#c74240".into(), filament: Some(4) })));
    assert!(t.starts_with("module bar(c) color(c) cube([10, 2, 2]);\nfilament(\"red\", 4);\nbar(\"red\");"), "{t}");
    assert!(!t.contains("filament = "), "nothing at the shared statement: {t}");
    let r = run(&mut s, &t);
    let FromEngine::Diagnostics { items, .. } = &r[0] else { panic!() };
    assert!(items.iter().all(|d| d.severity != Severity::Warning), "{items:?}");
    assert_eq!(slots(last_scene(&r)), [("red".into(), 4, true), ("blue".into(), 1, false), ("green".into(), 2, false)]);
    // a second colour goes below the first declaration; changing one edits its slot
    let t2 = apply(&t, edits_of(&s.post(ToEngine::SetFilament { color: "#4ca167".into(), filament: Some(7) })));
    assert!(t2.contains("filament(\"red\", 4);\nfilament(\"green\", 7);\n"), "{t2}");
    let r = run(&mut s, &t2);
    assert_eq!(slots(last_scene(&r))[2], ("green".into(), 7, true));
    let t3 = apply(&t2, edits_of(&s.post(ToEngine::SetFilament { color: "#c74240".into(), filament: Some(5) })));
    assert!(t3.contains("filament(\"red\", 5);"), "{t3}");
    run(&mut s, &t3);
    // automatic removes the statement with its line
    let t4 = apply(&t3, edits_of(&s.post(ToEngine::SetFilament { color: "#c74240".into(), filament: None })));
    let t4r = run(&mut s, &t4);
    let t5 = apply(&t4, edits_of(&s.post(ToEngine::SetFilament { color: "#4ca167".into(), filament: None })));
    assert_eq!(t5, src);
    assert_eq!(slots(last_scene(&t4r))[0], ("red".into(), 1, false));
}

/// Bericht 12, F2: colours from a loop (`for (i) color(names[i]) …`) get a table too; a
/// literal colour keeps `filament = n` at its statement; the table wins over `filament = m`.
#[test]
fn filament_table_rules() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let src = "names = [\"red\", \"blue\"];\nfor (i = [0:1]) translate([i * 10, 0, 0]) color(names[i]) cube(5);\ncolor(\"teal\") translate([0, 10, 0]) cube(5);\n";
    let mut s = session();
    run(&mut s, src);
    let t = apply(src, edits_of(&s.post(ToEngine::SetFilament { color: "#2e65b9".into(), filament: Some(3) })));
    assert!(t.starts_with("names = [\"red\", \"blue\"];\nfilament(\"blue\", 3);\nfor"), "{t}");
    let r = run(&mut s, &t);
    assert_eq!(slots(last_scene(&r)), [("red".into(), 1, false), ("blue".into(), 3, true), ("teal".into(), 2, false)]);
    let t2 = apply(&t, edits_of(&s.post(ToEngine::SetFilament { color: "#217274".into(), filament: Some(6) })));
    assert!(t2.contains("color(\"teal\", filament = 6) translate"), "{t2}");
    // the table wins; a conflicting filament = m warns
    let src =
        "filament(\"red\", 5);\ncolor(\"red\", filament = 2) cube(1);\ntranslate([3, 0, 0]) filament(\"blue\", 2);\n";
    let r = run(&mut s, src);
    let sc = last_scene(&r);
    assert_eq!(slots(sc), [("red".into(), 5, true)]);
    let FromEngine::Diagnostics { items, .. } = &r[0] else { panic!() };
    let w: Vec<&str> = items.iter().map(|d| d.message.as_str()).collect();
    assert!(w.iter().any(|m| m.contains("Farbe hat schon Filament 5")), "{w:?}");
    assert!(w.iter().any(|m| m.contains("nur auf oberster Ebene")), "{w:?}");
}

/// Bericht 12, F4: material outside the parts is an object "no part" (fixed name) of the default
/// export and has a volume in the colour regions; the export names its objects.
#[test]
fn material_outside_parts_is_exported() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let src = "module a() color(\"red\") cube(5);\nteil: a();\ntranslate([10, 0, 0]) color(\"blue\") cube(4);\n";
    let mut s = session();
    run(&mut s, src);
    let regs = s.color_regions(false, false).unwrap();
    let parts: Vec<(Option<&str>, f64)> = regs.iter().map(|r| (r.part.as_deref(), r.total)).collect();
    assert_eq!(parts, [(Some("teil"), 125.0), (Some("no part"), 64.0)]);
    s.post(ToEngine::Export { format: ExportFormat::ThreeMf, parts: None, zip: None, colors: None });
    let r = s.run_to_idle();
    let FromEngine::ExportResult { bytes, notes: Some(notes), .. } = &r[0] else { panic!("{r:?}") };
    let t = String::from_utf8_lossy(bytes);
    assert_eq!(t.matches("<item ").count(), 2);
    assert!(t.contains("name=\"no part – blue\""), "{t}");
    assert!(notes[0].starts_with("2 Objekte (teil, no part)"), "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("ohne Farbdialog")), "{notes:?}");
}
