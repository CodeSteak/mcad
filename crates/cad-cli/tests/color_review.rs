//! Regression tests of the independent review of the colour functions (Bericht 12,
//! `tests/color-review/README.md`, findings F1–F9): every case of
//! `tests/color-review/cases/` runs through `mcad measure --colors` (region volumes add up,
//! no kernel error), plus one test per finding.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use cad_eval::palette;
use serde_json::Value;

fn cases() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/color-review/cases").canonicalize().unwrap()
}

fn case(name: &str) -> String {
    cases().join(name).display().to_string()
}

/// `mcad <args> --json` (no daemon): JSON and the text of the same call.
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
    let v: Value = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
    let t = Command::cargo_bin("mcad")
        .unwrap()
        .env("MCAD_LANG", "de")
        .env("MCAD_NO_DAEMON", "1")
        .args(args)
        .output()
        .unwrap();
    (v, String::from_utf8_lossy(&t.stdout).into_owned() + &String::from_utf8_lossy(&t.stderr))
}

/// The colour rows of `outline`/`export` (`colors`) or `measure --colors` (`colors.colors`).
fn rows(v: &Value) -> &Value {
    if v["colors"].is_object() { &v["colors"]["colors"] } else { &v["colors"] }
}

/// Region sets (`color_regions`, `measure`: `colors.parts`).
fn sets(v: &Value) -> Vec<Value> {
    let s = if v["colors"].is_object() { &v["colors"]["parts"] } else { &v["color_regions"] };
    s.as_array().cloned().unwrap_or_default()
}

/// Colour rows: label → (key, slot, explicit, volume).
fn colors(v: &Value) -> Vec<(String, String, u64, bool, f64)> {
    rows(v)
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["label"].as_str().unwrap().to_string(),
                c["key"].as_str().unwrap().to_string(),
                c["filament"].as_u64().unwrap(),
                c["explicit"].as_bool().unwrap(),
                c["volume"].as_f64().unwrap_or(-1.0),
            )
        })
        .collect()
}

fn messages(v: &Value) -> Vec<(String, u64, String)> {
    v["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["severity"].as_str().unwrap().to_string(),
                d["range"]["line"].as_u64().unwrap_or(0),
                d["message"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// Decoded RGBA image.
struct Img {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

fn png(p: &Path) -> Img {
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(p).unwrap()));
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    Img { w: info.width as usize, h: info.height as usize, px: buf }
}

impl Img {
    /// Pixels within `tol` of a colour (any shading of a face lies between dark and light,
    /// so the test colours are the lit top faces).
    fn count(&self, rgb: [u8; 3], tol: i32) -> usize {
        (0..self.w * self.h)
            .filter(|i| (0..3).all(|k| (self.px[i * 4 + k] as i32 - rgb[k] as i32).abs() <= tol))
            .count()
    }

    /// Pixels whose hue is near the colour's (saturated pixels only): robust against shading.
    fn hue_count(&self, rgb: [u8; 3]) -> usize {
        let hue = |c: [u8; 3]| {
            let [_, ch, h] = palette::oklch(c);
            (ch, h)
        };
        let (_, h0) = hue(rgb);
        (0..self.w * self.h)
            .filter(|i| {
                let (c, h) = hue([self.px[i * 4], self.px[i * 4 + 1], self.px[i * 4 + 2]]);
                let d = (h - h0).abs();
                c > 0.06 && d.min(360.0 - d) < 12.0
            })
            .count()
    }
}

fn pal(name: &str) -> [u8; 3] {
    palette::palette_color(name).unwrap().rgb
}

/// STEP colours: colour (rgb bytes) → number of styled faces.
fn step_face_colors(step: &str) -> BTreeMap<[u8; 3], usize> {
    let mut ents: BTreeMap<u64, &str> = BTreeMap::new();
    for line in step.lines() {
        if let Some(rest) = line.strip_prefix('#')
            && let Some((id, body)) = rest.split_once('=')
        {
            ents.insert(id.parse().unwrap(), body);
        }
    }
    let refs = |s: &str| -> Vec<u64> {
        s.split('#').skip(1).filter_map(|x| x.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()).collect()
    };
    fn find(ents: &BTreeMap<u64, &str>, id: u64, refs: &dyn Fn(&str) -> Vec<u64>, d: u32) -> Option<[u8; 3]> {
        let e = ents.get(&id)?;
        if let Some(v) = e.strip_prefix("COLOUR_RGB('',") {
            let n: Vec<f64> = v.trim_end_matches(");").split(',').map(|x| x.parse().unwrap()).collect();
            return Some([0, 1, 2].map(|i| (n[i] * 255.0).round() as u8));
        }
        if d > 8 {
            return None;
        }
        refs(e).into_iter().find_map(|r| find(ents, r, refs, d + 1))
    }
    let mut out = BTreeMap::new();
    for (_, e) in ents.iter().filter(|(_, e)| e.starts_with("STYLED_ITEM('color'")) {
        let psa = refs(e)[0];
        if let Some(c) = find(&ents, psa, &refs, 0) {
            *out.entry(c).or_insert(0) += 1;
        }
    }
    out
}

/// All cases of the review: evaluation without kernel errors, region volumes = part volume
/// (no "Farbregionen ungenau"), every random colour away from the palette.
#[test]
fn all_review_cases_evaluate_and_regions_add_up() {
    let mut files: Vec<PathBuf> = Vec::new();
    for e in std::fs::read_dir(cases()).unwrap().flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "scad") {
            files.push(p);
        }
    }
    for name in ["main_inc.scad", "main_use.scad"] {
        files.push(cases().join("inc").join(name));
    }
    files.sort();
    assert!(files.len() >= 38, "{}", files.len());
    for f in &files {
        // xf.scad names a builtin call on purpose (a language error of the review)
        let lang_error = f.ends_with("xf.scad");
        let f = f.display().to_string();
        let (v, t) = run(&["measure", &f, "--colors"], if lang_error { 1 } else { 0 });
        for (sev, line, msg) in messages(&v) {
            assert!(sev != "error" || (lang_error && msg.contains("benennbar")), "{f}:{line}: {msg}");
            assert!(!msg.contains("Farbregionen"), "{f}:{line}: {msg}");
        }
        assert!(!t.contains("Farbregionen ungenau"), "{f}: {t}");
        for set in sets(&v) {
            let sum: f64 = set["regions"].as_array().unwrap().iter().map(|r| r["volume"].as_f64().unwrap()).sum();
            let total = set["total"].as_f64().unwrap();
            assert!((sum - total).abs() <= 1e-6 * total.abs() + 1e-6, "{f}: {sum} ≠ {total}");
        }
        for (label, key, _, _, _) in colors(&v) {
            let b = u32::from_str_radix(&key[1..7], 16).unwrap();
            let rgb = [(b >> 16) as u8, (b >> 8) as u8, b as u8];
            if rows(&v).as_array().unwrap().iter().any(|c| c["label"] == label.as_str() && c["source"] == "random") {
                for p in palette::PALETTE.iter() {
                    assert!(
                        palette::delta_e(rgb, p.rgb) >= palette::RANDOM_MIN_PALETTE - 1e-9,
                        "{f}: {label} ≈ {}",
                        p.name
                    );
                }
            }
        }
    }
}

/// F1: equal geometry in different colours (shared cached node, one face tag) – the view and
/// STEP showed the first colour. The 3MF was right.
#[test]
fn f1_equal_geometry_keeps_its_colour_in_view_and_step() {
    let dir = tempfile::tempdir().unwrap();
    let img = dir.path().join("pal2.png");
    let (v, _) = run(&["render", &case("pal2.scad"), "-o", &img.display().to_string(), "--size", "480x320"], 0);
    let roles: Vec<&str> = v["visible_nodes"].as_array().unwrap().iter().map(|n| n["role"].as_str().unwrap()).collect();
    assert_eq!(
        roles.iter().filter(|r| **r == "region").count(),
        3,
        "default view of a coloured model: regions {roles:?}"
    );
    let im = png(&img);
    for name in ["red", "blue", "green"] {
        assert!(im.hue_count(pal(name)) > 800, "{name}: {}", im.hue_count(pal(name)));
    }
    // colours per face (`--show faces`): the instance's own colour as well
    let (_, _) = run(
        &["render", &case("pal2.scad"), "-o", &img.display().to_string(), "--size", "480x320", "--show", "faces"],
        0,
    );
    let im = png(&img);
    for name in ["red", "blue", "green"] {
        assert!(im.hue_count(pal(name)) > 800, "faces {name}: {}", im.hue_count(pal(name)));
    }
    // a loop over the palette (16 equal tiles): every tile its colour
    let (_, _) = run(
        &["render", &case("palette.scad"), "-o", &img.display().to_string(), "--size", "640x480", "--show", "faces"],
        0,
    );
    let im = png(&img);
    for name in ["red", "teal", "blue", "purple", "magenta", "orange"] {
        assert!(im.hue_count(pal(name)) > 300, "palette {name}: {}", im.hue_count(pal(name)));
    }
    // STEP: six faces per cube in its colour
    let step = dir.path().join("pal2.step");
    run(&["export", &case("pal2.scad"), "-o", &step.display().to_string()], 0);
    let fc = step_face_colors(&std::fs::read_to_string(&step).unwrap());
    assert_eq!(fc, BTreeMap::from([(pal("red"), 6), (pal("blue"), 6), (pal("green"), 6)]), "{fc:?}");
}

/// F3: the view matches the print – overlap (later wins), fillet between colours (rest
/// colour, not the default grey), colour in a 2D profile below an extrusion.
#[test]
fn f3_view_matches_the_print() {
    let dir = tempfile::tempdir().unwrap();
    let img = dir.path().join("x.png");
    for f in ["r_overlap2.scad", "r_overlap3.scad"] {
        let (v, _) = run(&["render", &case(f), "-o", &img.display().to_string(), "--size", "480x320"], 0);
        let n = v["visible_nodes"].as_array().unwrap().iter().filter(|n| n["role"] == "region").count();
        assert_eq!(n, 2, "{f}");
        let im = png(&img);
        // blue owns the overlap: more blue than red is visible (before: the merged top face was red)
        assert!(im.hue_count(pal("blue")) > im.hue_count(pal("red")), "{f}");
        assert!(im.hue_count(pal("red")) > 500, "{f}");
    }
    // fillet between red and blue: no grey (default) pixels, the fillet prints red
    let (_, _) = run(&["render", &case("r_fillet2.scad"), "-o", &img.display().to_string(), "--size", "480x320"], 0);
    let im = png(&img);
    let grey = im.count([0xb4, 0xba, 0xc4], 10);
    assert!(grey < 50, "grey fillet pixels: {grey}");
    let step = dir.path().join("f.step");
    run(&["export", &case("r_fillet2.scad"), "-o", &step.display().to_string()], 0);
    let fc = step_face_colors(&std::fs::read_to_string(&step).unwrap());
    assert_eq!(fc.values().sum::<usize>(), 9, "every face coloured {fc:?}");
    assert!(fc[&pal("red")] >= 6 && fc[&pal("blue")] >= 2, "{fc:?}");
    // 2D colour below an extrusion: uniform → the body's colour; mixed → warning, default
    let (v, t) = run(&["measure", &case("r_extrude.scad"), "--colors"], 0);
    let c = colors(&v);
    let get = |l: &str| c.iter().find(|x| x.0 == l).unwrap().4;
    assert!((get("red") - 500.0).abs() < 1e-6 && (get("blue") - 500.0).abs() < 1e-6, "{c:?}");
    // label `default` is fixed in JSON; the German text output says "Standard"
    assert!((get("default") - 250.0).abs() < 1e-6, "{c:?}");
    assert!(rows(&v).as_array().unwrap().iter().any(|r| r["label"] == "default" && r["source"] == "default"), "{v}");
    assert!(t.contains("F1  Standard "), "{t}");
    let m = messages(&v);
    assert!(m.iter().any(|(s, l, t)| s == "warning" && *l == 3 && t.contains("2D-Profil")), "{m:?}");
}

/// F4: material outside parts – the colour list has its volume, the export an object of its
/// own, and the message names the objects.
#[test]
fn f4_material_outside_parts() {
    let dir = tempfile::tempdir().unwrap();
    let mf = dir.path().join("m.3mf");
    let f = cases().join("inc/main_use.scad").display().to_string();
    let (v, t) = run(&["export", &f, "-o", &mf.display().to_string()], 0);
    let s = String::from_utf8_lossy(&std::fs::read(&mf).unwrap()).into_owned();
    assert_eq!(s.matches("<item ").count(), 3);
    // the object name is fixed (`no part` in every language), the text output translates it
    assert!(s.contains("name=\"no part – yellow\""), "{s}");
    assert!(t.contains("3 Objekte (teil, deko, no part)"), "{t}");
    assert!(!t.contains("je Bauteil ein Objekt"), "{t}");
    let yellow = colors(&v).into_iter().find(|c| c.0 == "yellow").unwrap();
    assert!((yellow.4 - 125.0).abs() < 1e-6, "{yellow:?}");
    let notes: Vec<&str> = v["notes"].as_array().unwrap().iter().map(|n| n.as_str().unwrap()).collect();
    assert!(notes.iter().any(|n| n.contains("no part")), "{notes:?}");
    let parts = v["color_regions"].as_array().unwrap();
    assert!(parts.iter().any(|p| p["part"] == "no part"), "{parts:?}");
}

/// F5: unknown names collide no more (34 names in one model, distinct, away from the
/// palette); different names merge only for exactly equal values.
#[test]
fn f5_random_colours_keep_their_distance() {
    let (v, _) = run(&["outline", &case("randnames.scad")], 0);
    let c = colors(&v);
    let random: Vec<&(String, String, u64, bool, f64)> =
        c.iter().filter(|x| palette::palette_color(&x.0).is_none()).collect();
    // rot/Rot/ROT are one name (case does not matter): 36 statements, 34 names
    assert_eq!(random.len(), 34, "{c:?}");
    let rgb = |k: &str| {
        let b = u32::from_str_radix(&k[1..7], 16).unwrap();
        [(b >> 16) as u8, (b >> 8) as u8, b as u8]
    };
    for (i, a) in random.iter().enumerate() {
        for b in &random[i + 1..] {
            let d = palette::delta_e(rgb(&a.1), rgb(&b.1));
            assert!(d >= palette::RANDOM_MIN_OTHER - 1e-9, "{} / {}: {d}", a.0, b.0);
        }
    }
    let slots: std::collections::HashSet<u64> = c.iter().map(|x| x.2).collect();
    assert_eq!(slots.len(), c.len(), "no two names share a slot by chance");
}

/// F6: touching neighbours of the same colour leave no slivers (volume exact).
#[test]
fn f6_no_slivers_between_same_coloured_neighbours() {
    for (f, red) in [("r_touch.scad", 2000.0), ("r_touch3.scad", 2000.0), ("r_touch4.scad", 2000.0)] {
        let (v, _) = run(&["measure", &case(f), "--colors"], 0);
        let r = colors(&v).into_iter().find(|c| c.0 == "red").unwrap();
        assert!((r.4 - red).abs() < 1e-6, "{f}: red {}", r.4);
    }
}

/// F7: alpha, clamping, `transparent`, empty name.
#[test]
fn f7_value_rules() {
    let (v, _) = run(&["measure", &case("values.scad"), "--colors"], 0);
    let c = colors(&v);
    // alpha only changes the view: every key is an RGB value, alpha variants are one entry
    assert!(c.iter().all(|x| x.1.len() == 7), "{c:?}");
    assert_eq!(c.iter().filter(|x| x.0 == "red").count(), 1, "{c:?}");
    assert!(!c.iter().any(|x| x.0.contains("transparent")), "transparent is no filament: {c:?}");
    let m = messages(&v);
    let has = |line: u64, s: &str| m.iter().any(|(sev, l, t)| sev == "warning" && *l == line && t.contains(s));
    assert!(has(8, "zwischen 0 und 1") && has(8, "#ff0000"), "[255, 0, 0]: {m:?}");
    assert!(has(14, "alpha liegt zwischen 0 und 1"), "alpha = 2: {m:?}");
    assert!(has(15, "`transparent` ist keine Druckfarbe"), "{m:?}");
    assert!(has(19, "zwischen 0 und 1"), "negative: {m:?}");
    assert!(has(23, "color() ohne Farbe"), "{m:?}");
    let (v, _) = run(&["measure", &case("unknown.scad"), "--colors"], 0);
    let m = messages(&v);
    assert!(m.iter().any(|(_, l, t)| *l == 5 && t.contains("leerer Farbname")), "{m:?}");
    // " red ", "RED" and "red" are one palette colour
    let c = colors(&v);
    assert_eq!(c.iter().filter(|x| x.0 == "red").count(), 1, "{c:?}");
    // the filament conflict check sees through alpha (f1.scad line 15)
    let (v, _) = run(&["measure", &case("f1.scad"), "--colors"], 0);
    assert!(messages(&v).iter().any(|(_, l, t)| *l == 15 && t.contains("Farbe hat schon Filament 3")));
}

/// F8: slot hints and warnings; a single-coloured model has no colour group.
#[test]
fn f8_slot_remarks_and_single_colour() {
    assert_eq!(cad_engine::colors::MAX_SLOT, palette::MAX_FILAMENT);
    let (v, _) = run(&["measure", &case("f1.scad"), "--colors"], 0);
    let m = messages(&v);
    assert!(
        m.iter().any(|(s, l, t)| s == "info" && *l == 4 && t.contains("Slot 3 hat schon die Farbe `red`")),
        "{m:?}"
    );
    assert!(
        m.iter().any(|(s, l, t)| s == "info" && *l == 6 && t.contains("Slot 1 druckt auch das Material ohne color()"))
    );
    // more than 16 colours: the automatic slots beyond 16 warn
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("viele.scad");
    let mut src = String::new();
    for i in 0..18 {
        src.push_str(&format!("translate([{}, 0, 0]) color([{}, 0.5, 0.5]) cube(1);\n", i * 2, i as f64 / 20.0));
    }
    std::fs::write(&f, src).unwrap();
    let (v, _) = run(&["measure", &f.display().to_string(), "--colors"], 0);
    let w: Vec<(String, u64, String)> = messages(&v).into_iter().filter(|x| x.2.contains("mehr als 16")).collect();
    assert_eq!(w.len(), 2, "{w:?}");
    assert_eq!((w[0].1, w[1].1), (17, 18));
    // single colour (also with --color-group): no colour group
    for extra in [None, Some("--color-group")] {
        let mf = dir.path().join("p.3mf");
        let plain = case("plain.scad");
        let mut args = vec!["export", plain.as_str()];
        let o = mf.display().to_string();
        args.extend(["-o", &o]);
        args.extend(extra);
        run(&args, 0);
        let s = String::from_utf8_lossy(&std::fs::read(&mf).unwrap()).into_owned();
        assert!(!s.contains("colorgroup") && !s.contains("pid="), "{extra:?}");
        assert!(s.contains("Metadata/model_settings.config"));
    }
}

/// F9: `--per-color` needs `.zip`; `--part` limits the colour list; files of `use`d lines.
#[test]
fn f9_per_colour_export_and_list() {
    let dir = tempfile::tempdir().unwrap();
    let f = cases().join("inc/main_use.scad").display().to_string();
    let stl = dir.path().join("x.stl");
    let (_, t) = run(&["export", &f, "--per-color", "-o", &stl.display().to_string()], 2);
    assert!(t.contains("ZIP") && t.contains("x.zip"), "{t}");
    assert!(!stl.exists());
    let zip = dir.path().join("x.zip");
    let (v, t) = run(&["export", &f, "--per-color", "--part", "teil", "-o", &zip.display().to_string()], 0);
    let labels: Vec<String> = colors(&v).into_iter().map(|c| c.0).collect();
    assert_eq!(labels, ["white", "red", "teal", "rosarot"], "only the colours of `teil`");
    let teal = v["colors"].as_array().unwrap().iter().find(|c| c["label"] == "teal").unwrap().clone();
    assert!((teal["volume"].as_f64().unwrap() - 150.7964).abs() < 1e-3, "teal of `teil` only: {teal}");
    assert!(t.contains("lib.scad:1"), "file of the used statement: {t}");
    assert_eq!(teal["places"][0], "lib.scad:1");
}
