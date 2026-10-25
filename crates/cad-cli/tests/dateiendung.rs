//! Model files after the rename `cad` → `mcad`: new files are `.mcad`, `.scad` files (OpenSCAD,
//! projects from before) open, evaluate, export and work in `include`/`use` the same way, also
//! mixed in one project.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

fn mcad() -> Command {
    let mut c = Command::cargo_bin("mcad").unwrap();
    c.env("MCAD_NO_DAEMON", "1");
    c
}

fn run(args: &[&str], code: i32) -> Value {
    let out = mcad().args(args).arg("--json").output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(code),
        "{args:?}\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn write(dir: &Path, rel: &str, text: &str) -> String {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
    p.display().to_string()
}

fn files(v: &Value) -> Vec<String> {
    v["scene"]["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap().to_string()).collect()
}

/// A project with both extensions: `.scad` main file, `include` of a `.mcad` file, `use` of a
/// `.scad` library which in turn uses a `.mcad` one.
fn mixed_project(dir: &Path) -> String {
    write(dir, "konstanten.mcad", "breite = 30;\n");
    write(dir, "lib/teile.scad", "use <form.mcad>\nmodule teil(h) { form(breite = 20, h = h); }\n");
    write(dir, "lib/form.mcad", "module form(breite, h) { cube([breite, 10, h]); }\n");
    write(
        dir,
        "alt.scad",
        "include <konstanten.mcad>\nuse <lib/teile.scad>\nmodule platte() { cube([breite, 10, 2]); }\nboden: platte();\n\
         translate([0, 20, 0]) teil(5);\n",
    )
}

#[test]
fn scad_main_file_with_mcad_and_scad_includes() {
    let dir = tempfile::tempdir().unwrap();
    let main = mixed_project(dir.path());
    let v = run(&["check", &main], 0);
    assert_eq!(v["errors"], 0, "{v}");
    let v = run(&["eval", &main], 0);
    assert_eq!(files(&v), ["alt.scad", "konstanten.mcad", "lib/teile.scad", "lib/form.mcad"]);
    let inc = v["scene"]["includes"].as_array().unwrap();
    assert!(inc.iter().all(|i| i["found"] == true), "{inc:?}");
    // 30·10·2 (include) + 20·10·5 (use → use)
    let v = run(&["measure", &main, "--volume", "all"], 0);
    let vol = v["volume"]["volume"].as_f64().unwrap_or_else(|| panic!("{v}"));
    assert!((vol - 1600.0).abs() < 1e-6, "{vol}");
    let v = run(&["outline", &main, "--no-kernel"], 0);
    assert_eq!(v["parts"][0]["name"], "boden", "{v}");
    for ext in ["stl", "step", "3mf"] {
        let out = dir.path().join(format!("alt.{ext}"));
        let v = run(&["export", &main, "-o", &out.display().to_string()], 0);
        assert!(v["bytes"].as_u64().unwrap() > 0 && out.exists(), "{ext}: {v}");
    }
    let step = std::fs::read_to_string(dir.path().join("alt.step")).unwrap();
    assert!(step.contains("mcad STEP export"), "STEP header names the program");
}

#[test]
fn mcad_main_file_includes_scad() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "lib/alt.scad", "module alt(d) { cylinder(d = d, h = 4); }\nr = 2;\n");
    let main = write(dir.path(), "neu.mcad", "include <lib/alt.scad>\nalt(2 * r);\n");
    let v = run(&["eval", &main], 0);
    assert_eq!(files(&v), ["neu.mcad", "lib/alt.scad"]);
    let out = dir.path().join("neu.stl");
    run(&["export", &main, "-o", &out.display().to_string()], 0);
    let img = dir.path().join("neu.png");
    run(&["render", &main, "-o", &img.display().to_string(), "--size", "160x120"], 0);
    assert!(img.exists());
    // `set` writes back into the same file, whatever its extension
    let lib = dir.path().join("lib/alt.scad");
    let o = mcad().args(["set", &lib.display().to_string(), "--param", "alt.d=7"]).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(std::fs::read_to_string(&lib).unwrap().contains("module alt(d = 7)"));
}

#[test]
fn openscad_example_stays_scad() {
    let f = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/openscad-compat/05_nested_booleans.scad");
    let f = f.display().to_string();
    assert_eq!(run(&["check", &f], 0)["errors"], 0);
    let dir = tempfile::tempdir().unwrap();
    run(&["export", &f, "-o", &dir.path().join("x.step").display().to_string()], 0);
}

#[test]
fn both_extensions_open_the_ui_entry() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["a.mcad", "b.scad", "C.MCAD"] {
        let f = write(dir.path(), name, "cube(1);\n");
        let out = mcad()
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env("MCAD_IPC_DIR", dir.path().join("ipc"))
            .arg(&f)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(!err.contains("unbekannter Befehl") && err.starts_with("mcad: "), "{name}: {err}");
    }
    // not existing, but named like a model file: a path, not a command
    for name in ["fehlt.mcad", "fehlt.scad"] {
        let out = mcad().env_remove("DISPLAY").env_remove("WAYLAND_DISPLAY").arg(name).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("gibt es nicht") && err.contains(".mcad/.scad"), "{name}: {err}");
    }
}
