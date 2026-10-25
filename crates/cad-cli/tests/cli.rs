//! End-to-end tests of the `cad` binary on small files in `tests/data/`.

use std::path::PathBuf;

use assert_cmd::Command;
use serde_json::Value;

fn data(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name).display().to_string()
}

fn cad() -> Command {
    // without daemon (tests/daemon.rs covers it, with a socket directory of its own)
    let mut c = Command::cargo_bin("cad").unwrap();
    c.env("CAD_NO_DAEMON", "1");
    c
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
    assert_eq!(v["schema_version"], 2);
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
    assert_eq!(n["selected_edge_count"], 2);
    // the edges themselves: polylines in world coordinates, as in `diff-kernels` and the scene
    let edges = n["selected_edges"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
    assert!(edges.iter().all(|e| e.as_array().unwrap().len() >= 2 && e[0].as_array().unwrap().len() == 3), "{edges:?}");
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
fn export_step() {
    let dir = tempfile::tempdir().unwrap();
    let step = dir.path().join("teil.step");
    let v = json(&["export", &data("platte.scad"), "-o", &step.display().to_string()], 0);
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
    let stp = dir.path().join("teil.stp");
    json(&["export", &data("platte.scad"), "-o", &stp.display().to_string(), "--schema", "ap214"], 0);
    let m = cad_step::p21::parse(&std::fs::read_to_string(&stp).unwrap()).unwrap();
    assert!(m.schema.starts_with("AUTOMOTIVE_DESIGN"), "{}", m.schema);
    json(&["export", &data("platte.scad"), "-o", &stp.display().to_string(), "--schema", "ap203"], 2);
}

/// Without the feature `solidcore`, `diff-kernels` is a usage error with a build hint.
#[cfg(not(feature = "solidcore"))]
#[test]
fn diff_kernels_needs_the_feature() {
    let v = json(&["diff-kernels", &data("platte.scad")], 2);
    assert!(v["error"].as_str().unwrap().contains("--features solidcore"), "{v}");
}

/// `diff-kernels` over all examples: still mostly `Unsupported` on the solidcore side; it must
/// answer for every file and node without crashing (exit 1 = not all nodes the same). Only with
/// the feature `solidcore` (`cargo test -p cad-cli --features solidcore`).
#[cfg(feature = "solidcore")]
#[test]
fn diff_kernels_on_examples() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files: Vec<String> = Vec::new();
    for d in ["examples/openscad-compat", "examples/spec"] {
        let mut v: Vec<String> = std::fs::read_dir(root.join(d))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "scad"))
            .map(|p| p.display().to_string())
            .collect();
        v.sort();
        files.extend(v);
    }
    assert!(files.len() >= 41, "{}", files.len());
    let mut args = vec!["diff-kernels"];
    args.extend(files.iter().map(|s| s.as_str()));
    let out = cad().args(&args).arg("--json").output().unwrap();
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
        // solidcore @ c9de85d: cylinders, spheres, booleans and transforms are unsupported; boxes
        // and plain extrusions agree with SMLib, nothing that answers differs
        assert!(s["unsupported"].as_u64().unwrap() > 100, "{s}");
        assert!(s["same"].as_u64().unwrap() > 200, "{s}");
        assert_eq!(s["differ"].as_u64().unwrap() + s["failed"].as_u64().unwrap(), 0, "{s}");
    }
    // 2D nodes need no kernel and agree
    assert!(s["same"].as_u64().unwrap() > 0, "{s}");
}

#[test]
fn usage_errors() {
    json(&["check", "/nonexistent/datei.scad"], 2);
    // -D without a value
    json(&["eval", &data("gehaeuse.scad"), "-D", "w"], 2);
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

#[test]
fn sketch_and_namespaces() {
    use std::f64::consts::PI;
    let want = (40.0 * 20.0 + PI * 400.0 / 2.0) * 5.0;
    // a sketch with an arc: exact volume
    let v = json(
        &[
            "export",
            &data("skizze.scad"),
            "-o",
            &tempfile::tempdir().unwrap().path().join("s.step").display().to_string(),
        ],
        0,
    );
    let vol = v["volume"].as_f64().unwrap();
    assert!((vol - want).abs() < 1e-6 * want, "{vol} {want}");
    // the function of the namespace
    let t = text(&["eval", &data("skizze.scad")], 0);
    assert!(t.contains(&format!("ECHO: {}", cad_eval::fmt_num(40.0 * 20.0 + PI * 200.0))), "{t}");
    // outline: qualified module and function names
    let v = json(&["outline", &data("skizze.scad")], 0);
    let q: Vec<&str> = v["modules"].as_array().unwrap().iter().map(|m| m["qualified"].as_str().unwrap()).collect();
    assert_eq!(q, ["teile", "teile.platte"]);
    let f: Vec<&str> = v["functions"].as_array().unwrap().iter().map(|m| m["name"].as_str().unwrap()).collect();
    assert_eq!(f, ["teile.flaeche"]);
    let t = text(&["outline", &data("skizze.scad")], 0);
    assert!(t.contains("teile.platte(a = 40: Zahl"), "{t}");
    // --module with a path: the inner module as context
    let v = json(&["eval", &data("skizze.scad"), "--module", "teile.platte", "-D", "a=30"], 0);
    assert_eq!(v["scene"]["context"]["path"], serde_json::json!(["teile", "platte"]));
    let v = json(&["inspect", &data("skizze.scad"), "--module", "teile.platte", "--line", "5"], 0);
    let n = &v["nodes"][0];
    assert_eq!(n["label"], "linear_extrude");
    let vol = n["volume"].as_f64().unwrap();
    assert!((vol - want).abs() < 1e-6 * want, "{vol}");
    // a segment line: the sketch node (render highlights it)
    let dir = tempfile::tempdir().unwrap();
    let o = dir.path().join("s.png").display().to_string();
    json(&["render", &data("skizze.scad"), "-o", &o, "--line", "9", "--size", "320x240"], 0);
    // errors per segment line
    let src = dir.path().join("fehler.scad");
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
    let f = data("skizze_ecken.scad");
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
    let src = dir.path().join("zu_gross.scad");
    std::fs::write(&src, "linear_extrude(1) sketch {\n start([0, 0]);\n line(to = [10, 0]);\n corner(r = 12);\n line(to = [10, 5]);\n line(to = [0, 5]);\n close();\n}\n").unwrap();
    let v = json(&["check", &src.display().to_string()], 1);
    let d = &v["diagnostics"][0];
    assert_eq!(d["range"]["line"], 4);
    assert!(d["message"].as_str().unwrap().contains("höchstens ≈ 5"), "{d}");
}

/// `-D` without a module context overrides top-level assignments (like `openscad -D`).
#[test]
fn define_top_level_variables() {
    let f = data("variablen.scad");
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
    let (v3, j) = vol(&["-D", "breite=3"]);
    assert!((v3 - v0).abs() < 1e-9);
    let d = j["diagnostics"].as_array().unwrap();
    assert!(
        d.iter().any(|x| x["severity"] == "warning" && x["message"].as_str().unwrap().contains("-D breite")),
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
    let t = text(&["check", &f, "-D", "nix=1"], 0);
    assert!(t.contains("Warnung") && t.contains("-D nix"), "{t}");
    // with a module context -D stays a module parameter
    let v = json(&["eval", &data("gehaeuse.scad"), "--module", "gehaeuse", "-D", "w=80"], 0);
    assert_eq!(v["context"], "gehaeuse");
}

/// `-m 'NAME(args)'`: module context with arguments as in a call (positional, named,
/// expressions with the file's variables and functions), for every command with `-m`.
#[test]
fn module_call_arguments() {
    let f = data("halter.scad");
    let vol = |m: &str, extra: &[&str]| {
        let mut a = vec!["measure", f.as_str(), "-m", m, "--volume", "all"];
        a.extend_from_slice(extra);
        let v = json(&a, 0);
        assert_eq!(v["context"], m.split('(').next().unwrap().trim(), "{v}");
        v["volume"]["volume"].as_f64().unwrap()
    };
    let close = |a: f64, b: f64| assert!((a - b).abs() < 1e-6 * b.abs().max(1.0), "{a} vs {b}");
    close(vol("halter", &[]), 10.0 * 5.0 * 2.0);
    close(vol("halter", &["-D", "w=30"]), 30.0 * 5.0 * 2.0);
    // positional and named
    close(vol("halter(30, h = 5)", &[]), 30.0 * 5.0 * 5.0);
    // expressions: file variables and functions
    close(vol("halter(breite, h = doppelt(2))", &[]), 30.0 * 5.0 * 4.0);
    // together with -D for another parameter
    close(vol("halter(30)", &["-D", "d=1"]), 30.0 * 1.0 * 2.0);
    // nested module
    close(vol("teile.schraube(d = 3)", &[]), std::f64::consts::PI * 1.5 * 1.5 * 10.0);
    close(vol("teile.schraube(4, l = 1)", &[]), std::f64::consts::PI * 4.0);
    // other commands: render, check, export
    let out = std::env::temp_dir().join(format!("cad_mcall_{}", std::process::id()));
    std::fs::create_dir_all(&out).unwrap();
    let stl = out.join("h.stl").display().to_string();
    let v = json(&["export", &f, "-m", "halter(30, h = 5)", "-o", &stl], 0);
    close(v["volume"].as_f64().unwrap(), 750.0);
    let png = out.join("h.png").display().to_string();
    json(&["render", &f, "-m", "halter(30, h = 5)", "-o", &png], 0);
    json(&["check", &f, "-m", "halter(h = 1)"], 0);
    // errors: exit 2 with a message
    let err = |m: &str, extra: &[&str], want: &str| {
        let mut a = vec!["check", f.as_str(), "-m", m];
        a.extend_from_slice(extra);
        let v = json(&a, 2);
        let e = v["error"].as_str().unwrap();
        assert!(e.contains(want), "{m}: {e}");
    };
    err("halter(1, 2, 3, 4)", &[], "zu viele Positionsargumente");
    err("halter(x = 1)", &[], "keinen Parameter `x`");
    err("halter(30)", &["-D", "w=20"], "in -D w=");
    err("halter(1, w = 2)", &[], "doppelt");
    err("halter(unbekannt + 1)", &[], "Argument `w`");
    err("halter(1", &[], "erwartet");
    err("fehlt(1)", &[], "Modul `fehlt` nicht gefunden");
    let _ = std::fs::remove_dir_all(&out);
}
