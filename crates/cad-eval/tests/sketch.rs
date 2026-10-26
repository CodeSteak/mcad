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
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
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
}

// ---------------------------------------------------------------- corner (17.3, step 2)

fn arcs_of(cs: &[cad_ir::Contour]) -> Vec<(DVec2, f64)> {
    cs.iter()
        .flat_map(|c| c.segs.iter())
        .filter_map(|g| match g {
            Seg::Arc { center, r, .. } => Some((*center, *r)),
            _ => None,
        })
        .collect()
}

#[test]
fn corner_rules() {
    // between two segments: the vertex between them
    let out = clean(
        "sketch { start([0, 0]); line(to = [20, 0]); corner(r = 3); line(to = [20, 10]); line(to = [0, 10]); close(); }",
    );
    let (cs, _) = sketch_of(&out);
    assert_eq!(cs[0].segs.len(), 5);
    let a = arcs_of(&cs);
    assert_eq!(a.len(), 1);
    assert!(near(a[0].0, [17.0, 3.0]) && approx(a[0].1, 3.0), "{a:?}");
    assert!(near(cs[0].segs[0].end(), [17.0, 0.0]));
    // directly after start(p): the start vertex (between the closing line and the first segment)
    let out = clean(
        "sketch { start([0, 0]); corner(r = 2); line(to = [20, 0]); line(to = [20, 10]); line(to = [0, 10]); close(); }",
    );
    let (cs, _) = sketch_of(&out);
    let a = arcs_of(&cs);
    assert!(near(a[0].0, [2.0, 2.0]), "{a:?}");
    // the contour starts after the rounding, the rounding is its last segment
    assert!(near(cs[0].segs[0].start(), [2.0, 0.0]));
    assert!(matches!(cs[0].segs.last(), Some(Seg::Arc { .. })));
    // before close(): the vertex between the last segment and the closing line
    let out = clean(
        "sketch { start([0, 0]); line(to = [20, 0]); line(to = [20, 10]); line(to = [0, 10]); corner(r = 2); close(); }",
    );
    let a = arcs_of(&sketch_of(&out).0);
    assert!(near(a[0].0, [2.0, 8.0]), "{a:?}");
    // at the end of a contour that ends at its start (no closing line): the start vertex
    let out = clean(
        "sketch { start([0, 0]); line(to = [20, 0]); line(to = [20, 10]); line(to = [0, 10]); line(to = [0, 0]); corner(r = 2); }",
    );
    let a = arcs_of(&sketch_of(&out).0);
    assert!(near(a[0].0, [2.0, 2.0]), "{a:?}");
    // chamfer: a line between the points at distance d from the vertex
    let out = clean(
        "sketch { start([0, 0]); line(to = [20, 0]); corner(chamfer = 4); line(to = [20, 10]); line(to = [0, 10]); close(); }",
    );
    let (cs, _) = sketch_of(&out);
    assert!(
        cs[0].segs.iter().any(|g| matches!(g, Seg::Line { a, b } if near(*a, [16.0, 0.0]) && near(*b, [20.0, 4.0])))
    );
    // line into an arc: the rounding is tangent to both
    let out = clean(
        "sketch { start([0, 0]); line(to = [10, 0]); corner(r = 1); arc(center = [0, 0], angle = 90); close(); }",
    );
    let a = arcs_of(&sketch_of(&out).0);
    let (c, r) = a.iter().find(|(_, r)| approx(*r, 1.0)).copied().unwrap();
    assert!(approx(c.y, 1.0) && approx(c.length(), 9.0), "{c} {r}");
    // in loops: a rounded zigzag
    let out = clean(
        "sketch { start([0, 0]); for (i = [1 : 4]) { line(to = [i * 10, (i % 2) * 5]); if (i < 4) corner(r = 1); } line(to = [40, -5]); line(to = [0, -5]); close(); }",
    );
    assert_eq!(arcs_of(&sketch_of(&out).0).len(), 3);
}

#[test]
fn corner_errors_at_the_line() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let src = "sketch {\n start([0, 0]);\n line(to = [4, 0]);\n corner(r = 5);\n line(to = [4, 4]);\n line(to = [0, 4]);\n close();\n}";
    let out = run(src);
    let e = errors_at(&out, src);
    assert_eq!(e.len(), 1, "{e:?}");
    assert_eq!(e[0].0, 4);
    assert!(e[0].1.contains("passt nicht") && e[0].1.contains("höchstens ≈ 4"), "{e:?}");
    // two corners sharing a short segment: both fail, with the largest common radius
    let src = "sketch {\n start([0, 0]);\n line(to = [10, 0]);\n corner(r = 2);\n line(to = [10, 3]);\n corner(r = 2);\n line(to = [0, 3]);\n close();\n}";
    let e = errors_at(&run(src), src);
    assert_eq!(e.iter().map(|x| x.0).collect::<Vec<_>>(), [4, 6], "{e:?}");
    assert!(e[0].1.contains("höchstens ≈ 1.5"), "{e:?}");
    // tangent: no corner
    let src = "sketch {\n start([0, 0]);\n line(to = [10, 0]);\n corner(r = 1);\n arc(to = [10, 10]);\n close();\n}";
    let e = errors_at(&run(src), src);
    assert!(e.iter().any(|(l, m)| *l == 4 && m.contains("tangential")), "{e:?}");
    // open contour: no start corner, nothing after the last segment
    let e =
        errors(&run("sketch(closed = false) { start([0, 0]); corner(r = 1); line(to = [5, 0]); line(to = [5, 5]); }"));
    assert!(e.iter().any(|m| m.contains("keine Startecke")), "{e:?}");
    let e =
        errors(&run("sketch(closed = false) { start([0, 0]); line(to = [5, 0]); line(to = [5, 5]); corner(r = 1); }"));
    assert!(e.iter().any(|m| m.contains("folgt keines mehr")), "{e:?}");
    // after close(), before start, twice at one vertex, bad values
    assert!(has_error(
        &run("sketch { start([0, 0]); line(to = [5, 0]); line(to = [0, 5]); close(); corner(r = 1); }"),
        "nach `close()`"
    ));
    assert!(has_error(
        &run("sketch { corner(r = 1); start([0, 0]); line(to = [5, 0]); line(to = [0, 5]); close(); }"),
        "vor `start(p)`"
    ));
    assert!(has_error(
        &run("sketch { start([0, 0]); line(to = [5, 0]); corner(r = 1); corner(r = 2); line(to = [0, 5]); close(); }"),
        "schon ein `corner()`"
    ));
    assert!(has_error(
        &run("sketch { start([0, 0]); line(to = [5, 0]); corner(r = -1); line(to = [0, 5]); close(); }"),
        "muss > 0 sein"
    ));
    assert!(has_error(
        &run("sketch { start([0, 0]); line(to = [5, 0]); corner(); line(to = [0, 5]); close(); }"),
        "braucht `r`"
    ));
    assert!(has_error(&run("corner(r = 1);"), "nur innerhalb von `sketch"));
}

#[test]
fn corner_radius_handle() {
    let src = "linear_extrude(2) sketch {\n start([0, 0]);\n line(to = [20, 0]);\n corner(r = 3);\n line(to = [20, 10]);\n line(to = [0, 10]);\n close();\n}";
    let out = clean(src);
    let h = out
        .handles
        .iter()
        .find(|h| span_text(src, h.span).starts_with("corner") && h.kind == HandleKind::Radius)
        .expect("radius handle");
    assert_eq!(h.value, vec![3.0]);
    // on the rounding's midpoint; base at the vertex, linear in r
    let mid = DVec2::new(17.0, 3.0) + DVec2::new(1.0, -1.0).normalize() * 3.0;
    assert!(approx3(h.pos, [mid.x, mid.y, 0.0]), "{}", h.pos);
    assert!(approx3(h.base.unwrap(), [20.0, 0.0, 0.0]));
    let axis = h.axis.unwrap();
    assert!(approx3(h.base.unwrap() + axis * 3.0, [mid.x, mid.y, 0.0]));
    let new = apply_edits(
        src,
        &drag_edits(&out, src, h.id, &DragTarget::Free(DragValue::Scalar(4.0)), &DragOptions::default()),
    );
    assert!(new.contains("corner(r = 4);"), "{new}");
}

#[test]
fn loops_variables_grids_and_modules_in_a_sketch() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let src = "module tooth(i, n) {\n a = 360 * i / n;\n line(to = [12 * cos(a + 4), 12 * sin(a + 4)]);\n line(to = [12 * cos(a + 360 / n - 4), 12 * sin(a + 360 / n - 4)]);\n line(to = [10 * cos(a + 360 / n), 10 * sin(a + 360 / n)]);\n}\nn = 12;\nlinear_extrude(4) sketch {\n grid s { x: l = 0, r = 10; y: u = 0; }\n start(s[r, u]);\n for (i = [0 : n - 1]) tooth(i, n);\n if (n > 3) close();\n}";
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
    let src = "module strip(b = 40) sketch { start([0, 0]); line(to = [b, 0]); line(to = [b, 5]); line(to = [0, 5]); close(); }\nlinear_extrude(3) strip(b = 30);\nmodule profile { module star() sketch { start([5, 0]); line(to = [0, 5]); line(to = [-5, 0]); close(); } }\nlinear_extrude(2) profile.star();";
    let out = clean(src);
    let n = out.tree.nodes.iter().filter(|n| matches!(n.op, Op::Profile(Profile::Sketch { .. }))).count();
    assert_eq!(n, 2);
    let (cs, _) = sketch_of(&out);
    assert!(near(cs[0].segs[0].end(), [30.0, 0.0]));
}

// ---------------------------------------------------------------- namespaces (17.2)

const NS: &str = "module parts(k = 2) {\n\
    width = 10 * k;\n\
    grid g { x: a = 0; }\n\
    function area(r) = PI * r * r;\n\
    module screw(d = 3) { cylinder(h = width, d = d); }\n\
    module nut { module m3() { cube(k); } }\n\
    cube(1);\n\
}\n";

#[test]
fn qualified_module_calls() {
    let src = format!("{NS}parts.screw(d = 4);\nparts.nut.m3();\nk: parts.screw();\necho(parts.area(2), parts.width);");
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
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    // grids of the outer module are not visible in the inner one
    let src = format!("{NS}module x {{}}\nparts.nut.m3();");
    clean(&src);
    let out = run("module a { grid g { x: p = 1; } module b() { translate([g.p, 0, 0]) cube(1); } }\na.b();");
    assert!(has_warning(&out, "unbekannte Variable `g`"), "{:?}", out.diagnostics);
    // unknown parts of a path
    assert!(has_error(&run(&format!("{NS}parts.bolt();")), "Modul `parts` hat kein Untermodul `bolt`"));
    assert!(has_error(&run("nothing.bolt();"), "unbekanntes Modul `nothing`"));
    assert!(has_warning(&run(&format!("{NS}echo(parts.missing);")), "hat keine Funktion oder Variable"));
    // a variable shadows the module name: plain member access (`v.x`)
    let out = clean("module v { function f() = 1; }\nv = [3, 4];\necho(v.x);");
    assert_eq!(echoes(&out), ["3"]);
}

#[test]
fn qualified_context_module() {
    let src = format!("{NS}parts.screw();");
    for name in ["parts.screw", "parts::screw", "screw"] {
        let out = run_module(&src, name, vec![]);
        assert!(errors(&out).is_empty(), "{name}: {:?}", errors(&out));
        assert_eq!(out.context.path, ["parts", "screw"], "{name}");
        let h = out.tree.nodes.iter().find_map(|n| match &n.op {
            Op::Prim { prim: cad_ir::Prim::Cylinder { h, .. }, .. } => Some(*h),
            _ => None,
        });
        assert_eq!(h, Some(20.0));
    }
    let out = run_module(&src, "parts.nut.m3", vec![]);
    assert_eq!(out.context.path, ["parts", "nut", "m3"]);
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

#[test]
fn arguments_of_2d_operations() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let op = |src: &str| -> Op {
        let out = clean(src);
        out.tree
            .nodes
            .iter()
            .find(|n| matches!(n.op, Op::Offset { .. } | Op::Stroke { .. } | Op::Blend2 { .. }))
            .unwrap()
            .op
            .clone()
    };
    assert_eq!(op("offset(2) square(5);"), Op::Offset { round: true, d: 2.0, chamfer: false });
    assert_eq!(
        op("offset(delta = -1, chamfer = true) square(5);"),
        Op::Offset { round: false, d: -1.0, chamfer: true }
    );
    // OpenSCAD: without arguments r = 1
    assert_eq!(op("offset() square(5);"), Op::Offset { round: true, d: 1.0, chamfer: false });
    assert!(has_warning(&run("offset(r = 1, delta = 2) square(5);"), "`delta` wird ignoriert"));
    use cad_ir::{BlendKind, Select, StrokeCap, StrokeJoin};
    assert_eq!(
        op(
            "stroke(2, cap = \"square\", join = \"miter\") sketch(closed = false) { start([0, 0]); line(to = [5, 0]); }"
        ),
        Op::Stroke { w: 2.0, cap: StrokeCap::Square, join: StrokeJoin::Miter }
    );
    assert!(has_warning(&run("stroke(2, cap = \"butt\") circle(3);"), "`cap` muss"));
    assert!(has_error(&run("stroke() circle(3);"), "braucht eine Breite"));
    assert_eq!(
        op("chamfer2d(1.5) square(5);"),
        Op::Blend2 { kind: BlendKind::Chamfer, r: 1.5, select: Select::Inside }
    );
    // zones: children after the first; zone() is valid in fillet2d
    let out = clean(
        "grid g { x: a = 0; y: b = 0; z: c = 0; }\nfillet2d(1, select = \"touch\") { square(5); circle(1); zone(g[a, *, *], d = 1); }",
    );
    let n = out.tree.nodes.iter().position(|n| matches!(n.op, Op::Blend2 { .. })).unwrap();
    let kids = &out.tree.nodes[n].children;
    assert_eq!(kids.len(), 3);
    assert!(!out.tree.nodes[kids[0] as usize].flags.zone);
    assert!(kids[1..].iter().all(|&k| out.tree.nodes[k as usize].flags.zone));
    assert_eq!(out.tree.dim(n as u32), Some(cad_ir::Dim::Two));
    assert!(has_error(&run("fillet2d() square(5);"), "braucht einen Radius"));
}

#[test]
fn errors_in_english() {
    let src = "sketch {\n start([0, 0]);\n line(to = [10, 0]);\n line(to = [10, 10]);\n}";
    let e = errors_at(&run(src), src);
    assert!(e.iter().any(|(l, m)| *l == 4 && m.starts_with("contour not closed")), "{e:?}");
    let src = "sketch {\n start([0, 0]);\n arc(to = [10, 0], r = 4);\n close();\n}";
    let e = errors_at(&run(src), src);
    assert!(e.iter().any(|(l, m)| *l == 3 && m.contains("smaller than half the chord")), "{e:?}");
    let src =
        "sketch {\n start([0, 0]);\n line(to = [10, 10]);\n line(to = [10, 0]);\n line(to = [0, 10]);\n close();\n}";
    let e = errors_at(&run(src), src);
    assert!(e.iter().any(|(l, m)| *l == 5 && m.contains("self-intersection") && m.contains("line 3")), "{e:?}");
}
