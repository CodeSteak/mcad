//! Findings from the everyday models (examples/alltag, Testbericht 13): the minimal examples
//! of tests/alltag/fehler through `cad_eval` and the engine with native SMLib. Each test checks
//! the corrected result, or the clear error where no correct result exists.

mod common;

use cad_eval::{EvalOptions, eval_source};
use cad_ir::Tree;
use common::*;

fn tree(src: &str) -> Tree {
    let out = eval_source(src, &EvalOptions::default());
    let errs: Vec<_> = out.errors().collect();
    assert!(errs.is_empty(), "{errs:#?}");
    out.tree
}

fn fehler(name: &str) -> String {
    let p = format!("{}/../../tests/alltag/fehler/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

/// Evaluates `src`; returns (errors, warnings, root volume).
fn run(src: &str) -> (Vec<String>, Vec<String>, f64) {
    let mut e = engine();
    let ev = e.evaluate(tree(src));
    let v = root_volume(&mut e, &ev);
    (errors(&ev), warnings(&ev), v)
}

#[track_caller]
fn assert_error(errs: &[String], part: &str) {
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert!(errs[0].contains(part), "expected `{part}` in {errs:#?}");
}

// ---------------------------------------------------------------- F1: sweep, spline profile

#[test]
fn f01_symmetric_spline_profile_sweeps_along_a_curved_path() {
    let (errs, _, v) = run(&fehler("f01_sweep_spline_profil.scad"));
    assert!(errs.is_empty(), "{errs:#?}");
    // profile area ≈ π·5·3.08 (the spline bulges a little), path ≈ 48 mm
    assert!(v > 2000.0 && v < 2400.0, "volume {v}");
}

#[test]
fn f01_off_centre_profile_follows_its_origin_on_a_curved_path() {
    // A profile that is not centred (circle at (4, 0)) swept on a symmetric arc: its centre
    // travels 4 mm beside the path; mirrored about the path plane the body is the same, so the
    // volume equals that of the centred circle swept along the offset path (Pappus: area times
    // the length of the centre's path) – here only checked against the centred sweep's volume
    // within the change of path length.
    let path = "[[0, 0, 0], [20, 0, 10], [40, 0, 0]]";
    let (e1, _, v_off) = run(&format!("sweep(path = {path}) translate([4, 0]) circle(1.5);"));
    let (e2, _, v_c) = run(&format!("sweep(path = {path}) circle(1.5);"));
    assert!(e1.is_empty() && e2.is_empty(), "{e1:#?} {e2:#?}");
    assert!((v_off - v_c).abs() < 0.25 * v_c, "{v_off} vs {v_c}");
    assert!((v_off - v_c).abs() > 1e-6 * v_c, "the offset profile must not be moved onto the path");
}

// ---------------------------------------------------------------- F2: one diagnostic

#[test]
fn f02_failed_union_is_reported_once() {
    let src = "union() {\n  cylinder(h = 40, r = 20);\n  sweep(path = [[19, 0, 30], [35, 0, 25], [19, 0, 10]]) circle(3);\n}\n";
    let (errs, _, _) = run(src);
    assert_eq!(errs.len(), 1, "the root union must not repeat the error: {errs:#?}");
    let (errs, _, _) = run(&fehler("f02_union_zylinder_sweep.scad"));
    assert_eq!(errs.len(), 1, "{errs:#?}");
}

// ---------------------------------------------------------------- F3: polygon sweep

#[test]
fn f03_open_polygon_sweep_names_the_cause() {
    let (errs, _, _) = run(&fehler("f03_sweep_polygon16.scad"));
    assert_error(&errs, "seam");
}

// ---------------------------------------------------------------- F5, F10: open blends

#[test]
fn f05_chamfer_that_leaves_an_open_shell_is_an_error() {
    let (errs, _, v) = run(&fehler("f05_fase_volumen_null.scad"));
    assert_error(&errs, "not closed");
    // the body without the chamfer stays (rounded box, wall 3)
    assert!(v > 15000.0, "volume {v}");
    // a chamfer that fits the wall at the corners works
    let ok = fehler("f05_fase_volumen_null.scad").replace("chamfer(0.8)", "chamfer(0.7)");
    let (errs, _, v) = run(&ok);
    assert!(errs.is_empty(), "{errs:#?}");
    assert!(v > 15000.0, "volume {v}");
}

#[test]
fn f10_union_fillet_that_loses_the_shell_is_an_error() {
    let (errs, _, v) = run(&fehler("f10_union_fillet_schale_verloren.scad"));
    assert_error(&errs, "Verrundung gescheitert");
    assert!(v > 20000.0, "the union without the fillet stays: {v}");
}

// ---------------------------------------------------------------- F7: torus as zone

#[test]
fn f07_translated_torus_selects_the_edge_on_its_core_circle() {
    let (errs, warns, _) = run(&fehler("f07_torus_zone.scad"));
    assert!(errs.is_empty() && warns.is_empty(), "{errs:#?} {warns:#?}");
}

// ---------------------------------------------------------------- F8: booleans with tori

#[test]
fn f08_booleans_with_moved_tori_and_revolved_circles() {
    for (f, want) in [("f08a_union_torus.scad", 5852.26), ("f08b_union_ring_verloren.scad", 5852.26)] {
        let (errs, _, v) = run(&fehler(f));
        assert!(errs.is_empty(), "{f}: {errs:#?}");
        assert_approx(f, v, want, 1e-4);
    }
    // the torus touching top and bottom face has no manifold result: an error, not volume 0
    let (errs, _, _) = run(&fehler("f08c_differenz_torus_tangential.scad"));
    assert_error(&errs, "not a closed solid");
    // shifted 0.1 mm off the faces it works: cylinder minus the full torus
    let (errs, _, v) = run("difference() { cylinder(h = 2.2, r = 30, center = true); torus(20, 1); }");
    assert!(errs.is_empty(), "{errs:#?}");
    assert_approx(
        "cyl - torus",
        v,
        std::f64::consts::PI * 900.0 * 2.2 - 2.0 * std::f64::consts::PI.powi(2) * 20.0,
        1e-4,
    );
}

// ---------------------------------------------------------------- F9: crash

#[test]
fn f09_tangent_contact_under_union_fillet_fails_cleanly() {
    let (errs, _, _) = run(&fehler("f09a_absturz_tangentialer_dom.scad"));
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.contains("touch tangentially")), "{errs:#?}");
    let (errs, _, _) = run(&fehler("f09b_absturz_zapfen_sweep.scad"));
    assert_error(&errs, "cannot blend the corner");
}

// ---------------------------------------------------------------- F13: shell message

#[test]
fn f13_shell_thicker_than_a_rounding_names_the_radius() {
    let (errs, _, _) = run(&fehler("f13_shell_verrundung.scad"));
    assert_error(&errs, "radius of a convex rounding");
}
