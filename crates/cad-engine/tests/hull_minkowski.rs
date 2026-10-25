//! `hull()` and `minkowski()` (Plan 21, phase A) through the engine with native SMLib: the exact
//! special cases against closed formulas, surfaces in STEP, face tags of the operation, and the
//! warning with its reason where no special case applies.

mod common;

use std::f64::consts::PI;

use cad_eval::{EvalOptions, eval_source};
use cad_ir::{NodeIdx, Tree};
use common::*;

const VREL: f64 = 1e-6;

fn tree(src: &str) -> Tree {
    let out = eval_source(src, &EvalOptions::default());
    let errs: Vec<_> = out.errors().collect();
    assert!(errs.is_empty(), "{errs:#?}");
    out.tree
}

/// Volume of the root of a clean model.
fn vol(src: &str) -> f64 {
    let mut e = engine();
    let ev = e.evaluate(tree(src));
    assert_clean(&ev);
    root_volume(&mut e, &ev)
}

/// Area of a 2D expression: volume of its extrusion by 1.
fn area(src2d: &str) -> f64 {
    vol(&format!("linear_extrude(1) {src2d}"))
}

fn node(ev: &Ev, label: &str) -> NodeIdx {
    ev.reports.iter().position(|r| r.label == label).expect("node") as NodeIdx
}

fn hints_of(ev: &Ev, label: &str) -> Vec<String> {
    ev.reports.iter().filter(|r| r.label == label).flat_map(|r| r.hints.clone()).collect()
}

/// Surface kinds of the STEP export.
fn surfaces(src: &str) -> std::collections::BTreeMap<String, usize> {
    let mut e = engine();
    let ev = e.evaluate(tree(src));
    assert_clean(&ev);
    let (_, st) = e.export_step(&ev, &cad_engine::StepOptions { name: "t".into(), ..Default::default() }).unwrap();
    assert_eq!(st.solids, 1, "{st:?}");
    assert_eq!(st.approximated_edges, 0, "{st:?}");
    st.surfaces.into_iter().collect()
}

fn map(list: &[(&str, usize)]) -> std::collections::BTreeMap<String, usize> {
    list.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

// ---------------------------------------------------------------- 2D

#[test]
fn hull_2d_formulas() {
    let (r, l) = (3.0, 10.0);
    // stadium
    assert_approx(
        "stadium",
        area(&format!("hull() {{ circle({r}); translate([{l}, 0]) circle({r}); }}")),
        PI * r * r + 2.0 * r * l,
        VREL,
    );
    // two circles of different radii: sectors + tangent trapezoid
    let (r1, r2, d): (f64, f64, f64) = (5.0, 2.0, 12.0);
    let a = ((r1 - r2) / d).acos();
    let t = (d * d - (r1 - r2).powi(2)).sqrt();
    let want = r1 * r1 * (PI - a) + r2 * r2 * a + t * (r1 + r2);
    assert_approx("belt", area(&format!("hull() {{ circle({r1}); translate([{d}, 0]) circle({r2}); }}")), want, VREL);
    // square + circle on its edge: half disc added
    assert_approx(
        "square + circle",
        area("hull() { square(10); translate([10, 5]) circle(5); }"),
        100.0 + PI * 12.5,
        VREL,
    );
    // circle inside a circle, a single circle
    assert_approx("circle in circle", area("hull() { circle(5); translate([1, 1]) circle(2); }"), PI * 25.0, VREL);
    // children that are Booleans count with their region (the hole does not matter)
    assert_approx(
        "difference",
        area("hull() { difference() { square(10); translate([5, 5]) circle(2); } }"),
        100.0,
        VREL,
    );
    // rounded rectangle from four circles: (w + 2r)(h + 2r) − (4 − π) r²
    let src = "hull() { for (x = [0, 20], y = [0, 10]) translate([x, y]) circle(2); }";
    assert_approx("four circles", area(src), 24.0 * 14.0 - (4.0 - PI) * 4.0, VREL);
}

#[test]
fn hull_2d_of_text_is_convex_and_contains_the_glyph() {
    let glyph = area("text(\"G\", size = 20);");
    let h = area("hull() text(\"G\", size = 20);");
    assert!(h > glyph * 1.1, "{h} vs {glyph}");
    // the hull of the hull is the hull
    assert_approx("idempotent", area("hull() hull() text(\"G\", size = 20);"), h, 1e-9);
    // and offsetting the hull by r adds perimeter r + π r² (convex)
    let mut e = engine();
    let ev = e.evaluate(tree("hull() text(\"G\", size = 20);"));
    assert_clean(&ev);
}

#[test]
fn hull_2d_extruded_and_revolved_is_exact() {
    // the stadium extruded: planes and cylinder faces (the kernel splits arcs), no B-splines
    let s = surfaces("linear_extrude(2) hull() { circle(3); translate([10, 0]) circle(3); }");
    assert!(s.keys().all(|k| k == "plane" || k == "cylinder") && s.contains_key("cylinder"), "{s:?}");
    // revolved: tori from the arcs; Pappus V = 2π x̄ A
    let a = PI * 4.0 + 2.0 * 2.0 * 6.0;
    let v = vol("rotate_extrude() hull() { translate([10, 0]) circle(2); translate([10, 6]) circle(2); }");
    assert_approx("pappus", v, 2.0 * PI * 10.0 * a, VREL);
}

#[test]
fn hull_2d_degenerate_warns() {
    let mut e = engine();
    let ev = e.evaluate(tree("linear_extrude(1) hull() polygon([[0, 0], [5, 0], [10, 0]]);"));
    assert!(warnings(&ev).iter().any(|w| w.contains("keine Fläche")), "{:?}", warnings(&ev));
}

#[test]
fn minkowski_2d() {
    // profile + circle = offset(r)
    assert_approx(
        "rect + circle",
        area("minkowski() { square([10, 20]); circle(2); }"),
        200.0 + 60.0 * 2.0 + 4.0 * PI,
        VREL,
    );
    // a moved circle moves the result, circles add up
    assert_approx("circles", area("minkowski() { translate([5, 5]) circle(1); circle(2); }"), 9.0 * PI, VREL);
    // two convex polygons: A + B + mixed area (square + right triangle 4 × 3: 100 + 6 + 10 (4 + 3))
    assert_approx("convex", area("minkowski() { square(10); polygon([[0, 0], [4, 0], [0, 3]]); }"), 176.0, VREL);
    // non-convex with a circle is still exact (rounded offset): L with 5 convex, 1 concave corner
    let l = "polygon([[0, 0], [20, 0], [20, 5], [5, 5], [5, 20], [0, 20]])";
    let want = 175.0 + 80.0 * 2.0 + (5.0 * PI / 4.0 - 1.0) * 4.0;
    assert_approx("L + circle", area(&format!("minkowski() {{ {l}; circle(2); }}")), want, VREL);
    // two non-convex profiles: warning, children unchanged
    let mut e = engine();
    let ev = e.evaluate(tree(&format!("linear_extrude(1) minkowski() {{ {l}; square(2); }}")));
    assert!(warnings(&ev).iter().any(|w| w.contains("minkowski: zwei Profile nur exakt")), "{:?}", warnings(&ev));
    assert_approx("unchanged", root_volume(&mut e, &ev), 175.0, VREL);
}

// ---------------------------------------------------------------- 3D hull

#[test]
fn hull_3d_planar_bodies() {
    // two cubes: one box
    let mut e = engine();
    let ev = e.evaluate(tree("hull() { cube(10); translate([20, 0, 0]) cube(10); }"));
    assert_clean(&ev);
    assert_approx("two cubes", root_volume(&mut e, &ev), 3000.0, VREL);
    assert_eq!(hints_of(&ev, "hull"), vec!["exakt: Polyeder (6 Flächen)".to_string()]);
    // all faces carry the tag of the hull node
    let h = node(&ev, "hull");
    let mesh = e.tessellate(&ev, h, None).unwrap();
    assert!(mesh.tri_tags.iter().all(|t| *t == ev.reports[h as usize].tag));
    // a cube and a rotated, scaled cube; a tetrahedron from a polyhedron and a point-like cube
    let v = vol(
        "hull() { cube(10, center = true); translate([0, 0, 10]) scale([1, 1, 0.001]) cube(0.001, center = true); }",
    );
    // cube + pyramid of height 5 on its top face
    assert_approx("cube + pyramid", v, 1000.0 + 100.0 * 5.0 / 3.0, 1e-4);
    // extrusions of polygons
    let v = vol("hull() { linear_extrude(2) square(10); translate([0, 0, 8]) linear_extrude(2) square(10); }");
    assert_approx("stacked plates", v, 1000.0, VREL);
}

#[test]
fn hull_3d_on_one_axis() {
    let r: f64 = 3.0;
    let l = 10.0;
    // capsule πr²L + 4/3 πr³: two spheres and a cylinder
    let src = format!("hull() {{ sphere({r}); translate([0, 0, {l}]) sphere({r}); }}");
    assert_approx("capsule", vol(&src), PI * r * r * l + 4.0 / 3.0 * PI * r.powi(3), VREL);
    // revolved section: surfaces of revolution of its arcs and line (exact, no B-splines)
    assert_eq!(surfaces(&src), map(&[("revolution", 3)]));
    // tilted axis
    let tilted = format!("rotate([30, 40, 0]) {src}");
    // (the volume integration of tilted surfaces of revolution is good to ~1e-6)
    assert_approx("capsule tilted", vol(&tilted), PI * r * r * l + 4.0 / 3.0 * PI * r.powi(3), 1e-5);
    // two spheres of different radii: cap + cone frustum + cap
    let (r1, r2, d): (f64, f64, f64) = (5.0, 2.0, 10.0);
    let s = (r1 - r2) / d;
    let c = (1.0 - s * s).sqrt();
    let (z1, z2) = (r1 * s, d + r2 * s);
    let (p1, p2) = (r1 * c, r2 * c);
    let cap = |r: f64, h: f64| PI * h * h * (3.0 * r - h) / 3.0;
    let want = 4.0 / 3.0 * PI * r1.powi(3) - cap(r1, r1 - z1)
        + PI * (z2 - z1) / 3.0 * (p1 * p1 + p1 * p2 + p2 * p2)
        + cap(r2, r2 - r2 * s);
    let src = format!("hull() {{ sphere({r1}); translate([0, 0, {d}]) sphere({r2}); }}");
    assert_approx("spheres", vol(&src), want, VREL);
    assert_eq!(surfaces(&src).keys().collect::<Vec<_>>(), ["revolution"]);
    // a cylinder with a sphere on its axis
    let v = vol("hull() { cylinder(r = 3, h = 10); translate([0, 0, 10]) sphere(3); }");
    assert_approx("cylinder + dome", v, PI * 9.0 * 10.0 + 2.0 / 3.0 * PI * 27.0, VREL);
    // a cone and a disc: the disc and a cone from its top edge to the tip
    let v = vol("hull() { cylinder(r1 = 4, r2 = 0, h = 6); cylinder(r = 4, h = 0.5); }");
    assert_approx("cone", v, PI * 16.0 * 0.5 + PI * 16.0 * 5.5 / 3.0, 1e-5);
}

#[test]
fn hull_3d_parallel_prisms() {
    // plate with round ends: three equal cylinders → extruded 2D hull
    let src = "hull() { cylinder(r = 3, h = 2); translate([20, 0, 0]) cylinder(r = 3, h = 2); translate([0, 15, 0]) cylinder(r = 3, h = 2); }";
    let want = 2.0 * (150.0 + 60.0 * 3.0 + 9.0 * PI);
    assert_approx("three cylinders", vol(src), want, VREL);
    let mut e = engine();
    let ev = e.evaluate(tree(src));
    assert_eq!(hints_of(&ev, "hull"), vec!["exakt: Prisma".to_string()]);
    // a cube and a cylinder of the same height
    let v = vol("hull() { cube([10, 10, 5]); translate([20, 5, 0]) cylinder(r = 5, h = 5); }");
    assert_approx("cube + cylinder", v, 5.0 * (100.0 + 10.0 * 10.0 + PI * 25.0 / 2.0), VREL);
    // stadium plate in another direction (cylinders along x)
    let v = vol(
        "hull() { rotate([0, 90, 0]) cylinder(r = 2, h = 4); translate([0, 10, 0]) rotate([0, 90, 0]) cylinder(r = 2, h = 4); }",
    );
    assert_approx("along x", v, 4.0 * (PI * 4.0 + 4.0 * 10.0), VREL);
}

#[test]
fn hull_3d_equal_spheres() {
    // eight spheres at the corners of a box: the rounded box
    let (a, b, c, r): (f64, f64, f64, f64) = (20.0, 10.0, 5.0, 2.0);
    let src = format!("hull() {{ for (x = [0, {a}], y = [0, {b}], z = [0, {c}]) translate([x, y, z]) sphere({r}); }}");
    let want = a * b * c + 2.0 * r * (a * b + b * c + c * a) + PI * r * r * (a + b + c) + 4.0 / 3.0 * PI * r.powi(3);
    assert_approx("rounded box", vol(&src), want, VREL);
    assert_eq!(surfaces(&src), map(&[("cylinder", 12), ("plane", 6), ("sphere", 8)]));
}

#[test]
fn hull_3d_unsupported_warns_with_reason() {
    let cases = [
        (
            "hull() { sphere(3); translate([10, 0, 0]) sphere(2); translate([0, 10, 0]) sphere(1); }",
            "verschiedenen Radien",
        ),
        ("hull() { cube(5); translate([10, 0, 0]) sphere(2); }", "ebene und runde Körper"),
        ("hull() { for (x = [0, 10], y = [0, 10]) translate([x, y, 0]) sphere(2); }", "in einer Ebene"),
        ("hull() { cylinder(r = 2, h = 5); translate([10, 0, 0]) cylinder(r = 2, h = 3); }", "weder auf einer Achse"),
        ("hull() { torus(10, 2); sphere(1); }", "Torus"),
        ("hull() { scale([1, 2, 1]) sphere(2); sphere(1); }", "verzerrte Kugel"),
    ];
    for (src, why) in cases {
        let mut e = engine();
        let ev = e.evaluate(tree(src));
        let w = warnings(&ev);
        assert!(
            w.iter().any(|m| m.starts_with("hull: ") && m.contains(why) && m.contains("unverändert")),
            "{src}: {w:?}"
        );
        assert!(errors(&ev).is_empty(), "{src}: {:?}", errors(&ev));
    }
}

// ---------------------------------------------------------------- 3D minkowski

#[test]
fn minkowski_3d() {
    // rounded cube: box + ball
    let (a, b, c, r): (f64, f64, f64, f64) = (10.0, 20.0, 30.0, 2.0);
    let want = a * b * c + 2.0 * r * (a * b + b * c + c * a) + PI * r * r * (a + b + c) + 4.0 / 3.0 * PI * r.powi(3);
    assert_approx("rounded cube", vol(&format!("minkowski() {{ cube([{a}, {b}, {c}]); sphere({r}); }}")), want, VREL);
    // order does not matter, a moved sphere moves the result
    let mut e = engine();
    let ev = e.evaluate(tree(&format!("minkowski() {{ translate([1, 2, 3]) sphere({r}); cube([{a}, {b}, {c}]); }}")));
    assert_clean(&ev);
    assert_approx("moved", root_volume(&mut e, &ev), want, VREL);
    let m = node(&ev, "minkowski");
    let (lo, hi) = ev.reports[m as usize].bounds.unwrap();
    assert!(
        lo.abs_diff_eq(v(1.0 - r, 2.0 - r, 3.0 - r), 1e-6)
            && hi.abs_diff_eq(v(1.0 + a + r, 2.0 + b + r, 3.0 + c + r), 1e-6)
    );
    let mesh = e.tessellate(&ev, m, None).unwrap();
    assert!(mesh.tri_tags.iter().all(|t| *t == ev.reports[m as usize].tag));
    // sphere + sphere
    assert_approx("spheres", vol("minkowski() { sphere(2); sphere(3); }"), 4.0 / 3.0 * PI * 125.0, VREL);
}

#[test]
fn minkowski_3d_extrusion_and_cylinder() {
    // linear_extrude(5) of a rectangle + cylinder(r 2, h 1): offset section over z 0..6
    let src = "minkowski() { linear_extrude(5) square([10, 20]); cylinder(r = 2, h = 1); }";
    let mut e = engine();
    let ev = e.evaluate(tree(src));
    assert_clean(&ev);
    assert_approx("volume", root_volume(&mut e, &ev), 6.0 * (200.0 + 60.0 * 2.0 + 4.0 * PI), VREL);
    let (lo, hi) = ev.reports[node(&ev, "minkowski") as usize].bounds.unwrap();
    assert!(lo.abs_diff_eq(v(-2.0, -2.0, 0.0), 1e-6) && hi.abs_diff_eq(v(12.0, 22.0, 6.0), 1e-6), "{lo} {hi}");
    // z-range: [z0X + z0C, z1X + z1C], centred extrusion and a moved cylinder
    let src =
        "minkowski() { linear_extrude(4, center = true) circle(3); translate([1, 0, 2]) cylinder(r = 1, h = 3); }";
    let ev = e.evaluate(tree(src));
    assert_clean(&ev);
    assert_approx("circle + cylinder", root_volume(&mut e, &ev), 7.0 * PI * 16.0, VREL);
    let (lo, hi) = ev.reports[node(&ev, "minkowski") as usize].bounds.unwrap();
    assert!((lo.z - 0.0).abs() < 1e-6 && (hi.z - 7.0).abs() < 1e-6 && (lo.x + 3.0).abs() < 1e-6, "{lo} {hi}");
    // two cubes: hull of the vertex sums
    assert_approx("cubes", vol("minkowski() { cube(10); cube([1, 2, 3]); }"), 11.0 * 12.0 * 13.0, VREL);
}

#[test]
fn minkowski_3d_unsupported_warns_with_reason() {
    let cases = [
        ("minkowski() { cube(5); cube(3); rotate([0, 0, 45]) cube(2); }", "exakt nur"),
        (
            "minkowski() { polyhedron([[0, 0, 0], [10, 0, 0], [0, 10, 0], [0, 0, 10]], [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]]); sphere(1); }",
            "not exact",
        ),
    ];
    for (src, why) in cases {
        let mut e = engine();
        let ev = e.evaluate(tree(src));
        let w = warnings(&ev);
        assert!(w.iter().any(|m| m.starts_with("minkowski: ") && m.contains(why)), "{src}: {w:?}");
        assert!(errors(&ev).is_empty(), "{src}: {:?}", errors(&ev));
    }
}
