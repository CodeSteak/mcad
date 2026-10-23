//! Session over the native SMLib kernel: protocol flow end to end.

use std::collections::BTreeMap;

use cad_session::protocol::*;
use cad_session::{Session, SessionOptions, std_clock};
use kernel_smlib::SmlibKernel;

const GEHAEUSE: &str = r#"module gehaeuse(w = 40, d = 30, h = 20, t = 3) {
    grid g {
        x: links = 0, innen_l = t, innen_r = w - t, rechts = w;
        y: vorne = 0, hinten = d;
        z: boden = 0, deckel = h;
    }

    difference() {
        box(g[links, vorne, boden], g[rechts, hinten, deckel]);
        box(g[innen_l, vorne, t], g[innen_r, hinten, deckel]);
    }

    anchor ecke = g[rechts, hinten, deckel];
}

module deckel() cube([10, 10, 2]);

k: gehaeuse(w = 60);
translate(k.ecke) deckel();
"#;

fn session() -> Session<SmlibKernel> {
    Session::new(SmlibKernel::new(), SessionOptions::default(), std_clock())
}

fn scenes(msgs: &[FromEngine]) -> Vec<&Scene> {
    msgs.iter()
        .filter_map(|m| match m {
            FromEngine::Scene(s) => Some(s.as_ref()),
            _ => None,
        })
        .collect()
}

fn meshes(msgs: &[FromEngine]) -> Vec<&MeshData> {
    msgs.iter()
        .filter_map(|m| match m {
            FromEngine::Meshes { items, .. } => Some(items.iter()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn edits(msgs: &[FromEngine]) -> Vec<Edit> {
    msgs.iter()
        .filter_map(|m| match m {
            FromEngine::Edits { edits, .. } => Some(edits.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn diagnostics(msgs: &[FromEngine]) -> Vec<Diagnostic> {
    msgs.iter()
        .filter_map(|m| match m {
            FromEngine::Diagnostics { items, .. } => Some(items.clone()),
            _ => None,
        })
        .next_back()
        .unwrap_or_default()
}

/// Applies UTF-16 edits to a string.
fn apply(src: &str, edits: &[Edit]) -> String {
    let mut u: Vec<u16> = src.encode_utf16().collect();
    let mut e = edits.to_vec();
    e.sort_by_key(|e| std::cmp::Reverse(e.from));
    for ed in e {
        u.splice(ed.from as usize..ed.to as usize, ed.text.encode_utf16());
    }
    String::from_utf16(&u).unwrap()
}

fn set_source(s: &mut Session<SmlibKernel>, text: &str, version: u64) -> Vec<FromEngine> {
    let mut msgs = s.post(ToEngine::SetSource { text: text.into(), version });
    msgs.extend(s.run_to_idle());
    msgs
}

#[test]
fn gehaeuse_flow() {
    let mut s = session();
    let first = s.post(ToEngine::SetSource { text: GEHAEUSE.into(), version: 1 });
    // Diagnostics, preliminary scene, progress
    assert!(matches!(first[0], FromEngine::Diagnostics { version: 1, ref items } if items.is_empty()), "{first:?}");
    let pre = scenes(&first);
    assert_eq!(pre.len(), 1);
    assert!(!pre[0].complete);
    assert!(pre[0].nodes.iter().all(|n| n.status == NodeStatus::Pending || n.status == NodeStatus::Skipped));
    assert!(pre[0].nodes.iter().all(|n| n.mesh_key.is_some()));
    assert!(matches!(first.last(), Some(FromEngine::Progress { done: 0, .. })));
    assert!(s.busy());

    let rest = s.run_to_idle();
    let sc = scenes(&rest);
    assert_eq!(sc.len(), 1);
    let scene = sc[0];
    assert!(scene.complete);
    assert_eq!(scene.version, 1);
    // same keys as announced in the preliminary scene
    for (a, b) in pre[0].nodes.iter().zip(&scene.nodes) {
        assert_eq!(a.mesh_key, b.mesh_key);
        assert_eq!(a.tag, b.tag);
    }
    assert!(scene.nodes.iter().all(|n| n.status == NodeStatus::Ok), "{:?}", scene.nodes);
    assert_eq!(scene.grids.len(), 1);
    assert_eq!(scene.grids[0].name, "g");
    // anchor ecke of k in world coordinates
    let ecke = scene.anchors.iter().find(|a| a.name == "ecke").expect("anchor");
    assert_eq!(ecke.text.as_deref(), Some("k.ecke"));
    assert_eq!(ecke.point, Vec3([60.0, 30.0, 20.0]));
    // lines of nodes: the translate is on line 19
    let tr = scene.nodes.iter().find(|n| n.label == "translate").unwrap();
    assert_eq!(tr.range.line, 19);
    assert_eq!(tr.range.col, 1);
    // root mesh sent automatically
    let m = meshes(&rest);
    assert_eq!(m.len(), 1);
    assert_eq!(Some(m[0].node), scene.root);
    assert!(!m[0].coarse);
    assert!(m[0].indices.len() / 3 > 10);
    assert_eq!(m[0].tri_tags.len(), m[0].indices.len() / 3);
    // every triangle tag maps to a node
    for t in &m[0].tri_tags {
        assert!(scene.tag_nodes.iter().any(|tn| tn.tag == *t && !tn.nodes.is_empty()), "tag {t}");
    }

    // request the subtracted box mesh
    let sub = scene.nodes.iter().find(|n| n.subtracted).expect("subtracted node");
    assert!(s.post(ToEngine::RequestMeshes { nodes: vec![sub.idx], coarse: None, version: Some(1) }).is_empty());
    let r = s.run_to_idle();
    let m = meshes(&r);
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].node, sub.idx);
    assert_eq!(Some(&m[0].key), sub.mesh_key.as_ref());
    // stale version: dropped
    assert!(s.post(ToEngine::RequestMeshes { nodes: vec![sub.idx], coarse: None, version: Some(99) }).is_empty());
    assert!(!s.busy());

    // drag the translate target (k.ecke) freely: the reference becomes numbers
    let h = scene.handles.iter().find(|h| h.kind == HandleKind::Point && h.range.line == 19).expect("translate handle");
    assert!(h.has_ref);
    assert_eq!(h.world_pos, Vec3([60.0, 30.0, 20.0]));
    let st = s.post(ToEngine::DragStart { handle: h.id });
    let FromEngine::SnapTargets { targets, .. } = &st[0] else { panic!("{st:?}") };
    assert!(targets.iter().any(|t| t.text == "k.ecke"), "{targets:?}");
    let e =
        edits(&s.post(ToEngine::Drag { handle: h.id, target: DragTarget::Point { pos: Vec3([12.2, 30.0, 20.0]) } }));
    assert_eq!(e.len(), 1, "{e:?}");
    let v2 = apply(GEHAEUSE, &e);
    assert!(v2.contains("translate([12, 30, 20]) deckel();"), "{v2}");
    // during the drag: coarse root mesh
    let r = set_source(&mut s, &v2, 2);
    let m = meshes(&r);
    assert_eq!(m.len(), 1);
    assert!(m[0].coarse);
    let sc2 = scenes(&r);
    let last = sc2.last().unwrap();
    assert!(last.stats.cache_hits > 0, "{:?}", last.stats);
    // drag end without a change: fine root mesh again
    assert!(s.post(ToEngine::DragEnd { handle: h.id, target: None }).is_empty());
    let r = s.run_to_idle();
    let m = meshes(&r);
    assert_eq!(m.len(), 1);
    assert!(!m[0].coarse);
    assert!(m[0].tol < 0.1);
}

#[test]
fn snap_writes_reference() {
    let mut s = session();
    s.post(ToEngine::SetContext { module: Some("gehaeuse".into()), overrides: BTreeMap::new() });
    let r = set_source(&mut s, GEHAEUSE, 1);
    let scene = *scenes(&r).last().unwrap();
    assert_eq!(scene.context.module.as_deref(), Some("gehaeuse"));
    assert_eq!(scene.context.path, vec!["gehaeuse".to_string()]);
    // second corner of the inner box: g[innen_r, hinten, deckel]
    let h = scene
        .handles
        .iter()
        .filter(|h| h.kind == HandleKind::Point && h.has_ref)
        .find(|h| h.range.line == 10 && h.index == 1)
        .expect("inner box corner handle");
    let t = s.post(ToEngine::SnapTargets { handle: h.id });
    let FromEngine::SnapTargets { targets, .. } = &t[0] else { panic!("{t:?}") };
    let target = targets.iter().find(|t| t.text == "g[rechts, hinten, deckel]").expect("snap target");
    let e = edits(
        &s.post(ToEngine::DragEnd { handle: h.id, target: Some(DragTarget::Snap { snap: target.snap.clone() }) }),
    );
    assert_eq!(e.len(), 1);
    let v2 = apply(GEHAEUSE, &e);
    assert!(v2.contains("box(g[innen_l, vorne, t], g[rechts, hinten, deckel]);"), "{v2}");
}

#[test]
fn context_overrides_and_default() {
    let mut s = session();
    let mut ov = BTreeMap::new();
    ov.insert("w".to_string(), Value::Num(80.0));
    s.post(ToEngine::SetContext { module: Some("gehaeuse".into()), overrides: ov });
    let r = set_source(&mut s, GEHAEUSE, 3);
    let scene = *scenes(&r).last().unwrap();
    let w = scene.context.params.iter().find(|p| p.name == "w").unwrap();
    assert_eq!(w.value, Value::Num(80.0));
    assert!(w.overridden);
    let root = scene.root.unwrap() as usize;
    let b = scene.nodes[root].bounds.as_ref().unwrap();
    assert!((b.max.0[0] - b.min.0[0] - 80.0).abs() < 1e-6, "{b:?}");
    // "Als Standard übernehmen"
    let e =
        edits(&s.post(ToEngine::SetDefault { module: "gehaeuse".into(), param: "w".into(), value: Value::Num(80.0) }));
    let v2 = apply(GEHAEUSE, &e);
    assert!(v2.starts_with("module gehaeuse(w = 80, d = 30"), "{v2}");
    // unknown parameter
    let r = s.post(ToEngine::SetDefault { module: "gehaeuse".into(), param: "zz".into(), value: Value::Num(1.0) });
    assert!(matches!(r[0], FromEngine::Error { .. }));
}

#[test]
fn syntax_error_keeps_scene_and_utf16_ranges() {
    let mut s = session();
    let src = "// Größe\ncube(10);\n";
    let r = set_source(&mut s, src, 1);
    assert_eq!(scenes(&r).len(), 2);
    let ok_scene = (*scenes(&r).last().unwrap()).clone();
    let cube = ok_scene.nodes.iter().find(|n| n.label == "box").unwrap();
    // "// Größe\n" is 9 UTF-16 units (10 bytes)
    assert_eq!(cube.range.from, 9);
    assert_eq!((cube.range.line, cube.range.col), (2, 1));

    let bad = "// Größe\ncube(10;\n";
    let r = set_source(&mut s, bad, 2);
    assert!(scenes(&r).is_empty());
    let d = diagnostics(&r);
    assert!(!d.is_empty());
    assert!(d.iter().all(|d| d.source == DiagSource::Syntax));
    assert_eq!(d[0].range.line, 2);
    // the last scene stays: meshes of version 1 can still be requested
    assert_eq!(s.done().unwrap().version, 1);
    s.post(ToEngine::RequestMeshes { nodes: vec![cube.idx], coarse: Some(false), version: Some(1) });
    assert_eq!(meshes(&s.run_to_idle()).len(), 1);
}

#[test]
fn new_source_aborts_running_job() {
    let mut s = session();
    let a = "difference() { cube(20); translate([10, 10, -1]) cylinder(h = 30, r = 5); }\n";
    let first = s.post(ToEngine::SetSource { text: a.into(), version: 1 });
    assert!(!scenes(&first).is_empty());
    // a tiny budget: still pending after one step
    let st = s.step(Some(0.0));
    assert!(matches!(st[0], FromEngine::Progress { .. }), "{st:?}");
    let b = "difference() { cube(20); translate([10, 10, -1]) cylinder(h = 30, r = 6); }\n";
    s.post(ToEngine::SetSource { text: b.into(), version: 2 });
    let r = s.run_to_idle();
    let sc = scenes(&r);
    assert_eq!(sc.len(), 1);
    assert_eq!(sc[0].version, 2);
    // cube(20) came from the cache of the aborted job or is recomputed; both fine
    assert!(sc[0].complete);
    // export
    s.post(ToEngine::Export { format: ExportFormat::Stl, parts: None, zip: None });
    let r = s.run_to_idle();
    let FromEngine::ExportResult { bytes, triangles, format, .. } = &r[0] else { panic!("{r:?}") };
    assert_eq!(*format, ExportFormat::Stl);
    assert_eq!(bytes.len(), 84 + 50 * *triangles as usize);
}

#[test]
fn kernel_diagnostics_and_cancel() {
    let mut s = session();
    // fillet radius too large: kernel error at the fillet node
    let src = "fillet(30) cube(10);\n";
    let r = set_source(&mut s, src, 1);
    let d = diagnostics(&r);
    let scene = *scenes(&r).last().unwrap();
    let k = d.iter().find(|d| d.source == DiagSource::Kernel).expect("kernel diagnostic");
    assert_eq!(k.severity, Severity::Error);
    assert_eq!(k.range.line, 1);
    let n = &scene.nodes[k.node.unwrap() as usize];
    assert_eq!(n.status, NodeStatus::Error);
    assert_eq!(n.label, "fillet");
    assert!(!n.messages.is_empty());
    s.post(ToEngine::SetSource { text: "cube(5);".into(), version: 2 });
    assert!(s.busy());
    assert!(s.post(ToEngine::Cancel).is_empty());
    assert!(!s.busy());
    assert_eq!(s.done().unwrap().version, 1);
}

#[test]
fn protocol_json_roundtrip() {
    let m: ToEngine =
        serde_json::from_str(r#"{"type":"SetContext","module":"a","overrides":{"w":60,"b":true,"v":[1,2,3]}}"#)
            .unwrap();
    let ToEngine::SetContext { module, overrides } = &m else { panic!() };
    assert_eq!(module.as_deref(), Some("a"));
    assert_eq!(overrides["v"], Value::List(vec![Value::Num(1.0), Value::Num(2.0), Value::Num(3.0)]));
    let m: ToEngine = serde_json::from_str(r#"{"type":"RequestMeshes","nodes":[1,2]}"#).unwrap();
    assert_eq!(m, ToEngine::RequestMeshes { nodes: vec![1, 2], coarse: None, version: None });
    let m: ToEngine = serde_json::from_str(
        r#"{"type":"Drag","handle":5,"target":{"kind":"snap","snap":{"kind":"anchor","inst":1,"name":"ecke"}}}"#,
    )
    .unwrap();
    assert!(matches!(m, ToEngine::Drag { target: DragTarget::Snap { .. }, .. }));
    let m: ToEngine = serde_json::from_str(r#"{"type":"Export","format":"3mf"}"#).unwrap();
    assert_eq!(m, ToEngine::Export { format: ExportFormat::ThreeMf, parts: None, zip: None });
    let j = serde_json::to_string(&FromEngine::Progress { version: 1, done: 2, total: 3, current: None }).unwrap();
    assert_eq!(j, r#"{"type":"Progress","version":1,"done":2,"total":3,"current":null}"#);
}

const PLANES: &str = "grid g {
    z: boden = 0, deckel = 20;
}
cylinder(h = 25, r = 3);
translate([1, 0, 0] * 2) cube(2);
";

#[test]
fn drag_always_answers_and_plane_snap() {
    let mut s = session();
    let r = set_source(&mut s, PLANES, 1);
    let scene = *scenes(&r).last().unwrap();
    // Computed translate target: locked handle, not in `handles`.
    let locked = scene.locked_handles.iter().find(|h| h.range.line == 5).expect("locked translate handle");
    assert!(locked.locked);
    assert_eq!(locked.world_pos, Vec3([2.0, 0.0, 0.0]));
    assert!(!scene.handles.iter().any(|h| h.id == locked.id));
    let h = scene.handles.iter().find(|h| h.kind == HandleKind::Height).expect("height handle");
    assert!(!h.locked);
    let st = s.post(ToEngine::DragStart { handle: h.id });
    let FromEngine::SnapTargets { targets, .. } = &st[0] else { panic!("{st:?}") };
    let texts: Vec<&str> = targets.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(texts, ["g.boden", "g.deckel"]);
    assert_eq!(targets[1].result_world, Vec3([0.0, 0.0, 20.0]));
    // A drag that does not change the text still answers (empty edit list).
    let same = s.post(ToEngine::Drag { handle: h.id, target: DragTarget::Value { value: 25.2 } });
    assert!(matches!(&same[..], [FromEngine::Edits { version: 1, edits }] if edits.is_empty()), "{same:?}");
    // Snap writes `g.deckel`.
    let e = edits(&s.post(ToEngine::Drag { handle: h.id, target: DragTarget::Snap { snap: targets[1].snap.clone() } }));
    let v2 = apply(PLANES, &e);
    assert!(v2.contains("cylinder(h = g.deckel, r = 3);"), "{v2}");
    let r = set_source(&mut s, &v2, 2);
    assert!(diagnostics(&r).is_empty(), "{:?}", diagnostics(&r));
    // Drag options: 0.5 mm steps.
    assert!(s.post(ToEngine::SetDragOptions { step_mm: 0.5, step_deg: 5.0 }).is_empty());
    let e = edits(&s.post(ToEngine::Drag { handle: h.id, target: DragTarget::Value { value: 17.3 } }));
    assert!(apply(&v2, &e).contains("cylinder(h = 17.5, r = 3);"), "{e:?}");
    // Invalid values are ignored.
    s.post(ToEngine::SetDragOptions { step_mm: f64::NAN, step_deg: -1.0 });
    assert_eq!(s.options().drag.step_mm, 0.5);
    assert_eq!(s.options().drag.step_deg, 5.0);
    assert!(s.post(ToEngine::DragEnd { handle: h.id, target: None }).is_empty());
}
