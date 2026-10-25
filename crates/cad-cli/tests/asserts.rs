//! `cad check`/`export`/`render`/`outline`/`inspect` with collision asserts (Plan 21 C).

use std::path::PathBuf;

use assert_cmd::Command;
use serde_json::Value;

fn data(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name).display().to_string()
}

fn example(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/spec").join(name).display().to_string()
}

fn run(args: &[&str]) -> (i32, String) {
    let out = Command::cargo_bin("cad").unwrap().env("CAD_NO_DAEMON", "1").args(args).output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr),
    )
}

fn json(args: &[&str], code: i32) -> Value {
    let mut a = args.to_vec();
    a.push("--json");
    let (c, out) = run(&a);
    assert_eq!(c, code, "{args:?}: {out}");
    serde_json::from_str(&out).unwrap()
}

#[test]
fn check_reports_violations_as_warnings() {
    let v = json(&["check", &data("asserts_bad.scad")], 0);
    assert_eq!(v["warnings"], 2);
    assert_eq!(v["violations"], 2);
    let a = &v["asserts"];
    assert_eq!(a[0]["kind"], "keepout");
    assert_eq!(a[0]["pairs"][0]["problem"], "overlap");
    assert!((a[0]["pairs"][0]["volume"].as_f64().unwrap() - 100.0).abs() < 1e-6);
    assert_eq!(a[1]["pairs"][0]["problem"], "gap");
    let (c, t) = run(&["check", &data("asserts_bad.scad")]);
    assert_eq!(c, 0);
    assert!(t.contains("asserts_bad.scad:2:1: Warnung (Kernel): assert_keepout: "), "{t}");
    assert!(t.contains("Asserts: 2 von 2 geprüft, 2 verletzt"), "{t}");
}

#[test]
fn export_stops_unless_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("x.stl");
    let o = out.display().to_string();
    let v = json(&["export", &data("asserts_bad.scad"), "-o", &o], 4);
    assert_eq!(v["aborted"], true);
    assert_eq!(v["violations"], 2);
    assert!(!out.exists(), "nothing written");
    let (c, t) = run(&["export", &data("asserts_bad.scad"), "-o", &o]);
    assert_eq!(c, 4);
    assert!(t.contains("Export abgebrochen: 2 Assert-Verletzung(en)") && t.contains("--ignore-asserts"), "{t}");
    let v = json(&["export", &data("asserts_bad.scad"), "-o", &o, "--ignore-asserts"], 0);
    assert_eq!(v["ignored_asserts"], true);
    assert!(out.exists());
    // the keep-out volume is not exported: plate + two cubes
    assert!((v["volume"].as_f64().unwrap() - (40.0 * 30.0 * 3.0 + 2000.0)).abs() < 1e-6, "{}", v["volume"]);
    // the example passes
    let ok = dir.path().join("ok.3mf").display().to_string();
    let v = json(&["export", &example("asserts.scad"), "-o", &ok], 0);
    assert_eq!(v["violations"], 0);
}

#[test]
fn outline_inspect_render() {
    let v = json(&["outline", &example("asserts.scad")], 0);
    let kinds: Vec<&str> = v["asserts"].as_array().unwrap().iter().map(|a| a["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["touches", "intersect", "disjoint", "keepout"]);
    assert!(v["asserts"].as_array().unwrap().iter().all(|a| a["ok"] == true && a["checked"] == true));
    let (_, t) = run(&["inspect", &data("asserts_bad.scad"), "--line", "2"]);
    assert!(t.contains("[Sperrvolumen: nicht im Ergebnis]") && t.contains("VERLETZT: "), "{t}");
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("a.png").display().to_string();
    let v = json(&["render", &data("asserts_bad.scad"), "-o", &png, "--show", "asserts"], 0);
    assert!(v["overlays"]["asserts"].as_u64().unwrap() >= 2, "{}", v["overlays"]);
    let roles: Vec<&str> = v["visible_nodes"].as_array().unwrap().iter().map(|n| n["role"].as_str().unwrap()).collect();
    assert!(roles.contains(&"keepout"), "{roles:?}");
}
