//! CLI for agents: `outline`, extended `render` (contact sheet, free camera, selection,
//! overlays, section, xray), `measure`, `watch`, `set` and the reserved UI entry `mcad FILE`.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use serde_json::Value;

fn data(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name).display().to_string()
}

fn example(rel: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/models").join(rel).display().to_string()
}

fn mcad() -> Command {
    // without daemon (tests/daemon.rs covers it, with a socket directory of its own)
    let mut c = Command::cargo_bin("mcad").unwrap();
    c.env("MCAD_LANG", "de");
    c.env("MCAD_NO_DAEMON", "1");
    c
}

/// Runs with `--json`, checks the exit code and the common fields, returns the output.
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
    assert_eq!(v["schema_version"], 3, "{v}");
    assert_eq!(v["exit_code"], code);
    assert_eq!(v["ok"], code == 0);
    v
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

/// Decoded RGBA image.
struct Png {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
}

fn png(path: &Path) -> Png {
    let dec = png::Decoder::new(std::fs::File::open(path).unwrap());
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    buf.truncate(info.buffer_size());
    Png { w: info.width, h: info.height, rgba: buf }
}

impl Png {
    /// Pixels that differ from the background (the corner pixel).
    fn drawn(&self) -> usize {
        let bg = &self.rgba[0..4].to_vec();
        self.rgba.chunks_exact(4).filter(|p| p.iter().zip(bg).any(|(a, b)| (*a as i32 - *b as i32).abs() > 6)).count()
    }

    fn differing(&self, o: &Png) -> usize {
        assert_eq!((self.w, self.h), (o.w, o.h));
        self.rgba
            .chunks_exact(4)
            .zip(o.rgba.chunks_exact(4))
            .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| (*x as i32 - *y as i32).abs() > 6))
            .count()
    }

    /// Pixels in the colour of a section cap (orange red).
    fn cap_pixels(&self) -> usize {
        self.rgba
            .chunks_exact(4)
            .filter(|p| p[0] as i32 > p[1] as i32 + 60 && p[0] as i32 > p[2] as i32 + 60 && p[0] > 90)
            .count()
    }
}

// ------------------------------------------------------------------------------------------
// outline
// ------------------------------------------------------------------------------------------

#[test]
fn outline_spec_example() {
    let v = json(&["outline", &data("housing.mcad")], 0);
    assert_eq!(v["command"], "outline");
    assert_eq!(v["counts"]["errors"], 0);
    let m = &v["modules"][0];
    assert_eq!(m["name"], "housing");
    assert_eq!(m["line"], 3);
    let w = &m["params"][0];
    assert_eq!(
        (w["name"].as_str(), w["default"].as_str(), w["type"].as_str()),
        (Some("w"), Some("40"), Some("number"))
    );
    let g = &m["grids"][0];
    assert_eq!(g["name"], "g");
    let planes = g["planes"].as_array().unwrap();
    let inner_right = planes.iter().find(|p| p["name"] == "inner_right").unwrap();
    assert_eq!(
        (inner_right["axis"].as_u64(), inner_right["axis_name"].as_str(), f(&inner_right["value"])),
        (Some(0), Some("x"), 37.0)
    );
    assert_eq!(inner_right["kind"], "expression");
    assert_eq!(inner_right["expr"], "w - t");
    let corner = m["anchors"].as_array().unwrap().iter().find(|a| a["name"] == "corner").unwrap();
    assert_eq!(corner["point"], serde_json::json!([40.0, 30.0, 20.0]));
    // file level: instance k is a part with bounds and volume, anchors in file coordinates
    let k = &v["instances"][0];
    assert_eq!((k["name"].as_str(), k["module"].as_str(), k["part"].as_str()), (Some("k"), Some("housing"), Some("k")));
    let part = &v["parts"][0];
    close(f(&part["volume"]), 60.0 * 30.0 * 20.0 - 54.0 * 30.0 * 17.0, 1e-6);
    assert_eq!(part["bounds"]["max"], serde_json::json!([60.0, 30.0, 20.0]));
    assert!(v["anchors"].as_array().unwrap().iter().any(|a| a["ref"] == "k.corner"));
    assert_eq!(v["grids"][0]["name"], "h");
    let t = text(&["outline", &data("housing.mcad")], 0);
    assert!(t.contains("housing(w = 40: Zahl"), "{t}");
    assert!(t.contains("inner_right = w - t → 37"), "{t}");
    assert!(t.contains("k: housing  Z.21  Bauteil"), "{t}");
    // without the kernel: no volumes
    let v = json(&["outline", "--no-kernel", &data("housing.mcad")], 0);
    assert_eq!(v["parts"][0]["volume"], Value::Null);
    // --module makes no sense for an overview
    json(&["outline", &data("housing.mcad"), "--module", "housing"], 2);
}

#[test]
fn outline_functions_variables_and_projects() {
    let v = json(&["outline", &data("functions.mcad")], 0);
    let fun = &v["functions"][0];
    assert_eq!((fun["name"].as_str(), fun["body"].as_str()), (Some("double"), Some("2 * x")));
    let vars = v["variables"].as_array().unwrap();
    let height = vars.iter().find(|x| x["name"] == "height").unwrap();
    assert_eq!((height["expr"].as_str(), f(&height["value"])), (Some("double(5)"), 10.0));
    let params = v["modules"][0]["params"].as_array().unwrap();
    assert_eq!(params[0]["type"], "vector");
    assert_eq!(params[1]["type"], "bool");
    // project: include/use, modules from the library with their file
    let v = json(&["outline", &example("project-housing/housing.mcad")], 0);
    let inc = v["includes"].as_array().unwrap();
    assert_eq!(inc.len(), 2);
    assert!(inc.iter().any(|i| i["use"] == true && i["path"] == "lib/parts.mcad" && i["found"] == true));
    let g = v["modules"].as_array().unwrap().iter().find(|m| m["name"] == "housing").unwrap();
    assert_eq!(g["file"], "lib/parts.mcad");
    assert!(v["variables"].as_array().unwrap().iter().any(|x| x["name"] == "width" && x["file"] == "constants.mcad"));
    assert_eq!(v["parts"].as_array().unwrap().len(), 2);
}

// ------------------------------------------------------------------------------------------
// render
// ------------------------------------------------------------------------------------------

#[test]
fn render_contact_sheet() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("sheet.png");
    let o = out.display().to_string();
    let v = json(&["render", &data("housing.mcad"), "-o", &o, "--views", "all", "--size", "800x600"], 0);
    assert_eq!((v["width"].as_u64(), v["height"].as_u64()), (Some(800), Some(600)));
    let img = png(&out);
    assert_eq!((img.w, img.h), (800, 600));
    let views = v["views"].as_array().unwrap();
    let names: Vec<&str> = views.iter().map(|x| x["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["iso", "front", "top", "right"]);
    // same scale in every tile, tiles inside the sheet
    let s0 = f(&views[0]["camera"]["scale_px_per_mm"]);
    for t in views {
        close(f(&t["camera"]["scale_px_per_mm"]), s0, 1e-9);
        assert!(t["x"].as_u64().unwrap() + t["width"].as_u64().unwrap() <= 800);
        assert!(t["y"].as_u64().unwrap() + t["height"].as_u64().unwrap() <= 600);
    }
    assert_eq!(views[1]["camera"]["dir"], serde_json::json!([0.0, 1.0, 0.0]));
    // explicit list
    let v = json(&["render", &data("housing.mcad"), "-o", &o, "--views", "front,top"], 0);
    assert_eq!(v["views"].as_array().unwrap().len(), 2);
    assert_eq!((v["width"].as_u64(), v["height"].as_u64()), (Some(1600), Some(1200)));
    json(&["render", &data("housing.mcad"), "-o", &o, "--views", "iso,oblique"], 2);
}

#[test]
fn render_free_camera() {
    let dir = tempfile::tempdir().unwrap();
    let o = dir.path().join("k.png").display().to_string();
    let g = data("housing.mcad");
    let v = json(&["render", &g, "-o", &o, "--azimuth", "30", "--elevation", "20", "--size", "320x240"], 0);
    let cam = &v["camera"];
    close(f(&cam["azimuth"]), 30.0, 1e-3);
    close(f(&cam["elevation"]), 20.0, 1e-3);
    assert_eq!(cam["projection"], "ortho");
    let base = f(&cam["scale_px_per_mm"]);
    let v = json(&["render", &g, "-o", &o, "--zoom", "2", "--size", "320x240"], 0);
    let iso = json(&["render", &g, "-o", &o, "--size", "320x240"], 0);
    close(f(&v["camera"]["scale_px_per_mm"]), 2.0 * f(&iso["camera"]["scale_px_per_mm"]), 1e-9);
    assert!(base > 0.0);
    // iso equals azimuth 45, elevation 35.26
    close(f(&iso["camera"]["azimuth"]), 45.0, 1e-3);
    close(f(&iso["camera"]["elevation"]), 35.2644, 1e-3);
    let v =
        json(&["render", &g, "-o", &o, "--perspective", "--fov", "40", "--target", "0,0,0", "--size", "320x240"], 0);
    assert_eq!(v["camera"]["projection"], "perspective");
    assert_eq!(v["camera"]["fov_deg"], 40.0);
    assert_eq!(v["camera"]["target"], serde_json::json!([0.0, 0.0, 0.0]));
    // fit to a part: the part fills the image (larger scale than the whole model)
    let all = json(&["render", &data("collision.mcad"), "-o", &o, "--view", "front", "--size", "320x240"], 0);
    let one = json(
        &["render", &data("collision.mcad"), "-o", &o, "--view", "front", "--fit", "part:c", "--size", "320x240"],
        0,
    );
    assert!(f(&one["camera"]["scale_px_per_mm"]) > 2.0 * f(&all["camera"]["scale_px_per_mm"]));
    json(&["render", &g, "-o", &o, "--fit", "part:missing"], 2);
    json(&["render", &g, "-o", &o, "--target", "1,2"], 2);
    json(&["render", &g, "-o", &o, "--zoom", "0"], 2);
}

#[test]
fn render_selection() {
    let dir = tempfile::tempdir().unwrap();
    let o = dir.path().join("s.png").display().to_string();
    let k = data("collision.mcad");
    let v = json(&["render", &k, "-o", &o, "--part", "b", "--size", "320x240"], 0);
    let vis = v["visible_nodes"].as_array().unwrap();
    assert_eq!(vis.len(), 1);
    assert_eq!((vis[0]["instance"].as_str(), vis[0]["role"].as_str()), (Some("b"), Some("base")));
    assert_eq!(v["bounds"]["min"], serde_json::json!([8.0, 0.0, 0.0]));
    json(&["render", &k, "-o", &o, "--part", "x"], 2);
    // several lines: highlighted union, rest dimmed; --only drops the rest
    let v = json(&["render", &k, "-o", &o, "--line", "5", "--line", "7", "--size", "320x240"], 0);
    assert_eq!(v["lines"], serde_json::json!([5, 7]));
    let roles: Vec<&str> = v["visible_nodes"].as_array().unwrap().iter().map(|n| n["role"].as_str().unwrap()).collect();
    assert!(roles.contains(&"dimmed") && roles.iter().filter(|r| **r == "highlighted").count() >= 2, "{roles:?}");
    let v = json(&["render", &k, "-o", &o, "--line", "5", "--only", "--size", "320x240"], 0);
    assert!(v["visible_nodes"].as_array().unwrap().iter().all(|n| n["role"] != "dimmed"));
    // by node index, with -D in a module context
    let v = json(&["render", &data("housing.mcad"), "-o", &o, "--module", "housing", "-D", "w=80", "--node", "1"], 0);
    assert_eq!(v["highlighted"], serde_json::json!([1]));
    assert_eq!(v["bounds"]["max"][0], 80.0);
}

#[test]
fn render_overlays_change_pixels() {
    let dir = tempfile::tempdir().unwrap();
    let g = data("housing.mcad");
    let plain = dir.path().join("plain.png");
    json(&["render", &g, "-o", &plain.display().to_string(), "--module", "housing", "--size", "400x300"], 0);
    let p0 = png(&plain);
    for (show, min) in [("grids", 200), ("anchors", 50), ("handles", 10), ("edges", 50), ("bbox", 100), ("dims", 100)] {
        let out = dir.path().join(format!("{show}.png"));
        let v = json(
            &[
                "render",
                &g,
                "-o",
                &out.display().to_string(),
                "--module",
                "housing",
                "--size",
                "400x300",
                "--show",
                show,
            ],
            0,
        );
        assert_eq!(v["show"][0], show);
        let n = png(&out).differing(&p0);
        assert!(n >= min, "--show {show}: {n} pixels changed");
    }
    let out = dir.path().join("all.png");
    let v = json(
        &[
            "render",
            &g,
            "-o",
            &out.display().to_string(),
            "--module",
            "housing",
            "--size",
            "400x300",
            "--show",
            "grids,anchors,labels",
        ],
        0,
    );
    let ov = &v["overlays"];
    assert_eq!(
        (ov["grids"].as_u64(), ov["grid_points"].as_u64(), ov["anchors"].as_u64()),
        (Some(1), Some(16), Some(2))
    );
    assert!(ov["labels"].as_u64().unwrap() > 16);
    // labels add text to the grid overlay
    let grids = png(&dir.path().join("grids.png"));
    assert!(png(&out).differing(&grids) > 300);
    // no grids in the file context of this file? h is there; a file without grids gives a note
    let v = json(
        &["render", &data("collision.mcad"), "-o", &out.display().to_string(), "--show", "grids", "--size", "200x150"],
        0,
    );
    assert!(v["notes"][0].as_str().unwrap().contains("--module"));
    json(&["render", &g, "-o", &out.display().to_string(), "--show", "lattice"], 2);
}

#[test]
fn render_section_reduces_visible_area() {
    let dir = tempfile::tempdir().unwrap();
    let g = data("housing.mcad");
    let full = dir.path().join("full.png");
    json(&["render", &g, "-o", &full.display().to_string(), "--size", "400x300", "--no-axes"], 0);
    let cut = dir.path().join("section.png");
    let v = json(
        &["render", &g, "-o", &cut.display().to_string(), "--size", "400x300", "--no-axes", "--section", "z=10"],
        0,
    );
    assert_eq!(v["section"]["normal"], serde_json::json!([0.0, 0.0, 1.0]));
    assert_eq!(v["section"]["point"], serde_json::json!([0.0, 0.0, 10.0]));
    let (a, b) = (png(&full), png(&cut));
    assert!(b.drawn() < a.drawn() * 9 / 10, "{} vs {}", b.drawn(), a.drawn());
    // the cut walls are capped in the cap colour, the full model has none of it
    assert!(b.cap_pixels() > 200, "{}", b.cap_pixels());
    assert_eq!(a.cap_pixels(), 0);
    // flipped: keeps the upper half (normal −z), any plane
    let v = json(
        &["render", &g, "-o", &cut.display().to_string(), "--size", "400x300", "--section", "z=10", "--section-flip"],
        0,
    );
    assert_eq!(v["section"]["normal"], serde_json::json!([-0.0, -0.0, -1.0]));
    let v = json(
        &[
            "render",
            &g,
            "-o",
            &cut.display().to_string(),
            "--size",
            "400x300",
            "--section-plane",
            "1.5,0,0,1,0,0",
            "--perspective",
        ],
        0,
    );
    assert_eq!(v["section"]["normal"], serde_json::json!([1.0, 0.0, 0.0]));
    assert!(png(&cut).cap_pixels() > 100);
    json(&["render", &g, "-o", &cut.display().to_string(), "--section", "w=3"], 2);
    json(&["render", &g, "-o", &cut.display().to_string(), "--section", "z=1", "--section-plane", "0,0,0,0,0,1"], 2);
    json(&["render", &g, "-o", &cut.display().to_string(), "--section-flip"], 2);
}

#[test]
fn render_xray() {
    let dir = tempfile::tempdir().unwrap();
    let g = data("housing.mcad");
    let a = dir.path().join("a.png");
    let b = dir.path().join("b.png");
    json(&["render", &g, "-o", &a.display().to_string(), "--size", "400x300"], 0);
    let v = json(&["render", &g, "-o", &b.display().to_string(), "--size", "400x300", "--xray"], 0);
    assert_eq!(v["xray"], true);
    assert!(png(&a).differing(&png(&b)) > 2000);
}

#[test]
fn render_studio_style() {
    let dir = tempfile::tempdir().unwrap();
    let g = data("housing.mcad");
    let p = |n: &str| dir.path().join(n).display().to_string();
    let std_v = json(&["render", &g, "-o", &p("a.png"), "--size", "400x300"], 0);
    assert_eq!(std_v["style"], "standard");
    assert_eq!(std_v["studio"], Value::Null);
    assert_eq!(std_v["background"], serde_json::json!(["#f5f5f7"]));
    let v = json(&["render", &g, "-o", &p("b.png"), "--size", "400x300", "--style", "studio"], 0);
    assert_eq!(v["style"], "studio");
    // defaults
    let s = &v["studio"];
    assert_eq!((f(&s["light_azimuth"]), f(&s["light_elevation"])), (-40.0, 50.0));
    assert_eq!((f(&s["light_intensity"]), f(&s["ambient"])), (1.0, 0.4));
    assert_eq!((s["shadows"].as_bool(), f(&s["shadow_softness"])), (Some(true), 0.35));
    assert_eq!((s["ao"].as_bool(), &s["ao_radius"], f(&s["ao_strength"])), (Some(true), &Value::Null, 1.0));
    assert_eq!((s["ground"].as_bool(), s["ground_color"].as_str()), (Some(true), Some("#e6e8ec")));
    assert_eq!(v["background"], serde_json::json!(["#f8f9fb", "#d9dde3"]));
    assert_eq!(s["edges"], true);
    assert_eq!(s["environment"], "studio");
    assert_eq!(
        s["lights"],
        serde_json::json!([{"azimuth": -40.0, "elevation": 50.0, "intensity": 1.0, "color": "#ffffff"}])
    );
    assert_eq!(s["materials"], serde_json::json!([]));
    assert_eq!(s["render_block"], Value::Null);
    let vs = &v["views"][0]["studio"];
    assert!(f(&vs["ao_radius_mm"]) > 0.0);
    assert_eq!(vs["shadow_map"], 2048);
    assert!(vs["ground_z"].is_number());
    assert!(f(&vs["light_dir"][2]) > 0.7);
    let (a, b) = (png(&dir.path().join("a.png")), png(&dir.path().join("b.png")));
    assert!(a.differing(&b) > 20000, "{}", a.differing(&b));
    // shadows, AO and ground change the image; settings appear in the JSON
    let v = json(
        &[
            "render",
            &g,
            "-o",
            &p("c.png"),
            "--size",
            "400x300",
            "--style",
            "studio",
            "--shadows",
            "off",
            "--ao",
            "off",
            "--ground",
            "off",
            "--light",
            "30,70",
            "--light-intensity",
            "1.5",
            "--ambient",
            "0.2",
            "--ao-radius",
            "3",
            "--ao-strength",
            "2",
            "--shadow-softness",
            "0",
            "--ground-color",
            "#808080",
            "--background",
            "white,silver",
        ],
        0,
    );
    let s = &v["studio"];
    assert_eq!(
        (s["shadows"].as_bool(), s["ao"].as_bool(), s["ground"].as_bool()),
        (Some(false), Some(false), Some(false))
    );
    assert_eq!((f(&s["light_azimuth"]), f(&s["light_elevation"]), f(&s["ao_radius"])), (30.0, 70.0, 3.0));
    assert_eq!(s["ground_color"], "#808080");
    assert_eq!(v["background"], serde_json::json!(["#f4f3f0", "#abaeb2"]));
    assert_eq!(v["views"][0]["studio"]["shadow_map"], Value::Null);
    assert_eq!(v["views"][0]["studio"]["ao_radius_mm"], Value::Null);
    assert_eq!(v["views"][0]["studio"]["ground_z"], Value::Null);
    assert!(png(&dir.path().join("c.png")).differing(&b) > 2000);
    // edges off: only the edge lines go
    let v = json(&["render", &g, "-o", &p("n.png"), "--size", "400x300", "--style", "studio", "--edges", "off"], 0);
    assert_eq!(v["studio"]["edges"], false);
    let n = png(&dir.path().join("n.png"));
    let d = n.differing(&b);
    assert!(d > 300 && d < 20000, "{d}");
    // materials and environment
    let v = json(
        &[
            "render",
            &g,
            "-o",
            &p("m.png"),
            "--size",
            "400x300",
            "--style",
            "studio",
            "--material",
            "default=chrome",
            "--material",
            "part:k=plastic,roughness=0.3",
            "--environment",
            "overcast",
        ],
        0,
    );
    assert_eq!(v["studio"]["environment"], "overcast");
    let mats = v["studio"]["materials"].as_array().unwrap();
    assert_eq!(mats.len(), 2);
    assert_eq!(
        (mats[0]["target"].as_str(), f(&mats[0]["metalness"]), f(&mats[0]["roughness"])),
        (Some("default"), 1.0, 0.08)
    );
    assert_eq!(mats[1]["target"], "part:k");
    close(f(&mats[1]["roughness"]), 0.3, 1e-6);
    assert!(png(&dir.path().join("m.png")).differing(&b) > 2000);
    // contact sheet, dark theme, section, perspective together
    let v = json(
        &[
            "render",
            &g,
            "-o",
            &p("d.png"),
            "--views",
            "all",
            "--size",
            "800x600",
            "--style",
            "studio",
            "--theme",
            "dark",
            "--section",
            "z=10",
            "--perspective",
        ],
        0,
    );
    assert_eq!(v["views"].as_array().unwrap().len(), 4);
    assert_eq!(v["background"], serde_json::json!(["#2b3038", "#111317"]));
    // gradient background without studio
    let v = json(&["render", &g, "-o", &p("e.png"), "--size", "200x150", "--background", "#ffffff,#000000"], 0);
    assert_eq!(v["background"], serde_json::json!(["#ffffff", "#000000"]));
    let e = png(&dir.path().join("e.png"));
    assert_eq!(&e.rgba[0..3], &[255, 255, 255]);
    let last = ((e.h - 1) * e.w * 4) as usize;
    assert_eq!(&e.rgba[last..last + 3], &[0, 0, 0]);
    // text line
    let t = text(&["render", &g, "-o", &p("f.png"), "--size", "200x150", "--style", "studio"], 0);
    assert!(t.contains("Studio"), "{t}");
    // usage errors
    for bad in [
        &["--ao", "off"][..],
        &["--style", "fancy"],
        &["--style", "studio", "--shadows", "maybe"],
        &["--style", "studio", "--light", "10"],
        &["--style", "studio", "--light", "10,0"],
        &["--style", "studio", "--ambient", "1.5"],
        &["--style", "studio", "--ao-radius", "0"],
        &["--style", "studio", "--ground-color", "notacolour"],
        &["--style", "studio", "--edges", "maybe"],
        &["--edges", "off"],
        &["--material", "red=chrome"],
        &["--style", "studio", "--material", "red=gold"],
        &["--style", "studio", "--material", "nocolour=chrome"],
        &["--style", "studio", "--material", "red"],
        &["--style", "studio", "--material", "red=chrome,shine=1"],
        &["--style", "studio", "--material", "red=roughness=2"],
        &["--style", "studio", "--environment", "beach"],
        &["--background", "#12"],
        &["--background", "red,green,blue"],
    ] {
        let mut args = vec!["render", g.as_str(), "-o", "/dev/null"];
        args.extend_from_slice(bad);
        let v = json(&args, 2);
        assert!(v["error"].as_str().is_some_and(|e| !e.is_empty()), "{bad:?}: {v}");
    }
}

#[test]
fn render_block_of_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("torch.mcad");
    std::fs::write(
        &file,
        r##"color("silver") cylinder(h = 30, r = 8);
color("red") translate([0, 0, 30]) sphere(6);
lid: cube_lid();
module cube_lid() translate([20, 0, 0]) cube(8);
render {
    edges = false;
    ambient = 0.3;
    environment = "overcast";
    background = "#ffffff";
    light key { azimuth = 20; elevation = 60; }
    light rim { azimuth = 160; intensity = 0.5; color = "#ffe0c0"; }
    material "silver" { preset = "chrome"; }
    material lid { preset = "rubber"; }
}
"##,
    )
    .unwrap();
    let g = file.display().to_string();
    let p = |n: &str| dir.path().join(n).display().to_string();
    // the block makes the studio look the default and sets its values
    let v = json(&["render", &g, "-o", &p("a.png"), "--size", "320x240"], 0);
    assert_eq!(v["style"], "studio");
    let s = &v["studio"];
    assert_eq!(s["render_block"]["line"], 5);
    assert_eq!(
        (s["edges"].as_bool(), f(&s["ambient"]), s["environment"].as_str()),
        (Some(false), 0.3, Some("overcast"))
    );
    assert_eq!(s["lights"].as_array().unwrap().len(), 2);
    assert_eq!((f(&s["lights"][0]["azimuth"]), f(&s["lights"][0]["elevation"])), (20.0, 60.0));
    assert_eq!((s["lights"][1]["color"].as_str(), f(&s["lights"][1]["elevation"])), (Some("#ffe0c0"), 50.0));
    assert_eq!(v["background"], serde_json::json!(["#ffffff"]));
    let mats = s["materials"].as_array().unwrap();
    assert_eq!((mats[0]["target"].as_str(), f(&mats[0]["metalness"])), (Some("#abaeb2"), 1.0));
    assert_eq!((mats[1]["target"].as_str(), f(&mats[1]["roughness"])), (Some("part:lid"), 0.9));
    // options override the block (the first light, materials after the block's)
    let v = json(
        &[
            "render",
            &g,
            "-o",
            &p("b.png"),
            "--size",
            "320x240",
            "--light",
            "-60,30",
            "--edges",
            "on",
            "--material",
            "silver=plastic",
            "--environment",
            "studio",
        ],
        0,
    );
    let s = &v["studio"];
    assert_eq!((f(&s["lights"][0]["azimuth"]), f(&s["lights"][0]["elevation"])), (-60.0, 30.0));
    assert_eq!(s["lights"].as_array().unwrap().len(), 2);
    assert_eq!((s["edges"].as_bool(), s["environment"].as_str()), (Some(true), Some("studio")));
    assert_eq!(s["materials"].as_array().unwrap().last().unwrap()["metalness"], 0.0);
    assert!(png(&dir.path().join("a.png")).differing(&png(&dir.path().join("b.png"))) > 1000);
    // --style standard ignores the block; studio options then need --style studio
    let v = json(&["render", &g, "-o", &p("c.png"), "--size", "320x240", "--style", "standard"], 0);
    assert_eq!((v["style"].as_str(), &v["studio"]), (Some("standard"), &Value::Null));
    let v = json(&["render", &g, "-o", &p("d.png"), "--style", "standard", "--ao", "off"], 2);
    assert!(v["error"].as_str().unwrap().contains("--ao"));
    // `style = "standard"` in the block: standard unless --style studio
    std::fs::write(&file, "cube(5);\nrender { style = \"standard\"; edges = false; }\n").unwrap();
    assert_eq!(json(&["render", &g, "-o", &p("e.png"), "--size", "160x120"], 0)["style"], "standard");
    let v = json(&["render", &g, "-o", &p("e.png"), "--size", "160x120", "--style", "studio"], 0);
    assert_eq!(v["studio"]["edges"], false);
    // outline shows the block
    std::fs::write(
        &file,
        "cube(5);\nrender { edges = false; light key { azimuth = 10; } material \"red\" { preset = \"chrome\"; } }\n",
    )
    .unwrap();
    let o = json(&["outline", &g], 0);
    assert_eq!(o["render"]["edges"], false);
    assert_eq!(o["render"]["range"]["line"], 2);
    assert_eq!(o["render"]["lights"][0]["name"], "key");
    assert_eq!(o["render"]["materials"][0]["target"], serde_json::json!({"color": "red"}));
    let t = mcad().env("MCAD_LANG", "en").args(["outline", &g]).output().unwrap();
    let t = String::from_utf8_lossy(&t.stdout);
    assert!(t.contains("Render block (line 2): style studio, edges off"), "{t}");
    assert!(t.contains("material \"red\": chrome"), "{t}");
    // a bad value: warning with its line, the rest of the block applies
    std::fs::write(&file, "cube(5);\nrender {\n  ambient = 7;\n  edges = false;\n}\n").unwrap();
    let c = json(&["check", &g], 0);
    let d = &c["diagnostics"][0];
    assert_eq!((d["severity"].as_str(), d["range"]["line"].as_u64()), (Some("warning"), Some(3)));
    assert!(d["message"].as_str().unwrap().contains("ambient"), "{d}");
}

#[test]
fn set_render_values() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("m.mcad");
    let src = "// model\ncolor(\"red\") cube(5);\n";
    std::fs::write(&file, src).unwrap();
    let g = file.display().to_string();
    // a new block at the end; bare words of string settings are quoted
    let v = json(
        &[
            "set",
            &g,
            "--render",
            "edges=false",
            "--render",
            "light.key.azimuth=30",
            "--render",
            "material.red.preset=chrome",
            "--render",
            "environment=overcast",
        ],
        0,
    );
    assert_eq!(v["written"].as_array().map(|a| a.len()), Some(1));
    let t = std::fs::read_to_string(&file).unwrap();
    assert_eq!(
        t,
        "// model\ncolor(\"red\") cube(5);\n\nrender {\n    edges = false;\n    environment = \"overcast\";\n    light key { azimuth = 30; }\n    material \"red\" { preset = \"chrome\"; }\n}\n"
    );
    // in place, removal, part material; the render JSON follows
    json(
        &[
            "set",
            &g,
            "--render",
            "light.key.azimuth=-60",
            "--render",
            "edges=",
            "--render",
            "material.part:lid.roughness=0.4",
        ],
        0,
    );
    let t = std::fs::read_to_string(&file).unwrap();
    assert!(t.contains("light key { azimuth = -60; }") && !t.contains("edges"), "{t}");
    assert!(t.contains("material lid { roughness = 0.4; }"), "{t}");
    let v = json(&["render", &g, "-o", &dir.path().join("a.png").display().to_string(), "--size", "160x120"], 0);
    assert_eq!((v["style"].as_str(), f(&v["studio"]["lights"][0]["azimuth"])), (Some("studio"), -60.0));
    // dry run writes nothing; bad specs are usage errors
    let before = std::fs::read_to_string(&file).unwrap();
    let v = json(&["set", &g, "--render", "ambient=0.2", "--dry-run"], 0);
    assert!(v["diff"].as_str().unwrap().contains("+    ambient = 0.2;"), "{v}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    for bad in ["shine=1", "light.key.height=3", "nothing", "material.red=x=1"] {
        json(&["set", &g, "--render", bad], 2);
    }
}

// ------------------------------------------------------------------------------------------
// measure
// ------------------------------------------------------------------------------------------

#[test]
fn measure_points_bounds_volume() {
    let g = data("housing.mcad");
    let v = json(&["measure", &g, "--between", "k.corner", "k.top"], 0);
    let b = &v["between"];
    assert_eq!(b["a"]["point"], serde_json::json!([60.0, 30.0, 20.0]));
    assert_eq!(b["a"]["kind"], "anchor");
    assert_eq!(b["b"]["kind"], "anchor");
    assert_eq!(b["delta"], serde_json::json!([-60.0, -30.0, 0.0]));
    close(f(&b["distance"]), (60.0f64 * 60.0 + 30.0 * 30.0).sqrt(), 1e-9);
    // grid points in the module context, numbers and coordinates
    let v = json(&["measure", &g, "--module", "housing", "--between", "g[left, front, floor]", "g[right,back,lid]"], 0);
    close(f(&v["between"]["distance"]), (40.0f64 * 40.0 + 30.0 * 30.0 + 20.0 * 20.0).sqrt(), 1e-9);
    let v = json(&["measure", &g, "--module", "housing", "-D", "w=50", "--between", "g[right, 5, _]", "[0,0,0]"], 0);
    assert_eq!(v["between"]["a"]["point"], serde_json::json!([50.0, 5.0, 0.0]));
    assert_eq!(v["between"]["a"]["kind"], "grid_point");
    assert_eq!(v["between"]["b"]["kind"], "coordinate");
    let v = json(&["measure", &g, "--module", "housing", "--between", "corner", "top"], 0);
    close(f(&v["between"]["distance"]), 50.0, 1e-9);
    json(&["measure", &g, "--between", "g[left,front,floor]", "[0,0,0]"], 2); // g not in the file context
    json(&["measure", &g, "--module", "housing", "--between", "g[left,front,top]", "[0,0,0]"], 2);
    json(&["measure", &g, "--between", "k.nowhere", "[0,0,0]"], 2);
    json(&["measure", &g], 2);
    // bounds, volume, area
    let v = json(&["measure", &g, "--bounds", "part:k", "--volume", "part:k", "--area", "line:21"], 0);
    assert_eq!(v["bounds"]["bounds"]["max"], serde_json::json!([60.0, 30.0, 20.0]));
    assert_eq!(v["bounds"]["bounds"]["size"], serde_json::json!([60.0, 30.0, 20.0]));
    close(f(&v["volume"]["volume"]), 8460.0, 1e-6);
    assert!(f(&v["area"]["area"]) > 0.0);
    let v = json(&["measure", &g, "--bounds", "--volume"], 0);
    assert_eq!(v["bounds"]["target"], "all");
    assert_eq!(v["bounds"]["bounds"]["max"], serde_json::json!([70.0, 40.0, 22.0]));
    close(f(&v["volume"]["volume"]), 8460.0 + 2.0 * 200.0, 1e-6);
    let t = text(&["measure", &g, "--between", "k.corner", "k.top", "--volume", "part:k"], 0);
    assert!(t.contains("67.082 mm"), "{t}");
    assert!(t.contains("Volumen part:k: 8460 mm³"), "{t}");
}

#[test]
fn measure_interference_and_clearance() {
    let k = data("collision.mcad");
    let v = json(&["measure", &k, "--interference"], 0);
    let i = &v["interference"];
    assert_eq!(i["parts"], serde_json::json!(["a", "b", "c"]));
    let cols = i["collisions"].as_array().unwrap();
    assert_eq!(cols.len(), 1);
    assert_eq!((cols[0]["a"].as_str(), cols[0]["b"].as_str()), (Some("a"), Some("b")));
    close(f(&cols[0]["volume"]), 200.0, 1e-6);
    // a/c: bounds apart, b/c: bounds apart too (5 mm gap)
    assert_eq!(i["pairs"].as_array().unwrap().len(), 3);
    let t = text(&["measure", &k, "--interference"], 0);
    assert!(t.contains("Kollision a ↔ b: 200 mm³"), "{t}");
    // clearance: 5 mm between b and c; a and b collide
    let v = json(&["measure", &k, "--clearance", "b", "c"], 0);
    let c = &v["clearance"];
    close(f(&c["distance"]), 5.0, f(&c["accuracy"]));
    assert_eq!(c["collision"], false);
    let v = json(&["measure", &k, "--clearance", "a", "b"], 0);
    assert_eq!(v["clearance"]["collision"], true);
    assert_eq!(v["clearance"]["distance"], 0.0);
    // the project example has no collisions
    let v = json(
        &["measure", &example("project-housing/housing.mcad"), "--interference", "--clearance", "housing", "lid"],
        0,
    );
    assert!(v["interference"]["collisions"].as_array().unwrap().is_empty());
    close(f(&v["clearance"]["distance"]), 10.0, 0.05);
    json(&["measure", &k, "--clearance", "a", "z"], 2);
}

// ------------------------------------------------------------------------------------------
// watch
// ------------------------------------------------------------------------------------------

#[test]
fn watch_reports_a_second_run_after_a_change() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("w.scad");
    std::fs::write(&file, std::fs::read_to_string(data("housing.mcad")).unwrap()).unwrap();
    let img = dir.path().join("w.png");
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("mcad"))
        .env("MCAD_LANG", "de")
        .args(["watch", &file.display().to_string(), "--json", "--once-after-change", "--timeout", "60"])
        .args(["--render", &img.display().to_string(), "--size", "200x150"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let first: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(
        (first["command"].as_str(), first["run"].as_u64(), first["trigger"].as_str()),
        (Some("watch"), Some(1), Some("start"))
    );
    assert_eq!(first["ok"], true);
    close(f(&first["parts"][0]["volume"]), 8460.0, 1e-6);
    assert!(f(&first["timings"]["total_ms"]) > 0.0);
    assert_eq!(first["render"]["width"], 200);
    assert!(img.exists());
    // change the file (the watcher is set up right after the first line)
    std::thread::sleep(Duration::from_millis(300));
    let src = std::fs::read_to_string(&file).unwrap().replace("k: housing(w = 60);", "k: housing(w = 80);");
    std::fs::write(&file, src).unwrap();
    let t0 = Instant::now();
    let second: Value = serde_json::from_str(&lines.next().expect("second line").unwrap()).unwrap();
    assert!(t0.elapsed() < Duration::from_secs(30));
    assert_eq!((second["run"].as_u64(), second["trigger"].as_str()), (Some(2), Some("change")));
    assert_eq!(second["changed"], serde_json::json!(["w.scad"]));
    close(f(&second["parts"][0]["volume"]), 80.0 * 30.0 * 20.0 - 74.0 * 30.0 * 17.0, 1e-6);
    // the engine cache stays: the unchanged lid nodes come from the cache
    assert!(second["stats"]["cache_hits"].as_u64().unwrap() > first["stats"]["cache_hits"].as_u64().unwrap());
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0));
}

#[test]
fn watch_text_and_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("t.scad");
    std::fs::write(&file, "cube(5);\n").unwrap();
    let t = text(&["watch", &file.display().to_string(), "--timeout", "0.3"], 0);
    assert!(t.starts_with("[1] "), "{t}");
    assert!(t.contains("0 Fehler"), "{t}");
    assert!(t.contains("Volumen 125 mm³"), "{t}");
    json(&["watch", "-"], 2);
}

// ------------------------------------------------------------------------------------------
// set
// ------------------------------------------------------------------------------------------

#[test]
fn set_param_and_literal() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("g.scad");
    let orig = std::fs::read_to_string(data("housing.mcad")).unwrap();
    std::fs::write(&file, &orig).unwrap();
    let fs = file.display().to_string();
    // dry run: diff only
    let v = json(&["set", &fs, "--param", "housing.w=50", "--literal", "21:16=70", "--dry-run"], 0);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), orig);
    let diff = v["diff"].as_str().unwrap();
    assert!(diff.contains("+module housing(w = 50, d = 30"), "{diff}");
    assert!(diff.contains("+k: housing(w = 70);"), "{diff}");
    let ch = v["changes"].as_array().unwrap();
    assert_eq!(
        (ch[0]["kind"].as_str(), ch[0]["old"].as_str(), ch[0]["new"].as_str()),
        (Some("param"), Some("40"), Some("50"))
    );
    assert_eq!(
        (ch[1]["kind"].as_str(), ch[1]["line"].as_u64(), ch[1]["old"].as_str()),
        (Some("literal"), Some(21), Some("60"))
    );
    assert_eq!(v["written"], serde_json::json!([]));
    // written atomically, the result evaluates
    let v = json(&["set", &fs, "--param", "housing.w=50", "--literal", "21:16=70"], 0);
    assert_eq!(v["written"].as_array().unwrap().len(), 1);
    let now = std::fs::read_to_string(&file).unwrap();
    assert!(now.contains("module housing(w = 50,") && now.contains("k: housing(w = 70);"), "{now}");
    let e = json(&["eval", &fs], 0);
    assert_eq!(
        e["scene"]["anchors"].as_array().unwrap().iter().find(|a| a["text"] == "k.corner").unwrap()["point"][0],
        70.0
    );
    // literal of a grid plane inside the module (line 5, `left = 0`)
    let v = json(&["set", &fs, "--literal", "5:19=1"], 0);
    assert_eq!(v["changes"][0]["old"], "0");
    // not a literal: a reference to a parameter
    let t = text(&["set", &fs, "--literal", "5:35=4"], 2);
    assert!(t.contains("kein editierbares Literal"), "{t}");
    json(&["set", &fs, "--param", "housing.x=1"], 2);
    json(&["set", &fs, "--param", "nowhere.w=1"], 2);
    json(&["set", &fs], 2);
    // a change that breaks the model is refused (exit 1), --force writes it
    let before = std::fs::read_to_string(&file).unwrap();
    let v = json(&["set", &fs, "--param", "housing.t=foo"], 1);
    assert_eq!(v["refused"], true);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    json(&["set", &fs, "--param", "housing.t=foo", "--force"], 0);
    assert!(std::fs::read_to_string(&file).unwrap().contains("t = foo"));
    // no temporary files left behind
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn set_in_project_file() {
    let dir = tempfile::tempdir().unwrap();
    let src = PathBuf::from(example("project-housing"));
    for f in ["housing.mcad", "constants.mcad", "lib/parts.mcad"] {
        let to = dir.path().join(f);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(src.join(f), &to).unwrap();
    }
    let main = dir.path().join("housing.mcad").display().to_string();
    // the module lives in lib/parts.mcad (use): its default is changed there
    let v = json(&["set", &main, "--param", "screw_boss.h=12"], 0);
    assert!(v["changes"][0]["file"].as_str().unwrap().ends_with("lib/parts.mcad"));
    assert!(std::fs::read_to_string(dir.path().join("lib/parts.mcad")).unwrap().contains("screw_boss(h = 12)"));
    // literal in the included file by path
    let consts = std::fs::read_to_string(dir.path().join("constants.mcad")).unwrap();
    let line = consts.lines().position(|l| l.starts_with("spacing")).unwrap() + 1;
    let spec = format!("constants.mcad:{line}:11=15");
    let v = json(&["set", &main, "--literal", &spec], 0);
    assert_eq!(v["changes"][0]["old"], "10");
    let o = json(&["outline", &main], 0);
    let p = o["parts"].as_array().unwrap().iter().find(|p| p["name"] == "lid").unwrap().clone();
    assert_eq!(p["bounds"]["min"][1], 55.0);
}

// ------------------------------------------------------------------------------------------
// UI entry (`mcad`, `mcad FILE`), `mcad ui`, help
// ------------------------------------------------------------------------------------------

/// `mcad` without a graphical session (and without a running UI).
fn headless() -> Command {
    let mut c = mcad();
    let ipc = std::env::temp_dir().join(format!("cad-ipc-test-{}", std::process::id()));
    c.env_remove("DISPLAY").env_remove("WAYLAND_DISPLAY").env("MCAD_IPC_DIR", ipc);
    c
}

#[test]
fn ui_entry_without_display() {
    for args in [vec![], vec![data("housing.mcad")], vec![data("")]] {
        let out = headless().args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        let expect = if cfg!(feature = "gui") { "keine grafische Sitzung" } else { "UI nicht in diesem Build" };
        assert!(err.contains(expect), "{err}");
        assert!(
            err.contains("render") && err.contains("outline") && err.contains("watch") && err.contains("check"),
            "{err}"
        );
    }
    let out = headless().arg("does-not-exist.scad").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("gibt es nicht"));
    let out = mcad().arg("rendr").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unbekannter Befehl"));
    // --help (English) explains both ways and points to the manual
    let out = mcad().arg("--help").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let help = String::from_utf8_lossy(&out.stdout);
    for c in [
        "outline",
        "measure",
        "watch",
        "set",
        "render",
        "check",
        "usage",
        "desktop UI",
        "headless",
        "mcad ui status",
        "DISPLAY",
        "mcad usage",
    ] {
        assert!(help.contains(c), "{c}: {help}");
    }
}

#[test]
fn usage_manual() {
    let out = mcad().arg("usage").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.starts_with("# mcad") && text.contains("Topics") && text.contains("measure"), "{text}");
    let out = mcad().args(["usage", "meas"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("# mcad measure"));
    let out = mcad().args(["usage", "no-such-topic"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown topic"));
}

#[test]
fn ui_commands_without_running_ui() {
    let out = headless().args(["ui", "status"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("keine laufende Desktop-UI"));
    let out = headless().args(["ui", "status", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["exit_code"], 3);
    assert_eq!(v["ok"], false);
    let out = headless().args(["ui", "screenshot", "-o", "/tmp/x.png"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    let out = headless().args(["ui", "show", &data("housing.mcad"), "--line", "3"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn ui_commands_talk_to_a_running_ui() {
    // A fake UI endpoint answers like the app: the CLI side of the control protocol.
    let dir = tempfile::tempdir().unwrap();
    let server = cad_ipc::Server::bind_path(&dir.path().join("ui-4242.sock")).unwrap();
    let file = data("housing.mcad");
    let root = std::path::Path::new(&file).parent().unwrap().to_path_buf();
    std::thread::spawn(move || {
        let _ = server.serve(move |f| match f.header.method.as_str() {
            "status" => Ok((
                serde_json::json!({ "pid": 4242, "endpoint": "ui-4242", "windows": [{
                    "project": { "root": root, "main": "housing.mcad", "files": ["housing.mcad"], "folder": false },
                    "dirty": ["housing.mcad"], "cursor": { "file": "housing.mcad", "line": 7, "col": 3 },
                    "selection": { "nodes": [{ "idx": 2, "label": "box", "line": 7 }] }, "context": "housing",
                    "params": { "w": 50 }, "diagnostics": [], "scene": { "version": 3, "complete": true, "nodes": 9 }, "section": null
                }]}),
                Vec::new(),
            )),
            "show" => Ok((serde_json::json!({ "shown": true, "line": f.header.body["line"] }), Vec::new())),
            "screenshot" => Ok((serde_json::json!({ "width": 2, "height": 1 }), vec![0x89, b'P', b'N', b'G'])),
            _ => Err("?".into()),
        });
    });
    let run = |args: &[&str]| mcad().env("MCAD_IPC_DIR", dir.path()).args(args).output().unwrap();
    let out = run(&["ui", "status"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(
        t.contains("Zeile 7") && t.contains("Modul housing") && t.contains("housing.mcad*") && t.contains("w = 50"),
        "{t}"
    );
    let out = run(&["ui", "status", "--json"]);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["uis"][0]["windows"][0]["cursor"]["line"], 7);
    let out = run(&["ui", "show", &file, "--line", "12", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["line"], 12);
    let png = dir.path().join("s.png");
    let out = run(&["ui", "screenshot", "-o", png.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(std::fs::read(&png).unwrap(), vec![0x89, b'P', b'N', b'G']);
}
