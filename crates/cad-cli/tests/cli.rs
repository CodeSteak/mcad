//! End-to-end tests of the `cad` binary on small files in `tests/data/`.

use std::path::PathBuf;

use assert_cmd::Command;
use serde_json::Value;

fn data(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name).display().to_string()
}

fn cad() -> Command {
    Command::cargo_bin("cad").unwrap()
}

/// Runs with `--json`, checks the exit code, returns the parsed output.
fn json(args: &[&str], code: i32) -> Value {
    let out = cad().args(args).arg("--json").output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(code),
        "{args:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["exit_code"], code);
    assert_eq!(v["ok"], code == 0);
    v
}

fn text(args: &[&str], code: i32) -> String {
    let out = cad().args(args).output().unwrap();
    assert_eq!(out.status.code(), Some(code), "{args:?}\nstderr: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap() + &String::from_utf8(out.stderr).unwrap()
}

#[test]
fn check_ok_syntax_error_and_kernel_error() {
    let v = json(&["check", &data("gehaeuse.scad")], 0);
    assert_eq!(v["command"], "check");
    assert_eq!(v["errors"], 0);

    let v = json(&["check", &data("syntax_fehler.scad")], 1);
    let d = &v["diagnostics"][0];
    assert_eq!(d["severity"], "error");
    assert_eq!(d["source"], "syntax");
    assert_eq!(d["range"]["line"], 3);
    let t = text(&["check", &data("syntax_fehler.scad")], 1);
    assert!(t.contains("syntax_fehler.scad:3:"), "{t}");

    let v = json(&["check", &data("fillet_zu_gross.scad")], 1);
    let d = &v["diagnostics"][0];
    assert_eq!(d["source"], "kernel");
    assert_eq!(d["range"]["line"], 2);
    // without the kernel the file is fine
    json(&["check", "--no-kernel", &data("fillet_zu_gross.scad")], 0);
}

#[test]
fn eval_scene_text_and_json() {
    let v = json(&["eval", &data("gehaeuse.scad")], 0);
    let scene = &v["scene"];
    assert_eq!(scene["complete"], true);
    let anchors = scene["anchors"].as_array().unwrap();
    let ecke = anchors.iter().find(|a| a["text"] == "k.ecke").unwrap();
    assert_eq!(ecke["point"], serde_json::json!([60.0, 30.0, 20.0]));
    assert!(scene["nodes"].as_array().unwrap().iter().all(|n| n["status"] == "ok"));

    let t = text(&["eval", &data("gehaeuse.scad")], 0);
    assert!(t.contains("Kontext: Datei"), "{t}");
    assert!(t.contains("difference"), "{t}");
    assert!(t.contains("k.ecke = [60, 30, 20]"), "{t}");
    assert!(t.contains("Raster:"), "{t}");

    // module context with -D
    let v = json(&["eval", &data("gehaeuse.scad"), "--module", "gehaeuse", "-D", "w=80"], 0);
    assert_eq!(v["context"], "gehaeuse");
    let params = v["scene"]["context"]["params"].as_array().unwrap();
    let w = params.iter().find(|p| p["name"] == "w").unwrap();
    assert_eq!(w["value"], 80.0);
    assert_eq!(w["overridden"], true);
    let t = text(&["eval", &data("platte.scad")], 0);
    assert!(t.contains("ECHO: \"fertig\", 4"), "{t}");
}

#[test]
fn inspect_line() {
    // line 11 is the outer box inside module gehaeuse: module context
    let v = json(&["inspect", &data("gehaeuse.scad"), "--line", "11"], 0);
    assert_eq!(v["context"], "gehaeuse");
    let nodes = v["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["label"], "box");
    let vol = nodes[0]["volume"].as_f64().unwrap();
    assert!((vol - 40.0 * 30.0 * 20.0).abs() < 1e-6, "{vol}");
    // in the file context the same line has no own node... the call on line 21 has
    let v = json(&["inspect", &data("gehaeuse.scad"), "--line", "21"], 0);
    assert_eq!(v["context"], Value::Null);
    let n = &v["nodes"][0];
    assert_eq!(n["label"], "gehaeuse");
    assert_eq!(n["instance"], "k");
    assert_eq!(n["world_bounds"]["max"][0], 60.0);
    // zone block: selected edges
    let v = json(&["inspect", &data("fillet_zonen.scad"), "--line", "12"], 0);
    let n = &v["nodes"][0];
    assert_eq!(n["label"], "fillet");
    assert_eq!(n["selected_edges"], 2);
    assert!(n["hints"].as_array().unwrap().iter().any(|h| h == "2 Kanten"));
    // variable line: value and handle-free
    let v = json(&["inspect", &data("platte.scad"), "--line", "2"], 0);
    assert_eq!(v["nodes"].as_array().unwrap().len(), 0);
    assert_eq!(v["hints"][0]["text"], "4");
    let t = text(&["inspect", &data("fillet_zu_gross.scad"), "--line", "2"], 1);
    assert!(t.contains("Verrundung gescheitert"), "{t}");
}

#[test]
fn render_png() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("g.png");
    let o = out.display().to_string();
    let v = json(&["render", &data("gehaeuse.scad"), "-o", &o, "--size", "320x240"], 0);
    assert_eq!(v["width"], 320);
    assert!(v["triangles"].as_u64().unwrap() > 10);
    let png = std::fs::read(&out).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    // highlighted line: subtracted body of line 12 (module context)
    let v = json(&["render", &data("gehaeuse.scad"), "-o", &o, "--line", "12", "--view", "top"], 0);
    assert_eq!(v["highlighted"].as_array().unwrap().len(), 1);
    assert_eq!(v["context"], "gehaeuse");
    // usage errors
    json(&["render", &data("gehaeuse.scad"), "-o", &o, "--view", "schräg"], 2);
    json(&["render", &data("gehaeuse.scad"), "-o", &o, "--size", "gross"], 2);
    // syntax error: no image, exit 1
    let o2 = dir.path().join("x.png");
    json(&["render", &data("syntax_fehler.scad"), "-o", &o2.display().to_string()], 1);
    assert!(!o2.exists());
}

#[test]
fn export_stl_and_3mf() {
    let dir = tempfile::tempdir().unwrap();
    let stl = dir.path().join("teil.stl");
    let v = json(&["export", &data("platte.scad"), "-o", &stl.display().to_string()], 0);
    let tris = v["triangles"].as_u64().unwrap();
    let bytes = std::fs::read(&stl).unwrap();
    assert_eq!(bytes.len() as u64, 84 + 50 * tris);
    assert_eq!(v["bytes"], bytes.len());
    let vol = v["volume"].as_f64().unwrap();
    assert!(vol > 5000.0 && vol < 6000.0, "{vol}");
    let mf = dir.path().join("teil.3mf");
    json(&["export", &data("platte.scad"), "-o", &mf.display().to_string()], 0);
    assert_eq!(&std::fs::read(&mf).unwrap()[..2], b"PK");
    json(&["export", &data("platte.scad"), "-o", &dir.path().join("teil.obj").display().to_string()], 2);
}

#[test]
fn usage_errors() {
    json(&["check", "/nonexistent/datei.scad"], 2);
    // -D without module context
    json(&["eval", &data("gehaeuse.scad"), "-D", "w=1"], 2);
    // clap errors exit with 2 as well
    let out = cad().args(["render"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn stdin_source() {
    let out = cad().args(["check", "-", "--json"]).write_stdin("cube(5);\n").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["file"], "<stdin>");
}
