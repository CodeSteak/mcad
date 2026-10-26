//! Geometric assertions (Plan 21 C) over the native SMLib kernel: values against formulas.

use std::f64::consts::PI;

use cad_session::protocol::*;
use cad_session::{Session, SessionOptions, std_clock};
use kernel_smlib::SmlibKernel;

fn session() -> Session<SmlibKernel> {
    Session::new(SmlibKernel::new(), SessionOptions::default(), std_clock())
}

/// Evaluates `src` and returns the assertion results and all messages.
fn run(s: &mut Session<SmlibKernel>, src: &str, version: u64) -> (Vec<AssertResult>, Vec<FromEngine>) {
    let mut msgs = s.post(ToEngine::SetSource { text: src.into(), version });
    msgs.extend(s.run_to_idle());
    let res = msgs
        .iter()
        .rev()
        .find_map(|m| match m {
            FromEngine::Asserts { items, .. } => Some(items.clone()),
            _ => None,
        })
        .unwrap_or_default();
    (res, msgs)
}

fn check(src: &str) -> Vec<AssertResult> {
    run(&mut session(), src, 1).0
}

fn warnings(msgs: &[FromEngine]) -> Vec<String> {
    msgs.iter()
        .rev()
        .find_map(|m| match m {
            FromEngine::Diagnostics { items, .. } => Some(items.clone()),
            _ => None,
        })
        .unwrap_or_default()
        .into_iter()
        .filter(|d| d.severity == Severity::Warning)
        .map(|d| d.message)
        .collect()
}

fn close(what: &str, got: f64, want: f64, tol: f64) {
    assert!((got - want).abs() <= tol, "{what}: {got} statt {want} (± {tol})");
}

const PLATE: &str = "cube([40, 30, 3]);\n";

#[test]
fn keepout_overlap_is_exact_and_not_in_the_result() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    let src = format!("{PLATE}assert_keepout() translate([10, 10, 2]) cube([10, 10, 5]);\n");
    let (r, msgs) = run(&mut s, &src, 1);
    assert_eq!(r.len(), 1);
    let a = &r[0];
    assert!(a.checked && !a.ok, "{a:?}");
    assert_eq!(a.kind, AssertKind::Keepout);
    assert_eq!(a.pairs.len(), 1);
    let p = &a.pairs[0];
    assert_eq!(p.problem.as_deref(), Some("overlap"));
    close("Schnittvolumen", p.volume.unwrap(), 10.0 * 10.0 * 1.0, 1e-6);
    assert!(p.overlap.is_some() && p.focus.is_some());
    assert!(p.b.origin.iter().any(|o| o.starts_with("Zeile 1: box")), "{:?}", p.b);
    let w = warnings(&msgs);
    assert!(w.iter().any(|m| m.starts_with("assert_keepout: ") && m.contains("ragt ins Sperrvolumen")), "{w:?}");
    // the keep-out volume is no material: root volume = plate
    let d = s.done().unwrap();
    let root = d.scene.root.unwrap();
    let kn = d.scene.nodes.iter().find(|n| n.flags.keepout).unwrap();
    assert!(kn.flags.background && kn.label == "assert_keepout");
    assert!(!d.scene.nodes[root as usize].children.is_empty());
    close("Volumen", s.mass_props(root).unwrap().volume, 40.0 * 30.0 * 3.0, 1e-6);
}

#[test]
fn keepout_clearance() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    // 0.3 mm above the plate, 0.5 mm required
    let r = check(&format!("{PLATE}assert_keepout(clearance = 0.5) translate([10, 10, 3.3]) cube(5);\n"));
    let p = &r[0].pairs[0];
    assert_eq!(p.problem.as_deref(), Some("too_close"), "{p:?}");
    close("Abstand", p.distance.unwrap(), 0.3, 1e-5);
    assert_eq!(p.points.len(), 2);
    // a: the keep-out volume, b: the plate
    close("Fußpunkt Sperrvolumen", p.points[0].0[2], 3.3, 1e-5);
    close("Fußpunkt Platte", p.points[1].0[2], 3.0, 1e-5);
    // 1 mm: fine (no pair listed)
    let r = check(&format!("{PLATE}assert_keepout(clearance = 0.5) translate([10, 10, 4]) cube(5);\n"));
    assert!(r[0].ok && r[0].pairs.is_empty(), "{:?}", r[0]);
    // clearance 0: touching is fine
    let r = check(&format!("{PLATE}assert_keepout() translate([10, 10, 3]) cube(5);\n"));
    assert!(r[0].ok, "{:?}", r[0]);
}

#[test]
fn keepout_checks_every_part() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let src = "module teil(x) translate([x, 0, 0]) cube(10);\n\
               a: teil(0);\nb: teil(20);\n\
               assert_keepout() translate([25, 5, 5]) cube(10);\n";
    let r = check(src);
    let a = &r[0];
    assert_eq!(a.bodies, 2);
    assert_eq!(a.pairs.len(), 1, "{a:?}");
    assert_eq!(a.pairs[0].b.label, "b");
    close("Schnittvolumen", a.pairs[0].volume.unwrap(), 5.0 * 5.0 * 5.0, 1e-6);
}

#[test]
fn disjoint() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let r = check("assert_disjoint() { cube(10); translate([8, 0, 0]) cube(10); }\n");
    let p = &r[0].pairs[0];
    assert!(!p.ok);
    close("Schnittvolumen", p.volume.unwrap(), 2.0 * 10.0 * 10.0, 1e-6);
    // apart and touching: fine (a failing boolean at touching faces would be a note)
    let r = check("assert_disjoint() { cube(10); translate([12, 0, 0]) cube(10); translate([0, 10, 0]) cube(10); }\n");
    assert!(r[0].ok, "{:?}", r[0]);
    assert_eq!(r[0].pairs.len(), 3);
}

#[test]
fn intersect_needs_a_connected_group() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    // chain a–b–c: a and c do not meet, but the group is connected
    let chain = "assert_intersect() { cube(10); translate([8, 0, 0]) cube(10); translate([16, 0, 0]) cube(10); }\n";
    let r = check(chain);
    assert!(r[0].ok, "{:?}", r[0]);
    // a fourth child far away: one violation (closest pair to the group)
    let r = check(
        "assert_intersect() { cube(10); translate([8, 0, 0]) cube(10); translate([16, 0, 0]) cube(10); translate([40, 0, 0]) cube(10); }\n",
    );
    let bad: Vec<&AssertPair> = r[0].pairs.iter().filter(|p| !p.ok).collect();
    assert_eq!(bad.len(), 1, "{:?}", r[0].pairs);
    assert_eq!(bad[0].problem.as_deref(), Some("apart"));
    close("Abstand", bad[0].distance.unwrap(), 40.0 - 26.0, 1e-5);
    // touching is no intersection
    let r = check("assert_intersect() { cube(10); translate([10, 0, 0]) cube(10); }\n");
    assert!(!r[0].ok);
}

#[test]
fn touches_face_contact_area() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    // box face 5 × 5 on the cube's face x = 10
    let r = check("assert_touches() { cube(10); translate([10, 2, 0]) cube([10, 5, 5]); }\n");
    let p = &r[0].pairs[0];
    assert!(p.ok, "{p:?}");
    assert_eq!(p.contact.as_deref(), Some("face"));
    assert!(p.contact_exact);
    close("Kontaktfläche", p.contact_area.unwrap(), 25.0, 1e-9);
    assert!(p.contact_mesh.is_some() && !p.contact_lines.is_empty());
    // circle on a face: π r² exactly (arcs in region2)
    let r = check("assert_touches() { cube(10); translate([5, 5, 10]) cylinder(h = 5, r = 3); }\n");
    let p = &r[0].pairs[0];
    assert!(p.ok, "{p:?}");
    close("Kontaktfläche Kreis", p.contact_area.unwrap(), PI * 9.0, 1e-6);
    assert!(p.contact_exact, "{p:?}");
}

#[test]
fn touches_line_and_point_contact() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let line =
        "assert_touches() { cube([30, 30, 3]); translate([5, 15, 8]) rotate([0, 90, 0]) cylinder(h = 20, r = 5); }\n";
    let r = check(line);
    let p = &r[0].pairs[0];
    assert_eq!(p.problem.as_deref(), Some("line_contact"), "{p:?}");
    assert!(!p.contact_lines.is_empty());
    let r = check(&line.replace("assert_touches()", "assert_touches(contact = \"any\")"));
    assert!(r[0].ok && r[0].pairs[0].contact.as_deref() == Some("line"), "{:?}", r[0]);
    let r = check("assert_touches() { cube([30, 30, 3]); translate([15, 15, 8]) sphere(5); }\n");
    assert_eq!(r[0].pairs[0].problem.as_deref(), Some("point_contact"), "{:?}", r[0].pairs[0]);
}

#[test]
fn touches_gap_and_overlap() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let r = check("assert_touches() { cube(10); translate([10.2, 0, 0]) cube(10); }\n");
    let p = &r[0].pairs[0];
    assert_eq!(p.problem.as_deref(), Some("gap"));
    close("Spalt", p.distance.unwrap(), 0.2, 1e-5);
    // within tol: contact
    let r = check("assert_touches(tol = 0.3) { cube(10); translate([10.2, 0, 0]) cube(10); }\n");
    assert!(r[0].ok, "{:?}", r[0]);
    let r = check("assert_touches() { cube(10); translate([9, 0, 0]) cube(10); }\n");
    let p = &r[0].pairs[0];
    assert_eq!(p.problem.as_deref(), Some("overlap"));
    close("Schnittvolumen", p.volume.unwrap(), 100.0, 1e-6);
}

#[test]
fn imported_mesh_as_keepout() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let stl = include_str!("../../../tests/models/stl/bracket.stl");
    let mut s = session();
    s.post(ToEngine::SetFiles { files: [("bracket.stl".to_string(), stl.to_string())].into(), main: None });
    let src = format!("{PLATE}assert_keepout() translate([5, 5, 2]) import(\"bracket.stl\", as = \"mesh\");\n");
    let (r, _) = run(&mut s, &src, 1);
    let a = &r[0];
    assert!(!a.ok, "{a:?}");
    let p = &a.pairs[0];
    assert!(p.mesh_based && p.problem.as_deref() == Some("overlap"), "{p:?}");
    assert!(a.note.as_deref().unwrap_or("").contains("Netz"));
    // above the plate: free
    let (r, _) = run(&mut s, &src.replace("[5, 5, 2]", "[5, 5, 3.5]"), 2);
    assert!(r[0].ok, "{:?}", r[0]);
}

#[test]
fn results_are_cached_and_skipped_while_dragging() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    let src = "assert_disjoint() { cube(10); translate([8, 0, 0]) cube(10); }\n";
    let (r, _) = run(&mut s, src, 1);
    assert!(!r[0].cached);
    // same content again (other version): every pair from the engine cache
    let (r, _) = run(&mut s, &format!("{src}\n"), 2);
    assert!(r[0].cached && !r[0].ok, "{:?}", r[0]);
    assert_eq!(s.done().unwrap().scene.asserts.len(), 1);
    // drag: unchecked
    s.post(ToEngine::DragStart { handle: 0 });
    let (r, _) = run(&mut s, &src.replace("[8, 0, 0]", "[7, 0, 0]"), 3);
    assert!(!r[0].checked && r[0].ok, "{:?}", r[0]);
}

#[test]
fn warnings_of_the_statement() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    let (_, msgs) =
        run(&mut s, "assert_disjoint() cube(1);\nassert_touches(contact = \"edge\", tol = -1) { cube(1); }\n", 1);
    let all: Vec<String> = msgs
        .iter()
        .filter_map(|m| match m {
            FromEngine::Diagnostics { items, .. } => Some(items.iter().map(|d| d.message.clone()).collect::<Vec<_>>()),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(all.iter().any(|m| m.contains("mindestens zwei Kinder")), "{all:?}");
    assert!(all.iter().any(|m| m.contains("`contact` muss")), "{all:?}");
    assert!(all.iter().any(|m| m.contains("`tol` muss > 0")), "{all:?}");
}
