//! Stage 1 of the OpenSCAD comparison: every file in `tests/models/openscad-compat/`
//! evaluates with `cad_eval::eval_source` without error diagnostics, yields 3D
//! geometry, and prints the same `echo` lines as OpenSCAD 2021.01.
//!
//! Reference: `tests/openscad-compat/reference.json` (written by `make-reference.py`).
//! The test does not need OpenSCAD; it fails if a model changed since the reference
//! was built.

mod common;

use common::*;
use serde_json::Value;

/// Files that are expected to fail stage 1 because of known bugs in `cad-eval`, with the
/// reason. The test fails as well when such a file starts passing, so the entry gets
/// removed together with the fix.
const KNOWN_FAILURES: &[(&str, &str)] = &[];

fn reference() -> Value {
    let p = workspace_root().join("tests/openscad-compat/reference.json");
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    serde_json::from_str(&text).unwrap()
}

fn echo_problems(ours: &[String], theirs: &[String]) -> Vec<String> {
    let mut p = Vec::new();
    for (i, (a, b)) in ours.iter().zip(theirs).enumerate() {
        if a != b {
            p.push(format!("echo {i} differs:\n      ours:     {a}\n      openscad: {b}"));
        }
    }
    if ours.len() != theirs.len() {
        p.push(format!("echo count: ours {}, openscad {} (ours: {ours:?})", ours.len(), theirs.len()));
    }
    p
}

#[test]
fn openscad_models_stage1() {
    let reference = reference();
    let files = model_files("tests/models/openscad-compat");
    assert!(files.len() >= 25, "expected at least 25 models, found {}", files.len());
    let refs = reference["files"].as_object().unwrap();

    let mut report = Vec::new();
    let mut unexpected = Vec::new();
    for path in &files {
        let name = file_name(path);
        let src = std::fs::read_to_string(path).unwrap();
        let mut problems = Vec::new();

        let head = src.lines().next().unwrap_or("");
        if !head.starts_with("// Purpose:") {
            problems.push("missing header comment `// Purpose: …`".to_string());
        }
        let out = eval(&src);
        problems.extend(stage1_problems(&src, &out));

        match refs.get(&name) {
            None => problems.push("no reference data (run tests/openscad-compat/make-reference.py)".into()),
            Some(r) => {
                if r["source_fnv1a64"].as_str() != Some(fnv1a64(src.as_bytes()).as_str()) {
                    problems.push("reference is stale (run tests/openscad-compat/make-reference.py)".into());
                }
                let theirs: Vec<String> =
                    r["echo"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
                let ours: Vec<String> = out.echoes.iter().map(|e| e.text.clone()).collect();
                problems.extend(echo_problems(&ours, &theirs));
            }
        }

        let warnings = diags(&src, &out, cad_syntax::Severity::Warning);
        let known = KNOWN_FAILURES.iter().find(|(f, _)| *f == name);
        let status = match (problems.is_empty(), known) {
            (true, None) => "ok",
            (false, Some(_)) => "known failure",
            (true, Some(_)) => {
                unexpected.push(format!("{name}: listed in KNOWN_FAILURES but passes now"));
                "FIXED?"
            }
            (false, None) => {
                unexpected.push(format!("{name}:\n    {}", problems.join("\n    ")));
                "FAIL"
            }
        };
        report.push(format!(
            "{name:32} {status:14} nodes={:4} solids={:3} echo={} warnings={}{}",
            out.tree.nodes.len(),
            solid_leaves(&out.tree),
            out.echoes.len(),
            warnings.len(),
            known.map(|(_, why)| format!("  ({why})")).unwrap_or_default()
        ));
        for w in warnings {
            report.push(format!("    warning {w}"));
        }
        if known.is_some() {
            for p in &problems {
                report.push(format!("    {p}"));
            }
        }
    }
    for name in refs.keys() {
        if !files.iter().any(|f| file_name(f) == *name) {
            unexpected.push(format!("{name}: in reference.json but the model is missing"));
        }
    }
    println!("{}", report.join("\n"));
    assert!(unexpected.is_empty(), "stage 1 failures:\n{}", unexpected.join("\n"));
}
