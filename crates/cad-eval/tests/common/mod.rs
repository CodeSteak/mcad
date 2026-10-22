#![allow(dead_code)]

use cad_eval::{Context, EvalOptions, EvalOutput, PValue, eval_source};
use cad_ir::{NodeIdx, Op};
use cad_syntax::Severity;

/// Evaluates `src` as a file; panics on syntax errors.
pub fn run(src: &str) -> EvalOutput {
    run_with(src, &EvalOptions::default())
}

pub fn run_with(src: &str, opts: &EvalOptions) -> EvalOutput {
    let (_, d) = cad_syntax::parse(src);
    assert!(d.is_empty(), "syntax errors in test source: {d:?}");
    eval_source(src, opts)
}

pub fn run_module(src: &str, name: &str, overrides: Vec<(&str, PValue)>) -> EvalOutput {
    let opts = EvalOptions {
        context: Context::Module {
            name: name.into(),
            overrides: overrides.into_iter().map(|(n, v)| (n.to_string(), v)).collect(),
        },
        ..Default::default()
    };
    run_with(src, &opts)
}

/// Evaluation without errors or warnings.
pub fn clean(src: &str) -> EvalOutput {
    let out = run(src);
    let bad: Vec<_> = out.diagnostics.iter().filter(|d| d.severity != Severity::Info).collect();
    assert!(bad.is_empty(), "unexpected diagnostics: {bad:#?}");
    out
}

pub fn errors(out: &EvalOutput) -> Vec<String> {
    out.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.message.clone()).collect()
}

pub fn warnings(out: &EvalOutput) -> Vec<String> {
    out.diagnostics.iter().filter(|d| d.severity == Severity::Warning).map(|d| d.message.clone()).collect()
}

pub fn has_error(out: &EvalOutput, part: &str) -> bool {
    errors(out).iter().any(|m| m.contains(part))
}

pub fn has_warning(out: &EvalOutput, part: &str) -> bool {
    warnings(out).iter().any(|m| m.contains(part))
}

/// Echo texts.
pub fn echoes(out: &EvalOutput) -> Vec<String> {
    out.echoes.iter().map(|e| e.text.clone()).collect()
}

/// Value of `expr` as OpenSCAD's echo would print it.
pub fn ev(expr: &str) -> String {
    let out = run(&format!("echo({expr});"));
    out.echoes.last().map(|e| e.text.clone()).unwrap_or_default()
}

/// Echo of `expr` after the definitions in `pre`.
pub fn ev_in(pre: &str, expr: &str) -> String {
    let out = run(&format!("{pre}\necho({expr});"));
    out.echoes.last().map(|e| e.text.clone()).unwrap_or_default()
}

/// Hint text of the assignment to `name` (first statement assigning it).
pub fn hint(out: &EvalOutput, src: &str, name: &str) -> String {
    out.hints
        .iter()
        .find(|h| src[h.span.start as usize..h.span.end as usize].trim_start().starts_with(&format!("{name} =")))
        .map(|h| h.text.clone())
        .unwrap_or_else(|| panic!("no hint for {name}"))
}

/// Compact tree dump: one line per node, indented, `label` or `label(detail)`.
pub fn dump(out: &EvalOutput) -> String {
    let mut s = String::new();
    if let Some(r) = out.tree.root {
        dump_node(out, r, 0, &mut s);
    }
    s
}

fn dump_node(out: &EvalOutput, i: NodeIdx, depth: usize, s: &mut String) {
    let n = out.tree.node(i);
    let mut label = n.op.label();
    if let Op::ModuleCall { instance: Some(k), .. } = &n.op {
        label = format!("{k}:{label}");
    }
    let mut flags = String::new();
    if n.flags.zone {
        flags.push_str(" [zone]");
    }
    if n.flags.background {
        flags.push_str(" [%]");
    }
    if n.flags.highlight {
        flags.push_str(" [#]");
    }
    s.push_str(&format!("{}{}{}\n", "  ".repeat(depth), label, flags));
    for &c in &n.children {
        dump_node(out, c, depth + 1, s);
    }
}

/// Nodes with a given label in tree order.
pub fn nodes_by_label(out: &EvalOutput, label: &str) -> Vec<NodeIdx> {
    (0..out.tree.nodes.len() as NodeIdx).filter(|&i| out.tree.node(i).op.label() == label).collect()
}

pub fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

pub fn approx3(a: glam::DVec3, b: [f64; 3]) -> bool {
    approx(a.x, b[0]) && approx(a.y, b[1]) && approx(a.z, b[2])
}

pub fn span_text(src: &str, s: cad_syntax::Span) -> &str {
    &src[s.start as usize..s.end as usize]
}
