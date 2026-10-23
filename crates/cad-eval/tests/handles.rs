//! Origins, handles and writing back (drag → edit → re-parse → expected value), snap
//! targets, "Als Standard übernehmen".

mod common;
use cad_eval::*;
use common::*;
use glam::DVec3;

/// Handle of the statement whose text starts with `stmt`, of `kind`, with `index`.
fn handle<'o>(out: &'o EvalOutput, src: &str, stmt: &str, kind: HandleKind, index: u32) -> &'o Handle {
    out.handles
        .iter()
        .find(|h| span_text(src, h.span).trim_start().starts_with(stmt) && h.kind == kind && h.index == index)
        .unwrap_or_else(|| {
            let all: Vec<_> = out.handles.iter().map(|h| (span_text(src, h.span), h.kind, h.index)).collect();
            panic!("no handle {kind:?}/{index} on `{stmt}`; have {all:#?}")
        })
}

fn no_handle(out: &EvalOutput, src: &str, stmt: &str) -> bool {
    !out.handles.iter().any(|h| span_text(src, h.span).trim_start().starts_with(stmt))
}

fn drag(out: &EvalOutput, src: &str, h: &Handle, t: DragTarget) -> String {
    let edits = drag_edits(out, src, h.id, &t, &DragOptions::default());
    apply_edits(src, &edits)
}

fn free(p: [f64; 3]) -> DragTarget {
    DragTarget::Free(DragValue::Point(DVec3::from_array(p)))
}

fn scalar(v: f64) -> DragTarget {
    DragTarget::Free(DragValue::Scalar(v))
}

fn comp_texts<'s>(src: &'s str, h: &Handle) -> Vec<Option<&'s str>> {
    h.comps.iter().map(|c| c.map(|c| span_text(src, c.span))).collect()
}

#[test]
fn origins_of_values() {
    let src = "a = -5; b = a; c = a + 1; v = [1, b, c]; w = v; x = w[1]; grid g { x: p = 2; } q = g[p, _, _]; n = g.p;";
    let out = clean(src);
    // Through hints we see the literal behind each value.
    let lit = |name: &str| {
        out.hints
            .iter()
            .find(|h| span_text(src, h.span).starts_with(&format!("{name} =")))
            .and_then(|h| h.literal)
            .map(|s| span_text(src, s))
    };
    assert_eq!(lit("a"), Some("-5"));
    assert_eq!(lit("b"), Some("-5"));
    assert_eq!(lit("c"), None);
    assert_eq!(lit("x"), Some("-5"));
    let w = handle(&out, src, "w =", HandleKind::Point, 0);
    assert_eq!(comp_texts(src, w), [Some("1"), Some("-5"), None]);
    assert!(w.via_var);
    assert_eq!(span_text(src, w.def_span.unwrap()), "[1, b, c]");
    let q = handle(&out, src, "q =", HandleKind::Point, 0);
    assert!(q.has_ref);
    assert_eq!(comp_texts(src, q), [None, None, None]);
    assert_eq!(lit("n"), None);
}

#[test]
fn cube_corner_handle() {
    let src = "cube([10, 20, 30]);\ncube(5);\ncube([4, 6, 8], center = true);";
    let out = clean(src);
    let h = handle(&out, src, "cube([10", HandleKind::Point, 0);
    assert!(approx3(h.pos, [10.0, 20.0, 30.0]));
    assert_eq!(comp_texts(src, h), [Some("10"), Some("20"), Some("30")]);
    assert!(!h.via_var && !h.via_default);
    let new = drag(&out, src, h, free([12.4, 20.0, 29.6]));
    assert!(new.starts_with("cube([12, 20, 30]);"), "{new}");
    // Scalar size: all components share one literal (mean).
    let h = handle(&out, src, "cube(5)", HandleKind::Point, 0);
    assert_eq!(drag(&out, src, h, free([7.0, 8.0, 9.0])).lines().nth(1), Some("cube(8);"));
    // Centered: the corner is at size / 2, literal = 2 * value.
    let h = handle(&out, src, "cube([4", HandleKind::Point, 0);
    assert!(approx3(h.pos, [2.0, 3.0, 4.0]));
    assert_eq!(h.comps[0].unwrap().scale, 2.0);
    let new = drag(&out, src, h, free([3.0, 3.0, 4.0]));
    assert!(new.ends_with("cube([6, 6, 8], center = true);"), "{new}");
}

#[test]
fn drag_round_trip_reevaluates() {
    let src = "translate([20, 0, 10]) cube(1);";
    let out = clean(src);
    let h = handle(&out, src, "translate", HandleKind::Point, 0);
    let new = drag(&out, src, h, free([23.4, -1.6, 10.2]));
    assert_eq!(new, "translate([23, -2, 10]) cube(1);");
    let out2 = clean(&new);
    let h2 = handle(&out2, &new, "translate", HandleKind::Point, 0);
    assert!(approx3(h2.pos, [23.0, -2.0, 10.0]));
    // Same handle id after a literal change.
    assert_eq!(h.id, h2.id);
    // Custom step.
    let opts = DragOptions { step_mm: 0.5, ..Default::default() };
    let e = drag_edits(&out, src, h.id, &free([23.3, 0.0, 10.0]), &opts);
    assert_eq!(apply_edits(src, &e), "translate([23.5, 0, 10]) cube(1);");
    let opts = DragOptions { step_mm: 0.0, ..Default::default() };
    let e = drag_edits(&out, src, h.id, &free([1.0 / 3.0, 0.0, 10.0]), &opts);
    assert_eq!(apply_edits(src, &e), "translate([0.333333, 0, 10]) cube(1);");
}

#[test]
fn negative_literals_and_partial_handles() {
    let src = "w = 10; translate([w - 2, -5, 0]) cube(1);";
    let out = clean(src);
    let h = handle(&out, src, "translate", HandleKind::Point, 0);
    assert_eq!(comp_texts(src, h), [None, Some("-5"), Some("0")]);
    let new = drag(&out, src, h, free([100.0, 3.0, 0.0]));
    assert_eq!(new, "w = 10; translate([w - 2, 3, 0]) cube(1);");
    // Fully computed values have no handle.
    let src = "p = [1, 2, 3]; translate(p + [1, 0, 0]) cube(1);";
    let out = clean(src);
    assert!(no_handle(&out, src, "translate"));
}

#[test]
fn variable_definition_is_edited() {
    let src = "p = [20, 0, 10];\ntranslate(p) cube(1);\ntranslate(p) sphere(1);";
    let out = clean(src);
    let h = handle(&out, src, "translate(p) cube", HandleKind::Point, 0);
    assert!(h.via_var);
    assert_eq!(span_text(src, h.expr_span), "p");
    assert_eq!(span_text(src, h.def_span.unwrap()), "[20, 0, 10]");
    let new = drag(&out, src, h, free([25.0, 0.0, 10.0]));
    assert_eq!(new, "p = [25, 0, 10];\ntranslate(p) cube(1);\ntranslate(p) sphere(1);");
    // The variable itself has a point handle.
    let hp = handle(&out, src, "p =", HandleKind::Point, 0);
    assert_eq!(
        drag(&out, src, hp, free([1.0, 2.0, 3.0])),
        "p = [1, 2, 3];\ntranslate(p) cube(1);\ntranslate(p) sphere(1);"
    );
}

#[test]
fn module_defaults_are_marked() {
    let src = "module m(s = 10, at = [1, 2, 3]) translate(at) cube(s);\nm();\nm(at = [5, 5, 5]);";
    // In file context, statements inside module bodies are not editable here.
    let out = clean(src);
    assert!(no_handle(&out, src, "translate"));
    // In module context the defaults are the literals.
    let out = run_module(src, "m", vec![]);
    let t = handle(&out, src, "translate", HandleKind::Point, 0);
    assert!(t.via_default && t.via_var);
    assert_eq!(comp_texts(src, t), [Some("1"), Some("2"), Some("3")]);
    let c = handle(&out, src, "cube", HandleKind::Point, 0);
    assert!(c.via_default);
    let new = drag(&out, src, c, free([12.0, 12.0, 12.0]));
    assert!(new.starts_with("module m(s = 12, at"), "{new}");
    // Overridden parameters (parameter bar) are not literals.
    let out = run_module(src, "m", vec![("s", PValue::Num(4.0))]);
    assert!(no_handle(&out, src, "cube"));
}

#[test]
fn radius_and_height_handles() {
    let src = "sphere(r = 5);\nsphere(d = 10);\ncylinder(h = 20, r = 3);\ncylinder(h = 10, d1 = 4, r2 = 1, center = true);\ncircle(2);\ntorus(10, 2);\nlinear_extrude(height = 8) square(3);";
    let out = clean(src);
    let r = handle(&out, src, "sphere(r", HandleKind::Radius, 0);
    assert!(approx3(r.pos, [5.0, 0.0, 0.0]));
    assert_eq!(drag(&out, src, r, scalar(6.6)).lines().next(), Some("sphere(r = 7);"));
    let d = handle(&out, src, "sphere(d", HandleKind::Radius, 0);
    assert_eq!(d.value, [5.0]);
    assert_eq!(drag(&out, src, d, scalar(7.4)).lines().nth(1), Some("sphere(d = 15);"));
    // A point drag is projected onto the handle axis.
    assert_eq!(drag(&out, src, d, free([6.0, 3.0, 9.0])).lines().nth(1), Some("sphere(d = 12);"));
    let h = handle(&out, src, "cylinder(h = 20", HandleKind::Height, 0);
    assert!(approx3(h.pos, [0.0, 0.0, 20.0]));
    assert_eq!(drag(&out, src, h, scalar(25.0)).lines().nth(2), Some("cylinder(h = 25, r = 3);"));
    let hr = handle(&out, src, "cylinder(h = 20", HandleKind::Radius, 0);
    assert_eq!(hr.value, [3.0]);
    // Centered cone: height handle at h/2 (scale 2), bottom radius from d1, top from r2.
    let hc = handle(&out, src, "cylinder(h = 10", HandleKind::Height, 0);
    assert!(approx3(hc.pos, [0.0, 0.0, 5.0]));
    assert_eq!(
        drag(&out, src, hc, scalar(6.0)).lines().nth(3),
        Some("cylinder(h = 12, d1 = 4, r2 = 1, center = true);")
    );
    let r1 = handle(&out, src, "cylinder(h = 10", HandleKind::Radius, 0);
    assert!(approx3(r1.pos, [2.0, 0.0, -5.0]));
    let r2 = handle(&out, src, "cylinder(h = 10", HandleKind::Radius, 1);
    assert!(approx3(r2.pos, [1.0, 0.0, 5.0]));
    handle(&out, src, "circle", HandleKind::Radius, 0);
    let tr = handle(&out, src, "torus", HandleKind::Radius, 1);
    assert!(approx3(tr.pos, [10.0, 0.0, 2.0]));
    let le = handle(&out, src, "linear_extrude", HandleKind::Height, 0);
    assert_eq!(drag(&out, src, le, scalar(3.2)).lines().last(), Some("linear_extrude(height = 3) square(3);"));
    handle(&out, src, "square", HandleKind::Point, 0);
}

#[test]
fn rotate_and_scale_handles() {
    let src = "rotate([0, 30, 45]) cube(1);\nrotate(10) cube(1);\nscale(2) cube(1);\nscale([1, 2, 3]) cube(1);";
    let out = run(src);
    let z = handle(&out, src, "rotate([", HandleKind::Angle { axis: 2 }, 2);
    assert_eq!(z.value, [45.0]);
    assert_eq!(z.axis, Some(DVec3::Z));
    // 15° steps.
    assert_eq!(drag(&out, src, z, scalar(52.0)).lines().next(), Some("rotate([0, 30, 45]) cube(1);"));
    assert_eq!(drag(&out, src, z, scalar(55.0)).lines().next(), Some("rotate([0, 30, 60]) cube(1);"));
    // The y ring lives in the system rotated by z.
    let y = handle(&out, src, "rotate([", HandleKind::Angle { axis: 1 }, 1);
    assert_eq!(y.frame, cad_eval::math::rot_z(45.0));
    let a = handle(&out, src, "rotate(10)", HandleKind::Angle { axis: 2 }, 0);
    assert_eq!(drag(&out, src, a, scalar(-40.0)).lines().nth(1), Some("rotate(-45) cube(1);"));
    let s = handle(&out, src, "scale(2)", HandleKind::Scale { axis: None }, 0);
    assert_eq!(drag(&out, src, s, scalar(2.46)).lines().nth(2), Some("scale(2.5) cube(1);"));
    let sy = handle(&out, src, "scale([", HandleKind::Scale { axis: Some(1) }, 1);
    assert_eq!(sy.value, [2.0]);
}

#[test]
fn polygon_and_path_point_handles() {
    let src = "pts = [[0, 0], [10, 0], [0, 10]];\npolygon(pts);\npolygon([[0, 0], [5, 0], [0, 5]]);\nsweep([[0, 0, 0], [0, 0, 20]]) circle(1);";
    let out = clean(src);
    let p1 = handle(&out, src, "polygon([[", HandleKind::Point, 1);
    assert_eq!(comp_texts(src, p1), [Some("5"), Some("0")]);
    assert_eq!(span_text(src, p1.expr_span), "[5, 0]");
    let new = drag(&out, src, p1, free([6.0, 1.0, 0.0]));
    assert!(new.contains("polygon([[0, 0], [6, 1], [0, 5]]);"), "{new}");
    let v = handle(&out, src, "polygon(pts)", HandleKind::Point, 2);
    assert!(v.via_var);
    let new = drag(&out, src, v, free([0.0, 12.0, 0.0]));
    assert!(new.starts_with("pts = [[0, 0], [10, 0], [0, 12]];"), "{new}");
    let s = handle(&out, src, "sweep", HandleKind::Point, 1);
    assert!(approx3(s.pos, [0.0, 0.0, 20.0]));
}

#[test]
fn box_and_cylinder_point_handles() {
    let src = "box([0, 0, 0], [10, 20, 5]);\ncylinder([0, 0, 0], [0, 0, 10], r = 2);";
    let out = clean(src);
    let b = handle(&out, src, "box", HandleKind::Point, 1);
    assert_eq!(drag(&out, src, b, free([11.0, 20.0, 5.0])).lines().next(), Some("box([0, 0, 0], [11, 20, 5]);"));
    let c = handle(&out, src, "cylinder", HandleKind::Point, 1);
    assert!(approx3(c.pos, [0.0, 0.0, 10.0]));
    let r = handle(&out, src, "cylinder", HandleKind::Radius, 0);
    assert!(approx(r.pos.length(), 2.0));
}

#[test]
fn handle_frames_follow_transforms() {
    let src = "translate([0, 0, 100]) rotate([0, 0, 90]) cube([1, 2, 3]);";
    let out = clean(src);
    let h = handle(&out, src, "cube", HandleKind::Point, 0);
    assert!(approx3(h.pos, [1.0, 2.0, 3.0]));
    assert!(approx3(h.frame.transform_point3(h.pos), [-2.0, 1.0, 103.0]));
}

const GRID_SRC: &str = "grid g {
    x: links = 0, rechts = 40;
    y: vorne = 0, hinten = 30;
    z: boden = 0, deckel = 20;
}
translate([5, 5, 0]) cube(2);
p = [1, 2, 3];
translate(p) sphere(1);
translate(g[rechts, hinten, boden]) cube(1);
";

#[test]
fn grid_plane_handles() {
    let src = GRID_SRC;
    let out = clean(src);
    let g = out.grid_by_name("g").unwrap();
    let ri = g.planes.iter().position(|p| p.name == "rechts").unwrap() as u32;
    let h = handle(&out, src, "grid g", HandleKind::Plane { axis: 0 }, ri);
    assert!(approx3(h.pos, [40.0, 0.0, 0.0]));
    let new = drag(&out, src, h, scalar(47.6));
    assert!(new.contains("rechts = 48;"), "{new}");
    // Re-evaluate: the grid point moved.
    let out2 = clean(&new);
    let t = handle(&out2, &new, "translate(g[", HandleKind::Point, 0);
    assert!(approx3(t.pos, [48.0, 30.0, 0.0]));
    // Expression planes have no handle.
    let src2 = "w = 4; grid g { x: a = w * 2, b = w; }";
    let out = clean(src2);
    let g = out.grid_by_name("g").unwrap();
    assert_eq!(out.handles.len(), 1);
    let b = g.planes.iter().position(|p| p.name == "b").unwrap() as u32;
    let hb = handle(&out, src2, "grid g", HandleKind::Plane { axis: 0 }, b);
    assert!(hb.via_var);
}

#[test]
fn snap_writes_grid_reference() {
    let src = GRID_SRC;
    let out = clean(src);
    let h = handle(&out, src, "translate([5", HandleKind::Point, 0);
    assert_eq!(h.grids, [0]);
    let r = SnapRef::Grid { grid: 0, planes: ["rechts".into(), "vorne".into(), "deckel".into()] };
    let new = drag(&out, src, h, DragTarget::Snap(r));
    assert!(new.contains("translate(g[rechts, vorne, deckel]) cube(2);"), "{new}");
    let out2 = clean(&new);
    let h2 = handle(&out2, &new, "translate(g[rechts, vorne", HandleKind::Point, 0);
    assert!(approx3(h2.pos, [40.0, 0.0, 20.0]));
    assert!(h2.has_ref);
    // Free drag of a pure grid reference writes numbers.
    let freed = drag(&out2, &new, h2, free([39.6, 1.2, 20.0]));
    assert!(freed.contains("translate([40, 1, 20]) cube(2);"), "{freed}");
    // Invalid plane names are refused.
    let bad = SnapRef::Grid { grid: 0, planes: ["vorne".into(), "vorne".into(), "deckel".into()] };
    assert!(drag_edits(&out, src, h.id, &DragTarget::Snap(bad), &DragOptions::default()).is_empty());
}

#[test]
fn snap_through_variable_replaces_definition() {
    let src = GRID_SRC;
    let out = clean(src);
    let h = handle(&out, src, "translate(p)", HandleKind::Point, 0);
    let r = SnapRef::Grid { grid: 0, planes: ["links".into(), "hinten".into(), "deckel".into()] };
    let new = drag(&out, src, h, DragTarget::Snap(r.clone()));
    assert!(new.contains("p = g[links, hinten, deckel];"), "{new}");
    clean(&new);
    // A definition before the grid cannot reference it: the use site is rewritten.
    let src2 = "p = [1, 2, 3];\ngrid g { x: a = 5; }\ntranslate(p) cube(1);";
    let out = clean(src2);
    let h = handle(&out, src2, "translate(p)", HandleKind::Point, 0);
    let r = SnapRef::Grid { grid: 0, planes: ["a".into(), "_".into(), "_".into()] };
    let new = drag(&out, src2, h, DragTarget::Snap(r));
    assert_eq!(new, "p = [1, 2, 3];\ngrid g { x: a = 5; }\ntranslate(g[a, _, _]) cube(1);");
    clean(&new);
}

#[test]
fn snap_to_anchor() {
    let src = "module part() { anchor ecke = [10, 0, 5]; cube(1); }\nk: part();\ntranslate([0, 0, 0]) sphere(1);";
    let out = clean(src);
    let h = handle(&out, src, "translate", HandleKind::Point, 0);
    assert_eq!(h.instances, [1]);
    let targets = snap_targets(&out, h.id);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].text, "k.ecke");
    assert!(approx3(targets[0].world, [10.0, 0.0, 5.0]));
    let new = drag(&out, src, h, DragTarget::Snap(targets[0].r#ref.clone()));
    assert!(new.ends_with("translate(k.ecke) sphere(1);"), "{new}");
    let out2 = clean(&new);
    let h2 = handle(&out2, &new, "translate", HandleKind::Point, 0);
    assert!(h2.has_ref);
    assert!(approx3(h2.pos, [10.0, 0.0, 5.0]));
}

#[test]
fn snap_targets_list_grid_points() {
    let src = "translate([0, 0, 50]) grid g { x: a = 0, b = 10; y: c = 1; }\ntranslate([7, 0, 0]) cube(1);\ntranslate([0, 0, 1]) { translate([1, 1, 1]) cube(1); }";
    let out = clean(src);
    let h = handle(&out, src, "translate([7", HandleKind::Point, 0);
    let t = snap_targets(&out, h.id);
    let texts: Vec<&str> = t.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(texts, ["g[a, c, _]", "g[b, c, _]"]);
    assert!(approx3(t[1].local, [10.0, 1.0, 50.0]));
    assert!(approx3(t[1].world, [10.0, 1.0, 50.0]));
    assert_eq!(t[1].world, t[1].result_world);
    // Under a transform the written numbers end up elsewhere.
    let h = handle(&out, src, "translate([1, 1, 1])", HandleKind::Point, 0);
    let t = snap_targets(&out, h.id);
    assert!(approx3(t[1].result_world, [10.0, 1.0, 51.0]));
    // Before the grid: no targets.
    let src = "translate([7, 0, 0]) cube(1); grid g { x: a = 0; }";
    let out = clean(src);
    let h = handle(&out, src, "translate", HandleKind::Point, 0);
    assert!(snap_targets(&out, h.id).is_empty());
    // Scaled handles cannot snap.
    let src = "grid g { x: a = 1; } cube([2, 2, 2], center = true);";
    let out = clean(src);
    let h = handle(&out, src, "cube", HandleKind::Point, 0);
    assert!(snap_targets(&out, h.id).is_empty());
}

#[test]
fn snap_visibility_respects_module_boundaries() {
    let src = "module m() {\n  grid g { x: a = 3; }\n  translate([1, 0, 0]) cube(1);\n}\nm();";
    let out = run_module(src, "m", vec![]);
    let h = handle(&out, src, "translate", HandleKind::Point, 0);
    let r = SnapRef::Grid { grid: 0, planes: ["a".into(), "_".into(), "_".into()] };
    let new = drag(&out, src, h, DragTarget::Snap(r));
    assert!(new.contains("translate(g[a, _, _]) cube(1);"), "{new}");
    let out = run_module(&new, "m", vec![]);
    assert!(errors(&out).is_empty());
}

#[test]
fn spec_gehaeuse_handles_in_module_context() {
    let src = include_str!("tree.rs");
    let start = src.find("module gehaeuse").unwrap();
    let end = src[start..].find("\"#;").unwrap() + start;
    let src = &src[start..end];
    let out = run_module(src, "gehaeuse", vec![]);
    let g = out.grid_by_name("g").unwrap();
    // Literal plane through a parameter default: editable, marked as default.
    let il = g.planes.iter().position(|p| p.name == "innen_l").unwrap() as u32;
    let h = handle(&out, src, "grid g", HandleKind::Plane { axis: 0 }, il);
    assert!(h.via_default);
    assert_eq!(comp_texts(src, h), [Some("3")]);
    // `w - t` is computed: no handle.
    let ir = g.planes.iter().position(|p| p.name == "innen_r").unwrap() as u32;
    assert!(!out.handles.iter().any(|h| h.kind == (HandleKind::Plane { axis: 0 }) && h.index == ir));
    // Box corners come from grid references.
    let b = handle(&out, src, "box(g[links", HandleKind::Point, 1);
    assert!(b.has_ref);
    assert!(approx3(b.pos, [40.0, 30.0, 20.0]));
    // The second box's z comes from `t` (literal through the default).
    let b2 = handle(&out, src, "box(g[innen_l", HandleKind::Point, 0);
    assert_eq!(comp_texts(src, b2), [None, None, Some("3")]);
}

#[test]
fn set_default() {
    let src = "module m(a = 1, b) cube(a);";
    let (file, _) = cad_syntax::parse(src);
    let e = set_default_edit(src, &file, "m", "a", "5").unwrap();
    assert_eq!(apply_edits(src, &[e]), "module m(a = 5, b) cube(a);");
    let e = set_default_edit(src, &file, "m", "b", "[1, 2, 3]").unwrap();
    assert_eq!(apply_edits(src, &[e]), "module m(a = 1, b = [1, 2, 3]) cube(a);");
    assert!(set_default_edit(src, &file, "m", "zz", "1").is_none());
    assert!(set_default_edit(src, &file, "nope", "a", "1").is_none());
}

#[test]
fn handles_only_in_context_and_unique() {
    let src = "module m() translate([1, 0, 0]) cube(1);\nm(); m();\nfor (i = [0:2]) translate([i, 5, 0]) cube(1);";
    let out = clean(src);
    assert!(no_handle(&out, src, "translate([1"));
    let loop_handles: Vec<&Handle> =
        out.handles.iter().filter(|h| span_text(src, h.span).starts_with("translate([i")).collect();
    assert_eq!(loop_handles.len(), 3);
    // Long loops: at most 32 handles per statement and component.
    let src2 = "for (i = [0:99]) translate([i, 0, 5]) cube(1);";
    let out2 = clean(src2);
    assert_eq!(out2.handles.iter().filter(|h| span_text(src2, h.span).starts_with("translate")).count(), 32);
    let mut ids: Vec<u64> = out.handles.iter().map(|h| h.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), out.handles.len());
    assert!(ids.iter().all(|i| *i < (1 << 53)));
}

#[test]
fn computed_values_have_locked_handles() {
    let src = "p = [1, 2, 3];\ntranslate(p + [1, 0, 0]) cube(1);\nw = 4; grid g { x: a = w * 2, b = 5; }";
    let out = clean(src);
    assert!(no_handle(&out, src, "translate"));
    let t = out
        .locked_handles
        .iter()
        .find(|h| span_text(src, h.span).starts_with("translate"))
        .expect("locked translate handle");
    assert!(t.locked && !t.has_ref);
    assert!(approx3(t.pos, [2.0, 2.0, 3.0]));
    assert!(t.comps.iter().all(|c| c.is_none()));
    // Locked handles cannot be dragged.
    assert!(drag_edits(&out, src, t.id, &free([9.0, 9.0, 9.0]), &DragOptions::default()).is_empty());
    // Expression plane `w * 2`: locked; literal plane `b = 5`: editable.
    let g = out.grid_by_name("g").unwrap();
    let a = g.planes.iter().position(|p| p.name == "a").unwrap() as u32;
    assert!(out.locked_handles.iter().any(|h| h.kind == (HandleKind::Plane { axis: 0 }) && h.index == a));
    assert!(out.handles.iter().all(|h| !h.locked));
    let mut ids: Vec<u64> = out.handles.iter().chain(&out.locked_handles).map(|h| h.id).collect();
    let n = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n);
}

const PLANE_SNAP_SRC: &str = "grid g {
    x: rechts = 15;
    z: boden = 0, mitte = 12, deckel = 20;
}
cylinder(h = 25, r = 3);
translate([0, 0, 5]) cylinder(h = 10, r = 2);
hh = 7;
linear_extrude(height = hh) square(3);
sphere(r = 4);
";

#[test]
fn one_d_handles_snap_to_grid_planes() {
    let src = PLANE_SNAP_SRC;
    let out = clean(src);
    let h = handle(&out, src, "cylinder(h = 25", HandleKind::Height, 0);
    let t = snap_targets(&out, h.id);
    let texts: Vec<&str> = t.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(texts, ["g.boden", "g.mitte", "g.deckel"]);
    assert!(approx3(t[2].world, [0.0, 0.0, 20.0]));
    assert_eq!(t[2].world, t[2].result_world);
    let new = drag(&out, src, h, DragTarget::Snap(t[2].r#ref.clone()));
    assert!(new.contains("cylinder(h = g.deckel, r = 3);"), "{new}");
    let out2 = clean(&new);
    let h2 = handle(&out2, &new, "cylinder(h = g.deckel", HandleKind::Height, 0);
    assert!(h2.has_ref && !h2.locked);
    assert_eq!(h2.value, [20.0]);
    // Alt drag of the reference writes a number again.
    let freed = drag(&out2, &new, h2, scalar(17.2));
    assert!(freed.contains("cylinder(h = 17, r = 3);"), "{freed}");
    // Radius along x snaps onto the x plane.
    let r = handle(&out, src, "cylinder(h = 25", HandleKind::Radius, 0);
    let t = snap_targets(&out, r.id);
    assert_eq!(t.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["g.rechts"]);
    let new = drag(&out, src, r, DragTarget::Snap(t[0].r#ref.clone()));
    assert!(new.contains("cylinder(h = 25, r = g.rechts);"), "{new}");
    clean(&new);
    // Moved by translate: writing g.deckel would not land on the plane → no targets.
    let ht = handle(&out, src, "cylinder(h = 10", HandleKind::Height, 0);
    assert!(snap_targets(&out, ht.id).is_empty());
    let bad = SnapRef::Plane { grid: 0, plane: "deckel".into() };
    assert!(drag_edits(&out, src, ht.id, &DragTarget::Snap(bad), &DragOptions::default()).is_empty());
    // Through a variable: the definition is rewritten.
    let le = handle(&out, src, "linear_extrude", HandleKind::Height, 0);
    let t = snap_targets(&out, le.id);
    let m = t.iter().find(|t| t.text == "g.mitte").expect("g.mitte");
    let new = drag(&out, src, le, DragTarget::Snap(m.r#ref.clone()));
    assert!(new.contains("hh = g.mitte;"), "{new}");
    let out3 = clean(&new);
    let le3 = handle(&out3, &new, "linear_extrude", HandleKind::Height, 0);
    assert_eq!(le3.value, [12.0]);
    // Sphere radius (axis x) → g.rechts.
    let s = handle(&out, src, "sphere", HandleKind::Radius, 0);
    assert_eq!(snap_targets(&out, s.id).len(), 1);
}

#[test]
fn plane_snap_respects_visibility() {
    // Before the grid declaration nothing can reference it.
    let src = "cylinder(h = 25, r = 3);\ngrid g { z: deckel = 20; }";
    let out = clean(src);
    let h = handle(&out, src, "cylinder", HandleKind::Height, 0);
    assert!(snap_targets(&out, h.id).is_empty());
    // A plane of a later-declared grid can snap onto an earlier grid's plane, not its own.
    let src = "grid a { z: top = 10; }\ngrid b { z: p = 3, q = 10; }";
    let out = clean(src);
    let b = out.grid_by_name("b").unwrap();
    let q = b.planes.iter().position(|p| p.name == "q").unwrap() as u32;
    let hq = handle(&out, src, "grid b", HandleKind::Plane { axis: 2 }, q);
    let t = snap_targets(&out, hq.id);
    assert_eq!(t.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["a.top"]);
    let new = drag(&out, src, hq, DragTarget::Snap(t[0].r#ref.clone()));
    assert_eq!(new, "grid a { z: top = 10; }\ngrid b { z: p = 3, q = a.top; }");
    clean(&new);
}

#[test]
fn zone_center_handle_through_translate() {
    let src = "grid g { x: r = 20; y: h = 20; z: d = 20; }\nfillet(2) {\n    cube(20);\n    translate([20, 20, 20]) sphere(8);\n}";
    let out = run(src);
    let h = handle(&out, src, "translate([20, 20, 20]) sphere", HandleKind::Point, 0);
    assert!(approx3(h.frame.transform_point3(h.pos), [20.0, 20.0, 20.0]));
    let t = snap_targets(&out, h.id);
    assert!(t.iter().any(|t| t.text == "g[r, h, d]"), "{t:?}");
    let new = drag(&out, src, h, DragTarget::Snap(SnapRef::Grid { grid: 0, planes: ["r".into(), "h".into(), "d".into()] }));
    assert!(new.contains("translate(g[r, h, d]) sphere(8);"), "{new}");
}
