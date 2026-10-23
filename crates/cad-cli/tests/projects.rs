//! CLI on projects of several files (`include`/`use` from the file system) and parts
//! (`export --part`, `--all-parts`).

use std::path::PathBuf;

use assert_cmd::Command;
use serde_json::Value;

fn data(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name).display().to_string()
}

fn example() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/projekt-gehaeuse/gehaeuse.scad")
        .display()
        .to_string()
}

fn run(args: &[&str], code: i32) -> (Value, String) {
    let out = Command::cargo_bin("cad").unwrap().env("CAD_NO_DAEMON", "1").args(args).arg("--json").output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(code),
        "{args:?}\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let t = Command::cargo_bin("cad").unwrap().env("CAD_NO_DAEMON", "1").args(args).output().unwrap();
    (v, String::from_utf8_lossy(&t.stdout).into_owned() + &String::from_utf8_lossy(&t.stderr))
}

#[test]
fn check_and_eval_read_included_and_used_files() {
    let (v, _) = run(&["check", &example()], 0);
    assert_eq!(v["errors"], 0);
    let (v, t) = run(&["eval", &example()], 0);
    let sc = &v["scene"];
    let files: Vec<&str> = sc["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap()).collect();
    assert_eq!(files, ["gehaeuse.scad", "konstanten.scad", "lib/teile.scad"]);
    let parts: Vec<&str> = sc["parts"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert_eq!(parts, ["gehaeuse", "deckel"]);
    let deckel = &sc["parts"][1];
    assert_eq!(deckel["bounds"]["min"][1], 50.0);
    assert!(deckel["volume"].as_f64().unwrap() > 7000.0);
    // Hints of the included file carry its file id; the text names the file.
    let h = sc["hints"].as_array().unwrap().iter().find(|h| h["text"] == "60").unwrap();
    assert_eq!(h["range"]["file"], 1);
    assert!(t.contains("konstanten.scad Z.2 breite = 60;"), "{t}");
    assert!(t.contains("dom[2].oben = [5, 35, 21]"), "{t}");
    assert!(t.contains("Bauteile:"), "{t}");
    // Include statements for Ctrl+click.
    let inc = sc["includes"].as_array().unwrap();
    assert!(inc.iter().any(|i| i["target"] == "lib/teile.scad" && i["is_use"] == true && i["found"] == true));
}

#[test]
fn diagnostics_name_their_file_and_line() {
    let (v, t) = run(&["check", "--no-kernel", &data("projekt/fehler.scad")], 1);
    let d = v["diagnostics"].as_array().unwrap();
    let missing = d.iter().find(|d| d["message"].as_str().unwrap().contains("fehlt.scad")).unwrap();
    assert_eq!(missing["range"]["line"], 1);
    assert!(missing.get("file").is_none());
    let lib = d.iter().find(|d| d["message"].as_str().unwrap().contains("unbekannt")).unwrap();
    assert_eq!(lib["file"], "lib/teile.scad");
    assert_eq!(lib["range"]["line"], 3);
    assert!(t.contains("fehler.scad:1:"), "{t}");
    assert!(t.contains("lib/teile.scad:3:"), "{t}");
}

#[test]
fn export_single_parts_and_all_parts() {
    let dir = tempfile::tempdir().unwrap();
    let stl = dir.path().join("deckel.stl");
    let (v, _) = run(&["export", &example(), "--part", "deckel", "-o", &stl.display().to_string()], 0);
    let bytes = std::fs::read(&stl).unwrap();
    let tris = v["triangles"].as_u64().unwrap();
    assert_eq!(bytes.len() as u64, 84 + 50 * tris);
    assert_eq!(v["parts"][0]["name"], "deckel");
    // STL coordinates are in the model system: the lid lies at y >= 50.
    let mut min_y = f32::INFINITY;
    for t in 0..tris as usize {
        for k in 0..3 {
            let o = 84 + 50 * t + 12 + 12 * k + 4;
            min_y = min_y.min(f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap()));
        }
    }
    assert!((min_y - 50.0).abs() < 1e-3, "{min_y}");

    // Two parts into one 3MF: one object each.
    let mf = dir.path().join("beide.3mf");
    run(&["export", &example(), "--part", "gehaeuse", "--part", "deckel", "-o", &mf.display().to_string()], 0);
    let zip = std::fs::read(&mf).unwrap();
    let text = String::from_utf8_lossy(&zip);
    assert_eq!(text.matches("<object ").count(), 2);
    assert!(text.contains("name=\"gehaeuse\"") && text.contains("name=\"deckel\""));

    // All parts into a folder.
    let out = dir.path().join("teile");
    let (v, t) = run(&["export", &example(), "--all-parts", "-o", &out.display().to_string()], 0);
    assert_eq!(v["parts"].as_array().unwrap().len(), 2);
    assert!(out.join("gehaeuse.stl").exists() && out.join("deckel.stl").exists());
    assert!(t.contains("deckel:"), "{t}");
    // Indexed instances: file names k_0 … k_2.
    let out = dir.path().join("reihe");
    run(
        &["export", &data("projekt/reihe.scad"), "--all-parts", "--format", "3mf", "-o", &out.display().to_string()],
        0,
    );
    for i in 0..3 {
        assert!(out.join(format!("k_{i}.3mf")).exists());
    }
    // STEP of one part (the same writer as the whole model), in model coordinates.
    let step = dir.path().join("deckel.step");
    let (v, _) = run(&["export", &example(), "--part", "deckel", "-o", &step.display().to_string()], 0);
    assert_eq!(v["format"], "step");
    let m = cad_step::p21::parse(&std::fs::read_to_string(&step).unwrap()).unwrap();
    assert!(m.schema.starts_with("AP242"), "{}", m.schema);
    assert_eq!(m.count("MANIFOLD_SOLID_BREP"), 1);
    let product = m.of_type("PRODUCT").next().unwrap();
    assert_eq!(product.args()[0], cad_step::p21::Value::Str("deckel".into()));
    let ys: Vec<f64> = m
        .of_type("VERTEX_POINT")
        .filter_map(|e| e.args().get(1).and_then(|r| r.as_ref_id()).and_then(|id| m.coords(id)))
        .map(|c| c[1])
        .collect();
    assert!(!ys.is_empty());
    let min_y = ys.iter().cloned().fold(f64::INFINITY, f64::min);
    assert!((min_y - 50.0).abs() < 1e-9, "{min_y}");
    // Two parts into one STEP: two solids; AP214 through --schema.
    let both = dir.path().join("beide.stp");
    run(
        &[
            "export",
            &example(),
            "--part",
            "gehaeuse",
            "--part",
            "deckel",
            "--schema",
            "ap214",
            "-o",
            &both.display().to_string(),
        ],
        0,
    );
    let m = cad_step::p21::parse(&std::fs::read_to_string(&both).unwrap()).unwrap();
    assert!(m.schema.starts_with("AUTOMOTIVE_DESIGN"), "{}", m.schema);
    // the housing's module body is an implicit union of several solids
    assert!(m.count("MANIFOLD_SOLID_BREP") >= 2);
    // All parts as STEP files.
    let out_step = dir.path().join("teile-step");
    let (v, _) =
        run(&["export", &example(), "--all-parts", "--format", "step", "-o", &out_step.display().to_string()], 0);
    assert_eq!(v["parts"].as_array().unwrap().len(), 2);
    for n in ["gehaeuse", "deckel"] {
        let t = std::fs::read_to_string(out_step.join(format!("{n}.step"))).unwrap();
        let m = cad_step::p21::parse(&t).unwrap();
        assert!(m.count("MANIFOLD_SOLID_BREP") >= 1, "{n}");
    }

    // Unknown part / no parts.
    run(&["export", &example(), "--part", "fehlt", "-o", &stl.display().to_string()], 3);
    run(&["export", &data("platte.scad"), "--all-parts", "-o", &out.display().to_string()], 2);
}
