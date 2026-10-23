//! Sketches (Plan 17.1) through the engine with native SMLib: exact volumes of extrusions,
//! revolutions, sweeps and lofts of mixed contours, holes by winding rule and `hole { … }`,
//! STEP with exact lines and circles.

mod common;

use std::f64::consts::PI;

use cad_eval::{EvalOptions, eval_source};
use cad_ir::{Op, Profile, Tree};
use common::*;

fn tree(src: &str) -> Tree {
    let out = eval_source(src, &EvalOptions::default());
    let errs: Vec<_> = out.errors().collect();
    assert!(errs.is_empty(), "{errs:#?}");
    out.tree
}

fn vol(src: &str) -> f64 {
    let mut e = engine();
    let ev = e.evaluate(tree(src));
    assert_clean(&ev);
    root_volume(&mut e, &ev)
}

#[test]
fn rectangle_with_half_circle() {
    let (a, b, h) = (40.0, 20.0, 5.0);
    let src = format!(
        "linear_extrude({h}) sketch {{\n start([0, 0]);\n line(to = [{a}, 0]);\n line(to = [{a}, {b}]);\n arc(to = [0, {b}], r = {});\n close();\n}}",
        a / 2.0
    );
    let want = (a * b + PI * (a / 2.0) * (a / 2.0) / 2.0) * h;
    assert_approx("rect + half circle", vol(&src), want, 1e-7);
    // the same with a tangent arc, a 3-point arc and centre + angle
    for arc in [
        "arc(to = [0, 20]);".to_string(),
        "arc(through = [20, 40], to = [0, 20]);".into(),
        "arc(center = [20, 20], angle = 180);".into(),
    ] {
        let src = format!(
            "linear_extrude(5) sketch {{ start([0, 0]); line(to = [40, 0]); line(by = [0, 20]); {arc} close(); }}"
        );
        assert_approx(&arc, vol(&src), want, 1e-7);
    }
}

#[test]
fn revolve_arc_profiles_sphere_and_torus() {
    let r = 7.0;
    let sphere = format!("rotate_extrude() sketch {{ start([0, -{r}]); arc(to = [0, {r}], r = {r}); close(); }}");
    assert_approx("sphere", vol(&sphere), 4.0 / 3.0 * PI * r * r * r, 1e-7);
    let (big, small) = (20.0, 4.0);
    let torus = format!(
        "rotate_extrude() sketch {{ start([{}, 0]); arc(to = [{}, 0], r = {small}); arc(to = [{}, 0], r = {small}); }}",
        big + small,
        big - small,
        big + small
    );
    assert_approx("torus", vol(&torus), 2.0 * PI * PI * big * small * small, 1e-7);
    // half turn of a quarter disc: a quarter of a sphere ... of radius r about the axis
    let quarter = format!(
        "rotate_extrude(angle = 180) sketch {{ start([0, 0]); line(to = [{r}, 0]); arc(center = [0, 0], angle = 90); close(); }}"
    );
    assert_approx("half hemisphere", vol(&quarter), 4.0 / 3.0 * PI * r * r * r / 4.0, 1e-7);
}

#[test]
fn holes_by_winding_and_hole_block() {
    // outer square counter-clockwise, inner circle clockwise: a hole (nonzero)
    let ring = "linear_extrude(2) sketch {\n start([0, 0]); line(to = [20, 0]); line(to = [20, 20]); line(to = [0, 20]); close();\n start([15, 10]); arc(to = [5, 10], r = 5, cw = true); arc(to = [15, 10], r = 5, cw = true);\n}";
    let want = (400.0 - PI * 25.0) * 2.0;
    assert_approx("cw hole", vol(ring), want, 1e-7);
    // the same circle counter-clockwise: same direction as the outer loop, filled (nonzero)
    let filled = ring.replace(", cw = true", "");
    assert_approx("ccw inner", vol(&filled), 800.0, 1e-7);
    // `hole { … }` is a hole in any direction
    let block = "linear_extrude(2) sketch {\n start([0, 0]); line(to = [20, 0]); line(to = [20, 20]); line(to = [0, 20]); close();\n hole { start([15, 10]); arc(to = [5, 10], r = 5); arc(to = [15, 10], r = 5); }\n}";
    assert_approx("hole block", vol(block), want, 1e-7);
    // island in the hole, and a second separate contour
    let island = format!(
        "{}\n",
        block.replace(
            "\n}",
            "\n start([11, 10]); arc(to = [9, 10], r = 1); arc(to = [11, 10], r = 1);\n start([30, 0]); line(by = [5, 0]); line(by = [0, 5]); line(by = [-5, 0]); close();\n}"
        )
    );
    assert_approx("island + second contour", vol(&island), want + PI * 2.0 + 50.0, 1e-7);
}

#[test]
fn bezier_and_spline_segments() {
    // square with a quadratic Bézier cap (area of the parabola segment: 2/3 · base · height)
    let src = "linear_extrude(1) sketch { start([0, 0]); line(to = [4, 0]); line(to = [4, 4]); bezier(c1 = [2, 6], to = [0, 4]); close(); }";
    assert_approx("quadratic bezier", vol(src), 16.0 + 8.0 / 3.0, 1e-7);
    // cubic Bézier: cap area from Green's theorem on the exact curve
    // x = 4 - 12t² + 8t³, y = 4 + 12t - 12t²: ∮ x dy over the cap = ∫ x y' dt (chord adds 0)
    let cap = {
        let n = 200_000;
        (0..n)
            .map(|i| {
                let t = (i as f64 + 0.5) / n as f64;
                let x = 4.0 - 12.0 * t * t + 8.0 * t * t * t;
                let dy = 12.0 - 24.0 * t;
                x * dy / n as f64
            })
            .sum::<f64>()
            .abs()
    };
    let cubic = "linear_extrude(1) sketch { start([0, 0]); line(to = [4, 0]); line(to = [4, 4]); bezier(c1 = [4, 8], c2 = [0, 8], to = [0, 4]); close(); }";
    assert_approx("cubic bezier", vol(cubic), 16.0 + cap, 1e-7);
    // spline: a closed body of positive volume through the points
    let spline = "linear_extrude(1) sketch { start([0, 0]); line(to = [10, 0]); spline([[8, 5], [5, 7], [2, 5]], tangent = true); close(); }";
    let v = vol(spline);
    assert!(v > 40.0 && v < 70.0, "{v}");
}

#[test]
fn sketch_modules_loops_and_twist() {
    // a module whose body is a sketch, teeth by a loop
    let src = "module stern(n = 6, r1 = 10, r2 = 5) sketch {\n start([r1, 0]);\n for (i = [1 : n]) {\n  line(to = [r2 * cos(360 * (i - 0.5) / n), r2 * sin(360 * (i - 0.5) / n)]);\n  line(to = [r1 * cos(360 * i / n), r1 * sin(360 * i / n)]);\n }\n}\nlinear_extrude(3) stern();";
    // star: 2n triangles centre–r1–r2 with angle π/n between them
    let area = 12.0 * 0.5 * 10.0 * 5.0 * (PI / 6.0).sin();
    assert_approx("star", vol(src), area * 3.0, 1e-7);
    // twist keeps the volume of a sketch with arcs
    let tw = "linear_extrude(10, twist = 90) sketch { start([-5, -3]); line(to = [5, -3]); arc(to = [5, 3], r = 3); line(to = [-5, 3]); arc(to = [-5, -3], r = 3); }";
    assert_approx("twist", vol(tw), (60.0 + PI * 9.0) * 10.0, 1e-3);
}

#[test]
fn sweep_along_open_sketch_path() {
    // smooth path: line, tangent quarter arc (radius 10), line; tube of radius 2
    let src = "sweep() {\n sketch(closed = false) { start([0, 0]); line(to = [20, 0]); arc(to = [30, 10]); line(by = [0, 15]); }\n circle(2);\n}";
    let len = 20.0 + PI * 10.0 / 2.0 + 15.0;
    assert_approx("smooth path", vol(src), PI * 4.0 * len, 1e-4);
    // path with a corner: mitered joint
    let corner = "sweep() {\n sketch(closed = false) { start([0, 0]); line(to = [20, 0]); line(to = [20, 15]); }\n square(2, center = true);\n}";
    assert_approx("mitered", vol(corner), 4.0 * 35.0, 1e-6);
    // the path sketch must be open
    let out = eval_source(
        "sweep() { sketch { start([0,0]); line(to=[5,0]); line(to=[0,5]); close(); } circle(1); }",
        &EvalOptions::default(),
    );
    let mut e = engine();
    let ev = e.evaluate(out.tree);
    assert!(errors(&ev).iter().any(|m| m.contains("offenen Skizzenpfad")), "{:?}", errors(&ev));
}

#[test]
fn loft_between_sketches() {
    // equal segment counts: matched segment by segment (frustum of a rounded square)
    let sec = |s: f64| {
        format!(
            "sketch {{ start([-{s}, -{s}]); line(to = [{s}, -{s}]); line(to = [{s}, {s}]); line(to = [-{s}, {s}]); close(); }}"
        )
    };
    let src = format!("loft(ruled = true) {{ {} translate([0, 0, 10]) {} }}", sec(5.0), sec(2.5));
    // frustum of a square pyramid: h/3 (A1 + A2 + sqrt(A1 A2))
    let (a1, a2): (f64, f64) = (100.0, 25.0);
    assert_approx("loft equal counts", vol(&src), 10.0 / 3.0 * (a1 + a2 + (a1 * a2).sqrt()), 1e-6);
    // different counts: each section becomes one closed curve
    let circ = "sketch { start([3, 0]); arc(to = [-3, 0], r = 3); arc(to = [3, 0], r = 3); }";
    let src = format!("loft() {{ {} translate([0, 0, 10]) {circ} }}", sec(4.0));
    let v = vol(&src);
    assert!(v > PI * 9.0 * 10.0 && v < 64.0 * 10.0, "{v}");
}

#[test]
fn step_has_exact_lines_and_circles() {
    let src = "linear_extrude(5) sketch { start([0, 0]); line(to = [40, 0]); line(to = [40, 20]); arc(to = [0, 20], r = 20); close(); }";
    let mut e = engine();
    let ev = e.evaluate(tree(src));
    assert_clean(&ev);
    let opts = cad_engine::StepOptions { name: "skizze".into(), ..Default::default() };
    let (bytes, st) = e.export_step(&ev, &opts).unwrap();
    assert_eq!(st.solids, 1);
    assert_eq!(st.faces, 6, "{st:?}");
    let text = std::str::from_utf8(&bytes).unwrap();
    let m = cad_step::p21::parse(text).unwrap();
    assert!(m.count("CIRCLE") >= 2, "{st:?}");
    assert!(m.count("LINE") >= 6, "{st:?}");
    assert_eq!(st.approximated_edges, 0, "{st:?}");
    // the curved side: SMLib's extrusion of the exact arc, written as a cylinder
    assert_eq!(st.surfaces.get("cylinder"), Some(&1), "{st:?}");
    assert_eq!(m.count("CYLINDRICAL_SURFACE"), 1);
    // revolved arcs: a torus from two arcs
    let src = "rotate_extrude() sketch { start([24, 0]); arc(to = [16, 0], r = 4); arc(to = [24, 0], r = 4); }";
    let ev = e.evaluate(tree(src));
    assert_clean(&ev);
    let (_, st) = e.export_step(&ev, &opts).unwrap();
    assert_eq!(st.curves.get("circle"), Some(&4), "{st:?}");
    assert_eq!((st.approximated_edges, st.approximated_faces), (0, 0), "{st:?}");
    let c = m.of_type("CIRCLE").next().unwrap();
    assert!((c.args()[2].as_num().unwrap() - 20.0).abs() < 1e-9);
    // the sketch is one node with one profile
    let n = ev.tree.nodes.iter().filter(|n| matches!(n.op, Op::Profile(Profile::Sketch { .. }))).count();
    assert_eq!(n, 1);
}

#[test]
fn sketch_alone_is_2d_and_open_sketch_is_a_path() {
    let mut e = engine();
    let ev = e.evaluate(tree("sketch { start([0, 0]); line(to = [10, 0]); arc(to = [0, 0], r = 5); }"));
    assert_clean(&ev);
    let root = ev.root().unwrap();
    let m = e.tessellate(&ev, root, Some(0.01)).unwrap();
    // area of the filled 2D mesh: half disc
    let area: f64 = mesh_area(&m);
    assert_approx("2D area", area, PI * 25.0 / 2.0, 5e-3);
    let ev =
        e.evaluate(tree("sketch(closed = false) { start([0, 0]); line(to = [10, 0]); arc(to = [20, 0], r = 5); }"));
    assert_clean(&ev);
    let sk = ev.tree.nodes.iter().position(|n| matches!(n.op, Op::Profile(Profile::Sketch { .. }))).unwrap() as u32;
    let m = e.tessellate(&ev, sk, Some(0.01)).unwrap();
    assert!(m.indices.is_empty() && !m.edge_segments.is_empty());
}

fn mesh_area(m: &cad_kernel::Mesh) -> f64 {
    let p = |i: u32| {
        let i = i as usize * 3;
        glam::DVec3::new(m.positions[i] as f64, m.positions[i + 1] as f64, m.positions[i + 2] as f64)
    };
    m.indices.chunks_exact(3).map(|t| (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0])).length() / 2.0).sum()
}
