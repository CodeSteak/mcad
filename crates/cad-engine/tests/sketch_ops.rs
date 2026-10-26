//! 2D operations (Plan 17.3, Testbericht 15) through the engine with native SMLib: `corner`,
//! `fillet2d`/`chamfer2d`, `offset`, `stroke` against closed formulas (extruded by 1, the
//! volume is the area), errors, hints, STEP with exact lines and circles.

mod common;

use std::f64::consts::PI;

use cad_eval::{EvalOptions, eval_source};
use cad_ir::Tree;
use common::*;

const VREL: f64 = 1e-5;

fn tree(src: &str) -> Tree {
    let out = eval_source(src, &EvalOptions::default());
    let errs: Vec<_> = out.errors().collect();
    assert!(errs.is_empty(), "{errs:#?}");
    out.tree
}

/// Area of a 2D expression: volume of its extrusion by 1.
fn area(src2d: &str) -> f64 {
    let mut e = engine();
    let ev = e.evaluate(tree(&format!("linear_extrude(1) {src2d}")));
    assert_clean_but_hints(&ev);
    root_volume(&mut e, &ev)
}

#[track_caller]
fn assert_clean_but_hints(ev: &Ev) {
    let d: Vec<_> = ev.diagnostics().collect();
    assert!(d.is_empty(), "unexpected diagnostics: {d:#?}");
}

fn hints_of(ev: &Ev, label: &str) -> Vec<String> {
    ev.reports.iter().filter(|r| r.label == label).flat_map(|r| r.hints.clone()).collect()
}

#[test]
fn rectangle_with_rounded_corners() {
    // ab − (4 − π) r²
    for (a, b, r) in [(30.0, 20.0, 4.0), (10.0, 10.0, 5.0), (12.0, 7.0, 0.5)] {
        let want = a * b - (4.0 - PI) * r * r;
        assert_approx("fillet2d", area(&format!("fillet2d({r}) square([{a}, {b}]);")), want, VREL);
        let sk = format!(
            "sketch {{ start([0, 0]); corner(r = {r}); line(to = [{a}, 0]); corner(r = {r}); line(to = [{a}, {b}]); corner(r = {r}); line(to = [0, {b}]); corner(r = {r}); close(); }}"
        );
        assert_approx("corner", area(&sk), want, VREL);
    }
    // chamfer: four triangles d²/2
    assert_approx("chamfer2d", area("chamfer2d(2) square([30, 20]);"), 600.0 - 4.0 * 2.0, VREL);
}

#[test]
fn concave_corners_and_zones() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    // L-shape: the concave corner rounded adds (1 − π/4) r², convex corners remove it
    let l = "polygon([[0, 0], [20, 0], [20, 8], [8, 8], [8, 20], [0, 20]])";
    let base = 400.0 - 144.0;
    let r: f64 = 2.0;
    let k = (1.0 - PI / 4.0) * r * r;
    assert_approx("all corners", area(&format!("fillet2d({r}) {l};")), base - 5.0 * k + k, VREL);
    // only the concave corner (zone around [8, 8])
    let src = format!("fillet2d({r}) {{ {l}; translate([8, 8]) circle(1); }}");
    assert_approx("zone", area(&src), base + k, VREL);
    let mut e = engine();
    let ev = e.evaluate(tree(&format!("linear_extrude(1) {src}")));
    assert_eq!(hints_of(&ev, "fillet2d"), vec!["1 Ecke".to_string()]);
    // select = "touch": a zone that only touches the rounding near [20, 4] (an edge, no vertex)
    let touch = format!("fillet2d({r}, select = \"touch\") {{ {l}; translate([20, 6.5]) circle(0.5); }}");
    let ev = e.evaluate(tree(&format!("linear_extrude(1) {touch}")));
    assert_eq!(hints_of(&ev, "fillet2d"), vec!["1 Ecke".to_string()]);
    let inside = touch.replace("select = \"touch\"", "select = \"inside\"");
    let ev = e.evaluate(tree(&format!("linear_extrude(1) {inside}")));
    assert_eq!(hints_of(&ev, "fillet2d"), vec!["0 Ecken".to_string()]);
    assert!(warnings(&ev).iter().any(|w| w.contains("leere Auswahl")), "{:?}", warnings(&ev));
}

#[test]
fn offset_formulas() {
    // square a with r: (a + 2r)² − (4 − π) r²
    for (a, r) in [(10.0, 2.0), (20.0, 0.5)] {
        let want = (a + 2.0 * r) * (a + 2.0 * r) - (4.0 - PI) * r * r;
        assert_approx("offset r", area(&format!("offset(r = {r}) square({a});")), want, VREL);
        assert_approx("offset positional", area(&format!("offset({r}) square({a});")), want, VREL);
    }
    // inward: corners stay sharp; delta: miter; chamfer: corners cut at distance delta
    assert_approx("offset -r", area("offset(r = -2) square(10);"), 36.0, VREL);
    assert_approx("delta", area("offset(delta = 2) square(10);"), 196.0, VREL);
    let t = (2.0 * 2f64.sqrt() - 2.0).powi(2);
    assert_approx("delta chamfer", area("offset(delta = 2, chamfer = true) square(10);"), 196.0 - 4.0 * t, VREL);
    // circle: radius grows; ring: both radii move
    assert_approx("circle", area("offset(r = 3) circle(5);"), PI * 64.0, VREL);
    assert_approx("ring", area("offset(r = 1) difference() { circle(10); circle(4); }"), PI * (121.0 - 9.0), VREL);
    // without arguments: r = 1 (OpenSCAD)
    assert_approx("default", area("offset() square(10);"), 144.0 - (4.0 - PI), VREL);
}

#[test]
fn offset_removes_thin_parts_and_closes_holes() {
    // a bar of width 1.5 between two squares vanishes at r = -1
    let src =
        "offset(r = -1) union() { square(10); translate([10, 4]) square([15, 1.5]); translate([25, 0]) square(10); }";
    // (the eroded squares bulge a little towards the bar's former foot: the true erosion)
    let a = area(src);
    assert!(a > 128.0 && a < 128.5, "{a}");
    // two separate regions
    let t = tree(src);
    let loops = cad_engine::region_2d(&t, t.root.unwrap()).unwrap();
    assert_eq!(loops.len(), 2);
    // hole of radius 4 closes at r = 4.5
    let plate = "difference() { square([30, 20]); translate([8, 10]) circle(4); }";
    let want = 30.0 * 20.0 + 2.0 * 4.5 * 50.0 + PI * 4.5 * 4.5;
    assert_approx("hole closes", area(&format!("offset(r = 4.5) {plate}")), want, VREL);
}

#[test]
fn stroke_formulas() {
    let (r, w): (f64, f64) = (10.0, 2.0);
    let sector = PI / 2.0 / 2.0 * ((r + w / 2.0).powi(2) - (r - w / 2.0).powi(2));
    let arc = format!("sketch(closed = false) {{ start([{r}, 0]); arc(center = [0, 0], angle = 90); }}");
    assert_approx("flat", area(&format!("stroke({w}, cap = \"flat\") {arc}")), sector, VREL);
    assert_approx("round", area(&format!("stroke({w}) {arc}")), sector + PI * (w / 2.0).powi(2), VREL);
    assert_approx("square", area(&format!("stroke({w}, cap = \"square\") {arc}")), sector + w * w, VREL);
    // polyline with a right angle: joins
    let pl = "sketch(closed = false) { start([0, 0]); line(to = [10, 0]); line(to = [10, 10]); }";
    let bands = 39.0;
    assert_approx("miter", area(&format!("stroke(2, cap = \"flat\", join = \"miter\") {pl}")), bands + 1.0, VREL);
    assert_approx("bevel", area(&format!("stroke(2, cap = \"flat\", join = \"bevel\") {pl}")), bands + 0.5, VREL);
    assert_approx("round", area(&format!("stroke(2, cap = \"flat\") {pl}")), bands + PI / 4.0, VREL);
    // the outline of a 2D profile: a ring
    assert_approx("outline", area("stroke(2) circle(10);"), PI * (121.0 - 81.0), VREL);
}

#[test]
fn bezier_corner_and_offset_of_spline() {
    // corner between a line and a Bézier curve: numerical, residual tiny
    let src = "sketch { start([0, 0]); line(to = [20, 0]); corner(r = 3); bezier(c1 = [20, 10], to = [10, 20]); line(to = [0, 20]); close(); }";
    let sharp = area(&src.replace("corner(r = 3); ", ""));
    let rounded = area(src);
    assert!(rounded < sharp && rounded > sharp - 9.0, "{rounded} {sharp}");
    // offset of a spline contour: biarcs, area ≈ A + L·r + π r² (convex, smooth)
    let sp = "sketch { start([10, 0]); spline([[0, 10], [-10, 0], [0, -10]], tangent = [0, 1], end_tangent = [1, 0.0001]); arc(to = [10, 0]); }";
    let a0 = area(sp);
    let a1 = area(&format!("offset(r = 1) {sp}"));
    assert!(a1 > a0 + 60.0 && a1 < a0 + 70.0, "{a0} {a1}");
}

#[test]
fn errors_at_the_operation() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut e = engine();
    // radius too large for a corner of a 10 × 2 rectangle: error at fillet2d, other corners done
    let ev = e.evaluate(tree("linear_extrude(1) fillet2d(1.5) square([10, 2]);"));
    let errs = errors(&ev);
    // every short edge would need 2 × 1.5: all four corners fail, at most r = 1 fits
    assert_eq!(errs.len(), 4, "{errs:?}");
    assert!(errs.iter().all(|e| e.contains("passt nicht") && e.contains("höchstens ≈ 1)")), "{errs:?}");
    // the corners were selected: no "keine Ecke" warning
    assert!(warnings(&ev).is_empty(), "{:?}", warnings(&ev));
    // offset of an open sketch: error, stroke takes it
    let ev = e.evaluate(
        eval_source(
            "linear_extrude(1) offset(1) sketch(closed = false) { start([0, 0]); line(to = [5, 0]); }",
            &EvalOptions::default(),
        )
        .tree,
    );
    assert!(errors(&ev).iter().any(|m| m.contains("offene Skizze")), "{:?}", errors(&ev));
    // 3D child
    let ev = e.evaluate(eval_source("offset(1) cube(3);", &EvalOptions::default()).tree);
    assert!(errors(&ev).iter().any(|m| m.contains("2D-Profile")), "{:?}", errors(&ev));
    // everything removed: warning
    let ev = e.evaluate(tree("offset(r = -6) square(10);"));
    assert!(warnings(&ev).iter().any(|m| m.contains("nichts übrig")), "{:?}", warnings(&ev));
}

#[test]
fn step_of_rounded_profile_has_circles() {
    let src = "linear_extrude(3) offset(r = 2) difference() { square([20, 10]); translate([10, 5]) circle(2); }";
    let mut e = engine();
    let ev = e.evaluate(tree(src));
    assert_clean(&ev);
    let opts = cad_engine::StepOptions { name: "offset".into(), ..Default::default() };
    let (bytes, st) = e.export_step(&ev, &opts).unwrap();
    assert_eq!(st.solids, 1);
    assert_eq!(st.approximated_edges, 0, "{st:?}");
    let text = std::str::from_utf8(&bytes).unwrap();
    let m = cad_step::p21::parse(text).unwrap();
    // 4 corner arcs + the hole (offset inward: r 2 − 2 = 0, the hole vanishes) → only corners
    assert!(m.count("CIRCLE") >= 8, "{st:?}");
    assert_eq!(m.count("B_SPLINE_CURVE_WITH_KNOTS"), 0);
    assert_eq!(st.surfaces.get("cylinder"), Some(&4), "{st:?}");
}
