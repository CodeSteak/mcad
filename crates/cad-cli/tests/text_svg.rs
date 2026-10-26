//! CLI with `text()` and `import("….svg")` (Bericht 16): examples, volumes, SVG warnings with
//! their line, STEP keeps imported circles exact, `watch` follows a changed SVG.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use serde_json::Value;

fn example(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/models/spec").join(name).display().to_string()
}

fn json(args: &[&str], code: i32) -> Value {
    let out = Command::cargo_bin("mcad")
        .unwrap()
        .env("MCAD_LANG", "de")
        .env("MCAD_NO_DAEMON", "1")
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(code),
        "{args:?}\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn write(dir: &Path, name: &str, text: &str) -> String {
    let p = dir.join(name);
    std::fs::write(&p, text).unwrap();
    p.display().to_string()
}

fn volume(file: &str) -> f64 {
    json(&["measure", file, "--volume"], 0)["volume"]["volume"].as_f64().unwrap()
}

const RECT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="10mm" viewBox="0 0 20 10">
<rect width="20" height="10"/>
<text x="1" y="5">nicht unterstützt</text>
</svg>
"#;

#[test]
fn examples_check_clean() {
    for f in ["svg_logo.mcad", "text_schild.mcad"] {
        let v = json(&["check", &example(f)], 0);
        assert_eq!((v["errors"].as_u64(), v["warnings"].as_u64()), (Some(0), Some(0)), "{f}: {v}");
    }
}

#[test]
fn import_volume_warning_and_step() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "r.svg", RECT);
    let f = write(dir.path(), "m.scad", "linear_extrude(3) import(\"r.svg\");\n");
    assert!((volume(&f) - 600.0).abs() < 1e-6);
    let v = json(&["check", &f], 0);
    let w = v["diagnostics"].as_array().unwrap();
    assert!(w.iter().any(|d| d["message"].as_str().unwrap().starts_with("r.svg:3: `text`")), "{v}");
    // circles stay circles in STEP
    write(
        dir.path(),
        "c.svg",
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="20mm" viewBox="0 0 20 20"><circle cx="10" cy="10" r="8"/></svg>"#,
    );
    let f = write(dir.path(), "c.scad", "linear_extrude(2) import(\"c.svg\", center = true);\n");
    let step = dir.path().join("c.step");
    json(&["export", &f, "-o", &step.display().to_string()], 0);
    let t = std::fs::read_to_string(&step).unwrap();
    assert!(t.contains("CIRCLE") && t.contains("CYLINDRICAL_SURFACE"), "exact circle in STEP");
    let v = volume(&f);
    assert!((v - std::f64::consts::PI * 64.0 * 2.0).abs() < 1e-3, "{v}");
    // ellipses: extrusion of an ELLIPSE (OpenCASCADE reads that one exactly)
    write(
        dir.path(),
        "e.svg",
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="30mm" height="30mm" viewBox="0 0 30 30"><ellipse cx="15" cy="15" rx="10" ry="6"/></svg>"#,
    );
    let f = write(dir.path(), "e.scad", "linear_extrude(2) import(\"e.svg\");\n");
    let step = dir.path().join("e.step");
    json(&["export", &f, "-o", &step.display().to_string()], 0);
    let t = std::fs::read_to_string(&step).unwrap();
    assert!(t.contains("ELLIPSE('',") && t.contains("SURFACE_OF_LINEAR_EXTRUSION"), "ellipse in STEP");
    assert!((volume(&f) - std::f64::consts::PI * 60.0 * 2.0).abs() < 1e-3);
}

#[test]
fn text_volume_is_area_times_height() {
    let dir = tempfile::tempdir().unwrap();
    let f = write(dir.path(), "t.scad", "linear_extrude(2) text(\"H\", size = 10, font = \"Inter\");\n");
    // area of Inter's H at size 10 (OpenSCAD 2021.01: 32.08 mm²)
    let v = volume(&f);
    assert!((v - 2.0 * 32.0824).abs() < 0.01, "{v}");
    let v = json(&["check", &write(dir.path(), "u.scad", "text(\"a\", font = \"Keine\");\n")], 0);
    assert!(v["diagnostics"][0]["message"].as_str().unwrap().contains("nicht vorhanden"));
    // glyphs apart from each other unite without loss (stencil "c": three pieces; SMLib's union
    // returned one of them twice, once inside out, before the facade merged disjoint bodies)
    let vol = |t: &str, name: &str| {
        volume(&write(
            dir.path(),
            name,
            &format!("linear_extrude(0.6) text(\"{t}\", size = 8, font = \"Allerta Stencil\");\n"),
        ))
    };
    let (a, c, ac) = (vol("a", "a.scad"), vol("c", "c.scad"), vol("ac", "ac.scad"));
    assert!((c - 7.2994).abs() < 1e-3, "c: {c}");
    assert!((ac - (a + c)).abs() < 1e-3, "ac {ac} = a {a} + c {c}");
}

#[test]
fn watch_follows_a_changed_svg() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "r.svg", RECT);
    let file = write(dir.path(), "w.scad", "linear_extrude(1) import(\"r.svg\");\n");
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("mcad"))
        .env("MCAD_LANG", "de")
        .args(["watch", &file, "--json", "--once-after-change", "--timeout", "60"])
        .env("MCAD_NO_DAEMON", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let first: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(first["ok"], true, "{first}");
    std::thread::sleep(Duration::from_millis(300));
    write(dir.path(), "r.svg", &RECT.replace("<rect width=\"20\"", "<rect width=\"10\""));
    let t0 = Instant::now();
    let second: Value = serde_json::from_str(&lines.next().expect("zweite Zeile").unwrap()).unwrap();
    assert!(t0.elapsed() < Duration::from_secs(30));
    assert_eq!(second["trigger"], "change");
    assert_eq!(second["changed"], serde_json::json!(["r.svg"]));
    assert_eq!(child.wait().unwrap().code(), Some(0));
}
