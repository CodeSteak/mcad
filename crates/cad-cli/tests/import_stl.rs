//! `import("x.stl")` (Plan 21, phase B): a cube STL becomes a B-rep of 6 faces (Booleans,
//! volume, STEP), a large sphere stays a mesh (render, STL/3MF export, bounds, volume,
//! collisions), `as = "solid" | "mesh"` overrides, warnings for broken, open and inverted
//! meshes, `watch` follows a changed STL. Test files are generated.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use serde_json::Value;

type Tri = [[f32; 3]; 3];

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

fn write(dir: &Path, name: &str, data: impl AsRef<[u8]>) -> String {
    let p = dir.join(name);
    std::fs::write(&p, data).unwrap();
    p.display().to_string()
}

fn cube(s: f32) -> Vec<Tri> {
    let p = |i: u32| [(i & 1) as f32 * s, ((i >> 1) & 1) as f32 * s, ((i >> 2) & 1) as f32 * s];
    let quads: [[u32; 4]; 6] = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    quads.iter().flat_map(|q| [[p(q[0]), p(q[1]), p(q[2])], [p(q[0]), p(q[2]), p(q[3])]]).collect()
}

/// UV sphere of radius `r` with `segs` meridians and `rings` bands (2·segs·(rings − 1)
/// triangles), outward.
fn sphere(r: f32, segs: usize, rings: usize) -> Vec<Tri> {
    let pt = |i: usize, j: usize| {
        let th = std::f32::consts::PI * i as f32 / rings as f32;
        let ph = 2.0 * std::f32::consts::PI * (j % segs) as f32 / segs as f32;
        [r * th.sin() * ph.cos(), r * th.sin() * ph.sin(), r * th.cos()]
    };
    let mut t = Vec::new();
    for i in 0..rings {
        for j in 0..segs {
            let (a, b, c, d) = (pt(i, j), pt(i, j + 1), pt(i + 1, j + 1), pt(i + 1, j));
            if i > 0 {
                t.push([a, d, b]);
            }
            if i + 1 < rings {
                t.push([b, d, c]);
            }
        }
    }
    t
}

fn node_hints(v: &Value) -> String {
    v["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|n| n["hints"].as_array().unwrap().iter())
        .map(|h| h.to_string())
        .collect()
}

fn messages(v: &Value) -> Vec<String> {
    v["diagnostics"].as_array().unwrap().iter().map(|d| d["message"].as_str().unwrap().to_string()).collect()
}

#[test]
fn cube_becomes_a_brep_with_booleans() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "w.stl", cad_stl::write_ascii("wuerfel", &cube(10.0)));
    // binary with a header starting with "solid" (the size decides)
    write(dir.path(), "b.stl", cad_stl::write_binary("solid binary from some CAD", &cube(10.0)));
    for stl in ["w.stl", "b.stl"] {
        let f = write(dir.path(), "a.scad", format!("import(\"{stl}\");\n"));
        let v = json(&["inspect", &f, "--line", "1"], 0);
        assert!(node_hints(&v).contains("B-Rep: 6 Flächen aus 12 Dreiecken"), "{v}");
        assert!((v["nodes"][0]["volume"].as_f64().unwrap() - 1000.0).abs() < 1e-6);
    }
    let f = write(
        dir.path(),
        "d.scad",
        "difference() {\n  import(\"w.stl\", center = true);\n  cylinder(h = 20, r = 3, center = true);\n}\n",
    );
    let v = json(&["measure", &f, "--volume", "--bounds"], 0);
    let vol = v["volume"]["volume"].as_f64().unwrap();
    assert!((vol - (1000.0 - std::f64::consts::PI * 90.0)).abs() < 1e-3, "{vol}");
    assert_eq!(v["bounds"]["bounds"]["min"][0].as_f64(), Some(-5.0), "{v}");
    // STEP: exact B-rep with a cylinder
    let step = dir.path().join("d.step").display().to_string();
    json(&["export", &f, "-o", &step], 0);
    assert!(std::fs::read_to_string(&step).unwrap().contains("CYLINDRICAL_SURFACE"));
    // scale per axis; hull of a mesh
    let f = write(dir.path(), "s.scad", "import(\"w.stl\", scale = [1, 2, 3]);\n");
    assert!((json(&["measure", &f, "--volume"], 0)["volume"]["volume"].as_f64().unwrap() - 6000.0).abs() < 1e-6);
    let f = write(dir.path(), "h.scad", "hull() { import(\"w.stl\"); translate([20, 0, 0]) cube(1); }\n");
    let v = json(&["inspect", &f, "--line", "1"], 0);
    assert!(node_hints(&v).contains("exakt: Polyeder"), "{v}");
}

#[test]
fn large_sphere_stays_a_mesh() {
    let dir = tempfile::tempdir().unwrap();
    let tris = sphere(20.0, 100, 101);
    assert_eq!(tris.len(), 20000);
    write(dir.path(), "kugel.stl", cad_stl::write_binary("kugel", &tris));
    let f = write(
        dir.path(),
        "m.scad",
        "module platine() color(\"green\") translate([0, 0, 20]) import(\"kugel.stl\");\nmodule klotz() cube(10);\n\
         k: platine();\ntranslate([25, 0, 0]) a: klotz();\ntranslate([15, -5, 15]) b: klotz();\n",
    );
    let v = json(&["check", &f], 0);
    assert_eq!(v["warnings"].as_u64(), Some(0), "{v}");
    let v = json(&["inspect", &f, "--line", "1"], 0);
    assert!(node_hints(&v).contains("Netz: 20000 Dreiecke"), "{v}");
    // bounds and volume from the mesh (sphere of r 20 inscribed)
    let v = json(&["measure", &f, "--volume", "part:k", "--bounds", "part:k"], 0);
    let vol = v["volume"]["volume"].as_f64().unwrap();
    assert!(vol > 33300.0 && vol < 4.0 / 3.0 * std::f64::consts::PI * 8000.0, "{vol}");
    assert!((v["bounds"]["bounds"]["max"][2].as_f64().unwrap() - 40.0).abs() < 1e-4, "{v}");
    // collisions mesh-based: k overlaps b, not a
    let v = json(&["measure", &f, "--interference", "--clearance", "k", "a"], 0);
    let col = v["interference"]["collisions"].as_array().unwrap();
    assert_eq!(col.len(), 1, "{v}");
    assert_eq!((col[0]["b"].as_str(), col[0]["method"].as_str()), (Some("b"), Some("mesh")));
    let d = v["clearance"]["distance"].as_f64().unwrap();
    // centre [0, 0, 20] to the cube edge [25, 0, 10], minus r (inscribed: a little more)
    assert!((d - (725f64.sqrt() - 20.0)).abs() < 0.01, "{d}");
    // exports hold the mesh
    for (ext, min) in [("stl", 20024), ("3mf", 20024)] {
        let out = dir.path().join(format!("m.{ext}")).display().to_string();
        let v = json(&["export", &f, "-o", &out], 0);
        assert_eq!(v["triangles"].as_u64(), Some(min), "{ext}: {v}");
    }
    let out = dir.path().join("m.step").display().to_string();
    json(&["export", &f, "-o", &out], 0);
    assert!(std::fs::read_to_string(&out).unwrap().contains("PLANE"));
    // render: the mesh is drawn (green pixels)
    let png = dir.path().join("m.png");
    let v = json(&["render", &f, "-o", &png.display().to_string(), "--module", "platine"], 0);
    assert_eq!(v["triangles"].as_u64(), Some(20000), "{v}");
    let dec = png::Decoder::new(std::fs::File::open(&png).unwrap());
    let mut reader = dec.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let ch = info.color_type.samples();
    let green = buf.chunks(ch).filter(|p| p[1] > 60 && p[1] as u32 > p[0] as u32 + 30 && p[1] > p[2]).count();
    assert!(green > 10000, "green pixels: {green}");
    // Booleans with a mesh: error with the hint
    let f = write(dir.path(), "d.scad", "difference() { import(\"kugel.stl\"); cube(5); }\n");
    let v = json(&["check", &f], 1);
    assert!(messages(&v).iter().any(|m| m.contains("beschnitten") && m.contains("as = \"solid\"")), "{v}");
}

#[test]
fn mode_override_and_limit() {
    let dir = tempfile::tempdir().unwrap();
    // 968 triangles, 890 faces after merging: above the limit, a mesh
    write(dir.path(), "k.stl", cad_stl::write_binary("k", &sphere(10.0, 22, 23)));
    let f = write(dir.path(), "a.scad", "import(\"k.stl\");\n");
    assert!(node_hints(&json(&["inspect", &f, "--line", "1"], 0)).contains("Netz:"));
    let f = write(
        dir.path(),
        "s.scad",
        "difference() { import(\"k.stl\", as = \"solid\"); cylinder(h = 30, r = 2, center = true); }\n",
    );
    let v = json(&["inspect", &f, "--line", "1"], 0);
    assert!(node_hints(&v).contains("B-Rep:"), "{v}");
    let vol = v["nodes"][0]["volume"].as_f64().unwrap();
    // inscribed sphere (r 10, 22 meridians) minus a cylinder r 2 through it
    assert!(vol > 3800.0 && vol < 3950.0, "{vol}");
    // the cube forced to a mesh
    write(dir.path(), "w.stl", cad_stl::write_ascii("w", &cube(2.0)));
    let f = write(dir.path(), "m.scad", "import(\"w.stl\", as = \"mesh\", convexity = 4);\n");
    let v = json(&["inspect", &f, "--line", "1"], 0);
    assert!(node_hints(&v).contains("Netz: 12 Dreiecke, 6 Flächen"), "{v}");
    let f = write(dir.path(), "x.scad", "import(\"w.stl\", as = \"brep\", id = \"a\");\n");
    let m = messages(&json(&["check", &f], 0));
    assert!(m.iter().any(|m| m.contains("\"solid\" (B-Rep) oder \"mesh\"")), "{m:?}");
    assert!(m.iter().any(|m| m.contains("`id` gilt nur für SVG")), "{m:?}");
}

#[test]
fn broken_open_and_inverted_meshes() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "kaputt.stl", "solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 nan_x\n");
    let f = write(dir.path(), "a.scad", "import(\"kaputt.stl\");\n");
    let m = messages(&json(&["check", &f], 1));
    assert!(m.iter().any(|m| m.contains("kaputt.stl: Zeile 4")), "{m:?}");
    write(dir.path(), "leer.stl", "solid leer\nendsolid leer\n");
    let f = write(dir.path(), "l.scad", "import(\"leer.stl\");\n");
    assert!(messages(&json(&["check", &f], 1)).iter().any(|m| m.contains("0 Dreiecke")));
    // open: one triangle missing → mesh with a warning; as = "solid" → error
    let mut open = cube(10.0);
    open.pop();
    write(dir.path(), "offen.stl", cad_stl::write_binary("offen", &open));
    let f = write(dir.path(), "o.scad", "import(\"offen.stl\");\n");
    let v = json(&["check", &f], 0);
    assert!(messages(&v).iter().any(|m| m.contains("nicht geschlossen (3 offene Kanten)")), "{v}");
    let f = write(dir.path(), "os.scad", "import(\"offen.stl\", as = \"solid\");\n");
    assert!(messages(&json(&["check", &f], 1)).iter().any(|m| m.contains("as = \"solid\" geht nicht")));
    // inside out and NaN: corrected / dropped with warnings, volume positive
    let mut inv: Vec<Tri> = cube(10.0).into_iter().map(|t| [t[0], t[2], t[1]]).collect();
    inv.push([[f32::NAN, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
    write(dir.path(), "inv.stl", cad_stl::write_binary("inv", &inv));
    let f = write(dir.path(), "i.scad", "import(\"inv.stl\");\n");
    let v = json(&["check", &f], 0);
    let m = messages(&v);
    assert!(m.iter().any(|m| m.contains("innen nach außen (alle 12 Dreiecke gewendet)")), "{m:?}");
    assert!(m.iter().any(|m| m.contains("1 Dreiecke mit ungültigen Koordinaten")), "{m:?}");
    assert!((json(&["measure", &f, "--volume"], 0)["volume"]["volume"].as_f64().unwrap() - 1000.0).abs() < 1e-6);
}

#[test]
fn watch_follows_a_changed_stl() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "w.stl", cad_stl::write_ascii("w", &cube(10.0)));
    let file = write(dir.path(), "w.scad", "import(\"w.stl\");\n");
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
    write(dir.path(), "w.stl", cad_stl::write_binary("w", &cube(5.0)));
    let t0 = Instant::now();
    let second: Value = serde_json::from_str(&lines.next().expect("zweite Zeile").unwrap()).unwrap();
    assert!(t0.elapsed() < Duration::from_secs(30));
    assert_eq!(second["trigger"], "change");
    assert_eq!(second["changed"], serde_json::json!(["w.stl"]));
    assert_eq!(child.wait().unwrap().code(), Some(0));
}
