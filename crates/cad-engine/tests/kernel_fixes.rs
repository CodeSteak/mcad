//! Source → `cad_eval` → engine (native SMLib): shells of rounded bodies, surfaces from few
//! points, seam/pole/tangent edges, and the variable radius (`fillet(r = function (p) …)`).

mod common;

use std::f64::consts::PI;

use cad_eval::{EvalOptions, eval_source};
use cad_ir::{Op, Tree};
use cad_kernel::{Class, Kernel};
use common::*;
use glam::DVec3;

fn tree(src: &str) -> Tree {
    let out = eval_source(src, &EvalOptions::default());
    let errs: Vec<_> = out.errors().collect();
    assert!(errs.is_empty(), "{errs:#?}");
    out.tree
}

fn find(tree: &Tree, pred: impl Fn(&Op) -> bool) -> u32 {
    tree.nodes.iter().position(|n| pred(&n.op)).expect("node") as u32
}

/// Volume of a box a × b × c with all edges rounded at r (Minkowski sum of the inner box and
/// a ball).
fn rounded_box(a: f64, b: f64, c: f64, r: f64) -> f64 {
    let (x, y, z) = (a - 2.0 * r, b - 2.0 * r, c - 2.0 * r);
    x * y * z + 2.0 * r * (x * y + y * z + x * z) + PI * r * r * (x + y + z) + 4.0 / 3.0 * PI * r.powi(3)
}

/// shell(t) of a box a × b × c rounded at r, opened at its flat top: the rounded box minus
/// the inner rounded box (r - t) minus the band of the wall under the flat top.
fn open_shell(a: f64, b: f64, c: f64, r: f64, t: f64) -> f64 {
    rounded_box(a, b, c, r)
        - rounded_box(a - 2.0 * t, b - 2.0 * t, c - 2.0 * t, (r - t).max(0.0))
        - (a - 2.0 * r) * (b - 2.0 * r) * t
}

// ---------------------------------------------------------------- F1: shell of rounded bodies

#[test]
fn shell_of_rounded_boxes() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    for (a, b, c, r, t) in [(90.0, 34.0, 18.0, 3.0, 1.6), (90.0, 34.0, 18.0, 5.0, 1.6), (50.0, 35.0, 30.0, 4.0, 1.5)] {
        let src = format!(
            "grid g {{ z: o = {c}; }}\nshell({t}) {{ box([0, 0, 0], [{a}, {b}, {c}], $fillet = {r}); zone(g.o, d = 1); }}"
        );
        let mut e = engine();
        let ev = e.evaluate(tree(&src));
        assert_clean(&ev);
        let s = find(&ev.tree, |o| matches!(o, Op::Shell { .. }));
        assert!(ev.report(s).hints.contains(&"1 Fläche geöffnet".to_string()), "{:?}", ev.report(s).hints);
        assert_approx(&format!("shell {a}x{b}x{c} r{r} t{t}"), volume(&mut e, &ev, s), open_shell(a, b, c, r, t), 1e-6);
        let parts = ev.parts(s).to_vec();
        assert_eq!(parts.len(), 1);
        let k = e.kernel_mut();
        assert!(k.is_solid(&parts[0].shape));
        // wall, cavity, opening: inside the wall, in the cavity, above the opening
        let cls = k
            .classify(
                &parts[0].shape,
                &[
                    DVec3::new(a / 2.0, t / 2.0, c / 2.0),
                    DVec3::new(a / 2.0, b / 2.0, c / 2.0),
                    DVec3::new(a / 2.0, b / 2.0, c - t / 2.0),
                ],
            )
            .unwrap();
        assert_eq!(cls, vec![Class::Inside, Class::Outside, Class::Outside]);
    }
}

#[test]
fn dose_shell_example_is_a_hollow_box() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/models/spec/dose_shell.mcad"))
        .unwrap();
    let mut e = engine();
    let ev = e.evaluate(tree(&src));
    assert_clean(&ev);
    let s = find(&ev.tree, |o| matches!(o, Op::Shell { .. }));
    assert_approx("dose", volume(&mut e, &ev, s), open_shell(50.0, 35.0, 30.0, 4.0, 1.5), 1e-6);
}

#[test]
fn shell_too_thick_still_fails() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let src = "grid g { z: o = 10; }\nshell(6) { box([0, 0, 0], [20, 20, 10], $fillet = 2); zone(g.o, d = 1); }";
    let mut e = engine();
    let ev = e.evaluate(tree(src));
    let errs = errors(&ev);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].starts_with("shell gescheitert"), "{errs:?}");
}

// ---------------------------------------------------------------- F2: surfaces from 2 or 3 points

#[test]
fn surfaces_from_two_and_three_points_per_direction() {
    // planar grids: the surface is the square 20 x 20 whatever the degree
    for (rows, cols) in [(2usize, 2usize), (3, 3), (2, 5), (5, 2), (3, 4), (4, 4)] {
        let pts: Vec<String> = (0..rows)
            .map(|i| {
                let row: Vec<String> = (0..cols)
                    .map(|j| {
                        format!("[{}, {}, 0]", 20.0 * j as f64 / (cols - 1) as f64, 20.0 * i as f64 / (rows - 1) as f64)
                    })
                    .collect();
                format!("[{}]", row.join(", "))
            })
            .collect();
        let src = format!("surface(points = [{}]);", pts.join(", "));
        let mut e = engine();
        let ev = e.evaluate(tree(&src));
        assert_clean(&ev);
        let r = ev.root().unwrap();
        let mp = e.mass_props(&ev, r).unwrap();
        assert_approx(&format!("area {rows}x{cols}"), mp.area, 400.0, 1e-6);
    }
    // a bilinear patch through four points, thickened
    let src = "thicken(1) surface(points = [[[0, 0, 0], [20, 0, 6]], [[0, 20, 6], [20, 20, 0]]]);";
    let mut e = engine();
    let ev = e.evaluate(tree(src));
    assert_clean(&ev);
    let r = ev.root().unwrap();
    let (lo, hi) = ev.report(r).bounds.unwrap();
    assert!(lo.z > -1.5 && hi.z < 7.5 && hi.x > 19.9 && hi.y > 19.9, "{lo} {hi}");
    assert!(volume(&mut e, &ev, r) > 400.0);
}

// ---------------------------------------------------------------- F3: seams, poles, tangent edges

#[test]
fn seams_and_poles_are_not_edges() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut e = engine();
    let ev = e.evaluate(tree("cylinder(h = 10, r = 5, $chamfer = 1);"));
    assert_clean(&ev);
    let c = find(&ev.tree, |o| matches!(o, Op::Prim { .. }));
    assert_eq!(ev.report(c).hints, vec!["2 Kanten".to_string()]);
    assert_eq!(ev.report(c).selected_edges.len(), 2);
    // two chamfer rings: triangle 1 x 1 / 2 at radius 5 - 1/3
    let want = PI * 25.0 * 10.0 - 2.0 * 2.0 * PI * (5.0 - 1.0 / 3.0) * 0.5;
    // (the chamfer surface is an approximated blend: 6e-6 relative)
    assert_approx("chamfered cylinder", volume(&mut e, &ev, c), want, 2e-5);

    let ev = e.evaluate(tree("sphere(5, $fillet = 1);"));
    let s = find(&ev.tree, |o| matches!(o, Op::Prim { .. }));
    assert_eq!(errors(&ev), Vec::<String>::new());
    assert_eq!(warnings(&ev).len(), 1, "{:?}", warnings(&ev));
    assert!(warnings(&ev)[0].starts_with("0 Kanten"));
    assert_eq!(ev.report(s).hints, vec!["0 Kanten".to_string()]);
    assert_approx("sphere unchanged", volume(&mut e, &ev, s), 4.0 / 3.0 * PI * 125.0, 1e-6);

    // zone around the whole cylinder: top and bottom edge, not the seam
    let ev = e.evaluate(tree("fillet(1) { cylinder(h = 10, r = 5); sphere(20); }"));
    assert_clean(&ev);
    let f = find(&ev.tree, |o| matches!(o, Op::Blend { .. }));
    assert!(ev.report(f).hints.contains(&"2 Kanten".to_string()), "{:?}", ev.report(f).hints);

    // fillet = at a union of two equal coaxial cylinders: the joint is tangent, no new edge
    let ev = e.evaluate(tree(
        "union(fillet = 1) { cylinder(h = 10, r = 5); translate([0, 0, 10]) cylinder(h = 10, r = 5); }",
    ));
    assert_eq!(errors(&ev), Vec::<String>::new());
    let u = find(&ev.tree, |o| matches!(o, Op::Boolean { .. }));
    assert!(ev.report(u).hints.contains(&"0 Kanten".to_string()), "{:?}", ev.report(u).hints);
    assert_approx("union unchanged", volume(&mut e, &ev, u), PI * 25.0 * 20.0, 1e-6);
}

#[test]
fn tangent_edges_of_rounds_are_not_selected() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    // all edges of a rounded box are tangent: nothing left to round
    let mut e = engine();
    let ev = e.evaluate(tree("fillet(1) { cube(20, $fillet = 2); }"));
    assert_eq!(errors(&ev), Vec::<String>::new());
    let f = find(&ev.tree, |o| matches!(o, Op::Blend { .. }));
    assert!(ev.report(f).hints.contains(&"0 Kanten".to_string()), "{:?}", ev.report(f).hints);
}

#[test]
fn coplanar_merged_faces_keep_old_edges_old() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    // union(chamfer) of a slab and a tower sharing the front plane: only the two concave
    // edges where the tower meets the slab's top are new
    let src = "union(chamfer = 0.5) { cube([30, 10, 5]); translate([10, 0, 0]) cube([10, 10, 15]); }";
    let mut e = engine();
    let ev = e.evaluate(tree(src));
    assert_clean(&ev);
    let u = find(&ev.tree, |o| matches!(o, Op::Boolean { .. }));
    assert_eq!(ev.report(u).hints, vec!["2 Kanten".to_string()]);
    // two concave chamfers add 0.5 * 0.5^2 * 10 each
    assert_approx("chamfered union", volume(&mut e, &ev, u), 1500.0 + 1000.0 + 2.5, 1e-6);
}

// ---------------------------------------------------------------- variable radius

/// Radius of the round on the top front edge (y = 0, z = 20) at x: where the top face begins.
fn probe_radius(e: &mut E, shape: &kernel_smlib::Shape, x: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, 10.0);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        let c = e.kernel_mut().classify(shape, &[DVec3::new(x, mid, 20.0 - 1e-4)]).unwrap();
        if c[0] == Class::Inside {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

#[test]
fn variable_radius_linear_law_on_one_edge() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let src = r#"
grid g { x: a = 0, b = 30; y: v = 0, h = 20; z: u = 0, o = 20; }
fillet(r = function (p) 1 + p.x / 15) {
    box(g[a, v, u], g[b, h, o]);
    zone(g[*, v, o], d = 1);
}"#;
    let mut e = engine();
    let ev = e.evaluate(tree(src));
    assert_clean(&ev);
    let f = find(&ev.tree, |o| matches!(o, Op::Blend { .. }));
    let hints = &ev.report(f).hints;
    assert!(hints.contains(&"1 Kante".to_string()) && hints.contains(&"r 1–3".to_string()), "{hints:?}");
    // removed: (1 - pi/4) * integral of (1 + x/15)^2 over 0..30 = (1 - pi/4) * 130
    assert_approx("linear law", volume(&mut e, &ev, f), 12000.0 - (1.0 - PI / 4.0) * 130.0, 1e-5);
    let shape = ev.parts(f)[0].shape.clone();
    for x in [2.0, 7.5, 15.0, 22.5, 28.0] {
        let r = probe_radius(&mut e, &shape, x);
        let want = 1.0 + x / 15.0;
        println!("probe x={x} r={r:.4} law={want:.4}");
        // the contact line of a ball of growing radius lies slightly behind r(x)
        assert!((r - want).abs() < 0.02 * want, "r({x}) = {r}, law {want}");
    }
}

#[test]
fn variable_radius_step_law_between_constant_radii() {
    let body = "box([0, 0, 0], [30, 30, 20]);";
    let mut e = engine();
    let mut vol = |src: String, hint: Option<&str>| {
        let ev = e.evaluate(tree(&src));
        assert_clean(&ev);
        let f = find(&ev.tree, |o| matches!(o, Op::Blend { .. }));
        if let Some(h) = hint {
            assert!(ev.report(f).hints.contains(&h.to_string()), "{:?}", ev.report(f).hints);
            assert_eq!(ev.report(f).selected_edges.len(), 12);
        }
        volume(&mut e, &ev, f)
    };
    let v2 = vol(format!("fillet(2) {{ {body} }}"), None);
    let v4 = vol(format!("fillet(4) {{ {body} }}"), None);
    let vv = vol(format!("fillet(r = function (p) p.z < 10 ? 2 : 4) {{ {body} }}"), Some("r 2–4"));
    assert_approx("r = 2", v2, rounded_box(30.0, 30.0, 20.0, 2.0), 1e-6);
    assert_approx("r = 4", v4, rounded_box(30.0, 30.0, 20.0, 4.0), 1e-6);
    println!("volumes r2={v2:.3} law={vv:.3} r4={v4:.3}");
    assert!(v4 < vv && vv < v2, "{v4} < {vv} < {v2}");
    // a constant function is the constant fillet
    let vc = vol(format!("fillet(r = function (p) 2) {{ {body} }}"), Some("r 2"));
    assert_approx("constant law", vc, v2, 1e-9);
}

#[test]
fn variable_radius_captured_values_user_functions_and_booleans() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let src = r#"
lo = 1;
function steigung(x, k = 40) = x / k;
module tasche(f) {
    difference(fillet = f) {
        cube([40, 30, 10]);
        translate([5, 8, 4]) cube([30, 14, 10]);
    }
}
tasche(function (p) let(s = steigung(p.x)) lo + s);
"#;
    let mut e = engine();
    let ev = e.evaluate(tree(src));
    assert_clean(&ev);
    let d = find(&ev.tree, |o| matches!(o, Op::Boolean { fillet_law: Some(_), .. }));
    assert!(ev.report(d).hints.contains(&"4 Kanten".to_string()), "{:?}", ev.report(d).hints);
    assert!(ev.report(d).hints.contains(&"r 1.125–1.875".to_string()), "{:?}", ev.report(d).hints);
    let vv = volume(&mut e, &ev, d);
    let vol_const = |e: &mut E, r: f64| {
        let ev = e.evaluate(tree(&format!(
            "difference(fillet = {r}) {{ cube([40, 30, 10]); translate([5, 8, 4]) cube([30, 14, 10]); }}"
        )));
        assert_clean(&ev);
        let d = find(&ev.tree, |o| matches!(o, Op::Boolean { .. }));
        volume(e, &ev, d)
    };
    let (vmin, vmax) = (vol_const(&mut e, 1.875), vol_const(&mut e, 1.125));
    println!("pocket r1.875={vmin:.3} law={vv:.3} r1.125={vmax:.3}");
    assert!(vmin < vv && vv < vmax, "{vmin} < {vv} < {vmax}");
}

#[test]
fn variable_radius_errors_like_constant_radius() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut e = engine();
    // radius <= 0 somewhere: error with the position, body unchanged
    let ev = e.evaluate(tree("fillet(r = function (p) p.x - 5) { cube(20); }"));
    let errs = errors(&ev);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("≤ 0 bei ["), "{errs:?}");
    let f = find(&ev.tree, |o| matches!(o, Op::Blend { .. }));
    assert_approx("unchanged", volume(&mut e, &ev, f), 8000.0, 1e-9);
    assert_eq!(ev.report(f).status, cad_engine::Status::Error);
    // too large: the kernel's error, body unchanged
    let ev = e.evaluate(tree("fillet(r = function (p) 8 + p.z / 2) { cube(20); }"));
    let errs = errors(&ev);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].starts_with("Verrundung gescheitert (8–18 mm)"), "{errs:?}");
    let f = find(&ev.tree, |o| matches!(o, Op::Blend { .. }));
    assert_approx("unchanged", volume(&mut e, &ev, f), 8000.0, 1e-9);
    // not a number
    let ev = e.evaluate(tree("fillet(r = function (p) p) { cube(20); }"));
    let errs = errors(&ev);
    assert!(errs.len() == 1 && errs[0].contains("keine Zahl"), "{errs:?}");
}

#[test]
fn law_changes_change_the_cache_key() {
    let mut e = engine();
    let a = e.evaluate(tree("k = 2; fillet(r = function (p) k + p.z / 20) { cube(20); }"));
    let b = e.evaluate(tree("k = 2.5; fillet(r = function (p) k + p.z / 20) { cube(20); }"));
    let c = e.evaluate(tree("k = 2; fillet(r = function (p) k + p.z / 20) { cube(20); }"));
    let blend = |t: &Tree| find(t, |o| matches!(o, Op::Blend { .. }));
    let (ra, rb, rc) = (blend(&a.tree), blend(&b.tree), blend(&c.tree));
    assert_ne!(a.report(ra).key, b.report(rb).key);
    assert_eq!(a.report(ra).key, c.report(rc).key);
    assert!(a.report(ra).hints.contains(&"r 2–3".to_string()), "{:?}", a.report(ra).hints);
}
