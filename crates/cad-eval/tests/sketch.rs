//! Sketches (Plan 17.1): segments, contours, errors per segment line, handles; modules as
//! namespaces (Plan 17.2).

mod common;

use std::f64::consts::PI;

use cad_eval::*;
use cad_ir::{Op, Profile, Seg};
use common::*;
use glam::{DVec2, DVec3};

fn sketch_of(out: &EvalOutput) -> (Vec<cad_ir::Contour>, bool) {
    let n = out.tree.nodes.iter().find(|n| matches!(n.op, Op::Profile(Profile::Sketch { .. }))).expect("sketch node");
    let Op::Profile(Profile::Sketch { contours, closed }) = &n.op else { unreachable!() };
    (contours.clone(), *closed)
}

fn line_of(src: &str, s: cad_syntax::Span) -> usize {
    src[..s.start as usize].matches('\n').count() + 1
}

/// Errors with their line numbers.
fn errors_at(out: &EvalOutput, src: &str) -> Vec<(usize, String)> {
    out.diagnostics
        .iter()
        .filter(|d| d.severity == cad_syntax::Severity::Error)
        .map(|d| (line_of(src, d.span), d.message.clone()))
        .collect()
}

fn near(a: DVec2, b: [f64; 2]) -> bool {
    (a - DVec2::from_array(b)).length() < 1e-9
}

#[test]
fn segments_of_all_kinds() {
    let src = "linear_extrude(5) sketch {\n\
        start([0, 0]);\n\
        line(to = [40, 0]);\n\
        arc(to = [50, 10], r = 10);\n\
        line(by = [0, 20]);\n\
        line(length = 5, angle = 180);\n\
        bezier(c1 = [40, 40], to = [20, 35]);\n\
        bezier(c1 = [15, 30], c2 = [10, 30], to = [5, 25]);\n\
        spline([[3, 15], [0, 10]], tangent = true);\n\
        close();\n\
        }";
    let out = clean(src);
    let (cs, closed) = sketch_of(&out);
    assert!(closed);
    assert_eq!(cs.len(), 1);
    let segs = &cs[0].segs;
    let kinds: Vec<&str> = segs.iter().map(|s| s.kind()).collect();
    assert_eq!(kinds, ["line", "arc", "line", "line", "bezier", "bezier", "spline", "line"]);
    // arc: end point + radius, counter-clockwise, small: centre (40, 10)
    let Seg::Arc { center, r, a0, a1, ccw } = &segs[1] else { panic!() };
    assert!(near(*center, [40.0, 10.0]) && (*r - 10.0).abs() < 1e-12 && *ccw);
    assert!((a1 - a0 - PI / 2.0).abs() < 1e-12);
    assert!(near(segs[2].end(), [50.0, 30.0]));
    assert!(near(segs[3].end(), [45.0, 30.0]));
    // each segment starts where the previous ends; close() adds the closing line
    for w in segs.windows(2) {
        assert!((w[0].end() - w[1].start()).length() < 1e-9);
    }
    assert!(near(segs[7].end(), [0.0, 0.0]));
    // the spline continues the Bézier's end direction
    let Seg::Spline { t0: Some(t), .. } = &segs[6] else { panic!("{:?}", segs[6]) };
    assert!((t.normalize() - DVec2::new(-1.0, -1.0).normalize()).length() < 1e-9);
    // statements of the segments
    let info = &out.sketches[0];
    assert_eq!(info.contours[0].segs.len(), 8);
    let lines: Vec<usize> = info.contours[0].segs.iter().map(|g| line_of(src, g.span)).collect();
    assert_eq!(lines, [3, 4, 5, 6, 7, 8, 9, 10]);
    assert_eq!(line_of(src, info.contours[0].start_span.unwrap()), 2);
}

#[test]
fn arc_forms() {
    let arc = |s: &str| {
        let out = clean(&format!("sketch {{ start([10, 0]); {s} close(); }}"));
        let (cs, _) = sketch_of(&out);
        cs[0].segs[0].clone()
    };
    // centre + angle, clockwise with a negative angle
    let Seg::Arc { center, a0, a1, ccw, .. } = arc("arc(center = [0, 0], angle = -90);") else { panic!() };
    assert!(near(center, [0.0, 0.0]) && !ccw && (a1 - a0 + PI / 2.0).abs() < 1e-12);
    // through a point: centre of the circle through the three points
    let Seg::Arc { center, r, ccw, .. } = arc("arc(through = [0, 10], to = [-10, 0]);") else { panic!() };
    assert!(near(center, [0.0, 0.0]) && (r - 10.0).abs() < 1e-12 && ccw);
    // large and clockwise variants of end point + radius
    let Seg::Arc { a0, a1, .. } = arc("arc(to = [0, 10], r = 10, large = true);") else { panic!() };
    assert!((a1 - a0 - 1.5 * PI).abs() < 1e-12);
    let Seg::Arc { center, a0, a1, ccw, .. } = arc("arc(to = [0, 10], r = 10, cw = true);") else { panic!() };
    assert!(!ccw && (a1 - a0 + 0.5 * PI).abs() < 1e-12 && near(center, [10.0, 10.0]));
    let Seg::Arc { a0, a1, ccw, .. } = arc("arc(to = [0, 10], r = 10, cw = true, large = true);") else { panic!() };
    assert!(!ccw && (a1 - a0 + 1.5 * PI).abs() < 1e-12);
    // tangent arc after a line: a quarter circle to the left
    let out = clean("sketch { start([0, 0]); line(to = [10, 0]); arc(to = [20, 10]); line(to = [0, 10]); close(); }");
    let (cs, _) = sketch_of(&out);
    let Seg::Arc { center, r, ccw, .. } = &cs[0].segs[1] else { panic!() };
    assert!(near(*center, [10.0, 10.0]) && (*r - 10.0).abs() < 1e-12 && *ccw);
}

#[test]
fn errors_per_segment_line() {
    // not closed
    let src = "sketch {\n start([0, 0]);\n line(to = [10, 0]);\n line(to = [10, 10]);\n}";
    let out = run(src);
    let e = errors_at(&out, src);
    assert_eq!(e.len(), 1, "{e:?}");
    assert_eq!(e[0].0, 4);
    assert!(e[0].1.contains("nicht geschlossen"), "{e:?}");
    // closed = false: no error
    let out = run("sketch(closed = false) { start([0, 0]); line(to = [10, 0]); line(to = [10, 10]); }");
    assert!(errors(&out).is_empty(), "{:?}", errors(&out));
    // impossible arc: radius below half the chord
    let src = "sketch {\n start([0, 0]);\n arc(to = [10, 0], r = 4);\n close();\n}";
    let e = errors_at(&run(src), src);
    assert!(e.iter().any(|(l, m)| *l == 3 && m.contains("halbe Sehne")), "{e:?}");
    // self-intersection (bow tie): reported at the later segment
    let src =
        "sketch {\n start([0, 0]);\n line(to = [10, 10]);\n line(to = [10, 0]);\n line(to = [0, 10]);\n close();\n}";
    let e = errors_at(&run(src), src);
    assert!(e.iter().any(|(l, m)| *l == 5 && m.contains("Selbstüberschneidung") && m.contains("Zeile 3")), "{e:?}");
    // contour without area
    let src = "sketch {\n start([0, 0]);\n line(to = [10, 0]);\n close();\n}";
    let e = errors_at(&run(src), src);
    assert!(e.iter().any(|(l, m)| *l == 2 && m.contains("ohne Fläche")), "{e:?}");
    // segment before start(), segment after close()
    let e = errors(&run("sketch { line(to = [1, 0]); }"));
    assert!(e.iter().any(|m| m.contains("zuerst `start(p)`")), "{e:?}");
    let e = errors(&run("sketch { start([0,0]); line(to=[1,0]); line(to=[0,1]); close(); line(to = [5, 5]); }"));
    assert!(e.iter().any(|m| m.contains("nach `close()`")), "{e:?}");
    // tangent arc on its tangent
    let e = errors(&run("sketch { start([0,0]); line(to=[1,0]); arc(to=[5,0]); close(); }"));
    assert!(e.iter().any(|m| m.contains("auf der Tangente")), "{e:?}");
    // outside a sketch, and other statements inside one
    assert!(has_error(&run("line(to = [1, 0]);"), "nur innerhalb von `sketch"));
    assert!(has_error(&run("sketch { start([0,0]); cube(3); }"), "in einer Skizze nicht erlaubt"));
    // corner() is reserved
    let out = run("sketch { start([0,0]); line(to=[4,0]); corner(r = 1); line(to=[0,4]); close(); }");
    assert!(has_warning(&out, "noch nicht unterstützt"), "{:?}", warnings(&out));
    assert!(errors(&out).is_empty());
}

#[test]
fn loops_variables_grids_and_modules_in_a_sketch() {
    let src = "module zahn(i, n) {\n a = 360 * i / n;\n line(to = [12 * cos(a + 4), 12 * sin(a + 4)]);\n line(to = [12 * cos(a + 360 / n - 4), 12 * sin(a + 360 / n - 4)]);\n line(to = [10 * cos(a + 360 / n), 10 * sin(a + 360 / n)]);\n}\nn = 12;\nlinear_extrude(4) sketch {\n grid s { x: l = 0, r = 10; y: u = 0; }\n start(s[r, u]);\n for (i = [0 : n - 1]) zahn(i, n);\n if (n > 3) close();\n}";
    let out = clean(src);
    let (cs, _) = sketch_of(&out);
    assert_eq!(cs[0].segs.len(), 36);
    assert!(near(cs[0].segs[0].start(), [10.0, 0.0]));
    // grid points: z ≠ 0 is projected with a warning
    let out = run(
        "grid g { x: a = 5; y: b = 2; z: c = 3; }\nsketch { start(g[a, b, c]); line(by = [1, 0]); line(by = [0, 1]); close(); }",
    );
    assert!(has_warning(&out, "z-Koordinate ignoriert"));
    // let in a sketch
    clean("sketch { let (w = 4) { start([0, 0]); line(to = [w, 0]); line(to = [w, w]); close(); } }");
}

#[test]
fn several_contours_holes_and_open_paths() {
    let out = clean(
        "sketch {\n start([0,0]); line(to=[20,0]); line(to=[20,20]); line(to=[0,20]); close();\n hole { start([5,5]); line(to=[10,5]); line(to=[10,10]); close(); }\n start([30,0]); line(by=[5,0]); line(by=[0,5]); close();\n}",
    );
    let (cs, _) = sketch_of(&out);
    assert_eq!(cs.len(), 3);
    assert_eq!(cs.iter().map(|c| c.hole).collect::<Vec<_>>(), [false, true, false]);
    let out = clean("sketch(closed = false) { start([0, 0]); line(to = [10, 0]); arc(to = [20, 10]); }");
    let (cs, closed) = sketch_of(&out);
    assert!(!closed && !cs[0].closed && cs[0].segs.len() == 2);
    // sweep with a sketch path child instead of `path`
    let out = clean("sweep() { sketch(closed = false) { start([0, 0]); line(to = [10, 0]); } circle(1); }");
    assert!(out.tree.nodes.iter().any(|n| matches!(&n.op, Op::Sweep { path, .. } if path.is_empty())));
}

#[test]
fn handles_of_segments_and_dragging() {
    let src = "linear_extrude(2) sketch {\n start([0, 0]);\n line(to = [40, 0]);\n arc(to = [50, 10], r = 10);\n line(by = [0, 20]);\n bezier(c1 = [45, 40], to = [20, 35]);\n close();\n}";
    let out = clean(src);
    let find = |stmt: &str, kind: HandleKind, index: u32| -> &Handle {
        out.handles
            .iter()
            .find(|h| span_text(src, h.span).trim_start().starts_with(stmt) && h.kind == kind && h.index == index)
            .unwrap_or_else(|| panic!("no handle on {stmt}"))
    };
    let drag = |h: &Handle, t: DragTarget| apply_edits(src, &drag_edits(&out, src, h.id, &t, &DragOptions::default()));
    // end point
    let h = find("line(to = [40", HandleKind::Point, 0);
    assert!(approx3(h.pos, [40.0, 0.0, 0.0]));
    let new = drag(h, DragTarget::Free(DragValue::Point(DVec3::new(42.2, 0.0, 0.0))));
    assert!(new.contains("line(to = [42, 0]);"), "{new}");
    // relative end point: the handle sits at the end, the value is relative
    let h = find("line(by", HandleKind::Point, 0);
    let world = h.frame.transform_point3(h.pos);
    assert!(approx3(world, [50.0, 30.0, 0.0]), "{world}");
    let local = h.frame.inverse().transform_point3(DVec3::new(50.0, 33.0, 0.0));
    let new = drag(h, DragTarget::Free(DragValue::Point(local)));
    assert!(new.contains("line(by = [0, 23]);"), "{new}");
    // relative values are not snapped to (absolute) grid points
    assert!(snap_targets(&out, h.id).is_empty());
    // arc radius
    let h = find("arc(", HandleKind::Radius, 0);
    assert_eq!(h.value, vec![10.0]);
    let new = drag(h, DragTarget::Free(DragValue::Scalar(12.0)));
    assert!(new.contains("r = 12"), "{new}");
    // Bézier control point
    let h = find("bezier", HandleKind::Point, 1);
    assert!(approx3(h.pos, [45.0, 40.0, 0.0]));
    // start point
    assert!(approx3(find("start", HandleKind::Point, 0).pos, [0.0, 0.0, 0.0]));
}

#[test]
fn named_sketch_module_and_qualified_profiles() {
    let src = "module profil(b = 40) sketch { start([0, 0]); line(to = [b, 0]); line(to = [b, 5]); line(to = [0, 5]); close(); }\nlinear_extrude(3) profil(b = 30);\nmodule profile { module stern() sketch { start([5, 0]); line(to = [0, 5]); line(to = [-5, 0]); close(); } }\nlinear_extrude(2) profile.stern();";
    let out = clean(src);
    let n = out.tree.nodes.iter().filter(|n| matches!(n.op, Op::Profile(Profile::Sketch { .. }))).count();
    assert_eq!(n, 2);
    let (cs, _) = sketch_of(&out);
    assert!(near(cs[0].segs[0].end(), [30.0, 0.0]));
}

// ---------------------------------------------------------------- namespaces (17.2)

const NS: &str = "module teile(k = 2) {\n\
    breite = 10 * k;\n\
    grid g { x: a = 0; }\n\
    function flaeche(r) = PI * r * r;\n\
    module schraube(d = 3) { cylinder(h = breite, d = d); }\n\
    module mutter { module m3() { cube(k); } }\n\
    cube(1);\n\
}\n";

#[test]
fn qualified_module_calls() {
    let src = format!(
        "{NS}teile.schraube(d = 4);\nteile.mutter.m3();\nk: teile.schraube();\necho(teile.flaeche(2), teile.breite);"
    );
    let out = clean(&src);
    // the inner module sees the outer parameters with their defaults
    let cyl = out.tree.nodes.iter().find_map(|n| match &n.op {
        Op::Prim { prim: cad_ir::Prim::Cylinder { h, r1, .. }, .. } => Some((*h, *r1)),
        _ => None,
    });
    assert_eq!(cyl, Some((20.0, 2.0)));
    assert!(
        out.tree
            .nodes
            .iter()
            .any(|n| matches!(&n.op, Op::Prim { prim: cad_ir::Prim::Box { max, .. }, .. } if max.x == 2.0))
    );
    // named instance of a qualified call
    assert!(out.parts.iter().any(|p| p.name == "k"));
    // functions and constants of a module
    assert_eq!(echoes(&out), [format!("{}, 20", fmt_num(PI * 4.0))]);
    // the namespace does not create the outer module's geometry
    assert!(
        !out.tree
            .nodes
            .iter()
            .any(|n| matches!(&n.op, Op::Prim { prim: cad_ir::Prim::Box { max, .. }, .. } if max.x == 1.0))
    );
}

#[test]
fn namespace_visibility_and_errors() {
    // grids of the outer module are not visible in the inner one
    let src = format!("{NS}module x {{}}\nteile.mutter.m3();");
    clean(&src);
    let out = run("module a { grid g { x: p = 1; } module b() { translate([g.p, 0, 0]) cube(1); } }\na.b();");
    assert!(has_warning(&out, "unbekannte Variable `g`"), "{:?}", out.diagnostics);
    // unknown parts of a path
    assert!(has_error(&run(&format!("{NS}teile.bolzen();")), "Modul `teile` hat kein Untermodul `bolzen`"));
    assert!(has_error(&run("nix.bolzen();"), "unbekanntes Modul `nix`"));
    assert!(has_warning(&run(&format!("{NS}echo(teile.gibtsnicht);")), "hat keine Funktion oder Variable"));
    // a variable shadows the module name: plain member access (`v.x`)
    let out = clean("module v { function f() = 1; }\nv = [3, 4];\necho(v.x);");
    assert_eq!(echoes(&out), ["3"]);
}

#[test]
fn qualified_context_module() {
    let src = format!("{NS}teile.schraube();");
    for name in ["teile.schraube", "teile::schraube", "schraube"] {
        let out = run_module(&src, name, vec![]);
        assert!(errors(&out).is_empty(), "{name}: {:?}", errors(&out));
        assert_eq!(out.context.path, ["teile", "schraube"], "{name}");
        let h = out.tree.nodes.iter().find_map(|n| match &n.op {
            Op::Prim { prim: cad_ir::Prim::Cylinder { h, .. }, .. } => Some(*h),
            _ => None,
        });
        assert_eq!(h, Some(20.0));
    }
    let out = run_module(&src, "teile.mutter.m3", vec![]);
    assert_eq!(out.context.path, ["teile", "mutter", "m3"]);
}

#[test]
fn openscad_files_are_unaffected() {
    // user modules named like segments keep working outside sketches
    let out = run(
        "module line(a, b) { hull() { translate(a) sphere(1); translate(b) sphere(1); } }\nline([0,0,0], [10,0,0]);",
    );
    assert!(errors(&out).is_empty(), "{:?}", errors(&out));
    assert_eq!(out.tree.nodes.iter().filter(|n| matches!(n.op, Op::Prim { .. })).count(), 2);
    let out = clean("module arc(r) { circle(r); }\nlinear_extrude(1) arc(5);");
    assert!(out.tree.nodes.iter().any(|n| matches!(n.op, Op::Profile(Profile::Circle { .. }))));
    // spline() outside a sketch is still the 2D primitive
    let out = clean("linear_extrude(1) spline([[0,0],[5,0],[5,5],[0,5]]);");
    assert!(out.tree.nodes.iter().any(|n| matches!(n.op, Op::Profile(Profile::Spline { .. }))));
}
