//! Session over the native SMLib kernel: protocol flow end to end.

use std::collections::BTreeMap;

use cad_session::protocol::*;
use cad_session::{Session, SessionOptions, std_clock};
use kernel_smlib::SmlibKernel;

const HOUSING: &str = r#"module housing(w = 40, d = 30, h = 20, t = 3) {
    grid g {
        x: left = 0, inner_left = t, inner_right = w - t, right = w;
        y: front = 0, back = d;
        z: floor = 0, lid = h;
    }

    difference() {
        box(g[left, front, floor], g[right, back, lid]);
        box(g[inner_left, front, t], g[inner_right, back, lid]);
    }

    anchor corner = g[right, back, lid];
}

module lid() cube([10, 10, 2]);

k: housing(w = 60);
translate(k.corner) lid();
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
fn housing_flow() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    let first = s.post(ToEngine::SetSource { text: HOUSING.into(), version: 1 });
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
    // anchor corner of k in world coordinates
    let corner = scene.anchors.iter().find(|a| a.name == "corner").expect("anchor");
    assert_eq!(corner.text.as_deref(), Some("k.corner"));
    assert_eq!(corner.point, Vec3([60.0, 30.0, 20.0]));
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

    // drag the translate target (k.corner) freely: the reference becomes numbers
    let h = scene.handles.iter().find(|h| h.kind == HandleKind::Point && h.range.line == 19).expect("translate handle");
    assert!(h.has_ref);
    assert_eq!(h.world_pos, Vec3([60.0, 30.0, 20.0]));
    let st = s.post(ToEngine::DragStart { handle: h.id });
    let FromEngine::SnapTargets { targets, .. } = &st[0] else { panic!("{st:?}") };
    assert!(targets.iter().any(|t| t.text == "k.corner"), "{targets:?}");
    let e =
        edits(&s.post(ToEngine::Drag { handle: h.id, target: DragTarget::Point { pos: Vec3([12.2, 30.0, 20.0]) } }));
    assert_eq!(e.len(), 1, "{e:?}");
    let v2 = apply(HOUSING, &e);
    assert!(v2.contains("translate([12, 30, 20]) lid();"), "{v2}");
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
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    s.post(ToEngine::SetContext { module: Some("housing".into()), overrides: BTreeMap::new() });
    let r = set_source(&mut s, HOUSING, 1);
    let scene = *scenes(&r).last().unwrap();
    assert_eq!(scene.context.module.as_deref(), Some("housing"));
    assert_eq!(scene.context.path, vec!["housing".to_string()]);
    // second corner of the inner box: g[inner_right, back, lid]
    let h = scene
        .handles
        .iter()
        .filter(|h| h.kind == HandleKind::Point && h.has_ref)
        .find(|h| h.range.line == 10 && h.index == 1)
        .expect("inner box corner handle");
    let t = s.post(ToEngine::SnapTargets { handle: h.id });
    let FromEngine::SnapTargets { targets, .. } = &t[0] else { panic!("{t:?}") };
    let target = targets.iter().find(|t| t.text == "g[right, back, lid]").expect("snap target");
    let e = edits(
        &s.post(ToEngine::DragEnd { handle: h.id, target: Some(DragTarget::Snap { snap: target.snap.clone() }) }),
    );
    assert_eq!(e.len(), 1);
    let v2 = apply(HOUSING, &e);
    assert!(v2.contains("box(g[inner_left, front, t], g[right, back, lid]);"), "{v2}");
}

#[test]
fn context_overrides_and_default() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    let mut ov = BTreeMap::new();
    ov.insert("w".to_string(), Value::Num(80.0));
    s.post(ToEngine::SetContext { module: Some("housing".into()), overrides: ov });
    let r = set_source(&mut s, HOUSING, 3);
    let scene = *scenes(&r).last().unwrap();
    let w = scene.context.params.iter().find(|p| p.name == "w").unwrap();
    assert_eq!(w.value, Value::Num(80.0));
    assert!(w.overridden);
    let root = scene.root.unwrap() as usize;
    let b = scene.nodes[root].bounds.as_ref().unwrap();
    assert!((b.max.0[0] - b.min.0[0] - 80.0).abs() < 1e-6, "{b:?}");
    // "Use as default"
    let e =
        edits(&s.post(ToEngine::SetDefault { module: "housing".into(), param: "w".into(), value: Value::Num(80.0) }));
    let v2 = apply(HOUSING, &e);
    assert!(v2.starts_with("module housing(w = 80, d = 30"), "{v2}");
    // unknown parameter
    let r = s.post(ToEngine::SetDefault { module: "housing".into(), param: "zz".into(), value: Value::Num(1.0) });
    assert!(matches!(r[0], FromEngine::Error { .. }));
}

#[test]
fn syntax_error_keeps_scene_and_utf16_ranges() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let mut s = session();
    let src = "// naïve\ncube(10);\n";
    let r = set_source(&mut s, src, 1);
    assert_eq!(scenes(&r).len(), 2);
    let ok_scene = (*scenes(&r).last().unwrap()).clone();
    let cube = ok_scene.nodes.iter().find(|n| n.label == "box").unwrap();
    // "// naïve\n" is 9 UTF-16 units (10 bytes)
    assert_eq!(cube.range.from, 9);
    assert_eq!((cube.range.line, cube.range.col), (2, 1));

    let bad = "// naïve\ncube(10;\n";
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
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
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
    s.post(ToEngine::Export { format: ExportFormat::Stl, parts: None, zip: None, colors: None });
    let r = s.run_to_idle();
    let FromEngine::ExportResult { bytes, triangles, format, .. } = &r[0] else { panic!("{r:?}") };
    assert_eq!(*format, ExportFormat::Stl);
    assert_eq!(bytes.len(), 84 + 50 * *triangles as usize);
    // STEP: exact B-rep, no triangles
    s.post(ToEngine::Export { format: ExportFormat::Step, parts: None, zip: None, colors: None });
    let r = s.run_to_idle();
    let FromEngine::ExportResult { bytes, triangles, format, .. } = &r[0] else { panic!("{r:?}") };
    assert_eq!((*format, *triangles), (ExportFormat::Step, 0));
    let text = std::str::from_utf8(bytes).unwrap();
    assert!(text.starts_with("ISO-10303-21;") && text.contains("MANIFOLD_SOLID_BREP"), "{text}");
    let m: ToEngine = serde_json::from_str(r#"{"type":"Export","format":"step"}"#).unwrap();
    assert_eq!(m, ToEngine::Export { format: ExportFormat::Step, parts: None, zip: None, colors: None });
}

#[test]
fn kernel_diagnostics_and_cancel() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
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
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let m: ToEngine =
        serde_json::from_str(r#"{"type":"SetContext","module":"a","overrides":{"w":60,"b":true,"v":[1,2,3]}}"#)
            .unwrap();
    let ToEngine::SetContext { module, overrides } = &m else { panic!() };
    assert_eq!(module.as_deref(), Some("a"));
    assert_eq!(overrides["v"], Value::List(vec![Value::Num(1.0), Value::Num(2.0), Value::Num(3.0)]));
    let m: ToEngine = serde_json::from_str(r#"{"type":"RequestMeshes","nodes":[1,2]}"#).unwrap();
    assert_eq!(m, ToEngine::RequestMeshes { nodes: vec![1, 2], coarse: None, version: None });
    let m: ToEngine = serde_json::from_str(
        r#"{"type":"Drag","handle":5,"target":{"kind":"snap","snap":{"kind":"anchor","inst":1,"name":"corner"}}}"#,
    )
    .unwrap();
    assert!(matches!(m, ToEngine::Drag { target: DragTarget::Snap { .. }, .. }));
    let m: ToEngine = serde_json::from_str(r#"{"type":"Export","format":"3mf"}"#).unwrap();
    assert_eq!(m, ToEngine::Export { format: ExportFormat::ThreeMf, parts: None, zip: None, colors: None });
    let j = serde_json::to_string(&FromEngine::Progress { version: 1, done: 2, total: 3, current: None }).unwrap();
    assert_eq!(j, r#"{"type":"Progress","version":1,"done":2,"total":3,"current":null}"#);
}

const PLANES: &str = "grid g {
    z: floor = 0, lid = 20;
}
cylinder(h = 25, r = 3);
translate([1, 0, 0] * 2) cube(2);
";

#[test]
fn drag_always_answers_and_plane_snap() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
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
    assert_eq!(texts, ["g.floor", "g.lid"]);
    assert_eq!(targets[1].result_world, Vec3([0.0, 0.0, 20.0]));
    // A drag that does not change the text still answers (empty edit list).
    let same = s.post(ToEngine::Drag { handle: h.id, target: DragTarget::Value { value: 25.2 } });
    assert!(matches!(&same[..], [FromEngine::Edits { version: 1, edits }] if edits.is_empty()), "{same:?}");
    // Snap writes `g.lid`.
    let e = edits(&s.post(ToEngine::Drag { handle: h.id, target: DragTarget::Snap { snap: targets[1].snap.clone() } }));
    let v2 = apply(PLANES, &e);
    assert!(v2.contains("cylinder(h = g.lid, r = 3);"), "{v2}");
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

/// Texts follow the thread's language: a session that evaluated in German answers in English
/// after a switch (cached node results with texts are computed again) and in German again.
#[test]
fn language_switch_recomputes_cached_texts() {
    use cad_i18n::{Lang, scoped};
    let src =
        std::fs::read_to_string(format!("{}/../cad-cli/tests/data/fillet_zones.mcad", env!("CARGO_MANIFEST_DIR")))
            .unwrap();
    let mut s = session();
    let mut run = |v: u64| -> (Vec<String>, Stats) {
        let mut msgs = s.post(ToEngine::SetSource { text: src.clone(), version: v });
        msgs.extend(s.run_to_idle());
        let sc = scenes(&msgs).last().copied().cloned().unwrap();
        (sc.nodes.iter().flat_map(|n| n.hints.clone()).collect(), sc.stats)
    };
    let (de, _) = {
        let _g = scoped(Lang::De);
        run(1)
    };
    let (en, st) = {
        let _g = scoped(Lang::En);
        run(2)
    };
    let (de2, _) = {
        let _g = scoped(Lang::De);
        run(3)
    };
    assert!(de.iter().any(|h| h.ends_with(" Kanten")), "{de:?}");
    assert!(en.iter().any(|h| h.ends_with(" edges")), "{en:?}");
    assert!(!en.iter().any(|h| h.contains("Kanten")), "{en:?}");
    assert_eq!(de, de2);
    // computed again, not the German results from the cache
    assert!(st.kernel_calls > 0, "{st:?}");
    // the same language again: everything cached
    let _g = scoped(Lang::De);
    let (de3, st) = run(4);
    assert_eq!(de3, de);
    assert_eq!(st.kernel_calls, 0, "{st:?}");
}

#[test]
fn render_block_in_the_scene_and_set_render() {
    let mut s = session();
    let src = "color(\"red\") cube(10);\n";
    let r = set_source(&mut s, src, 1);
    assert!(scenes(&r).last().unwrap().render.is_none());
    // a new block at the end
    let e = edits(&s.post(ToEngine::SetRender {
        changes: vec![
            RenderChange::Set { key: "edges".into(), value: Value::Bool(false) },
            RenderChange::Light { name: "key".into(), key: "azimuth".into(), value: Value::Num(30.0) },
            RenderChange::Material {
                target: RenderTarget::Color { color: "red".into() },
                key: "preset".into(),
                value: Value::Str("chrome".into()),
            },
        ],
    }));
    let v2 = apply(src, &e);
    assert_eq!(
        v2,
        "color(\"red\") cube(10);\n\nrender {\n    edges = false;\n    light key { azimuth = 30; }\n    material \"red\" { preset = \"chrome\"; }\n}\n"
    );
    let r = set_source(&mut s, &v2, 2);
    let b = scenes(&r).last().unwrap().render.clone().expect("render block");
    assert_eq!((b.edges, b.range.line, b.lights[0].azimuth), (Some(false), 3, 30.0));
    assert_eq!(b.materials[0].target, RenderTarget::Color { color: "red".into() });
    assert_eq!(b.materials[0].preset.as_deref(), Some("chrome"));
    // a changed value in place; null removes
    let e = edits(&s.post(ToEngine::SetRender {
        changes: vec![
            RenderChange::Set { key: "edges".into(), value: Value::Undef },
            RenderChange::Light { name: "key".into(), key: "azimuth".into(), value: Value::Num(-15.5) },
        ],
    }));
    let v3 = apply(&v2, &e);
    assert!(v3.contains("light key { azimuth = -15.5; }") && !v3.contains("edges"), "{v3}");
    // nothing to change: empty edits; a bad name: empty edits and an error
    let r = s.post(ToEngine::SetRender { changes: vec![] });
    assert!(matches!(&r[..], [FromEngine::Edits { edits, .. }] if edits.is_empty()));
    let r = s
        .post(ToEngine::SetRender { changes: vec![RenderChange::Set { key: "shine".into(), value: Value::Num(1.0) }] });
    assert!(matches!(&r[..], [FromEngine::Edits { edits, .. }, FromEngine::Error { .. }] if edits.is_empty()), "{r:?}");
}
