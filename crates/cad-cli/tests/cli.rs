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
    // the edges themselves: polylines in world coordinates, as in `diff-kernels` and the scene
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

/// Without the feature `solidcore`, `diff-kernels` is a usage error with a build hint.
#[cfg(not(feature = "solidcore"))]
#[test]
fn diff_kernels_needs_the_feature() {
    let v = json(&["diff-kernels", &data("plate.mcad")], 2);
    assert!(v["error"].as_str().unwrap().contains("--features solidcore"), "{v}");
}

/// `--kernel` (global option) and `MCAD_KERNEL`: the option wins, unknown names are usage
/// errors, the JSON reports the kernel used (`kernel_id`).
#[test]
fn kernel_option_and_env() {
    let f = data("plate.mcad");
    let v = json(&["check", &f], 0);
    assert!(v["kernel_id"].as_str().unwrap().starts_with("smlib-"), "{v}");
    let v = json(&["check", "--kernel", "smlib", &f], 0);
    assert!(v["kernel_id"].as_str().unwrap().starts_with("smlib-"), "{v}");
    // the option is global: also after the file
    let v = json(&["eval", &f, "--kernel", "smlib"], 0);
    assert!(v["kernel_id"].as_str().unwrap().starts_with("smlib-"), "{v}");
    // the option wins over the variable
    let out =
        mcad().env("MCAD_KERNEL", "solidcore").args(["check", "--kernel", "smlib", &f, "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stdout));
    let out = mcad().env("MCAD_KERNEL", "SMLib").args(["check", &f, "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    // unknown names: clap for the option, our message for the variable
    let out = mcad().args(["check", "--kernel", "occt", &f]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let out = mcad().env("MCAD_KERNEL", "occt").args(["check", &f, "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["error"].as_str().unwrap().contains("MCAD_KERNEL") && v["error"].as_str().unwrap().contains("occt"),
        "{v}"
    );
    // no kernel was selected: the output names none
    assert!(v.get("kernel_id").is_none(), "{v}");
    // commands that evaluate nothing ignore the variable
    let out = mcad().env("MCAD_KERNEL", "occt").args(["usage", "--list"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
}

/// Without the feature `solidcore`, `--kernel solidcore` and `MCAD_KERNEL=solidcore` are usage
/// errors with a build hint.
#[cfg(not(feature = "solidcore"))]
#[test]
fn kernel_solidcore_needs_the_feature() {
    let f = data("plate.mcad");
    for args in [vec!["check", "--kernel", "solidcore", &f], vec!["render", &f, "-o", "x.png", "--kernel", "solidcore"]]
    {
        let v = json(&args, 2);
        assert!(v.get("kernel_id").is_none(), "{v}");
        let e = v["error"].as_str().unwrap();
        assert!(e.contains("--kernel solidcore") && e.contains("--features solidcore"), "{v}");
    }
    let out = mcad().env("MCAD_KERNEL", "solidcore").args(["eval", &f]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let e = String::from_utf8_lossy(&out.stderr);
    assert!(e.contains("MCAD_KERNEL=solidcore") && e.contains("--features solidcore"), "{e}");
}

/// With the feature `solidcore`: the commands evaluate with solidcore, volumes as with SMLib
/// (box exact, cylinder within the accuracy of SMLib's integration).
#[cfg(feature = "solidcore")]
#[test]
fn kernel_solidcore_evaluates() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("box.mcad");
    std::fs::write(&f, "cube([10, 20, 30]);\ntranslate([20, 0, 0]) cylinder(h = 10, r = 5);\n").unwrap();
    let f = f.display().to_string();
    let v = json(&["eval", "--kernel", "solidcore", &f], 0);
    assert!(v["kernel_id"].as_str().unwrap().starts_with("solidcore-"), "{v}");
    assert_eq!(v["daemon"]["used"], false);
    let volume = |extra: &[&str], env: Option<&str>| {
        let mut c = mcad();
        if let Some(k) = env {
            c.env("MCAD_KERNEL", k);
        }
        let out = c.args(["measure", &f, "--volume", "--json"]).args(extra).output().unwrap();
        assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stdout));
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        (v["volume"]["volume"].as_f64().unwrap(), v["kernel_id"].as_str().unwrap().to_string())
    };
    let (a, ka) = volume(&[], None);
    let (b, kb) = volume(&["--kernel", "solidcore"], None);
    let (c, kc) = volume(&[], Some("solidcore"));
    assert!(ka.starts_with("smlib-") && kb.starts_with("solidcore-") && kc == kb, "{ka} {kb} {kc}");
    let exact = 6000.0 + std::f64::consts::PI * 25.0 * 10.0;
    assert_eq!(b, c);
    assert!(
        (a - exact).abs() < 2e-4 * exact && (b - exact).abs() < 1e-6 * exact,
        "smlib {a}, solidcore {b}, exact {exact}"
    );
    // the cube alone agrees exactly
    let out = mcad().args(["inspect", &f, "--line", "1", "--json", "--kernel", "solidcore"]).output().unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!((v["nodes"][0]["volume"].as_f64().unwrap() - 6000.0).abs() < 1e-9, "{v}");
    // no tessellation in solidcore yet: render is a kernel error that says so
    let png = dir.path().join("b.png").display().to_string();
    let out = mcad().args(["render", &f, "-o", &png, "--kernel", "solidcore"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("tessellate"), "{}", String::from_utf8_lossy(&out.stderr));
    // a failed command still names its kernel
    let v = json(&["render", &f, "-o", &png, "--kernel", "solidcore"], 3);
    assert!(v["kernel_id"].as_str().unwrap().starts_with("solidcore-"), "{v}");
    // exact measures need the tessellation too (plan phase 2): a kernel error that names it
    let v = json(&["measure", &f, "--at", "[10,10,30]", "--kernel", "solidcore"], 3);
    let e = v["error"].as_str().unwrap();
    assert!(e.contains("solidcore: tessellate"), "{v}");
    // a wrong reference is a usage error before the kernel is asked
    json(&["measure", &f, "--at", "bogus", "--kernel", "solidcore"], 2);
    json(&["measure", &f, "--distance", "corner@[0,0,0]", "[1,1,1]", "--kernel", "solidcore"], 2);
    let step = dir.path().join("b.step").display().to_string();
    // STEP from the neutral B-rep (plan 1.11): the same faces and volume as with SMLib
    let v = json(&["export", &f, "-o", &step, "--kernel", "solidcore"], 0);
    let w = json(&["export", &f, "-o", &step], 0);
    assert_eq!(v["step"]["faces"], 9, "{v}");
    assert_eq!(v["step"]["surfaces"], w["step"]["surfaces"], "{v} {w}");
    assert_eq!(v["step"]["curves"], w["step"]["curves"], "{v} {w}");
    assert!((v["volume"].as_f64().unwrap() - exact).abs() < 1e-6 * exact, "{v}");
}

/// `diff-kernels --brep`: the neutral B-reps of both kernels agree (kinds, positions,
/// orientation, tags) on primitives, transforms, Booleans and fillets.
#[cfg(feature = "solidcore")]
#[test]
fn diff_kernels_brep() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut args = vec!["diff-kernels".to_string(), "--brep".into(), "--json".into()];
    for f in [
        "tests/models/openscad-compat/02_sphere_variants.scad",
        "tests/models/openscad-compat/03_cylinder_variants.scad",
        "tests/models/openscad-compat/04_cone_variants.scad",
        "tests/models/openscad-compat/05_nested_booleans.scad",
        "tests/models/spec/hole_plate.mcad",
        "tests/models/spec/fillet_zones.mcad",
    ] {
        args.push(root.join(f).display().to_string());
    }
    let out = mcad().args(&args).output().unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let b = &v["summary"]["brep"];
    assert!(b["same"].as_u64().unwrap() > 50, "{b}");
    assert!(b.get("differ").is_none() && b.get("unsupported").is_none(), "{b}");
}

/// `diff-kernels` over all examples: still mostly `Unsupported` on the solidcore side; it must
/// answer for every file and node without crashing (exit 1 = not all nodes the same). Only with
/// the feature `solidcore` (`cargo test -p cad-cli --features solidcore`).
#[cfg(feature = "solidcore")]
#[test]
fn diff_kernels_on_examples() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files: Vec<String> = Vec::new();
    // the same list as scripts/ci/solidcore.sh: model files of each directory, of an assembly
    // folder only the main file named after the folder (the others are include/use)
    for d in ["tests/models/openscad-compat", "tests/models/spec", "tests/models/everyday", "tests/everyday/bugs"] {
        let mut v: Vec<PathBuf> = Vec::new();
        for p in std::fs::read_dir(root.join(d)).unwrap().map(|e| e.unwrap().path()) {
            if p.is_dir() {
                let main = p.join(p.file_name().unwrap()).with_extension("mcad");
                if main.is_file() {
                    v.push(main);
                }
            } else if cad_eval::is_model_file(&p.to_string_lossy()) {
                v.push(p);
            }
        }
        v.sort();
        files.extend(v.iter().map(|p| p.display().to_string()));
    }
    files.push(root.join("tests/models/project-housing/housing.mcad").display().to_string());
    assert!(files.len() >= 100, "{}", files.len());
    let mut args = vec!["diff-kernels"];
    args.extend(files.iter().map(|s| s.as_str()));
    let out = mcad().args(&args).arg("--json").output().unwrap();
    let code = out.status.code().unwrap();
    assert!(code == 0 || code == 1, "exit {code}: {}", String::from_utf8_lossy(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["command"], "diff-kernels");
    assert_eq!(v["files"].as_array().unwrap().len(), files.len());
    let s = &v["summary"];
    assert!(s["nodes"].as_u64().unwrap() > 500);
    for f in v["files"].as_array().unwrap() {
        for n in f["nodes"].as_array().unwrap() {
            let verdict = n["verdict"].as_str().unwrap();
            assert!(["same", "unsupported", "blocked", "differ", "failed", "skipped"].contains(&verdict), "{verdict}");
        }
    }
    if v["solidcore_available"] == true {
        // solidcore @ bba8d87 over 101 files (2604 nodes): 2523 same, 26 unsupported
        // (polyhedra, shell, shear, a variable fillet with several laws), 47 blocked, 8 differ,
        // nothing fails. Curved solids are compared against their exact volume (solidcore is the
        // more accurate one), blends within the fit of their surfaces: no node differs in its
        // volume alone; the differences are Booleans that lose faces or that SMLib fails
        assert!(s["unsupported"].as_u64().unwrap() <= 30, "{s}");
        assert!(s["same"].as_u64().unwrap() >= 2510, "{s}");
        assert!(s["differ"].as_u64().unwrap() <= 10, "{s}");
        assert_eq!(s["failed"].as_u64().unwrap(), 0, "{s}");
        // every rule is in use; curved 72 (with fillets and chamfers), blend 8
        let r = &s["volume_rules"];
        assert!(r["exact"].as_u64().unwrap() >= 110 && r["operands"].as_u64().unwrap() >= 45, "{r}");
        assert!(r["curved"].as_u64().unwrap() <= 85, "{r}");
        assert!((1..=15).contains(&r["blend"].as_u64().unwrap_or(0)), "{r}");
        // queries work on the shape's own body: few copies (1549 at bba8d87)
        let copies = s["solidcore_cost"]["copies"].as_u64().unwrap();
        assert!(copies > 0 && copies < 2500, "{copies}");
        for f in v["files"].as_array().unwrap() {
            for n in f["nodes"].as_array().unwrap() {
                if n["verdict"] == "differ" {
                    let notes = n["notes"].as_array().unwrap();
                    assert!(!notes.iter().all(|x| x.as_str().unwrap().starts_with("volume")), "volume only: {n}");
                }
                // a box is exact in both kernels
                if n["verdict"] == "same" && n["label"] == "box" {
                    assert_eq!(n["volume_check"]["rule"], "relative", "{n}");
                }
                // with an exact value, solidcore is the closer one (or within 1e-6)
                if n["verdict"] == "same" && n["volume_check"]["rule"] == "exact" {
                    let c = &n["volume_check"];
                    let (a, b) = (c["smlib_error"].as_f64().unwrap(), c["solidcore_error"].as_f64().unwrap());
                    assert!(b <= a.max(1e-6), "{n}");
                }
            }
        }
    }
    // 2D nodes need no kernel and agree
    assert!(s["same"].as_u64().unwrap() > 0, "{s}");
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
