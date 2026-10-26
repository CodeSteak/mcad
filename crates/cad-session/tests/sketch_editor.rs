//! Sketch editor over the protocol (Plan 17.12): mouse ray → snap in the sketch plane, preview,
//! clicks → edits, under transformations, delete, close, syntax errors.

use cad_session::protocol::*;
use cad_session::{Session, SessionOptions, std_clock};
use kernel_smlib::SmlibKernel;

fn session() -> Session<SmlibKernel> {
    Session::new(SmlibKernel::new(), SessionOptions::default(), std_clock())
}

fn answer(msgs: Vec<FromEngine>) -> SketchAnswer {
    msgs.into_iter()
        .find_map(|m| match m {
            FromEngine::Sketch(a) => Some(*a),
            _ => None,
        })
        .expect("Sketch-Antwort")
}

/// Applies protocol edits (ASCII sources: UTF-16 = bytes).
fn apply(text: &str, edits: &[Edit]) -> String {
    let mut e: Vec<&Edit> = edits.iter().collect();
    e.sort_by_key(|e| std::cmp::Reverse(e.from));
    let mut s = text.to_string();
    for x in e {
        s.replace_range(x.from as usize..x.to as usize, &x.text);
    }
    s
}

fn req(id: u32, cursor: u32, tool: SketchTool, points: Vec<SketchPoint>, ray: Option<Ray>, click: bool) -> ToEngine {
    ToEngine::Sketch(SketchRequest {
        id,
        cursor,
        tool,
        points,
        ray,
        pixel: 0.001,
        snap_px: 10.0,
        free: false,
        click,
        angle: None,
        info: false,
    })
}

/// Ray from above onto the xy-plane at (x, y): 100 mm away → snap radius 1 mm.
fn down(x: f64, y: f64) -> Option<Ray> {
    Some(Ray { origin: Vec3([x, y, 100.0]), dir: Vec3([0.0, 0.0, -1.0]) })
}

#[test]
fn draws_lines_into_a_new_sketch_and_continues() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    let mut text = String::from("cube(2);\n");
    s.post(ToEngine::SetSource { text: text.clone(), version: 1 });
    // entering: plane and raster, no sketch yet
    let a = answer(s.post(ToEngine::Sketch(SketchRequest {
        id: 1,
        cursor: 0,
        tool: SketchTool::Line,
        points: vec![],
        ray: None,
        pixel: 0.0,
        snap_px: 0.0,
        free: false,
        click: false,
        angle: None,
        info: true,
    })));
    assert_eq!(a.id, 1);
    assert!(a.sketch.is_none());
    assert_eq!(a.prompt, "Linie: Anfangspunkt");
    assert!(a.raster.is_some());
    // hover: the cube's corner is no snap target (not a sketch), the step rounds
    let a = answer(s.post(req(2, 0, SketchTool::Line, vec![], down(0.3, 0.2), false)));
    let sn = a.snap.unwrap();
    assert_eq!((sn.text.as_str(), sn.kind.as_str()), ("[0, 0]", "step"));
    // first click keeps the point, second completes the line
    let a = answer(s.post(req(3, 0, SketchTool::Line, vec![], down(0.3, 0.2), true)));
    assert!(a.edits.is_empty());
    assert_eq!(a.points.len(), 1);
    let a2 = answer(s.post(req(4, 0, SketchTool::Line, a.points.clone(), down(20.2, 0.1), false)));
    assert_eq!(a2.preview.len(), 1, "Vorschau der Linie");
    let a = answer(s.post(req(5, 0, SketchTool::Line, a.points, down(20.2, 0.1), true)));
    assert_eq!(a.edits.len(), 1);
    text = apply(&text, &a.edits);
    assert_eq!(text, "cube(2);\n\nsketch {\n    start([0, 0]);\n    line(to = [20, 0]);\n}\n");
    let [i, off] = a.select.unwrap();
    let cursor = a.edits[i as usize].from + off;
    assert_eq!(&text[..cursor as usize], "cube(2);\n\nsketch {\n    start([0, 0]);\n    line(to = [20, 0]);");
    // next click continues the chain (no new start)
    s.post(ToEngine::SetSource { text: text.clone(), version: 2 });
    let a = answer(s.post(req(6, cursor, SketchTool::Line, a.points, down(20.0, 10.3), true)));
    assert!(a.sketch.is_some());
    text = apply(&text, &a.edits);
    assert!(text.ends_with("    line(to = [20, 0]);\n    line(to = [20, 10]);\n}\n"), "{text}");
    // close at the cursor
    let [i, off] = a.select.unwrap();
    let cursor = a.edits[i as usize].from + off;
    s.post(ToEngine::SetSource { text: text.clone(), version: 3 });
    let a = answer(s.post(req(7, cursor, SketchTool::Close, vec![], None, false)));
    text = apply(&text, &a.edits);
    assert!(text.ends_with("    line(to = [20, 10]);\n    close();\n}\n"), "{text}");
    // delete the second line
    s.post(ToEngine::SetSource { text: text.clone(), version: 4 });
    let at = text.find("line(to = [20, 10])").unwrap() as u32;
    let a = answer(s.post(req(8, at, SketchTool::Delete, vec![], None, false)));
    assert_eq!(a.version, 4);
    let t = apply(&text, &a.edits);
    assert!(!t.contains("[20, 10]") && t.contains("line(to = [20, 0]);\n    close();"), "{t}");
}

#[test]
fn sketch_under_a_transformation_snaps_in_its_plane() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    let text = "grid g {\n    x: a = 0, b = 10;\n    y: u = 0, v = 10;\n}\ntranslate([0, 0, 5]) rotate([90, 0, 0]) linear_extrude(2) sketch {\n    start(g[a, u]);\n    line(to = g[b, u]);\n}\n";
    s.post(ToEngine::SetSource { text: text.into(), version: 1 });
    let cursor = text.find("line(to").unwrap() as u32;
    // the plane: local x → x, local y → z, at z = 5; looking along +y from y = -100
    let ray = Some(Ray { origin: Vec3([10.2, -100.0, 15.3]), dir: Vec3([0.0, 1.0, 0.0]) });
    let a = answer(s.post(req(1, cursor, SketchTool::Line, vec![], ray, false)));
    assert!(a.sketch.is_some());
    let sn = a.snap.unwrap();
    assert_eq!(sn.text, "g[b, v]");
    assert!((sn.point.0[2] - 15.0).abs() < 1e-9 && sn.point.0[1].abs() < 1e-9, "{:?}", sn.point);
    assert_eq!(a.current.unwrap().0, [10.0, 0.0, 5.0]);
    // a click from the contour end writes the reference
    let p0 = SketchPoint { p: [10.0, 0.0], text: "g[b, u]".into(), kind: "grid".into(), grid: None };
    let a = answer(s.post(req(2, cursor, SketchTool::Line, vec![p0], ray, true)));
    let t = apply(text, &a.edits);
    assert!(t.contains("    line(to = g[b, u]);\n    line(to = g[b, v]);\n}"), "{t}");
}

#[test]
fn syntax_errors_stop_drawing_and_select_finds_segments() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    let text = "sketch {\n    start([0, 0]);\n    line(to = [10, 0]);\n    line(to = [10, 10]);\n    close();\n}\n";
    s.post(ToEngine::SetSource { text: text.into(), version: 1 });
    let a = answer(s.post(req(1, 12, SketchTool::Select, vec![], down(10.3, 5.0), false)));
    let seg = a.segment.unwrap();
    assert_eq!(&text[seg.from as usize..seg.to as usize], "line(to = [10, 10]);");
    s.post(ToEngine::SetSource { text: "sketch { start([0, 0]) ".into(), version: 2 });
    let a = answer(s.post(req(2, 12, SketchTool::Line, vec![], down(1.0, 1.0), true)));
    assert!(a.message.unwrap().contains("Syntaxfehler"));
    assert!(a.edits.is_empty());
    {
        let _en = cad_i18n::scoped(cad_i18n::Lang::En);
        let a = answer(s.post(req(2, 12, SketchTool::Line, vec![], down(1.0, 1.0), true)));
        assert!(a.message.unwrap().starts_with("syntax error in the source"));
    }
}

#[test]
fn sketch_op_info_carries_the_plane() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    let text = "rotate_extrude() sketch {\n    start([10, 0]);\n    line(to = [20, 0]);\n    line(to = [20, 5]);\n    close();\n}\n";
    let msgs = s.post(ToEngine::SetSource { text: text.into(), version: 1 });
    let scene = msgs
        .iter()
        .find_map(|m| match m {
            FromEngine::Scene(sc) => Some(sc.clone()),
            _ => None,
        })
        .unwrap();
    let n = scene.nodes.iter().find(|n| matches!(n.op_info, Some(OpInfo::Sketch { .. }))).unwrap();
    let Some(OpInfo::Sketch { plane, .. }) = &n.op_info else { unreachable!() };
    // profile y → z
    assert_eq!([plane.0[8], plane.0[9], plane.0[10]], [0.0, -1.0, 0.0]);
    assert_eq!([plane.0[4], plane.0[5], plane.0[6]], [0.0, 0.0, 1.0]);
}
