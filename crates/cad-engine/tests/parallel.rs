//! Evaluation on worker threads (`Engine::set_workers`): independent nodes really run at the
//! same time, and the results are those of the sequential evaluation (reports with their
//! diagnostics, hints, tags and keys; volumes; statistics), also when repeated.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use cad_engine::{CountingKernel, Engine, Evaluation};
use cad_eval::{EvalOptions, eval_source};
use cad_ir::Tree;
use kernel_solidcore::{Shape, SolidcoreKernel};

fn tree(src: &str) -> Tree {
    let out = eval_source(src, &EvalOptions::default());
    let errs: Vec<_> = out.errors().collect();
    assert!(errs.is_empty(), "{errs:#?}");
    out.tree
}

type K = CountingKernel<SolidcoreKernel>;

fn engine(jobs: usize) -> Engine<K> {
    let mut e = Engine::new(CountingKernel::new(SolidcoreKernel::new()));
    e.set_workers(jobs, Arc::new(|| CountingKernel::new(SolidcoreKernel::new())));
    assert_eq!(e.jobs(), jobs.max(1));
    e
}

/// Everything of an evaluation that must not depend on the threads: the reports (status,
/// diagnostics in order, hints, selected edges, bounds, keys, tags, cached), the statistics
/// (nodes, cache hits, kernel calls) and the volume of every node with parts.
fn summary(e: &mut Engine<K>, ev: &Evaluation<Shape>) -> String {
    let mut out = String::new();
    for r in &ev.reports {
        let vol =
            if ev.parts(r.node).is_empty() { None } else { e.mass_props(ev, r.node).ok().map(|m| m.volume.to_bits()) };
        out.push_str(&format!("{r:?} {vol:?}\n"));
    }
    let s = ev.stats;
    out.push_str(&format!("{} {} {}\n", s.nodes, s.cache_hits, s.kernel_calls));
    out
}

/// Two kinds of work at once: four independent Booleans under a union, each made slow, run on
/// four threads; the number of Booleans running at once exceeds 1.
#[test]
fn independent_siblings_run_at_the_same_time() {
    let running = Arc::new(AtomicUsize::new(0));
    let most = Arc::new(AtomicUsize::new(0));
    let (r, m) = (running.clone(), most.clone());
    let hook: cad_engine::counting::CallHook = Arc::new(move |name, start| {
        if name != "boolean" {
            return;
        }
        if start {
            let now = r.fetch_add(1, Ordering::SeqCst) + 1;
            m.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(150));
        } else {
            r.fetch_sub(1, Ordering::SeqCst);
        }
    });
    let h = hook.clone();
    let mut e = Engine::new(CountingKernel::with_hook(SolidcoreKernel::new(), hook));
    e.set_workers(4, Arc::new(move || CountingKernel::with_hook(SolidcoreKernel::new(), h.clone())));
    let src = "for (i = [0 : 3]) translate([30 * i, 0, 0]) difference() { cube(20); translate([5, 5, -1]) cube([10 + i, 10, 22]); }";
    let ev = e.evaluate(tree(src));
    assert!(ev.diagnostics().next().is_none(), "{:?}", ev.diagnostics().collect::<Vec<_>>());
    assert!(most.load(Ordering::SeqCst) >= 2, "at most {} Booleans at once", most.load(Ordering::SeqCst));
    // and the same as one thread
    let mut s = engine(1);
    let seq = s.evaluate(tree(src));
    let mut p = engine(4);
    let par = p.evaluate(tree(src));
    assert_eq!(summary(&mut s, &seq), summary(&mut p, &par));
}

/// Identical subtrees: the later one takes the earlier one's result as a cache hit, with any
/// number of threads (the same statistics as sequential).
#[test]
fn identical_subtrees_are_cache_hits_in_parallel() {
    let src = "module m() difference() { cube(10); translate([2, 2, -1]) cube([6, 6, 12]); }\nfor (i = [0 : 5]) translate([20 * i, 0, 0]) m();";
    let mut s = engine(1);
    let seq = s.evaluate(tree(src));
    let mut p = engine(4);
    let par = p.evaluate(tree(src));
    assert!(seq.stats.cache_hits > 0, "{:?}", seq.stats);
    assert_eq!(summary(&mut s, &seq), summary(&mut p, &par));
}

/// The spec examples and the examples (those without include/use): one thread, then four,
/// twice, on fresh engines and on one engine (cache hits in between): the same.
#[test]
fn parallel_matches_sequential_on_the_examples() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for d in ["examples", "tests/models/spec"] {
        for p in std::fs::read_dir(root.join(d)).unwrap().map(|e| e.unwrap().path()) {
            if p.extension().is_some_and(|x| x == "mcad") {
                files.push(p);
            }
        }
    }
    files.sort();
    let shared = Mutex::new(engine(4));
    let mut checked = 0;
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap();
        if src.contains("include <") || src.contains("use <") || src.contains("import(") {
            continue;
        }
        let out = eval_source(&src, &EvalOptions::default());
        if out.errors().next().is_some() {
            continue;
        }
        let mut s = engine(1);
        let ev = s.evaluate(out.tree.clone());
        let want = summary(&mut s, &ev);
        for round in 0..2 {
            let mut p = engine(4);
            let ev = p.evaluate(out.tree.clone());
            assert_eq!(summary(&mut p, &ev), want, "{} (round {round})", f.display());
        }
        // a second evaluation on the same engine: everything from the cache, the same reports
        let mut p = shared.lock().unwrap();
        let first = p.evaluate(out.tree.clone());
        let a = summary(&mut p, &first);
        let again = p.evaluate(out.tree.clone());
        assert_eq!(again.stats.cache_hits, again.stats.nodes, "{}", f.display());
        let reports = |ev: &Evaluation<Shape>| {
            ev.reports.iter().map(|r| (r.status, r.diagnostics.clone(), r.hints.clone(), r.key)).collect::<Vec<_>>()
        };
        assert_eq!(reports(&first), reports(&again), "{}", f.display());
        assert_eq!(a.lines().count(), summary(&mut p, &again).lines().count());
        checked += 1;
    }
    assert!(checked >= 15, "{checked} files");
}
