//! Sketch editor (Plan 17.12): placement of new statements, snapping, code of the tools,
//! corners, deleting segments, the plane of sketches under transformations.

use cad_eval::sketch_edit::*;
use cad_eval::*;
use glam::{DMat4, DVec2, DVec3};

fn eval(src: &str) -> (Project, EvalOutput) {
    eval_files("main.scad", src, &NoFiles, &EvalOptions::default())
}

/// Byte offset of a 1-based line and column.
fn at(src: &str, line: usize, col: usize) -> u32 {
    let start: usize = src.split_inclusive('\n').take(line - 1).map(|l| l.len()).sum();
    (start + col) as u32
}

fn no_errors(src: &str) {
    let (_, out) = eval(src);
    let errs: Vec<_> = out.diagnostics.iter().filter(|d| d.severity == cad_syntax::Severity::Error).collect();
    assert!(errs.is_empty(), "Fehler in\n{src}\n{errs:?}");
}

/// Drives a tool like the UI: each point is snapped (tolerance `tol`) and clicked; a completed
/// action is applied, the source re-evaluated and the cursor put where the answer says.
/// Returns the final source and the messages.
fn draw(src: &str, cursor: u32, tool: SketchTool, pts: &[[f64; 2]], tol: f64, free: bool) -> (String, Vec<String>) {
    let mut text = src.to_string();
    let mut cursor = cursor;
    let mut picks: Vec<Pick> = Vec::new();
    let mut msgs = Vec::new();
    for q in pts {
        let (proj, out) = eval(&text);
        let place = place(&out, &proj, cursor);
        let o = snap_opts(&place, tool, &picks, tol, 1.0, free);
        let s = snap(&out, &place, DVec2::from_array(*q), &o);
        let mut all = picks.clone();
        all.push(s.pick);
        match click(&out, &proj, &place, tool, &all, 1.0, 15.0, None) {
            Ok(Click::More(p)) => picks = p,
            Ok(Click::Done(a)) => {
                let (i, off) = a.select.unwrap();
                let e = &a.edits[i];
                let new_cursor = e.span.start as usize + off;
                text = apply_edits(&text, &a.edits);
                cursor = new_cursor as u32;
                picks = a.points;
            }
            Err(m) => msgs.push(m),
        }
    }
    (text, msgs)
}

const BASE: &str =
    "linear_extrude(5) sketch {\n    start([0, 0]);\n    line(to = [40, 0]);\n    line(to = [40, 20]);\n}\n";

#[test]
fn new_sketch_at_the_end_of_the_file() {
    let src = "cube(5);\n";
    let (t, m) = draw(src, 0, SketchTool::Line, &[[0.2, -0.1], [10.3, 0.4], [10.0, 7.8]], 0.5, false);
    assert!(m.is_empty(), "{m:?}");
    assert_eq!(t, "cube(5);\n\nsketch {\n    start([0, 0]);\n    line(to = [10, 0]);\n    line(to = [10, 8]);\n}\n");
}

#[test]
fn line_continues_the_contour_at_the_cursor() {
    // cursor on line 4: the next line starts at [40, 20]
    let (t, _) = draw(BASE, at(BASE, 4, 6), SketchTool::Line, &[[40.1, 19.9], [0.2, 20.3]], 0.5, false);
    assert_eq!(
        t,
        "linear_extrude(5) sketch {\n    start([0, 0]);\n    line(to = [40, 0]);\n    line(to = [40, 20]);\n    line(to = [0, 20]);\n}\n"
    );
    // clicking the contour's start closes it
    let (t2, _) = draw(&t, at(&t, 5, 6), SketchTool::Line, &[[0.0, 20.0], [0.05, 0.1]], 0.5, false);
    assert!(t2.contains("    line(to = [0, 20]);\n    close();\n}"), "{t2}");
    no_errors(&t2);
}

#[test]
fn line_between_statements_and_new_contour() {
    // cursor on line 3 (after the first line): inserted between, continues from [40, 0]
    let (t, _) = draw(BASE, at(BASE, 3, 0), SketchTool::Line, &[[40.0, 0.0], [50.0, 10.0]], 0.5, false);
    assert!(t.contains("    line(to = [40, 0]);\n    line(to = [50, 10]);\n    line(to = [40, 20]);"), "{t}");
    // a line elsewhere starts a new contour: after the end of the contour that goes on
    let (t, _) = draw(BASE, at(BASE, 3, 0), SketchTool::Line, &[[60.0, 0.0], [70.0, 0.0]], 0.5, false);
    assert!(t.contains("    line(to = [40, 20]);\n    start([60, 0]);\n    line(to = [70, 0]);\n}"), "{t}");
}

#[test]
fn grid_points_write_references_and_rectangles_combine_them() {
    let src = "grid g {\n    x: a = 0, b = 10, c = 30;\n    y: u = 0, v = 15;\n}\nlinear_extrude(2) sketch {\n}\n";
    let (t, m) = draw(src, at(src, 5, 3), SketchTool::Rect, &[[10.2, 0.3], [29.6, 15.4]], 1.0, false);
    assert!(m.is_empty(), "{m:?}");
    assert!(
        t.contains(
            "linear_extrude(2) sketch {\n    start(g[b, u]);\n    line(to = g[c, u]);\n    line(to = g[c, v]);\n    line(to = g[b, v]);\n    close();\n}"
        ),
        "{t}"
    );
    no_errors(&t);
    // Alt: free with step, no reference
    let (t, _) = draw(src, at(src, 5, 3), SketchTool::Rect, &[[10.2, 0.3], [29.6, 15.4]], 1.0, true);
    assert!(t.contains("start([10, 0]);") && t.contains("line(to = [30, 15]);"), "{t}");
}

#[test]
fn snaps_by_kind() {
    let src = "grid g {\n    x: a = 0, b = 10;\n    y: u = 0;\n}\nsketch(closed = false) {\n    start([0, 5]);\n    line(to = [20, 5]);\n    arc(to = [20, 25], r = 10);\n}\n";
    let (proj, out) = eval(src);
    let place = place(&out, &proj, at(src, 8, 4));
    let o = |from: Option<DVec2>| SnapOpts { tol: 0.6, step: 1.0, free: false, from, tangent: None, vertices: false };
    let s = |x: f64, y: f64, from| snap(&out, &place, DVec2::new(x, y), &o(from));
    assert_eq!(s(10.2, 0.1, None).pick.text, "g[b, u]");
    assert_eq!(s(10.2, 0.1, None).pick.kind, SnapKind::Grid);
    // end point and midpoint of the line, centre and middle of the arc
    assert_eq!(s(19.8, 5.2, None).pick.kind, SnapKind::End);
    assert_eq!(s(10.0, 5.3, None).pick.kind, SnapKind::Mid);
    assert_eq!(s(20.3, 15.2, None).pick.kind, SnapKind::Center);
    let mid = s(29.9, 15.1, None).pick;
    assert_eq!(mid.kind, SnapKind::Mid);
    assert_eq!(mid.text, "[30, 15]");
    // intersection of the arc with the vertical line? none; free positions: step
    let free = s(13.3, 40.4, None).pick;
    assert_eq!((free.kind, free.text.as_str()), (SnapKind::Step, "[13, 40]"));
    // horizontal to the previous point, grid line x = g.b
    let h = s(33.6, 25.3, Some(DVec2::new(20.0, 25.0))).pick;
    assert_eq!((h.kind, h.text.as_str()), (SnapKind::Horizontal, "[34, 25]"));
    let g = s(10.4, 40.2, None).pick;
    assert_eq!((g.kind, g.text.as_str()), (SnapKind::GridLine, "[g.b, 40]"));
    // the grid line value is a number in a point
    let t = format!("{}sketch {{ start([0, 0]); line(to = {}); line(to = [0, 40]); close(); }}\n", src, g.text);
    no_errors(&t);
}

#[test]
fn intersections_are_snapped() {
    let src =
        "sketch {\n    start([0, 0]);\n    line(to = [20, 20]);\n    line(to = [20, 0]);\n    line(to = [0, 20]);\n}\n";
    let (proj, out) = eval(src);
    let place = place(&out, &proj, at(src, 5, 4));
    let o = SnapOpts { tol: 1.0, step: 1.0, free: false, from: None, tangent: None, vertices: false };
    let s = snap(&out, &place, DVec2::new(10.3, 9.6), &o).pick;
    assert_eq!(s.kind, SnapKind::Intersection);
    assert_eq!(s.text, "[10, 10]");
}

#[test]
fn tangent_line_writes_length() {
    let src = "sketch(closed = false) {\n    start([0, 0]);\n    line(to = [10, 0]);\n    arc(to = [20, 10]);\n}\n";
    // the arc ends going up: a line along +y
    let (t, _) = draw(src, at(src, 4, 4), SketchTool::Line, &[[20.0, 10.0], [20.2, 17.4]], 0.5, false);
    assert!(t.contains("    arc(to = [20, 10]);\n    line(length = 7);\n}"), "{t}");
    no_errors(&t);
}

#[test]
fn arcs_bezier_circle() {
    let open = "sketch(closed = false) {\n    start([0, 0]);\n    line(to = [10, 0]);\n}\n";
    let c = at(open, 3, 4);
    let (t, m) = draw(open, c, SketchTool::ArcThrough, &[[10.0, 0.0], [15.0, 5.0], [10.0, 10.0]], 0.3, false);
    assert!(m.is_empty(), "{m:?}");
    assert!(t.contains("    line(to = [10, 0]);\n    arc(through = [15, 5], to = [10, 10]);\n}"), "{t}");
    no_errors(&t);
    // centre + angle: 90° counter-clockwise around [10, 5] from [10, 0]
    let (t, _) = draw(open, c, SketchTool::ArcCenter, &[[10.0, 0.0], [10.0, 5.0], [16.0, 5.3]], 0.3, false);
    assert!(t.contains("    arc(center = [10, 5], angle = 90);\n}"), "{t}");
    no_errors(&t);
    // tangent arc: start implied (end of the contour)
    let (t, _) = draw(open, c, SketchTool::ArcTangent, &[[20.0, 10.0]], 0.3, false);
    assert!(t.contains("    arc(to = [20, 10]);\n}"), "{t}");
    no_errors(&t);
    let (t, _) = draw(open, c, SketchTool::Bezier, &[[10.0, 0.0], [14.0, 0.0], [16.0, 6.0], [16.0, 10.0]], 0.3, false);
    assert!(t.contains("    bezier(c1 = [14, 0], c2 = [16, 6], to = [16, 10]);\n}"), "{t}");
    no_errors(&t);
    let (t, _) = draw(open, c, SketchTool::Circle, &[[30.0, 0.0], [34.0, 0.0]], 0.3, false);
    assert!(t.contains("    start([34, 0]);\n    arc(center = [30, 0], angle = 360);\n    close();\n}"), "{t}");
    no_errors(&t);
}

#[test]
fn center_arc_to_a_snapped_point_keeps_the_reference() {
    let src =
        "grid g {\n    x: a = 0, b = 10;\n    y: u = 0, v = 10;\n}\nsketch(closed = false) {\n    start(g[b, u]);\n}\n";
    let (t, _) = draw(src, at(src, 6, 4), SketchTool::ArcCenter, &[[10.0, 0.0], [0.0, 0.0], [0.1, 9.8]], 0.5, false);
    assert!(t.contains("    start(g[b, u]);\n    arc(center = g[a, u], to = g[a, v]);\n}"), "{t}");
    no_errors(&t);
}

#[test]
fn corner_tool_inserts_and_updates() {
    let src = "linear_extrude(2) sketch {\n    start([0, 0]);\n    line(to = [30, 0]);\n    line(to = [30, 20]);\n    line(to = [0, 20]);\n    close();\n}\n";
    // vertex [30, 0], radius 4 (cursor 4 mm away)
    let (t, m) = draw(src, at(src, 2, 4), SketchTool::Corner, &[[29.6, 0.3], [26.0, 0.0]], 1.0, false);
    assert!(m.is_empty(), "{m:?}");
    assert!(t.contains("    line(to = [30, 0]);\n    corner(r = 4);\n    line(to = [30, 20]);"), "{t}");
    no_errors(&t);
    // the same vertex again: the radius changes
    let (t2, _) = draw(&t, 0, SketchTool::Corner, &[[30.0, 0.0], [24.0, 0.0]], 1.0, false);
    assert!(t2.contains("corner(r = 6);") && !t2.contains("corner(r = 4)"), "{t2}");
    // the start vertex of the closed contour: after start()
    let (t3, _) = draw(src, 0, SketchTool::Corner, &[[0.0, 0.0], [0.0, 3.0]], 1.0, false);
    assert!(t3.contains("    start([0, 0]);\n    corner(r = 3);\n    line(to = [30, 0]);"), "{t3}");
    no_errors(&t3);
}

#[test]
fn close_and_delete() {
    let src = "linear_extrude(2) sketch {\n    start([0, 0]);\n    line(to = [30, 0]);\n    corner(r = 2);\n    line(to = [30, 20]);\n    line(to = [0, 20]);\n    start([50, 0]);\n    line(to = [60, 0]);\n}\n";
    let (proj, out) = eval(src);
    let p = place(&out, &proj, at(src, 6, 4));
    let a = close(&p).unwrap();
    let t = apply_edits(src, &a.edits);
    assert!(t.contains("    line(to = [0, 20]);\n    close();\n    start([50, 0]);"), "{t}");
    // delete the line of line 3: its corner() goes with it
    let p = place(&out, &proj, at(src, 3, 6));
    let e = delete_segment(&proj, &p, at(src, 3, 6)).unwrap();
    let t = apply_edits(src, &e);
    assert_eq!(
        t,
        "linear_extrude(2) sketch {\n    start([0, 0]);\n    line(to = [30, 20]);\n    line(to = [0, 20]);\n    start([50, 0]);\n    line(to = [60, 0]);\n}\n"
    );
    // the only segment of a contour takes the contour along
    let p = place(&out, &proj, at(src, 8, 6));
    let e = delete_segment(&proj, &p, at(src, 8, 6)).unwrap();
    let t = apply_edits(src, &e);
    assert!(!t.contains("start([50, 0])") && !t.contains("[60, 0]") && t.contains("line(to = [0, 20]);\n}"), "{t}");
    // no segment on the line
    let p = place(&out, &proj, at(src, 2, 4));
    assert!(delete_segment(&proj, &p, at(src, 2, 4)).is_err());
}

#[test]
fn hole_blocks_and_empty_blocks() {
    let src = "linear_extrude(2) sketch {\n    start([0, 0]);\n    line(to = [30, 0]);\n    line(to = [30, 30]);\n    close();\n    hole {\n    }\n}\n";
    let (t, _) = draw(src, at(src, 7, 2), SketchTool::Circle, &[[15.0, 15.0], [20.0, 15.0]], 0.3, false);
    assert!(t.contains("    hole {\n        start([20, 15]);\n        arc(center = [15, 15], angle = 360);\n        close();\n    }\n}"), "{t}");
    no_errors(&t);
    // single-line empty block
    let src = "linear_extrude(2) sketch {}\n";
    let (t, _) = draw(src, at(src, 1, 24), SketchTool::Rect, &[[0.0, 0.0], [5.0, 5.0]], 0.3, false);
    assert!(t.starts_with("linear_extrude(2) sketch {\n    start([0, 0]);\n    line(to = [5, 0]);"), "{t}");
    assert!(t.ends_with("    close();\n}\n"), "{t}");
    no_errors(&t);
}

#[test]
fn new_sketch_in_the_context_module() {
    let src = "module teil(b = 20) {\n    cube([b, 5, 5]);\n}\nteil();\n";
    let opts =
        EvalOptions { context: Context::Module { name: "teil".into(), overrides: Vec::new() }, ..Default::default() };
    let (proj, out) = eval_files("main.scad", src, &NoFiles, &opts);
    let p = place(&out, &proj, at(src, 2, 4));
    assert!(p.info.is_none());
    let picks = [
        Pick { p: DVec2::ZERO, text: "[0, 0]".into(), kind: SnapKind::Step, grid: None },
        Pick { p: DVec2::new(4.0, 4.0), text: "[4, 4]".into(), kind: SnapKind::Step, grid: None },
    ];
    let Click::Done(a) = click(&out, &proj, &p, SketchTool::Rect, &picks, 1.0, 15.0, None).unwrap() else { panic!() };
    let t = apply_edits(src, &a.edits);
    assert!(
        t.starts_with("module teil(b = 20) {\n    cube([b, 5, 5]);\n    sketch {\n        start([0, 0]);\n"),
        "{t}"
    );
    assert!(t.contains("        close();\n    }\n}\nteil();"), "{t}");
}

#[test]
fn plane_of_sketches_under_transformations() {
    let src = "translate([0, 0, 5]) rotate([90, 0, 0]) linear_extrude(2) sketch {\n    start([0, 0]);\n    line(to = [10, 0]);\n    line(to = [0, 10]);\n    close();\n}\nrotate_extrude() sketch {\n    start([10, 0]);\n    line(to = [20, 0]);\n    line(to = [20, 5]);\n    close();\n}\nlinear_extrude(4, center = true) sketch { start([0, 0]); line(to = [1, 0]); line(to = [0, 1]); close(); }\n";
    let (proj, out) = eval(src);
    let p = place(&out, &proj, at(src, 2, 4));
    let m = p.frame;
    // local x → world x, local y → world z (rotate 90° about x), origin at z = 5
    assert!((m.transform_point3(DVec3::ZERO) - DVec3::new(0.0, 0.0, 5.0)).length() < 1e-9);
    assert!((m.transform_vector3(DVec3::Y) - DVec3::Z).length() < 1e-9);
    // rotate_extrude: the profile lies in the xz-plane
    let p = place(&out, &proj, at(src, 8, 4));
    assert!((p.frame.transform_point3(DVec3::new(20.0, 5.0, 0.0)) - DVec3::new(20.0, 0.0, 5.0)).length() < 1e-9);
    // centred linear_extrude: the bottom
    let p = place(&out, &proj, at(src, 13, 40));
    assert!((p.frame.transform_point3(DVec3::ZERO) - DVec3::new(0.0, 0.0, -2.0)).length() < 1e-9);
    assert_ne!(p.frame, DMat4::IDENTITY);
}

#[test]
fn anchors_are_snap_targets() {
    let src =
        "module holder() {\n    cube([10, 10, 2]);\n    anchor corner = [10, 10, 0];\n}\nk: holder();\nsketch {\n}\n";
    let (t, _) = draw(src, at(src, 6, 3), SketchTool::Line, &[[0.0, 0.0], [10.2, 9.9]], 0.5, false);
    assert!(t.contains("    start([0, 0]);\n    line(to = k.corner);"), "{t}");
}

#[test]
fn segment_under_the_mouse() {
    let (proj, out) = eval(BASE);
    let p = place(&out, &proj, at(BASE, 3, 4));
    let s = segment_at(&out, &p, DVec2::new(40.2, 10.0), 0.5).unwrap();
    assert_eq!(&BASE[s.start as usize..s.end as usize], "line(to = [40, 20]);");
    assert!(segment_at(&out, &p, DVec2::new(20.0, 10.0), 0.5).is_none());
}

#[test]
fn dragging_a_sketch_point_onto_a_grid_point_keeps_the_short_form() {
    let src = "grid g {\n    x: a = 0, b = 10, c = 30;\n    y: u = 0, v = 15;\n}\nsketch {\n    start(g[a, u]);\n    line(to = g[c, u]);\n    line(to = g[b, v]);\n    close();\n}\n";
    let (proj, out) = eval(src);
    let h = out.handles.iter().find(|h| h.kind == HandleKind::Point && h.value[..2] == [10.0, 15.0]).expect("Griff");
    let g = out.grid_by_name("g").unwrap().id;
    let r = SnapRef::Grid { grid: g, planes: ["c".into(), "v".into(), "_".into()] };
    let e = drag_edits(&out, &proj, h.id, &DragTarget::Snap(r), &DragOptions::default());
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].text, "g[c, v]");
}

#[test]
fn dragging_points_through_variables_and_indexed_planes() {
    // a point from a variable: the variable's literal changes (all uses move)
    let src = "p = [10, 5];\nsketch(closed = false) {\n    start([0, 0]);\n    line(to = p);\n}\n";
    let (proj, out) = eval(src);
    let h = out.handles.iter().find(|h| h.kind == HandleKind::Point && h.value[..2] == [10.0, 5.0]).expect("Griff");
    let to = DragTarget::Free(DragValue::Point(DVec3::new(12.0, 6.0, 0.0)));
    let e = drag_edits(&out, &proj, h.id, &to, &DragOptions::default());
    let t = apply_edits(src, &e);
    assert!(t.starts_with("p = [12, 6];"), "{t}");
    // an indexed plane: snapping writes the other index
    let src = "grid g {\n    x: hole = [0, 10, 20];\n    y: u = 0, v = 8;\n}\nsketch(closed = false) {\n    start(g[hole[0], u]);\n    line(to = g[hole[1], v]);\n}\n";
    let (proj, out) = eval(src);
    let h = out.handles.iter().find(|h| h.kind == HandleKind::Point && h.value[..2] == [10.0, 8.0]).expect("Griff");
    let g = out.grid_by_name("g").unwrap().id;
    let r = SnapRef::Grid { grid: g, planes: ["hole[2]".into(), "v".into(), "_".into()] };
    let e = drag_edits(&out, &proj, h.id, &DragTarget::Snap(r), &DragOptions::default());
    let t = apply_edits(src, &e);
    assert!(t.contains("    line(to = g[hole[2], v]);"), "{t}");
    no_errors(&t);
}

#[test]
fn prompts_and_snap_labels_follow_the_language() {
    let src = "sketch {\n}\n";
    let (proj, out) = eval(src);
    let place = place(&out, &proj, at(src, 1, 8));
    assert_eq!(prompt(&place, SketchTool::Line, &[]), "Line: start point");
    assert_eq!(SnapKind::Grid.label(), "grid point");
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    assert_eq!(prompt(&place, SketchTool::Line, &[]), "Linie: Anfangspunkt");
    assert_eq!(SnapKind::Grid.label(), "Rasterpunkt");
}
