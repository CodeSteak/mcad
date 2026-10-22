//! Recursion limits, step budget and stack guard: errors instead of crashes or
//! endless loops.

mod common;
use cad_eval::EvalOptions;
use common::*;

/// Runs `f` on a thread with a large stack (deep recursion tests).
fn big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new().stack_size(512 << 20).spawn(f).unwrap().join().unwrap()
}

fn deep_opts() -> EvalOptions {
    EvalOptions { max_stack_bytes: 400 << 20, ..Default::default() }
}

#[test]
fn module_recursion_limit() {
    let msg = big_stack(|| {
        let out = run_with("module r(n) { cube(1); r(n + 1); } r(0);", &deep_opts());
        assert!(out.aborted);
        errors(&out)
    });
    assert!(msg.iter().any(|m| m.contains("module `r` nested more than 1000 levels")), "{msg:?}");
}

#[test]
fn function_recursion_limit() {
    let msg = big_stack(|| {
        let out = run_with("function f(n) = 1 + f(n + 1); echo(f(0));", &deep_opts());
        assert!(out.aborted);
        errors(&out)
    });
    assert!(msg.iter().any(|m| m.contains("function `f` called more than 2000 levels deep")), "{msg:?}");
}

#[test]
fn recursion_within_limits_works() {
    let out = big_stack(|| {
        let out = run_with("function f(n) = n == 0 ? 0 : 1 + f(n - 1); echo(f(1500));", &deep_opts());
        (echoes(&out), errors(&out))
    });
    assert_eq!(out, (vec!["1500".to_string()], vec![]));
}

#[test]
fn stack_guard_reports_instead_of_overflowing() {
    // Default options on a normal test thread: deep recursion hits the stack guard (or
    // the depth limit) and reports an error.
    let out = run("function f(n) = 1 + f(n + 1); echo(f(0));");
    assert!(out.aborted);
    assert!(errors(&out).iter().any(|m| m.contains("recursion too deep")), "{:?}", out.diagnostics);
    let opts = EvalOptions { max_stack_bytes: 64 << 10, ..Default::default() };
    let out = run_with("module r(n) r(n + 1); r(0);", &opts);
    assert!(has_error(&out, "evaluation stack exhausted"));
}

#[test]
fn tail_calls_do_not_recurse() {
    let src = "function t(n, acc = 0) = n == 0 ? acc : t(n - 1, acc + n); echo(t(100000));
               g = function(n) n <= 0 ? \"done\" : let(m = n - 1) g(m); echo(g(50000));";
    let out = clean(src);
    assert_eq!(echoes(&out), ["5.00005e+9", "\"done\""]);
}

#[test]
fn step_budget() {
    let opts = EvalOptions { max_steps: 100_000, ..Default::default() };
    let out = run_with("function f(n) = n == 0 ? 0 : f(n - 1); echo(f(1e9));", &opts);
    assert!(out.aborted);
    assert!(has_error(&out, "more than 100000 steps"));
    let out = run_with("for (i = [0:1e6]) cube(i);", &opts);
    assert!(has_error(&out, "more than 100000 steps"));
    let out = run_with("x = [for (i = 0; true; i = i + 1) i];", &opts);
    assert!(out.aborted);
    // The partial tree is still consistent.
    let out = run_with("cube(1); for (i = [0:1e6]) sphere(1);", &opts);
    assert!(out.tree.nodes.len() > 2);
    for (i, n) in out.tree.nodes.iter().enumerate() {
        for &c in &n.children {
            assert_eq!(out.tree.node(c).parent, Some(i as u32));
        }
    }
}

#[test]
fn huge_ranges_and_lists_are_rejected() {
    let out = run("x = [for (i = [0:1e9]) i];");
    assert!(has_error(&out, "range has too many elements"));
    let opts = EvalOptions { max_list_len: 1000, ..Default::default() };
    let out = run_with("x = [for (i = [0:500]) each [i, i]];", &opts);
    assert!(has_error(&out, "list has more than 1000 elements"));
}

#[test]
fn evaluation_is_deterministic() {
    let a = run("module m(n) for (i = [0:n]) translate([i, rands(0, 1, 1)[0], 0]) cube(1); m(5); k: m(2);");
    let b = run("module m(n) for (i = [0:n]) translate([i, rands(0, 1, 1)[0], 0]) cube(1); m(5); k: m(2);");
    assert_eq!(format!("{:?}", a.tree), format!("{:?}", b.tree));
    let ha: Vec<u64> = a.handles.iter().map(|h| h.id).collect();
    let hb: Vec<u64> = b.handles.iter().map(|h| h.id).collect();
    assert_eq!(ha, hb);
}
