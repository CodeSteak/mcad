//! Colours in the CLI (Plan 16): `outline` and `measure --colors` list colours, slots and
//! volumes; `export` writes multi-colour 3MF by default, `--single-color`, `--per-color`;
//! `render --theme dark` and `--show regions`.

use std::path::PathBuf;

use assert_cmd::Command;
use serde_json::Value;

fn example() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/models/spec/mehrfarbig.mcad").display().to_string()
}

fn run(args: &[&str], code: i32) -> (Value, String) {
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
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let t = Command::cargo_bin("mcad")
        .unwrap()
        .env("MCAD_LANG", "de")
        .env("MCAD_NO_DAEMON", "1")
        .args(args)
        .output()
        .unwrap();
    (v, String::from_utf8_lossy(&t.stdout).into_owned() + &String::from_utf8_lossy(&t.stderr))
}

fn rows(v: &Value) -> Vec<(String, u64, f64)> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["label"].as_str().unwrap().to_string(),
                c["filament"].as_u64().unwrap(),
                c["volume"].as_f64().unwrap_or(-1.0),
            )
        })
        .collect()
}

#[test]
fn outline_lists_colours_slots_and_volumes() {
    let (v, t) = run(&["outline", &example()], 0);
    let r = rows(&v["colors"]);
    let labels: Vec<(&str, u64)> = r.iter().map(|(l, f, _)| (l.as_str(), *f)).collect();
    assert_eq!(labels, [("white", 1), ("teal", 2), ("red", 3), ("blue", 4), ("yellow", 5)]);
    assert_eq!(v["colors"][2]["explicit"], true);
    assert!(r.iter().all(|x| x.2 > 0.0), "{r:?}");
    // per part: the volumes add up to the part volume
    for set in v["color_regions"].as_array().unwrap() {
        let sum: f64 = set["regions"].as_array().unwrap().iter().map(|x| x["volume"].as_f64().unwrap()).sum();
        assert!((sum - set["total"].as_f64().unwrap()).abs() < 1e-6 * sum, "{set}");
        assert!(set["warning"].is_null());
    }
    assert_eq!(v["counts"]["colors"], 5);
    assert!(t.contains("Farben (Filament, Farbe, Volumen, Masse PLA 1,24 g/cm³):"), "{t}");
    assert!(t.contains("F3  red") && t.contains("filament = 3"), "{t}");
    assert!(t.contains("schild: F1 white"), "{t}");
    // without kernel: no volumes
    let (v, _) = run(&["outline", "--no-kernel", &example()], 0);
    assert!(v["colors"][0]["volume"].is_null());
    assert!(v["color_regions"].is_null());
}

#[test]
fn measure_colours() {
    let (v, t) = run(&["measure", &example(), "--colors"], 0);
    let c = &v["colors"];
    assert_eq!(c["density_g_cm3"], 1.24);
    let red = c["colors"].as_array().unwrap().iter().find(|x| x["label"] == "red").unwrap();
    let vol = red["volume"].as_f64().unwrap();
    assert!((vol - 208.8).abs() < 1e-6, "{vol}");
    assert!((red["mass_g"].as_f64().unwrap() - vol * 1.24e-3).abs() < 1e-9);
    let parts: Vec<&str> = c["parts"].as_array().unwrap().iter().map(|p| p["part"].as_str().unwrap()).collect();
    assert_eq!(parts, ["schild", "figur"]);
    assert!(t.contains("F4  blue"), "{t}");
}

#[test]
fn export_multicolour_single_and_per_colour() {
    let dir = tempfile::tempdir().unwrap();
    let mf = dir.path().join("m.3mf");
    let (v, t) = run(&["export", &example(), "-o", &mf.display().to_string()], 0);
    assert_eq!(v["multicolor"], true);
    assert_eq!(rows(&v["colors"]).len(), 5);
    assert!(t.contains("3MF mehrfarbig"), "{t}");
    let s = String::from_utf8_lossy(&std::fs::read(&mf).unwrap()).into_owned();
    // Bericht 12: one object per part also without --part, no colour group (Bambu dialog)
    assert_eq!(s.matches("<m:color ").count(), 0);
    assert_eq!(s.matches("<item ").count(), 2, "one object per part");
    assert!(t.contains("2 Objekte (schild, figur)") && t.contains("ohne Farbdialog"), "{t}");
    // --color-group: the colour group for other viewers, with the hint for Bambu Studio
    let (v, t) = run(&["export", &example(), "--color-group", "-o", &mf.display().to_string()], 0);
    assert_eq!(v["color_group"], true);
    assert!(t.contains("„Abbrechen“"), "{t}");
    let s = String::from_utf8_lossy(&std::fs::read(&mf).unwrap()).into_owned();
    assert_eq!(s.matches("<m:color ").count(), 5);
    // parts: one object per part, one volume per colour
    let (v, _) =
        run(&["export", &example(), "--part", "schild", "--part", "figur", "-o", &mf.display().to_string()], 0);
    assert_eq!(v["multicolor"], true);
    let s = String::from_utf8_lossy(&std::fs::read(&mf).unwrap()).into_owned();
    assert_eq!(s.matches("<item ").count(), 2);
    assert_eq!(s.matches("<component ").count(), 6);
    assert!(s.contains("<metadata key=\"extruder\" value=\"3\"/>"));
    // single colour
    let (v, _) = run(&["export", &example(), "--single-color", "-o", &mf.display().to_string()], 0);
    assert_eq!(v["multicolor"], false);
    assert!(!String::from_utf8_lossy(&std::fs::read(&mf).unwrap()).contains("colorgroup"));
    // one STL per colour and part
    let zip = dir.path().join("farben.zip");
    let (v, t) = run(
        &["export", &example(), "--per-color", "--part", "schild", "--part", "figur", "-o", &zip.display().to_string()],
        0,
    );
    assert_eq!(v["per_color"], true);
    assert_eq!(v["files"], 6);
    let s = String::from_utf8_lossy(&std::fs::read(&zip).unwrap()).into_owned();
    for f in ["schild_F1_white.stl", "schild_F3_red.stl", "figur_F4_blue.stl", "figur_F5_yellow.stl"] {
        assert!(s.contains(f), "{f}");
    }
    assert!(t.contains("6 STL (eine je Farbe und Bauteil)"), "{t}");
    // STL is never multi-coloured without --per-color
    let stl = dir.path().join("m.stl");
    let (v, _) = run(&["export", &example(), "-o", &stl.display().to_string()], 0);
    assert_eq!(v["multicolor"], false);
}

/// Mean RGB of the image corners (background).
fn corner(png_path: &std::path::Path) -> [f64; 3] {
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(png_path).unwrap()));
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    let px = |x: usize, y: usize| {
        let i = (y * info.width as usize + x) * 4;
        [buf[i] as f64, buf[i + 1] as f64, buf[i + 2] as f64]
    };
    let w = info.width as usize - 2;
    let a = px(2, 2);
    let b = px(w, 2);
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0]
}

#[test]
fn render_theme_and_regions() {
    let dir = tempfile::tempdir().unwrap();
    let light = dir.path().join("l.png");
    let dark = dir.path().join("d.png");
    run(&["render", &example(), "-o", &light.display().to_string(), "--size", "320x240"], 0);
    let (v, t) = run(
        &[
            "render",
            &example(),
            "-o",
            &dark.display().to_string(),
            "--size",
            "320x240",
            "--theme",
            "dark",
            "--show",
            "regions",
        ],
        0,
    );
    assert_eq!(v["theme"], "dark");
    assert!(t.contains("dunkel"), "{t}");
    let roles: Vec<&str> = v["visible_nodes"].as_array().unwrap().iter().map(|n| n["role"].as_str().unwrap()).collect();
    assert_eq!(roles.iter().filter(|r| **r == "region").count(), 6, "{roles:?}");
    let (l, d) = (corner(&light), corner(&dark));
    assert!(l.iter().all(|c| *c > 230.0), "light background {l:?}");
    assert!(d.iter().all(|c| *c < 40.0), "dark background {d:?}");
    run(&["render", &example(), "-o", &dark.display().to_string(), "--theme", "lila"], 2);
}
