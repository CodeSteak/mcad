//! Draft mode (drag: slow blends left out) and blocked keys (watchdog), native SMLib.

mod common;

use std::f64::consts::PI;

use cad_engine::{
    BLOCKED_MESSAGE, BLOCKED_MESSAGE_EN, CountingKernel, Engine, EngineConfig, SUSPENDED_HINT_EN, Status,
    blocked_message, std_clock, suspended_hint,
};
use cad_eval::{EvalOptions, eval_source};
use cad_ir::{NodeIdx, Op, Tree};
use common::*;
use kernel_smlib::SmlibKernel;

/// Plate with four bores, `difference(fillet = 1)` (like `tests/models/spec/lochplatte.mcad`).
fn plate(t: f64) -> Tree {
    let src = format!(
        r#"
difference(fillet = 1) {{
    box([0, 0, 0], [48, 12, {t}]);
    for (i = [0 : 3]) translate([6 + 12 * i, 6, -1]) cylinder(h = {t} + 2, d = 4);
}}
"#
    );
    let out = eval_source(&src, &EvalOptions::default());
    assert!(out.errors().next().is_none());
    out.tree
}

fn plain_volume(t: f64) -> f64 {
    48.0 * 12.0 * t - 4.0 * PI * 4.0 * t
}

fn blend_node(tree: &Tree) -> NodeIdx {
    tree.nodes.iter().position(|n| matches!(n.op, Op::Boolean { fillet: Some(_), .. })).unwrap() as NodeIdx
}

fn engine_with(cfg: EngineConfig) -> E {
    let mut e = Engine::with_config(CountingKernel::new(SmlibKernel::new()), cfg);
    e.set_clock(std_clock());
    e
}

#[test]
fn draft_mode_suspends_slow_blends() {
    // budget 0: every measured fillet counts as slow
    let mut e = engine_with(EngineConfig { drag_blend_budget_ms: Some(0.0), ..Default::default() });
    // 1. full evaluation measures the fillet
    let ev = e.evaluate(plate(4.0));
    assert_clean(&ev);
    let v_fine4 = root_volume(&mut e, &ev);
    assert!(v_fine4 < plain_volume(4.0) - 1.0, "rounded rims remove material: {v_fine4}");
    let n = blend_node(&ev.tree);
    let fine_key = ev.report(n).key;
    // 2. drag: new thickness, the fillet is left out
    e.set_draft(true);
    e.kernel_mut().reset();
    let ev = e.evaluate(plate(5.0));
    assert_eq!(e.kernel().count("fillet"), 0);
    assert!(ev.draft);
    assert_eq!(ev.stats.suspended_blends, 1);
    let r = ev.report(n);
    assert!(r.blend_suspended);
    assert!(r.hints.iter().any(|h| h == suspended_hint() && h == SUSPENDED_HINT_EN), "{:?}", r.hints);
    assert!(r.hints.iter().any(|h| h == "8 edges"), "edges are still selected: {:?}", r.hints);
    assert_eq!(r.status, Status::Ok);
    assert!(errors(&ev).is_empty() && warnings(&ev).is_empty());
    assert_approx("body without fillet", root_volume(&mut e, &ev), plain_volume(5.0), 1e-6);
    let draft_key = ev.report(n).key;
    // 3. drag back to a cached value: the cached full result is used, nothing suspended
    let ev = e.evaluate(plate(4.0));
    assert_eq!(ev.stats.suspended_blends, 0);
    assert_eq!(ev.report(n).key, fine_key);
    assert!(ev.report(n).cached);
    // 4. drag end: full evaluation with fillet, under another key than the draft result
    e.set_draft(false);
    e.kernel_mut().reset();
    let ev = e.evaluate(plate(5.0));
    assert_clean(&ev);
    assert_eq!(e.kernel().count("fillet"), 1);
    assert!(!ev.report(n).blend_suspended && !ev.report(n).cached);
    assert_ne!(ev.report(n).key, draft_key);
    assert!(root_volume(&mut e, &ev) < plain_volume(5.0) - 1.0);
}

#[test]
fn draft_mode_keeps_fast_blends() {
    let mut e = engine_with(EngineConfig { drag_blend_budget_ms: Some(1e9), ..Default::default() });
    let ev = e.evaluate(plate(4.0));
    assert_clean(&ev);
    e.set_draft(true);
    e.kernel_mut().reset();
    let ev = e.evaluate(plate(5.0));
    assert_eq!(e.kernel().count("fillet"), 1);
    assert_eq!(ev.stats.suspended_blends, 0);
    assert!(root_volume(&mut e, &ev) < plain_volume(5.0) - 1.0);
    // never suspended without a budget either
    let mut e = engine_with(EngineConfig { drag_blend_budget_ms: None, ..Default::default() });
    e.evaluate(plate(4.0));
    e.set_draft(true);
    assert_eq!(e.evaluate(plate(5.0)).stats.suspended_blends, 0);
}

#[test]
fn blocked_key_is_not_computed_until_it_changes() {
    let mut e = engine();
    let ev = e.evaluate(plate(4.0));
    let n = blend_node(&ev.tree);
    let key = ev.report(n).key;
    // watchdog restart: a new engine gets the key with its configuration
    let mut e = Engine::with_config(
        CountingKernel::new(SmlibKernel::new()),
        EngineConfig { blocked_keys: vec![key], ..Default::default() },
    );
    let ev = e.evaluate(plate(4.0));
    // only the children ran (the `for` group unites its cylinders), not the blocked node
    assert_eq!((e.kernel().count("fillet"), e.kernel().count("edges"), e.kernel().count("boolean")), (0, 0, 1));
    let r = ev.report(n);
    assert!(r.blocked);
    assert_eq!(r.status, Status::Error);
    assert_eq!(errors(&ev), vec![blocked_message().to_string()]);
    assert_eq!(blocked_message(), BLOCKED_MESSAGE_EN);
    {
        let _de = cad_i18n::scoped(cad_i18n::Lang::De);
        assert_eq!(blocked_message(), BLOCKED_MESSAGE);
    }
    assert_eq!(ev.stats.blocked, 1);
    assert!(ev.parts(n).is_empty());
    // the children were computed
    assert!(ev.tree.node(n).children.iter().all(|&c| !ev.parts(c).is_empty()));
    // a changed parameter gives another key: computed again
    let ev = e.evaluate(plate(5.0));
    assert_clean(&ev);
    assert!(!ev.report(n).blocked);
    assert_eq!(e.kernel().count("fillet"), 1);
    // blocking at run time, and lifting it
    let k5 = ev.report(n).key;
    e.block_keys([k5]);
    let ev = e.evaluate(plate(5.0));
    assert!(ev.report(n).blocked, "a cached result is blocked as well");
    e.unblock_all();
    let ev = e.evaluate(plate(5.0));
    assert!(!ev.report(n).blocked && ev.report(n).cached);
    // hex round trip (protocol)
    assert_eq!(cad_engine::CacheKey::from_hex(&key.to_hex()), Some(key));
    assert_eq!(cad_engine::CacheKey::from_hex("12"), None);
}
