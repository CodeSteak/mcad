//! `include`/`use` against OpenSCAD 2021.01: every project in `tests/openscad-compat/projects`
//! is evaluated from disk; its `ECHO:` lines must equal `expected.echo` (written by
//! `openscad -o x.echo main.scad`). With `openscad` installed the reference is also
//! recomputed and compared.

mod common;

use std::path::PathBuf;
use std::process::Command;

use cad_eval::{EvalOptions, FsResolver, eval_files};

fn projects() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/openscad-compat/projects");
    let mut out: Vec<PathBuf> =
        std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.is_dir()).collect();
    out.sort();
    out
}

fn ours(dir: &PathBuf) -> Vec<String> {
    let main = std::fs::read_to_string(dir.join("main.scad")).unwrap();
    let r = FsResolver { root: dir.clone() };
    let (p, out) = eval_files("main.scad", &main, &r, &EvalOptions::default());
    assert!(p.syntax.is_empty() && p.link.is_empty(), "{}: {:?} {:?}", dir.display(), p.syntax, p.link);
    out.echoes.iter().map(|e| format!("ECHO: {}", e.text)).collect()
}

fn echo_lines(text: &str) -> Vec<String> {
    text.lines().filter(|l| l.starts_with("ECHO:")).map(String::from).collect()
}

#[test]
fn projects_match_openscad_reference() {
    let ps = projects();
    assert!(ps.len() >= 4);
    for dir in &ps {
        let expected = echo_lines(&std::fs::read_to_string(dir.join("expected.echo")).unwrap());
        assert!(!expected.is_empty());
        assert_eq!(ours(dir), expected, "{}", dir.display());
    }
}

#[test]
fn projects_match_openscad_live() {
    let tmp = env!("CARGO_TARGET_TMPDIR");
    for dir in projects() {
        let echo = format!("{tmp}/{}.echo", dir.file_name().unwrap().to_string_lossy());
        let Ok(st) = Command::new("openscad").current_dir(&dir).args(["-o", &echo, "main.scad"]).output() else {
            eprintln!("openscad not installed, skipping live comparison");
            return;
        };
        assert!(st.status.success(), "openscad failed: {}", String::from_utf8_lossy(&st.stderr));
        let theirs = echo_lines(&std::fs::read_to_string(&echo).unwrap());
        assert_eq!(ours(&dir), theirs, "{}", dir.display());
    }
}
