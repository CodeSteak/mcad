//! Testbericht 13 (Korrekturen Geometrie), CLI side: `outline` scopes named instances (S3),
//! `render` draws profile grids in the profile plane (S1), sections of models made of parts
//! part by part (S4), and the minimal examples of tests/alltag/fehler never crash (F9).

use std::path::PathBuf;

use assert_cmd::Command;
use serde_json::Value;

fn alltag(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/alltag").join(name).display().to_string()
}

fn fehler(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/alltag/fehler").join(name).display().to_string()
}

fn cad() -> Command {
    let mut c = Command::cargo_bin("cad").unwrap();
    c.env("CAD_NO_DAEMON", "1");
    c
}

fn json(args: &[&str]) -> (Option<i32>, Value) {
    let out = cad().args(args).arg("--json").output().unwrap();
    let v: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{args:?}: {e}\nstderr: {}", String::from_utf8_lossy(&out.stderr)));
    (out.status.code(), v)
}

#[test]
fn s3_outline_lists_instances_named_in_modules_separately() {
    let (code, v) = json(&["outline", &alltag("pi_platte.scad")]);
    assert_eq!(code, Some(0), "{v:#}");
    let inst = v["instances"].as_array().unwrap();
    let scope = |name: &str| inst.iter().find(|i| i["name"] == name).map(|i| i["scope"].as_str().unwrap().to_string());
    assert_eq!(scope("platte").as_deref(), Some("file"));
    assert_eq!(scope("dom[0]").as_deref(), Some("module"));
    let dom = inst.iter().find(|i| i["name"] == "dom[0]").unwrap();
    assert_eq!(dom["in_module"], "pi_platte");
    // anchors of module-internal instances are no file anchors
    let anchors: Vec<&str> = v["anchors"].as_array().unwrap().iter().filter_map(|a| a["ref"].as_str()).collect();
    assert!(anchors.iter().all(|a| !a.starts_with("dom[")), "{anchors:?}");
    let out = cad().args(["outline", &alltag("pi_platte.scad")]).output().unwrap();
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("Benannte Instanzen in Modulen"), "{t}");
}

#[test]
fn s1_profile_grid_of_rotate_extrude_is_drawn_in_the_profile_plane() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("b.png");
    let (code, v) = json(&[
        "render",
        &alltag("becher.scad"),
        "--module",
        "becher",
        "--show",
        "grids",
        "--view",
        "front",
        "-o",
        png.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{v:#}");
    let notes: Vec<&str> = v["notes"].as_array().unwrap().iter().filter_map(|n| n.as_str()).collect();
    assert!(notes.iter().any(|n| n.contains("Raster p") && n.contains("Profilebene")), "{notes:?}");
    assert!(!notes.iter().any(|n| n.contains("Raster g")), "the 3D grid stays: {notes:?}");
}

#[test]
fn s4_section_of_a_model_with_parts_draws_each_part() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("g.png");
    let (code, v) = json(&[
        "render",
        &alltag("gehaeuse/gehaeuse.scad"),
        "--section",
        "x=30",
        "--view",
        "right",
        "-o",
        png.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{v:#}");
    // one drawn node per part (unterteil, deckel, 4 screws) instead of the root
    let nodes = v["visible_nodes"].as_array().map(|a| a.len()).unwrap_or(0);
    assert!(nodes >= 6, "{:#}", v["visible_nodes"]);
}

#[test]
fn f09_examples_report_errors_instead_of_crashing() {
    for f in ["f09a_absturz_tangentialer_dom.scad", "f09b_absturz_zapfen_sweep.scad"] {
        let (code, v) = json(&["check", &fehler(f)]);
        assert_eq!(code, Some(1), "{f}: {v:#}");
        assert!(v["errors"].as_u64().unwrap() >= 1, "{f}: {v:#}");
    }
}
