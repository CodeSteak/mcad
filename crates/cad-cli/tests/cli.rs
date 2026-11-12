//! End-to-end tests of the `mcad` binary on small files in `tests/data/`.

use std::path::PathBuf;

use assert_cmd::Command;
use serde_json::Value;

fn data(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name).display().to_string()
}

fn mcad() -> Command {
    // without daemon (tests/daemon.rs covers it, with a socket directory of its own)
    let mut c = Command::cargo_bin("mcad").unwrap();
    c.env("MCAD_LANG", "de");
    c.env("MCAD_NO_DAEMON", "1");
    c
}

/// Runs with `--json`, checks the exit code, returns the parsed output.
fn json(args: &[&str], code: i32) -> Value {
    let out = mcad().args(args).arg("--json").output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(code),
        "{args:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], 3);
    assert_eq!(v["exit_code"], code);
    assert_eq!(v["ok"], code == 0);
    v
}

fn text(args: &[&str], code: i32) -> String {
    let out = mcad().args(args).output().unwrap();
    assert_eq!(out.status.code(), Some(code), "{args:?}\nstderr: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap() + &String::from_utf8(out.stderr).unwrap()
}

#[test]
fn check_ok_syntax_error_and_kernel_error() {
    let v = json(&["check", &data("housing.mcad")], 0);
    assert_eq!(v["command"], "check");
    assert_eq!(v["errors"], 0);

    let v = json(&["check", &data("syntax_error.mcad")], 1);
    let d = &v["diagnostics"][0];
    assert_eq!(d["severity"], "error");
    assert_eq!(d["source"], "syntax");
    assert_eq!(d["range"]["line"], 3);
    let t = text(&["check", &data("syntax_error.mcad")], 1);
    assert!(t.contains("syntax_error.mcad:3:"), "{t}");

    let v = json(&["check", &data("fillet_too_large.mcad")], 1);
    let d = &v["diagnostics"][0];
    assert_eq!(d["source"], "kernel");
    assert_eq!(d["range"]["line"], 2);
    // without the kernel the file is fine
    json(&["check", "--no-kernel", &data("fillet_too_large.mcad")], 0);
}

#[test]
fn eval_scene_text_and_json() {
    let v = json(&["eval", &data("housing.mcad")], 0);
    let scene = &v["scene"];
    assert_eq!(scene["complete"], true);
    let anchors = scene["anchors"].as_array().unwrap();
    let corner = anchors.iter().find(|a| a["text"] == "k.corner").unwrap();
    assert_eq!(corner["point"], serde_json::json!([60.0, 30.0, 20.0]));
    assert!(scene["nodes"].as_array().unwrap().iter().all(|n| n["status"] == "ok"));

    let t = text(&["eval", &data("housing.mcad")], 0);
    assert!(t.contains("Kontext: Datei"), "{t}");
    assert!(t.contains("difference"), "{t}");
    assert!(t.contains("k.corner = [60, 30, 20]"), "{t}");
    assert!(t.contains("Raster:"), "{t}");

    // module context with -D
    let v = json(&["eval", &data("housing.mcad"), "--module", "housing", "-D", "w=80"], 0);
    assert_eq!(v["context"], "housing");
    let params = v["scene"]["context"]["params"].as_array().unwrap();
    let w = params.iter().find(|p| p["name"] == "w").unwrap();
    assert_eq!(w["value"], 80.0);
    assert_eq!(w["overridden"], true);
    let t = text(&["eval", &data("plate.mcad")], 0);
    assert!(t.contains("ECHO: \"done\", 4"), "{t}");
}

#[test]
fn inspect_line() {
    // line 11 is the outer box inside module housing: module context
    let v = json(&["inspect", &data("housing.mcad"), "--line", "11"], 0);
    assert_eq!(v["context"], "housing");
    let nodes = v["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["label"], "box");
    let vol = nodes[0]["volume"].as_f64().unwrap();
    assert!((vol - 40.0 * 30.0 * 20.0).abs() < 1e-6, "{vol}");
    // in the file context the same line has no own node... the call on line 21 has
    let v = json(&["inspect", &data("housing.mcad"), "--line", "21"], 0);
    assert_eq!(v["context"], Value::Null);
    let n = &v["nodes"][0];
    assert_eq!(n["label"], "housing");
    assert_eq!(n["instance"], "k");
    assert_eq!(n["world_bounds"]["max"][0], 60.0);
    // zone block: selected edges
    let v = json(&["inspect", &data("fillet_zones.mcad"), "--line", "12"], 0);
    let n = &v["nodes"][0];
    assert_eq!(n["label"], "fillet");
    assert_eq!(n["selected_edge_count"], 2);
    // the edges themselves: polylines in world coordinates, as in `diff-reference` and the scene
    let edges = n["selected_edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
    assert!(edges.iter().all(|e| e.as_array().unwrap().len() >= 2 && e[0].as_array().unwrap().len() == 3), "{edges:?}");
    assert!(n["hints"].as_array().unwrap().iter().any(|h| h == "2 Kanten"));
    // variable line: value and handle-free
    let v = json(&["inspect", &data("plate.mcad"), "--line", "2"], 0);
    assert_eq!(v["nodes"].as_array().unwrap().len(), 0);
    assert_eq!(v["hints"][0]["text"], "4");
    let t = text(&["inspect", &data("fillet_too_large.mcad"), "--line", "2"], 1);
    assert!(t.contains("Verrundung gescheitert"), "{t}");
}

#[test]
fn render_png() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("g.png");
    let o = out.display().to_string();
    let v = json(&["render", &data("housing.mcad"), "-o", &o, "--size", "320x240"], 0);
    assert_eq!(v["width"], 320);
    assert!(v["triangles"].as_u64().unwrap() > 10);
    let png = std::fs::read(&out).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    // highlighted line: subtracted body of line 12 (module context)
    let v = json(&["render", &data("housing.mcad"), "-o", &o, "--line", "12", "--view", "top"], 0);
    assert_eq!(v["highlighted"].as_array().unwrap().len(), 1);
    assert_eq!(v["context"], "housing");
    // usage errors
    json(&["render", &data("housing.mcad"), "-o", &o, "--view", "oblique"], 2);
    json(&["render", &data("housing.mcad"), "-o", &o, "--size", "huge"], 2);
    // syntax error: no image, exit 1
    let o2 = dir.path().join("x.png");
    json(&["render", &data("syntax_error.mcad"), "-o", &o2.display().to_string()], 1);
    assert!(!o2.exists());
}

#[test]
fn export_stl_and_3mf() {
    let dir = tempfile::tempdir().unwrap();
    let stl = dir.path().join("part.stl");
    let v = json(&["export", &data("plate.mcad"), "-o", &stl.display().to_string()], 0);
    let tris = v["triangles"].as_u64().unwrap();
    let bytes = std::fs::read(&stl).unwrap();
    assert_eq!(bytes.len() as u64, 84 + 50 * tris);
    assert_eq!(v["bytes"], bytes.len());
    let vol = v["volume"].as_f64().unwrap();
    assert!(vol > 5000.0 && vol < 6000.0, "{vol}");
    let mf = dir.path().join("part.3mf");
    json(&["export", &data("plate.mcad"), "-o", &mf.display().to_string()], 0);
    assert_eq!(&std::fs::read(&mf).unwrap()[..2], b"PK");
    json(&["export", &data("plate.mcad"), "-o", &dir.path().join("part.obj").display().to_string()], 2);
}

#[test]
fn export_step() {
    let dir = tempfile::tempdir().unwrap();
    let step = dir.path().join("part.step");
    let v = json(&["export", &data("plate.mcad"), "-o", &step.display().to_string()], 0);
    assert_eq!(v["format"], "step");
    assert_eq!(v["triangles"], 0);
    let st = &v["step"];
    assert_eq!(st["solids"], 1);
    let text = std::fs::read_to_string(&step).unwrap();
    assert_eq!(v["bytes"], text.len());
    let m = cad_step::p21::parse(&text).unwrap();
    assert!(m.schema.starts_with("AP242"), "{}", m.schema);
    assert_eq!(m.count("ADVANCED_FACE") as u64, st["faces"].as_u64().unwrap());
    assert_eq!(m.count("EDGE_CURVE") as u64, st["edges"].as_u64().unwrap());
    assert_eq!(m.count("MANIFOLD_SOLID_BREP"), 1);
    assert!(v["area"].as_f64().unwrap() > 0.0);
    assert!(v["bounds"]["min"].is_array());
    // .stp and AP214
    let stp = dir.path().join("part.stp");
    json(&["export", &data("plate.mcad"), "-o", &stp.display().to_string(), "--schema", "ap214"], 0);
    let m = cad_step::p21::parse(&std::fs::read_to_string(&stp).unwrap()).unwrap();
    assert!(m.schema.starts_with("AUTOMOTIVE_DESIGN"), "{}", m.schema);
    json(&["export", &data("plate.mcad"), "-o", &stp.display().to_string(), "--schema", "ap203"], 2);
}

/// STEP export: one named body per part and per top-level object outside the parts, also when
/// they only touch; `--merge` fuses everything into one body as before.
#[test]
fn export_step_separate_bodies() {
    let dir = tempfile::tempdir().unwrap();
    let step = dir.path().join("b.step").display().to_string();
    let v = json(&["export", &data("touching.mcad"), "-o", &step], 0);
    let st = &v["step"];
    assert_eq!(st["solids"], 4, "{st}");
    assert_eq!(st["bodies"], serde_json::json!(["a", "b", "box", "box 2"]));
    let text = std::fs::read_to_string(&step).unwrap();
    let m = cad_step::p21::parse(&text).unwrap();
    assert_eq!(m.count("MANIFOLD_SOLID_BREP"), 4);
    for name in ["'a'", "'b'", "'box'", "'box 2'"] {
        assert!(text.contains(&format!("MANIFOLD_SOLID_BREP({name},")), "{name}");
    }
    // touching only: no overlap note, the union volume
    assert_eq!(v["notes"], serde_json::json!([]));
    assert!((v["volume"].as_f64().unwrap() - 2000.0 - 1000.0).abs() < 1e-6, "{v}");
    // the colour of the red cube stays on its faces
    assert!(text.contains("COLOUR_RGB"));
    // --merge: one fused body (faces of a and b merge)
    let v = json(&["export", &data("touching.mcad"), "-o", &step, "--merge"], 0);
    assert_eq!(v["step"]["solids"], 1, "{}", v["step"]);
    // without parts: the top-level children; a single one stays the product's body
    let src = dir.path().join("two.scad");
    std::fs::write(&src, "cube(10);\ntranslate([10, 0, 0]) cube(10);\n").unwrap();
    let v = json(&["export", &src.display().to_string(), "-o", &step], 0);
    assert_eq!(v["step"]["bodies"], serde_json::json!(["box", "box 2"]));
    std::fs::write(&src, "cube(10);\n").unwrap();
    let v = json(&["export", &src.display().to_string(), "-o", &step], 0);
    assert_eq!(v["step"]["bodies"], serde_json::json!(["b 1"]));
    // overlapping objects are fused into one body, a touching one stays apart
    std::fs::write(&src, "color(\"red\") cube(10);\ntranslate([5, 0, 0]) cube(10);\ntranslate([15, 0, 0]) cube(10);\n")
        .unwrap();
    let v = json(&["export", &src.display().to_string(), "-o", &step], 0);
    assert_eq!(v["step"]["solids"], 2, "{v}");
    assert_eq!(v["step"]["bodies"], serde_json::json!(["box + box 2", "box 3"]));
    assert_eq!(v["notes"].as_array().unwrap().len(), 1, "{v}");
    assert!(v["notes"][0].as_str().unwrap().contains("als ein Körper „box + box 2“"), "{v}");
    let text = std::fs::read_to_string(&step).unwrap();
    assert!(text.contains("COLOUR_RGB"), "the red faces keep their colour in the fused body");
    let t = text_of(&["export", &src.display().to_string(), "-o", &step]);
    assert!(
        t.contains("2 Körper (box + box 2, box 3)") && t.contains("Hinweis: Körper box, box 2 überschneiden sich"),
        "{t}"
    );
    // --part: the parts by name
    let v = json(&["export", &data("touching.mcad"), "--part", "a", "--part", "b", "-o", &step], 0);
    let text = std::fs::read_to_string(&step).unwrap();
    assert!(text.contains("MANIFOLD_SOLID_BREP('a',") && text.contains("MANIFOLD_SOLID_BREP('b',"), "{v}");
}

fn text_of(args: &[&str]) -> String {
    text(args, 0)
}

/// solidcore, the kernel of every command: volumes of curved solids within 1e-6 of the exact
/// ones, render, measure, STL and STEP from its B-rep (meshes by cad-tess, plan 2.8).
#[test]
fn solidcore_evaluates() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("box.mcad");
    std::fs::write(&f, "cube([10, 20, 30]);\ntranslate([20, 0, 0]) cylinder(h = 10, r = 5);\n").unwrap();
    let f = f.display().to_string();
    let v = json(&["eval", &f], 0);
    assert!(v["kernel_id"].as_str().unwrap().starts_with("solidcore-"), "{v}");
    let v = json(&["measure", &f, "--volume"], 0);
    let exact = 6000.0 + std::f64::consts::PI * 25.0 * 10.0;
    let vol = v["volume"]["volume"].as_f64().unwrap();
    assert!((vol - exact).abs() < 1e-6 * exact, "{vol} exact {exact}");
    // the cube alone is exact
    let out = mcad().args(["inspect", &f, "--line", "1", "--json"]).output().unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!((v["nodes"][0]["volume"].as_f64().unwrap() - 6000.0).abs() < 1e-9, "{v}");
    let png = dir.path().join("b.png").display().to_string();
    json(&["render", &f, "-o", &png], 0);
    let v = json(&["measure", &f, "--at", "[10,10,30]"], 0);
    assert!(v["at"][0]["elements"].as_array().unwrap().iter().any(|e| e["kind"] == "face"), "{v}");
    let stl = dir.path().join("b.stl").display().to_string();
    let v = json(&["export", &f, "-o", &stl], 0);
    // the mesh encloses the exact volume within tol · area (0.01 mm)
    let (vol, area) = (v["volume"].as_f64().unwrap(), v["area"].as_f64().unwrap());
    assert!((vol - exact).abs() <= 0.01 * area && v["triangles"].as_u64().unwrap() > 0, "{v}");
    // a wrong reference is a usage error before the kernel is asked
    json(&["measure", &f, "--at", "bogus"], 2);
    json(&["measure", &f, "--distance", "corner@[0,0,0]", "[1,1,1]"], 2);
    // STEP from the neutral B-rep (plan 1.11)
    let step = dir.path().join("b.step").display().to_string();
    let v = json(&["export", &f, "-o", &step], 0);
    assert_eq!(v["step"]["faces"], 9, "{v}");
    assert!((v["volume"].as_f64().unwrap() - exact).abs() < 1e-6 * exact, "{v}");
}

/// Contact kinds: a ball in its seat (concentric spheres) and a cone in a hole of nearly its
/// radius touch over an area, a torus on a plate and a cube tilted by 30° on a plate's rim along
/// a line.
#[test]
fn contact_kinds() {
    let dir = tempfile::tempdir().unwrap();
    for (src, want) in [
        ("assert_touches(tol = 0.01) { difference() { cube(20, center = true); sphere(5); } sphere(5); }\n", "face"),
        (
            "assert_touches(tol = 0.01) { difference() { cylinder(r = 10, h = 20); translate([0, 0, -1]) cylinder(r = 5.002, h = 22); } cylinder(r1 = 5, r2 = 5.002, h = 20); }\n",
            "face",
        ),
        ("assert_touches(contact = \"any\") { torus(20, 2); translate([-30, -30, 2]) cube([60, 60, 1]); }\n", "line"),
        (
            "assert_touches(tol = 0.01) { translate([-20, -20, -2]) cube([40, 40, 2]); translate([-5, 20, 0]) rotate([-30, 0, 0]) cube(10); }\n",
            "line",
        ),
    ] {
        let f = dir.path().join("touch.mcad");
        std::fs::write(&f, src).unwrap();
        let f = f.display().to_string();
        let v = json(&["check", &f], 0);
        let p = &v["asserts"][0]["pairs"][0];
        assert_eq!(p["contact"], want, "{src}: {p}");
    }
}

fn reference() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/kernel-reference.json").display().to_string()
}

/// `diff-reference --mesh` (plan 2.7) on a few files of the reference: solidcore's meshes
/// watertight and within the tolerance of the exact surfaces; files outside the reference and
/// bad tolerances are usage errors.
#[test]
fn diff_reference_mesh() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let r = reference();
    let mut args = vec![
        "diff-reference".to_string(),
        r.clone(),
        "--mesh".into(),
        "--mesh-tol".into(),
        "0.02".into(),
        "--json".into(),
    ];
    for f in [
        "tests/models/openscad-compat/02_sphere_variants.scad",
        "tests/models/openscad-compat/12_linear_extrude_twist.scad",
        "tests/models/spec/fillet_zones.mcad",
    ] {
        args.push(root.join(f).display().to_string());
    }
    let out = mcad().args(&args).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["command"], "diff-reference");
    assert!(v["reference_kernel"].as_str().unwrap().starts_with("smlib-"), "{}", v["reference_kernel"]);
    assert_eq!(v["files"].as_array().unwrap().len(), 3);
    assert_eq!(v["summary"]["same"], v["summary"]["nodes"], "{}", v["summary"]);
    let m = &v["summary"]["mesh"];
    assert_eq!(m["verdicts"]["ok"], 3, "{m}");
    assert_eq!((m["files"].as_u64(), m["watertight"].as_u64()), (Some(3), Some(3)), "{m}");
    assert!(m["chord_max"].as_f64().unwrap() <= 0.022, "{m}");
    assert!(m["volume_ratio_max"].as_f64().unwrap() <= 1.0, "{m}");
    for f in v["files"].as_array().unwrap() {
        let s = &f["mesh"];
        assert!(s["triangles"].as_u64().unwrap() > 0 && s["error"].is_null(), "{f}");
    }
    // the tolerance must be positive
    let out = mcad().args(["diff-reference", &r, "--mesh", "--mesh-tol", "0", &args[6]]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    // a file the reference does not hold
    let v = json(&["diff-reference", &r, &data("plate.mcad")], 2);
    assert!(v["error"].as_str().unwrap().contains("plate.mcad"), "{v}");
    // not a reference file
    let v = json(&["diff-reference", &data("plate.mcad")], 2);
    assert!(v["error"].as_str().unwrap().contains("plate.mcad"), "{v}");
}

/// The nodes of the corpus where solidcore does not give SMLib's result, as (file, line, label,
/// verdict) of `diff-reference`: a variable radius with different laws on several
/// edges (unsupported, and the nodes above it blocked), a union of three extrusions where
/// solidcore loses faces (`sketch_cable_duct`, upstream), a Boolean SMLib fails and solidcore
/// does not (`f11`, a gain), a shell SMLib fails and solidcore builds since `55d0e76` (`f12`, a
/// gain: walls 2 mm, the volume of the exact walls), and a regression of `55d0e76`: the round of
/// `wall_hook`'s `union(fillet = 2.5)` where the hook's tube enters the knob overlaps itself
/// (upstream #2863, a fillet solver's guess from the edge use's UV trim; 2.25 builds;
/// `docs/solidcore-migration.md`, "Status").
const REFERENCE_EXCEPTIONS: &[(&str, u32, &str, &str)] = &[
    ("everyday/bugs/f11_pin_coincident.mcad", 1, "group", "differ"),
    ("everyday/bugs/f12_shell_loft_square_circle.mcad", 1, "group", "differ"),
    ("everyday/bugs/f12_shell_loft_square_circle.mcad", 6, "shell", "differ"),
    ("models/everyday/wall_hook.mcad", 1, "group", "blocked"),
    ("models/everyday/wall_hook.mcad", 13, "difference", "blocked"),
    ("models/everyday/wall_hook.mcad", 14, "union", "failed"),
    ("models/everyday/wall_hook.mcad", 42, "wall_hook", "blocked"),
    ("models/spec/sketch_cable_duct.mcad", 1, "group", "differ"),
    ("models/spec/variable_radius.mcad", 1, "group", "blocked"),
    ("models/spec/variable_radius.mcad", 11, "fillet", "unsupported"),
    ("models/spec/variable_radius.mcad", 16, "difference", "unsupported"),
    ("models/spec/variable_radius.mcad", 16, "translate", "blocked"),
    ("models/spec/variable_radius.mcad", 22, "fillet", "unsupported"),
    ("models/spec/variable_radius.mcad", 22, "translate", "blocked"),
];

/// solidcore against the frozen SMLib results (`tests/kernel-reference.json`, 101 files of
/// `tests/models` and `tests/everyday/bugs`): every node is the same by the rules of
/// `diff-reference` (as `diff-kernels` compared both kernels before SMLib was removed) but the
/// known exceptions ([`REFERENCE_EXCEPTIONS`]; a new difference fails, and so does an exception
/// that went away, so the list stays exact), and every result meshes watertight within the
/// tolerance.
#[test]
fn solidcore_against_the_reference() {
    let out = mcad().args(["diff-reference", &reference(), "--mesh", "--json"]).output().unwrap();
    let code = out.status.code().unwrap();
    assert!(code == 0 || code == 1, "exit {code}: {}", String::from_utf8_lossy(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let files = v["files"].as_array().unwrap();
    assert_eq!(files.len(), 101);
    let mut found: Vec<(String, u32, String, String)> = Vec::new();
    for f in files {
        let name = f["file"].as_str().unwrap().replace('\\', "/");
        let short = name.rsplit("tests/").next().unwrap().to_string();
        for n in f["nodes"].as_array().unwrap() {
            let verdict = n["verdict"].as_str().unwrap();
            if verdict != "same" && verdict != "skipped" {
                found.push((
                    short.clone(),
                    n["line"].as_u64().unwrap() as u32,
                    n["label"].as_str().unwrap().into(),
                    verdict.into(),
                ));
            }
        }
    }
    found.sort();
    let mut want: Vec<(String, u32, String, String)> =
        REFERENCE_EXCEPTIONS.iter().map(|(f, l, n, v)| (f.to_string(), *l, n.to_string(), v.to_string())).collect();
    want.sort();
    assert_eq!(found, want, "nodes that differ from the reference (left) against the known exceptions (right)");
    let s = &v["summary"];
    assert!(s["nodes"].as_u64().unwrap() > 2500, "{s}");
    // every rule is in use (curved: blended results use rule blend)
    let r = &s["volume_rules"];
    assert!(r["exact"].as_u64().unwrap() >= 110 && r["operands"].as_u64().unwrap() >= 45, "{r}");
    assert!(r["curved"].as_u64().unwrap() <= 40, "{r}");
    assert!((40..=100).contains(&r["blend"].as_u64().unwrap_or(0)), "{r}");
    for f in files {
        for n in f["nodes"].as_array().unwrap() {
            // a box is exact in both kernels
            if n["verdict"] == "same" && n["label"] == "box" {
                assert_eq!(n["volume_check"]["rule"], "relative", "{n}");
            }
        }
    }
    // every result meshes watertight with solidcore within the tolerance of 0.05 of the exact
    // surfaces; one file has no B-rep result (skipped)
    let m = &s["mesh"];
    assert_eq!(m["verdicts"]["ok"].as_u64().unwrap() + 1, files.len() as u64, "{m}");
    assert_eq!(m["watertight"], m["files"], "{m}");
    assert!(m["chord_max"].as_f64().unwrap() <= 0.051, "{m}");
    assert!(m["volume_ratio_max"].as_f64().unwrap() <= 1.0, "{m}");
    // queries work on the shape's own body: few copies (2427 at f4f05b1 with the Boolean
    // checks' retries)
    let copies = s["cost"]["copies"].as_u64().unwrap();
    assert!(copies > 0 && copies < 3000, "{copies}");
}

#[test]
fn usage_errors() {
    json(&["check", "/nonexistent/file.scad"], 2);
    // -D without a value
    json(&["eval", &data("housing.mcad"), "-D", "w"], 2);
    // clap errors exit with 2 as well
    let out = mcad().args(["render"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn stdin_source() {
    let out = mcad().args(["check", "-", "--json"]).write_stdin("cube(5);\n").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["file"], "<stdin>");
}

#[test]
fn sketch_and_namespaces() {
    use std::f64::consts::PI;
    let want = (40.0 * 20.0 + PI * 400.0 / 2.0) * 5.0;
    // a sketch with an arc: exact volume
    let v = json(
        &[
            "export",
            &data("sketch.mcad"),
            "-o",
            &tempfile::tempdir().unwrap().path().join("s.step").display().to_string(),
        ],
        0,
    );
    let vol = v["volume"].as_f64().unwrap();
    assert!((vol - want).abs() < 1e-6 * want, "{vol} {want}");
    // the function of the namespace
    let t = text(&["eval", &data("sketch.mcad")], 0);
    assert!(t.contains(&format!("ECHO: {}", cad_eval::fmt_num(40.0 * 20.0 + PI * 200.0))), "{t}");
    // outline: qualified module and function names
    let v = json(&["outline", &data("sketch.mcad")], 0);
    let q: Vec<&str> = v["modules"].as_array().unwrap().iter().map(|m| m["qualified"].as_str().unwrap()).collect();
    assert_eq!(q, ["parts", "parts.plate"]);
    let f: Vec<&str> = v["functions"].as_array().unwrap().iter().map(|m| m["name"].as_str().unwrap()).collect();
    assert_eq!(f, ["parts.area"]);
    let t = text(&["outline", &data("sketch.mcad")], 0);
    assert!(t.contains("parts.plate(a = 40: Zahl"), "{t}");
    // --module with a path: the inner module as context
    let v = json(&["eval", &data("sketch.mcad"), "--module", "parts.plate", "-D", "a=30"], 0);
    assert_eq!(v["scene"]["context"]["path"], serde_json::json!(["parts", "plate"]));
    let v = json(&["inspect", &data("sketch.mcad"), "--module", "parts.plate", "--line", "5"], 0);
    let n = &v["nodes"][0];
    assert_eq!(n["label"], "linear_extrude");
    let vol = n["volume"].as_f64().unwrap();
    assert!((vol - want).abs() < 1e-6 * want, "{vol}");
    // a segment line: the sketch node (render highlights it)
    let dir = tempfile::tempdir().unwrap();
    let o = dir.path().join("s.png").display().to_string();
    json(&["render", &data("sketch.mcad"), "-o", &o, "--line", "9", "--size", "320x240"], 0);
    // errors per segment line
    let src = dir.path().join("error.mcad");
    std::fs::write(&src, "linear_extrude(1) sketch {\n start([0, 0]);\n arc(to = [10, 0], r = 2);\n close();\n}\n")
        .unwrap();
    let v = json(&["check", &src.display().to_string()], 1);
    let d = &v["diagnostics"][0];
    assert_eq!(d["range"]["line"], 3);
    assert!(d["message"].as_str().unwrap().contains("halbe Sehne"), "{d}");
}

#[test]
fn sketch_corners_offset_stroke() {
    use std::f64::consts::PI;
    let f = data("sketch_corners.mcad");
    let k = 4.0 - PI;
    let sketch = 600.0 - 2.0 * k * 16.0 / 4.0 - 4.5;
    let fillet = 600.0 - k * 9.0;
    let offset = 24.0 * 14.0 - k * 4.0;
    let stroke = PI / 4.0 * (11.0f64.powi(2) - 9.0f64.powi(2));
    let want = 2.0 * (sketch + fillet + offset + stroke);
    let dir = tempfile::tempdir().unwrap();
    let v = json(&["export", &f, "-o", &dir.path().join("e.step").display().to_string()], 0);
    let vol = v["volume"].as_f64().unwrap();
    assert!((vol - want).abs() < 1e-5 * want, "{vol} {want}");
    // the inline hint of fillet2d and the corner's radius handle
    let v = json(&["inspect", &f, "--line", "12"], 0);
    let n = v["nodes"].as_array().unwrap().iter().find(|n| n["label"] == "fillet2d").expect("fillet2d node").clone();
    assert_eq!(n["hints"], serde_json::json!(["4 Ecken"]));
    let v = json(&["inspect", &f, "--line", "5"], 0);
    let h = v["handles"].as_array().unwrap();
    assert!(h.iter().any(|h| h["kind"] == "radius" || h["kind"]["kind"] == "radius"), "{v}");
    // render of the offset line: the offset contour (dashed) and the input contour as lines
    let o = dir.path().join("o.png").display().to_string();
    let v = json(&["render", &f, "-o", &o, "--line", "13", "--view", "top", "--show", "handles"], 0);
    assert_eq!(v["overlays"]["lines"], 2, "{v}");
    assert!(v["overlays"]["handles"].as_u64().unwrap() > 0);
    // a radius too large for the corner: error at the corner line
    let src = dir.path().join("too_large.scad");
    std::fs::write(&src, "linear_extrude(1) sketch {\n start([0, 0]);\n line(to = [10, 0]);\n corner(r = 12);\n line(to = [10, 5]);\n line(to = [0, 5]);\n close();\n}\n").unwrap();
    let v = json(&["check", &src.display().to_string()], 1);
    let d = &v["diagnostics"][0];
    assert_eq!(d["range"]["line"], 4);
    assert!(d["message"].as_str().unwrap().contains("höchstens ≈ 5"), "{d}");
}

/// `-D` without a module context overrides top-level assignments (like `openscad -D`).
#[test]
fn define_top_level_variables() {
    let f = data("variables.mcad");
    let vol = |extra: &[&str]| {
        let mut a = vec!["measure", f.as_str(), "--volume", "all"];
        a.extend_from_slice(extra);
        let v = json(&a, 0);
        (v["volume"]["volume"].as_f64().unwrap(), v)
    };
    let (v0, _) = vol(&[]);
    assert!((v0 - 20.0 * 10.0 * 8.0).abs() < 1e-6, "{v0}");
    // d = w / 2 computes with the new w
    let (v1, j) = vol(&["-D", "w=40"]);
    assert!((v1 - 40.0 * 20.0 * 8.0).abs() < 1e-6, "{v1}");
    assert_eq!(j["context"], Value::Null);
    // the last of two assignments takes the value; the last -D of a name wins
    let (v2, _) = vol(&["-D", "h=2", "-D", "h=3"]);
    assert!((v2 - 20.0 * 10.0 * 3.0).abs() < 1e-6, "{v2}");
    // unknown name: warning, exit 0, model unchanged
    let (v3, j) = vol(&["-D", "width=3"]);
    assert!((v3 - v0).abs() < 1e-9);
    let d = j["diagnostics"].as_array().unwrap();
    assert!(
        d.iter().any(|x| x["severity"] == "warning" && x["message"].as_str().unwrap().contains("-D width")),
        "{d:?}"
    );
    // `$` variables need no assignment
    let (_, j) = vol(&["-D", "$fn=64"]);
    assert!(!j["diagnostics"].as_array().unwrap().iter().any(|x| x["message"].as_str().unwrap().contains("-D")), "{j}");
    // the values show up as variable values (outline) and in other commands
    let v = json(&["outline", &f, "-D", "w=40"], 0);
    let w = v["variables"].as_array().unwrap().iter().find(|x| x["name"] == "w").unwrap().clone();
    assert_eq!(w["value"], 40.0);
    let dd = v["variables"].as_array().unwrap().iter().find(|x| x["name"] == "d").unwrap().clone();
    assert_eq!(dd["value"], 20.0);
    let v = json(&["inspect", &f, "--line", "6", "-D", "w=40"], 0);
    assert_eq!(v["nodes"][0]["world_bounds"]["max"][0], 40.0);
    let t = text(&["check", &f, "-D", "nope=1"], 0);
    assert!(t.contains("Warnung") && t.contains("-D nope"), "{t}");
    // with a module context -D stays a module parameter
    let v = json(&["eval", &data("housing.mcad"), "--module", "housing", "-D", "w=80"], 0);
    assert_eq!(v["context"], "housing");
}

/// `-m 'NAME(args)'`: module context with arguments as in a call (positional, named,
/// expressions with the file's variables and functions), for every command with `-m`.
#[test]
fn module_call_arguments() {
    let f = data("holder.mcad");
    let vol = |m: &str, extra: &[&str]| {
        let mut a = vec!["measure", f.as_str(), "-m", m, "--volume", "all"];
        a.extend_from_slice(extra);
        let v = json(&a, 0);
        assert_eq!(v["context"], m.split('(').next().unwrap().trim(), "{v}");
        v["volume"]["volume"].as_f64().unwrap()
    };
    let close = |a: f64, b: f64| assert!((a - b).abs() < 1e-6 * b.abs().max(1.0), "{a} vs {b}");
    close(vol("holder", &[]), 10.0 * 5.0 * 2.0);
    close(vol("holder", &["-D", "w=30"]), 30.0 * 5.0 * 2.0);
    // positional and named
    close(vol("holder(30, h = 5)", &[]), 30.0 * 5.0 * 5.0);
    // expressions: file variables and functions
    close(vol("holder(width, h = double(2))", &[]), 30.0 * 5.0 * 4.0);
    // together with -D for another parameter
    close(vol("holder(30)", &["-D", "d=1"]), 30.0 * 1.0 * 2.0);
    // nested module
    close(vol("parts.screw(d = 3)", &[]), std::f64::consts::PI * 1.5 * 1.5 * 10.0);
    close(vol("parts.screw(4, l = 1)", &[]), std::f64::consts::PI * 4.0);
    // other commands: render, check, export
    let out = std::env::temp_dir().join(format!("cad_mcall_{}", std::process::id()));
    std::fs::create_dir_all(&out).unwrap();
    let stl = out.join("h.stl").display().to_string();
    let v = json(&["export", &f, "-m", "holder(30, h = 5)", "-o", &stl], 0);
    close(v["volume"].as_f64().unwrap(), 750.0);
    let png = out.join("h.png").display().to_string();
    json(&["render", &f, "-m", "holder(30, h = 5)", "-o", &png], 0);
    json(&["check", &f, "-m", "holder(h = 1)"], 0);
    // errors: exit 2 with a message
    let err = |m: &str, extra: &[&str], want: &str| {
        let mut a = vec!["check", f.as_str(), "-m", m];
        a.extend_from_slice(extra);
        let v = json(&a, 2);
        let e = v["error"].as_str().unwrap();
        assert!(e.contains(want), "{m}: {e}");
    };
    err("holder(1, 2, 3, 4)", &[], "zu viele Positionsargumente");
    err("holder(x = 1)", &[], "keinen Parameter `x`");
    err("holder(30)", &["-D", "w=20"], "in -D w=");
    err("holder(1, w = 2)", &[], "doppelt");
    err("holder(unknown + 1)", &[], "Argument `w`");
    err("holder(1", &[], "erwartet");
    err("missing(1)", &[], "Modul `missing` nicht gefunden");
    let _ = std::fs::remove_dir_all(&out);
}

/// Kernel error with a place (Plan 21, D1): a dome touching a rib tangentially. The diagnostic
/// names both lines and the place (`at`, `nodes`), `mcad render` marks it.
#[test]
fn tangent_contact_place_in_diagnostics_and_render() {
    let f = data("boss_rib.mcad");
    let v = json(&["check", &f], 1);
    let d =
        v["diagnostics"].as_array().unwrap().iter().find(|d| d["at"].is_array()).expect("diagnostic with at").clone();
    let msg = d["message"].as_str().unwrap();
    assert!(
        msg.starts_with(
            "Vereinigung gescheitert: Zeile 15 (boss) und Zeile 16 (rib) berühren sich tangential bei [7, 15,"
        ),
        "{msg}"
    );
    let at: Vec<f64> = d["at"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
    assert!((at[0] - 7.0).abs() < 1e-6 && (at[1] - 15.0).abs() < 1e-6 && at[2] > 2.0 && at[2] < 8.0, "{at:?}");
    assert_eq!(d["nodes"].as_array().unwrap().len(), 2);
    // text output: the same message
    let t = text(&["check", &f], 1);
    assert!(t.contains("Zeile 15 (boss) und Zeile 16 (rib) berühren sich tangential"), "{t}");
    // render: marked (red edges, point, label), counted
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("r.png").display().to_string();
    let v = json(&["render", &f, "-o", &png], 1);
    assert_eq!(v["overlays"]["failures"], 1, "{}", v["overlays"]);
    assert!(
        v["notes"].as_array().unwrap().iter().any(|n| n.as_str().unwrap().contains("Fehlerstelle")),
        "{}",
        v["notes"]
    );
    // scene: the node message carries the place (node system) and the contact edges
    let v = json(&["eval", &f], 1);
    let msgs: Vec<Value> = v["scene"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|n| n["messages"].as_array().unwrap().clone())
        .collect();
    let m = msgs.iter().find(|m| m["at"].is_array()).expect("message with at");
    assert!(m["message"].as_str().unwrap().contains("berühren sich tangential"), "{m}");
    assert!(!m["edges"].as_array().unwrap().is_empty());
}

/// `v` without what measures time (`cost`, `ms`, `brep_ms`, `timings`, the daemon's note).
fn without_times(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for k in ["cost", "ms", "brep_ms", "timings", "daemon", "eval_ms", "kernel_ms"] {
                m.remove(k);
            }
            m.values_mut().for_each(without_times);
        }
        Value::Array(a) => a.iter_mut().for_each(without_times),
        _ => {}
    }
}

/// Evaluation on several threads gives what one thread gives, over the whole corpus of the
/// reference: every node's status, message, volume, faces, edges and selected edges, and every
/// result's mesh (hash of positions and indices); `--jobs 4` twice (races would show as a
/// difference between the runs). `eval --json` of the heaviest examples (diagnostics, hints,
/// bounds, statistics of every node): the same for 1, 2 and 4 threads.
#[test]
fn parallel_evaluation_is_deterministic() {
    let run = |jobs: &str| {
        let out = mcad().args(["diff-reference", &reference(), "--mesh", "--json", "--jobs", jobs]).output().unwrap();
        let mut v: Value = serde_json::from_slice(&out.stdout).unwrap();
        without_times(&mut v);
        v
    };
    let one = run("1");
    assert_eq!(one["files"].as_array().unwrap().len(), 101);
    for round in 0..2 {
        let four = run("4");
        if four != one {
            for (a, b) in one["files"].as_array().unwrap().iter().zip(four["files"].as_array().unwrap()) {
                assert_eq!(a, b, "round {round}: {} differs with --jobs 4", a["file"]);
            }
            assert_eq!(one, four, "round {round}");
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for f in [
        "examples/08-macropad.mcad",
        "examples/09-flashlight.mcad",
        "examples/07-mug.mcad",
        "tests/models/everyday/key_rack/key_rack.mcad",
        "tests/models/spec/hull_minkowski.mcad",
        "tests/models/project-housing/housing.mcad",
    ] {
        let f = root.join(f).display().to_string();
        let eval = |jobs: &str| {
            let out = mcad().args(["eval", &f, "--json", "--jobs", jobs]).output().unwrap();
            let mut v: Value = serde_json::from_slice(&out.stdout).unwrap();
            without_times(&mut v);
            v
        };
        let want = eval("1");
        for jobs in ["2", "4"] {
            assert_eq!(eval(jobs), want, "{f} with --jobs {jobs}");
        }
    }
    // the option and the variable: bad values are usage errors
    let out = mcad().args(["check", &data("plate.mcad"), "--jobs", "0"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let out = mcad().env("MCAD_JOBS", "many").args(["check", &data("plate.mcad"), "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["error"].as_str().unwrap().contains("MCAD_JOBS"), "{v}");
    let out = mcad().env("MCAD_JOBS", "2").args(["check", &data("plate.mcad"), "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
}
