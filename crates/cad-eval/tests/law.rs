//! Radius laws: `fillet(r = function (p) …)` compiled into `cad_ir::Law`.

mod common;
use cad_ir::{Law, Op};
use common::*;
use glam::DVec3;

fn law_of(out: &cad_eval::EvalOutput) -> &Law {
    out.tree
        .nodes
        .iter()
        .find_map(|n| match &n.op {
            Op::Blend { law: Some(l), .. } => Some(l.as_ref()),
            Op::Boolean { fillet_law: Some(l), .. } => Some(l.as_ref()),
            _ => None,
        })
        .expect("a node with a radius law")
}

fn at(l: &Law, x: f64, y: f64, z: f64) -> f64 {
    l.eval(DVec3::new(x, y, z)).unwrap()
}

#[test]
fn step_law_and_text() {
    let out = clean("fillet(r = function (p) p.z < 10 ? 2 : 4) cube(20);");
    let l = law_of(&out);
    assert_eq!(at(l, 0.0, 0.0, 5.0), 2.0);
    assert_eq!(at(l, 0.0, 0.0, 15.0), 4.0);
    assert_eq!(l.text, "function (p) p.z < 10 ? 2 : 4");
    // positional radius as well; r is 0 when a law is given
    let out = clean("fillet(function (q) q[0] / 10 + 1) cube(20);");
    let l = law_of(&out);
    assert_eq!(at(l, 20.0, 0.0, 0.0), 3.0);
    assert!(out.tree.nodes.iter().any(|n| matches!(n.op, Op::Blend { r, law: Some(_), .. } if r == 0.0)));
}

#[test]
fn captured_values_functions_let_and_builtins() {
    let src = r#"
grid g { z: mitte = 10; }
basis = 1.5;
tab = [for (i = [0:2]) [i * 10, basis + i]];
function rampe(t, k = 2) = t / k;
f = function (x) max(x, 0.5);
fillet(r = function (p) let(d = norm([p.x, p.y])) f(basis + rampe(d) - (p.z > g.mitte ? 1 : 0)) + lookup(p.z, tab) - basis) cube(20);
"#;
    let out = clean(src);
    let l = law_of(&out);
    // p = [6, 8, 0]: d = 10, rampe = 5, f(6.5) = 6.5, lookup(0) = 1.5 → 6.5
    assert!((at(l, 6.0, 8.0, 0.0) - 6.5).abs() < 1e-12);
    // p = [0, 0, 15]: f(max(0.5, 0.5)) = 0.5, lookup(15) = 3 → 0.5 + 3 - 1.5
    assert!((at(l, 0.0, 0.0, 15.0) - 2.0).abs() < 1e-12);
    assert!((at(l, 0.0, 0.0, 20.0) - (0.5 + 3.5 - 1.5)).abs() < 1e-12);
}

#[test]
fn boolean_parameter_and_trig() {
    let out =
        clean("difference(fillet = function (p) 1 + sin(p.x * 9) / 2) { cube(20); translate([5, 5, 5]) cube(20); }");
    let l = law_of(&out);
    assert!((at(l, 10.0, 0.0, 0.0) - 1.5).abs() < 1e-12);
    assert!((at(l, 0.0, 0.0, 0.0) - 1.0).abs() < 1e-12);
}

#[test]
fn unsupported_parts_are_errors() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let out = run("fillet(r = function (p) str(p.x)) cube(20);");
    assert!(has_error(&out, "Radiusfunktion"), "{:?}", errors(&out));
    let out = run("fillet(r = function (p) [for (i = [0:p.x]) i][0]) cube(20);");
    assert!(has_error(&out, "Listenausdrücke"), "{:?}", errors(&out));
    let out = run("function f(p) = f(p); fillet(r = function (p) f(p)) cube(20);");
    assert!(has_error(&out, "verschachtelt"), "{:?}", errors(&out));
    let out = run("fillet(r = function () 2) cube(20);");
    assert!(has_error(&out, "Parameter"), "{:?}", errors(&out));
    // chamfer and $fillet take no function
    let out = run("chamfer(function (p) 1) cube(20);");
    assert!(has_warning(&out, "Funktion"), "{:?}", warnings(&out));
    let out = run("cube(20, $fillet = function (p) 1);");
    assert!(has_warning(&out, "$fillet"), "{:?}", warnings(&out));
}

#[test]
fn law_is_part_of_the_tree_data() {
    // equal sources give equal laws, a changed captured value a different one
    let a = clean("k = 2; fillet(r = function (p) k + p.z) cube(20);");
    let b = clean("k = 3; fillet(r = function (p) k + p.z) cube(20);");
    let c = clean("k = 2; fillet(r = function (p) k + p.z) cube(20);");
    assert_ne!(law_of(&a), law_of(&b));
    assert_eq!(law_of(&a), law_of(&c));
    assert_eq!(at(law_of(&b), 0.0, 0.0, 1.0), 4.0);
}
