//! `mcad measure` on the exact B-rep (Plan 18): distance with foot points, radius, angle,
//! length, area, body, section (contours, circles, wall thickness), `--at`, JSON for agents
//! and usage errors.

use std::path::PathBuf;

use assert_cmd::Command;
use serde_json::Value;

fn example(rel: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/models").join(rel).display().to_string()
}

fn mcad() -> Command {
    let mut c = Command::cargo_bin("mcad").unwrap();
    c.env("MCAD_LANG", "de");
    c.env("MCAD_NO_DAEMON", "1");
    c
}

fn json(args: &[&str], code: i32) -> Value {
    let out = mcad().args(args).arg("--json").output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(code),
        "{args:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or(Value::Null)
}

fn text(args: &[&str], code: i32) -> String {
    let out = mcad().args(args).output().unwrap();
    assert_eq!(out.status.code(), Some(code), "{args:?}\nstderr: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap() + &String::from_utf8(out.stderr).unwrap()
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn close(a: f64, b: f64, eps: f64) {
    assert!((a - b).abs() <= eps, "{a} != {b} (±{eps})");
}

fn value(m: &Value, key: &str) -> f64 {
    f(
        &m["values"].as_array().unwrap().iter().find(|v| v["key"] == key).unwrap_or_else(|| panic!("{key}: {m}"))["value"],
    )
}

#[test]
fn housing_section_wall_and_distances() {
    let g = example("spec/housing.mcad");
    let v = json(
        &["measure", &g, "--section", "z=10", "--wall", "[0,15,10]", "--distance", "edge@[3,15,10]", "edge@[57,15,10]"],
        0,
    );
    let s = &v["section"];
    assert_eq!(s["regions"].as_array().unwrap().len(), 2);
    close(f(&s["area"]), 180.0, 1e-9);
    assert_eq!(s["exact"], true);
    close(f(&s["min_wall"]["thickness"]), 3.0, 1e-9);
    let ms = v["measurements"].as_array().unwrap();
    assert_eq!(ms.len(), 2);
    let d = &ms[0];
    assert_eq!(d["kind"], "distance");
    close(f(&d["value"]), 54.0, 1e-12);
    assert_eq!(d["exact"], true);
    let w = &ms[1];
    assert_eq!(w["kind"], "wall");
    close(f(&w["value"]), 3.0, 1e-12);
    assert_eq!(w["cli"], serde_json::json!(["--section", "z=10", "--wall", "[0,15,10]"]));
    assert_eq!(w["section"]["normal"], serde_json::json!([0.0, 0.0, 1.0]));
    // the same in 3D: parallel inner faces, provenance
    let v = json(&["measure", &g, "--distance", "face@[3,15,10]", "face@[57,15,10]"], 0);
    let d = &v["measurements"][0];
    close(f(&d["value"]), 54.0, 1e-12);
    assert!(d["details"].as_array().unwrap().iter().any(|x| x == "parallele Ebenen: Normalabstand"), "{d}");
    let origin = d["picks"][0]["origin"].as_array().unwrap();
    assert!(origin.iter().any(|x| x.as_str().unwrap().contains("Instanz k: housing")), "{origin:?}");
    assert!(origin.iter().any(|x| x == "Rasterebene g.inner_left"), "{origin:?}");
    // body of the part k: 60·30·20 − 54·30·17
    let v = json(&["measure", &g, "--body", "face@[0,15,10]"], 0);
    let b = &v["measurements"][0];
    close(value(b, "volume"), 8460.0, 0.5);
    assert_eq!(b["details"][0], "Bauteil k (Zeile 21)");
    // text
    let t = text(&["measure", &g, "--section", "z=10", "--wall", "[60,20,10]"], 0);
    assert!(t.contains("Schnitt z = 10 (entfernt z > 10): 2 Bereich(e), Fläche 180 mm²"), "{t}");
    assert!(t.contains("Wandstärke Punkt [60,20,10]: 3 mm"), "{t}");
}

#[test]
fn hole_plate_radii_bores_fillets_and_pitch() {
    let l = example("spec/hole_plate.mcad");
    let v = json(
        &[
            "measure",
            &l,
            "--radius",
            "face@[8,6,2]",
            "--radius",
            "face@[8.292893,6,3.707107]",
            "--distance",
            "center@[6,6,0]",
            "center@[18,6,0]",
            "--length",
            "edge@[9,6,0]",
        ],
        0,
    );
    // order: distances, radii, angles, lengths, area, bodies, walls
    let ms = v["measurements"].as_array().unwrap();
    let kinds: Vec<&str> = ms.iter().map(|m| m["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["distance", "radius", "radius", "length"]);
    close(f(&ms[0]["value"]), 12.0, 1e-9);
    close(f(&ms[1]["value"]), 2.0, 1e-9);
    close(value(&ms[1], "diameter"), 4.0, 1e-9);
    assert_eq!(ms[1]["label"], "R 2 · ⌀ 4");
    close(f(&ms[2]["value"]), 1.0, 1e-6);
    close(f(&ms[3]["value"]), 2.0 * std::f64::consts::PI * 3.0, 1e-9);
    // section through the bores: four circles of radius 2 (exact), plate minus bores
    let v = json(&["measure", &l, "--section", "z=2"], 0);
    let s = &v["section"];
    let circles = s["circles"].as_array().unwrap();
    assert_eq!(circles.len(), 4, "{s}");
    assert!(circles.iter().all(|c| (f(&c["radius"]) - 2.0).abs() < 1e-9 && c["full"] == true));
    close(f(&s["area"]), 58.0 * 12.0 - 4.0 * std::f64::consts::PI * 4.0, 1e-9);
    // radius of a section arc and the wall between bores (pitch 12 − 2·2)
    let v = json(&["measure", &l, "--section", "z=2", "--radius", "edge@[8,6,2]", "--wall", "[8,6,2]"], 0);
    close(f(&v["measurements"][0]["value"]), 2.0, 1e-12);
    close(f(&v["measurements"][1]["value"]), 8.0, 1e-9);
}

#[test]
fn chamfer_angle_and_at() {
    let c = example("spec/chamfers.mcad");
    let v = json(&["measure", &c, "--angle", "face@[39.5,12,9.5]", "face@[20,12,10]"], 0);
    let a = &v["measurements"][0];
    close(f(&a["value"]), 45.0, 1e-9);
    assert_eq!(a["unit"], "°");
    close(value(a, "opening"), 135.0, 1e-9);
    let v = json(&["measure", &c, "--at", "[40,25,10]"], 0);
    let els = v["at"][0]["elements"].as_array().unwrap();
    assert_eq!(els[0]["kind"], "vertex");
    assert!(els.iter().any(|e| e["kind"] == "face"));
    assert!(els[0]["reference"].as_str().unwrap().starts_with("vertex@["));
    let t = text(&["measure", &c, "--at", "[20,12,10]"], 0);
    assert!(t.contains("face@[20,12,10]"), "{t}");
    assert!(t.contains("Rasterebene g.o"), "{t}");
}

#[test]
fn usage_errors() {
    let g = example("spec/housing.mcad");
    // --wall needs a section
    json(&["measure", &g, "--wall", "[0,15,10]"], 2);
    // face@ inside a section, unknown prefix
    json(&["measure", &g, "--section", "z=10", "--distance", "face@[3,15,10]", "[0,0,10]"], 2);
    json(&["measure", &g, "--radius", "rim@[0,0,0]"], 2);
    // radius of a plane
    let t = text(&["measure", &g, "--radius", "face@[3,15,10]"], 2);
    assert!(t.contains("kein Radius"), "{t}");
    // plane misses the model
    json(&["measure", &g, "--section", "z=100"], 2);
}
