//! Regression tests for deviations from OpenSCAD 2021.01 found by probing. Language
//! behaviour is compared with `openscad -o x.echo` when it is installed (the expected
//! lines below are its output); geometry with values measured on OpenSCAD's STL export.

mod common;

use std::process::Command;

use cad_eval::HandleKind;
use cad_ir::{Op, Prim, Profile};
use cad_syntax::Severity;
use common::*;

const SRC: &str = r#"
module m(a, b = a * 2) echo(b = b);
m(1);
function f(a, b = a) = b;
echo(f(1), f(1, 2));
function h(a, b = c, c = 2) = [b, c];
echo(h(1));
module n(b = $fn) echo(b = b);
module w() { $fn = 7; n(); }
n($fn = 5);
w();
module outer(a = 3) { module inner(a, b = a) echo(inner_b = b); inner(7); }
outer();
echo(min([1, [2]]), max([1, [2]]), min(1, [2]), min([3, "a", 1]), min([]), min(), max("a"), min(1, "b", 0));
echo(min([1, 2], [3]), max([[1], 2]), min(undef), min(1, undef), max([1, undef, 3]), min(0/0, 1), max(1, 0/0));
echo(min([[2], [1]]), min([[1, 2], [1]]), min(["b", "a"]), min([true, false]), max([[1, 2], [1, 3]]), min([1, true]));
echo(search(["l", "o"], "Hallo"), search("lo", "Hallo"), search("l", "Hallo", 0), search("lx", "Hallo"));
echo(search("a", "aaa", 2), search(["a", "b"], ["a", "b", "a"]), search(["a", "b"], ["a", "b", "a"], 0));
echo(search([[1, 2]], [[1, 2], [3, 4]]), search(5, [1, 2]), search([5], [1, 2]), search("", "abc"));
echo(search("abc", [["a", 1], ["b", 2]]), search(1, "abc"), search(undef, [undef, 1]), search(true, [true]));
echo(search("1", [[1, 2]]), search("a", [["ab", 1]]), search("a", [["xa", 1]]), search("a", [["a"]], 0, 1));
echo(search(["ab"], ["ab", "cd"]), search("ab", ["ab", "cd"]), search([1, 2], [1, 2, 1], 0), search(1, [[2, 1]], 0, 1));
echo(123456.5, 1234565, 12345.25, 1234.125, -123456.5, -1234.125, 100000.5, str(123456.5));
echo(lookup(1.5, [[1, 10, 5], [2, 20]]), lookup(1.5, [[1, 10], [2]]), lookup(1/0, [[1, 1]]), lookup(1.5, [[1, 10], [2, 1/0], [3, 30]]), lookup(1, [[0, 0], [2, "a"]]));
echo([] * [], [1, 2, 3][-0.5], [10, 20][1.9], 0/0 ? 1 : 2, +"a", +true);
echo(let(a = 1, a = 2) a, [each undef], [each 5], chr(65.5), chr(0.9));
echo([for (i = [0 : 0.1 : 0.3]) i], len([for (i = [0 : 0.1 : 1.2]) i]));
echo($a = 1, b = 2, $c = 3);
echo(function(x) -x + !x * 2.50 - 1e10 / 3 % 2 ^ 2);
echo(function(a, b = [1, "s\"q", true, undef]) a ? b : [a : 2 : b]);
echo(function(v) [for (i = [0 : 2], j = [1 : 2]) if (i > j) i else j], function() let(a = 1) [each a, for (i = 0; i < 2; i = i + 1) i]);
3x = 4;
echo(3x, 1e2);
module dup(x = 1) echo(x = x);
dup(x = 2, x = 3);
echo(len(rands(0, 1, -2)), len(rands(0, 1, 0.5)));
"#;

/// `openscad -o x.echo` of [`SRC`] (OpenSCAD 2021.01).
const EXPECTED: &[&str] = &[
    r#"b = undef"#,
    r#"undef, 2"#,
    r#"[undef, 2]"#,
    r#"b = 0"#,
    r#"b = 7"#,
    r#"inner_b = 3"#,
    r#"1, 1, undef, 1, undef, undef, undef, undef"#,
    r#"undef, [1], undef, undef, 3, nan, 1"#,
    r#"[1], [1], "a", false, [1, 3], 1"#,
    r#"[[], []], [2, 4], [[2, 3]], [2]"#,
    r#"[[0, 1]], [0, 1], [[0, 2], [1]]"#,
    r#"[0], [], [[]], []"#,
    r#"[0, 1], [], undef, undef"#,
    r#"[0], [0], [], []"#,
    r#"[0], [], [[0, 2], [1]], [0]"#,
    r#"123457, 1.23457e+6, 12345.3, 1234.13, -123457, -1234.13, 100001, "123457""#,
    r#"undef, 10, undef, inf, 0"#,
    r#"undef, 1, 20, 1, "a", true"#,
    r#"1, [], [5], "A", """#,
    r#"[0, 0.1, 0.2, 0.3], 13"#,
    r#"$a = 1, b = 2, $c = 3"#,
    r#"function(x) ((-x + (!x * 2.5)) - ((1e+10 / 3) % (2 ^ 2)))"#,
    r#"function(a, b = [1, "s\"q", true, undef]) (a ? b : [a : 2 : b])"#,
    r#"function(v) [for(i = [0 : 2]) (for(j = [1 : 2]) (if((i > j)) (i) else (j)))], function() let(a = 1) [each (a), for(i = 0;(i < 2);i = (i + 1)) i]"#,
    r#"4, 100"#,
    r#"x = 3"#,
    r#"2, 0"#,
];

#[test]
fn language_deviations_match_openscad() {
    let out = run(SRC);
    assert_eq!(echoes(&out), EXPECTED);
    // The expectations are OpenSCAD's output: check them against it when installed.
    let dir = env!("CARGO_TARGET_TMPDIR");
    let (scad, echo) = (format!("{dir}/deviations.scad"), format!("{dir}/deviations.echo"));
    std::fs::write(&scad, SRC).unwrap();
    let Ok(st) = Command::new("openscad").args(["-o", &echo, &scad]).output() else {
        eprintln!("openscad not installed, skipping comparison");
        return;
    };
    assert!(st.status.success(), "openscad failed: {}", String::from_utf8_lossy(&st.stderr));
    let theirs: Vec<String> = std::fs::read_to_string(&echo)
        .unwrap()
        .lines()
        .filter_map(|l| l.strip_prefix("ECHO: "))
        .map(String::from)
        .collect();
    assert_eq!(theirs, EXPECTED);
}

#[test]
fn defaults_in_module_context_use_the_definition_scope() {
    let src = "a = 10; module m(a = 1, b = a * 2) cube(b);";
    let out = run_module(src, "m", vec![]);
    let b = out.context.params.iter().find(|p| p.name == "b").unwrap();
    assert_eq!(b.value, cad_eval::PValue::Num(20.0));
}

fn prims(out: &cad_eval::EvalOutput) -> Vec<&Prim> {
    out.tree
        .nodes
        .iter()
        .filter_map(|n| match &n.op {
            Op::Prim { prim, .. } => Some(prim),
            _ => None,
        })
        .collect()
}

fn profiles(out: &cad_eval::EvalOutput) -> Vec<&Profile> {
    out.tree
        .nodes
        .iter()
        .filter_map(|n| match &n.op {
            Op::Profile(p) => Some(p),
            _ => None,
        })
        .collect()
}

#[test]
fn diameter_wins_over_radius() {
    // OpenSCAD: sphere r = 2 (bbox ±1.85 at $fn = 0), cylinder r1 = 4 / r2 = 1,
    // circle r = 3, each with "Ignoring radius variable … as diameter … is defined too".
    let src = "sphere(r = 5, d = 4);\ncylinder(h = 1, r = 5, d = 4);\ncylinder(h = 1, r1 = 3, d1 = 8, r2 = 1);\n\
               cylinder(h = 1, r = 1, d1 = 4);\ncylinder(1, 5, d = 4);\ncircle(1, d = 6);";
    let out = run(src);
    let p = prims(&out);
    assert!(matches!(p[0], Prim::Sphere { r } if approx(*r, 2.0)), "{:?}", p[0]);
    let cyl = |i: usize| match p[i] {
        Prim::Cylinder { r1, r2, .. } => (*r1, *r2),
        other => panic!("{other:?}"),
    };
    assert_eq!(cyl(1), (2.0, 2.0));
    assert_eq!(cyl(2), (4.0, 1.0));
    // `d1` sets the bottom, `r` the top ("Cylinder parameters ambiguous").
    assert_eq!(cyl(3), (2.0, 1.0));
    assert_eq!(cyl(4), (5.0, 2.0));
    assert!(matches!(profiles(&out)[0], Profile::Circle { r } if approx(*r, 3.0)));
    assert!(has_warning(&out, "`r` ignoriert, da auch `d` angegeben ist"));
    assert!(has_warning(&out, "`r1` ignoriert, da auch `d1` angegeben ist"));
    assert!(has_warning(&out, "mehrdeutige Zylinderparameter"));
    // The radius handle edits the diameter literal (scale 2).
    let hs: Vec<_> = out.handles.iter().filter(|h| h.kind == HandleKind::Radius).collect();
    let lit = |h: &cad_eval::Handle| h.comps[0].map(|c| (span_text(src, c.span), c.scale));
    assert_eq!(lit(hs[0]), Some(("4", 2.0)));
    assert!(approx(hs[0].value[0], 2.0));
    let cyl3: Vec<_> = hs.iter().filter(|h| span_text(src, h.span).starts_with("cylinder(h = 1, r1")).collect();
    assert_eq!(cyl3.iter().map(|h| lit(h)).collect::<Vec<_>>(), [Some(("8", 2.0)), Some(("1", 1.0))]);
}

#[test]
fn small_fn_is_clamped_to_three() {
    // OpenSCAD: `$fn` 1 or 2 gives a triangle (6 vertices for the cylinder).
    for f in ["1", "2", "0.5"] {
        let out = run(&format!("cylinder(h = 2, r = 3, $fn = {f});"));
        let Prim::Polyhedron { points, .. } = prims(&out)[0] else { panic!("$fn = {f}: {:?}", prims(&out)) };
        assert_eq!(points.len(), 6, "$fn = {f}");
    }
}

#[test]
fn fn_at_an_extrusion_applies_to_its_circles() {
    // OpenSCAD: `linear_extrude(1, $fn = 6) circle(1)` is a hexagonal prism (12 vertices).
    let out = run("linear_extrude(1, $fn = 6) circle(1);");
    let Profile::Polygon { loops } = profiles(&out)[0] else { panic!("{:?}", profiles(&out)) };
    assert_eq!(loops[0].len(), 6);
    // Through a module call too (`$fn` is dynamic).
    let out = run("module c() circle(1); linear_extrude(1, $fn = 5) translate([2, 0]) c();");
    assert!(matches!(profiles(&out)[0], Profile::Polygon { loops } if loops[0].len() == 5));
    // A nearer `$fn` that is not at a call keeps the circle exact (with a hint).
    let out = run("linear_extrude(1, $fn = 6) { $fn = 30; circle(1); }");
    assert!(matches!(profiles(&out)[0], Profile::Circle { .. }));
    assert!(out.diagnostics.iter().any(|d| d.severity == Severity::Info && d.message.contains("`$fn` ignoriert")));
    // rotate_extrude: the profile circle becomes a hexagon; the revolution stays exact
    // (OpenSCAD makes 6 segments) and an info says so.
    let out = run("rotate_extrude($fn = 6) translate([2, 0]) circle(1);");
    assert!(matches!(profiles(&out)[0], Profile::Polygon { loops } if loops[0].len() == 6));
    assert!(out.tree.nodes.iter().any(|n| matches!(n.op, Op::RotateExtrude { angle } if angle == 360.0)));
    assert!(out.diagnostics.iter().any(|d| d.severity == Severity::Info && d.message.contains("Umlauf bleibt exakt")));
    // Above 12 nothing changes.
    let out = run("linear_extrude(1, $fn = 40) circle(1);");
    assert!(matches!(profiles(&out)[0], Profile::Circle { .. }));
}

#[test]
fn degenerate_extrusions_are_empty() {
    // OpenSCAD exports nothing for these.
    for src in [
        "linear_extrude(height = -3) square(1);",
        "linear_extrude(height = 0) square(1);",
        "rotate_extrude(angle = 0) translate([2, 0]) square(1);",
    ] {
        let out = run(src);
        assert_eq!(out.tree.nodes.len(), 2, "{src}: {}", dump(&out));
        assert!(matches!(out.tree.nodes[1].op, Op::Empty), "{src}");
        assert!(!warnings(&out).is_empty(), "{src}");
    }
    let out = run("linear_extrude(height = -3) square(1);");
    assert!(has_warning(&out, "linear_extrude: Höhe -3 ≤ 0, kein Körper"));
    // The height handle stays, so the value can be dragged back.
    assert!(out.handles.iter().any(|h| h.kind == HandleKind::Height));
}

#[test]
fn rotate_extrude_angle_like_openscad() {
    let angle = |src: &str| {
        let out = run(src);
        out.tree
            .nodes
            .iter()
            .find_map(|n| match n.op {
                Op::RotateExtrude { angle } => Some(angle),
                _ => None,
            })
            .unwrap()
    };
    // OpenSCAD: outside (-360, 360] a full turn (same mesh as without `angle`).
    assert_eq!(angle("rotate_extrude(angle = 400) translate([2, 0]) square(1);"), 360.0);
    assert_eq!(angle("rotate_extrude(angle = -400) translate([2, 0]) square(1);"), 360.0);
    assert_eq!(angle("rotate_extrude(angle = -360) translate([2, 0]) square(1);"), 360.0);
    // Negative angles turn clockwise: OpenSCAD's bbox for -90 is x 0…3, y -3…0.
    assert_eq!(angle("rotate_extrude(angle = -90) translate([2, 0]) square(1);"), -90.0);
    assert_eq!(angle("rotate_extrude(angle = 360) translate([2, 0]) square(1);"), 360.0);
    let out = run("rotate_extrude(angle = 400) translate([2, 0]) square(1);");
    assert!(has_warning(&out, "Winkel 400° wird zu 360°"));
}

#[test]
fn invalid_primitive_dimensions_give_nothing() {
    // OpenSCAD exports nothing for each of these.
    for src in [
        "cube(-1);",
        "cube([2, -1, 1]);",
        "cube(0);",
        "cube(1/0);",
        "sphere(-1);",
        "sphere(0, $fn = 6);",
        "cylinder(h = -2, r = 1);",
        "cylinder(h = 2, r = -1);",
        "cylinder(h = 2, r1 = 0, r2 = 0);",
        "cylinder(h = 1, r = 1/0);",
        "square([2, -1]);",
        "circle(-1);",
    ] {
        let out = run(src);
        assert!(matches!(out.tree.nodes[1].op, Op::Empty), "{src}: {}", dump(&out));
        assert!(has_warning(&out, "kein Körper"), "{src}");
    }
    // A cone with one zero radius is fine.
    let out = clean("cylinder(h = 2, r1 = 1, r2 = 0);");
    assert!(matches!(prims(&out)[0], Prim::Cylinder { .. }));
}

#[test]
fn only_true_centers() {
    // OpenSCAD checks the type: `center = 1` or a string does not center (STL bbox of
    // the cylinder and of the extrusion: z 0…2).
    let src = "cylinder(h = 2, r = 1, center = 1);\ncube(2, center = \"yes\");\nsquare(2, center = 1);\n\
               linear_extrude(2, center = 1) square(1);\ncube(2, center = true);";
    let out = run(src);
    let places: Vec<_> = out
        .tree
        .nodes
        .iter()
        .filter_map(|n| match &n.op {
            Op::Prim { prim, place } => Some((prim, place.w_axis.z)),
            _ => None,
        })
        .collect();
    assert_eq!(places[0].1, 0.0);
    assert!(matches!(places[1].0, Prim::Box { min, .. } if *min == glam::DVec3::ZERO));
    assert!(matches!(places[2].0, Prim::Box { min, .. } if *min == glam::DVec3::splat(-1.0)));
    assert!(matches!(profiles(&out)[0], Profile::Rect { min, .. } if *min == glam::DVec2::ZERO));
    assert!(out.tree.nodes.iter().any(|n| matches!(n.op, Op::LinearExtrude { center: false, .. })));
}

#[test]
fn negative_extrude_scale_becomes_zero() {
    // OpenSCAD: `scale = [2, -1]` extrudes to a wedge (top edge of length 2, 6 vertices).
    let out = run("linear_extrude(1, scale = [2, -1]) square(1);");
    let s = out
        .tree
        .nodes
        .iter()
        .find_map(|n| match n.op {
            Op::LinearExtrude { scale, .. } => Some(scale),
            _ => None,
        })
        .unwrap();
    assert_eq!(s, glam::DVec2::new(2.0, 0.0));
    assert!(has_warning(&out, "negativer `scale`"));
}
