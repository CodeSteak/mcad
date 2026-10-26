//! Tutorial models of `examples/` (README, `mcad usage`): every model checks without errors and
//! warnings, and the asserts of the macro pad hold (and fail for a case that is too low).

use std::path::PathBuf;

use assert_cmd::Command;
use serde_json::Value;

fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn json(args: &[&str]) -> Value {
    let out = Command::cargo_bin("mcad").unwrap().env("MCAD_NO_DAEMON", "1").args(args).arg("--json").output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{args:?}\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn tutorial_models_check_clean() {
    let mut files: Vec<_> = std::fs::read_dir(examples())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "mcad"))
        .collect();
    files.sort();
    assert!(files.len() >= 8, "{files:?}");
    for f in &files {
        let v = json(&["check", &f.display().to_string()]);
        assert_eq!((v["errors"].as_u64(), v["warnings"].as_u64()), (Some(0), Some(0)), "{}: {v}", f.display());
    }
}

#[test]
fn macropad_keepout_violated_when_case_too_low() {
    let f = examples().join("08-macropad.mcad").display().to_string();
    let v = json(&["check", &f, "-D", "h=12"]);
    assert_eq!(v["errors"].as_u64(), Some(0), "{v}");
    let d = v["diagnostics"].as_array().unwrap();
    assert!(d.iter().any(|d| d["message"].as_str().unwrap().contains("assert_keepout")), "{v}");
}
