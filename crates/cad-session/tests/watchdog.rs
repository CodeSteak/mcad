//! Watchdog (`BlockKeys`, `Progress.current.key`) and draft mode while dragging, native SMLib.

use cad_session::protocol::*;
use cad_session::{Session, SessionOptions, std_clock};
use kernel_smlib::SmlibKernel;

fn plate(t: f64) -> String {
    format!(
        "difference(fillet = 1) {{\n    box([0, 0, 0], [48, 12, {t}]);\n    for (i = [0 : 3]) translate([6 + 12 * i, 6, -1]) cylinder(h = {t} + 2, d = 4);\n}}\n"
    )
}

fn session(opts: SessionOptions) -> Session<SmlibKernel> {
    Session::new(SmlibKernel::new(), opts, std_clock())
}

fn last_scene(msgs: &[FromEngine]) -> Option<&Scene> {
    msgs.iter()
        .filter_map(|m| match m {
            FromEngine::Scene(s) => Some(s.as_ref()),
            _ => None,
        })
        .next_back()
}

fn blend(scene: &Scene) -> &SceneNode {
    scene.nodes.iter().find(|n| n.label == "difference").expect("difference node")
}

#[test]
fn block_keys_from_progress() {
    let mut s = session(SessionOptions::default());
    let first = s.post(ToEngine::SetSource { text: plate(4.0), version: 1 });
    let Some(FromEngine::Progress { current: Some(cur), .. }) = first.last() else { panic!("{first:?}") };
    let key = cur.key.clone().expect("key in Progress.current");
    assert_eq!(key.len(), 64);
    let prelim = last_scene(&first).unwrap();
    // the key equals the node's mesh key in the preliminary scene
    assert_eq!(prelim.nodes[cur.node as usize].mesh_key.as_deref(), Some(key.as_str()));
    let done = s.run_to_idle();
    let diff_key = blend(last_scene(&done).unwrap()).mesh_key.clone().unwrap();

    // watchdog restart: new session, block the key, same source
    let mut s = session(SessionOptions::default());
    assert!(s.post(ToEngine::BlockKeys { keys: vec![diff_key.clone()] }).is_empty());
    s.post(ToEngine::SetSource { text: plate(4.0), version: 1 });
    let done = s.run_to_idle();
    let n = blend(last_scene(&done).unwrap());
    assert_eq!(n.status, NodeStatus::Error);
    assert_eq!(n.messages[0].message, cad_engine::blocked_message());
    let diags: Vec<_> = done
        .iter()
        .filter_map(|m| match m {
            FromEngine::Diagnostics { items, .. } => Some(items.clone()),
            _ => None,
        })
        .next_back()
        .unwrap();
    assert!(diags.iter().any(|d| d.message == cad_engine::blocked_message() && d.node == Some(n.idx)));
    // changed parameter: computed again
    s.post(ToEngine::SetSource { text: plate(5.0), version: 2 });
    let done = s.run_to_idle();
    assert_eq!(blend(last_scene(&done).unwrap()).status, NodeStatus::Ok);
    // bad keys are reported
    let r = s.post(ToEngine::BlockKeys { keys: vec!["xyz".into()] });
    assert!(matches!(r.as_slice(), [FromEngine::Error { .. }]), "{r:?}");
    // JSON form of the message
    let m: ToEngine = serde_json::from_str(&format!(r#"{{"type":"BlockKeys","keys":["{diff_key}"]}}"#)).unwrap();
    assert_eq!(m, ToEngine::BlockKeys { keys: vec![diff_key] });
}

#[test]
fn drag_suspends_slow_fillet_and_drag_end_restores_it() {
    let mut opts = SessionOptions::default();
    opts.engine.drag_blend_budget_ms = Some(0.0);
    let mut s = session(opts);
    s.post(ToEngine::SetSource { text: plate(4.0), version: 1 });
    s.run_to_idle();
    s.post(ToEngine::DragStart { handle: 0 });
    s.post(ToEngine::SetSource { text: plate(5.0), version: 2 });
    let done = s.run_to_idle();
    let n = blend(last_scene(&done).unwrap());
    assert!(n.hints.iter().any(|h| h == cad_engine::suspended_hint()), "{:?}", n.hints);
    assert_eq!(n.status, NodeStatus::Ok);
    let draft_key = n.mesh_key.clone();
    // release without a new source: evaluated in full again
    let msgs = s.post(ToEngine::DragEnd { handle: 0, target: None });
    assert!(last_scene(&msgs).is_some(), "re-evaluation starts: {msgs:?}");
    let done = s.run_to_idle();
    let n = blend(last_scene(&done).unwrap());
    assert!(!n.hints.iter().any(|h| h == cad_engine::suspended_hint()), "{:?}", n.hints);
    assert_ne!(n.mesh_key, draft_key);
    let fine: Vec<_> = done
        .iter()
        .filter_map(|m| match m {
            FromEngine::Meshes { items, .. } => Some(items.iter().map(|i| i.coarse)),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(fine, vec![false]);
}
